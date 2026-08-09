//! Repodata generation and XML type definitions
//!
//! This module contains repodata XML type definitions and the helper functions required to generate
//! those XML files.

pub mod appstream;
pub mod filelists;
pub mod other;
pub mod primary;
pub mod repomd;

use sha2::Digest;

use crate::prelude::*;
use std::os::linux::fs::MetadataExt;
use tracing::{debug, info, trace, warn};

pub type CompsDb = heed::Database<heed::types::Str, heed::types::SerdeBincode<repomd::Data>>;
pub type FragDb = heed::Database<heed::types::Bytes, heed::types::Bytes>;
pub type MarkDb =
    heed::Database<heed::types::Bytes, heed::types::U128<heed::byteorder::NativeEndian>>;

/// Cache for repository packages.
///
/// Handle for managing XML fragments [`RepoCacheFragment`] of package metadata. These fragments are
/// directly concatenated to form the final XMLs.
///
/// This internally uses a [`heed::Database`], which is an efficient KV database (not relational!).
/// The keys are the filenames of the RPM packages, while the values are [`RepoCacheFragment`].
#[derive(Clone, Debug)]
pub struct RepoCache {
    pub repo: String,
    pub cachedir: std::path::PathBuf,
    pub repodata_dir: std::path::PathBuf,
    pub env: heed::Env<heed::WithoutTls>,
    pub zstd_level: i32 = 0,
    pub zstd_multi: u32 = 0,
    pub db_pri: FragDb,
    pub db_fil: FragDb,
    pub db_oth: FragDb,
    pub db_app: FragDb,
    pub db_epo: MarkDb,
}

impl RepoCache {
    /// Default LMDB virtual address-space reservation for the cache file.
    ///
    /// 10 GiB is chosen because the actual memory usage remains proportional
    /// to the working set; this only reserves address space. Repos with
    /// 10k+ packages can still fit easily, and the file grows sparsely.
    const DEFAULT_MAP_SIZE: usize = 10 * 1024 * 1024 * 1024;

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
    pub fn new(repo: &str, cachedir: &Path, repodata_dir: &Path) -> heed::Result<Self> {
        debug!(repo, cachedir = %cachedir.display(), repodata_dir = %repodata_dir.display(), "opening cache");
        let path = cachedir.join(repo);
        // PERF: might be better to take in owned values?
        let cachedir = cachedir.to_owned();
        let repodata_dir = repodata_dir.to_owned();

        // Remove any stale file sitting where LMDB wants a directory.
        if path.is_file() {
            warn!(path = %path.display(), "removing stale cache file");
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
                .map_size(Self::DEFAULT_MAP_SIZE)
                .flags(heed::EnvFlags::WRITE_MAP | heed::EnvFlags::NO_SYNC)
                .open(path)?
        };
        trace!(repo, "cache opened");
        let mut txn = env.write_txn()?;

        let db_pri = env.create_database(&mut txn, Some("pri"))?;
        let db_fil = env.create_database(&mut txn, Some("fil"))?;
        let db_oth = env.create_database(&mut txn, Some("oth"))?;
        let db_app = env.create_database(&mut txn, Some("app"))?;
        let db_epo = env.create_database(&mut txn, Some("epo"))?;
        txn.commit()?;
        Ok(Self {
            db_pri,
            db_fil,
            db_oth,
            db_app,
            db_epo,
            repo: repo.into(),
            env,
            cachedir,
            repodata_dir, // TODO: don't hardcode
            ..
        })
    }

    #[inline]
    fn write<'a, T, K, B>(
        &'a self,
        db: &heed::Database<K, B>,
        wtxn: &mut heed::RwTxn<'a>,
        f: impl Fn(&heed::Database<K, B>, &mut heed::RwTxn<'_>) -> heed::Result<T>,
    ) -> heed::Result<T> {
        match f(db, wtxn) {
            Err(heed::Error::Mdb(heed::MdbError::MapFull)) => {
                info!("committing due to MapFull");
                replace_with::replace_with_or_abort(wtxn, |wtxn| {
                    wtxn.commit().expect("cannot commit");
                    self.env.write_txn().expect("cannot obtain wtxn")
                });
                f(db, wtxn)
            }
            x => x,
        }
    }

    pub fn write_comps(&self, data: &repomd::Data) -> heed::Result<()> {
        let mut txn = self.env.write_txn()?;
        // TODO: should we open this in new()?
        let db: CompsDb = self.env.create_database(&mut txn, Some("comps"))?;
        db.put(&mut txn, "compsdata", data)?;
        txn.commit()?;
        Ok(())
    }

