//! Some helpers for [`object_store`].
//!
//! [`object_store::ObjectStore`] requires the input bytes to be owned (because the crate is
//! designed for sending bytes over the network to another store), but in `kiritan`, we have very
//! high expectations on memory efficiency.
//!
//! Therefore, while we implement [`AsyncWrite`] on [`MultipartUploadWriter`], we create an extra
//! [`StoreBackend`] wrapper that takes in `&[u8]` instead, then clone for [`object_store::ObjectStore`].
use std::io;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures::FutureExt;
use futures::stream::{FuturesUnordered, StreamExt};
use object_store::{Extensions, MultipartUpload, ObjectStoreExt, PutPayload, UploadPart};
use sha2::Digest;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufWriter};

use crate::link::Link;

#[non_exhaustive]
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

/// Threshold deciding between a single PUT and a multipart upload on [`StoreBackend::Remote`].
///
/// Objects whose actual received size is at most this value are uploaded with a single
/// [`object_store::ObjectStoreExt::put`]; once the received size exceeds it, a multipart
/// upload is started and the staged prefix is replayed into it.
///
/// This is deliberately distinct from [`MIN_PART_SIZE`]: the threshold decides *whether* to
/// use multipart, while the part size decides how multipart payloads are chunked.
pub const SINGLE_PUT_THRESHOLD: u64 = 50 * 1024 * 1024;

/// Maximum number of in-flight part uploads retained by [`MultipartUploadWriter`].
///
/// Each in-flight part holds up to [`MIN_PART_SIZE`] bytes, plus one part-sized write
/// buffer, so steady-state memory per multipart upload is bounded to roughly
/// `(MAX_CONCURRENT_PARTS + 1) * MIN_PART_SIZE` (~25 MiB with defaults).
pub const MAX_CONCURRENT_PARTS: usize = 4;

/// Read buffer size used when streaming request bodies and replaying staged prefixes.
const IO_BUF_SIZE: usize = 64 * 1024;

/// A wrapper around [`object_store::ObjectStore`] with performance benefits when writing to the
/// local filesystem.
///
/// See [`Self::writer()`] for more information.
#[non_exhaustive]
#[derive(Clone, Debug, Default)]
pub enum StoreBackend {
    #[default]
    Local,
    Remote(Arc<dyn object_store::ObjectStore>),
}

/// Blanket trait for [`StoreBackend::writer`].
///
/// This trait is not meant to be implemented externally.
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

/// Result of [`StoreBackend::put_streamed`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamedPut {
    /// Hex-encoded SHA-256 of the received bytes.
    pub sha256_hex: String,
    /// Total number of received bytes.
    pub size: u64,
    /// Whether a multipart upload was used (only possible on [`StoreBackend::Remote`]).
    pub used_multipart: bool,
}

