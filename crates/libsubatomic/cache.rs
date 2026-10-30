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
use kuchiyose::link::{Link, LinkBuf};
use kuchiyose::store::StoreBackend;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::AsyncWriteExt;

/// Directory (relative to [`Hierarchize::basedir`]) where all metadata is published.
///
/// Aliased here (instead of repeating the literal) because it is part of the *deletion* contract
/// in [`Cache::cleanup_obsolete_repodata`], which must not operate outside this directory.
const REPODATA_DIR: &str = "repodata";

pub(crate) type DataDb = heed::Database<heed::types::Str, heed::types::SerdeBincode<repomd::Data>>;
pub(crate) type FragDb = heed::Database<heed::types::Bytes, heed::types::Bytes>;
pub(crate) type MarkDb =
    heed::Database<heed::types::Bytes, heed::types::U128<heed::byteorder::NativeEndian>>;
/// Persistent map of the `<data>` entries of the last successfully published `repomd.xml`,
/// keyed by href: the tracked state used for obsolete-metadata cleanup.
pub(crate) type TrackedDb =
    heed::Database<heed::types::Str, heed::types::SerdeBincode<repomd::Data>>;

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
    /// LMDB environment shared by all datatypes.
    ///
    /// Handler to the LMDB "session". Use this to create transactions for interacting with any
    /// LMDB databases.
    pub env: Arc<heed::Env<heed::WithoutTls>>,
    /// Database used to mark existence of package. A record exists in this database iff the cache
    /// acknowledges the package's existence, and violation of this condition results in (safe) UB.
    ///
    /// This is used in [`Self::update_frags`] to temporarily store an arbitrary unix epoch in μs
    /// of all found packages, and records that do not have the correct latest epoch are removed.
    pub epo: MarkDb,
    /// User-supplied custom datatypes keyed by [`repomd::Data::r#type`].
    pub cus: DataDb,
    /// The `<data>` entries of the last successfully published `repomd.xml`, keyed by href.
    ///
    /// This is the persistent source for obsolete-metadata cleanup: [`Self::write_all`] reads it
    /// to learn which files the previous publication referenced, never reading `repomd.xml`
    /// from (or listing) the storage backend.
    pub tracked: TrackedDb,
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

        let max_dbs = metans.iter().map(|m| m.db_count()).sum::<u32>() + 3; // + epo & cus & tracked
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
        let tracked = env.create_database(&mut txn, Some("tracked"))?;
        for metan in &metans {
            metan.db_init(Arc::clone(&env), &mut txn)?;
        }
        txn.commit()?;

        Ok(Self { cfg, env, epo, cus, tracked, metans })
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
        let link = link
            .or_else(|| self.cfg.hier.locate_relative(filename))
            .ok_or(Error::HierRejectPath)?;
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

    /// Add new packages from `recv` into the cache.
    ///
    /// Each package is fanned out to every registered [`Metan`]. To purge other old packages, use
    /// [`Self::update_frags`].
    ///
    /// # Errors
    /// Propagates any error from [`Metan::save`], [`Metan::del`], or LMDB.
    ///
    /// # Panics
    /// Panics on the (essentially impossible) system-time underflow.
    pub fn add_frags(
        &self,
        recv: &crossbeam_channel::Receiver<(PathBuf, Vec<MetanComputed>)>,
    ) -> Res<()> {
        let epoch =
            SystemTime::now().duration_since(UNIX_EPOCH).expect("time underflow").as_micros();
        let mut wtxn = self.env.write_txn()?;

        for (abs_path, computed) in recv {
            let Some(filename) = abs_path.file_name() else {
                tracing::error!(p = %abs_path.display(), "path has no filename; skipping");
                continue;
            };

            self.wtxn(&mut wtxn, |wtxn| self.epo.put(wtxn, filename.as_bytes(), &epoch))?;
            self.save_computed_result(&mut wtxn, computed)?;
        }
        wtxn.commit()?;
        Ok(())
    }

    /// Update the cache to include all and only the packages from `recv`.
    ///
    /// Each package is fanned out to every registered [`Metan`]. After the channel drains, any
    /// package whose epoch is stale (not seen this round) is purged from every metan.
    ///
    /// Return `(new, cached)` — the number of parsed packages and the number of cache hits.
    ///
    /// To not purge existing packages, use [`Self::add_frags`].
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
                    new += 1;
                    self.save_computed_result(&mut wtxn, computed_results)?;
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
                metan.del(&mut wtxn, k)?;
            }
        }
        wtxn.commit()?;
        Ok((new, cached))
    }

    fn save_computed_result<'a>(
        &'a self,
        wtxn: &mut heed::RwTxn<'a>,
        computed_results: Vec<crate::repodata::MetanComputed>,
    ) -> Res<()> {
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

    /*
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
    */

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

    /*
    /// # Errors
    /// Propagates LMDB errors.
    pub(crate) fn read_custom_datatype(&self, dt: &str) -> heed::Result<Option<repomd::Data>> {
        let txn = self.env.read_txn()?;
        self.cus.get(&txn, dt)
    }
    */

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
    /// After the new `repomd.xml` is successfully published, metadata files that were referenced
    /// by the *previously* publication are removed (see [`Self::cleanup_obsolete_repodata`]).
    /// Which files the previous publication referenced is learned from the persistent [`Cache`]
    /// tracking database — never by reading `repomd.xml` from, or listing, the storage backend.
    /// Obsolete-file removal is best-effort: a failed individual deletion is logged and its
    /// tracking entry is kept, so a later publication retries it and the publication itself never
    /// fails.
    ///
    /// # Errors
    /// Propagates writer creation, compression, LMDB, and IO errors.
    ///
    /// # Panics
    /// Panics if [`SystemTime::now`] is before the unix epoch.
    pub async fn write_all(&self, comp_cfg: &CompConfig) -> Res<Vec<u8>> {
        tracing::info!("writing repodata");
        // Snapshot the tracked metadata locations of the last *successful* publication before
        // anything changes, so superseded files can be garbage collected afterwards.
        let previous = self.tracked_datas()?;

        let repodata_dir = self.cfg.hier.basedir().join(REPODATA_DIR);
        if let StoreBackend::Local = &*self.cfg.store {
            tokio::fs::create_dir_all(&repodata_dir).await?;
        }

        let timestamp =
            SystemTime::now().duration_since(UNIX_EPOCH).expect("time underflow").as_secs() as i64;

        let futs = self.metans.iter().map(|metan| self.write_one(metan, comp_cfg, timestamp));
        let data = futures::future::try_join_all(futs).await?;
        let mut data = data.into_iter().flatten().collect();

        self.extend_custom_datatypes(&mut data)?;
        // `data` is about to be published by `write_repomd` verbatim; snapshot its hrefs so that
        // cleanup can prove it never deletes a file the fresh manifest still references, and the
        // entries themselves so that the tracked state can be mirrored onto it.
        let kept: HashSet<LinkBuf> = data.iter().map(|d| d.location.href.clone()).collect();
        let fresh = data.clone();
        let repomd_bytes = self.write_repomd(data).await?;

        // The new manifest is now published; everything else the tracked state referenced is
        // obsolete. The tracked state is only updated (in `cleanup_and_track`) after this point,
        // so a failed publication leaves it exactly as it was.
        self.cleanup_obsolete_repodata(previous, fresh, &kept).await;
        Ok(repomd_bytes)
    }

    /// Generate and write the xml for one [`Metan`].
    async fn write_one(
        &self,
        metan: &Arc<dyn Metan>,
        comp_cfg: &CompConfig,
        timestamp: i64,
    ) -> Res<Option<repomd::Data>> {
        let repodata_dir = self.cfg.hier.basedir().join(REPODATA_DIR);
        let link = repodata_dir.join(metan.filename());

        // `StoreBackend::writer` is async because a remote backend must open a multipart upload.
        // For local files, this resolves immediately.
        let writer = self.cfg.store.writer(&link).await?;

        // Pipeline: metan → open-checksum → compression → checksum → store.
        let ftmm = self.cfg.ftmm;
        let mut inner_mochi = Mochi::new(writer, ftmm);
        let (open_size, open_checksum) = {
            let mut w = comp_cfg.to_mochi(&mut inner_mochi, ftmm);

            let env = Arc::clone(&self.env);
            metan.on_generate(env, Box::pin(&mut w)).await?;
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
        let repodata_dir = self.cfg.hier.basedir().join(REPODATA_DIR);
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

    /// Read the `<data>` entries tracked for the previously successful publication.
    ///
    /// Returns an empty vector before the very first publication (nothing to clean up) or when
    /// there simply was nothing published. The state is local (LMDB) and self-authoritative: it
    /// is written back only by [`Self::cleanup_obsolete_repodata`], which runs after a
    /// successful publication. Errors are LMDB errors; a corrupt record cannot be repaired from
    /// the store because cleanup never reads the backend — this matches every other LMDB
    /// failure mode of the cache.
    ///
    /// # Errors
    /// Propagates LMDB errors.
    pub(crate) fn tracked_datas(&self) -> Res<Vec<repomd::Data>> {
        let txn = self.env.read_txn()?;
        Ok(self
            .tracked
            .iter(&txn)?
            .map(|res| res.map(|(_, data)| data))
            .collect::<heed::Result<_>>()?)
    }

    /// Garbage-collect metadata orphaned by this publication and update the tracked state.
    ///
    /// Only runs after [`Self::write_repomd`] has published the new manifest, so the tracked
    /// state describes *the last successful publication*. Every candidate href is validated
    /// against [`is_deletable_repodata_href`] before deletion; `kept` guarantees a file the
    /// fresh manifest still references is never removed.
    ///
    /// Two-phase update: the remote deletions run first (outside the LMDB transaction, so the
    /// write lock is never held across await points), then one short LMDB write transaction
    /// (1) drops the entries whose deletion succeeded and (2) mirrors the freshly published
    /// entries. Deletion is best-effort: a failed individual removal is logged and its entry is
    /// *kept*, so the file keeps being tracked and a later publication retries the deletion —
    /// it must never roll back or fail the publication that already succeeded.
    async fn cleanup_obsolete_repodata(
        &self,
        previous: Vec<repomd::Data>,
        fresh: Vec<repomd::Data>,
        kept: &HashSet<LinkBuf>,
    ) {
        let mut removed: Vec<LinkBuf> = Vec::new();
        for data in previous {
            let href = data.location.href;
            if kept.contains(&href) {
                continue; // still referenced by the newly published repomd.xml
            }
            if !is_deletable_repodata_href(&href) {
                tracing::debug!(href = %href, "skipping unexpected tracked repomd href");
                continue;
            }
            let link = self.cfg.hier.basedir().join(&href);
            match self.cfg.store.delete(&link).await {
                Ok(()) => {
                    tracing::debug!(href = %href, "removed obsolete metadata file");
                    removed.push(href);
                }
                Err(e) => {
                    tracing::warn!(href = %href, %e, "cannot remove obsolete metadata file; its tracking entry is kept so a later publication retries");
                }
            }
        }

        // Mirror the freshly published manifest and forget the successfully removed files.
        // Even if this fails, the stale state converges on its own: leftover entries are
        // re-checked (and deleting an already-deleted file maps to success) on the next
        // publication, and cleanup only ever touches tracked hrefs that `kept` no longer has.
        let res = (|| -> heed::Result<()> {
            let mut wtxn = self.env.write_txn()?;
            for href in &removed {
                // SAFETY: we do not keep any references to the database's contents
                self.tracked.delete(&mut wtxn, href.as_str())?;
            }
            for data in &fresh {
                // Defense in depth: never track a href that the cleanup machinery could not
                // delete. (Tracked entries only ever originate here, so this is a no-op guard.)
                if !is_deletable_repodata_href(&data.location.href) {
                    tracing::warn!(href = %data.location.href, "metadata href is not deletable; it will not be garbage collected");
                    continue;
                }
                self.tracked.put(&mut wtxn, data.location.href.as_str(), data)?;
            }
            wtxn.commit()?;
            Ok(())
        })();
        if let Err(e) = res {
            tracing::warn!(error = %e, "cannot update the tracked metadata locations");
        }
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

/// Whether `href` — as recorded in a `<data>` entry of a previously published `repomd.xml` — is
/// safe to use as a deletion target.
///
/// Only metadata published by this crate qualifies: a relative path strictly under
/// [`REPODATA_DIR`] that cannot escape it (no `..` or empty path segments). The manifest
/// `repodata/repomd.xml` is excluded — it is rewritten, never removed by cleanup. This is
/// defense in depth for hrefs that were never written by us (e.g. hand-crafted tracking state).
fn is_deletable_repodata_href(href: &Link) -> bool {
    let Some(rest) = href.as_str().strip_prefix(REPODATA_DIR).and_then(|r| r.strip_prefix('/'))
    else {
        return false;
    };
    rest != "repomd.xml" && !rest.split('/').any(|segment| segment.is_empty() || segment == "..")
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
    pub csum: Option<String> = None,
    /// The final location of the provided package, from [`Hierarchize::locate_relative`].
    pub link: Option<kuchiyose::LinkBuf> = None,

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
/// use kuchiyose::Kiri;
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

#[cfg(test)]
mod tests {
    use super::*;

    // `super::*` carries the crate-wide `String` alias (smartstring); tests use the std one.
    use std::string::String;

    /// A minimal RPM, built in-process so tests do not depend on external fixture files.
    fn build_rpm(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(format!("{name}-1.0-1.noarch.rpm"));
        let pkg = rpm::PackageBuilder::new(name, "1.0", "MIT", "noarch", "Test package")
            .with_file_contents(
                b"hello world",
                rpm::FileOptions::new(format!("/usr/share/doc/{name}/readme.md")),
            )
            .expect("rpm builds")
            .build()
            .expect("rpm builds");
        pkg.write_file(&path).expect("rpm writes");
        path
    }

    fn local_test_cache(out: &Path) -> Cache<crate::repo::hierarchy::Satm0Flat> {
        let cfg = CacheConfig {
            repo: "repotest".into(),
            cache_dir: out.join("cache"),
            hier: crate::repo::hierarchy::Satm0Flat { base: out.display().to_string().into(), .. },
            store: Arc::new(StoreBackend::Local),
            ftmm: Ftmm::Sha256,
            ..
        };
        let metans: Metans = vec![
            Arc::new(PrimaryMetan::default()) as Arc<dyn Metan>,
            Arc::new(FilelistsMetan::default()),
            Arc::new(OtherMetan::default()),
        ];
        Cache::new(cfg, metans).expect("cache opens")
    }

    /// Insert one package into the cache (without the epoch-purge semantics of
    /// [`Cache::update_frags`]).
    fn add_pkg(cache: &Cache<impl Hierarchize>, path: &Path) {
        let computed = cache.compute(path, ComputeInput::default()).expect("fragment computes");
        let (tx, rx) = crossbeam_channel::bounded(1);
        tx.send((path.into(), computed)).expect("channel open");
        drop(tx);
        cache.add_frags(&rx).unwrap();
    }

    /// Publish and return the hrefs of the freshly written `repomd.xml`.
    async fn publish(cache: &Cache<impl Hierarchize>) -> Vec<String> {
        let buf = cache
            .write_all(&kuchiyose::comp::CompConfig::default())
            .await
            .expect("repodata published");
        let text = std::str::from_utf8(&buf).expect("repomd is utf-8");
        quick_xml::de::from_str::<repomd_datas>(text)
            .expect("freshly serialized repomd.xml parses back")
            .data
            .into_iter()
            .map(|d| d.location.href.as_str().to_owned())
            .collect()
    }

    /// Root-level shape to recover the `<data>` entries from freshly serialized `repomd.xml`
    /// bytes. Deliberately test-local, so that [`publish`] verifies what was *published*
    /// independently of the tracked state ([`Cache::tracked_datas`]) the cleanup uses.
    #[expect(non_camel_case_types)]
    #[derive(serde::Deserialize)]
    struct repomd_datas {
        #[serde(default)]
        data: Vec<repomd::Data>,
    }

    /// Bare filenames currently present inside the local store's `repodata/` directory.
    fn repodata_files(base: &Path) -> HashSet<String> {
        std::fs::read_dir(base.join(REPODATA_DIR))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect()
    }

    fn hrefs_to_filenames(hrefs: &[String]) -> HashSet<String> {
        hrefs
            .iter()
            .map(|h| {
                h.strip_prefix(REPODATA_DIR)
                    .and_then(|r| r.strip_prefix('/'))
                    .expect("test hrefs always start with repodata/")
                    .to_owned()
            })
            .collect()
    }

    #[tokio::test]
    async fn removes_obsolete_metadata_files_after_publication() {
        let out = tempfile::tempdir().unwrap();
        let in_dir = tempfile::tempdir().unwrap();
        let cache = local_test_cache(out.path());

        add_pkg(&cache, &build_rpm(in_dir.path(), "testa"));
        let old: Vec<String> = publish(&cache).await;
        assert_eq!(old.len(), 3, "primary+filelists+other published: {old:?}");
        // Nothing was published before this run, so nothing may have been deleted: every href
        // (and the manifest) must exist on disk.
        let files = repodata_files(out.path());
        let expected = {
            let mut e = hrefs_to_filenames(&old);
            e.insert("repomd.xml".to_owned());
            e
        };
        assert_eq!(files, expected);

        // Adding a package changes every metan's content → every checksum filename changes.
        add_pkg(&cache, &build_rpm(in_dir.path(), "testb"));
        let fresh: Vec<String> = publish(&cache).await;
        assert_eq!(fresh.len(), 3);
        assert!(fresh.iter().all(|h| !old.contains(h)), "filenames must change: {old:?} {fresh:?}");

        // Obsolete files are gone; fresh files and the manifest are the whole directory.
        let mut expected = hrefs_to_filenames(&fresh);
        expected.insert("repomd.xml".to_owned());
        assert_eq!(repodata_files(out.path()), expected);
    }

    #[tokio::test]
    async fn unchanged_metadata_is_preserved_and_not_deleted() {
        let out = tempfile::tempdir().unwrap();
        let in_dir = tempfile::tempdir().unwrap();
        let cache = local_test_cache(out.path());

        add_pkg(&cache, &build_rpm(in_dir.path(), "testa"));
        let old: Vec<String> = publish(&cache).await;
        // Second publication without any cache changes: hrefs must be identical, and cleanup
        // must not touch (or remove) any of the current files.
        let fresh: Vec<String> = publish(&cache).await;
        assert_eq!(old, fresh);
        let mut expected = hrefs_to_filenames(&fresh);
        expected.insert("repomd.xml".to_owned());
        assert_eq!(repodata_files(out.path()), expected);
    }

    #[tokio::test]
    async fn failed_publication_preserves_previous_metadata() {
        let out = tempfile::tempdir().unwrap();
        let in_dir = tempfile::tempdir().unwrap();
        let cache = local_test_cache(out.path());

        add_pkg(&cache, &build_rpm(in_dir.path(), "testa"));
        let old: Vec<String> = publish(&cache).await;

        // Sabotage the manifest target so that everything except the final publication succeeds.
        let repomd_path = out.path().join(REPODATA_DIR).join("repomd.xml");
        std::fs::remove_file(&repomd_path).unwrap();
        std::fs::create_dir(&repomd_path).unwrap();
        let res = cache.write_all(&CompConfig::default()).await;
        assert!(res.is_err(), "sabotaged repomd.xml target must fail the publication");
        std::fs::remove_dir(&repomd_path).unwrap();

        // Nothing may have been removed — cleanup runs strictly after publication succeeds.
        for href in &old {
            assert!(out.path().join(href).exists(), "old file must survive: {href}");
        }
    }

    #[tokio::test]
    async fn failed_generation_preserves_previous_metadata() {
        let out = tempfile::tempdir().unwrap();
        let in_dir = tempfile::tempdir().unwrap();
        let cache = local_test_cache(out.path());

        add_pkg(&cache, &build_rpm(in_dir.path(), "testa"));
        let old: Vec<String> = publish(&cache).await;

        // Make `repodata/` unwritable so that metan generation itself fails.
        let repodata = out.path().join(REPODATA_DIR);
        std::fs::set_permissions(&repodata, std::fs::Permissions::from_mode(0o555)).unwrap();
        let res = cache.write_all(&CompConfig::default()).await;
        std::fs::set_permissions(&repodata, std::fs::Permissions::from_mode(0o755)).unwrap();
        let Err(e) = res else { panic!("read-only repodata must fail generation") };
        assert!(matches!(e, crate::Error::Kuchiyose(_)), "e={e:?}");

        for href in &old {
            assert!(out.path().join(href).exists(), "old file must survive: {href}");
        }
    }

    #[tokio::test]
    async fn obsolete_custom_datatype_files_are_removed() {
        let out = tempfile::tempdir().unwrap();
        let in_dir = tempfile::tempdir().unwrap();
        let cache = local_test_cache(out.path());

        add_pkg(&cache, &build_rpm(in_dir.path(), "testa"));
        // Plant a custom datatype (`group`) file + cache entry, as [`crate::repo::Repo`] does.
        let group_href = "repodata/deadbeef-group.zst";
        let group_path = out.path().join(group_href);
        std::fs::create_dir_all(group_path.parent().unwrap()).unwrap();
        std::fs::write(&group_path, b"").unwrap();
        let data = repomd::Data {
            r#type: "group".into(),
            checksum: repomd::Checksum { r#type: Ftmm::Sha256, sha: "deadbeef".into() },
            open_checksum: repomd::Checksum { r#type: Ftmm::Sha256, sha: "cafe".into() },
            location: repomd::Location { href: group_href.into() },
            timestamp: 1,
            size: 0,
            open_size: 0,
        };
        assert!(cache.write_custom_datatype(&data).unwrap().is_none());

        let published: Vec<String> = publish(&cache).await;
        assert!(
            published.iter().any(|h| h == group_href),
            "group must be referenced: {published:?}"
        );
        assert!(group_path.exists());

        // Removing the custom datatype from the cache means the next publication no longer
        // references its file → the file must be deleted, while the regular metadata stays.
        assert!(cache.del_custom_datatype("group").unwrap().is_some());
        let fresh: Vec<String> = publish(&cache).await;
        assert!(!fresh.iter().any(|h| h == group_href));
        assert!(!group_path.exists(), "obsolete custom datatype file must be removed");
        for href in &fresh {
            assert!(out.path().join(href).exists(), "still-referenced file must remain: {href}");
        }
    }

    #[tokio::test]
    async fn cleanup_works_with_remote_store() {
        let out = tempfile::tempdir().unwrap();
        let in_dir = tempfile::tempdir().unwrap();
        let backend =
            Arc::new(StoreBackend::Remote(Arc::new(object_store::memory::InMemory::new())));
        let cache_dir = tempfile::tempdir_in(out.path()).unwrap();
        std::mem::forget(cache_dir);
        let cfg = CacheConfig {
            repo: "repotest".into(),
            cache_dir: out.path().join("cache"),
            hier: crate::repo::hierarchy::Satm0Flat {
                base: out.path().display().to_string().into(),
                ..
            },
            store: Arc::clone(&backend),
            ftmm: Ftmm::Sha256,
            ..
        };
        let metans: Metans = vec![
            Arc::new(PrimaryMetan::default()) as Arc<dyn Metan>,
            Arc::new(FilelistsMetan::default()),
            Arc::new(OtherMetan::default()),
        ];
        let cache = Cache::new(cfg, metans).expect("cache opens");

        add_pkg(&cache, &build_rpm(in_dir.path(), "testa"));
        let old: Vec<String> = publish(&cache).await;
        add_pkg(&cache, &build_rpm(in_dir.path(), "testb"));
        let fresh: Vec<String> = publish(&cache).await;
        assert!(fresh.iter().all(|h| !old.contains(h)));

        // Everything still referenced must be in the remote store; obsolete files must not.
        let StoreBackend::Remote(inner) = &*backend else { unreachable!() };
        use futures::StreamExt as _;
        let all = inner
            .list(None)
            .map(|meta| meta.unwrap().location.as_ref().to_owned())
            .collect::<Vec<_>>()
            .await;
        for href in &fresh {
            let hit = all.iter().any(|p| p.ends_with(&**href));
            assert!(hit, "fresh file must be stored: {href} (have {all:?})");
        }
        for href in &old {
            let hit = all.iter().any(|p| p.ends_with(&**href));
            assert!(!hit, "obsolete file must be removed: {href} (have {all:?})");
        }
        assert_eq!(all.len(), fresh.len() + 1, "repomd.xml plus fresh metadata");
    }

    /// Hrefs currently present in the tracked (LMDB) publication state, as a set.
    fn tracked_hrefs(cache: &Cache<impl Hierarchize>) -> std::collections::HashSet<String> {
        cache
            .tracked_datas()
            .unwrap()
            .into_iter()
            .map(|d| d.location.href.as_str().to_owned())
            .collect()
    }

    #[tokio::test]
    async fn tracked_state_is_empty_before_first_publication() {
        let out = tempfile::tempdir().unwrap();
        let in_dir = tempfile::tempdir().unwrap();
        let cache = local_test_cache(out.path());
        assert!(tracked_hrefs(&cache).is_empty(), "fresh cache tracks nothing");

        // Even with fragments staged, nothing is tracked until a publication succeeds: the
        // tracked state always describes the last *successful* publication, not a draft.
        add_pkg(&cache, &build_rpm(in_dir.path(), "testa"));
        assert!(tracked_hrefs(&cache).is_empty());

        let published: Vec<String> = publish(&cache).await;
        assert_eq!(
            tracked_hrefs(&cache),
            published.into_iter().collect(),
            "first publication is tracked"
        );
    }

    #[tokio::test]
    async fn failed_publication_leaves_tracked_state_unchanged() {
        let out = tempfile::tempdir().unwrap();
        let in_dir = tempfile::tempdir().unwrap();
        let cache = local_test_cache(out.path());

        add_pkg(&cache, &build_rpm(in_dir.path(), "testa"));
        let old: Vec<String> = publish(&cache).await;
        assert_eq!(tracked_hrefs(&cache), old.iter().map(|s| s.clone()).collect());

        // Sabotage the manifest target so that everything except the final publication succeeds.
        let repomd_path = out.path().join(REPODATA_DIR).join("repomd.xml");
        std::fs::remove_file(&repomd_path).unwrap();
        std::fs::create_dir(&repomd_path).unwrap();
        let res = cache.write_all(&CompConfig::default()).await;
        assert!(res.is_err(), "sabotaged repomd.xml target must fail the publication");
        std::fs::remove_dir(&repomd_path).unwrap();

        // Nothing may have been removed, and the tracked state must still be the OLD one:
        // cleanup and tracking updates run strictly after publication succeeds.
        for href in &old {
            assert!(out.path().join(href).exists(), "old file must survive: {href}");
        }
        assert_eq!(tracked_hrefs(&cache), old.iter().map(|s| s.clone()).collect());

        // A later successful publication still garbage-collects the old state (convergence).
        add_pkg(&cache, &build_rpm(in_dir.path(), "testb"));
        let fresh: Vec<String> = publish(&cache).await;
        assert_eq!(tracked_hrefs(&cache), fresh.iter().map(|s| s.clone()).collect());
        for href in &old {
            assert!(!out.path().join(href).exists(), "obsolete file removed later: {href}");
        }
    }

    #[tokio::test]
    async fn tracked_state_persists_across_cache_reopen() {
        let out = tempfile::tempdir().unwrap();
        let in_dir = tempfile::tempdir().unwrap();
        let cache = local_test_cache(out.path());

        add_pkg(&cache, &build_rpm(in_dir.path(), "testa"));
        let old: Vec<String> = publish(&cache).await;
        drop(cache);

        // Reopening the cache must restore the tracked state from LMDB (and not lose database
        // count/mappings): the persisted state is what enables cleanup after a restart.
        let cache = local_test_cache(out.path());
        assert_eq!(tracked_hrefs(&cache), old.iter().map(|s| s.clone()).collect());

        add_pkg(&cache, &build_rpm(in_dir.path(), "testb"));
        let fresh: Vec<String> = publish(&cache).await;
        for href in &old {
            assert!(!out.path().join(href).exists(), "obsolete removed after reopen: {href}");
        }
        assert_eq!(tracked_hrefs(&cache), fresh.iter().map(|s| s.clone()).collect());
    }

    #[tokio::test]
    async fn failed_deletion_is_retried_on_a_later_publication() {
        let out = tempfile::tempdir().unwrap();
        let in_dir = tempfile::tempdir().unwrap();
        let cache = local_test_cache(out.path());

        add_pkg(&cache, &build_rpm(in_dir.path(), "testa"));
        let old: Vec<String> = publish(&cache).await;

        // Sabotage one obsolete metadata file by replacing it with a directory: its deletion
        // must fail (and only its deletion), without failing the publication itself.
        let sab_path = out.path().join(&old[0]);
        std::fs::remove_file(&sab_path).unwrap();
        std::fs::create_dir(&sab_path).unwrap();

        add_pkg(&cache, &build_rpm(in_dir.path(), "testb"));
        let fresh: Vec<String> = publish(&cache).await;
        assert!(fresh.iter().all(|h| !old.contains(h)));
        for (i, href) in old.iter().enumerate() {
            if i == 0 {
                assert!(sab_path.exists(), "sabotaged file must keep failing to delete");
            } else {
                assert!(!out.path().join(href).exists(), "file must be removed: {href}");
            }
        }
        // The failed deletion's tracking entry survives; the fresh manifest is also tracked.
        let mut expected =
            fresh.iter().map(|s| s.clone()).collect::<std::collections::HashSet<_>>();
        expected.insert(old[0].clone());
        assert_eq!(tracked_hrefs(&cache), expected);

        // Resolve the failure; the next publication must retry (Task: NotFound on the now
        // absent path maps to success) and converge.
        std::fs::remove_dir(&sab_path).unwrap();
        let again: Vec<String> = publish(&cache).await;
        assert_eq!(again, fresh, "unchanged cache must produce identical hrefs");
        assert_eq!(tracked_hrefs(&cache), fresh.iter().map(|s| s.clone()).collect());
    }

    #[test]
    fn href_validation_allows_only_metadata_paths() {
        let ok =
            ["repodata/ab12-primary.xml.zst", "repodata/ab12-other.xml.gz", "repodata/a/b/c.xml"];
        let bad = [
            "",
            "repodata",
            "repodata/",
            "repodata//",
            "repodata/repomd.xml",
            "repodata/../sneaky.txt",
            "repodata/a/../../sneaky.txt",
            "repodatas/ab12.xml",
            "/repodata/ab12.xml",
            "repodata//x.xml",
            "http://evil.example/repodata/x.xml",
            "rpm/testa-1.0-1.noarch.rpm",
        ];
        for href in ok {
            assert!(is_deletable_repodata_href(Link::new(href)), "{href} must be allowed");
        }
        for href in bad {
            assert!(!is_deletable_repodata_href(Link::new(href)), "{href} must be rejected");
        }
    }
}
