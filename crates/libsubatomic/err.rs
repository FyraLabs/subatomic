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
}

pub type Res<T> = Result<T, Error>;
