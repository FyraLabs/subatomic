use crate::prelude::*;
use crate::repodata::{FragEph, repomd};
use crate::{repo::hierarchy::Hierarchize, repodata::repomd::Data};
use object_store::ObjectStoreExt;
use std::{path::PathBuf, sync::Arc};
use tokio::io::AsyncWrite;
use tokio::io::AsyncWriteExt;

pub type DataDb = heed::Database<heed::types::Str, heed::types::SerdeBincode<Data>>;
pub type FragDb = heed::Database<heed::types::Bytes, heed::types::Bytes>;
pub type MarkDb =
    heed::Database<heed::types::Bytes, heed::types::U128<heed::byteorder::NativeEndian>>;

/// A collection of databases.
///
/// There are 3 types of databases:
/// - [`FragDb`]: filenames of RPM packages (utf-8 bytes) → xml bytes in e.g. `primary.xml` for
///   [`Self::db_pri`]
/// - [`DataDb`]: custom datatype in `repomd.xml` → xml bytes in `repomd.xml`
/// - [`MarkDb`]: filenames of RPM packages (utf-8 bytes) → unix epoch as [`u128`]
#[non_exhaustive]
pub struct DatabaseRack {
    // NOTE: should I own the `env`? Otherwise who should?
    /// Controller of the heed/lmdb database.
    env: heed::Env<heed::WithoutTls>,
    /// Database for storing XML fragments for primary `<metadata />`
    pri: FragDb,
    /// Database for storing XML fragments for `<filelists />`
    fil: FragDb,
    /// Database for storing XML fragments for `<otherdata />`
    oth: FragDb,
    /// Database for storing XML fragments for appstream `<components />`
    app: FragDb,
    /// Database for storing the upsert timestamp.
    ///
    /// This is useful for automatically deleting entries that do not exist. In `update_frags`, the
    /// epoch is updated for packages that have been found. Packages without the correct epoch are
    /// then purged.
    epo: MarkDb,
    /// Database for storing `repomd` fragments for custom datatypes.
    cus: DataDb,
}

impl DatabaseRack {
    fn from_env(env: heed::Env<heed::WithoutTls>) -> heed::Result<Self> {
        let mut txn = env.write_txn()?;

        let pri = env.create_database(&mut txn, Some("pri"))?;
        let fil = env.create_database(&mut txn, Some("fil"))?;
        let oth = env.create_database(&mut txn, Some("oth"))?;
        let app = env.create_database(&mut txn, Some("app"))?;
        let epo = env.create_database(&mut txn, Some("epo"))?;
        let cus = env.create_database(&mut txn, Some("cus"))?;

        txn.commit()?;

        Ok(Self { env, pri, fil, oth, app, epo, cus })
    }

    // 特に意味はないけどTKBだね
    #[inline]
    fn write<'a, T, K, B>(
        &'a self,
        db: &heed::Database<K, B>,
        wtxn: &mut heed::RwTxn<'a>,
        f: impl Fn(&heed::Database<K, B>, &mut heed::RwTxn<'_>) -> heed::Result<T>,
    ) -> heed::Result<T> {
        let res = f(db, wtxn);
        let Err(heed::Error::Mdb(heed::MdbError::MapFull)) = res else { return res };
        tracing::info!("committing due to MapFull");
        replace_with::replace_with_or_abort(wtxn, |wtxn| {
            wtxn.commit().expect("cannot commit");
            self.env.write_txn().expect("cannot obtain wtxn")
        });
        f(db, wtxn)
    }

