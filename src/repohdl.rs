use std::{collections::HashMap, sync::Arc};

use libsubatomic::prelude::*;
use libsubatomic::repodata::RepoCache;
use tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::{config::Config, error::Result};

pub struct Locker {
    repolocks: RwLock<HashMap<String, RwLock<RepoHdl>>>,
    db: Arc<sqlx::Pool<sqlx::Postgres>>,
    cfg: Arc<Config>,
}

impl Locker {
    pub fn new(db: Arc<sqlx::Pool<sqlx::Postgres>>, cfg: Arc<Config>) -> Self {
        Self { repolocks: RwLock::new(HashMap::new()), db, cfg }
    }
    #[tracing::instrument(skip(self, f))]
    pub async fn read<T, F>(&self, repo: &str, mut f: F) -> Result<Option<T>>
    where
        F: AsyncFnMut(RwLockReadGuard<'_, RepoHdl>) -> T,
    {
        if let Some(lock) = self.repolocks.read().await.get(repo) {
            return Ok(Some(f(lock.read().await).await));
        }
        tracing::debug!(repo, "cache miss");
        let Some(repohdl) = RepoHdl::new(&self.db, &self.cfg, repo).await? else { return Ok(None) };
        self.repolocks.write().await.insert(repo.into(), RwLock::new(repohdl));
        Ok(Some(f(self.repolocks.read().await.get(repo).unwrap().read().await).await))
    }
    #[tracing::instrument(skip(self, f))]
    pub async fn write<T, F>(&self, repo: &str, mut f: F) -> Result<Option<T>>
    where
        F: AsyncFnMut(RwLockWriteGuard<'_, RepoHdl>) -> T,
    {
        if let Some(lock) = self.repolocks.read().await.get(repo) {
            return Ok(Some(f(lock.write().await).await));
        }
        tracing::debug!(repo, "cache miss");
        let Some(repohdl) = RepoHdl::new(&self.db, &self.cfg, repo).await? else { return Ok(None) };
        self.repolocks.write().await.insert(repo.into(), RwLock::new(repohdl));
        Ok(Some(f(self.repolocks.read().await.get(repo).unwrap().write().await).await))
    }
    #[tracing::instrument(skip(self))]
    pub async fn del(&self, repo: &str) -> Result<bool> {
        let rows_affected = sqlx::query("DELETE FROM repos WHERE name = $1")
            .bind(repo)
            .execute(&*self.db)
            .await?
            .rows_affected();
        if rows_affected == 0 {
            return Ok(false);
        }

        let Some(hdl) = self.repolocks.write().await.remove(repo) else {
            return Ok(true);
        };
        hdl.write().await.delete_physical(Arc::clone(&self.cfg)).await?;
        Ok(true)
    }
}

/// Thin wrapper around [`libsubatomic::Repo`].
pub struct RepoHdl {
    pub repo: libsubatomic::Repo,
}

impl RepoHdl {
    async fn new(pool: &sqlx::PgPool, config: &Config, repo_name: &str) -> Result<Option<Self>> {
        let Some(repo) =
            sqlx::query_as::<_, crate::db::Repo>("SELECT * FROM repos WHERE name = $1")
                .bind(repo_name)
                .fetch_optional(pool)
                .await?
        else {
            return Ok(None);
        };

        let cache = RepoCache::new(repo_name, &config.cache_dir, &config.storage_dir)
            .map_err(libsubatomic::err::Error::from)?;

        let sig = if let Some(key_id) = repo.key_id {
            let key = sqlx::query_as::<_, crate::db::Key>("SELECT * FROM keys WHERE id = $1")
                .bind(key_id)
                .fetch_one(pool)
                .await?;
            Some(libsubatomic::sig::Mgr::parse(&key.pri).map_err(libsubatomic::err::Error::from)?)
        } else {
            None
        };

        let repo = libsubatomic::Repo { cache, sig, use_appstream: true };

        Ok(Some(Self { repo }))
    }

    pub async fn delete_physical(&self, config: Arc<Config>) -> Result<()> {
        let path = std::path::Path::new(&config.storage_dir).join(&*self.repo.cache.repo);
        if path.exists() {
            std::fs::remove_dir_all(path)?;
        }
        Ok(())
    }
}
