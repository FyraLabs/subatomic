//! Some helpers for [`object_store`].
//!
//! [`object_store::ObjectStore`] requires the input bytes to be owned (because the crate is
//! designed for sending bytes over the network to another store), but in `kiritan`, we have very
//! high expectations on memory efficiency.
//!
//! Therefore, while we implement [`AsyncWrite`] on [`MultipartUploadWriter`], we create an extra
//! [`StoreBackend`] wrapper that takes in `&[u8]` instead, then clone for [`object_store::ObjectStore`].
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures::FutureExt;
use futures::stream::{FuturesUnordered, StreamExt};
use object_store::{MultipartUpload, ObjectStoreExt, PutPayload, UploadPart};
use tokio::io::AsyncWrite;

use crate::link::Link;

#[derive(Debug, thiserror::Error)]
pub enum StoreErr {
    #[error("io error: {source}")]
    Io {
        #[from]
        source: std::io::Error,
        backtrace: std::backtrace::Backtrace,
    },
    #[error("remote error: {source}")]
    Remote {
        #[from]
        source: object_store::Error,
        backtrace: std::backtrace::Backtrace,
    },
}

/// A wrapper around [`object_store::ObjectStore`] with performance benefits when writing to the
/// local filesystem.
///
/// See [`Self::writer()`] for more information.
#[derive(Debug)]
pub enum StoreBackend {
    Local,
    Remote(Arc<dyn object_store::ObjectStore>),
}

pub trait StoreWrite: AsyncWrite + Send + Unpin + std::any::Any {
    fn as_any_ref(&self) -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
    fn into_box_any(self) -> Box<dyn std::any::Any>;
}
// impl StoreWrite for tokio::io::BufWriter<tokio::fs::File> {}
// impl StoreWrite for MultipartUploadWriter {}
impl<W: AsyncWrite + Send + Unpin + std::any::Any> StoreWrite for W {
    fn as_any_ref(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn into_box_any(self) -> Box<dyn std::any::Any> {
        Box::new(self)
    }
}

impl StoreBackend {
    /// Create a writer. This is either [`tokio::fs::File`] or [`MultipartUploadWriter`] depending
    /// on the actual backend.
    ///
    /// The `link` given must respect the actual backend. In `libsubatomic`, this should be stored
    /// inside the hierarchy configuration.
    ///
    /// # Errors
    /// If the parent directory or the local file cannot be created, [`StoreErr::Io`] is returned.
    /// Errors from [`object_store::ObjectStoreExt::put_multipart`] are also propagated.
    pub async fn writer(&self, link: &Link) -> Result<Box<dyn StoreWrite>, StoreErr> {
        match self {
            Self::Local => {
                if let Some(parent) = link.as_path().parent() {
                    tokio::fs::create_dir_all(parent).await?;
                }
                Ok(Box::new(tokio::io::BufWriter::new(
                    tokio::fs::File::create(link.as_path()).await?,
                )))
            }
            Self::Remote(store) => {
                let upload = store.put_multipart(&link.to_storepath()).await?;
                Ok(Box::new(MultipartUploadWriter::with_defaults(upload)))
            }
        }
    }

    pub async fn rename(&self, a: &Link, b: &Link) -> Result<(), StoreErr> {
        match self {
            StoreBackend::Local => {
                tokio::fs::rename(a, b).await?;
            }
            StoreBackend::Remote(store) => {
                store.rename(&a.to_storepath(), &b.to_storepath()).await?;
            }
        }
        Ok(())
    }

    /// Delete a file at `link`.
    ///
    /// Missing files are treated as success.
    ///
    /// # Errors
    /// Propagates io and `object_store` errors other than `NotFound`.
    pub async fn delete(&self, link: &Link) -> Result<(), StoreErr> {
        match self {
            Self::Local => match tokio::fs::remove_file(link).await {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e.into()),
            },
            Self::Remote(store) => match store.delete(&link.to_storepath()).await {
                Ok(()) | Err(object_store::Error::NotFound { .. }) => Ok(()),
                Err(e) => Err(e.into()),
            },
        }
    }
}

/// Default minimum part size.
///
/// From [`MultipartUpload::put_part`]:
/// > Most stores require that all parts excluding the last are at least 5 MiB, and some further
/// > require that all parts excluding the last be the same size, e.g. [R2].
///
/// [R2]: https://developers.cloudflare.com/r2/objects/multipart-objects/#limitations
pub const MIN_PART_SIZE: usize = 5 * 1024 * 1024;