    /// Store a custom datatype repomd fragment `data` into the cache.
    #[inline]
    pub fn write_custom_datatype(&self, data: &repomd::Data) -> heed::Result<()> {
        let mut txn = self.env.write_txn()?;
        self.cus.put(&mut txn, data.r#type.as_type(), data)?;
        txn.commit()?;
        Ok(())
    }
    /// Read a custom datatype repomd fragment by `dt`, the datatype (first key in
    /// [`repomd::DataType::Custom`]).
    #[inline]
    pub fn read_custom_datatype(&self, dt: &str) -> heed::Result<Option<repomd::Data>> {
        let txn = self.env.read_txn()?;
        self.cus.get(&txn, dt)
    }

    /// Insert a batch of already-serialised fragments directly into the split DBs.
    /// No purging – intended for manual/add mode where we overwrite.
    pub fn extend<I: IntoIterator<Item = (B, FragEph)>, B: AsRef<[u8]>>(
        &self,
        fragments: I,
    ) -> heed::Result<()> {
        let mut wtxn = self.env.write_txn()?;
        for (key, frag) in fragments {
            self.pri.put(&mut wtxn, key.as_ref(), frag.pri.0.as_deref().unwrap_or(b""))?;
            self.fil.put(&mut wtxn, key.as_ref(), frag.fil.0.as_deref().unwrap_or(b""))?;
            self.oth.put(&mut wtxn, key.as_ref(), frag.oth.0.as_deref().unwrap_or(b""))?;
            if let Some(app) = &frag.app.0 {
                self.app.put(&mut wtxn, key.as_ref(), app)?;
            }
            // Mark as present (epoch doesn't matter for non‑incremental use)
            self.epo.put(&mut wtxn, key.as_ref(), &0u128)?;
        }
        wtxn.commit()?;
        Ok(())
    }

    pub fn has(&self, key: &[u8]) -> Res<bool> {
        let txn = self.env.read_txn()?;
        Ok(self.epo.get(&txn, key)?.is_some())
    }

    /// Return the number of cached fragments.
    pub fn len(&self) -> heed::Result<u64> {
        let txn = self.env.read_txn()?;
        self.epo.len(&txn)
    }

    pub fn is_empty(&self) -> heed::Result<bool> {
        Ok(self.len()? == 0)
    }

    /// Delete a list of packages (by key), returning the keys that were NOT found.
    ///
    /// # Errors
    /// An error is returned if deleting a package failed. Note that an invalid key (the package
    /// doesn't exist) would not result in an error.
    pub fn drain<'a>(&self, pkgs: &[&'a [u8]]) -> heed::Result<Vec<&'a [u8]>> {
        let mut not_found = Vec::new();
        let mut wtxn = self.env.write_txn()?;
        for &key in pkgs {
            if self.epo.get(&wtxn, key)?.is_none() {
                not_found.push(key);
                continue;
            }
            self.epo.delete(&mut wtxn, key)?;
            self.pri.delete(&mut wtxn, key)?;
            self.fil.delete(&mut wtxn, key)?;
            self.oth.delete(&mut wtxn, key)?;
            self.app.delete(&mut wtxn, key)?;
        }
        wtxn.commit()?;
        Ok(not_found)
    }

    /// Collect every key currently stored in the cache.
    pub fn keys(&self) -> heed::Result<Vec<Vec<u8>>> {
        let txn = self.env.read_txn()?;
        let mut out = Vec::new();
        for res in self.epo.iter(&txn)? {
            let (k, _) = res?;
            out.push(k.to_owned());
        }
        Ok(out)
    }

    /// Compact the underlying LMDB file by writing a fresh copy.
    ///
    /// This consumes `self` so the environment can be closed before the file is replaced.
    ///
    /// # Errors
    /// Propagates IO errors from copying/renaming, and [`heed`] errors from re-opening.
    pub fn compact_close(self) -> heed::Result<()> {
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

pub const DEFAULT_MAP_SIZE: usize = 10 * 1024 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct CacheConfig<H: Hierarchize, S: Store> {
    /// Identifier for the repository.
    pub repo: String,
    /// Directory for storing cache files.
    ///
    /// This should be persistent, otherwise subatomic will forget all package metadata!
    pub cache_dir: PathBuf,
    /// Directory hierarchy used for the repository.
    ///
    /// This governs the folder structure of the repository.
    pub hier: H,
    /// The store backend.
    ///
    /// This governs where and how the metadata files are stored.
    pub store: Arc<S>,
    /// LMDB virtual address-space reservation for the cache file.
    pub lmdb_map_size: usize = DEFAULT_MAP_SIZE,
}

/// Cache for repository packages.
///
/// Handle for managing XML fragments of package metadata. These fragments are concatenated
/// to form the final XMLs.
///
/// This internally uses a [`heed::Database`], which is an efficient KV database (not relational!).
#[non_exhaustive]
pub struct Cache<H: Hierarchize, S: object_store::ObjectStore> {
    pub cfg: CacheConfig<H, S>,
    db: DatabaseRack,
}

impl<H: Hierarchize, S: object_store::ObjectStore> Cache<H, S> {
    /// Initialize a repository cache for writing the final XML files.
    ///
    /// This uses [`heed`] to write cached xml fragments ([`RepoCacheFragment`]) to a cache file per
    /// repository. We create separate files for different repositories to make sure subatomic can
    /// handle multiple repositories concurrently.
    ///
    /// The `path` to the cache file is specified by the caller.
    ///
    /// # Errors
    /// An error is returned when `heed` fails to open the cache file.
    pub fn new(cfg: CacheConfig<H, S>) -> heed::Result<Self> {
        tracing::debug!(?cfg, "opening cache");
        let path = cfg.cache_dir.join(&*cfg.repo);

        // Remove any stale file sitting where LMDB wants a directory.
        if path.is_file() {
            tracing::warn!(path = %path.display(), "removing stale cache file");
            std::fs::remove_file(&path)?;
        }

        // LMDB expects the parent dirs to exist. Create them upfront.
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Ensure the LMDB directory exists (heed creates it, but only after a successful open).
        // Pre-creating avoids races where the directory is briefly not there.
        std::fs::create_dir_all(&path)?;

        // SAFETY: assume this file is not modified concurrently
        let env = unsafe {
            heed::EnvOpenOptions::new()
                .read_txn_without_tls()
                .max_dbs(6)
                .map_size(cfg.lmdb_map_size)
                .flags(heed::EnvFlags::WRITE_MAP | heed::EnvFlags::NO_SYNC)
                .open(path)?
        };
        Self::new_with_env(cfg, env)
    }

    pub fn new_with_env(
        cfg: CacheConfig<H, S>,
        env: heed::Env<heed::WithoutTls>,
    ) -> heed::Result<Self> {
        Ok(Self { cfg, db: DatabaseRack::from_env(env)?, .. })
    }

    /// Write all xml outputs (include repomd), then return the contents of `repomd.xml`.
    ///
    /// The caller should handling signing of the `repomd.xml` file.
    ///
    /// # Panics
    ///
    /// Currently, the function panics if `datatypes` contains things that subatomic does not process.
    pub fn write_all(&self, datatypes: &[repomd::DataType]) -> Res<Vec<u8>> {
        tracing::info!(repodata_dir = %self.repodata_dir.display(), "writing repodata");
        std::fs::create_dir_all(&self.repodata_dir)?;
        let files = datatypes.iter().map(|dt| self.repodata_dir.join(dt.as_str())).collect_vec();
        let data = datatypes.par_iter().cloned().zip_eq(&files);
        let data = data.map(|(dt, path)| self.write_stage1(path, dt));
        let mut data = data.collect::<Res<Vec<_>>>()?;
        for dat in &data {
            let oldname = self.repodata_dir.join(dat.r#type.as_str());
            let newname = format!("{}-{}.xml.zst", dat.checksum.sha, dat.r#type);
            std::fs::rename(oldname, self.repodata_dir.join(newname))?;
        }
        self.extend_custom_datatypes(&mut data)?;

        self.write_repomd(data)
    }

    fn extend_custom_datatypes(&self, data: &mut Vec<repomd::Data>) -> Res<()> {
        let txn = self.env.read_txn()?;
        self.db_cus.iter(&txn)?.map_ok(|(_, d)| d).process_results(|it| {
            // FIXME: refactor this????
            data.extend(it.update(|d| {
                if let repomd::DataType::Custom(typ, filename) = &mut d.r#type {
                    d.r#type = repomd::DataType::Custom(
                        std::mem::take(typ),
                        format!(":{filename}").into(),
                    );
                }
            }));
        })?;
        Ok(())
    }

    fn write_repomd(&self, data: Vec<repomd::Data>) -> Res<Vec<u8>> {
        tracing::debug!("writing repomd");
        let path = self.repodata_dir.join("repomd.xml");
        let mut fd_repomd = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)?;
        repomd::repomd::generate(&mut fd_repomd, data)?;

        let pos = fd_repomd.stream_position()?;
        fd_repomd.seek(std::io::SeekFrom::Start(0))?;
        #[allow(clippy::cast_possible_truncation)] // same behaviour even on 32-bit platforms
        let mut buf = Vec::with_capacity(pos as usize);
        fd_repomd.read_to_end(&mut buf)?;

        Ok(buf)
    }

    /// Upsert fragments, and remove ones that are not inserted.
    ///
    /// Return numbers of (new, cached) packages.
    ///
    /// # Panics
    /// Panic on time underflow and frag keys that are not found.
    ///
    /// # Errors
    /// Mostly heed errors.
    pub fn update_frags(
        &self,
        // TODO: should we use Vec<u8> (filename) instead of PathBuf to reduce mem?
        recv: &crossbeam_channel::Receiver<(PathBuf, Option<FragEph>)>,
    ) -> Res<(u64, u64)> {
        let epoch = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .expect("time underflow")
            .as_micros();
        let mut wtxn = self.env.write_txn()?;
        let mut new: u64 = 0;
        let mut cached: u64 = 0;
        while let Ok((p, frag)) = recv.recv() {
            tracing::debug!(p=%p.display(), "received");
            let key = p.as_os_str().as_encoded_bytes();
            self.write(&self.db_epo, &mut wtxn, |db, wtxn| db.put(wtxn, key, &epoch))?;
            let Some(frag) = frag else {
                cached += 1;
                continue;
            };
            new += 1;
            self.write(&self.db_pri, &mut wtxn, |db, wtxn| {
                db.put(wtxn, key, frag.pri.0.as_deref().expect("pri"))
            })?;
            self.write(&self.db_fil, &mut wtxn, |db, wtxn| {
                db.put(wtxn, key, frag.fil.0.as_deref().expect("fil"))
            })?;
            self.write(&self.db_oth, &mut wtxn, |db, wtxn| {
                db.put(wtxn, key, frag.oth.0.as_deref().expect("oth"))
            })?;
            if let Some(app) = frag.app.0 {
                self.write(&self.db_app, &mut wtxn, |db, wtxn| db.put(wtxn, key, &app))?;
            }
            tracing::trace!(p=%p.display(), "finished");
        } // until recv is closed
        tracing::info!("purging old fragments");
        let mut it = self.db_epo.iter_mut(&mut wtxn)?;
        // NOTE: unfortunately we cannot delete items in different dbs in parallel, but fortunately
        // most of the time we don't delete packages.
        let mut purged = Vec::new();
        while let Some(res) = it.next() {
            let (k, v) = res?;
            if v != epoch {
                tracing::debug!(old_key = %OsStr::from_bytes(k).display());
                purged.push(k.to_owned());
                // SAFETY: we do not keep any references to any values from this db
                assert!(unsafe { it.del_current()? }, "cannot delete item");
            }
        }
        drop(it);
        let dbs = [&self.db_pri, &self.db_fil, &self.db_oth, &self.db_app];
        for (db, k) in dbs.into_iter().cartesian_product(&purged) {
            db.delete(&mut wtxn, k)?;
        }
        wtxn.commit()?;
        Ok((new, cached))
    }
}

