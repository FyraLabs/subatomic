use std::io::Write;
use std::os::unix::ffi::OsStrExt;

use crate::db::{Key, Repo};
use crate::error::{ApiError, Result};
use crate::{DbState, LockerState};
use axum::Json;
use axum::extract::{Multipart, Path, State};
use axum::http::StatusCode;
use futures_util::TryStreamExt;
use libsubatomic::err::Res;
use libsubatomic::prelude::Itertools;
use tokio_util::io::StreamReader;

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
) -> Result<Json<serde_json::Value>> {
    let Some((dir, keys, sig)) = locker
        .read(&repo, async |hdl| {
            (hdl.repo.dir.clone(), hdl.repo.cache.keys(), hdl.repo.sig.clone())
        })
        .await?
    else {
        return Err(ApiError::NotFound);
    };
    let keys = keys.map_err(|e| ApiError::Internal(format!("can't get cache keys: {e}")))?;
    tokio::fs::create_dir_all(&dir).await?;
    let parsed_keys = keys
        .iter()
        .map(|k| (k, libsubatomic::pkg::parse_filename(k).expect("can't parse cache keys")))
        .collect_vec();
    let mut bad_filenames = Vec::new();
    let mut removed = Vec::new();
    let mut pkgs = Vec::new();
    let mut out = Vec::new();
    while let Some(field) =
        multipart.next_field().await.map_err(|e| ApiError::Internal(e.to_string()))?
    {
        let name = (field.name())
            .ok_or_else(|| ApiError::BadRequest("filename should not be empty".into()))?;
        let path = dir.join(name);
        let mut body_reader =
            std::pin::pin!(StreamReader::new(field.map_err(std::io::Error::other)));
        let writer = try bikeshed std::io::Result<_> {
            let fd = tokio::io::BufWriter::new(tokio::fs::File::create(&path).await?);
            let mut writer = UploadWriter {
                fd,
                csum: libsubatomic::repodata::RepoWriterCsum::Sha256(Default::default()),
            };
            tokio::io::copy(&mut body_reader, &mut writer).await?;
            writer
        }
        .map_err(|e| ApiError::Internal(format!("cannot process uploads: {e}")))?;

        let filename = path.file_name().expect("expected file").as_bytes();
        let Some(libsubatomic::pkg::ParsePathOutput { name, arch, .. }) =
            libsubatomic::pkg::parse_filename(filename)
        else {
            bad_filenames.push(path);
            continue;
        };
        let prev_versions = (parsed_keys.iter())
            .filter(|(_, k)| k.name == name && k.arch == arch)
            .filter(|(k, _)| *k != filename);
        removed.extend(prev_versions.map(|(k, _)| k.as_slice()));
        let (pkg, mut rpmreader) = libsubatomic::Package::parse(
            writer.fd.into_inner().into_std().await,
            writer.csum.csum(),
        )
        .map_err(|e| ApiError::BadRequest(format!("cannot parse rpm: {e}")))?;
        let sig = if let Some(sig) = &sig {
            tracing::debug!("signing");
            let sig = sig
                .sign_rpm(&rpmreader.metadata)
                .map_err(|e| ApiError::Internal(format!("cannot sign: {e}")))?;
            if let Err(e) =
                libsubatomic::prelude::rpm::Package::apply_signature_in_place(&path, sig.clone())
            {
                let libsubatomic::prelude::rpm::Error::InsufficientReservedSpace { .. } = e else {
                    return Err(ApiError::Internal(format!("cannot sign pkg: {e}")));
                };
                tracing::debug!("cannot apply signature in place, opening full file");
                let mut p = libsubatomic::prelude::rpm::Package::open(&path)
                    .map_err(|e| ApiError::Internal(format!("cannot open rpm: {e}")))?;
                p.apply_signature(sig.clone())
                    .map_err(|e| ApiError::Internal(format!("cannot apply signature: {e}")))?;
                p.write_file(&path)
                    .map_err(|e| ApiError::Internal(format!("cannot write file with sig: {e}")))?;
            }
            Some(sig)
        } else {
            None
        };
        out.push(serde_json::json!({
            "pkg": filename,
            "sig": sig,
        }));
        let mut frag = libsubatomic::repodata::FragEph::new(&pkg, path.as_os_str());
        let appstream = libsubatomic::pkg::Package::appstream_frag(&mut rpmreader)
            .map_err(|e| ApiError::BadRequest(format!("cannot parse appstream in rpm: {e}")))?;
        if !appstream.is_empty() {
            frag.app = libsubatomic::repodata::Frag(Some(appstream));
        }
        pkgs.push((path.as_os_str().as_encoded_bytes().to_owned(), frag));
    }
    let Some(res) = locker
        .write(&repo, async |hdl| try bikeshed Res<_> {
            hdl.repo.del(&removed)?;
            hdl.repo.cache.insert_fragments(pkgs)?;
            hdl.repo.generate()?;
        })
        .await?
    else {
        return Err(ApiError::NotFound);
    };
    res?;
    Ok(Json(serde_json::json!({
        "added": out,
        "bad_filenames": bad_filenames,
        "removed": removed,
    })))
}