/// A kuchiyose wrapper around [`MultipartUpload`] that implements [`AsyncWrite`].
pub enum MultipartUploadWriter {
    /// Indicates an ongoing upload.
    Active {
        /// The upload handler.
        upload: Box<dyn MultipartUpload>,
        /// A buffer that aggregates writes to larger than the part size.
        ///
        /// The part size is stored as the [`Vec::capacity`].
        buffer: Vec<u8>,
        /// A pool of ongoing futures of uploading tasks.
        ongoing: FuturesUnordered<UploadPart>,
    },
    /// The writer is finalizing by calling [`MulitpartUpload::complete()`].
    ///
    /// Currently the result of the future [`object_store::PutResult`] is discarded. Errors are
    /// propagated in shutdown.
    Completing {
        fut: std::pin::Pin<
            Box<dyn Future<Output = object_store::Result<object_store::PutResult>> + Send>,
        >,
    },
    /// The writer has completed and should be discarded.
    Done,
}

impl MultipartUploadWriter {
    #[must_use]
    pub fn new(upload: Box<dyn MultipartUpload>, min_part_size: usize) -> Self {
        Self::Active {
            upload,
            buffer: Vec::with_capacity(min_part_size),
            ongoing: FuturesUnordered::new(),
        }
    }

    /// Create a writer with the default part size [`MIN_PART_SIZE`].
    #[must_use]
    pub fn with_defaults(upload: Box<dyn MultipartUpload>) -> Self {
        Self::new(upload, MIN_PART_SIZE)
    }

    // to force `upload` to be moved into the future, take ownership.
    async fn complete(
        mut upload: Box<dyn MultipartUpload + 'static>,
    ) -> object_store::Result<object_store::PutResult> {
        upload.complete().await
    }
}

impl AsyncWrite for MultipartUploadWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let Self::Active { upload, buffer, ongoing } = self.get_mut() else {
            tracing::error!("write to inactive MultipartUploadWriter");
            return Poll::Ready(Ok(0));
        };

        // free up some completed futures
        while let Poll::Ready(Some(res)) = ongoing.poll_next_unpin(cx) {
            if let Err(e) = res {
                return Poll::Ready(Err(io::Error::other(e)));
            }
        }

        // buffer: [..len] [len..cap] (before)      [..] (after)
        //                   🭯                         🭯
        //                   │                         │
        // buf:            [..n]     it[0] it[1] .. remainder
        let n = std::cmp::min(buffer.capacity() - buffer.len(), buf.len());
        buffer.extend_from_slice(&buf[..n]);
        if buffer.capacity() != buffer.len() {
            return Poll::Ready(Ok(buf.len()));
        }

        let mut it = buf[n..].chunks_exact(buffer.capacity());
        let mut remainder = Vec::with_capacity(buffer.capacity());
        remainder.extend_from_slice(it.remainder());
        let payload = std::mem::replace(buffer, remainder).into();
        ongoing.push(upload.put_part(PutPayload::from_bytes(payload)));
        for chunk in &mut it {
            ongoing.push(upload.put_part(PutPayload::from_bytes(chunk.to_owned().into())));
        }
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let Self::Active { upload, buffer, ongoing } = self.get_mut() else {
            tracing::error!("write to inactive MultipartUploadWriter");
            return Poll::Ready(Ok(()));
        };
        // keep the capacity
        #[allow(clippy::drain_collect)]
        let bytes: Vec<u8> = buffer.drain(..).collect();
        ongoing.push(upload.put_part(PutPayload::from_bytes(bytes.into())));

        loop {
            match ongoing.poll_next_unpin(cx) {
                Poll::Ready(Some(Ok(()))) => {}
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Err(io::Error::other(e)));
                }
                Poll::Pending => return Poll::Pending,
                Poll::Ready(None) => return Poll::Ready(Ok(())),
            }
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if let Self::Active { .. } = self.as_mut().get_mut() {
            match self.as_mut().poll_flush(cx) {
                Poll::Ready(Ok(())) => {}
                others => return others,
            }
            let Self::Active { upload, .. } =
                std::mem::replace(self.as_mut().get_mut(), Self::Done)
            else {
                unreachable!()
            };
            *self.as_mut().get_mut() = Self::Completing { fut: Box::pin(Self::complete(upload)) };
        } else if let Self::Done = self.as_mut().get_mut() {
            tracing::warn!("calling shutdown to MultipartUploadWriter::Done");
            return Poll::Ready(Ok(()));
        }
        let Self::Completing { fut } = self.as_mut().get_mut() else { unreachable!() };
        match fut.poll_unpin(cx) {
            Poll::Ready(Ok(_)) => Poll::Ready(Ok(())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(io::Error::other(e))),
            Poll::Pending => Poll::Pending,
        }
    }
}
