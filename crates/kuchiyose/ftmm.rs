//! Checksum handling.
//!
//! Checksum types are denoted using [`Ftmm`]. To start hashing, use [`Ftmm::to_digest`] to obtain a
//! [`FtmmDigest`].
//!
//! ふとももprpr
use sha2::Digest;

macro_rules! ftmm {
    ($($item:ident),*$(,)?) => { preinterpret::preinterpret! {
        /// Checksum / Hash type.
        #[non_exhaustive]
        #[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
        #[serde(rename_all = "lowercase")]
        pub enum Ftmm {
            $($item),*
        }

        /// A static dispatcher for the corresponding hasher.
        #[non_exhaustive]
        #[derive(Clone, Debug)]
        pub enum FtmmDigest {
            $($item(sha2::$item)),*
        }

        impl Ftmm {
            /// Create the corresponding hasher.
            pub fn to_digest(&self) -> FtmmDigest {
                match self { $(
                    Self::$item => FtmmDigest::$item(sha2::$item::new()),
                )* }
            }
            pub const fn as_str(&self) -> &'static str {
                match self { $(
                    Self::$item => [!lower! $item],
                )* }
            }
        }

        impl FtmmDigest {
            pub fn update<B: AsRef<[u8]>>(&mut self, data: B) {
                match self { $(
                    Self::$item(digest) => digest.update(data),
                )* }
            }
            pub const fn as_ftmm(&self) -> Ftmm {
                match self { $(
                    Self::$item(_) => Ftmm::$item,
                )* }
            }
            #[must_use]
            pub fn finalize(self) -> Vec<u8> {
                match self { $(
                    Self::$item(digest) => digest.finalize().to_vec(),
                )* }
            }
        }

        impl std::str::FromStr for Ftmm {
            type Err = ParseFtmmErr;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok(match s {
                    $( [!lower! $item] => Self::$item, )*
                    _ => return Err(ParseFtmmErr::Unknown(s.to_owned())),
                })
            }
        }
    }};
}

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum ParseFtmmErr {
    #[error("unknown checksum algorithm: {0}")]
    Unknown(String),
}

ftmm![Sha224, Sha256, Sha384, Sha512, Sha512_224, Sha512_256];
