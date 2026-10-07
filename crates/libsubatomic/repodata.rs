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

use crate::cache::FragDb;

pub mod metan_prelude {
    pub use super::Metan;
    pub use super::MetanError;
    pub use super::appstream::AppstreamMetan;
    pub use super::filelists::FilelistsMetan;
    pub use super::other::OtherMetan;
    pub use super::primary::PrimaryMetan;

    /// The list of metans passed to [`crate::cache::Cache::new`].
    pub type Metans = Vec<std::sync::Arc<dyn Metan>>;
}

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
}

#[derive(Clone, Debug)]
pub struct MetanReady {
    pub env: std::sync::Arc<heed::Env<heed::WithoutTls>>,
    pub generation: Option<MetanGeneration>,
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
    fn db_init<'s, 't, 'db>(
        &'s self,
        env: std::sync::Arc<heed::Env<heed::WithoutTls>>,
        txn: &'t mut heed::RwTxn<'db>,
    ) -> heed::Result<()>;

    /// Serialize one package's fragment and store it under `pkg.path`.
    fn save<'t, 'db>(
        &self,
        txn: &'t mut heed::RwTxn<'db>,
        pkg: &crate::pkg::MetanInput,
    ) -> Result<(), MetanError>;

    /// Remove the fragment keyed by `path`.
    fn del<'t, 'db>(&self, txn: &'t mut heed::RwTxn<'db>, path: &[u8]) -> heed::Result<()>;

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
    fn on_ready<'db>(&self, ready: MetanReady) -> std::io::Result<Option<repomd::Data>>;

    /// Hook invoked after `repomd.xml` has been written. Useful to create output that depend on it
    /// (e.g. `tetsudou.json`).
    fn on_post_repomd<'db>(
        &self,
        env: std::sync::Arc<heed::Env<heed::WithoutTls>>,
        repomd: &repomd::repomd,
    ) -> std::io::Result<()>;
}