struct UploadWriter {
    fd: tokio::io::BufWriter<tokio::fs::File>,
    csum: libsubatomic::repodata::RepoWriterCsum,
}

impl tokio::io::AsyncWrite for UploadWriter {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let ret = tokio::io::AsyncWrite::poll_write(std::pin::pin!(&mut self.as_mut().fd), cx, buf);
        if let std::task::Poll::Ready(res) = &ret {
            res.as_ref()
                .inspect(|&&len| _ = self.csum.write(&buf[..len]))
                .expect("hashing should not panic");
        }
        ret
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        tokio::io::AsyncWrite::poll_flush(std::pin::pin!(&mut self.as_mut().fd), cx)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        tokio::io::AsyncWrite::poll_shutdown(std::pin::pin!(&mut self.as_mut().fd), cx)
    }
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
        .unwrap_or_else(|| Err(ApiError::NotFound))
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

// pub async fn resign(State(locker): LockerState, Path(repo): Path<String>) -> Result<StatusCode> {
//     let q = locker.write(&repo, async |repohdl| repohdl.repo.resign_all()).await?;
//     Ok(if q.transpose()?.is_some() { StatusCode::NO_CONTENT } else { StatusCode::NOT_FOUND })
// }

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
    tracing::info!(?rpms, "deleting rpms");
    let q = locker.write(&repo, async |repohdl| try bikeshed Result<_> {
        let out =
            repohdl.repo.del(&rpms.iter().map(std::string::String::as_bytes).collect_vec()).map(
                |v| v.into_iter().map(|s| String::from_utf8_lossy(s).to_string()).collect_vec(),
            )?;
        repohdl.repo.generate()?;
        out
    });
    let Some(not_found) = q.await?.transpose()? else {
        return Err(ApiError::NotFound);
    };
    Ok(Json(serde_json::json!({ "not_found": not_found })))
}

pub async fn upl_md(
    State(locker): LockerState,
    Path((repo, md)): Path<(String, String)>,
    mut multipart: Multipart,
) -> Result<StatusCode> {
    let field = (multipart.next_field().await.expect("multipart err"))
        .ok_or_else(|| ApiError::BadRequest("expect multipart (file upload)".to_owned()))?;
    let filename =
        field.file_name().ok_or_else(|| ApiError::BadRequest("expected filename".into()))?.into();
    let content = (field.bytes().await)
        .map_err(|e| ApiError::BadRequest(format!("cannot get file bytes: {e}")))?;

    let w = locker.write(&repo, async |hdl| try bikeshed Res<()> {
        hdl.repo.cache.update_custom_datatype(
            libsubatomic::DataType::Custom(md.into(), filename),
            &content,
        )?;
        // TODO: only generate repomd
        hdl.repo.generate()?;
    });
    if w.await?.transpose()?.is_some() {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Ok(StatusCode::NOT_FOUND)
    }
}

pub async fn del_md(
    State(locker): LockerState,
    Path((repo, md)): Path<(String, String)>,
) -> Result<StatusCode> {
    let w = locker.write(&repo, async |hdl| try bikeshed Res<()> {
        hdl.repo.cache.del_custom_datatype(&md)?;
        // TODO: only generate repomd
        hdl.repo.generate()?;
    });
    if w.await?.transpose()?.is_some() {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Ok(StatusCode::NOT_FOUND)
    }
}
