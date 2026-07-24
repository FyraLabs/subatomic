//! Repodata generation and XML type definitions
//!
//! This module contains repodata XML type definitions and the helper functions required to generate
//! those XML files.

pub mod filelists;
pub mod other;
pub mod primary;
pub mod repomd;

use crate::prelude::*;
use std::io::Write;
use std::path::Path;

pub type RepoCacheDb =
    heed::Database<heed::types::Str, heed::types::SerdeBincode<RepoCacheFragment>>;

#[derive(Clone, Debug)]
pub struct RepoCache {
    repo: String,
    env: heed::Env<heed::WithoutTls>,
}

impl RepoCache {
    pub fn new(repo: &str, path: &Path) -> heed::Result<Self> {
        // SAFETY: assume this file is not modified concurrently
        let env = unsafe {
            heed::EnvOpenOptions::new()
            .read_txn_without_tls()
            .map_size(1 * 1024 * 1024 * 1024) // alloc 1 GB
            .max_dbs(10)
            .open(path)
        }?;
        Ok(Self { repo: repo.into(), env })
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

    pub fn insert_fragments<'a>(
        &self,
        key: &str,
        frags: impl IntoIterator<Item = &'a RepoCacheFragment>,
    ) {
        self.write(move |db, txn| {
            frags.into_iter().for_each(|frag| db.put(txn, key, frag).expect("can't put frag"));
        });
    }

    pub fn get_fragment(&self, key: &str) -> Option<RepoCacheFragment> {
        self.read(|db, txn| db.get(txn, key).expect("cannot get frag"))
    }

    pub fn write_all(&self, files: [&mut std::fs::File; 3]) -> std::io::Result<()> {
        RepoWriteDispatcher::dispatch(self, files)
    }
}

pub enum RepoWriteDispatcher<'f> {
    Primary(&'f mut std::fs::File),
    Filelists(&'f mut std::fs::File),
    Other(&'f mut std::fs::File),
}
impl<'f> RepoWriteDispatcher<'f> {
    fn dispatch(
        repocache: &RepoCache,
        [pri, fil, oth]: [&'f mut std::fs::File; 3],
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
        file: &mut std::fs::File,
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
        file: &mut std::fs::File,
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
        file: &mut std::fs::File,
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