    pub fn read_comps(&self) -> heed::Result<Option<repomd::Data>> {
        let txn = self.env.read_txn()?;
        self.env
            .open_database(&txn, Some("comps"))?
            .and_then(|db: CompsDb| db.get(&txn, "compsdata").transpose())
            .transpose()
    }
    pub fn del_comps(&self) -> heed::Result<bool> {
        let mut txn = self.env.write_txn()?;
        let db: CompsDb = self.env.create_database(&mut txn, Some("comps"))?;
        let existed = db.delete(&mut txn, "compsdata")?;
        txn.commit()?;
        Ok(existed)
    }

    /// Insert a batch of already-serialised fragments directly into the split DBs.
    /// No purging – intended for manual/add mode where we overwrite.
    pub fn insert_fragments(
        &self,
        fragments: impl IntoIterator<Item = (Vec<u8>, FragEph)>,
    ) -> heed::Result<()> {
        let mut wtxn = self.env.write_txn()?;
        for (key, frag) in fragments {
            self.db_pri.put(&mut wtxn, &key, frag.pri.0.as_deref().unwrap_or(b""))?;
            self.db_fil.put(&mut wtxn, &key, frag.fil.0.as_deref().unwrap_or(b""))?;
            self.db_oth.put(&mut wtxn, &key, frag.oth.0.as_deref().unwrap_or(b""))?;
            if let Some(app) = &frag.app.0 {
                self.db_app.put(&mut wtxn, &key, app)?;
            }
            // Mark as present (epoch doesn't matter for non‑incremental use)
            self.db_epo.put(&mut wtxn, &key, &0u128)?;
        }
        wtxn.commit()?;
        Ok(())
    }

    pub fn has(&self, key: &[u8]) -> Res<bool> {
        let txn = self.env.read_txn()?;
        Ok(self.db_epo.get(&txn, key)?.is_some())
    }

    /// Return the number of cached fragments.
    pub fn len(&self) -> heed::Result<u64> {
        let txn = self.env.read_txn()?;
        self.db_epo.len(&txn)
    }

    pub fn is_empty(&self) -> heed::Result<bool> {
        Ok(self.len()? == 0)
    }

