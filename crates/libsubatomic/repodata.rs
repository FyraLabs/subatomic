//! Repodata generation and XML type definitions.
//!
//! This module contains the repodata XML type definitions and the [`Metan`] trait, which
//! encapsulates everything the generator needs to know about a single repomd datatype:
//! its `type="…"` attribute, its output filename, its LMDB database, how to serialize a
//! package into an XML fragment, and how to write the final XML document.
//!
//! The main entry point is [`crate::cache::Cache`], which owns the LMDB environment and a
//! vector of [`Metan`] trait objects.

pub mod appstream;
pub mod filelists;
pub mod other;
pub mod primary;
pub mod repomd;

use crate::prelude::*;

use crate::cache::FragDb;

pub mod metan_prelude {
    pub use super::Metan;
    pub use super::MetanComputed;
    pub use super::MetanError;
    pub use super::appstream::AppstreamMetan;
    pub use super::filelists::FilelistsMetan;
    pub use super::other::OtherMetan;
    pub use super::primary::PrimaryMetan;

    /// The list of metans passed to [`crate::cache::Cache::new`].
    pub type Metans = Vec<std::sync::Arc<dyn Metan>>;
}

pub struct MetanComputed(Box<dyn std::any::Any + Send>);

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum MetanError {
    #[error("heed/lmdb error: {source}")]
    Heed {
        #[from]
        source: heed::Error,
        backtrace: std::backtrace::Backtrace,
    },
    #[error("io error: {source}")]
    Io {
        #[from]
        source: std::io::Error,
        backtrace: std::backtrace::Backtrace,
    },
    #[error("rpm error: {0}")]
    Rpm(Box<rpm::Error>), // was too big according to clippy
    #[error("xml serialization error: {source}")]
    XmlSe {
        #[from]
        source: quick_xml::SeError,
        backtrace: std::backtrace::Backtrace,
    },
}
impl From<rpm::Error> for MetanError {
    fn from(value: rpm::Error) -> Self {
        Self::Rpm(Box::new(value))
    }
}

/// Compression + checksum results for a single metan's output.
///
/// Filled in by [`crate::cache::Cache::write_one`] and handed to [`Metan::on_ready`], which
/// turns it into the `<data>` element for `repomd.xml`.
#[derive(Clone, Debug)]
pub struct MetanGeneration {
    pub csum: repomd::Checksum,
    pub osum: repomd::Checksum,
    pub comp_ext: String,
    pub timestamp: i64,
    pub size: u64,
    pub open_size: u64,

    /// Force struct constructions to use the `MyStruct { fields, .. }` notation.
    #[expect(private_interfaces)]
    pub non_exhaustive: crate::NonExhaustive = crate::NonExhaustive,
}

#[derive(Clone, Debug)]
pub struct MetanReady {
    pub env: std::sync::Arc<heed::Env<heed::WithoutTls>>,
    pub generation: Option<MetanGeneration>,

    /// Force struct constructions to use the `MyStruct { fields, .. }` notation.
    #[expect(private_interfaces)]
    pub non_exhaustive: crate::NonExhaustive = crate::NonExhaustive,
}

/// One repomd datatype.
///
/// Implementors own their own LMDB database (created in [`Metan::db_init`]) and are fully
/// responsible for the shape of the XML they emit. The orchestrator ([`crate::cache::Cache`])
/// only knows about the trait; it never assumes a fixed set of datatypes.
#[async_trait::async_trait]
pub trait Metan: std::fmt::Debug + Send + Sync {
    /// The literal value emitted as `type="…"` in `repomd.xml`.
    ///
    /// For the standard datatypes this is `"primary"`, `"filelists"`, `"other"`, `"appstream"`.
    fn mdtype(&self) -> &str;

    /// The filename stem used as `repodata/{checksum}-{filename}.xml.{ext}`.
    ///
    /// Usually identical to [`Metan::mdtype`], but kept separate so that e.g. a future
    /// zchunk variant can share a filename while using a distinct `type=` value.
    fn filename(&self) -> &str;

    /// Number of LMDB databases this metan requires. Used to size `max_dbs` on the env.
    fn db_count(&self) -> u32;

    /// Open (or create) this metan's LMDB database(s) inside `txn`.
    fn db_init(
        &self,
        env: std::sync::Arc<heed::Env<heed::WithoutTls>>,
        txn: &mut heed::RwTxn<'_>,
    ) -> heed::Result<()>;

    /// Compute the fragment. The output will be sent to [`Self::save()`].
    fn compute(&self, pkg: &crate::pkg::MetanInput) -> Result<MetanComputed, MetanError>;

