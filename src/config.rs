use std::path::PathBuf;

#[derive(Debug, serde::Deserialize, Clone)]
pub struct Config {
    pub server_host: String,
    pub server_port: u16,
    pub database_url: String,
    pub jwt_secret: String,
    pub storage_dir: PathBuf,
    pub cache_dir: PathBuf,
}

impl Config {
    pub fn from_env() -> envy::Result<Self> {
        _ = dotenvy::dotenv();
        envy::from_env::<Self>()
    }
}
