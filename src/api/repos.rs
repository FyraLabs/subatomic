#![allow(clippy::missing_errors_doc)]
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

pub async fn sign_headers(
    State(locker): LockerState,
    Path(repo): Path<String>,
    mut multipart: Multipart,
) -> Result<impl axum::response::IntoResponse> {
    let sig =
        locker.read(&repo, async |hdl| hdl.repo.sig.clone()).await?.ok_or(ApiError::NotFound)?;
    let Some(mgr) = sig.as_ref() else { return Ok((StatusCode::NO_CONTENT, Default::default())) };
    let mut resp = rust_multipart_rfc7578_2::client::multipart::Form::default();

    while let Some(field) =
        multipart.next_field().await.map_err(|e| ApiError::Internal(e.to_string()))?
    {
        let body = (field.bytes().await)
            .map_err(|e| ApiError::BadRequest(format!("cannot get multipart field: {e}")))?;
        let mut bufr = std::io::BufReader::new(body.as_ref());
        let mut metadata = libsubatomic::rpm::PackageMetadata::parse(&mut bufr)
            .map_err(|e| ApiError::BadRequest(format!("cannot parse rpm metadata: {e}")))?;

        let sig = (mgr.sign_rpm(&mut metadata))
            .map_err(|e| ApiError::Internal(format!("cannot sign: {e}")))?;
        // `Cursor` to feed in owned sig
        resp.add_reader_2("", std::io::Cursor::new(sig), None, None, vec![]);
    }
    Ok((
        StatusCode::OK,
        axum::body::Body::from_stream(rust_multipart_rfc7578_2::client::multipart::Body::from(
            resp,
        )),
    ))
}

pub async fn upload_pkgs(
    State(locker): LockerState,
    Path(repo): Path<String>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>> {
    let r = locker.read(&repo, async |hdl| {
        (hdl.repo.dir.clone(), hdl.repo.cache.keys(), hdl.repo.sig.clone())
    });
    let (dir, keys, sig) = r.await?.ok_or(ApiError::NotFound)?;
    let keys = keys.map_err(|e| ApiError::Internal(format!("can't get cache keys: {e}")))?;
    tokio::fs::create_dir_all(&dir).await?;

    let parsed_keys = keys
        .iter()
        .map(|k| (k, libsubatomic::pkg::parse_filename(k).expect("can't parse cache keys")))
        .collect_vec();
    let mut processor = UploadProcessor { dir, parsed_keys, sig, ..UploadProcessor::default() };
    processor.receive_rpms(&mut multipart).await?;
    let w = locker.write(&repo, async |hdl| try bikeshed Res<_> {
        hdl.repo.del(&processor.removed)?;
        hdl.repo.cache.insert_fragments(processor.pkgs)?;
        hdl.repo.generate()?;
    });
    w.await?.ok_or_else(|| ApiError::NotFound)??;
    Ok(Json(serde_json::json!({
        // "added": processor.out,
        "removed": processor.removed,
    })))
}

#[derive(Default)]
struct UploadProcessor<'k> {
    dir: std::path::PathBuf,
    parsed_keys: Vec<(&'k Vec<u8>, libsubatomic::pkg::ParsePathOutput<'k>)>,
    removed: Vec<&'k [u8]>,
    out: Vec<serde_json::Value>,
    pkgs: Vec<(Vec<u8>, libsubatomic::repodata::FragEph)>,
    sig: Option<libsubatomic::sig::Mgr>,
}

struct ReceiveRpmOut {
    csum: String,
    path: std::path::PathBuf,
}

