use std::{collections::HashMap, sync::Arc};

use kuchiyose::ftmm::Ftmm;
use kuchiyose::store::StoreBackend;
use libsubatomic::metan_prelude::*;
use libsubatomic::repo::hierarchy::{Hierarchize, Hierarchy};
use libsubatomic::{Cache, CacheConfig};
use tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::validate::repo_name;
use crate::{config::Config, error::Result};

pub type Repo = libsubatomic::Repo<Hierarchy>;

pub struct Locker {
    repolocks: RwLock<HashMap<String, RwLock<RepoHdl>>>,
    db: Arc<sqlx::Pool<sqlx::Postgres>>,
    cfg: Arc<Config>,
}

impl Locker {
    #[must_use]
    pub fn new(db: Arc<sqlx::Pool<sqlx::Postgres>>, cfg: Arc<Config>) -> Self {
        Self { repolocks: RwLock::new(HashMap::new()), db, cfg }
    }
    #[tracing::instrument(skip(self, f))]
    pub async fn read<T, F>(&self, repo: &str, f: F) -> Result<Option<T>>
    where
        F: AsyncFnOnce(RwLockReadGuard<'_, RepoHdl>) -> T,
    {
        repo_name(repo)?;
        if let Some(lock) = self.repolocks.read().await.get(repo) {
            return Ok(Some(f(lock.read().await).await));
        }
        tracing::debug!(repo, "cache miss");
        let Some(repohdl) = RepoHdl::new(&self.db, &self.cfg, repo).await? else { return Ok(None) };
        self.repolocks.write().await.insert(repo.into(), RwLock::new(repohdl));
        Ok(Some(f(self.repolocks.read().await.get(repo).unwrap().read().await).await))
    }
    #[tracing::instrument(skip(self, f))]
    pub async fn write<T, F>(&self, repo: &str, f: F) -> Result<Option<T>>
    where
        F: AsyncFnOnce(RwLockWriteGuard<'_, RepoHdl>) -> T,
    {
        repo_name(repo)?;
        if let Some(lock) = self.repolocks.read().await.get(repo) {
            return Ok(Some(f(lock.write().await).await));
        }
        tracing::debug!(repo, "cache miss");
        let Some(repohdl) = RepoHdl::new(&self.db, &self.cfg, repo).await? else { return Ok(None) };
        self.repolocks.write().await.insert(repo.into(), RwLock::new(repohdl));
        let ret = f(self.repolocks.read().await.get(repo).unwrap().write().await).await;
        // TODO: handle error properly
        let mut w = self.repolocks.write().await;
        let (_, repohdl) = w.remove_entry(repo).unwrap();
        drop(w);
        let repohdl = repohdl.into_inner();
        repohdl.repo.cache.compact_close()?;
        Ok(Some(ret))
    }
    #[tracing::instrument(skip(self))]
    pub async fn del(&self, repo: &str) -> Result<bool> {
        repo_name(repo)?;
        let hdl = self.repolocks.write().await.remove(repo);
        let hdl = if let Some(hdl) = hdl {
            hdl.into_inner()
        } else if let Some(hdl) = RepoHdl::new(&self.db, &self.cfg, repo).await? {
            hdl
        } else {
            return Ok(false);
        };
        sqlx::query("DELETE FROM repos WHERE name = $1").bind(repo).execute(&*self.db).await?;

        hdl.delete_physical(Arc::clone(&self.cfg)).await?;
        Ok(true)
    }
}

/// Thin wrapper around [`libsubatomic::Repo`].
pub struct RepoHdl {
    pub repo: Repo,
}

impl RepoHdl {
    async fn new(pool: &sqlx::PgPool, config: &Config, repo_name: &str) -> Result<Option<Self>> {
        crate::validate::repo_name(repo_name)?;
        let Some(repo) =
            sqlx::query_as!(crate::db::Repo, "SELECT * FROM repos WHERE name = $1", repo_name)
                .fetch_optional(pool)
                .await?
        else {
            return Ok(None);
        };
        let mut hier = config.hierarchy.clone();
        match &mut hier {
            Hierarchy::Satm0Flat(inner) => inner.base = inner.base.join(repo_name),
            Hierarchy::Fedora(inner) => inner.base = inner.base.join(repo_name),
            _ => unimplemented!(),
        }

        let cfg = CacheConfig {
            repo: repo_name.into(),
            cache_dir: config.cache_dir.clone(),
            hier,
            store: Arc::new(StoreBackend::Local),
            ftmm: Ftmm::Sha256,
            ..
        };

        let metans: Metans = vec![
            Arc::new(PrimaryMetan::default()) as Arc<dyn Metan>,
            Arc::new(FilelistsMetan::default()),
            Arc::new(OtherMetan::default()),
            // TODO: unhardcode?
            {
                let mut m = AppstreamMetan::default();
                m.repo = repo_name.into();
                Arc::new(m)
            },
        ];

        let cache = Cache::new(cfg, metans).map_err(libsubatomic::Error::from)?;

        let sig = if let Some(key_id) = repo.key_id {
            let q = sqlx::query_as!(crate::db::Key, "SELECT * FROM keys WHERE id = $1", key_id);
            let key = q.fetch_one(pool).await?;
            Some(libsubatomic::sig::Mgr::from_armor(&key.pri).map_err(libsubatomic::Error::from)?)
        } else {
            None
        };

        let repo = libsubatomic::Repo {
            tempdir: None,
            cache,
            sig,
            comp_cfg: kuchiyose::comp::CompConfig::default(),
            ..
        };

        Ok(Some(Self { repo }))
    }

    pub async fn delete_physical(&self, config: Arc<Config>) -> Result<()> {
        let link: kuchiyose::LinkBuf = config.hierarchy.basedir().join(&*self.repo.cache.cfg.repo);
        if link.as_path().exists() {
            tokio::fs::remove_dir_all(link.as_path()).await?;
        }
        Ok(())
    }
}
