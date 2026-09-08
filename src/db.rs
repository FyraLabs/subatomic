use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

/// Open a connection to the database, and migrate automatically.
///
/// The database is not created automatically.
///
/// # Errors
/// Migration failure or `sqlx` connection errors are propagated.
pub async fn create_pool(cfg: &crate::Config) -> sqlx::Result<PgPool> {
    tracing::debug!(cfg.database_url, "connecting to db");
    let opts = PgPoolOptions::new().max_connections(cfg.db_max_conns);
    let pool = opts.connect(&cfg.database_url).await?;
    sqlx::migrate!().run(&pool).await?;
    Ok(pool)
}

/// Represent the `keys` table.
#[derive(Clone, Debug, PartialEq, Eq, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct Key {
    pub id: String,
    pub userid: String,
    pub pri: String,
}

/// Represent the `repos` table.
#[derive(Clone, Debug, PartialEq, Eq, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct Repo {
    pub id: i32,
    pub name: String,
    pub key_id: Option<String>,
}
