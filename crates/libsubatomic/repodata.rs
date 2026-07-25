//! Repodata generation and XML type definitions
//!
//! This module contains repodata XML type definitions and the helper functions required to generate
//! those XML files.

pub mod filelists;
pub mod other;
pub mod primary;
pub mod repomd;

use sha2::Digest;

use crate::prelude::*;
use std::io::{Read, Seek, Write};
use std::os::linux::fs::MetadataExt;
use std::path::Path;
use tracing::{debug, info, trace, warn};

pub type RepoCacheDb =
    heed::Database<heed::types::Str, heed::types::SerdeBincode<RepoCacheFragment>>;

#[derive(Clone, Debug)]
pub struct RepoCache {
    pub repo: String,
    pub cachedir: std::path::PathBuf,
    pub dir: std::path::PathBuf,
    pub env: heed::Env<heed::WithoutTls>,
    pub zstd_level: i32 = 0,
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
    pub fn new(repo: &str, cachedir: &Path, dir: &Path) -> heed::Result<Self> {
        debug!(repo, cachedir = %cachedir.display(), dir = %dir.display(), "opening cache");
        let path = cachedir.join(repo);
        let cachedir = cachedir.to_owned();
        let dir = dir.to_owned();
        // SAFETY: assume this file is not modified concurrently
        let env = unsafe {
            heed::EnvOpenOptions::new()
                .read_txn_without_tls()
                .max_dbs(1)
                .map_size(Self::DEFAULT_MAP_SIZE)
                .open(path)?
        };
        trace!(repo, "cache opened");
        Ok(Self { repo: repo.into(), env, cachedir, dir, .. })
    }