    /// Delete a list of packages (by key), returning the keys that were NOT found.
    ///
    /// # Errors
    /// An error is returned if deleting a package failed. Note that an invalid key (the package
    /// doesn't exist) would not result in an error.
    pub fn delete_pkgs<'a>(&self, pkgs: &[&'a [u8]]) -> heed::Result<Vec<&'a [u8]>> {
        let mut not_found = Vec::new();
        let mut wtxn = self.env.write_txn()?;
        for &key in pkgs {
            if self.db_epo.get(&wtxn, key)?.is_none() {
                not_found.push(key);
                continue;
            }
            self.db_epo.delete(&mut wtxn, key)?;
            self.db_pri.delete(&mut wtxn, key)?;
            self.db_fil.delete(&mut wtxn, key)?;
            self.db_oth.delete(&mut wtxn, key)?;
            self.db_app.delete(&mut wtxn, key)?;
        }
        wtxn.commit()?;
        Ok(not_found)
    }

    /// Collect every key currently stored in the cache.
    pub fn keys(&self) -> heed::Result<Vec<Vec<u8>>> {
        debug!(repo = %self.repo, "listing cache keys");
        let txn = self.env.read_txn()?;
        let mut out = Vec::new();
        for res in self.db_epo.iter(&txn)? {
            let (k, _) = res?;
            out.push(k.to_owned());
        }
        Ok(out)
    }

    /// Delete every key not present in `expected`. Returns number of removed entries.
    pub fn prune(&self, expected: &std::collections::HashSet<&[u8]>) -> heed::Result<u64> {
        let to_remove: Vec<Vec<u8>> =
            self.keys()?.into_iter().filter(|k| !expected.contains(&k.as_slice())).collect();
        let count = to_remove.len() as u64;
        let mut wtxn = self.env.write_txn()?;
        for k in &to_remove {
            self.db_epo.delete(&mut wtxn, k)?;
            self.db_pri.delete(&mut wtxn, k)?;
            self.db_fil.delete(&mut wtxn, k)?;
            self.db_oth.delete(&mut wtxn, k)?;
            self.db_app.delete(&mut wtxn, k)?;
        }
        wtxn.commit()?;
        Ok(count)
    }

    /// Compact the underlying LMDB file by writing a fresh copy and swapping it in.
    ///
    /// This consumes `self` so the environment can be closed before the file is replaced.
    ///
    /// # Errors
    /// Propagates IO errors from copying/renaming, and [`heed`] errors from re-opening.
    pub fn compact(self) -> heed::Result<Self> {
        let env_dir = self.env.path().to_path_buf();
        let repo = self.repo.clone();
        let cachedir = self.cachedir.clone();
        let zstd = self.zstd_level;
        let repodata_dir = self.repodata_dir.clone();

        self.compact_close()?;

        let mut new = Self::new(&repo, &cachedir, &repodata_dir)?;
        new.zstd_level = zstd;
        info!(dir = %env_dir.display(), "reopened cache");
        Ok(new)
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

        info!(dir = %env_dir.display(), "compacting cache");
        self.env.copy_to_path(&tmp_file, heed::CompactionOption::Enabled)?;

        // Close the env so the mmap is released and we can rename the file
        drop(self);

        // Atomically replace data.mdb with the compacted copy
        if data_file.exists() {
            std::fs::rename(&data_file, &old_file)?;
        }
        std::fs::rename(&tmp_file, &data_file)?;
        _ = std::fs::remove_file(&old_file);

        info!(dir = %env_dir.display(), "cache compacted");
        Ok(())
    }

    #[allow(clippy::unimplemented)]
    fn write_stage1_prexml<W: Write>(
        &self,
        dt: repomd::DataType,
        mut w: W,
        l: u64,
    ) -> std::io::Result<()> {
        match dt {
            repomd::DataType::Primary => write!(
                w,
                r#"<?xml version="1.0" encoding="UTF-8"?><metadata xmlns="http://linux.duke.edu/metadata/common" xmlns:rpm="http://linux.duke.edu/metadata/rpm" packages="{l}">"#
            ),
            repomd::DataType::Filelists => write!(
                w,
                r#"<?xml version="1.0" encoding="UTF-8"?><filelists xmlns="http://linux.duke.edu/metadata/filelists" packages="{l}">"#
            ),
            repomd::DataType::Other => write!(
                w,
                r#"<?xml version="1.0" encoding="UTF-8"?><otherdata xmlns="http://linux.duke.edu/metadata/other" packages="{l}">"#
            ),
            repomd::DataType::Group => unimplemented!("comps are not generated by libsubatomic"),
            repomd::DataType::Appstream => write!(
                w,
                r#"<?xml version="1.0" encoding="UTF-8"?><components origin="{}" version="0.14">"#,
                self.repo
            ),
        }
    }
    #[allow(clippy::unimplemented, clippy::unused_self)]
    fn write_stage1_postxml<W: Write>(
        &self,
        dt: repomd::DataType,
        mut w: W,
    ) -> std::io::Result<()> {
        match dt {
            repomd::DataType::Primary => write!(w, "</metadata>"),
            repomd::DataType::Filelists => write!(w, "</filelists>"),
            repomd::DataType::Other => write!(w, "</otherdata>"),
            repomd::DataType::Group => unimplemented!("comps are not generated by libsubatomic"),
            repomd::DataType::Appstream => write!(w, "</components>"),
        }
    }

    pub fn write_stage1(&self, path: &Path, dt: repomd::DataType) -> Res<repomd::Data> {
        let fd = std::fs::File::create_buffered(path)?;
        let csum = RepoWriterCsum::Sha256(sha2::Sha256::new());
        let mut comp = zstd::Encoder::new(RepoWriterCompInner { fd, csum, .. }, self.zstd_level)?;
        // enable multithread means we separate zstd from hashing, alleviating the bottleneck
        #[allow(clippy::cast_possible_truncation)]
        comp.multithread(self.zstd_multi)?;
        let comp = RepoWriterComp::Zstd(comp);
        let w = RepoWriter { comp, osum: RepoWriterCsum::Sha256(sha2::Sha256::new()), .. };
        let mut disp = RepoWriteDispatcher { dt, w };
        let txn = self.env.read_txn()?;
        let db = match dt {
            repomd::DataType::Primary => &self.db_pri,
            repomd::DataType::Filelists => &self.db_fil,
            repomd::DataType::Other => &self.db_oth,
            repomd::DataType::Group => panic!("do not expect group in stage1"),
            repomd::DataType::Appstream => &self.db_app,
        };
        let l = db.len(&txn)?;
        trace!(count = l, "reading fragments from cache");
        let frags = db.iter(&txn)?.map(|r| r.map(|(_, v)| v));
        self.write_stage1_prexml(disp.dt, &mut disp.w, l)?;

        for frag in frags {
            disp.w.write_all(frag?)?;
        }
        self.write_stage1_postxml(disp.dt, &mut disp.w)?;
        Ok(disp.w.into_data(disp.dt).map(|x| x.0)?)
    }

    /// Write all xml outputs (include repomd), then return the contents of `repomd.xml`.
    ///
    /// The caller should handling signing of the `repomd.xml` file.
    ///
    /// # Panics
    ///
    /// Currently, the function panics if `datatypes` contains [`repomd::DataType::Group`].
    pub fn write_all(&self, datatypes: &[repomd::DataType]) -> Res<Vec<u8>> {
        info!(repodata_dir = %self.repodata_dir.display(), "writing repodata");
        std::fs::create_dir_all(&self.repodata_dir)?;
        let files = datatypes.iter().map(|dt| self.repodata_dir.join(dt.as_str())).collect_vec();
        let data = datatypes.par_iter().copied().zip_eq(&files);
        let data = data.map(|(dt, path)| self.write_stage1(path, dt));
        let mut data = data.collect::<Res<Vec<_>>>()?;
        data.extend(self.read_comps()?);
        for (path, dat) in files.iter().zip(&data) {
            let newname = format!("{}-{}.xml.zst", dat.checksum.sha, dat.r#type);
            std::fs::rename(path, self.repodata_dir.join(newname))?;
        }

        self.write_repomd(data)
    }

    fn write_repomd(&self, data: Vec<repomd::Data>) -> Res<Vec<u8>> {
        debug!("writing repomd");
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
            if let Some(frag) = frag {
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
                new += 1;
            } else {
                cached += 1;
            }
            tracing::trace!(p=%p.display(), "finished");
        } // until recv is closed
        info!("purging old fragments");
        let mut it = self.db_epo.iter_mut(&mut wtxn)?;
        // NOTE: unfortunately we cannot delete items in different dbs in parallel, but fortunately
        // most of the time we don't delete packages.
        let mut purged = Vec::new();
        while let Some(res) = it.next() {
            let (k, v) = res?;
            if v != epoch {
                debug!(old_key = %OsStr::from_bytes(k).display());
                purged.push(k.to_owned());
                // SAFETY: we do not keep any references to any values from this db
                assert!(unsafe { it.del_current()? }, "cannot delete item");
            }
        }
        drop(it);
        for db in [&self.db_pri, &self.db_fil, &self.db_oth, &self.db_app] {
            for k in &purged {
                db.delete(&mut wtxn, k)?;
            }
        }
        wtxn.commit()?;
        Ok((new, cached))
    }
}