pub struct MetaWriter<D: sha2::Digest + Unpin + Clone> {
    pub digest: D,
    pub comp_cfg: kuchiyose::comp::CompConfig,
    pub timestamp: i64,
}

impl<D: sha2::Digest + Unpin + Clone> MetaWriter<D> {
    async fn process<H: Hierarchize, S: Store>(
        self,
        dt: repomd::DataType,
        cache: &Cache<H, S>,
    ) -> object_store::Result<heed::Result<repomd::Data>> {
        let repodata_dir =
            object_store::path::Path::from(cache.cfg.hier.basedir()).join("repodata");
        let tmp_path = repodata_dir.join(dt.as_str());
        let mut w = kuchiyose::store::MultipartUploadWriter::with_defaults(
            cache.cfg.store.put_multipart(&tmp_path).await?,
        );
        let mut inner_mochi = kuchiyose::comp::Mochi::new(w, self.digest.clone());
        let mut w = self.comp_cfg.to_mochi(&mut inner_mochi, self.digest);
        Ok(try {
            let txn = cache.db.env.read_txn()?;
            let db = match &dt {
                repomd::DataType::Primary => &cache.db.pri,
                repomd::DataType::Filelists => &cache.db.fil,
                repomd::DataType::Other => &cache.db.oth,
                repomd::DataType::Group => panic!("do not expect group in stage1"),
                repomd::DataType::Appstream => &cache.db.app,
                repomd::DataType::Custom(_, _) => panic!("do not expect custom in stage1"),
            };
            let l = db.len(&txn)?;
            tracing::trace!(count = l, "reading fragments from cache");
            let frags = db.iter(&txn)?.map(|r| r.map(|(_, v)| v));
            Self::write_stage1_prexml(cache, &dt, &mut w, l).await?;

            for frag in frags {
                w.write_all(frag?).await?;
            }
            Self::write_stage1_postxml(&dt, &mut w).await?;
            let osum = hex::encode(w.ftmm.finalize()).into();
            let csum = hex::encode(inner_mochi.ftmm.finalize()).into();

            // HACK: specific naming case for custom dt
            if let repomd::DataType::Custom(typ, filename) = dt {
                dt = repomd::DataType::Custom(typ, format!("{csum}-{filename}.zst").into());
            }
            let href = format!("repodata/{csum}-{dt}.xml.zst").into();
            // TODO: rename、今あくまでもtmp_pathや
            repomd::Data {
                location: repomd::Location { href },
                r#type: dt,
                checksum: repomd::Checksum { sha: csum, .. },
                open_checksum: repomd::Checksum { sha: osum, .. },
                timestamp: self.timestamp,
                size: inner_mochi.size,
                open_size: w.size,
            }
        })
    }

