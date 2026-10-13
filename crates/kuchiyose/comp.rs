//! Helpers related to compression.
//!
//! This modules provides the [`CompConfigure`] trait and a static dispatch of all officially
//! supported compression algorithms via the [`CompConfig`] enum.
//!
//! [`Mochi`] also resides in this module, which is a writer that performs hashing while writing.
use crate::ftmm::{Ftmm, FtmmDigest};
use tokio::io::AsyncWrite;

/// A configuration trait that supports turning into [`AsyncWrite`].
pub trait CompConfigure: crate::Sealed {
    /// Create the actual compression writer from the configuration.
    fn to_async_write<'a, W: AsyncWrite + 'a>(&'a self, inner: W) -> impl AsyncWrite;
    /// File extension (typical) of the compresion algorithm.
    fn ext(&self) -> &'static str;
}

macro_rules! comp_algs {
    ($($alg:ident),*$(,)?) => {
        $(pub mod $alg;)*
        ::preinterpret::preinterpret! {
            /// A static dispatch enum for all supported compression algorithms.
            #[non_exhaustive]
            #[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
            #[serde(rename_all = "snake_case")]
            pub enum CompConfig {
                $([!ident_camel! $alg]($alg::Cfg)),*
            }

            impl CompConfig {
                pub fn to_async_write<'a, W: AsyncWrite + Send + 'a>(
                    &'a self,
                    inner: W,
                ) -> std::pin::Pin<Box<dyn tokio::io::AsyncWrite + Send + 'a>> {
                    match self {
                        $(Self::[!ident_camel! $alg](cfg) => {
                            Box::pin(cfg.to_async_write(inner)) as std::pin::Pin<Box<dyn tokio::io::AsyncWrite + Send>>
                        })*
                    }
                }

                /// File extension (typical) of the compresion algorithm.
                pub fn ext(&self) -> &'static str {
                    match self {
                        $(Self::[!ident_camel! $alg](cfg) => cfg.ext(),)*
                    }
                }
            }
        }
    };
}

comp_algs![zstd, xz];

impl Default for CompConfig {
    fn default() -> Self {
        Self::Zstd(zstd::Cfg::default())
    }
}

impl CompConfig {
    pub fn to_mochi<'a, W: AsyncWrite + Send + 'a>(
        &'a self,
        inner: W,
        ftmm: Ftmm,
    ) -> Mochi<std::pin::Pin<Box<dyn tokio::io::AsyncWrite + Send + 'a>>> {
        let inner = self.to_async_write(inner);
        Mochi::new(inner, ftmm)
    }
}

/// A thin wrapper around an [`AsyncWrite`] with checksum & size calculation.
#[non_exhaustive]
pub struct Mochi<W: AsyncWrite + Unpin> {
    pub inner: W,
    pub ftmm: FtmmDigest,
    pub size: u64,
}

impl<W: AsyncWrite + Unpin> Mochi<W> {
    #[must_use]
    pub fn new(inner: W, ftmm: Ftmm) -> Self {
        Self { inner, ftmm: ftmm.to_digest(), size: 0 }
    }
}

impl<W: AsyncWrite + Unpin> AsyncWrite for Mochi<W> {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let ret = AsyncWrite::poll_write(std::pin::pin!(&mut self.inner), cx, buf);
        if let std::task::Poll::Ready(Ok(len)) = &ret {
            self.size += *len as u64;
            self.ftmm.update(&buf[..*len]);
        }
        ret
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        AsyncWrite::poll_flush(std::pin::pin!(&mut self.inner), cx)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        AsyncWrite::poll_shutdown(std::pin::pin!(&mut self.inner), cx)
    }
}
