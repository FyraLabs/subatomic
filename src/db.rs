use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

pub async fn create_pool(database_url: &str) -> Result<PgPool, sqlx::Error> {
    tracing::debug!(database_url, "connecting to db");
    PgPoolOptions::new().connect(database_url).await
}

#[derive(Clone, Debug, PartialEq, Eq, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct Key {
    pub id: String,
    pub userid: String,
    pub pri: String,
}

#[derive(Clone, Debug, PartialEq, Eq, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct Repo {
    pub id: i32,
    pub name: String,
    pub key_id: Option<String>,
}
