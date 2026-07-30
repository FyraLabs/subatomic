use crate::db::{Key, Repo};
use crate::error::{ApiError, Result};
use crate::{DbState, LockerState};
use axum::Json;
use axum::extract::{Multipart, Path, State};
use axum::http::StatusCode;
use libsubatomic::err::Res;
use libsubatomic::prelude::Itertools;

pub async fn list_repos(State(pool): DbState) -> Result<Json<Vec<Repo>>> {
    Ok(Json(sqlx::query_as!(Repo, "SELECT * FROM repos ORDER BY name").fetch_all(&*pool).await?))
}

pub async fn create_repo(State(pool): DbState, Path(name): Path<String>) -> Result<Json<Repo>> {
    Ok(Json(
        sqlx::query_as!(Repo, "INSERT INTO repos (name) VALUES ($1) RETURNING *", &name)
            .fetch_one(&*pool)
            .await?,
    ))
}

pub async fn upload_pkgs(
    State(locker): LockerState,
    Path(repo): Path<String>,
    mut multipart: Multipart,
) -> Result<StatusCode> {
    let Some(dir) = locker.read(&repo, async |repohdl| repohdl.repo.cache.dir.clone()).await?
    else {
        return Ok(StatusCode::NOT_FOUND);
    };
    let mut pkgs = Vec::new();
    while let Some(field) = multipart.next_field().await.expect("multipart err") {
        let name = (field.name())
            .ok_or_else(|| ApiError::BadRequest("filename should not be empty".into()))?;
        let path = dir.join(name);
        let data = (field.bytes().await)
            .map_err(|e| ApiError::BadRequest(format!("cannot get file bytes: {e}")))?;

        tokio::fs::write(&path, data).await?;
        pkgs.push(path);
    }
    let pkgs2 = pkgs.iter().map(std::path::PathBuf::as_path).collect_vec();
    let pkgs2 = pkgs2.as_slice();
    let Some(out) = locker.write(&repo, async |hdl| hdl.repo.add_replace(pkgs2)).await? else {
        // something happened during last processing…!?
        return Ok(StatusCode::NOT_FOUND);
    };
    out?;
    // TODO: maybe return `out` as json?
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_repo(
    State(locker): LockerState,
    Path(name): Path<String>,
) -> Result<StatusCode> {
    Ok(if locker.del(&name).await? { StatusCode::NO_CONTENT } else { StatusCode::NOT_FOUND })
}

pub async fn push_comps(
    State(locker): LockerState,
    Path(repo): Path<String>,
    mut multipart: Multipart,
) -> Result<StatusCode> {
    let field = (multipart.next_field().await.expect("multipart err"))
        .ok_or_else(|| ApiError::BadRequest("expect multipart (file upload)".to_owned()))?;
    let comps = (field.bytes().await)
        .map_err(|e| ApiError::BadRequest(format!("cannot get file bytes: {e}")))?;

    if locker
        .write(&repo, async |hdl| try bikeshed Res<()> {
            hdl.repo.add_comps(&comps)?;
            // TODO: only generate repomd
            hdl.repo.generate()?;
        })
        .await?
        .transpose()?
        .is_some()
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Ok(StatusCode::NOT_FOUND)
    }
}

pub async fn del_comps(State(locker): LockerState, Path(repo): Path<String>) -> Result<StatusCode> {
    if locker
        .write(&repo, async |hdl| try bikeshed Res<()> {
            hdl.repo.del_comps()?;
            // TODO: only generate repomd
            hdl.repo.generate()?;
        })
        .await?
        .transpose()?
        .is_some()
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Ok(StatusCode::NOT_FOUND)
    }
}

pub async fn get_key(State(locker): LockerState, Path(repo): Path<String>) -> Result<String> {
    locker
        .read(&repo, async |hdl| {
            let Some(mgr) = &hdl.repo.sig else {
                return Err(ApiError::Internal("no key for this repo".into()));
            };
            mgr.public_armor().map_err(|e| ApiError::Internal(format!("pgp error: {e}")))
        })
        .await?
        .map_or_else(|| Err(ApiError::NotFound), |res| res)
}

