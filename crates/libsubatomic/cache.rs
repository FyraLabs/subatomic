//! Cache for repository package metadata and XML fragment storage.
//!
//! [`Cache`] owns the LMDB environment and holds a list of [`Metan`] trait objects, one per
//! repomd datatype (primary, filelists, other, appstream, …). Each `Metan` owns its own LMDB
//! database and knows how to serialize packages into XML fragments, generate the final XML,
//! and emit the corresponding `<data>` entry in `repomd.xml`.

use crate::metan_prelude::*;
use crate::pkg::MetanInput;
use crate::prelude::*;
use crate::repo::FragRequest;
use crate::repo::hierarchy::Hierarchize;
use crate::repodata::{Metan, MetanGeneration, MetanReady, repomd};
use kuchiyose::comp::{CompConfig, Mochi};
use kuchiyose::ftmm::Ftmm;
use kuchiyose::store::StoreBackend;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) type DataDb = heed::Database<heed::types::Str, heed::types::SerdeBincode<repomd::Data>>;
pub(crate) type FragDb = heed::Database<heed::types::Bytes, heed::types::Bytes>;
pub(crate) type MarkDb =
    heed::Database<heed::types::Bytes, heed::types::U128<heed::byteorder::NativeEndian>>;

pub const DEFAULT_MAP_SIZE: usize = 10 * 1024 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct CacheConfig<H: Hierarchize> {
    /// Identifier for the repository.
    pub repo: String,
    /// Persistent directory for the LMDB file.
    pub cache_dir: PathBuf,
    /// Filesystem/object-store hierarchy for RPMs and metadata.
    pub hier: H,
    /// Where metadata files are written.
    pub store: Arc<StoreBackend>,
    /// LMDB virtual address-space reservation.
    pub lmdb_map_size: usize = DEFAULT_MAP_SIZE,
    /// Preferred checksum algorithm for all generated metadata.
    pub ftmm: Ftmm,
}

/// Repository metadata cache.
#[non_exhaustive]
#[derive(Debug)]
pub struct Cache<H: Hierarchize + Sync> {
    pub cfg: CacheConfig<H>,
    /// The LMDB environment shared by all datatypes.
    pub env: Arc<heed::Env<heed::WithoutTls>>,
    /// Package filename → last-seen unix micros. Drives incremental pruning.
    pub epo: MarkDb,
    /// User-supplied custom datatypes keyed by [`repomd::DataType::as_type`].
    pub cus: DataDb,
    /// All datatypes this repo generates.
    metans: Metans,
}