    #[allow(clippy::unimplemented)]
    async fn write_stage1_prexml<H: Hierarchize, S: Store, W: AsyncWrite + Unpin>(
        cache: &Cache<H, S>,
        dt: &repomd::DataType,
        mut w: W,
        l: u64,
    ) -> std::io::Result<()> {
        match dt {
            repomd::DataType::Primary => w.write_all(
                format!(r#"<?xml version="1.0" encoding="UTF-8"?><metadata xmlns="http://linux.duke.edu/metadata/common" xmlns:rpm="http://linux.duke.edu/metadata/rpm" packages="{l}">"#).as_bytes()
            ).await,
            repomd::DataType::Filelists => w.write_all(
                format!(r#"<?xml version="1.0" encoding="UTF-8"?><filelists xmlns="http://linux.duke.edu/metadata/filelists" packages="{l}">"#).as_bytes()
            ).await,
            repomd::DataType::Other => w.write_all(
                format!(r#"<?xml version="1.0" encoding="UTF-8"?><otherdata xmlns="http://linux.duke.edu/metadata/other" packages="{l}">"#).as_bytes()
            ).await,
            repomd::DataType::Group => unimplemented!("comps are not generated by libsubatomic"),
            repomd::DataType::Appstream => w.write_all(
                format!(r#"<?xml version="1.0" encoding="UTF-8"?><components origin="{}" version="0.14">"#,
                cache.cfg.repo).as_bytes()
            ).await,
            repomd::DataType::Custom(_, s) => {
                unimplemented!("custom dt `{s}` not generated by libsubatomic")
            }
        }
    }
    #[allow(clippy::unimplemented, clippy::unused_self)]
    async fn write_stage1_postxml<W: AsyncWrite + Unpin>(
        dt: &repomd::DataType,
        mut w: W,
    ) -> std::io::Result<()> {
        match dt {
            repomd::DataType::Primary => w.write_all(b"</metadata>").await,
            repomd::DataType::Filelists => w.write_all(b"</filelists>").await,
            repomd::DataType::Other => w.write_all(b"</otherdata>").await,
            repomd::DataType::Group => unimplemented!("comps are not generated by libsubatomic"),
            repomd::DataType::Appstream => w.write_all(b"</components>").await,
            repomd::DataType::Custom(_, s) => {
                unimplemented!("custom dt `{s}` not generated by libsubatomic")
            }
        }
    }
}
