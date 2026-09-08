use std::path::PathBuf;

#[derive(Debug, serde::Deserialize, Clone)]
pub struct Config {
    #[serde(default = "defaults::localhost")]
    pub server_host: String,
    #[serde(default = "defaults::_3000")]
    pub server_port: u16,
    pub database_url: String,
    pub jwt_secret: String,
    #[serde(default = "defaults::subatomic_repos")]
    pub storage_dir: PathBuf,
    #[serde(default = "defaults::kiritanpo_nabe")]
    pub cache_dir: PathBuf,
    #[serde(default = "defaults::_1_073_741_824")]
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
        if self.body_limit <= 1_073_741_824 {
            tracing::warn!("uploads > {} B are not accepted", self.body_limit);
            tracing::warn!("we recommend setting $BODY_LIMIT to above 1 GiB");
        }
        assert!(!self.jwt_secret.is_empty(), "$JWT_SECRET is empty");
        assert!(!self.database_url.is_empty(), "$DATABASE_URL is empty");
    }
}

mod defaults {
    pub fn localhost() -> String {
        String::from("localhost")
    }
    /// Port used in subatomic v0.
    pub const fn _3000() -> u16 {
        3000
    }
    pub fn subatomic_repos() -> std::path::PathBuf {
        std::path::PathBuf::from("./subatomic-repos/")
    }
    /// きりたんぽ鍋🍲
    pub fn kiritanpo_nabe() -> std::path::PathBuf {
        std::path::PathBuf::from("./kiritanpo-nabe/")
    }
    /// To encourage users to change the `$BODY_LIMIT` depending on their usecase,
    /// the value is set to one that triggers the warning.
    pub const fn _1_073_741_824() -> usize {
        1_073_741_824
    }
}