pub struct RepoWriter<'a> {
    pub comp: RepoWriterComp<'a>,
    pub osum: RepoWriterCsum,
    pub osize: u64 = 0,
}
impl Write for RepoWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let len = match &mut self.comp {
            RepoWriterComp::Zstd(encoder) => encoder.write(buf)?,
        };
        self.osum.write_all(&buf[..len])?;
        self.osize += len as u64;
        Ok(len)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match &mut self.comp {
            RepoWriterComp::Zstd(encoder) => encoder.flush()?,
        }
        self.osum.flush()?;
        Ok(())
    }

    fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        match &mut self.comp {
            RepoWriterComp::Zstd(encoder) => encoder.write_all(buf)?,
        }
        self.osum.write_all(buf)?;
        self.osize += buf.len() as u64;
        Ok(())
    }
}
impl RepoWriter<'_> {
    /// Finalize, consume self and return [`repomd::Data`] and the inner file.
    ///
    /// # Errors
    /// This propagates errors from the comp encoder finalizing their output.
    pub fn into_data(
        self,
        r#type: repomd::DataType,
    ) -> std::io::Result<(repomd::Data, std::fs::File)> {
        let inner = match self.comp {
            RepoWriterComp::Zstd(encoder) => encoder.finish()?,
        };
        let sha = inner.csum.csum();
        let fd = inner.fd.into_inner().expect("cannot get bufreader inner");
        // TODO: don't hardcode href (esp when comp may be diff)
        Ok((
            repomd::Data {
                location: repomd::Location {
                    href: format!("repodata/{sha}-{type}.xml.zst").into(),
                },
                r#type,
                checksum: repomd::Checksum { sha, .. },
                open_checksum: repomd::Checksum { sha: self.osum.csum(), .. },
                timestamp: fd.metadata()?.st_atime(),
                size: inner.size,
                open_size: self.osize,
            },
            fd,
        ))
    }
}

