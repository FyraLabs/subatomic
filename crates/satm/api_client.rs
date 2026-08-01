use color_eyre::{Result, eyre::ContextCompat};
use reqwest::{Client, multipart};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::path::Path;
use tokio::fs::File;

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
    pub id: i32,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateKeyResp {
    pub id: i32,
    pub name: String,
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

    async fn send_json<T: DeserializeOwned>(&self, req: reqwest::RequestBuilder) -> Result<T> {
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

    async fn send_text(&self, req: reqwest::RequestBuilder) -> Result<String> {
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

    async fn send_empty(&self, req: reqwest::RequestBuilder) -> Result<()> {
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
        self.send_json(self.request_builder(reqwest::Method::GET, "/v1/repos")).await
    }

    pub async fn create_repo(&self, name: &str) -> Result<Repo> {
        self.send_json(self.request_builder(reqwest::Method::POST, &format!("/v1/repos/{name}")))
            .await
    }

    pub async fn upload_pkgs<P: AsRef<Path> + Send + Sync>(
        &self,
        name: &str,
        paths: &[P],
    ) -> Result<()> {
        let mut form = multipart::Form::new();
        for p in paths {
            let file_name = p.as_ref().file_name().context("invalid file name")?.to_string_lossy();
            form = form.file(file_name.to_string(), p).await?;
        }
        let req = self
            .request_builder(reqwest::Method::POST, &format!("/v1/repos/{name}"))
            .multipart(form);
        self.send_empty(req).await
    }

    pub async fn delete_repo(&self, name: &str) -> Result<()> {
        self.send_empty(self.request_builder(reqwest::Method::DELETE, &format!("/v1/repos/{name}")))
            .await
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
        self.send_empty(req).await
    }

    pub async fn delete_comps(&self, name: &str) -> Result<()> {
        self.send_empty(
            self.request_builder(reqwest::Method::DELETE, &format!("/v1/repos/{name}/comps")),
        )
        .await
    }

    pub async fn get_repo_key(&self, repo: &str) -> Result<String> {
        self.send_text(self.request_builder(reqwest::Method::GET, &format!("/v1/repos/{repo}/key")))
            .await
    }

    pub async fn set_repo_key(&self, repo: &str, key_id: i32) -> Result<()> {
        let body = serde_json::json!({ "id": key_id });
        let req = self
            .request_builder(reqwest::Method::PUT, &format!("/v1/repos/{repo}/key"))
            .json(&body);
        self.send_empty(req).await
    }

    pub async fn del_repo_key(&self, repo: &str) -> Result<()> {
        self.send_empty(
            self.request_builder(reqwest::Method::DELETE, &format!("/v1/repos/{repo}/key")),
        )
        .await
    }

    pub async fn list_rpms(&self, name: &str) -> Result<Vec<String>> {
        self.send_json(
            self.request_builder(reqwest::Method::GET, &format!("/v1/repos/{name}/rpms")),
        )
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
        let resp: DelRpmsResp = self.send_json(req).await?;
        Ok(resp.not_found)
    }

    pub async fn refresh_repo(&self, name: &str) -> Result<()> {
        self.send_empty(
            self.request_builder(reqwest::Method::POST, &format!("/v1/repos/{name}/refresh")),
        )
        .await
    }

    pub async fn list_keys(&self) -> Result<Vec<KeySummary>> {
        self.send_json(self.request_builder(reqwest::Method::GET, "/v1/keys")).await
    }

    pub async fn get_key(&self, id: i32) -> Result<String> {
        self.send_text(self.request_builder(reqwest::Method::GET, &format!("/v1/keys/{id}"))).await
    }

    pub async fn create_key(&self, name: &str, userid: &str) -> Result<CreateKeyResp> {
        let body = serde_json::json!({ "name": name, "userid": userid });
        let req = self.request_builder(reqwest::Method::POST, "/v1/keys").json(&body);
        self.send_json(req).await
    }

    pub async fn del_key(&self, id: i32) -> Result<()> {
        self.send_empty(self.request_builder(reqwest::Method::DELETE, &format!("/v1/keys/{id}")))
            .await
    }
}
