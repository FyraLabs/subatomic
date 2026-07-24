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
use std::io::Write;
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

    pub fn write<T>(&self, f: impl FnOnce(&RepoCacheDb, &mut heed::RwTxn<'_>) -> T) -> T {
        let mut txn = self.env.write_txn().expect("cannot create rw txn");
        let db = self.env.create_database(&mut txn, Some(&self.repo)).expect("cannot create db");
        let res = f(&db, &mut txn);
        txn.commit().expect("can't commit");
        res
    }

    pub fn read<T>(&self, f: impl FnOnce(&RepoCacheDb, &heed::RoTxn<'_>) -> T) -> T {
        let txn = self.env.read_txn().expect("cannot create rw txn");
        let db = (self.env.open_database(&txn, Some(&self.repo)).expect("cannot open db"))
            .expect("db doesn't exist?");
        f(&db, &txn)
    }

    pub fn insert_fragments<'a, 'b>(
        &self,
        frags: impl IntoIterator<Item = (&'a str, &'b RepoCacheFragment)>,
    ) {
        self.write(move |db, txn| {
            frags
                .into_iter()
                .for_each(|(key, frag)| db.put(txn, key, frag).expect("can't put frag"));
        });
    }

    pub fn insert_pkgs<'a, 'b>(
        &self,
        pkgs: impl IntoIterator<Item = (&'a crate::pkg::Package, &'b Path)>,
    ) {
        self.write(move |db, txn| {
            pkgs.into_iter().for_each(|(pkg, path)| {
                db.put(txn, &pkg.name, &RepoCacheFragment::new(pkg, path)).expect("can't put frag");
            });
        });
    }

    #[must_use]
    pub fn get_fragment(&self, key: &str) -> Option<RepoCacheFragment> {
        self.read(|db, txn| db.get(txn, key).expect("cannot get frag"))
    }

    pub fn write_stage1<'a, 'b>(&self, files: &'a mut [RepoWriter<'b>; 3]) -> std::io::Result<()> {
        RepoWriteDispatcher::dispatch(self, files)
    }

    pub fn write_all(&self, dir: &Path, tempdir: &Path) -> std::io::Result<()> {
        let filenames = ["primary.xml.zst", "filelists.xml.zst", "other.xml.zst"];
        let filepaths = [
            tempdir.join("primary.xml.zst"),
            tempdir.join("filelists.xml.zst"),
            tempdir.join("other.xml.zst"),
        ];
        let make_writer = |p: &Path| {
            std::io::Result::Ok(RepoWriter {
                comp: RepoWriterComp::Zstd(zstd::Encoder::new(
                    std::fs::File::create(p)?,
                    self.zstd_level,
                )?),
                csum: RepoWriterCsum::Sha256(sha2::Sha256::new()),
            })
        };
        // TODO: expand support for more compression & csum formats in [`RepoWriter`]
        let mut files =
            [make_writer(&filepaths[0])?, make_writer(&filepaths[1])?, make_writer(&filepaths[2])?];
        self.write_stage1(&mut files)?;
        let [pri, fil, oth] = files;
        let csums = [pri.into_csum()?, fil.into_csum()?, oth.into_csum()?];
        for ((path, csum), name) in filepaths.iter().zip_eq(&csums).zip_eq(&filenames) {
            let mut new_name = std::ffi::OsString::from(csum.as_str());
            new_name.push("-");
            new_name.push(name);
            std::fs::rename(path, dir.join(new_name))?;
        }
        let [psum, fsum, osum] = csums;
        // TODO: how do we get the normal checksum? we only have the open checksum

        Ok(())
    }
}

pub struct RepoWriter<'a> {
    pub comp: RepoWriterComp<'a>,
    pub csum: RepoWriterCsum,
}
impl Write for RepoWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let len = match &mut self.comp {
            RepoWriterComp::Zstd(encoder) => encoder.write(buf)?,
        };
        self.csum.write_all(&buf[..len])?;
        Ok(len)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match &mut self.comp {
            RepoWriterComp::Zstd(encoder) => encoder.flush()?,
        }
        self.csum.flush()?;
        Ok(())
    }

    fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        match &mut self.comp {
            RepoWriterComp::Zstd(encoder) => encoder.write_all(buf)?,
        }
        self.csum.write_all(&buf)?;
        Ok(())
    }
}
impl RepoWriter<'_> {
    pub fn into_csum(self) -> std::io::Result<String> {
        match self.comp {
            RepoWriterComp::Zstd(encoder) => encoder.finish()?,
        };
        Ok(self.csum.csum())
    }
}

pub enum RepoWriterComp<'a> {
    Zstd(zstd::Encoder<'a, std::fs::File>),
}

pub enum RepoWriterCsum {
    Sha256(sha2::Sha256),
}
impl Write for RepoWriterCsum {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            RepoWriterCsum::Sha256(sha256) => sha256.update(buf),
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

pub enum RepoWriteDispatcher<'f, 'r> {
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
    pub fn new(pkg: &crate::pkg::Package, path: &Path) -> Self {
        let mut frag = Self::default();
        frag.update_primary(pkg, path);
        frag.update_filelists(pkg);
        frag.update_other(pkg);
        frag
    }
    pub fn update_primary(&mut self, pkg: &crate::pkg::Package, path: &Path) {
        quick_xml::se::to_writer(&mut self.primary, &primary::Package::from_pkg(pkg, path))
            .expect("cannot serialize");
    }

    pub fn update_filelists(&mut self, pkg: &crate::pkg::Package) {
        quick_xml::se::to_writer(&mut self.filelists, &filelists::FilelistsPackage::from_pkg(pkg))
            .expect("cannot serialize");
    }

    pub fn update_other(&mut self, pkg: &crate::pkg::Package) {
        quick_xml::se::to_writer(&mut self.other, &other::OtherPackage::from_pkg(pkg))
            .expect("cannot serialize");
    }
}