#[derive(serde::Deserialize)]
pub struct SetKeyReq {
    id: i32,
}
pub async fn set_key(
    State(db): DbState,
    State(locker): LockerState,
    Path(repo): Path<String>,
    Json(SetKeyReq { id }): Json<SetKeyReq>,
) -> Result<StatusCode> {
    let q = sqlx::query_as!(Key, "SELECT * FROM keys WHERE id = $1", id);
    let Some(key) = q.fetch_optional(&*db).await? else {
        return Ok(StatusCode::NOT_FOUND);
    };
    let mgr = libsubatomic::sig::Mgr::parse(&key.pri).map_err(libsubatomic::err::Error::from)?;

    let Some(true) = locker
        .write(&repo, async |mut hdl| try bikeshed sqlx::Result<bool> {
            let q = sqlx::query!("UPDATE repos SET key_id = $1 WHERE name = $2", key.id, &repo);
            let ra = q.execute(&*db).await?.rows_affected();
            if ra == 0 {
                return Ok(false);
            }
            hdl.repo.sig = Some(mgr);
            true
        })
        .await?
        .transpose()?
    else {
        return Ok(StatusCode::NOT_FOUND);
    };
    Ok(StatusCode::NO_CONTENT)
}

pub async fn del_key(
    State(db): DbState,
    State(locker): LockerState,
    Path(repo): Path<String>,
) -> Result<StatusCode> {
    let Some(true) = locker
        .write(&repo, async |mut hdl| try bikeshed sqlx::Result<bool> {
            let q = sqlx::query!("UPDATE repos SET key_id = NULL WHERE name = $1", &repo);
            let ra = q.execute(&*db).await?.rows_affected();
            if ra == 0 {
                return Ok(false);
            }
            hdl.repo.sig = None;
            true
        })
        .await?
        .transpose()?
    else {
        return Ok(StatusCode::NOT_FOUND);
    };
    Ok(StatusCode::NO_CONTENT)
}

#[deprecated = "todo"]
pub async fn resign(State(locker): LockerState, Path(repo): Path<String>) -> Result<StatusCode> {
    drop((locker, repo));
    todo!()
}

pub async fn refresh_repo(
    State(locker): LockerState,
    Path(name): Path<String>,
) -> Result<StatusCode> {
    let q = locker.read(&name, async |repohdl| repohdl.repo.regenerate(true)).await?;
    Ok(if q.transpose()?.is_some() { StatusCode::NO_CONTENT } else { StatusCode::NOT_FOUND })
}

pub async fn rebuild_repo(
    State(locker): LockerState,
    Path(name): Path<String>,
) -> Result<StatusCode> {
    let q = locker.read(&name, async |repohdl| repohdl.repo.regenerate(false)).await?;
    Ok(if q.transpose()?.is_some() { StatusCode::NO_CONTENT } else { StatusCode::NOT_FOUND })
}

pub async fn list_rpms(
    State(locker): LockerState,
    Path(repo): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let Some(keys) = locker
        .read(&repo, async |repohdl| try bikeshed Res<_> { repohdl.repo.cache.keys()? })
        .await?
        .transpose()?
    else {
        return Err(ApiError::NotFound);
    };
    Ok(Json(serde_json::Value::Array(
        keys.into_iter()
            .map(|v| serde_json::Value::String(String::from_utf8_lossy(&v).to_string()))
            .collect(),
    )))
}

#[derive(serde::Deserialize)]
pub struct DelRpmsReq {
    rpms: Vec<String>,
}
pub async fn del_rpms(
    State(locker): LockerState,
    Path(repo): Path<String>,
    Json(DelRpmsReq { rpms }): Json<DelRpmsReq>,
) -> Result<Json<serde_json::Value>> {
    let Some(not_found) = locker
        .write(&repo, async |repohdl| {
            repohdl.repo.del(&rpms.iter().map(std::string::String::as_bytes).collect_vec()).map(
                |v| v.into_iter().map(|s| String::from_utf8_lossy(s).to_string()).collect_vec(),
            )
        })
        .await?
        .transpose()?
    else {
        return Err(ApiError::NotFound);
    };
    Ok(Json(serde_json::json!({ "not_found": not_found })))
}
