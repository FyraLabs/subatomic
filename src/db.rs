use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

pub async fn create_pool(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new().connect(database_url).await
}

#[derive(Clone, Debug, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct Key {
    pub id: i32,
    pub name: String,
    pub pri: Vec<u8>,
}

#[derive(Clone, Debug, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct Repo {
    pub id: i32,
    pub name: String,
    pub key_id: Option<i32>,
}
