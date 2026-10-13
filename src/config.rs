use std::{path::PathBuf, sync::Arc};

use figment::providers::Format;

#[derive(Debug, serde::Deserialize)]
pub struct Config {
    /// Server host. This is usually `localhost`. To accept all connections, use `0.0.0.0`.
    #[serde(default = "defaults::localhost")]
    pub server_host: String,
    /// Server port.
    #[serde(default = "defaults::_3000")]
    pub server_port: u16,
    /// Database URL.
    ///
    /// A PostgreSQL database is used to store signing keys and repository information. All tables
    /// are listed in [`crate::db`].
    ///
    /// Note that package/repository metadata are stored in [`Self::cache_dir`].
    pub database_url: String,
    /// Maximum number of concurrent connections to the relational database.
    ///
    /// From [`sqlx::postgres::PgPoolOptions::max_connections`]:
    /// > Set the maximum number of connections that this pool should maintain.
    /// >
    /// > Be mindful of the connection limits for your database as well as other applications
    /// > which may want to connect to the same database (or even multiple instances of the same
    /// > application in high-availability deployments).
    ///
    /// A connection is active for the runtime of a query.<super>[citations needed]</super> Increase
    /// this value if you have to make an enourmous number of requests in parallel.
    #[serde(default = "defaults::_32")]
    pub db_max_conns: u32,
    /// JWT secret. Generate this by `openssl rand -hex 32`.
    pub jwt_secret: String,
    /// Directory of the cache. Subatomic requires a local cache for temporarily storing uploaded
    /// RPMs for metadata parsing, and **persistent LMDBs storing package/repository metadata**.
    ///
    /// This directory SHOULD be persistent across runs, otherwise you will get empty `repodata`.
    #[serde(default = "defaults::kiritanpo_nabe")]
    pub cache_dir: PathBuf,
    /// Maximum body length in bytes allowed by the REST API, including file uploads. Increase this
    /// to allow larger package uploads.
    #[serde(default = "defaults::_1_073_741_824")]
    pub body_limit: usize,
    /// Type of the package storage (store). Store is where your RPMs will be stored.
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default, skip)]
    pub store: kuchiyose::store::StoreBackend,
    /// File hierarchy of the repositories. This is where you specify the path to the repositories.
    #[serde(default = "defaults::satm0")]
    pub hierarchy: libsubatomic::repo::hierarchy::Hierarchy,
    #[serde(default)]
    pub compression: kuchiyose::comp::CompConfig,
}

impl Config {
    /// Populate a new [`Config`] from environment variables.
    ///
    /// # Errors
    /// Missing environment variables cause an error.
    pub fn new() -> figment::Result<Self> {
        if let Err(err) = dotenvy::dotenv() {
            tracing::warn!(?err, "could not load `.env`");
        }
        let mut cfg: Self = figment::Figment::new()
            .merge(figment::providers::Toml::file("subatomic.toml"))
            .merge(figment::providers::Env::raw())
            .extract()?;
        cfg.store = cfg.storage.to_store_backend().expect("cannot obtain store");
        cfg.check();
        Ok(cfg)
    }

    /// Check if all configurations are valid.
    ///
    /// # Panics
    /// The program panics if bad configurations are found.
    pub fn check(&self) {
        if self.body_limit <= 1_073_741_824 {
            tracing::warn!("uploads > {} B are not accepted", self.body_limit);
            tracing::warn!("$BODY_LIMIT > 1 GiB is recommended for large repositories");
        }
        assert!(!self.jwt_secret.is_empty(), "$JWT_SECRET is empty");
        assert!(!self.database_url.is_empty(), "$DATABASE_URL is empty");
    }
}

#[derive(Debug, Default, serde::Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum StorageConfig {
    /// Store the package repository locally.
    #[default]
    Local,
    /// Store package repositories in e.g. an S3-compatible bucket.
    ///
    /// You must configure this via environment variables, documented at
    /// [`object_store::aws::AmazonS3Builder::from_env`].
    Aws,
}
impl StorageConfig {
    pub fn to_store_backend(self) -> object_store::Result<kuchiyose::store::StoreBackend> {
        Ok(match self {
            Self::Local => kuchiyose::store::StoreBackend::Local,
            Self::Aws => kuchiyose::store::StoreBackend::Remote(Arc::new(
                object_store::aws::AmazonS3Builder::from_env().build()?,
            )),
        })
    }
}

mod defaults {
    use libsubatomic::repo::hierarchy as hier;

    pub fn localhost() -> String {
        String::from("localhost")
    }
    /// Port used in subatomic v0.
    pub const fn _3000() -> u16 {
        3000
    }
    pub const fn _32() -> u32 {
        32
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
    pub fn satm0() -> hier::Hierarchy {
        hier::Hierarchy::Satm0Flat(hier::Satm0Flat {
            base: kuchiyose::LinkBuf::from(subatomic_repos()),
            ..
        })
    }
}
