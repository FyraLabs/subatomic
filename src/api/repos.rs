#![allow(clippy::missing_errors_doc)]
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::Arc;

use crate::db::{Key, Repo as DbRepo};
use crate::error::{ApiError, Result};
use crate::{DbState, LockerState};
use axum::Json;
use axum::extract::{Multipart, Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use futures_util::{StreamExt, TryStreamExt};
use libsubatomic::metan_prelude::*;
use libsubatomic::prelude::Itertools;
use libsubatomic::repo::{FragRequest, hierarchy::Hierarchize};
use rayon::prelude::*;
use tokio::io::AsyncWriteExt;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use tokio_util::io::StreamReader;

pub async fn list_repos(State(pool): DbState) -> Result<Json<Vec<DbRepo>>> {
    Ok(Json(sqlx::query_as!(DbRepo, "SELECT * FROM repos ORDER BY name").fetch_all(&*pool).await?))
}

pub async fn create_repo(State(pool): DbState, Path(name): Path<String>) -> Result<Json<DbRepo>> {
    Ok(Json(
        sqlx::query_as!(DbRepo, "INSERT INTO repos (name) VALUES ($1) RETURNING *", &name)
            .fetch_one(&*pool)
            .await?,
    ))
}

pub async fn sign_headers(
    State(locker): LockerState,
    Path(repo): Path<String>,
    mut multipart: Multipart,
) -> Result<Response> {
    let sig =
        locker.read(&repo, async |hdl| hdl.repo.sig.clone()).await?.ok_or(ApiError::NotFound)?;
    let Some(mgr) = sig.as_ref() else {
        return Ok(StatusCode::NO_CONTENT.into_response());
    };
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
        resp.add_reader_2("", std::io::Cursor::new(sig), None, None, vec![]);
    }

    let content_type = resp.content_type();
    let body = axum::body::Body::from_stream(
        rust_multipart_rfc7578_2::client::multipart::Body::from(resp),
    );
    Ok((StatusCode::OK, [(header::CONTENT_TYPE, content_type)], body).into_response())
}

