//! Cache for repository package metadata and XML fragment storage.
//!
//! [`Cache`] owns the LMDB environment and holds a list of [`Metan`] trait objects, one per
//! repomd datatype (primary, filelists, other, appstream, …). Each `Metan` owns its own LMDB
//! database and knows how to serialize packages into XML fragments, generate the final XML,
//! and emit the corresponding `<data>` entry in `repomd.xml`.

use crate::metan_prelude::*;
use crate::pkg::MetanInput;
use crate::prelude::*;
use crate::repo::hierarchy::Hierarchize;
use crate::repodata::{MetanGeneration, MetanReady, repomd};
use kuchiyose::comp::{CompConfig, Mochi};
use kuchiyose::ftmm::Ftmm;
use kuchiyose::store::StoreBackend;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::AsyncWriteExt;

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

    /// Force struct constructions to use the `MyStruct { fields, .. }` notation.
    #[expect(private_interfaces)]
    pub non_exhaustive: crate::NonExhaustive = crate::NonExhaustive,
}

/// Repository metadata cache.
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

        let max_dbs = metans.iter().map(|m| m.db_count()).sum::<u32>() + 2; // + env & cus
        // SAFETY: assume this file is not modified concurrently
        let env = unsafe {
            heed::EnvOpenOptions::new()
                .read_txn_without_tls()
                .max_dbs(max_dbs)
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

    #[inline]
    fn wtxn<'a, T>(
        &'a self,
        wtxn: &mut heed::RwTxn<'a>,
        f: impl Fn(&mut heed::RwTxn<'_>) -> heed::Result<T>,
    ) -> heed::Result<T> {
        let res = f(wtxn);
        let Err(heed::Error::Mdb(heed::MdbError::MapFull)) = res else { return res };
        tracing::info!("committing due to MapFull");
        replace_with::replace_with_or_abort(wtxn, |wtxn| {
            wtxn.commit().expect("cannot commit");
            self.env.write_txn().expect("cannot obtain wtxn")
        });
        f(wtxn)
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
    pub fn has(&self, key: &crate::Kiri) -> Res<bool> {
        let txn = self.env.read_txn()?;
        Ok(self.epo.get(&txn, key.as_bytes())?.is_some())
    }

    /// Collect every cached package filename.
    ///
    /// # Errors
    /// Propagates LMDB errors.
    pub fn keys(&self) -> Res<Vec<crate::Kirifuda>> {
        let txn = self.env.read_txn()?;
        let mut out = Vec::new();
        for res in self.epo.iter(&txn)? {
            let (k, _) = res?;
            out.push(crate::Kiri::from_bytes(k)?.to_owned());
        }
        Ok(out)
    }

    pub fn compute(&self, path: &Path, input: ComputeInput) -> Res<Vec<MetanComputed>> {
        let ComputeInput { csum, link, .. } = input;
        let filename = crate::Kiri::new(path)?;
        let link = link.or(self.cfg.hier.locate_relative(filename)).ok_or(Error::HierRejectPath)?;
        let reader = rpm::PackageReader::open(path)?;
        let csum = match csum {
            Some(c) => c,
            None => digest(self.cfg.ftmm, path)?,
        };
        let input = MetanInput {
            metadata: reader.metadata,
            fmeta: std::fs::metadata(path)?,
            csum,
            link,
            filename: filename.to_owned(),
            csum_type: self.cfg.ftmm,
            path: path.to_path_buf(),
            ..
        };
        self.metans.par_iter().map(|metan| Res::Ok(metan.compute(&input)?)).collect()
    }

    /// Update the cache to include all and only the packages from `recv`.
    ///
    /// Each package is fanned out to every registered [`Metan`]. After the channel drains, any
    /// package whose epoch is stale (not seen this round) is purged from every metan.
    ///
    /// Return `(new, cached)` — the number of parsed packages and the number of cache hits.
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

        for (abs_path, req) in recv {
            let Some(filename) = abs_path.file_name() else {
                tracing::error!(p = %abs_path.display(), "path has no filename; skipping");
                continue;
            };

            self.wtxn(&mut wtxn, |wtxn| self.epo.put(wtxn, filename.as_bytes(), &epoch))?;

            match req {
                FragRequest::Cached => cached += 1,
                FragRequest::Put(computed_results) => {
                    self.save_computed_result(&mut wtxn, &mut new, computed_results)?;
                }
            }
        }

        // purge stale keys
        let mut it = self.epo.iter_mut(&mut wtxn)?;
        let mut purged: Vec<crate::Kirifuda> = Vec::new();
        while let Some(res) = it.next() {
            let (k, v) = res?;
            if v != epoch {
                purged.push(crate::Kiri::from_bytes(k)?.to_owned());
                // SAFETY: we do not keep any references to any values from this db
                assert!(unsafe { it.del_current()? }, "cannot delete item");
            }
        }
        drop(it);
        for metan in &self.metans {
            for k in &purged {
                metan.del(&mut wtxn, &k)?;
            }
        }
        wtxn.commit()?;
        Ok((new, cached))
    }

    fn save_computed_result<'a>(
        &'a self,
        wtxn: &mut heed::RwTxn<'a>,
        new: &mut u64,
        computed_results: Vec<crate::repodata::MetanComputed>,
    ) -> Res<()> {
        *new += 1;
        for (metan, computed) in self.metans.iter().zip_eq(computed_results) {
            self.wtxn(wtxn, |wtxn| match metan.save(wtxn, &computed) {
                Ok(r) => Ok(Ok(r)),
                Err(MetanError::Heed { source, .. }) => Err(source),
                Err(e) => Ok(Err(e)),
            })??;
        }
        Ok(())
    }

    /// Delete the given packages from every metan database.
    ///
    /// Returns the subset of `pkgs` that were not present.
    ///
    /// # Errors
    /// Propagates LMDB errors.
    pub fn delete_pkgs<'a, I, K>(&self, pkgs: I) -> heed::Result<Vec<K>>
    where
        I: IntoIterator<Item = K>,
        K: AsRef<crate::Kiri> + 'a,
    {
        let mut wtxn = self.env.write_txn()?;
        let mut not_found = Vec::new();
        for key in pkgs {
            if self.epo.get(&wtxn, key.as_ref().as_bytes())?.is_none() {
                not_found.push(key);
                continue;
            }
            self.epo.delete(&mut wtxn, key.as_ref().as_bytes())?;
            for metan in &self.metans {
                metan.del(&mut wtxn, key.as_ref())?;
            }
        }
        wtxn.commit()?;
        Ok(not_found)
    }

    /// Remove every key not present in `expected`. Return the number removed.
    ///
    /// # Errors
    /// Propagates LMDB errors.
    pub(crate) fn prune(&self, expected: &std::collections::HashSet<crate::Kirifuda>) -> Res<u64> {
        let to_remove: Vec<kuchiyose::kiri::Kirifuda> =
            self.keys()?.into_iter().filter(|k| !expected.contains(k.deref())).collect();
        let count = to_remove.len() as u64;
        let mut wtxn = self.env.write_txn()?;
        for k in &to_remove {
            self.epo.delete(&mut wtxn, k.as_bytes())?;
            for metan in &self.metans {
                metan.del(&mut wtxn, k)?;
            }
        }
        wtxn.commit()?;
        Ok(count)
    }

    /// Store a custom datatype's `repomd` fragment, and return the old instance.
    ///
    /// # Errors
    /// Propagates LMDB errors.
    pub(crate) fn write_custom_datatype(
        &self,
        data: &repomd::Data,
    ) -> heed::Result<Option<repomd::Data>> {
        let mut txn = self.env.write_txn()?;
        let ret = self.cus.get(&txn, &data.r#type)?;
        self.cus.put(&mut txn, &data.r#type, data)?;
        txn.commit()?;
        Ok(ret)
    }

    /// # Errors
    /// Propagates LMDB errors.
    pub(crate) fn read_custom_datatype(&self, dt: &str) -> heed::Result<Option<repomd::Data>> {
        let txn = self.env.read_txn()?;
        self.cus.get(&txn, dt)
    }

    /// Delete a custom datatype's file and cache entry.
    ///
    /// # Errors
    /// Propagates LMDB and filesystem errors.
    #[tracing::instrument(skip(self))]
    pub(crate) fn del_custom_datatype(&self, dt: &str) -> heed::Result<Option<repomd::Data>> {
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
    pub async fn write_all(&self, comp_cfg: &CompConfig) -> Res<Vec<u8>> {
        tracing::info!("writing repodata");
        let repodata_dir = self.cfg.hier.basedir().join("repodata");
        if let StoreBackend::Local = &*self.cfg.store {
            tokio::fs::create_dir_all(&repodata_dir).await?;
        }

        let timestamp =
            SystemTime::now().duration_since(UNIX_EPOCH).expect("time underflow").as_secs() as i64;

        let futs = self.metans.iter().map(|metan| self.write_one(metan, comp_cfg, timestamp));
        let data = futures::future::try_join_all(futs).await?;
        let mut data = data.into_iter().flatten().collect();

        self.extend_custom_datatypes(&mut data)?;
        self.write_repomd(data).await
    }

    /// Generate and write the xml for one [`Metan`].
    async fn write_one(
        &self,
        metan: &Arc<dyn Metan>,
        comp_cfg: &CompConfig,
        timestamp: i64,
    ) -> Res<Option<repomd::Data>> {
        let repodata_dir = self.cfg.hier.basedir().join("repodata");
        let link = repodata_dir.join(metan.filename());

        // `StoreBackend::writer` is async because a remote backend must open a multipart upload.
        // For local files, this resolves immediately.
        let writer = self.cfg.store.writer(&link).await.map_err(crate::err::Error::from)?;

        // Pipeline: metan → open-checksum → compression → checksum → store.
        let ftmm = self.cfg.ftmm;
        let mut inner_mochi = Mochi::new(writer, ftmm);
        let (open_size, open_checksum) = {
            let mut w = comp_cfg.to_mochi(&mut inner_mochi, ftmm);

            let env = Arc::clone(&self.env);
            metan.on_generate(env, Box::pin(&mut w)).await.map_err(crate::err::Error::from)?;
            w.shutdown().await?;

            (w.size, w.ftmm.finalize())
        };
        let size = inner_mochi.size;
        let sha = hex::encode(inner_mochi.ftmm.finalize()).into();
        let (filename, ext) = (metan.filename(), comp_cfg.ext());
        let newlink = repodata_dir.join(format!("{sha}-{filename}.{ext}"));
        self.cfg.store.rename(&link, &newlink).await?;

        let generation = MetanGeneration {
            csum: repomd::Checksum { r#type: ftmm, sha },
            osum: repomd::Checksum { r#type: ftmm, sha: hex::encode(open_checksum).into() },
            comp_ext: ext.into(),
            timestamp,
            size,
            open_size,
            ..
        };
        let ready = MetanReady { env: Arc::clone(&self.env), generation: Some(generation), .. };
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

    async fn write_repomd(&self, data: Vec<repomd::Data>) -> Res<Vec<u8>> {
        let repodata_dir = self.cfg.hier.basedir().join("repodata");
        let link = repodata_dir.join("repomd.xml");
        let mut async_writer =
            self.cfg.store.writer(&link).await.map_err(crate::err::Error::from)?;

        let mut buf = Vec::new();
        let repomd = repomd::repomd {
            data,
            revision: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time underflow")
                .as_secs(),
            ..
        };
        quick_xml::se::to_utf8_io_writer(&mut buf, &repomd)?;
        async_writer.write_all(&buf).await.map_err(crate::err::Error::from)?;
        async_writer.shutdown().await?;
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

fn digest(ftmm: Ftmm, path: &Path) -> std::io::Result<String> {
    let mut reader = std::fs::File::open_buffered(path)?;
    let mut hasher = ftmm.to_digest();
    let mut buffer = [0; 10240];

    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }

    Ok(hex::encode(hasher.finalize()).into())
}

/// Options to [`Cache::compute`].
#[derive(Clone, Debug, Default)]
pub struct ComputeInput {
    /// Checksum of the package file. You MUST generate the checksum using [`CacheConfig::ftmm`].
    /// The checksum will not be validated if provided. If `None`, [`Cache::compute`] will read the
    /// entire package file to obtain a checksum.
    pub csum: Option<String>,
    /// The final location of the provided package, from [`Hierarchize::locate_relative`].
    pub link: Option<kuchiyose::LinkBuf>,

    /// Force struct constructions to use the `MyStruct { fields, .. }` notation.
    #[expect(private_interfaces)]
    pub non_exhaustive: crate::NonExhaustive = crate::NonExhaustive,
}

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("hierarchy reject path")]
    HierRejectPath,
    #[error("bad filename: {0}")]
    BadFilename(#[from] kuchiyose::kiri::Error),
}

/// Communication object with [`crate::Cache::update_frags`].
///
/// A request to [`crate::Cache::update_frags`] that marks the existence of a package. To check
/// whether a package is cached or not:
///
/// ```
/// use libsubatomic::repo::hierarchy::Hierarchize;
///
/// fn has<H: Hierarchize>(cache: libsubatomic::Cache<H>, filename: &Kiri) -> heed::Result<bool> {
///     let txn = cache.env.read_txn()?;
///     Ok(cache.epo.get(&txn, filename.as_bytes())?.is_some())
/// }
/// ```
///
/// [`Cache::has`] may also be used, but a new read transaction is created for each call.
///
/// If `has()` returns false, the package can be added via [`FragRequest::Put`]. Obtain the
/// computed vector via [`Cache::compute`].
#[non_exhaustive]
pub enum FragRequest {
    Cached,
    /// Request a new (cache-miss) package to be inserted.
    ///
    /// This requests [`crate::Cache::update_frags`] to insert a new package that was not previously
    /// in the cache. Obtain the computed vector via [`Cache::compute`].
    Put(Vec<crate::repodata::MetanComputed>),
}
