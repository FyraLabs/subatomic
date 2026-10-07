#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("heed/lmdb cache error: {0}")]
    Heed(#[from] heed::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("rpm error: {0}")]
    Rpm(#[from] rpm::Error),
    #[error("pgp error: {0}")]
    Pgp(#[from] pgp::errors::Error),
    #[error("xml serialization error: {0}")]
    XmlSe(#[from] quick_xml::SeError),
    #[error("kuchiyose store error: {0}")]
    Kuchiyose(#[from] kuchiyose::store::StoreErr),
    #[error("metadata error: {0}")]
    Metan(#[from] crate::repodata::MetanError),
    #[error("store error: {0}")]
    Store(#[from] object_store::Error),
}

pub type Res<T> = Result<T, Error>;
