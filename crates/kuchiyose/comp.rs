use crate::ftmm::{Ftmm, FtmmDigest};
use tokio::io::AsyncWrite;

/// A configuration trait that supports turning into [`AsyncWrite`].
pub trait CompConfigure {
    fn to_async_write<'a, W: AsyncWrite + 'a>(&'a self, inner: W) -> impl AsyncWrite;
}

macro_rules! comp_algs {
    ($($alg:ident),*$(,)?) => {
        $(pub mod $alg;)*
        ::preinterpret::preinterpret! {
            #[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
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
