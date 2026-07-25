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

pub type RepoCacheDb =
    heed::Database<heed::types::Str, heed::types::SerdeBincode<RepoCacheFragment>>;

#[derive(Clone, Debug)]
pub struct RepoCache {
    pub repo: String,
    pub env: heed::Env<heed::WithoutTls>,
    pub zstd_level: i32 = 0,
}

impl RepoCache {
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
    pub fn new(repo: &str, path: &Path) -> heed::Result<Self> {
        // SAFETY: assume this file is not modified concurrently
        let env =
            unsafe { heed::EnvOpenOptions::new().read_txn_without_tls().max_dbs(1).open(path) }?;
        Ok(Self { repo: repo.into(), env, .. })
    }

    fn write<T>(&self, f: impl FnOnce(&RepoCacheDb, &mut heed::RwTxn<'_>) -> T) -> T {
        let mut txn = self.env.write_txn().expect("cannot create rw txn");
        let db = self.env.create_database(&mut txn, Some(&self.repo)).expect("cannot create db");
        let res = f(&db, &mut txn);
        txn.commit().expect("can't commit");
        res
    }

    fn read<T>(&self, f: impl FnOnce(&RepoCacheDb, &heed::RoTxn<'_>) -> T) -> T {
        let txn = self.env.read_txn().expect("cannot create rw txn");
        let db = (self.env.open_database(&txn, Some(&self.repo)).expect("cannot open db"))
            .expect("db doesn't exist?");
        f(&db, &txn)
    }

    /// Insert packages (not in parallel!) into the cache.
    ///
    /// # Errors
    /// This propagates errors from [`heed::Database::put`].
    pub fn insert_pkgs<'a, 'b, I: IntoIterator<Item = (&'a crate::pkg::Package, &'b Path)>>(
        &self,
        pkgs: I,
    ) -> heed::Result<()> {
        self.write(move |db, txn| {
            pkgs.into_iter().try_for_each(|(pkg, path)| {
                db.put(txn, &pkg.name, &RepoCacheFragment::new(pkg, path))
            })
        })
    }

    #[must_use]
    pub fn get_fragment(&self, key: &str) -> Option<RepoCacheFragment> {
        self.read(|db, txn| db.get(txn, key).expect("cannot get frag"))
    }

    #[inline]
    fn write_stage1(&self, files: &mut [RepoWriter<'_>; 3]) -> std::io::Result<()> {
        RepoWriteDispatcher::dispatch(self, files)
    }

    pub fn write_all(&self, dir: &Path, tempdir: &Path) -> std::io::Result<()> {
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
        let [pri, fil, oth] = files;
        let data = [
            pri.into_data(repomd::DataType::Primary)?,
            fil.into_data(repomd::DataType::Filelists)?,
            oth.into_data(repomd::DataType::Other)?,
        ];
        // can safely rename since fds are dropped via into_data()
        for (path, dat) in filepaths.iter().zip_eq(&data) {
            let newname = format!("{}-{}.xml.zst", dat.checksum.sha, dat.r#type);
            std::fs::rename(path, dir.join(newname))?;
        }

        let mut fd_repomd = std::fs::File::create(dir.join("repomd.xml"))?;
        repomd::Repomd::generate(&mut fd_repomd, data.into_iter().collect())
            .expect("cannot write to repomd");

        let pos = fd_repomd.stream_position()?;
        fd_repomd.seek(std::io::SeekFrom::Start(0))?;
        let mut buf = Vec::with_capacity(pos as usize);
        fd_repomd.read_to_end(&mut buf)?;
        // TODO: sign

        Ok(())
    }
}

pub struct RepoWriter<'a> {
    comp: RepoWriterComp<'a>,
    osum: RepoWriterCsum,
    osize: u64 = 0,
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
    /// Finalize, consume self and return [`repomd::Data`].
    ///
    /// # Errors
    /// This propagates errors from the comp encoder finalizing their output.
    pub fn into_data(self, r#type: repomd::DataType) -> std::io::Result<repomd::Data> {
        let inner = match self.comp {
            RepoWriterComp::Zstd(encoder) => encoder.finish()?,
        };
        let sha = inner.csum.csum();
        // TODO: don't hardcode href (esp when comp may be diff)
        Ok(repomd::Data {
            location: repomd::Location { href: format!("repodata/{sha}-{type}.xml.zst").into() },
            r#type,
            checksum: repomd::Checksum { sha, .. },
            open_checksum: repomd::Checksum { sha: self.osum.csum(), .. },
            timestamp: inner.fd.metadata()?.st_atime(),
            size: inner.size,
            open_size: self.osize,
        })
    }
}

struct RepoWriterCompInner {
    fd: std::fs::File,
    csum: RepoWriterCsum,
    size: u64 = 0,
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

enum RepoWriterComp<'a> {
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
        [Self::Primary(pri), Self::Filelists(fil), Self::Other(oth)]
            .into_par_iter()
            .try_for_each(|mut disp| repocache.read(|db, txn| disp.process(db, txn)))
    }

    fn process(&mut self, db: &RepoCacheDb, txn: &heed::RoTxn<'_>) -> Result<(), std::io::Error> {
        let l = db.len(txn).expect("can't get db len");
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
            write!(file, "{}", frag.primary)?;
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
            write!(file, "{}", frag.filelists)?;
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
            write!(file, "{}", frag.other)?;
        }
        write!(file, "</otherdata>")?;
        Ok(())
    }
}

#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
pub struct RepoCacheFragment {
    pub primary: String,
    pub filelists: String,
    pub other: String,
}

impl RepoCacheFragment {
    #[must_use]
    fn new(pkg: &crate::pkg::Package, path: &Path) -> Self {
        let mut frag = Self::default();
        frag.update_primary(pkg, path);
        frag.update_filelists(pkg);
        frag.update_other(pkg);
        frag
    }
    fn update_primary(&mut self, pkg: &crate::pkg::Package, path: &Path) {
        quick_xml::se::to_writer(&mut self.primary, &primary::Package::from_pkg(pkg, path))
            .expect("cannot serialize");
    }

    fn update_filelists(&mut self, pkg: &crate::pkg::Package) {
        quick_xml::se::to_writer(&mut self.filelists, &filelists::FilelistsPackage::from_pkg(pkg))
            .expect("cannot serialize");
    }

    fn update_other(&mut self, pkg: &crate::pkg::Package) {
        quick_xml::se::to_writer(&mut self.other, &other::OtherPackage::from_pkg(pkg))
            .expect("cannot serialize");
    }
}