/// Failure modes of [`StoreBackend::put_streamed`].
///
/// `Source` reports a failure while reading the client request body and must be mapped to a
/// client (4xx) error. `Tmp` and `Store` report server-side failures and must be mapped to
/// 5xx errors; they must never be silently converted into client validation errors.
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum StreamErr {
    #[error("client body error: {0}")]
    Source(#[source] std::io::Error),
    #[error("temporary file error: {0}")]
    Tmp(#[source] std::io::Error),
    #[error(transparent)]
    Store(#[from] StoreErr),
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

    /// Single-PUT `payload` to `link`.
    ///
    /// For [`StoreBackend::Remote`] this uses [`object_store::ObjectStoreExt::put`].
    /// For [`StoreBackend::Local`] this creates parent directories and writes the payload
    /// chunks to the destination file, preserving the error behavior of [`Self::writer`].
    ///
    /// # Errors
    /// Propagates local I/O errors as [`StoreErr::Io`] and remote errors as [`StoreErr::Remote`].
    pub async fn put(
        &self,
        link: &Link,
        payload: PutPayload,
    ) -> Result<object_store::PutResult, StoreErr> {
        match self {
            Self::Local => {
                if let Some(parent) = link.as_path().parent() {
                    tokio::fs::create_dir_all(parent).await?;
                }
                let mut file = tokio::fs::File::create(link.as_path()).await?;
                for chunk in &payload {
                    file.write_all(chunk).await?;
                }
                file.shutdown().await?;
                Ok(object_store::PutResult {
                    e_tag: None,
                    version: None,
                    extensions: Extensions::default(),
                })
            }
            Self::Remote(store) => {
                store.put(&link.to_storepath(), payload).await.map_err(StoreErr::from)
            }
        }
    }

    /// Stream `body` into the store while staging into `tmp_path` and hashing with SHA-256.
    ///
    /// `threshold` only affects [`StoreBackend::Remote`]; [`StoreBackend::Local`] streams
    /// directly to the destination (and the staging file) exactly as before, ignoring it.
    ///
    /// Remote behavior:
    /// - Bytes are written to the staging file and hashed as they arrive. No multipart
    ///   session is created until the received size exceeds `threshold`.
    /// - On crossing, a multipart upload is initiated and the staged prefix is replayed
    ///   from disk with a bounded (`IO_BUF_SIZE`) buffer, then newly received bytes are
    ///   written to both the staging file and the multipart writer in order. Each byte is
    ///   uploaded exactly once.
    /// - At EOF, sizes at most `threshold` are uploaded with a single PUT by reading the
    ///   staged file back into a [`PutPayload`]. This is the only point where up to
    ///   `threshold` bytes are held in RAM at once (transient peak ~= object size, at most
    ///   50 MiB with [`SINGLE_PUT_THRESHOLD`]); reception itself never buffers the whole
    ///   object. Larger objects finish the multipart upload without ever holding the whole
    ///   object in memory.
    ///
    /// Failure handling: if a multipart upload was initiated but a later read, write, part
    /// upload, or completion fails, the upload is aborted where supported. The original
    /// error is always preserved; abort failures are logged via `tracing::warn`. Staging
    /// files are left to the caller's existing RAII cleanup (e.g. `tempfile::TempDir`).
    /// Dropping a [`MultipartUploadWriter`] without completion does *not* abort remote
    /// parts on stores like S3/GCS, hence the explicit abort.
    ///
    /// # Errors
    /// - [`StreamErr::Source`] if reading `body` fails (client error).
    /// - [`StreamErr::Tmp`] if the staging file cannot be used (server error).
    /// - [`StreamErr::Store`] if the destination store fails (server error).
    pub async fn put_streamed<R: AsyncRead + Unpin + Send>(
        &self,
        link: &Link,
        tmp_path: &Path,
        body: R,
        threshold: u64,
    ) -> Result<StreamedPut, StreamErr> {
        match self {
            Self::Local => self.put_streamed_local(link, tmp_path, body).await,
            Self::Remote(store) => {
                Self::put_streamed_remote(Arc::clone(store), link, tmp_path, body, threshold).await
            }
        }
    }

    /// Local path of [`Self::put_streamed`]: stream directly to the destination and the
    /// staging file, preserving pre-existing directory creation, file writing, and error
    /// behavior. `threshold` is intentionally ignored.
    async fn put_streamed_local<R: AsyncRead + Unpin + Send>(
        &self,
        link: &Link,
        tmp_path: &Path,
        body: R,
    ) -> Result<StreamedPut, StreamErr> {
        debug_assert!(matches!(self, Self::Local));
        if let Some(parent) = link.as_path().parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| StreamErr::Store(e.into()))?;
        }
        let dest = tokio::fs::File::create(link.as_path())
            .await
            .map_err(|e| StreamErr::Store(e.into()))?;
        let mut dest = BufWriter::new(dest);
        let tmp = tokio::fs::File::create(tmp_path).await.map_err(StreamErr::Tmp)?;
        let mut tmp = BufWriter::new(tmp);
        let mut hasher = sha2::Sha256::new();
        let mut total: u64 = 0;
        let mut body = body;
        let mut buf = vec![0u8; IO_BUF_SIZE];
        loop {
            let n = body.read(&mut buf).await.map_err(StreamErr::Source)?;
            if n == 0 {
                break;
            }
            dest.write_all(&buf[..n]).await.map_err(|e| StreamErr::Store(e.into()))?;
            tmp.write_all(&buf[..n]).await.map_err(StreamErr::Tmp)?;
            hasher.update(&buf[..n]);
            total += n as u64;
        }
        dest.shutdown().await.map_err(|e| StreamErr::Store(e.into()))?;
        tmp.shutdown().await.map_err(StreamErr::Tmp)?;
        Ok(StreamedPut {
            sha256_hex: hex::encode(hasher.finalize()),
            size: total,
            used_multipart: false,
        })
    }

    /// Remote path of [`Self::put_streamed`]; see its documentation for the full contract.
    async fn put_streamed_remote<R: AsyncRead + Unpin + Send>(
        store: Arc<dyn object_store::ObjectStore>,
        link: &Link,
        tmp_path: &Path,
        body: R,
        threshold: u64,
    ) -> Result<StreamedPut, StreamErr> {
        let tmp = tokio::fs::File::create(tmp_path).await.map_err(StreamErr::Tmp)?;
        let mut tmp = BufWriter::new(tmp);
        let mut hasher = sha2::Sha256::new();
        let mut total: u64 = 0;
        let mut multipart: Option<MultipartUploadWriter> = None;
        let mut body = body;
        let mut buf = vec![0u8; IO_BUF_SIZE];

        loop {
            let n = match body.read(&mut buf).await {
                Ok(n) => n,
                Err(e) => {
                    abort_started(&mut multipart).await;
                    return Err(StreamErr::Source(e));
                }
            };
            if n == 0 {
                break;
            }
            if let Err(e) = tmp.write_all(&buf[..n]).await {
                abort_started(&mut multipart).await;
                return Err(StreamErr::Tmp(e));
            }
            hasher.update(&buf[..n]);
            total += n as u64;

            if let Some(w) = multipart.as_mut() {
                if let Err(e) = w.write_all(&buf[..n]).await {
                    abort_started(&mut multipart).await;
                    return Err(StreamErr::Store(e.into()));
                }
            } else if total > threshold {
                match Self::start_multipart(&store, link, tmp_path, &mut tmp).await {
                    Ok(writer) => multipart = Some(writer),
                    Err(e) => return Err(e),
                }
            }
        }

        if let Err(e) = tmp.shutdown().await {
            abort_started(&mut multipart).await;
            return Err(StreamErr::Tmp(e));
        }
        let sha256_hex = hex::encode(hasher.finalize());

        if let Some(mut w) = multipart {
            if let Err(e) = w.shutdown().await {
                abort_and_log(w).await;
                return Err(StreamErr::Store(e.into()));
            }
            Ok(StreamedPut { sha256_hex, size: total, used_multipart: true })
        } else {
            // Single-PUT path. The staged file (at most `threshold` bytes) is read back
            // into a `PutPayload`, so transient memory peaks at roughly the object size.
            let bytes = tokio::fs::read(tmp_path).await.map_err(StreamErr::Tmp)?;
            debug_assert_eq!(bytes.len() as u64, total);
            store
                .put(&link.to_storepath(), PutPayload::from(bytes))
                .await
                .map_err(|e| StreamErr::Store(e.into()))?;
            Ok(StreamedPut { sha256_hex, size: total, used_multipart: false })
        }
    }

    /// Start a multipart upload for `link` and replay the staged prefix into it.
    ///
    /// The staging writer is flushed first so the prefix is visible on disk, then the
    /// whole prefix is streamed back with a bounded buffer. On replay-write failure the
    /// new upload is aborted before returning.
    ///
    /// # Errors
    /// - [`StreamErr::Tmp`] if the staging file cannot be flushed or read.
    /// - [`StreamErr::Store`] if the multipart session cannot be initiated or written.
    async fn start_multipart(
        store: &Arc<dyn object_store::ObjectStore>,
        link: &Link,
        tmp_path: &Path,
        tmp: &mut BufWriter<tokio::fs::File>,
    ) -> Result<MultipartUploadWriter, StreamErr> {
        tmp.flush().await.map_err(StreamErr::Tmp)?;
        let upload = store
            .put_multipart(&link.to_storepath())
            .await
            .map_err(|e| StreamErr::Store(e.into()))?;
        let mut writer = MultipartUploadWriter::with_defaults(upload);
        match replay_prefix(tmp_path, &mut writer).await {
            Ok(()) => Ok(writer),
            Err(ReplayErr::Read(e)) => Err(StreamErr::Tmp(e)),
            Err(ReplayErr::Write(e)) => {
                abort_and_log(writer).await;
                Err(StreamErr::Store(e.into()))
            }
        }
    }

    /// Rename `a` to `b`.
    ///
    /// # Errors
    /// Propagate [`std::io::Error`] and [`object_store::Error`].
    pub async fn rename(&self, a: &Link, b: &Link) -> Result<(), StoreErr> {
        match self {
            Self::Local => {
                tokio::fs::rename(a, b).await?;
            }
            Self::Remote(store) => {
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

/// Abort a finished writer, logging (but never propagating) abort failures.
///
/// The original upload error is always reported by the caller; cleanup failures must not
/// replace it.
async fn abort_and_log(writer: MultipartUploadWriter) {
    if let Err(ab) = writer.abort().await {
        tracing::warn!(%ab, "failed to abort multipart upload after failure");
    }
}

/// Abort an in-progress multipart writer if one was started.
///
/// Used on failure paths after the threshold was crossed. Abort errors are logged but
/// never replace the original error.
async fn abort_started(multipart: &mut Option<MultipartUploadWriter>) {
    if let Some(writer) = multipart.take() {
        abort_and_log(writer).await;
    }
}

#[derive(Debug)]
enum ReplayErr {
    Read(std::io::Error),
    Write(std::io::Error),
}

/// Replay the staged prefix at `tmp_path` into `writer` with a bounded buffer.
///
/// The caller must have flushed the staging writer first so the prefix is visible on disk.
async fn replay_prefix(
    tmp_path: &Path,
    writer: &mut MultipartUploadWriter,
) -> Result<(), ReplayErr> {
    let mut file = tokio::fs::File::open(tmp_path).await.map_err(ReplayErr::Read)?;
    let mut buf = vec![0u8; IO_BUF_SIZE];
    loop {
        let n = file.read(&mut buf).await.map_err(ReplayErr::Read)?;
        if n == 0 {
            break;
        }
        writer.write_all(&buf[..n]).await.map_err(ReplayErr::Write)?;
    }
    Ok(())
}

/// Drive `ongoing` until no immediately-ready part remains.
///
/// Returns the first part-upload error encountered, if any.
fn drain_ready_parts(
    ongoing: &mut FuturesUnordered<UploadPart>,
    cx: &mut Context<'_>,
) -> io::Result<()> {
    while let Poll::Ready(Some(res)) = ongoing.poll_next_unpin(cx) {
        res.map_err(io::Error::other)?;
    }
    Ok(())
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
///
/// At most [`MAX_CONCURRENT_PARTS`] part uploads are kept in flight: once the limit is
/// reached, [`AsyncWrite::poll_write`] returns [`Poll::Pending`] until earlier parts finish,
/// applying backpressure instead of accumulating unbounded payloads under a slow network.
pub enum MultipartUploadWriter {
    /// Indicates an ongoing upload.
    Active {
        /// The upload handler.
        upload: Box<dyn MultipartUpload>,
        /// A buffer that aggregates writes to larger than the part size.
        ///
        /// The part size is stored as the [`Vec::capacity`].
        buffer: Vec<u8>,
        /// A pool of ongoing futures of uploading tasks, bounded by [`MAX_CONCURRENT_PARTS`].
        ongoing: FuturesUnordered<UploadPart>,
    },
    /// The writer is finalizing by calling [`MultipartUpload::complete()`].
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

    /// Abort the upload, cleaning up uploaded parts where the store supports it.
    ///
    /// In-flight part uploads are dropped, then [`MultipartUpload::abort`] is invoked.
    /// A writer that already completed (or is completing) reports success; note that
    /// [`Self::complete`] already attempts an abort when part upload or completion fails,
    /// so an explicit abort after a failed shutdown is best-effort and idempotent-tolerant.
    ///
    /// # Errors
    /// Propagates [`MultipartUpload::abort`] failures for in-progress uploads.
    pub async fn abort(self) -> object_store::Result<()> {
        match self {
            Self::Active { mut upload, .. } => upload.abort().await,
            Self::Completing { .. } | Self::Done => Ok(()),
        }
    }

    // to force `upload` to be moved into the future, take ownership.
    async fn complete(
        mut upload: Box<dyn MultipartUpload + 'static>,
        mut ongoing: FuturesUnordered<UploadPart>,
    ) -> object_store::Result<object_store::PutResult> {
        while let Some(res) = ongoing.next().await {
            if let Err(e) = res {
                if let Err(ab) = upload.abort().await {
                    tracing::warn!(%ab, "failed to abort multipart upload after part failure");
                }
                return Err(e);
            }
        }
        match upload.complete().await {
            Ok(res) => Ok(res),
            Err(e) => {
                if let Err(ab) = upload.abort().await {
                    tracing::warn!(%ab, "failed to abort multipart upload after completion failure");
                }
                Err(e)
            }
        }
    }
}

impl AsyncWrite for MultipartUploadWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        if buf.is_empty() {
            return Poll::Ready(Ok(0));
        }
        let Self::Active { upload, buffer, ongoing } = self.get_mut() else {
            tracing::error!("write to inactive MultipartUploadWriter");
            return Poll::Ready(Ok(0));
        };

        // free up some completed futures
        if let Err(e) = drain_ready_parts(ongoing, cx) {
            return Poll::Ready(Err(e));
        }

        // If at the concurrency limit, wait for at least one part to finish instead
        // of queueing another payload.
        if ongoing.len() >= MAX_CONCURRENT_PARTS {
            match ongoing.poll_next_unpin(cx) {
                Poll::Ready(Some(Ok(()))) => {
                    if let Err(e) = drain_ready_parts(ongoing, cx) {
                        return Poll::Ready(Err(e));
                    }
                }
                Poll::Ready(Some(Err(e))) => return Poll::Ready(Err(io::Error::other(e))),
                Poll::Ready(None) | Poll::Pending => return Poll::Pending,
            }
        }

        let mut consumed: usize = 0;
        loop {
            let space = buffer.capacity() - buffer.len();
            if space == 0 {
                if ongoing.len() >= MAX_CONCURRENT_PARTS {
                    break;
                }
                let cap = buffer.capacity();
                let payload: PutPayload = std::mem::replace(buffer, Vec::with_capacity(cap)).into();
                ongoing.push(upload.put_part(payload));
                continue;
            }
            if consumed >= buf.len() {
                break;
            }
            let n = std::cmp::min(space, buf.len() - consumed);
            buffer.extend_from_slice(&buf[consumed..consumed + n]);
            consumed += n;
            if buffer.len() == buffer.capacity() {
                if ongoing.len() >= MAX_CONCURRENT_PARTS {
                    break;
                }
                let cap = buffer.capacity();
                let payload: PutPayload = std::mem::replace(buffer, Vec::with_capacity(cap)).into();
                ongoing.push(upload.put_part(payload));
            } else {
                break;
            }
        }

        if consumed == 0 {
            // No progress: the part buffer is full and part uploads are saturated.
            // Apply backpressure until a slot frees up.
            match ongoing.poll_next_unpin(cx) {
                Poll::Ready(Some(Err(e))) => Poll::Ready(Err(io::Error::other(e))),
                Poll::Ready(None) => Poll::Ready(Err(io::Error::other("multipart writer stalled"))),
                Poll::Ready(Some(Ok(()))) | Poll::Pending => Poll::Pending,
            }
        } else {
            Poll::Ready(Ok(consumed))
        }
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let Self::Active { ongoing, .. } = self.get_mut() else {
            tracing::error!("flush on inactive MultipartUploadWriter");
            return Poll::Ready(Ok(()));
        };

        loop {
            match ongoing.poll_next_unpin(cx) {
                Poll::Ready(Some(Ok(()))) => {}
                Poll::Ready(Some(Err(e))) => return Poll::Ready(Err(io::Error::other(e))),
                Poll::Pending => return Poll::Pending,
                Poll::Ready(None) => return Poll::Ready(Ok(())),
            }
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if let Self::Active { .. } = self.as_mut().get_mut() {
            match self.as_mut().poll_flush(cx) {
                Poll::Ready(Ok(())) => {}
                other => return other,
            }

            let Self::Active { mut upload, buffer, ongoing } =
                std::mem::replace(self.as_mut().get_mut(), Self::Done)
            else {
                unreachable!()
            };

            if !buffer.is_empty() {
                ongoing.push(upload.put_part(PutPayload::from_bytes(buffer.into())));
            }

            *self.as_mut().get_mut() =
                Self::Completing { fut: Box::pin(Self::complete(upload, ongoing)) };
        } else if let Self::Done = self.as_mut().get_mut() {
            tracing::warn!("shutdown called on MultipartUploadWriter::Done");
            return Poll::Ready(Ok(()));
        }

        let Self::Completing { fut } = self.as_mut().get_mut() else { unreachable!() };
        match fut.poll_unpin(cx) {
            Poll::Ready(Ok(_)) => {
                *self.as_mut().get_mut() = Self::Done;
                Poll::Ready(Ok(()))
            }
            Poll::Ready(Err(e)) => Poll::Ready(Err(io::Error::other(e))),
            Poll::Pending => Poll::Pending,
        }
    }
}

#[cfg(test)]
#[allow(clippy::cast_possible_truncation, clippy::struct_excessive_bools)]
mod test {
    use super::*;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::Duration;

    use futures::stream::BoxStream;
    use object_store::{
        GetOptions, GetResult, ListResult, ObjectMeta, PutMultipartOptions, PutOptions,
    };
    use tokio::io::ReadBuf;

    const SMALL_THRESHOLD: u64 = 64;

    fn test_data(len: usize, seed: u8) -> Vec<u8> {
        (0..len).map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed)).collect()
    }

    fn sha256_hex(data: &[u8]) -> String {
        use sha2::Digest as _;
        hex::encode(sha2::Sha256::digest(data))
    }

    #[derive(Debug, Default)]
    struct SpyConfig {
        fail_put_multipart: bool,
        fail_put: bool,
        fail_part_at: Option<usize>,
        fail_complete: bool,
        fail_abort: bool,
        part_delay_ms: u64,
    }

    #[derive(Debug, Default)]
    struct SpyUploadState {
        parts: Mutex<Vec<(usize, Vec<u8>)>>,
        completed: AtomicBool,
        aborted: AtomicBool,
        complete_calls: AtomicUsize,
        abort_calls: AtomicUsize,
        part_calls: AtomicUsize,
        in_flight: AtomicUsize,
        max_in_flight: AtomicUsize,
    }

    #[derive(Debug)]
    struct SpyShared {
        config: SpyConfig,
        puts: Mutex<Vec<(object_store::path::Path, Vec<u8>)>>,
        create_count: AtomicUsize,
        uploads: Mutex<Vec<Arc<SpyUploadState>>>,
    }

    #[derive(Debug, Clone)]
    struct SpyStore(Arc<SpyShared>);

    impl SpyStore {
        fn new(config: SpyConfig) -> Self {
            Self(Arc::new(SpyShared {
                config,
                puts: Mutex::new(Vec::new()),
                create_count: AtomicUsize::new(0),
                uploads: Mutex::new(Vec::new()),
            }))
        }

        fn puts(&self) -> Vec<(object_store::path::Path, Vec<u8>)> {
            self.0.puts.lock().expect("puts poisoned").clone()
        }

        fn create_count(&self) -> usize {
            self.0.create_count.load(Ordering::SeqCst)
        }

        fn uploads(&self) -> Vec<Arc<SpyUploadState>> {
            self.0.uploads.lock().expect("uploads poisoned").clone()
        }
    }

    impl std::fmt::Display for SpyStore {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("spy")
        }
    }

    fn spy_error(msg: &'static str) -> object_store::Error {
        object_store::Error::Generic { store: "spy", source: msg.into() }
    }

    #[derive(Debug)]
    struct SpyUpload {
        shared: Arc<SpyShared>,
        state: Arc<SpyUploadState>,
    }

    #[async_trait::async_trait]
    impl MultipartUpload for SpyUpload {
        fn put_part(&mut self, data: PutPayload) -> UploadPart {
            let idx = self.state.part_calls.fetch_add(1, Ordering::SeqCst);
            if self.shared.config.fail_part_at == Some(idx) {
                return Box::pin(async move { Err(spy_error("injected part failure")) });
            }
            let state = Arc::clone(&self.state);
            let delay = self.shared.config.part_delay_ms;
            let mut bytes = Vec::new();
            for chunk in &data {
                bytes.extend_from_slice(chunk);
            }
            Box::pin(async move {
                state.in_flight.fetch_add(1, Ordering::SeqCst);
                state
                    .max_in_flight
                    .fetch_max(state.in_flight.load(Ordering::SeqCst), Ordering::SeqCst);
                if delay > 0 {
                    tokio::time::sleep(Duration::from_millis(delay)).await;
                }
                state.parts.lock().expect("parts poisoned").push((idx, bytes));
                state.in_flight.fetch_sub(1, Ordering::SeqCst);
                Ok(())
            })
        }

        async fn complete(&mut self) -> object_store::Result<object_store::PutResult> {
            self.state.complete_calls.fetch_add(1, Ordering::SeqCst);
            if self.shared.config.fail_complete {
                return Err(spy_error("injected complete failure"));
            }
            self.state.completed.store(true, Ordering::SeqCst);
            Ok(object_store::PutResult {
                e_tag: None,
                version: None,
                extensions: Extensions::default(),
            })
        }

        async fn abort(&mut self) -> object_store::Result<()> {
            self.state.abort_calls.fetch_add(1, Ordering::SeqCst);
            if self.shared.config.fail_abort {
                return Err(spy_error("injected abort failure"));
            }
            self.state.aborted.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    #[async_trait::async_trait]
    impl object_store::ObjectStore for SpyStore {
        async fn put_opts(
            &self,
            location: &object_store::path::Path,
            payload: PutPayload,
            _opts: PutOptions,
        ) -> object_store::Result<object_store::PutResult> {
            if self.0.config.fail_put {
                return Err(spy_error("injected put failure"));
            }
            let mut bytes = Vec::new();
            for chunk in &payload {
                bytes.extend_from_slice(chunk);
            }
            self.0.puts.lock().expect("puts poisoned").push((location.clone(), bytes));
            Ok(object_store::PutResult {
                e_tag: None,
                version: None,
                extensions: Extensions::default(),
            })
        }

        async fn put_multipart_opts(
            &self,
            _location: &object_store::path::Path,
            _opts: PutMultipartOptions,
        ) -> object_store::Result<Box<dyn MultipartUpload>> {
            self.0.create_count.fetch_add(1, Ordering::SeqCst);
            if self.0.config.fail_put_multipart {
                return Err(spy_error("injected multipart initiation failure"));
            }
            let state = Arc::new(SpyUploadState::default());
            self.0.uploads.lock().expect("uploads poisoned").push(Arc::clone(&state));
            Ok(Box::new(SpyUpload { shared: Arc::clone(&self.0), state }))
        }

        async fn get_opts(
            &self,
            _location: &object_store::path::Path,
            _options: GetOptions,
        ) -> object_store::Result<GetResult> {
            Err(object_store::Error::NotSupported { source: "spy".into() })
        }

        fn delete_stream(
            &self,
            _locations: BoxStream<'static, object_store::Result<object_store::path::Path>>,
        ) -> BoxStream<'static, object_store::Result<object_store::path::Path>> {
            futures::stream::empty().boxed()
        }

        fn list(
            &self,
            _prefix: Option<&object_store::path::Path>,
        ) -> BoxStream<'static, object_store::Result<ObjectMeta>> {
            futures::stream::empty().boxed()
        }

        async fn list_with_delimiter(
            &self,
            _prefix: Option<&object_store::path::Path>,
        ) -> object_store::Result<ListResult> {
            Ok(ListResult {
                common_prefixes: Vec::new(),
                objects: Vec::new(),
                extensions: Extensions::default(),
            })
        }

        async fn copy_opts(
            &self,
            _from: &object_store::path::Path,
            _to: &object_store::path::Path,
            _options: object_store::CopyOptions,
        ) -> object_store::Result<()> {
            Err(object_store::Error::NotSupported { source: "spy".into() })
        }
    }

    fn assembled(state: &SpyUploadState) -> Vec<u8> {
        let mut parts = state.parts.lock().expect("parts poisoned").clone();
        parts.sort_by_key(|(idx, _)| *idx);
        parts.into_iter().flat_map(|(_, bytes)| bytes).collect()
    }

    fn remote_backend(config: SpyConfig) -> (StoreBackend, SpyStore) {
        let spy = SpyStore::new(config);
        let store =
            StoreBackend::Remote(Arc::new(spy.clone()) as Arc<dyn object_store::ObjectStore>);
        (store, spy)
    }

    struct FailReader {
        data: Vec<u8>,
        pos: usize,
        fail_after: usize,
    }

    impl AsyncRead for FailReader {
        fn poll_read(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            if self.pos >= self.fail_after {
                return Poll::Ready(Err(io::Error::other("injected stream failure")));
            }
            if self.pos >= self.data.len() {
                return Poll::Ready(Ok(()));
            }
            let allowed = std::cmp::min(self.data.len() - self.pos, self.fail_after - self.pos);
            let n = std::cmp::min(buf.remaining(), allowed);
            buf.put_slice(&self.data[self.pos..self.pos + n]);
            self.pos += n;
            Poll::Ready(Ok(()))
        }
    }

    #[test]
    fn constants_are_sane() {
        const _: () = assert!(MAX_CONCURRENT_PARTS > 0);
        assert_eq!(SINGLE_PUT_THRESHOLD, 50 * 1024 * 1024);
        assert_eq!(MIN_PART_SIZE, 5 * 1024 * 1024);
        assert_ne!(SINGLE_PUT_THRESHOLD, MIN_PART_SIZE as u64);
    }

    #[tokio::test]
    async fn empty_object_uses_single_put() {
        let (store, spy) = remote_backend(SpyConfig::default());
        let tmp = tempfile::tempdir().expect("tempdir");
        let tmp_path = tmp.path().join("pkg.rpm");
        let link = Link::new("repo/empty-1-1.noarch.rpm");

        let out = store.put_streamed(link, &tmp_path, &[][..], SMALL_THRESHOLD).await.expect("put");
        assert!(!out.used_multipart);
        assert_eq!(out.size, 0);
        assert_eq!(out.sha256_hex, sha256_hex(&[]));
        assert_eq!(spy.create_count(), 0);
        let puts = spy.puts();
        assert_eq!(puts.len(), 1);
        assert_eq!(puts[0].1.len(), 0);
        assert_eq!(std::fs::read(&tmp_path).expect("staged"), Vec::<u8>::new());
    }

    #[tokio::test]
    async fn object_smaller_than_threshold_uses_single_put() {
        let (store, spy) = remote_backend(SpyConfig::default());
        let tmp = tempfile::tempdir().expect("tempdir");
        let tmp_path = tmp.path().join("pkg.rpm");
        let link = Link::new("repo/small-1-1.noarch.rpm");
        let data = test_data(SMALL_THRESHOLD as usize - 1, 7);

        let out =
            store.put_streamed(link, &tmp_path, &data[..], SMALL_THRESHOLD).await.expect("put");
        assert!(!out.used_multipart);
        assert_eq!(out.size, data.len() as u64);
        assert_eq!(out.sha256_hex, sha256_hex(&data));
        assert_eq!(spy.create_count(), 0);
        let puts = spy.puts();
        assert_eq!(puts.len(), 1);
        assert_eq!(puts[0].1, data);
        assert_eq!(std::fs::read(&tmp_path).expect("staged"), data);
    }

    #[tokio::test]
    async fn object_exactly_at_threshold_uses_single_put() {
        let (store, spy) = remote_backend(SpyConfig::default());
        let tmp = tempfile::tempdir().expect("tempdir");
        let tmp_path = tmp.path().join("pkg.rpm");
        let link = Link::new("repo/exact-1-1.noarch.rpm");
        let data = test_data(SMALL_THRESHOLD as usize, 9);

        let out =
            store.put_streamed(link, &tmp_path, &data[..], SMALL_THRESHOLD).await.expect("put");
        assert!(!out.used_multipart);
        assert_eq!(spy.create_count(), 0);
        assert_eq!(spy.puts().len(), 1);
        assert_eq!(spy.puts()[0].1, data);
    }

    #[tokio::test]
    async fn object_one_byte_above_threshold_uses_multipart() {
        let (store, spy) = remote_backend(SpyConfig::default());
        let tmp = tempfile::tempdir().expect("tempdir");
        let tmp_path = tmp.path().join("pkg.rpm");
        let link = Link::new("repo/above-1-1.noarch.rpm");
        let data = test_data(SMALL_THRESHOLD as usize + 1, 11);

        let out =
            store.put_streamed(link, &tmp_path, &data[..], SMALL_THRESHOLD).await.expect("put");
        assert!(out.used_multipart);
        assert_eq!(out.sha256_hex, sha256_hex(&data));
        assert_eq!(spy.create_count(), 1);
        assert_eq!(spy.puts().len(), 0);
        let uploads = spy.uploads();
        assert_eq!(uploads.len(), 1);
        assert_eq!(assembled(&uploads[0]), data);
        assert_eq!(std::fs::read(&tmp_path).expect("staged"), data);
    }

    #[tokio::test]
    async fn large_object_multipart_is_byte_identical() {
        let (store, spy) = remote_backend(SpyConfig::default());
        let tmp = tempfile::tempdir().expect("tempdir");
        let tmp_path = tmp.path().join("pkg.rpm");
        let link = Link::new("repo/large-1-1.noarch.rpm");
        let data = test_data(10_000, 13);

        let out =
            store.put_streamed(link, &tmp_path, &data[..], SMALL_THRESHOLD).await.expect("put");
        assert!(out.used_multipart);
        assert_eq!(out.size, data.len() as u64);
        assert_eq!(out.sha256_hex, sha256_hex(&data));
        let uploads = spy.uploads();
        assert_eq!(uploads.len(), 1);
        assert_eq!(assembled(&uploads[0]), data);
        assert!(uploads[0].completed.load(Ordering::SeqCst));
        assert!(!uploads[0].aborted.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn single_write_crossing_threshold_by_large_amount() {
        let spy = SpyStore::new(SpyConfig::default());
        let upload = object_store::ObjectStoreExt::put_multipart(
            &spy,
            &object_store::path::Path::from("repo/x.rpm"),
        )
            .await
            .expect("multipart");
        let mut writer = MultipartUploadWriter::new(upload, 16);
        let data = test_data(256, 21);
        writer.write_all(&data).await.expect("write");
        writer.shutdown().await.expect("shutdown");
        let uploads = spy.uploads();
        assert_eq!(uploads.len(), 1);
        assert_eq!(assembled(&uploads[0]), data);
        assert!(uploads[0].completed.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn multiple_writes_crossing_threshold() {
        let spy = SpyStore::new(SpyConfig::default());
        let upload = object_store::ObjectStoreExt::put_multipart(
            &spy,
            &object_store::path::Path::from("repo/x.rpm"),
        )
            .await
            .expect("multipart");
        let mut writer = MultipartUploadWriter::new(upload, 16);
        let chunks = [test_data(5, 1), test_data(40, 2), test_data(7, 3), test_data(100, 4)];
        let mut expected = Vec::new();
        for chunk in &chunks {
            writer.write_all(chunk).await.expect("write");
            expected.extend_from_slice(chunk);
        }
        writer.shutdown().await.expect("shutdown");
        let uploads = spy.uploads();
        assert_eq!(uploads.len(), 1);
        assert_eq!(assembled(&uploads[0]), expected);
    }

    #[tokio::test]
    async fn repeated_flushes_do_not_finalize() {
        let spy = SpyStore::new(SpyConfig::default());
        let upload = object_store::ObjectStoreExt::put_multipart(
            &spy,
            &object_store::path::Path::from("repo/x.rpm"),
        )
            .await
            .expect("multipart");
        let mut writer = MultipartUploadWriter::new(upload, 16);
        writer.write_all(b"hello world, this is a flush test").await.expect("write");
        for _ in 0..5 {
            writer.flush().await.expect("flush");
        }
        let uploads = spy.uploads();
        assert_eq!(uploads.len(), 1);
        assert_eq!(uploads[0].complete_calls.load(Ordering::SeqCst), 0);
        assert!(!uploads[0].completed.load(Ordering::SeqCst));
        writer.shutdown().await.expect("shutdown");
        assert_eq!(uploads[0].complete_calls.load(Ordering::SeqCst), 1);
        assert!(uploads[0].completed.load(Ordering::SeqCst));
        assert_eq!(assembled(&uploads[0]), b"hello world, this is a flush test");
    }

    #[tokio::test]
    async fn sha256_matches_reference_and_mismatch_is_detectable() {
        let (store, _spy) = remote_backend(SpyConfig::default());
        let tmp = tempfile::tempdir().expect("tempdir");
        let tmp_path = tmp.path().join("pkg.rpm");
        let link = Link::new("repo/hash-1-1.noarch.rpm");
        let data = test_data(1_000, 31);

        let out =
            store.put_streamed(link, &tmp_path, &data[..], SMALL_THRESHOLD).await.expect("put");
        assert_eq!(out.sha256_hex, sha256_hex(&data));
        // The server compares this hex against the client-supplied checksum field;
        // a corrupted expectation must not compare equal.
        assert_ne!(out.sha256_hex, sha256_hex(b"something else entirely"));
    }

    #[tokio::test]
    async fn stream_failure_before_threshold_creates_no_session() {
        let (store, spy) = remote_backend(SpyConfig::default());
        let tmp = tempfile::tempdir().expect("tempdir");
        let tmp_path = tmp.path().join("pkg.rpm");
        let link = Link::new("repo/fail-early-1-1.noarch.rpm");
        let data = test_data(100, 41);
        let body = FailReader { data, pos: 0, fail_after: 10 };

        let err = store.put_streamed(link, &tmp_path, body, 50).await.expect_err("must fail");
        assert!(
            matches!(err, StreamErr::Source(_)),
            "storage errors must not mask source: {err:?}"
        );
        assert_eq!(spy.create_count(), 0);
        assert_eq!(spy.puts().len(), 0);
    }

    #[tokio::test]
    async fn stream_failure_after_threshold_aborts() {
        let (store, spy) = remote_backend(SpyConfig::default());
        let tmp = tempfile::tempdir().expect("tempdir");
        let tmp_path = tmp.path().join("pkg.rpm");
        let link = Link::new("repo/fail-late-1-1.noarch.rpm");
        let data = test_data(200, 43);
        let body = FailReader { data, pos: 0, fail_after: 120 };

        let err = store.put_streamed(link, &tmp_path, body, 50).await.expect_err("must fail");
        assert!(matches!(err, StreamErr::Source(_)), "must stay a client error: {err:?}");
        assert_eq!(spy.create_count(), 1);
        assert_eq!(spy.puts().len(), 0);
        let uploads = spy.uploads();
        assert_eq!(uploads.len(), 1);
        assert!(!uploads[0].completed.load(Ordering::SeqCst));
        assert!(uploads[0].aborted.load(Ordering::SeqCst));
        assert_eq!(uploads[0].abort_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn multipart_initiation_failure_is_a_store_error() {
        let (store, spy) =
            remote_backend(SpyConfig { fail_put_multipart: true, ..SpyConfig::default() });
        let tmp = tempfile::tempdir().expect("tempdir");
        let tmp_path = tmp.path().join("pkg.rpm");
        let link = Link::new("repo/noinit-1-1.noarch.rpm");
        let data = test_data(200, 47);

        let err = store.put_streamed(link, &tmp_path, &data[..], 50).await.expect_err("must fail");
        assert!(matches!(err, StreamErr::Store(_)), "must not look like client error: {err:?}");
        assert_eq!(spy.create_count(), 1);
        assert_eq!(spy.puts().len(), 0);
        assert!(spy.uploads().is_empty());
    }

    #[tokio::test]
    async fn part_upload_failure_aborts() {
        let (store, spy) =
            remote_backend(SpyConfig { fail_part_at: Some(0), ..SpyConfig::default() });
        let tmp = tempfile::tempdir().expect("tempdir");
        let tmp_path = tmp.path().join("pkg.rpm");
        let link = Link::new("repo/badpart-1-1.noarch.rpm");
        // Large enough to force several parts through the default 5 MiB part size? No:
        // replay uses 64 KiB writes into a 5 MiB buffer, so use a tiny-part writer path
        // via multiple threshold-exceeding bytes instead. Any multipart failure suffices.
        let data = test_data(500, 53);

        let err = store.put_streamed(link, &tmp_path, &data[..], 50).await.expect_err("must fail");
        assert!(matches!(err, StreamErr::Store(_)), "must stay a store error: {err:?}");
        assert_eq!(spy.puts().len(), 0);
        let uploads = spy.uploads();
        assert_eq!(uploads.len(), 1);
        assert!(!uploads[0].completed.load(Ordering::SeqCst));
        assert!(uploads[0].aborted.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn completion_failure_aborts_and_preserves_original_error() {
        let (store, spy) =
            remote_backend(SpyConfig { fail_complete: true, ..SpyConfig::default() });
        let tmp = tempfile::tempdir().expect("tempdir");
        let tmp_path = tmp.path().join("pkg.rpm");
        let link = Link::new("repo/badcomplete-1-1.noarch.rpm");
        let data = test_data(200, 59);

        let err = store.put_streamed(link, &tmp_path, &data[..], 50).await.expect_err("must fail");
        assert!(matches!(err, StreamErr::Store(_)), "must stay a store error: {err:?}");
        let uploads = spy.uploads();
        assert_eq!(uploads.len(), 1);
        assert!(!uploads[0].completed.load(Ordering::SeqCst));
        assert!(uploads[0].aborted.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn abort_failure_still_reports_original_store_error() {
        let (store, spy) = remote_backend(SpyConfig {
            fail_complete: true,
            fail_abort: true,
            ..SpyConfig::default()
        });
        let tmp = tempfile::tempdir().expect("tempdir");
        let tmp_path = tmp.path().join("pkg.rpm");
        let link = Link::new("repo/badabort-1-1.noarch.rpm");
        let data = test_data(200, 61);

        let err = store.put_streamed(link, &tmp_path, &data[..], 50).await.expect_err("must fail");
        // The completion error is preserved even though the abort also failed (which is
        // only logged); it must still surface as a store error, never a validation error.
        assert!(matches!(err, StreamErr::Store(_)), "original error lost: {err:?}");
        assert_eq!(spy.uploads()[0].abort_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn tempdir_cleanup_on_success_and_failure() {
        let data = test_data(200, 67);
        let dir = tempfile::tempdir().expect("tempdir");
        let dir_path = dir.path().to_path_buf();

        let (store, _spy) = remote_backend(SpyConfig::default());
        let tmp_path = dir.path().join("ok.rpm");
        let link = Link::new("repo/ok-1-1.noarch.rpm");
        store.put_streamed(link, &tmp_path, &data[..], SMALL_THRESHOLD).await.expect("put");
        assert!(tmp_path.exists());
        drop(dir);
        assert!(!dir_path.exists());

        let dir = tempfile::tempdir().expect("tempdir");
        let dir_path = dir.path().to_path_buf();
        let (store, _spy) = remote_backend(SpyConfig::default());
        let tmp_path = dir.path().join("fail.rpm");
        let link = Link::new("repo/fail-1-1.noarch.rpm");
        let body = FailReader { data, pos: 0, fail_after: 10 };
        let err = store.put_streamed(link, &tmp_path, body, SMALL_THRESHOLD).await;
        assert!(matches!(err, Err(StreamErr::Source(_))));
        drop(dir);
        assert!(!dir_path.exists(), "RAII tempdir must clean up after failure");
    }

    #[tokio::test]
    async fn local_backend_ignores_threshold_and_streams() {
        let dir = tempfile::tempdir().expect("tempdir");
        let dest = dir.path().join("nested/repo/local-1-1.noarch.rpm");
        let tmp_path = dir.path().join("staging.rpm");
        let link = Link::new(dest.to_str().expect("utf-8"));
        let store = StoreBackend::Local;
        // Far above any threshold: Local must still stream directly, never multipart.
        let data = test_data(5_000, 71);

        let out = store.put_streamed(link, &tmp_path, &data[..], 8).await.expect("local put");
        assert!(!out.used_multipart);
        assert_eq!(out.size, data.len() as u64);
        assert_eq!(out.sha256_hex, sha256_hex(&data));
        assert_eq!(std::fs::read(&dest).expect("dest"), data);
        assert_eq!(std::fs::read(&tmp_path).expect("staged"), data);
    }

    #[tokio::test]
    async fn local_backend_source_errors_are_not_store_errors() {
        let dir = tempfile::tempdir().expect("tempdir");
        let dest = dir.path().join("repo/local-1-1.noarch.rpm");
        let tmp_path = dir.path().join("staging.rpm");
        let link = Link::new(dest.to_str().expect("utf-8"));
        let store = StoreBackend::Local;
        let body = FailReader { data: test_data(100, 73), pos: 0, fail_after: 10 };

        let err = store.put_streamed(link, &tmp_path, body, 8).await.expect_err("must fail");
        assert!(matches!(err, StreamErr::Source(_)));
    }

    #[tokio::test]
    async fn local_single_put_writes_file_and_creates_dirs() {
        let dir = tempfile::tempdir().expect("tempdir");
        let dest = dir.path().join("a/b/c.rpm");
        let link = Link::new(dest.to_str().expect("utf-8"));
        let store = StoreBackend::Local;
        let payload = PutPayload::from(test_data(128, 79));

        store.put(link, payload).await.expect("put");
        assert_eq!(std::fs::read(&dest).expect("dest"), test_data(128, 79));
    }

    #[tokio::test]
    async fn remote_single_put_records_payload() {
        let (store, spy) = remote_backend(SpyConfig::default());
        let link = Link::new("repo/direct-1-1.noarch.rpm");
        let payload = PutPayload::from(test_data(32, 83));

        store.put(link, payload).await.expect("put");
        let puts = spy.puts();
        assert_eq!(puts.len(), 1);
        assert_eq!(puts[0].1, test_data(32, 83));
        assert_eq!(spy.create_count(), 0);
    }

    #[tokio::test]
    async fn part_concurrency_is_bounded_under_backpressure() {
        let spy = SpyStore::new(SpyConfig { part_delay_ms: 10, ..SpyConfig::default() });
        let upload = object_store::ObjectStoreExt::put_multipart(
            &spy,
            &object_store::path::Path::from("repo/x.rpm"),
        )
            .await
            .expect("multipart");
        // Tiny parts plus a slow store: an unbounded writer would queue dozens of
        // in-flight parts for a single large write.
        let mut writer = MultipartUploadWriter::new(upload, 16);
        let data = test_data(512, 91);
        writer.write_all(&data).await.expect("write");
        writer.shutdown().await.expect("shutdown");
        let uploads = spy.uploads();
        assert_eq!(uploads.len(), 1);
        assert_eq!(assembled(&uploads[0]), data);
        assert!(
            uploads[0].max_in_flight.load(Ordering::SeqCst) <= MAX_CONCURRENT_PARTS,
            "in-flight parts exceeded the bound: {}",
            uploads[0].max_in_flight.load(Ordering::SeqCst)
        );
        // With 512 bytes in 16-byte parts and 10 ms per part, an unbounded writer
        // would have overlapped far more than the bound allows.
        assert!(uploads[0].part_calls.load(Ordering::SeqCst) > MAX_CONCURRENT_PARTS);
    }

    #[tokio::test]
    async fn concurrent_uploads_stay_bounded_and_correct() {
        let (store, spy) = remote_backend(SpyConfig { part_delay_ms: 5, ..SpyConfig::default() });
        let threshold = 128u64;
        let mut handles = Vec::new();
        for i in 0u8..8 {
            let store = store.clone();
            // Alternate small (single PUT) and large (multipart) objects.
            let len = if i % 2 == 0 { 64 } else { 4_000 };
            let expected = test_data(len, 100 + i);
            handles.push(tokio::spawn(async move {
                let dir = tempfile::tempdir().expect("tempdir");
                let tmp_path = dir.path().join(format!("pkg-{i}.rpm"));
                let link_str = format!("repo/concurrent-{i}-1-1.noarch.rpm");
                let link = Link::new(&link_str);
                let out = store
                    .put_streamed(link, &tmp_path, &expected[..], threshold)
                    .await
                    .expect("concurrent put");
                assert_eq!(out.sha256_hex, sha256_hex(&expected));
                (expected, out.used_multipart, dir)
            }));
        }
        let mut small = 0usize;
        let mut large = 0usize;
        let mut seen: Vec<Vec<u8>> = Vec::new();
        for handle in handles {
            let (expected, used_multipart, _dir) = handle.await.expect("join");
            if expected.len() <= threshold as usize {
                assert!(!used_multipart);
                small += 1;
            } else {
                assert!(used_multipart);
                large += 1;
            }
            seen.push(expected);
        }
        assert_eq!((small, large), (4, 4));
        assert_eq!(spy.puts().len(), 4);
        assert_eq!(spy.create_count(), 4);
        let uploads = spy.uploads();
        assert_eq!(uploads.len(), 4);
        for upload in &uploads {
            assert!(
                upload.max_in_flight.load(Ordering::SeqCst) <= MAX_CONCURRENT_PARTS,
                "part concurrency exceeded the bound"
            );
            assert!(upload.completed.load(Ordering::SeqCst));
        }
        let mut contents: Vec<Vec<u8>> = uploads.iter().map(|upload| assembled(upload)).collect();
        contents.sort();
        let mut expected_large: Vec<Vec<u8>> =
            seen.into_iter().filter(|data| data.len() > threshold as usize).collect();
        expected_large.sort();
        assert_eq!(contents, expected_large);}}