pub struct RepoWriterCompInner {
    pub fd: std::io::BufWriter<std::fs::File>,
    pub csum: RepoWriterCsum,
    pub size: u64 = 0,
}
impl Write for RepoWriterCompInner {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let len = self.fd.write(buf)?;
        self.csum.write_all(&buf[..len])?;
        self.size += len as u64;
        Ok(len)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.fd.flush()?;
        self.csum.flush()?;
        Ok(())
    }

    fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        self.fd.write_all(buf)?;
        self.csum.write_all(buf)?;
        self.size += buf.len() as u64;
        Ok(())
    }
}

pub enum RepoWriterComp<'a> {
    Zstd(zstd::Encoder<'a, RepoWriterCompInner>),
}

pub enum RepoWriterCsum {
    Sha256(sha2::Sha256),
}
impl Write for RepoWriterCsum {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::Sha256(sha256) => sha256.update(buf),
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }

    fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        self.write(buf)?;
        Ok(())
    }
}
impl RepoWriterCsum {
    #[must_use]
    pub fn csum(self) -> String {
        match self {
            Self::Sha256(sha256) => hex::encode(sha256.finalize()).into(),
        }
    }
}

struct RepoWriteDispatcher<'r> {
    dt: repomd::DataType,
    w: RepoWriter<'r>,
}

#[derive(Debug, Default)]
pub struct Frag(pub Option<Vec<u8>>);
impl std::fmt::Write for Frag {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        let buf = if let Some(buf) = &mut self.0 {
            buf
        } else {
            self.0 = Some(Vec::with_capacity(256));
            self.0.as_mut().unwrap()
        };
        buf.extend(s.as_bytes());
        Ok(())
    }
}

/// Ephemeral fragment struct for sending xml fragments
#[derive(Debug, Default)]
pub struct FragEph {
    pub pri: Frag,
    pub fil: Frag,
    pub oth: Frag,
    pub app: Frag,
}
impl FragEph {
    #[must_use]
    pub fn new(pkg: &crate::pkg::Package, path: &OsStr) -> Self {
        trace!(name = %pkg.name, path = %path.display(), "building cache fragment");
        let mut frag = Self::default();
        frag.gen_pri(pkg, path.as_bytes());
        frag.gen_fil(pkg);
        frag.gen_oth(pkg);
        trace!(name = %pkg.name, "cache fragment complete");
        frag
    }
    fn gen_pri(&mut self, pkg: &crate::pkg::Package, path: &[u8]) {
        trace!(name = %pkg.name, "serializing primary.xml");
        quick_xml::se::to_writer(&mut self.pri, &primary::Package::from_pkg(pkg, path))
            .expect("cannot serialize");
    }

    fn gen_fil(&mut self, pkg: &crate::pkg::Package) {
        trace!(name = %pkg.name, "serializing filelists.xml");
        quick_xml::se::to_writer(&mut self.fil, &filelists::FilelistsPackage::from_pkg(pkg))
            .expect("cannot serialize");
    }

    fn gen_oth(&mut self, pkg: &crate::pkg::Package) {
        trace!(name = %pkg.name, "serializing other.xml");
        quick_xml::se::to_writer(&mut self.oth, &other::OtherPackage::from_pkg(pkg))
            .expect("cannot serialize");
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct RepoCacheFragment {
    pub primary: String,
    pub filelists: String,
    pub other: String,
    pub appstream: Vec<u8>,
    pub epoch: u128,
}

impl RepoCacheFragment {
    #[must_use]
    pub fn new(pkg: &crate::pkg::Package, path: &OsStr, appstream_frag: Vec<u8>) -> Self {
        trace!(name = %pkg.name, path = %path.display(), "building cache fragment");
        let mut frag = Self::default();
        frag.update_primary(pkg, path.as_bytes());
        frag.update_filelists(pkg);
        frag.update_other(pkg);
        frag.appstream = appstream_frag;
        trace!(name = %pkg.name, "cache fragment complete");
        frag
    }
    fn update_primary(&mut self, pkg: &crate::pkg::Package, path: &[u8]) {
        trace!(name = %pkg.name, "serializing primary.xml");
        quick_xml::se::to_writer(&mut self.primary, &primary::Package::from_pkg(pkg, path))
            .expect("cannot serialize");
    }

    fn update_filelists(&mut self, pkg: &crate::pkg::Package) {
        trace!(name = %pkg.name, "serializing filelists.xml");
        quick_xml::se::to_writer(&mut self.filelists, &filelists::FilelistsPackage::from_pkg(pkg))
            .expect("cannot serialize");
    }

    fn update_other(&mut self, pkg: &crate::pkg::Package) {
        trace!(name = %pkg.name, "serializing other.xml");
        quick_xml::se::to_writer(&mut self.other, &other::OtherPackage::from_pkg(pkg))
            .expect("cannot serialize");
    }
}
