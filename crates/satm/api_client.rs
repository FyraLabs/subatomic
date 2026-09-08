use color_eyre::{Result, eyre::ContextCompat};
use libsubatomic::prelude::Itertools;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use reqwest::{Client, multipart};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::path::Path;
use tokio::{fs::File, io::AsyncReadExt};

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

#[derive(Debug, Deserialize)]
pub struct GetKeyResp {
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

    fn calculate_csum(paths: &[&Path]) -> Result<Vec<libsubatomic::smartstring::alias::String>> {
        tracing::info!("calculating checksums");
        Ok(paths
            .par_iter()
            .map(|path| libsubatomic::pkg::sha256_digest(std::fs::File::open(path)?))
            .collect::<std::io::Result<_>>()?)
    }

    async fn sign_header(
        &self,
        repo: &str,
        paths: &[&Path],
    ) -> Result<Vec<libsubatomic::smartstring::alias::String>> {
        tracing::info!("requesting signatures");
        let mut form = multipart::Form::new();
        for path in paths {
            let rpm = libsubatomic::rpm::PackageMetadata::open(path)?;
            let header = rpm.header_bytes().expect("cannot serialize rpmmeta");
            form = form.part("", multipart::Part::bytes(header));
        }
        let req = self
            .request_builder(reqwest::Method::POST, &format!("/v1/repos/{repo}/sign"))
            .multipart(form);
        let res = req.send().await?;
        let status = res.status();
        if status.is_success() {
            if status == reqwest::StatusCode::NO_CONTENT {
                return Ok(Self::calculate_csum(paths)?);
            }
            let content_type =
                res.headers().get(reqwest::header::CONTENT_TYPE).expect("can't get content-type");
            let bound =
                multer::parse_boundary(content_type.to_str().expect("can't parse content type"))
                    .expect("can't parse multer boundary");
            let mut multipart = multer::Multipart::new(res.bytes_stream(), bound);
            let mut res = vec![];
            tracing::info!("signing rpms & calculating checksums");
            for path in paths {
                let field = multipart
                    .next_field()
                    .await
                    .expect("can't get field")
                    .expect("expect next field");
                let sig = field.bytes().await.expect("can't get bytes").to_vec();
                let mut rpm = libsubatomic::rpm::Package::open(path).expect("cannot reopen rpm");
                rpm.apply_signature(sig)?;

                let fd = std::fs::File::create(&path)?;
                let fd = std::io::BufWriter::new(fd);
                let csum = libsubatomic::repodata::RepoWriterCsum::Sha256(Default::default());
                let mut w = libsubatomic::repodata::RepoWriterCompInner { fd, csum, size: 0 };
                rpm.write(&mut w)?;
                res.push(w.csum.csum());
            }
            Ok(res)
        } else {
            let text = res.text().await?;
            eprintln!("{status}\n{text}");
            Err(color_eyre::eyre::eyre!("server returned status: {status}"))
        }
    }

    pub async fn upload_pkgs<P: AsRef<Path> + Send + Sync>(
        &self,
        repo: &str,
        paths: &[P],
    ) -> Result<()> {
        let csums = self.sign_header(repo, &paths.iter().map(|p| p.as_ref()).collect_vec()).await?;

        let mut form = multipart::Form::new();
        for (path, csum) in paths.iter().zip_eq(csums) {
            let filename = path.as_ref().file_name().expect("expect filename").to_string_lossy();
            form = form.file(filename.to_string(), path).await?;
            form = form.text("csum", csum.to_string());
        }

        let req = self
            .request_builder(reqwest::Method::POST, &format!("/v1/repos/{repo}"))
            .multipart(form);
        self.void(req).await
    }

    pub async fn delete_repo(&self, name: &str) -> Result<()> {
        self.void(self.request_builder(reqwest::Method::DELETE, &format!("/v1/repos/{name}"))).await
    }

    // FIXME: use new endpoint & support more datatypes
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

    pub async fn refresh_repo(&self, name: &str) -> Result<()> {
        self.void(self.request_builder(reqwest::Method::POST, &format!("/v1/repos/{name}/refresh")))
            .await
    }

    pub async fn list_keys(&self) -> Result<Vec<KeySummary>> {
        self.json(self.request_builder(reqwest::Method::GET, "/v1/keys")).await
    }

    pub async fn get_key(&self, id: i32) -> Result<String> {
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

async fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let bytes_read = file.read(&mut buffer).await?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }
    Ok(hex::encode(hasher.finalize()))
}