impl UploadProcessor<'_> {
    async fn receive_rpms(&mut self, multipart: &mut Multipart) -> Result<()> {
        while let Some(field) =
            multipart.next_field().await.map_err(|e| ApiError::Internal(e.to_string()))?
        {
            let ReceiveRpmOut { csum, path } = self.receive_rpm(field).await?;
            // let filename_str = (path.file_name().expect("filename").to_str())
            //     .ok_or_else(|| ApiError::BadRequest("invalid utf8 filename".to_owned()))?;
            self.check_csum(multipart, &csum).await?;
            // let sig = self.sign(&path)?;
            let frag = Self::parse_to_frag(&path, csum)?;
            let path = path.strip_prefix(&self.dir).expect("rpm not in repodir");
            self.pkgs.push((path.as_os_str().as_bytes().to_owned(), frag));
            // self.out.push(serde_json::json!({
            //     "pkg": filename_str,
            //     "sig": sig,
            // }));
        }
        Ok(())
    }

    async fn check_csum(&self, multipart: &mut Multipart, csum: &str) -> Result<()> {
        let field = (multipart.next_field().await)
            .map_err(|e| {
                ApiError::Internal(format!("can't get hash field: {e}: {}", e.body_text()))
            })?
            .ok_or_else(|| ApiError::BadRequest("expect hash after file upload".to_owned()))?;
        if (field.text().await)
            .map_err(|e| ApiError::BadRequest(format!("can't get hash text: {e}")))?
            != csum
        {
            return Err(ApiError::BadRequest(format!("calculated sha256: {csum}")));
        }
        Ok(())
    }

    fn sign(&self, path: &std::path::PathBuf) -> Result<Option<Vec<u8>>> {
        let Some(mgr) = self.sig.as_ref() else { return Ok(None) };
        let sig = Self::get_sig(path, mgr)?;
        let Err(e) = libsubatomic::rpm::Package::apply_signature_in_place(path, sig.clone()) else {
            return Ok(Some(sig));
        };
        let libsubatomic::rpm::Error::InsufficientReservedSpace { .. } = e else {
            return Err(ApiError::Internal(format!("cannot sign pkg: {e}")));
        };
        tracing::debug!("cannot apply signature in place, opening full file");
        let mut p = libsubatomic::prelude::rpm::Package::open(path)
            .map_err(|e| ApiError::Internal(format!("cannot open rpm: {e}")))?;
        p.apply_signature(sig.clone())
            .map_err(|e| ApiError::Internal(format!("cannot apply signature: {e}")))?;
        p.write_file(path)
            .map_err(|e| ApiError::Internal(format!("cannot write file with sig: {e}")))?;
        Ok(Some(sig))
    }

    fn get_sig(path: &std::path::PathBuf, mgr: &libsubatomic::sig::Mgr) -> Result<Vec<u8>> {
        let fd = std::fs::File::open(path)?;
        let mut bufr = std::io::BufReader::new(fd);
        let mut metadata = libsubatomic::rpm::PackageMetadata::parse(&mut bufr)
            .map_err(|e| ApiError::BadRequest(format!("cannot parse rpm: {e}")))?;
        let metadata: &mut libsubatomic::rpm::PackageMetadata = &mut metadata;
        tracing::debug!("signing");
        let sig =
            mgr.sign_rpm(metadata).map_err(|e| ApiError::Internal(format!("cannot sign: {e}")))?;
        Ok(sig)
    }

    async fn receive_rpm(
        &mut self,
        field: axum::extract::multipart::Field<'_>,
    ) -> Result<ReceiveRpmOut> {
        let name = (field.file_name())
            .ok_or_else(|| ApiError::BadRequest("filename should not be empty".into()))?;
        let path = self.dir.join(name);
        let mut body_reader =
            std::pin::pin!(StreamReader::new(field.map_err(std::io::Error::other)));
        let writer = try bikeshed std::io::Result<_> {
            let fd = tokio::fs::File::create(&path).await?;
            let fd = tokio::io::BufWriter::new(fd);
            let csum = libsubatomic::repodata::RepoWriterCsum::Sha256(Default::default());
            let mut writer = UploadWriter { fd, csum };
            tokio::io::copy(&mut body_reader, &mut writer).await?;
            writer
        }
        .map_err(|e| ApiError::Internal(format!("cannot process uploads: {e}")))?;

        let filename = path.file_name().expect("expected file").as_bytes();
        let Some(libsubatomic::pkg::ParsePathOutput { name, arch, .. }) =
            libsubatomic::pkg::parse_filename(filename)
        else {
            return Err(ApiError::BadRequest("invalid rpm filename format".to_owned()));
        };
        let prev_versions = (self.parsed_keys.iter())
            .filter(|(_, k)| k.name == name && k.arch == arch)
            .filter(|(k, _)| *k != filename);
        self.removed.extend(prev_versions.map(|(k, _)| k.as_slice()));
        let csum = writer.csum.csum().into();
        Ok(ReceiveRpmOut { csum, path })
    }

    fn parse_to_frag(
        path: &std::path::Path,
        csum: String,
    ) -> Result<libsubatomic::repodata::FragEph, ApiError> {
        let fd = std::fs::File::open(path)?;
        let (pkg, mut rpmreader) = libsubatomic::Package::parse(fd, csum.into())
            .map_err(|e| ApiError::BadRequest(format!("cannot parse rpm: {e}")))?;
        let mut frag = libsubatomic::repodata::FragEph::new(&pkg, path.as_os_str());
        let appstream = libsubatomic::pkg::Package::appstream_frag(&mut rpmreader)
            .map_err(|e| ApiError::BadRequest(format!("cannot parse appstream in rpm: {e}")))?;
        if !appstream.is_empty() {
            frag.app = libsubatomic::repodata::Frag(Some(appstream));
        }
        Ok(frag)
    }
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
        if let std::task::Poll::Ready(Ok(len)) = &ret {
            self.csum.write_all(&buf[..*len]).expect("hashing should not fail");
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

#[deprecated]
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

#[deprecated]
pub async fn del_comps(State(locker): LockerState, Path(repo): Path<String>) -> Result<StatusCode> {
    let w = locker.write(&repo, async |hdl| try bikeshed Res<()> {
        hdl.repo.del_comps()?;
        // TODO: only generate repomd
        hdl.repo.generate()?;
    });
    if w.await?.transpose()?.is_some() {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Ok(StatusCode::NOT_FOUND)
    }
}

pub async fn get_key(State(locker): LockerState, Path(repo): Path<String>) -> Result<String> {
    locker
        .read(&repo, async |hdl| {
            let Some(mgr) = &hdl.repo.sig else {
                return Err(ApiError::NotFound);
            };
            mgr.public_armor().map_err(|e| ApiError::Internal(format!("pgp error: {e}")))
        })
        .await?
        .unwrap_or_else(|| Err(ApiError::NotFound))
}

#[derive(serde::Deserialize)]
pub struct SetKeyReq {
    id: String,
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
    let mgr =
        libsubatomic::sig::Mgr::from_armor(&key.pri).map_err(libsubatomic::err::Error::from)?;

    let w = locker.write(&repo, async |mut hdl| try bikeshed sqlx::Result<StatusCode> {
        let q = sqlx::query!("UPDATE repos SET key_id = $1 WHERE name = $2", key.id, &repo);
        let ra = q.execute(&*db).await?.rows_affected();
        if ra == 0 {
            return Ok(StatusCode::NOT_FOUND);
        }
        hdl.repo.sig = Some(mgr);
        StatusCode::NO_CONTENT
    });
    w.await?.transpose()?.ok_or(ApiError::NotFound)
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
        tokio::fs::create_dir_all(&hdl.repo.cache.repodata_dir).await?;
        tokio::fs::create_dir_all(&hdl.repo.cache.cachedir).await?;
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

#[cfg(test)]
mod test {
    use http_body_util::BodyExt;
    use std::sync::Arc;
    use tower::util::ServiceExt;

    use axum::extract::{Json, Path};
    use axum::{body::Body, http::Request};
    use rust_multipart_rfc7578_2::client::multipart::{
        Body as MultipartBody, Form as MultipartForm,
    };

    type Pool = sqlx::Pool<sqlx::Postgres>;

    fn db(pool: Pool) -> crate::DbState {
        axum::extract::State(Arc::new(pool))
    }

    const AUTH: &str = "Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiYWRtaW4iOnRydWUsImlhdCI6MTUxNjIzOTAyMiwiZXhwIjoyNzg1NTc4MDI2fQ.1t5hFfRtAcBCa68tuk4iJ9NOwZ09FttVzqmXo06oiVU";

    fn cfg() -> (Arc<crate::config::Config>, impl std::any::Any) {
        let storage_dir = tempfile::tempdir().expect("storage_dir");
        let cache_dir = tempfile::tempdir().expect("cache_dir");
        (
            Arc::new(crate::config::Config {
                server_host: "".into(),
                server_port: 0,
                database_url: "".into(),
                jwt_secret: "cad4a3a28cfdb1a464e26e5851e6cd44a95fd8c57c117d294a9e8391e70274d2"
                    .into(),
                storage_dir: storage_dir.path().to_owned(),
                cache_dir: cache_dir.path().to_owned(),
                body_limit: 10485760, // 10 MiB
            }),
            (storage_dir, cache_dir),
        )
    }

    fn locker(pool: Arc<Pool>, cfg: Arc<crate::config::Config>) -> crate::LockerState {
        axum::extract::State(Arc::new(crate::repohdl::Locker::new(pool, cfg)))
    }

    struct States<A> {
        app: axum::Router,
        cfg: Arc<crate::config::Config>,
        pool: crate::DbState,
        locker: crate::LockerState,
        dirobjs: A,
    }

    fn app(pool: Pool) -> States<impl std::any::Any> {
        let (cfg, dirobjs) = cfg();
        let pool = db(pool);
        let locker = locker(pool.0.clone(), cfg.clone());
        let app = crate::app(&cfg, pool.0.clone(), locker.0.clone());
        States { app, cfg, pool, locker, dirobjs }
    }

    #[sqlx::test(fixtures("keys", "repos"))]
    async fn list_repos(pool: Pool) {
        let axum::Json(resp) = super::list_repos(db(pool)).await.unwrap();
        assert!(resp.contains(&crate::db::Repo {
            id: 1,
            name: "rpmfission".into(),
            key_id: Some("key1".into())
        }));
        assert!(resp.contains(&crate::db::Repo { id: 2, name: "rpmball".into(), key_id: None }));
        assert_eq!(resp.len(), 2);
    }

    #[sqlx::test]
    async fn create_repo(pool: Pool) {
        let axum::Json(resp) = super::create_repo(db(pool), Path("neptune".into())).await.unwrap();
        assert_eq!(resp.name, "neptune");
        assert_eq!(resp.key_id, None);
    }

    #[sqlx::test(fixtures("keys", "repos"))]
    async fn upload_list_del_pkgs(pool: Pool) {
        const CSUM: &str = "bb6f1421400b7ac575d3b223f910b600990842a37b9f143fdd42380431165f77";
        let states = app(pool);
        let States { app, cfg, locker, .. } = states;
        let mut form = MultipartForm::default();
        form.add_reader_2(
            "terra-release-44-4.noarch.rpm",
            &include_bytes!("../../random-rpm-examples/terra-release-44-4.noarch.rpm")[..],
            Some("terra-release-44-4.noarch.rpm".into()),
            None,
            vec![],
        );
        form.add_text("a", CSUM);
        let req = Request::post("/v1/repos/rpmfission")
            .header("Authorization", AUTH)
            .header(axum::http::header::CONTENT_TYPE, form.content_type().as_str())
            .body(Body::from_stream(MultipartBody::from(form)))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        println!("{body}");
        assert!(body.get("removed").unwrap().as_array().unwrap().is_empty());
        let path = cfg.storage_dir.join("rpmfission/terra-release-44-4.noarch.rpm");
        assert!(std::fs::exists(&path).unwrap());
        let rpms = vec!["terra-release-44-4.noarch.rpm".into()];
        let ret = super::list_rpms(locker.clone(), Path("rpmfission".into())).await.unwrap().0;
        assert_eq!(ret.as_array().unwrap().len(), 1);
        assert_eq!(
            ret.as_array().unwrap().first().unwrap().as_str().unwrap(),
            "terra-release-44-4.noarch.rpm"
        );
        let ret =
            super::del_rpms(locker, Path("rpmfission".into()), Json(super::DelRpmsReq { rpms }));
        let ret = ret.await.unwrap().0;
        println!("{ret:?}");
        assert!(!std::fs::exists(&path).unwrap());
        assert!(ret.get("not_found").unwrap().as_array().unwrap().is_empty());
    }

    #[sqlx::test(fixtures("keys", "repos"))]
    async fn sign_headers(pool: Pool) {
        let states = app(pool);
        let States { app, .. } = states;
        let mut form = MultipartForm::default();
        let mut rpmmeta = libsubatomic::rpm::PackageMetadata::parse(&mut std::io::BufReader::new(
            &mut &include_bytes!("../../random-rpm-examples/terra-release-44-4.noarch.rpm")[..],
        ))
        .unwrap();
        let mut buf = vec![];
        rpmmeta.write(&mut buf).unwrap();
        form.add_reader_2(
            "terra-release-44-4.noarch.rpm",
            std::io::Cursor::new(buf.clone()),
            Some("terra-release-44-4.noarch.rpm".into()),
            None,
            vec![],
        );
        let req = Request::post("/v1/repos/rpmfission/sign")
            .header("Authorization", AUTH)
            .header(axum::http::header::CONTENT_TYPE, form.content_type().as_str())
            .body(Body::from_stream(MultipartBody::from(form)))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let i = body.windows(4).position(|bs| bs == b"\r\n\r\n").unwrap() + 4;
        let j = body.windows(4).rposition(|bs| bs == b"\r\n--").unwrap();
        let sig = body.slice(i..j);
        let mgr = libsubatomic::sig::Mgr::from_armor(
            "-----BEGIN PGP PRIVATE KEY BLOCK-----

xUkEap7sJBswZX6KXHpfqETJO4rY+QtWtpdN0LDn5xThopaO+0OrrwCb9NEYCgt/
X+732x931pW/h8IirjscbwJ5CQcG44Z1eA6xzTFSUE0gRmlzc2lvbiA8bnVjbGVh
cmZpc3Npb24tYnVpbGRzeXNAZXhhbXBsZS5jb20+woIEExsIAC4FAmqe7CQWIQRc
lFlXZHT+Kt+TSSkJBsMmjObbWQIbAwIeAQELARUBFgEnAhkBAAoJEAkGwyaM5ttZ
yZ6lF65yoaCYmmR8GwlPLYYHGiw1Y1UmANRDe2Z7s+uVWTJZLwyAQab7f1VtbAiT
qg38sG21+aKNUiFFHynSF64O
=lkCs
-----END PGP PRIVATE KEY BLOCK-----",
        )
        .unwrap();
        rpmmeta.signature =
            libsubatomic::rpm::SignatureHeaderBuilder::from_existing(&rpmmeta.signature)
                .unwrap()
                .add_openpgp_signature(sig.to_vec())
                .build()
                .unwrap();
        let verifier =
            libsubatomic::rpm::signature::pgp::Verifier::from_asc(&mgr.public_armor().unwrap())
                .unwrap();
        rpmmeta.verify_signature(verifier).unwrap();
    }

    #[sqlx::test(fixtures("keys", "repos"))]
    async fn repo_key_management(pool: Pool) {
        let states = app(pool);
        let States { app, .. } = states;

        let req = Request::get("/v1/repos/rpmfission/key")
            .header("Authorization", AUTH)
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 200);

        let req = Request::put("/v1/repos/rpmfission/key")
            .header("Authorization", AUTH)
            .header("Content-Type", "application/json")
            .body(Body::from(serde_json::json!({ "id": "key2" }).to_string()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 204);

        let req = Request::get("/v1/repos/rpmfission/key")
            .header("Authorization", AUTH)
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 200);
        let pubarmor = resp.into_body().collect().await.unwrap().to_bytes();
        assert!(pubarmor.starts_with(b"-----BEGIN PGP PUBLIC KEY BLOCK-----"));

        let req = Request::delete("/v1/repos/rpmfission/key")
            .header("Authorization", AUTH)
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 204);

        let req = Request::get("/v1/repos/rpmfission/key")
            .header("Authorization", AUTH)
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 404);
    }

    #[sqlx::test(fixtures("keys", "repos"))]
    async fn custom_metadata(pool: Pool) {
        let states = app(pool);
        let States { app, cfg, .. } = states;

        // Upload custom metadata
        let mut form = MultipartForm::default();
        form.add_reader_2(
            "my-custom.xml",
            std::io::Cursor::new("<custom>data</custom>"),
            Some("my-custom.xml".into()),
            None,
            vec![],
        );
        let req = Request::put("/v1/repos/rpmfission/md/mytype")
            .header("Authorization", AUTH)
            .header(axum::http::header::CONTENT_TYPE, form.content_type().as_str())
            .body(Body::from_stream(MultipartBody::from(form)))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        assert_eq!(status, 204);

        // Verify repomd.xml contains the new data type
        let repomd_path = cfg.storage_dir.join("rpmfission/repodata/repomd.xml");
        let content = std::fs::read_to_string(&repomd_path).unwrap();
        assert!(content.contains(r#"<data type="mytype">"#));

        // Delete custom metadata
        let req = Request::delete("/v1/repos/rpmfission/md/mytype")
            .header("Authorization", AUTH)
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 204);

        // Verify removed
        let content = std::fs::read_to_string(repomd_path).unwrap();
        assert!(!content.contains(r#"<data type="mytype">"#));
    }

    #[sqlx::test(fixtures("keys", "repos"))]
    async fn unknown_repo(pool: Pool) {
        let states = app(pool);
        let States { app, .. } = states;

        let req = Request::get("/v1/repos/rpmcone/rpms")
            .header("Authorization", AUTH)
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 404);
        // This endpoint doesn't exist (only list_repos). But for refresh, delete, etc.:
        let req = Request::post("/v1/repos/nosuch/refresh")
            .header("Authorization", AUTH)
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 404);
    }

    #[sqlx::test]
    async fn invalid_jwt(pool: Pool) {
        let states = app(pool);
        let States { app, .. } = states;
        let req = Request::get("/v1/repos")
            .header("Authorization", "Bearer invalid")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 401);
    }
}
