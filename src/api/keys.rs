use crate::DbState;
use crate::db::Key;
use crate::error::{ApiError, Result};
use axum::Json;
use axum::extract::{Path, State};

#[derive(serde::Serialize)]
pub struct KeySummary {
    pub id: i32,
    pub name: String,
}
pub async fn list_keys(State(pool): DbState) -> Result<Json<Vec<KeySummary>>> {
    let keys = sqlx::query_as!(Key, "SELECT * FROM keys ORDER BY name").fetch_all(&*pool).await?;

    let summaries = keys.into_iter().map(|k| KeySummary { id: k.id, name: k.name }).collect();
    Ok(Json(summaries))
}

#[derive(serde::Deserialize)]
pub struct CreateKeyReq {
    pub name: String,
    pub userid: String,
}
#[derive(serde::Serialize)]
pub struct CreateKeyResp {
    pub id: i32,
    pub name: String,
    pub public_armor: String,
}
pub async fn create_key(
    State(pool): DbState,
    Json(req): Json<CreateKeyReq>,
) -> Result<Json<CreateKeyResp>> {
    let mgr = libsubatomic::sig::Mgr::new(req.userid);

    let pri_bytes = mgr.write();

    let public_armor = mgr
        .public_armor()
        .map_err(|e| ApiError::Internal(format!("fail to armor public key: {e}")))?;

    let rec = sqlx::query_as!(
        Key,
        "INSERT INTO keys (name, pri) VALUES ($1, $2) RETURNING *",
        &req.name,
        &pri_bytes
    )
    .fetch_one(&*pool)
    .await?;

    Ok(Json(CreateKeyResp { id: rec.id, name: req.name, public_armor }))
}

pub async fn get_key(State(pool): DbState, Path(id): Path<i32>) -> Result<String> {
    let key = sqlx::query_as!(Key, "SELECT * FROM keys WHERE id = $1", id)
        .fetch_optional(&*pool)
        .await?
        .ok_or(ApiError::NotFound)?;

    let mgr = libsubatomic::sig::Mgr::parse(&key.pri)
        .map_err(|e| ApiError::Internal(format!("Failed to parse private key: {e}")))?;

    mgr.public_armor().map_err(|e| ApiError::Internal(format!("Failed to armor public key: {e}")))
}
