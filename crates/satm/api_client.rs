use color_eyre::{Result, eyre::ContextCompat};
use reqwest::{Client, multipart};
use serde::{Deserialize, Serialize};
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

impl ApiClient {
    pub fn new(base_url: &str, token: String) -> Self {
        Self { client: Client::new(), base_url: base_url.trim_end_matches('/').to_owned(), token }
    }

    fn request_builder(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let url = format!("{}{path}", self.base_url);
        self.client.request(method, &url).header("Authorization", format!("Bearer {}", self.token))
    }

    pub async fn list_repos(&self) -> Result<Vec<Repo>> {
        let res = self
            .request_builder(reqwest::Method::GET, "/v1/repos")
            .send()
            .await?
            .error_for_status()?;
        Ok(res.json().await?)
    }

    pub async fn create_repo(&self, name: &str) -> Result<Repo> {
        let res = self
            .request_builder(reqwest::Method::POST, &format!("/v1/repos/{name}"))
            .send()
            .await?
            .error_for_status()?;
        Ok(res.json().await?)
    }
    pub async fn upload_packages<P: AsRef<Path> + Send + Sync>(
        &self,
        name: &str,
        paths: &[P],
    ) -> Result<()> {
        let mut form = multipart::Form::new();
        for p in paths {
            let file_name = p.as_ref().file_name().context("invalid file name")?.to_string_lossy();
            let file = File::open(p).await?;
            let part = multipart::Part::stream(file).file_name(file_name.to_string());
            form = form.part("files", part);
        }
        self.request_builder(reqwest::Method::PUT, &format!("/v1/repos/{name}"))
            .multipart(form)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    pub async fn delete_repo(&self, name: &str) -> Result<()> {
        self.request_builder(reqwest::Method::DELETE, &format!("/v1/repos/{name}"))
            .send()
            .await?
            .error_for_status()?;
        Ok(())
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

        self.request_builder(reqwest::Method::PUT, &format!("/v1/repos/{name}/comps"))
            .multipart(form)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    pub async fn delete_comps(&self, name: &str) -> Result<()> {
        self.request_builder(reqwest::Method::DELETE, &format!("/v1/repos/{name}/comps"))
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    pub async fn get_repo_key(&self, repo: &str) -> Result<String> {
        let res = self
            .request_builder(reqwest::Method::GET, &format!("/v1/repos/{repo}/key"))
            .send()
            .await?
            .error_for_status()?;
        Ok(res.text().await?)
    }

    pub async fn set_repo_key(&self, repo: &str, key_id: i32) -> Result<()> {
        let body = serde_json::json!({ "id": key_id });
        self.request_builder(reqwest::Method::PUT, &format!("/v1/repos/{repo}/key"))
            .json(&body)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    pub async fn del_repo_key(&self, repo: &str) -> Result<()> {
        self.request_builder(reqwest::Method::DELETE, &format!("/v1/repos/{repo}/key"))
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    pub async fn list_rpms(&self, name: &str) -> Result<Vec<String>> {
        let res = self
            .request_builder(reqwest::Method::GET, &format!("/v1/repos/{name}/rpms"))
            .send()
            .await?
            .error_for_status()?;
        Ok(res.json().await?)
    }

    pub async fn delete_rpms(&self, name: &str, rpms: &[String]) -> Result<Vec<String>> {
        let body = serde_json::json!({ "rpms": rpms });
        let res = self
            .request_builder(reqwest::Method::POST, &format!("/v1/repos/{name}/rpms"))
            .json(&body)
            .send()
            .await?
            .error_for_status()?;
        Ok(res.json().await?)
    }

    pub async fn refresh_repo(&self, name: &str) -> Result<()> {
        self.request_builder(reqwest::Method::POST, &format!("/v1/repos/{name}/refresh"))
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    pub async fn list_keys(&self) -> Result<Vec<KeySummary>> {
        let res = self
            .request_builder(reqwest::Method::GET, "/v1/keys")
            .send()
            .await?
            .error_for_status()?;
        Ok(res.json().await?)
    }

    pub async fn get_key(&self, id: i32) -> Result<String> {
        let res = self
            .request_builder(reqwest::Method::GET, &format!("/v1/keys/{id}"))
            .send()
            .await?
            .error_for_status()?;
        Ok(res.text().await?)
    }

    pub async fn create_key(&self, name: &str, userid: &str) -> Result<CreateKeyResp> {
        let body = serde_json::json!({ "name": name, "userid": userid });
        let res = self
            .request_builder(reqwest::Method::POST, "/v1/keys")
            .json(&body)
            .send()
            .await?
            .error_for_status()?;
        Ok(res.json().await?)
    }

    pub async fn del_key(&self, id: i32) -> Result<()> {
        self.request_builder(reqwest::Method::DELETE, &format!("/v1/keys/{id}"))
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }
}
