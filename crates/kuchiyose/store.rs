//! Some helpers for [`object_store`].
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::FutureExt;
use futures::stream::{FuturesUnordered, StreamExt};
use object_store::{MultipartUpload, PutPayload, UploadPart};
use tokio::io::AsyncWrite;

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
        fut: std::pin::Pin<Box<dyn Future<Output = object_store::Result<object_store::PutResult>>>>,
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