    /// Save the computed input to database.
    ///
    /// Implementations must be _idempotent_: calling it once is no different from calling it
    /// several times successively (there are no side effects).
    fn save(&self, txn: &mut heed::RwTxn<'_>, computed: &MetanComputed) -> Result<(), MetanError>;

    /// Remove the fragment keyed by `path`.
    ///
    /// Implementations must be _idempotent_: calling it once is no different from calling it
    /// several times successively (there are no side effects).
    fn del(&self, txn: &mut heed::RwTxn<'_>, path: &crate::Kiri) -> heed::Result<()>;

    /// Stream the full XML document to `w`.
    ///
    /// The writer is the head of a compression + hashing pipeline, so implementors should
    /// simply `write_all` their envelope and each cached fragment in order.
    async fn on_generate<'t, 'db>(
        &self,
        env: std::sync::Arc<heed::Env<heed::WithoutTls>>,
        w: std::pin::Pin<Box<dyn tokio::io::AsyncWrite + Send + 't>>,
    ) -> Result<(), MetanError>;

    /// Produce the `<data>` entry for `repomd.xml`.
    ///
    /// Returns `Ok(None)` if the metan produced no output (e.g. an appstream file with zero
    /// components). The default impls all return `Some(…)`.
    fn on_ready(&self, ready: MetanReady) -> std::io::Result<Option<repomd::Data>>;

    // TODO: actually invoke this hook
    /// Hook invoked after `repomd.xml` has been written. Useful to create output that depend on it
    /// (e.g. `tetsudou.json`).
    fn on_post_repomd<'db>(
        &self,
        env: std::sync::Arc<heed::Env<heed::WithoutTls>>,
        repomd: &repomd::repomd,
    ) -> std::io::Result<()>;
}

/// RPM Metadata to be fed into [`crate::repodata::Metan`].
#[derive(Debug)]
pub struct MetanInput {
    pub metadata: rpm::PackageMetadata,
    pub fmeta: std::fs::Metadata,
    pub csum: String,
    /// Repository-relative path, used as the `<location href>` in `primary.xml`.
    pub link: kuchiyose::link::LinkBuf,
    /// The RPM filename (last path component), used as the cache key in every
    /// per-metan database and in the epoch database.
    pub filename: crate::Kirifuda,
    pub csum_type: kuchiyose::ftmm::Ftmm,
    /// Absolute path to the RPM on disk.
    pub path: std::path::PathBuf,

    /// Force struct constructions to use the `MyStruct { fields, .. }` notation.
    #[expect(private_interfaces)]
    pub non_exhaustive: crate::NonExhaustive = crate::NonExhaustive,
}
impl MetanInput {
    /// Reopen the rpm archive for streaming reads (e.g. appstream).
    ///
    /// # Errors
    /// Propagates IO and rpm parse errors.
    pub fn reader(&self) -> Result<rpm::PackageReader, rpm::Error> {
        rpm::PackageReader::open(&self.path)
    }
}

/// Helper for accessing databases used in [`Metan`] modules.
#[derive(Debug)]
pub struct MetanDb<T> {
    id: std::borrow::Cow<'static, str>,
    db: std::sync::OnceLock<std::sync::Arc<T>>,
}
// impl<K, V> Default for MetanDb<heed::Database<K, V>> {
//     fn default() -> Self {
//         Self { id: String::new(), db: std::sync::OnceLock::new() }
//     }
// }
impl<K: 'static, V: 'static> MetanDb<heed::Database<K, V>> {
    const fn new(id: &'static str) -> Self {
        Self { id: std::borrow::Cow::Borrowed(id), db: std::sync::OnceLock::new() }
    }
    fn init(
        &self,
        env: impl AsRef<heed::Env<heed::WithoutTls>>,
        txn: &mut heed::RwTxn<'_>,
    ) -> heed::Result<()> {
        self.db
            .set(std::sync::Arc::new(env.as_ref().create_database(txn, Some(&self.id))?))
            .expect("double db_init");
        Ok(())
    }
    fn arc(&self) -> std::sync::Arc<heed::Database<K, V>> {
        std::sync::Arc::clone(self.db.get().expect("db uninit"))
    }
}
impl<K: 'static, V: 'static> std::ops::Deref for MetanDb<heed::Database<K, V>> {
    type Target = heed::Database<K, V>;

    fn deref(&self) -> &Self::Target {
        self.db.get().expect("db uninit")
    }
}