pub async fn upload_pkgs(
    State(locker): LockerState,
    Path(repo): Path<String>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>> {
    let r = locker.read(&repo, async |hdl| {
        let cfg = &hdl.repo.cache.cfg;
        (cfg.cache_dir.clone(), hdl.repo.cache.keys(), cfg.hier.clone(), Arc::clone(&cfg.store))
    });
    let (cache_dir, keys, hier, store) = r.await?.ok_or(ApiError::NotFound)?;
    let keys = keys.map_err(|e| ApiError::Internal(format!("can't get cache keys: {e}")))?;

    let tempdir = tempfile::Builder::new().prefix("upload-").tempdir_in(&cache_dir);
    let tempdir =
        tempdir.map_err(|e| ApiError::Internal(format!("cannot create upload tempdir: {e}")))?;

    let parsed_keys: Vec<_> =
        keys.iter().filter_map(|k| Some((k.clone(), kuchiyose::rpm::parse_filename(k)?))).collect();

    let mut processor =
        UploadProcessor { dir: tempdir.path().to_path_buf(), hier, store, parsed_keys, .. };
    processor.receive_rpms(&mut multipart).await?;
    let UploadProcessor { removed, received, .. } = processor;

    let frags: Vec<(PathBuf, Vec<MetanComputed>)> = {
        let staged = received.clone();
        let r = locker.read(&repo, async move |hdl| {
            staged
                .into_par_iter()
                .map(|ReceiveRpmOut { csum, path }| {
                    let mut input = libsubatomic::cache::ComputeInput::default();
                    input.csum = Some(csum.into());
                    let computed = hdl.repo.cache.compute(&path, input)?;
                    Ok::<_, libsubatomic::Error>((path, computed))
                })
                .collect::<Result<Vec<_>, _>>()
        });
        r.await?.ok_or(ApiError::NotFound)??
    };

    let removed_out = removed.clone();
    let w = locker.write(&repo, async move |hdl| {
        if !removed.is_empty() {
            let not_found = hdl.repo.del(&removed).await?;
            if !not_found.is_empty() {
                tracing::error!(?not_found, "superseded packages missing from cache");
            }
        }

        let cache = &hdl.repo.cache;
        std::thread::scope(|s| {
            let (tx, rx) = crossbeam_channel::bounded(num_cpus::get() * 20);
            let h = s.spawn(move || cache.update_frags(&rx));
            for (p, computed) in frags {
                tx.send((p, FragRequest::Put(computed))).expect("writer thread died");
            }
            drop(tx);
            h.join().expect("writer thread panicked")
        })?;
        hdl.repo.generate().await?;
        Ok::<_, libsubatomic::Error>(())
    });
    w.await?.ok_or(ApiError::NotFound)??;

    drop(tempdir);

    Ok(Json(serde_json::json!({
        "removed": removed_out
            .iter()
            .map(|bs| String::from_utf8_lossy(bs).to_string())
            .collect_vec(),
    })))
}

struct UploadProcessor<'k, H: Hierarchize> {
    dir: PathBuf,
    hier: H,
    store: Arc<kuchiyose::store::StoreBackend>,
    parsed_keys: Vec<(Vec<u8>, kuchiyose::rpm::ParsePathOutput<'k>)>,
    removed: Vec<Vec<u8>> = Vec::new(),
    received: Vec<ReceiveRpmOut> = Vec::new(),
}

#[derive(Clone)]
struct ReceiveRpmOut {
    csum: String,
    path: PathBuf,
}

impl<H: Hierarchize> UploadProcessor<'_, H> {
    async fn receive_rpms(&mut self, multipart: &mut Multipart) -> Result<()> {
        while let Some(field) =
            multipart.next_field().await.map_err(|e| ApiError::Internal(e.to_string()))?
        {
            let received = self.receive_rpm(field).await?;
            let ReceiveRpmOut { csum, .. } = &received;
            self.check_csum(multipart, csum).await?;
            self.received.push(received);
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
    async fn receive_rpm(
        &mut self,
        field: axum::extract::multipart::Field<'_>,
    ) -> Result<ReceiveRpmOut> {
        let name = field
            .file_name()
            .ok_or_else(|| ApiError::BadRequest("filename should not be empty".into()))?
            .to_owned();
        if name.is_empty()
            || name.contains('/')
            || name.contains('\\')
            || name.contains("..")
            || !name.ends_with(".rpm")
        {
            return Err(ApiError::BadRequest("invalid rpm filename".to_owned()));
        }
        let path = self.dir.join(&name);

        let link = self
            .hier
            .locate_relative(name.as_str())
            .ok_or_else(|| ApiError::BadRequest("hierarchy rejected filename".into()))?;
        let link = self.hier.basedir().join(&link);

        let body_reader = std::pin::pin!(StreamReader::new(field.map_err(std::io::Error::other)));
        let body_reader = body_reader.compat();

        let csum = try {
            let store_fd = self.store.writer(&link).await.map_err(std::io::Error::other)?;
            let fd = tokio::fs::File::create(&path).await?;
            let fd = tokio::io::BufWriter::new(fd);
            let writer: MochiWriter =
                kuchiyose::comp::Mochi::new(fd, kuchiyose::ftmm::Ftmm::Sha256);
            let (mut store_fd, mut writer) = match double_write(body_reader, store_fd, writer).await
            {
                Ok(res) => res,
                Err(err) => return Err(err),
            };
            store_fd.shutdown().await?;
            writer.shutdown().await?;
            let writer =
                writer.as_any_mut().downcast_mut::<MochiWriter>().expect("can't downcast mochi");
            let dummy = kuchiyose::FtmmDigest::Sha512(Default::default());
            hex::encode(std::mem::replace(&mut writer.ftmm, dummy).finalize())
        }
        .map_err(|e| ApiError::Internal(format!("cannot process uploads: {e}")))?;

        let filename = path.file_name().expect("expected file").as_bytes();
        let Some(kuchiyose::rpm::ParsePathOutput { name, arch, .. }) =
            kuchiyose::rpm::parse_filename(filename)
        else {
            return Err(ApiError::BadRequest("invalid rpm filename format".to_owned()));
        };
        let prev_versions = self
            .parsed_keys
            .iter()
            .filter(|(_, k)| k.name == name && k.arch == arch)
            .filter(|(k, _)| k.as_slice() != filename);
        self.removed.extend(prev_versions.map(|(k, _)| k.clone()));
        Ok(ReceiveRpmOut { csum, path })
    }
}

type MochiWriter = kuchiyose::comp::Mochi<tokio::io::BufWriter<tokio::fs::File>>;

async fn double_write(
    mut body_reader: impl futures_util::AsyncRead + Unpin + Send,
    store_fd: Box<dyn kuchiyose::store::StoreWrite>,
    writer: MochiWriter,
) -> Result<(Box<dyn kuchiyose::store::StoreWrite>, Box<dyn kuchiyose::store::StoreWrite>)> {
    let mut multi_writer = srmw::MultiWriter::default();
    multi_writer.insert(store_fd.compat_write());
    multi_writer.insert((Box::new(writer) as Box<dyn kuchiyose::store::StoreWrite>).compat_write());
    let buf = &mut [0u8; 64 * 1024];
    let mut generator = multi_writer.copy(&mut body_reader, buf);
    while let Some(event) = generator.next().await {
        match event {
            srmw::CopyEvent::Progress(_) => {}
            srmw::CopyEvent::Failure(0, err) => {
                tracing::error!(?err, "cannot write to store");
                return Err(ApiError::Internal(format!("cannot write to store: {err}")));
            }
            srmw::CopyEvent::Failure(1, err) => {
                tracing::error!(?err, "cannot write to tmpdir");
                return Err(ApiError::Internal(format!("cannot write to tmpdir: {err}")));
            }
            srmw::CopyEvent::Failure(i, _) => unreachable!("unknown writer idx {i}"),
            srmw::CopyEvent::NoWriters => unreachable!("no writers"),
            srmw::CopyEvent::SourceFailure(error) => {
                tracing::warn!(?error, "source failure");
                return Err(ApiError::BadRequest(format!("multipart: {error}")));
            }
        }
    }
    drop(generator);
    let left = multi_writer.remove(0).into_inner();
    let right = multi_writer.remove(1).into_inner();
    Ok((left, right))
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
    let field = (multipart.next_field().await)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::BadRequest("expect multipart (file upload)".to_owned()))?;
    let comps = (field.bytes().await)
        .map_err(|e| ApiError::BadRequest(format!("cannot get file bytes: {e}")))?;

    let w = locker.write(&repo, async move |hdl| {
        hdl.repo.write_custom("group", "comps.xml", &comps[..]).await?;
        hdl.repo.generate().await?;
        Ok::<_, libsubatomic::Error>(())
    });
    if w.await?.transpose()?.is_some() {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Ok(StatusCode::NOT_FOUND)
    }
}

pub async fn del_comps(State(locker): LockerState, Path(repo): Path<String>) -> Result<StatusCode> {
    let w = locker.write(&repo, async |hdl| {
        hdl.repo.del_custom("group").await?;
        hdl.repo.generate().await?;
        Ok::<_, libsubatomic::Error>(())
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
    let mgr = libsubatomic::sig::Mgr::from_armor(&key.pri).map_err(libsubatomic::Error::from)?;

    let w = locker.write(&repo, async |mut hdl| try {
        let q = sqlx::query!("UPDATE repos SET key_id = $1 WHERE name = $2", key.id, &repo);
        let ra = q.execute(&*db).await?.rows_affected();
        if ra == 0 {
            return Ok::<_, sqlx::Error>(StatusCode::NOT_FOUND);
        }
        hdl.repo.sig = Some(mgr);
        StatusCode::NO_CONTENT
    });
    match w.await?.ok_or(ApiError::NotFound)? {
        Ok(s) => Ok(s),
        Err(e) => Err(e.into()),
    }
}

pub async fn del_key(
    State(db): DbState,
    State(locker): LockerState,
    Path(repo): Path<String>,
) -> Result<StatusCode> {
    let w = locker.write(&repo, async |mut hdl| try {
        let q = sqlx::query!("UPDATE repos SET key_id = NULL WHERE name = $1", &repo);
        let ra = q.execute(&*db).await?.rows_affected();
        if ra == 0 {
            return Ok::<_, sqlx::Error>(false);
        }
        hdl.repo.sig = None;
        true
    });
    let Some(true) = w.await?.map(|r| r.map_err(ApiError::from)).transpose()? else {
        return Ok(StatusCode::NOT_FOUND);
    };
    Ok(StatusCode::NO_CONTENT)
}

pub async fn refresh_repo(
    State(locker): LockerState,
    Path(name): Path<String>,
) -> Result<StatusCode> {
    let q = locker.read(&name, async |repohdl| repohdl.repo.regenerate(true).await).await?;
    Ok(if q.transpose()?.is_some() { StatusCode::NO_CONTENT } else { StatusCode::NOT_FOUND })
}

pub async fn rebuild_repo(
    State(locker): LockerState,
    Path(name): Path<String>,
) -> Result<StatusCode> {
    let q = locker.read(&name, async |repohdl| repohdl.repo.regenerate(false).await).await?;
    Ok(if q.transpose()?.is_some() { StatusCode::NO_CONTENT } else { StatusCode::NOT_FOUND })
}

pub async fn list_rpms(
    State(locker): LockerState,
    Path(repo): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let Some(keys) = locker
        .read(&repo, async |repohdl| repohdl.repo.cache.keys())
        .await?
        .transpose()
        .map_err(|e| ApiError::Internal(format!("cannot list keys: {e}")))?
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
    let rpms_bytes: Vec<Vec<u8>> = rpms.iter().map(|s| s.as_bytes().to_vec()).collect();
    let w = locker.write(&repo, async move |repohdl| {
        let out = repohdl.repo.del(&rpms_bytes).await?;
        repohdl.repo.generate().await?;
        Ok::<_, libsubatomic::Error>(out)
    });
    let Some(not_found) = w.await?.transpose()? else {
        return Err(ApiError::NotFound);
    };
    let not_found: Vec<String> =
        not_found.iter().map(|s| String::from_utf8_lossy(s).to_string()).collect();
    Ok(Json(serde_json::json!({ "not_found": not_found })))
}

pub async fn upl_md(
    State(locker): LockerState,
    Path((repo, md)): Path<(String, String)>,
    mut multipart: Multipart,
) -> Result<StatusCode> {
    let field = (multipart.next_field().await)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::BadRequest("expect multipart (file upload)".to_owned()))?;
    let filename = field
        .file_name()
        .ok_or_else(|| ApiError::BadRequest("expected filename".into()))?
        .to_owned();
    let content = (field.bytes().await)
        .map_err(|e| ApiError::BadRequest(format!("cannot get file bytes: {e}")))?;

    let w = locker.write(&repo, async move |hdl| {
        hdl.repo.write_custom(&md, &filename, &content[..]).await?;
        hdl.repo.generate().await?;
        Ok::<_, libsubatomic::Error>(())
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
    let w = locker.write(&repo, async move |hdl| {
        hdl.repo.del_custom(&md).await?;
        hdl.repo.generate().await?;
        Ok::<_, libsubatomic::Error>(())
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
                server_host: String::new(),
                server_port: 0,
                database_url: String::new(),
                db_max_conns: 32,
                jwt_secret: "cad4a3a28cfdb1a464e26e5851e6cd44a95fd8c57c117d294a9e8391e70274d2"
                    .into(),
                storage_dir: storage_dir.path().to_owned(),
                cache_dir: cache_dir.path().to_owned(),
                body_limit: 10_485_760,
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
        form.add_text("", CSUM);
        let req = Request::post("/v1/repos/rpmfission")
            .header("Authorization", AUTH)
            .header(axum::http::header::CONTENT_TYPE, form.content_type().as_str())
            .body(Body::from_stream(MultipartBody::from(form)))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        println!("{body}");
        assert!(body.get("removed").unwrap().as_array().unwrap().is_empty());
        let old = cfg.storage_dir.join("rpmfission/terra-release-44-4.noarch.rpm");
        assert!(std::fs::exists(&old).unwrap());

        let mut form = MultipartForm::default();
        form.add_reader_2(
            "terra-release-44-5.noarch.rpm",
            &include_bytes!("../../random-rpm-examples/terra-release-44-4.noarch.rpm")[..],
            Some("terra-release-44-5.noarch.rpm".into()),
            None,
            vec![],
        );
        form.add_text("", CSUM);
        let req = Request::post("/v1/repos/rpmfission")
            .header("Authorization", AUTH)
            .header(axum::http::header::CONTENT_TYPE, form.content_type().as_str())
            .body(Body::from_stream(MultipartBody::from(form)))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        println!("{body}");
        let removed = body.get("removed").unwrap().as_array().unwrap();
        assert_eq!(removed.len(), 1);
        assert_eq!(removed.first().unwrap().as_str().unwrap(), "terra-release-44-4.noarch.rpm");
        let new = cfg.storage_dir.join("rpmfission/terra-release-44-5.noarch.rpm");
        assert!(!old.exists());
        assert!(new.exists());

        let ret = super::list_rpms(locker.clone(), Path("rpmfission".into())).await.unwrap().0;
        let rpms = ret.as_array().unwrap();
        assert_eq!(rpms.len(), 1);
        assert_eq!(rpms.first().unwrap().as_str().unwrap(), "terra-release-44-5.noarch.rpm");

        let rpms = vec!["terra-release-44-5.noarch.rpm".into()];
        let ret =
            super::del_rpms(locker, Path("rpmfission".into()), Json(super::DelRpmsReq { rpms }));
        let ret = ret.await.unwrap().0;
        println!("{ret:?}");
        assert!(!new.exists());
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
        assert_eq!(resp.status(), 204);

        let repomd_path = cfg.storage_dir.join("rpmfission/repodata/repomd.xml");
        let content = std::fs::read_to_string(&repomd_path).unwrap();
        assert!(content.contains(r#"<data type="mytype">"#));

        let req = Request::delete("/v1/repos/rpmfission/md/mytype")
            .header("Authorization", AUTH)
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 204);

        let content = std::fs::read_to_string(repomd_path).unwrap();
        assert!(!content.contains(r#"<data type="mytype">"#));
    }

    #[sqlx::test(fixtures("keys", "repos"))]
    async fn del_repos(pool: Pool) {
        let states = app(pool);
        let States { cfg, locker, .. } = states;
        std::fs::create_dir_all(cfg.storage_dir.join("rpmball")).unwrap();
        assert_eq!(
            super::delete_repo(locker.clone(), Path("rpmball".into())).await.unwrap(),
            axum::http::StatusCode::NO_CONTENT
        );
        assert!(!cfg.storage_dir.join("rpmball").exists());
        assert!(locker.read("rpmball", async |_| unreachable!()).await.unwrap().is_none());
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
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 401);
        let req = Request::get("/v1/repos").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 401);
    }
}
