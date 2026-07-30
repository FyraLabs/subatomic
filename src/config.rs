use std::path::PathBuf;

#[derive(Debug, serde::Deserialize, Clone)]
pub struct Config {
    pub server_host: String,
    pub server_port: u16,
    pub database_url: String,
    pub jwt_secret: String,
    pub storage_dir: PathBuf,
    pub cache_dir: PathBuf,
    // NOTE: what is this for??
    pub subatomic_appstream_dir: String,
}

impl Config {
    pub fn from_env() -> envy::Result<Self> {
        dotenvy::dotenv().ok();
        envy::from_env::<Self>()
    }
}