impl<H: Hierarchize + Sync> Cache<H> {
    /// Open a cache at `<cache_dir>/<repo>`, initializing every metan's LMDB database.
    ///
    /// # Errors
    /// Propagates LMDB open/create errors.
    pub fn new(cfg: CacheConfig<H>, metans: Metans) -> heed::Result<Self> {
        tracing::debug!(?cfg, "opening cache");
        let path = cfg.cache_dir.join(&*cfg.repo);

        if path.is_file() {
            tracing::warn!(path = %path.display(), "removing stale cache file");
            std::fs::remove_file(&path)?;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::create_dir_all(&path)?;

        let max_dbs = metans.iter().map(|m| m.db_count()).sum::<u32>() + 2;
        // SAFETY: assume this file is not modified concurrently
        let env = unsafe {
            heed::EnvOpenOptions::new()
                .read_txn_without_tls()
                .max_dbs(max_dbs) // epo & cus
                .map_size(cfg.lmdb_map_size)
                .flags(heed::EnvFlags::WRITE_MAP | heed::EnvFlags::NO_SYNC)
                .open(path)?
        };
        let env = Arc::new(env);

        let mut txn = env.write_txn()?;
        let epo = env.create_database(&mut txn, Some("epo"))?;
        let cus = env.create_database(&mut txn, Some("cus"))?;
        for metan in &metans {
            metan.db_init(Arc::clone(&env), &mut txn)?;
        }
        txn.commit()?;

        Ok(Self { cfg, env, epo, cus, metans })
    }

    #[must_use]
    pub fn metans(&self) -> &[Arc<dyn Metan>] {
        &self.metans
    }

    /// Return the number of packages currently cached.
    ///
    /// # Errors
    /// Propagates LMDB errors.
    pub fn len(&self) -> heed::Result<u64> {
        let txn = self.env.read_txn()?;
        self.epo.len(&txn)
    }

    /// # Errors
    /// Propagates LMDB errors.
    pub fn is_empty(&self) -> heed::Result<bool> {
        Ok(self.len()? == 0)
    }

    /// # Errors
    /// Propagates LMDB errors.
    pub fn has(&self, key: &[u8]) -> Res<bool> {
        let txn = self.env.read_txn()?;
        Ok(self.epo.get(&txn, key)?.is_some())
    }

    /// Collect every cached package filename.
    ///
    /// # Errors
    /// Propagates LMDB errors.
    pub fn keys(&self) -> heed::Result<Vec<Vec<u8>>> {
        let txn = self.env.read_txn()?;
        let mut out = Vec::new();
        for res in self.epo.iter(&txn)? {
            let (k, _) = res?;
            out.push(k.to_owned());
        }
        Ok(out)
    }

    /// Check whether the RPM at `abs_path` is cached, using the hierarchy to
    /// derive its repository-relative key.
    ///
    /// # Errors
    /// Propagates LMDB errors.
    pub fn has_rpm(&self, abs_path: &Path) -> Res<bool> {
        let Some(filename) = abs_path.file_name() else { return Ok(false) };
        let Some(rel) = self.cfg.hier.locate_relative(filename) else { return Ok(false) };
        self.has(rel.as_bytes())
    }

    /// Consume `recv` until the channel closes, saving every package it receives.
    ///
    /// Each package is fanned out to every registered [`Metan`]. After the channel drains, any
    /// package whose epoch is stale (not seen this round) is purged from every metan.
    ///
    /// Returns `(new, cached)` — the number of parsed packages and the number of cache hits.
    ///
    /// # Errors
    /// Propagates any error from [`Metan::save`], [`Metan::del`], or LMDB.
    ///
    /// # Panics
    /// Panics on the (essentially impossible) system-time underflow.
    pub fn update_frags(
        &self,
        recv: &crossbeam_channel::Receiver<(PathBuf, FragRequest)>,
    ) -> Res<(u64, u64)> {
        let epoch =
            SystemTime::now().duration_since(UNIX_EPOCH).expect("time underflow").as_micros();
        let mut wtxn = self.env.write_txn()?;
        let mut new: u64 = 0;
        let mut cached: u64 = 0;

        while let Ok((abs_path, req)) = recv.recv() {
            let Some(filename) = abs_path.file_name() else {
                tracing::warn!(p = %abs_path.display(), "path has no filename; skipping");
                continue;
            };
            let Some(rel) = self.cfg.hier.locate_relative(filename) else {
                tracing::warn!(p = %abs_path.display(), "hierarchy cannot locate file; skipping");
                continue;
            };

            self.epo.put(&mut wtxn, rel.as_bytes(), &epoch)?;

            match req {
                FragRequest::Cached => cached += 1,
                FragRequest::Parse { csum } => {
                    new += 1;
                    let input = MetanInput::from_path(&abs_path, rel, csum)?;
                    for metan in &self.metans {
                        metan.save(&mut wtxn, &input)?;
                    }
                }
            }
        }

        // purge stale keys
        let mut it = self.epo.iter_mut(&mut wtxn)?;
        let mut purged: Vec<Vec<u8>> = Vec::new();
        while let Some(res) = it.next() {
            let (k, v) = res?;
            if v != epoch {
                purged.push(k.to_owned());
                // SAFETY: we do not keep any references to any values from this db
                assert!(unsafe { it.del_current()? }, "cannot delete item");
            }
        }
        drop(it);
        for metan in &self.metans {
            for k in &purged {
                metan.del(&mut wtxn, k)?;
            }
        }
        wtxn.commit()?;
        Ok((new, cached))
    }

    /// Delete the given packages from every metan database.
    ///
    /// Returns the subset of `pkgs` that were not present.
    ///
    /// # Errors
    /// Propagates LMDB errors.
    pub fn delete_pkgs<'a>(&self, pkgs: &[&'a [u8]]) -> heed::Result<Vec<&'a [u8]>> {
        let mut wtxn = self.env.write_txn()?;
        let mut not_found = Vec::new();
        for &key in pkgs {
            if self.epo.get(&wtxn, key)?.is_none() {
                not_found.push(key);
                continue;
            }
            self.epo.delete(&mut wtxn, key)?;
            for metan in &self.metans {
                metan.del(&mut wtxn, key)?;
            }
        }
        wtxn.commit()?;
        Ok(not_found)
    }

    /// Remove every key not present in `expected`. Returns the number removed.
    ///
    /// # Errors
    /// Propagates LMDB errors.
    pub fn prune(&self, expected: &std::collections::HashSet<&[u8]>) -> heed::Result<u64> {
        let to_remove: Vec<Vec<u8>> =
            self.keys()?.into_iter().filter(|k| !expected.contains(&k.as_slice())).collect();
        let count = to_remove.len() as u64;
        let mut wtxn = self.env.write_txn()?;
        for k in &to_remove {
            self.epo.delete(&mut wtxn, k)?;
            for metan in &self.metans {
                metan.del(&mut wtxn, k)?;
            }
        }
        wtxn.commit()?;
        Ok(count)
    }

    /// Store a custom datatype's `repomd` fragment.
    ///
    /// # Errors
    /// Propagates LMDB errors.
    pub fn write_custom_datatype(&self, data: &repomd::Data) -> heed::Result<()> {
        let mut txn = self.env.write_txn()?;
        self.cus.put(&mut txn, &data.r#type, data)?;
        txn.commit()?;
        Ok(())
    }

    /// # Errors
    /// Propagates LMDB errors.
    pub fn read_custom_datatype(&self, dt: &str) -> heed::Result<Option<repomd::Data>> {
        let txn = self.env.read_txn()?;
        self.cus.get(&txn, dt)
    }

    /// Delete a custom datatype's file and cache entry.
    ///
    /// # Errors
    /// Propagates LMDB and filesystem errors.
    #[tracing::instrument(skip(self))]
    pub fn del_custom_datatype(&self, dt: &str) -> heed::Result<Option<repomd::Data>> {
        let mut txn = self.env.write_txn()?;
        let Some(data) = self.cus.get(&txn, dt)? else { return Ok(None) };
        self.cus.delete(&mut txn, dt)?;
        txn.commit()?;
        Ok(Some(data))
    }

    /// Write every metan's XML into `repodata/`, then produce `repomd.xml`.
    ///
    /// Each metan runs in parallel: it opens its writer (via [`StoreBackend`]), writes the XML
    /// envelope plus all cached fragments through a compression + checksum pipeline, then returns
    /// its [`repomd::Data`] entry through [`Metan::on_ready`].
    ///
    /// # Errors
    /// Propagates writer creation, compression, LMDB, and IO errors.
    ///
    /// # Panics
    /// Panics if [`SystemTime::now`] is before the unix epoch.
    #[must_use]
    pub fn write_all(&self, comp_cfg: &CompConfig) -> Res<Vec<u8>> {
        tracing::info!("writing repodata");
        let repodata_dir = self.cfg.hier.basedir().join("repodata");
        std::fs::create_dir_all(&repodata_dir)?;

        let timestamp =
            SystemTime::now().duration_since(UNIX_EPOCH).expect("time underflow").as_secs() as i64;

        // PERF: don't collect here?'
        let mut data: Vec<repomd::Data> = self
            .metans
            .par_iter()
            .map(|metan| self.write_one(metan, comp_cfg, timestamp))
            .collect::<Res<Vec<Option<repomd::Data>>>>()?
            .into_iter()
            .flatten()
            .collect();

        self.extend_custom_datatypes(&mut data)?;
        self.write_repomd(data)
    }

    fn write_one(
        &self,
        metan: &Arc<dyn Metan>,
        comp_cfg: &CompConfig,
        timestamp: i64,
    ) -> Res<Option<repomd::Data>> {
        let repodata_dir = self.cfg.hier.basedir().join("repodata");
        let link = repodata_dir.join(metan.filename());

        // `StoreBackend::writer` is async because a remote backend must open a multipart upload.
        // For local files, this resolves immediately.
        let writer = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(self.cfg.store.writer(&link))
                .map_err(crate::err::Error::from)
        })?;

        // Pipeline: metan → open-checksum → compression → checksum → store.
        let ftmm = self.cfg.ftmm;
        let mut inner_mochi = Mochi::new(writer, ftmm);
        let (open_size, open_checksum) = {
            let mut w = comp_cfg.to_mochi(&mut inner_mochi, ftmm);

            let env = Arc::clone(&self.env);
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current()
                    .block_on(metan.on_generate(env, Box::pin(&mut w)))
                    .map_err(crate::err::Error::from)
            })?;

            (w.size, w.ftmm.finalize())
        };
        let size = inner_mochi.size;
        let checksum = inner_mochi.ftmm.finalize();

        // Build the repomd `<data>` via the metan itself.
        let generation = MetanGeneration {
            csum: repomd::Checksum { r#type: ftmm, sha: hex::encode(checksum).into() },
            osum: repomd::Checksum { r#type: ftmm, sha: hex::encode(open_checksum).into() },
            comp_ext: "xml.zst".into(),
            timestamp,
            size,
            open_size,
        };
        let ready = MetanReady { env: Arc::clone(&self.env), generation: Some(generation) };
        Ok(metan.on_ready(ready)?)
    }

    fn extend_custom_datatypes(&self, data: &mut Vec<repomd::Data>) -> Res<()> {
        let txn = self.env.read_txn()?;
        for res in self.cus.iter(&txn)? {
            let (_, d) = res?;
            data.push(d);
        }
        Ok(())
    }

    fn write_repomd(&self, data: Vec<repomd::Data>) -> Res<Vec<u8>> {
        let repodata_dir = Path::new(self.cfg.hier.basedir()).join("repodata");
        let path = repodata_dir.join("repomd.xml");
        let mut fd = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)?;
        repomd::repomd::generate(&mut fd, data)?;

        let pos = fd.stream_position()?;
        fd.seek(std::io::SeekFrom::Start(0))?;
        #[allow(clippy::cast_possible_truncation)]
        let mut buf = Vec::with_capacity(pos as usize);
        fd.read_to_end(&mut buf)?;
        Ok(buf)
    }

    /// Compact the underlying LMDB file by writing a fresh copy.
    ///
    /// This consumes `self` so the environment can be closed before the file is replaced.
    ///
    /// # Errors
    /// Propagates IO errors from copying/renaming, and [`heed`] errors from re-opening.
    pub fn compact_close(self) -> Res<()> {
        let env_dir = self.env.path().to_path_buf();

        let tmp_file = env_dir.join("data.compact");
        let data_file = env_dir.join("data.mdb");
        let old_file = env_dir.join("data.mdb.old");

        tracing::info!(dir = %env_dir.display(), "compacting cache");
        self.env.copy_to_path(&tmp_file, heed::CompactionOption::Enabled)?;

        // Close the env so the mmap is released and we can rename the file
        drop(self);

        // Atomically replace data.mdb with the compacted copy
        if data_file.exists() {
            std::fs::rename(&data_file, &old_file)?;
        }
        std::fs::rename(&tmp_file, &data_file)?;
        if let Err(e) = std::fs::remove_file(&old_file) {
            tracing::warn!(?old_file, ?e, "cannot remove file");
        }

        tracing::info!(dir = %env_dir.display(), "cache compacted");
        Ok(())
    }
}
