use crate::db::Repo;
use crate::error::{ApiError, Result};
use crate::{DbState, LockerState};
use axum::Json;
use axum::extract::{Multipart, Path, State};
use axum::http::StatusCode;
use libsubatomic::prelude::*;

pub async fn list_repos(State(pool): DbState) -> Result<Json<Vec<Repo>>> {
    Ok(Json(
        sqlx::query_as::<_, Repo>("SELECT * FROM repos ORDER BY name").fetch_all(&*pool).await?,
    ))
}

pub async fn create_repo(State(pool): DbState, Path(name): Path<String>) -> Result<Json<Repo>> {
    Ok(Json(
        sqlx::query_as::<_, Repo>("INSERT INTO repos (name) VALUES ($1) RETURNING *")
            .bind(&*name)
            .fetch_one(&*pool)
            .await?,
    ))
}

pub async fn upload_pkg(
    State(locker): LockerState,
    Path(repo): Path<String>,
    mut multipart: Multipart,
) -> Result<StatusCode> {
    let Some(dir) = locker.read(&*repo, async |repohdl| repohdl.repo.cache.dir.clone()).await?
    else {
        return Ok(StatusCode::NOT_FOUND);
    };
    let mut pkgs = Vec::new();
    while let Some(field) = multipart.next_field().await.expect("multipart err") {
        let name = (field.name())
            .ok_or(ApiError::BadRequest("filename should not be empty".into()))?
            .to_owned();
        let data = (field.bytes().await)
            .map_err(|e| ApiError::BadRequest(format!("cannot get file bytes: {e}")))?;

        let path = dir.join(&name);
        tokio::fs::write(&path, data).await?;
        pkgs.push(path);
    }
    let pkgs2 = pkgs.iter().map(|p| p.as_path()).collect_vec();
    let pkgs2 = pkgs2.as_slice();
    let Some(out) = locker.write(&*repo, async |hdl| hdl.repo.add_replace(pkgs2)).await? else {
        // something happened during last processing…!?
        return Ok(StatusCode::NOT_FOUND);
    };
    out?;
    // TODO: maybe return `out` as json?
    Ok(StatusCode::NO_CONTENT)
}

pub async fn refresh_repo(
    State(locker): LockerState,
    Path(name): Path<String>,
) -> Result<StatusCode> {
    let q = locker.read(&*name, async |repohdl| repohdl.repo.regenerate(true)).await?;
    Ok(if q.transpose()?.is_some() { StatusCode::NO_CONTENT } else { StatusCode::NOT_FOUND })
}

pub async fn rebuild_repo(
    State(locker): LockerState,
    Path(name): Path<String>,
) -> Result<StatusCode> {
    let q = locker.read(&*name, async |repohdl| repohdl.repo.regenerate(false)).await?;
    Ok(if q.transpose()?.is_some() { StatusCode::NO_CONTENT } else { StatusCode::NOT_FOUND })
}

pub async fn delete_repo(
    State(locker): LockerState,
    Path(name): Path<String>,
) -> Result<StatusCode> {
    Ok(if locker.del(&name).await? { StatusCode::NO_CONTENT } else { StatusCode::NOT_FOUND })
}
