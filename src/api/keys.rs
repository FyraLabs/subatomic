use crate::DbState;
use crate::db::Key;
use crate::error::{ApiError, Result};
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;

#[derive(serde::Serialize)]
pub struct KeySummary {
    pub id: String,
    pub userid: String,
}
pub async fn list_keys(State(pool): DbState) -> Result<Json<Vec<KeySummary>>> {
    let keys = sqlx::query_as!(Key, "SELECT * FROM keys").fetch_all(&*pool).await?;

    let summaries = keys.into_iter().map(|k| KeySummary { id: k.id, userid: k.userid }).collect();
    Ok(Json(summaries))
}

#[derive(serde::Deserialize)]
pub struct CreateKeyReq {
    pub id: String,
    pub userid: String,
}
#[derive(serde::Serialize)]
pub struct CreateKeyResp {
    pub id: String,
    pub public_armor: String,
}
pub async fn create_key(
    State(pool): DbState,
    Json(req): Json<CreateKeyReq>,
) -> Result<Json<CreateKeyResp>> {
    let mgr = libsubatomic::sig::Mgr::new(req.userid.clone());

    let pri_bytes = mgr.to_armor();

    let public_armor = mgr
        .public_armor()
        .map_err(|e| ApiError::Internal(format!("fail to armor public key: {e}")))?;

    let rec = sqlx::query_as!(
        Key,
        "INSERT INTO keys (id, userid, pri) VALUES ($1, $2, $3) RETURNING *",
        &req.id,
        &req.userid,
        &pri_bytes
    )
    .fetch_one(&*pool)
    .await?;

    Ok(Json(CreateKeyResp { id: rec.id, public_armor }))
}

#[derive(serde::Serialize)]
pub struct GetKeyResp {
    pub userid: String,
    pub public_armor: String,
}
pub async fn get_key(State(pool): DbState, Path(id): Path<String>) -> Result<Json<GetKeyResp>> {
    let key = sqlx::query_as!(Key, "SELECT * FROM keys WHERE id = $1", id)
        .fetch_optional(&*pool)
        .await?
        .ok_or(ApiError::NotFound)?;

    let mgr = libsubatomic::sig::Mgr::from_armor(&key.pri)
        .map_err(|e| ApiError::Internal(format!("Failed to parse private key: {e}")))?;

    let public_armor = mgr
        .public_armor()
        .map_err(|e| ApiError::Internal(format!("Failed to armor public key: {e}")))?;
    Ok(Json(GetKeyResp { userid: key.userid, public_armor }))
}

pub async fn del_key(
    State(pool): DbState,
    Path(id): Path<String>,
) -> Result<(StatusCode, &'static str)> {
    // db has fk check, we don't need to modify locker as we are certain nobody is using the key
    let q = sqlx::query!("DELETE FROM keys WHERE id = $1", id);
    let q = q.execute(&*pool).await;
    if q.as_ref()
        .is_err_and(|e| e.as_database_error().is_some_and(|e| e.is_foreign_key_violation()))
    {
        return Ok((StatusCode::CONFLICT, "the key is still in use (by another repo)"));
    }
    Ok((if q?.rows_affected() == 0 { StatusCode::NOT_FOUND } else { StatusCode::NO_CONTENT }, ""))
}

#[cfg(test)]
mod test {
    use axum::extract::Path;

    type Pool = sqlx::Pool<sqlx::Postgres>;

    fn state(pool: Pool) -> crate::DbState {
        axum::extract::State(std::sync::Arc::new(pool))
    }

    #[sqlx::test(fixtures("keys"))]
    async fn list_keys(pool: Pool) {
        let axum::Json(keys) = super::list_keys(state(pool)).await.unwrap();
        assert_eq!(keys[0].id, "key1");
        assert_eq!(keys[0].userid, "key1 <k1@example.com>");
        assert_eq!(keys[1].id, "key2");
        assert_eq!(keys[1].userid, "key2 <k2@example.com>");
    }

    #[sqlx::test]
    async fn create_key(pool: Pool) {
        const ID: &str = "testkey";
        let req = axum::Json(crate::api::keys::CreateKeyReq {
            id: ID.into(),
            userid: "name <testkeymail@example.com>".into(),
        });
        let axum::Json(resp) = super::create_key(state(pool), req).await.unwrap();
        assert_eq!(resp.id, ID);
    }

    #[sqlx::test(fixtures("keys"))]
    async fn get_key(pool: Pool) {
        let resp = super::get_key(state(pool), Path("key1".into())).await.unwrap();
        // ↓ もーちょっといいフォーマットほしい
        assert_eq!(
            resp.public_armor,
            "-----BEGIN PGP PUBLIC KEY BLOCK-----\n\nxiYEap7sJBswZX6KXHpfqETJO4rY+QtWtpdN0LDn5xThopaO+0Orr80xUlBNIEZp\nc3Npb24gPG51Y2xlYXJmaXNzaW9uLWJ1aWxkc3lzQGV4YW1wbGUuY29tPsKCBBMb\nCAAuBQJqnuwkFiEEXJRZV2R0/irfk0kpCQbDJozm21kCGwMCHgEBCwEVARYBJwIZ\nAQAKCRAJBsMmjObbWcmepReucqGgmJpkfBsJTy2GBxosNWNVJgDUQ3tme7PrlVky\nWS8MgEGm+39VbWwIk6oN/LBttfmijVIhRR8p0heuDg==\n=vX5C\n-----END PGP PUBLIC KEY BLOCK-----\n"
        );
        assert_eq!(resp.userid, "key1 <k1@example.com>");
    }

    #[sqlx::test(fixtures("keys", "repos"))]
    async fn del_key(pool: Pool) {
        let call = super::del_key(state(pool.clone()), Path("key2".into()));
        assert_eq!(call.await.unwrap().0, super::StatusCode::NO_CONTENT);
        let axum::Json(keys) = super::list_keys(state(pool.clone())).await.unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].id, "key1");
        assert_eq!(keys[0].userid, "key1 <k1@example.com>");
        let call = super::del_key(state(pool.clone()), Path("key2".into()));
        assert_eq!(call.await.unwrap().0, super::StatusCode::NOT_FOUND);
        let call = super::del_key(state(pool.clone()), Path("key1".into()));
        assert_eq!(call.await.unwrap().0, super::StatusCode::CONFLICT);
    }
}
