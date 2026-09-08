use std::path::PathBuf;

#[derive(Debug, serde::Deserialize, Clone)]
pub struct Config {
    pub server_host: String,
    pub server_port: u16,
    pub database_url: String,
    pub jwt_secret: String,
    pub storage_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub body_limit: usize,
}

impl Config {
    /// Populate a new [`Config`] from environment variables.
    ///
    /// # Errors
    /// Missing environment variables cause an error.
    pub fn from_env() -> envy::Result<Self> {
        _ = dotenvy::dotenv();
        envy::from_env::<Self>()
    }

    /// Check if all configurations are valid.
    ///
    /// # Panics
    /// The program panics if bad configurations are found.
    pub fn check(&self) {
        if self.body_limit < 1_073_741_824 {
            tracing::warn!("uploads > {} B are not accepted", self.body_limit);
            tracing::warn!("we recommend setting $BODY_LIMIT to above 1 GiB");
        }
        assert!(!self.jwt_secret.is_empty(), "$JWT_SECRET is empty");
        assert!(!self.database_url.is_empty(), "$DATABASE_URL is empty");
    }
}