    fn write<T>(
        &self,
        f: impl FnOnce(&RepoCacheDb, &mut heed::RwTxn<'_>) -> heed::Result<T>,
    ) -> heed::Result<T> {
        trace!(repo = %self.repo, "starting write txn");
        let mut txn = self.env.write_txn().expect("cannot create rw txn");
        let db = self.env.create_database(&mut txn, Some(&self.repo)).expect("cannot create db");
        match f(&db, &mut txn) {
            Ok(v) => {
                txn.commit().expect("can't commit");
                trace!(repo = %self.repo, "write txn committed");
                Ok(v)
            }
            Err(e) => {
                warn!(repo = %self.repo, error = %e, "write txn failed, aborting");
                txn.abort();
                Err(e)
            }
        }
    }

    fn read<T>(&self, f: impl FnOnce(&RepoCacheDb, &heed::RoTxn<'_>) -> T) -> T {
        trace!(repo = %self.repo, "starting read txn");
        let txn = self.env.read_txn().expect("cannot create rw txn");
        let db = (self.env.open_database(&txn, Some(&self.repo)).expect("cannot open db"))
            .expect("db doesn't exist?");
        let res = f(&db, &txn);
        trace!(repo = %self.repo, "read txn finished");
        res
    }

    pub fn write_comps(&self, data: &repomd::Data) -> heed::Result<()> {
        let mut txn = self.env.write_txn()?;
        let db: heed::Database<heed::types::Str, heed::types::SerdeBincode<repomd::Data>> =
            self.env.create_database(&mut txn, Some("comps"))?;
        db.put(&mut txn, "compsdata", data)?;
        txn.commit()?;
        Ok(())
    }
    pub fn read_comps(&self) -> heed::Result<Option<repomd::Data>> {
        let txn = self.env.read_txn()?;
        let Some(db): Option<
            heed::Database<heed::types::Str, heed::types::SerdeBincode<repomd::Data>>,
        > = self.env.open_database(&txn, Some("comps"))?
        else {
            return Ok(None);
        };
        db.get(&txn, "compsdata")
    }
    pub fn del_comps(&self) -> heed::Result<bool> {
        let mut txn = self.env.write_txn()?;
        let db: heed::Database<heed::types::Str, heed::types::SerdeBincode<repomd::Data>> =
            self.env.create_database(&mut txn, Some("comps"))?;
        let existed = db.delete(&mut txn, "compsdata")?;
        txn.commit()?;
        Ok(existed)
    }

    /// Insert packages into the cache.
    ///
    /// Fragments are generated in parallel via rayon, then written serially
    /// to LMDB in a single write transaction.
    ///
    /// Keys are derived from the full NEVRA so multiple versions of the same
    /// package name can coexist in the cache.
    ///
    /// # Errors
    /// This propagates errors from [`heed::Database::put`].
    pub fn insert_pkgs<'a, 'b, I: IntoIterator<Item = (&'a crate::pkg::Package, &'b OsStr)>>(
        &self,
        pkgs: I,
    ) -> heed::Result<()> {
        let pkgs: Vec<_> = pkgs.into_iter().collect();
        info!(count = pkgs.len(), "generating xml fragments in parallel");
        let fragments: Vec<(std::string::String, RepoCacheFragment)> = pkgs
            .into_par_iter()
            .map(|(pkg, path)| {
                trace!(name = %pkg.name, "serializing package");
                let key = format!(
                    "{}-{}:{}-{}.{}",
                    pkg.name, pkg.version.epoch, pkg.version.ver, pkg.version.rel, pkg.arch
                );
                (key, RepoCacheFragment::new(pkg, path))
            })
            .collect();
        trace!(count = fragments.len(), "writing fragments to cache");
        self.write(move |db, txn| {
            fragments.into_iter().try_for_each(|(key, frag)| db.put(txn, &key, &frag))
        })
    }

    /// Check whether a key already exists in the cache.
    #[must_use]
    pub fn has(&self, key: &str) -> bool {
        trace!(key, "checking cache key");
        self.read(|db, txn| db.get(txn, key).expect("cannot check key")).is_some()
    }

    /// Insert a single package fragment under an explicit key.
    ///
    /// # Errors
    /// This propagates errors from [`heed::Database::put`].
    pub fn insert(&self, key: &str, pkg: &crate::pkg::Package, path: &OsStr) -> heed::Result<()> {
        trace!(key, path = %path.display(), "inserting single package");
        self.write(|db, txn| db.put(txn, key, &RepoCacheFragment::new(pkg, path)))
    }

    /// Return the number of cached fragments.
    #[must_use]
    pub fn len(&self) -> u64 {
        self.read(|db, txn| db.len(txn).expect("cannot get db len"))
    }

    pub fn delete_pkgs<'a>(&self, pkgs: &'a [&'a str]) -> heed::Result<Vec<&'a &'a str>> {
        self.write(move |db, txn| {
            pkgs.into_iter()
                .map(|key| Ok((!db.delete(txn, key)?).then_some(key)))
                .filter_map_ok(|f| f)
                .try_collect()
        })
    }

    #[must_use]
    pub fn get_fragment(&self, key: &str) -> Option<RepoCacheFragment> {
        self.read(|db, txn| db.get(txn, key).expect("cannot get frag"))
    }

    /// Collect every key currently stored in the cache.
    pub fn keys(&self) -> Vec<std::string::String> {
        debug!(repo = %self.repo, "listing cache keys");
        self.read(|db, txn| {
            let mut out = Vec::with_capacity(db.len(txn).unwrap_or(0) as usize);
            for item in db.iter(txn).expect("cannot iterate db") {
                let (k, _v) = item.expect("cannot read db item");
                out.push(k.into());
            }
            trace!(count = out.len(), "collected cache keys");
            out
        })
    }

    /// Remove a single key from the cache.
    ///
    /// # Errors
    /// This propagates errors from [`heed::Database::delete`].
    pub fn remove(&self, key: &str) -> heed::Result<bool> {
        trace!(key, "removing cache key");
        self.write(|db, txn| db.delete(txn, key))
    }

    /// Delete every key not present in `expected`.
    ///
    /// Returns the number of removed entries.
    ///
    /// # Errors
    /// This propagates errors from LMDB write operations.
    pub fn prune(&self, expected: &std::collections::HashSet<&str>) -> heed::Result<u64> {
        let to_remove: Vec<_> =
            self.keys().into_iter().filter(|k| !expected.contains(k.as_str())).collect();
        debug!(stale = to_remove.len(), "pruning cache");
        let mut count = 0u64;
        for key in to_remove {
            trace!(key, "pruning stale key");
            if self.write(|db, txn| db.delete(txn, &key))? {
                count += 1;
            }
        }
        if count > 0 {
            info!(removed = count, "cache pruned");
        }
        Ok(count)
    }

    /// Compact the underlying LMDB file by writing a fresh copy and swapping it in.
    ///
    /// This consumes `self` so the environment can be closed before the file is replaced.
    ///
    /// # Errors
    /// Propagates IO errors from copying/renaming, and [`heed`] errors from re-opening.
    pub fn compact(self) -> heed::Result<Self> {
        let repo = self.repo.clone();
        let cachedir = self.cachedir.clone();
        let dir = self.dir.clone();
        let zstd = self.zstd_level;
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
        let _ = std::fs::remove_file(&old_file);

        let mut new = Self::new(&repo, &cachedir, &dir)?;
        new.zstd_level = zstd;
        info!(dir = %env_dir.display(), "cache compacted");
        Ok(new)
    }

    #[inline]
    fn write_stage1(&self, files: &mut [RepoWriter<'_>; 3]) -> std::io::Result<()> {
        RepoWriteDispatcher::dispatch(self, files)
    }

    /// Write all xml outputs (include repomd), then return the contents of `repomd.xml`.
    ///
    /// The caller should handling signing of the `repomd.xml` file.
    pub fn write_all(&self, tempdir: &Path) -> std::io::Result<Vec<u8>> {
        info!(dir = %self.dir.display(), tempdir = %tempdir.display(), "writing repodata");
        let filepaths = [
            tempdir.join("primary.xml.zst"),
            tempdir.join("filelists.xml.zst"),
            tempdir.join("other.xml.zst"),
        ];
        let make_writer = |p: &Path| {
            std::io::Result::Ok(RepoWriter {
                comp: RepoWriterComp::Zstd(zstd::Encoder::new(
                    RepoWriterCompInner {
                        fd: std::fs::File::create(p)?,
                        csum: RepoWriterCsum::Sha256(sha2::Sha256::new()),
                        ..
                    },
                    self.zstd_level,
                )?),
                osum: RepoWriterCsum::Sha256(sha2::Sha256::new()),
                ..
            })
        };
        // TODO: expand support for more compression & csum formats in [`RepoWriter`]
        let mut files =
            [make_writer(&filepaths[0])?, make_writer(&filepaths[1])?, make_writer(&filepaths[2])?];
        self.write_stage1(&mut files)?;
        let mut data: Vec<_> = files
            .into_iter()
            .zip_eq([
                repomd::DataType::Primary,
                repomd::DataType::Filelists,
                repomd::DataType::Other,
            ])
            .map(|(f, dt)| f.into_data(dt).map(|x| x.0))
            .try_collect()?;
        data.extend(self.read_comps().expect("cannot read comps db"));
        // can safely rename since fds are dropped via into_data()
        for (path, dat) in filepaths.iter().zip_eq(&data) {
            let newname = format!("{}-{}.xml.zst", dat.checksum.sha, dat.r#type);
            std::fs::rename(path, self.dir.join(newname))?;
        }

        Self::write_repomd(&self.dir, data.to_vec())
    }

    fn write_repomd(dir: &Path, data: Vec<repomd::Data>) -> std::io::Result<Vec<u8>> {
        let mut fd_repomd = std::fs::File::create(dir.join("repomd.xml"))?;
        repomd::Repomd::generate(&mut fd_repomd, data).expect("cannot write to repomd");

        let pos = fd_repomd.stream_position()?;
        fd_repomd.seek(std::io::SeekFrom::Start(0))?;
        #[allow(clippy::cast_possible_truncation)] // same behaviour even on 32-bit platforms
        let mut buf = Vec::with_capacity(pos as usize);
        fd_repomd.read_to_end(&mut buf)?;

        Ok(buf)
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
        // TODO: don't hardcode href (esp when comp may be diff)
        Ok((
            repomd::Data {
                location: repomd::Location {
                    href: format!("repodata/{sha}-{type}.xml.zst").into(),
                },
                r#type,
                checksum: repomd::Checksum { sha, .. },
                open_checksum: repomd::Checksum { sha: self.osum.csum(), .. },
                timestamp: inner.fd.metadata()?.st_atime(),
                size: inner.size,
                open_size: self.osize,
            },
            inner.fd,
        ))
    }
}

pub struct RepoWriterCompInner {
    pub fd: std::fs::File,
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

enum RepoWriteDispatcher<'f, 'r> {
    Primary(&'f mut RepoWriter<'r>),
    Filelists(&'f mut RepoWriter<'r>),
    Other(&'f mut RepoWriter<'r>),
}
impl<'f, 'r> RepoWriteDispatcher<'f, 'r> {
    fn dispatch(
        repocache: &RepoCache,
        [pri, fil, oth]: &'f mut [RepoWriter<'r>; 3],
    ) -> std::io::Result<()> {
        debug!("dispatching parallel xml writes");
        [Self::Primary(pri), Self::Filelists(fil), Self::Other(oth)]
            .into_par_iter()
            .try_for_each(|mut disp| repocache.read(|db, txn| disp.process(db, txn)))
    }

    fn process(&mut self, db: &RepoCacheDb, txn: &heed::RoTxn<'_>) -> Result<(), std::io::Error> {
        let l = db.len(txn).expect("can't get db len");
        trace!(count = l, "reading fragments from cache");
        let frags = db.iter(txn).expect("can't iter frags");
        let frags = frags.into_iter().map(|r| r.expect("can't get item in iter").1);
        match self {
            Self::Primary(file) => Self::write_primary(l, frags, file),
            Self::Filelists(file) => Self::write_filelists(l, frags, file),
            Self::Other(file) => Self::write_other(l, frags, file),
        }
    }

    fn write_primary(
        l: u64,
        frags: impl Iterator<Item = RepoCacheFragment>,
        file: &mut RepoWriter<'_>,
    ) -> Result<(), std::io::Error> {
        write!(
            file,
            r#"<?xml version="1.0" encoding="UTF-8"?><metadata xmlns="http://linux.duke.edu/metadata/common" xmlns:rpm="http://linux.duke.edu/metadata/rpm" packages="{l}">"#
        )?;
        for frag in frags {
            file.write_all(frag.primary.as_bytes())?;
        }
        write!(file, "</metadata>")?;
        Ok(())
    }
    fn write_filelists(
        l: u64,
        frags: impl Iterator<Item = RepoCacheFragment>,
        file: &mut RepoWriter<'_>,
    ) -> Result<(), std::io::Error> {
        write!(
            file,
            r#"<?xml version="1.0" encoding="UTF-8"?><filelists xmlns="http://linux.duke.edu/metadata/filelists" packages="{l}">"#
        )?;
        for frag in frags {
            file.write_all(frag.filelists.as_bytes())?;
        }
        write!(file, "</filelists>")?;
        Ok(())
    }
    fn write_other(
        l: u64,
        frags: impl Iterator<Item = RepoCacheFragment>,
        file: &mut RepoWriter<'_>,
    ) -> Result<(), std::io::Error> {
        write!(
            file,
            r#"<?xml version="1.0" encoding="UTF-8"?><otherdata xmlns="http://linux.duke.edu/metadata/other" packages="{l}">"#
        )?;
        for frag in frags {
            file.write_all(frag.other.as_bytes())?;
        }
        write!(file, "</otherdata>")?;
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct RepoCacheFragment {
    pub primary: String,
    pub filelists: String,
    pub other: String,
}

impl RepoCacheFragment {
    #[must_use]
    pub fn new(pkg: &crate::pkg::Package, path: &OsStr) -> Self {
        trace!(name = %pkg.name, path = %path.display(), "building cache fragment");
        let mut frag = Self::default();
        frag.update_primary(pkg, path);
        frag.update_filelists(pkg);
        frag.update_other(pkg);
        trace!(name = %pkg.name, "cache fragment complete");
        frag
    }
    fn update_primary(&mut self, pkg: &crate::pkg::Package, path: &OsStr) {
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
