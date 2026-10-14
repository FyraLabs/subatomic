use bytes::Bytes;
use color_eyre::{Result, eyre::ContextCompat};
use reqwest::{Client, multipart};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::Digest;
use std::path::Path;
use std::sync::LazyLock;
use tokio::fs::File;
use tokio::io::AsyncReadExt;

#[derive(Clone)]
pub struct ApiClient {
    client: Client,
    base_url: String,
    token: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Repo {
    pub id: i32,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct KeySummary {
    pub id: String,
    pub userid: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateKeyResp {
    pub id: String,
    pub public_armor: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DelRpmsResp {
    not_found: Vec<String>,
}

impl ApiClient {
    pub fn new(base_url: &str, token: String) -> Self {
        Self { client: Client::new(), base_url: base_url.trim_end_matches('/').to_owned(), token }
    }

    fn request_builder(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let url = format!("{}{path}", self.base_url);
        self.client.request(method, &url).header("Authorization", format!("Bearer {}", self.token))
    }

    async fn json<T: DeserializeOwned>(&self, req: reqwest::RequestBuilder) -> Result<T> {
        let res = req.send().await?;
        let status = res.status();
        if status.is_success() {
            Ok(res.json().await?)
        } else {
            let text = res.text().await?;
            eprintln!("{status}\n{text}");
            Err(color_eyre::eyre::eyre!("server returned status: {status}"))
        }
    }

    async fn text(&self, req: reqwest::RequestBuilder) -> Result<String> {
        let res = req.send().await?;
        let status = res.status();
        if status.is_success() {
            Ok(res.text().await?)
        } else {
            let text = res.text().await?;
            eprintln!("{status}\n{text}");
            Err(color_eyre::eyre::eyre!("server returned status: {status}"))
        }
    }

    async fn void(&self, req: reqwest::RequestBuilder) -> Result<()> {
        let res = req.send().await?;
        let status = res.status();
        if status.is_success() {
            Ok(())
        } else {
            let text = res.text().await?;
            eprintln!("{status}\n{text}");
            Err(color_eyre::eyre::eyre!("server returned status: {status}"))
        }
    }

    pub async fn list_repos(&self) -> Result<Vec<Repo>> {
        self.json(self.request_builder(reqwest::Method::GET, "/v1/repos")).await
    }

    pub async fn create_repo(&self, name: &str) -> Result<Repo> {
        self.json(self.request_builder(reqwest::Method::PUT, &format!("/v1/repos/{name}"))).await
    }

    async fn sign_header<P: AsRef<Path> + Send + Sync>(
        &self,
        repo: &str,
        paths: &[P],
    ) -> Result<()> {
        tracing::info!("requesting signatures");
        let mut form = multipart::Form::new();
        for path in paths {
            let rpm = libsubatomic::rpm::PackageMetadata::open(path)?;
            let mut buf = vec![];
            rpm.write(&mut buf).unwrap();
            form = form.part("", multipart::Part::bytes(buf));
        }
        let req = self
            .request_builder(reqwest::Method::POST, &format!("/v1/repos/{repo}/sign"))
            .multipart(form);
        let res = req.send().await?;
        let status = res.status();
        if !status.is_success() {
            let text = res.text().await?;
            eprintln!("{status}\n{text}");
            return Err(color_eyre::eyre::eyre!("server returned status: {status}"));
        }
        if status == reqwest::StatusCode::NO_CONTENT {
            tracing::info!(?repo, "proceeding without signing for this repo");
            return Ok(());
        }

        let content_type = res
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .context("server response is missing the multipart Content-Type header")?;
        let content_type = content_type.to_str().map_err(|e| {
            color_eyre::eyre::eyre!("server returned an invalid Content-Type header: {e}")
        })?;
        let bound = multer::parse_boundary(content_type).map_err(|e| {
            color_eyre::eyre::eyre!("server returned an invalid multipart boundary: {e}")
        })?;
        let mut multipart = multer::Multipart::new(res.bytes_stream(), bound);

        tracing::info!("applying signatures");
        for path in paths {
            let field =
                multipart.next_field().await.expect("can't get field").expect("expect next field");
            let sig = field.bytes().await.expect("can't get bytes").to_vec();

            let mut rpm = libsubatomic::rpm::Package::open(path).expect("cannot reopen rpm");
            rpm.apply_signature(sig)?;

            let file = std::fs::File::create(path)?;
            let mut w = std::io::BufWriter::new(file);
            rpm.write(&mut w)?;
            std::io::Write::flush(&mut w)?;
        }
        Ok(())
    }

    pub async fn upload_pkgs<P: AsRef<Path> + Send + Sync>(
        &self,
        repo: &str,
        paths: &[P],
    ) -> Result<()> {
        self.sign_header(repo, &paths).await?;

        let mut form = multipart::Form::new();
        for (i, path) in paths.iter().enumerate() {
            let path = path.as_ref();
            let filename = path
                .file_name()
                .expect("expect filename")
                .to_str()
                .unwrap_or_else(|| panic!("bad utf-8: {:?}", path.display()));
            let (file_stream, checksum_stream) =
                hashing_progress_stream(path, i, paths.len()).await?;

            form = form.part(
                "",
                multipart::Part::stream(reqwest::Body::wrap_stream(file_stream))
                    .file_name(filename.to_owned()),
            );
            form = form
                .part("csum", multipart::Part::stream(reqwest::Body::wrap_stream(checksum_stream)));
        }

        tracing::info!(total = paths.len(), "uploading");
        let req = self
            .request_builder(reqwest::Method::POST, &format!("/v1/repos/{repo}"))
            .multipart(form);
        self.void(req).await
    }

    pub async fn delete_repo(&self, name: &str) -> Result<()> {
        self.void(self.request_builder(reqwest::Method::DELETE, &format!("/v1/repos/{name}"))).await
    }

    pub async fn upload_comps<P: AsRef<Path> + Send + Sync>(
        &self,
        name: &str,
        file: P,
    ) -> Result<()> {
        let file = file.as_ref();
        let file_name = file.file_name().context("invalid file name")?.to_string_lossy();
        let file = File::open(file).await?;
        let part = multipart::Part::stream(file).file_name(file_name.to_string());
        let form = multipart::Form::new().part("comps", part);

        let req = self
            .request_builder(reqwest::Method::PUT, &format!("/v1/repos/{name}/comps"))
            .multipart(form);
        self.void(req).await
    }

    pub async fn delete_comps(&self, name: &str) -> Result<()> {
        self.void(self.request_builder(reqwest::Method::DELETE, &format!("/v1/repos/{name}/comps")))
            .await
    }

    pub async fn get_repo_key(&self, repo: &str) -> Result<String> {
        self.text(self.request_builder(reqwest::Method::GET, &format!("/v1/repos/{repo}/key")))
            .await
    }

    pub async fn set_repo_key(&self, repo: &str, key_id: &str) -> Result<()> {
        let body = serde_json::json!({ "id": key_id });
        let req = self
            .request_builder(reqwest::Method::PUT, &format!("/v1/repos/{repo}/key"))
            .json(&body);
        self.void(req).await
    }

    pub async fn del_repo_key(&self, repo: &str) -> Result<()> {
        self.void(self.request_builder(reqwest::Method::DELETE, &format!("/v1/repos/{repo}/key")))
            .await
    }

    pub async fn list_rpms(&self, name: &str) -> Result<Vec<String>> {
        self.json(self.request_builder(reqwest::Method::GET, &format!("/v1/repos/{name}/rpms")))
            .await
    }

    pub async fn delete_rpms(&self, name: &str, rpms: &[String]) -> Result<Vec<String>> {
        if rpms.is_empty() {
            return Err(color_eyre::eyre::eyre!("you're deleting nothing smh"));
        }
        let body = serde_json::json!({ "rpms": rpms });
        let req = self
            .request_builder(reqwest::Method::POST, &format!("/v1/repos/{name}/rpms"))
            .json(&body);
        let resp: DelRpmsResp = self.json(req).await?;
        Ok(resp.not_found)
    }

    //pub async fn refresh_repo(&self, name: &str) -> Result<()> {
    //    self.void(self.request_builder(reqwest::Method::POST, &format!("/v1/repos/{name}/refresh")))
    //        .await
    //}

    pub async fn list_keys(&self) -> Result<Vec<KeySummary>> {
        self.json(self.request_builder(reqwest::Method::GET, "/v1/keys")).await
    }

    pub async fn get_key(&self, id: &str) -> Result<String> {
        self.text(self.request_builder(reqwest::Method::GET, &format!("/v1/keys/{id}"))).await
    }

    pub async fn create_key(&self, id: &str, userid: &str) -> Result<CreateKeyResp> {
        let body = serde_json::json!({ "id": id, "userid": userid });
        let req = self.request_builder(reqwest::Method::POST, "/v1/keys").json(&body);
        self.json(req).await
    }

    pub async fn del_key(&self, id: &str) -> Result<()> {
        self.void(self.request_builder(reqwest::Method::DELETE, &format!("/v1/keys/{id}"))).await
    }
}

async fn hashing_progress_stream(
    path: &Path,
    n: usize,
    all: usize,
) -> Result<(
    impl futures::Stream<Item = std::io::Result<Bytes>> + Send + 'static,
    impl futures::Stream<Item = std::io::Result<Bytes>> + Send + 'static,
)> {
    let file = File::open(path).await?;
    let total = file.metadata().await?.len();
    let filename = path.file_name().unwrap().to_str().unwrap();

    let prec = (all.ilog10() + 1) as usize;
    let msg = format!("[{:>prec$}/{all}] {filename}", n + 1);

    let pb = LazyLock::new(move || {
        let pb = indicatif::ProgressBar::new(total);
        pb.set_style(
            indicatif::ProgressStyle::with_template(
                "{msg} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, ETA {eta})",
            )
            .expect("static template is valid")
            .progress_chars("=> "),
        );
        pb.set_message(msg);
        pb
    });
    let (tx, rx) = tokio::sync::oneshot::channel::<String>();

    let file_stream = {
        let mut reader = tokio::io::BufReader::new(file);
        let mut hasher = sha2::Sha256::new();
        let mut buf = vec![0u8; 64 * 1024];
        async_stream::try_stream! {
            loop {
                let n = reader.read(&mut buf).await?;
                if n == 0 {
                    break;
                }
                hasher.update(&buf[..n]);
                pb.inc(n as u64);
                yield Bytes::copy_from_slice(&buf[..n]);
            }
            pb.finish();
            _ = tx.send(hex::encode(hasher.finalize()));
        }
    };

    let checksum_stream = async_stream::try_stream! {
        let digest = rx
            .await
            .map_err(|_| std::io::Error::other("hashing stream terminated early"))?;
        tracing::debug!(digest);
        yield Bytes::from(digest);
    };

    Ok((file_stream, checksum_stream))
}
