//! Integration tests for [`StoreBackend::Remote`] against a real S3-compatible service (`RustFS`).
//!
//! The unit tests in [`kuchiyose::store`] cover path selection and failure handling with an
//! in-memory test double. These tests instead exercise the actual remote backend
//! (`object_store` S3 client over HTTP) to verify single PUTs, multipart uploads, content
//! integrity, and abort behavior against real S3 semantics.
//!
//! They require `SUBATOMIC_S3_TESTS=1` plus the standard `AWS_*` environment used by
//! [`object_store::aws::AmazonS3Builder::from_env`]:
//!
//! - `AWS_ENDPOINT`, `AWS_BUCKET_NAME`, access/secret keys, region, `AWS_ALLOW_HTTP=true`.
//!
//! In CI these point at the `RustFS` service container started by `.github/workflows/rust.yml`.
//! `RustFS` enforces the standard S3 constraint that all parts except the last are at least
//! 5 MiB (see <https://docs.rustfs.com/en/concepts/limit>). Without the gate the tests skip;
//! with the gate the remote backend must initialize, otherwise the test fails loudly instead
//! of silently falling back to local storage.

// Clippy's `tests_outside_test_module` does not account for integration-test crates
// (this whole file only compiles under `cargo test`); allow it here instead of adding
// a redundant wrapper module.
#![allow(clippy::tests_outside_test_module)]

use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use kuchiyose::store::{MultipartUploadWriter, PutStreamedParams, StoreBackend, StreamErr};
use kuchiyose::{Ftmm, Link, LinkBuf};
use object_store::ObjectStoreExt;
use sha2::{Digest, Sha256, Sha384};
use tokio::io::{AsyncRead, AsyncWriteExt, ReadBuf};

/// 64 KiB threshold: small enough that every boundary object is above it, keeping CI fast.
const THRESHOLD: u64 = 64 * 1024;

/// Part-size unit: 1 MiB.
const MIB: usize = 1024 * 1024;

/// `RustFS` mandates that all parts except the last be at least 5 MiB
/// (<https://docs.rustfs.com/en/concepts/limit>), matching [`kuchiyose`]'s default part size.
const PART_MIN: usize = 5 * MIB;

/// Unique-per-run object key component, so concurrent/rerun CI jobs never collide.
fn run_id() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time is before the unix epoch")
        .as_nanos()
}

fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

fn sha384_hex(data: &[u8]) -> String {
    hex::encode(Sha384::digest(data))
}

fn test_data(len: usize, seed: u8) -> Vec<u8> {
    (0..len)
        .map(|i| {
            u8::try_from(i % 256)
                .expect("a value reduced modulo 256 always fits in u8")
                .wrapping_mul(31)
                .wrapping_add(seed)
        })
        .collect()
}

/// Build the remote backend under test.
///
/// Returns `None` (callers skip) unless `SUBATOMIC_S3_TESTS=1`. Otherwise builds via
/// [`object_store::aws::AmazonS3Builder::from_env`], mirroring `StorageConfig::Aws` in
/// `src/config.rs`, and panics if that fails: S3 tests
/// must never silently fall back to [`StoreBackend::Local`].
fn remote_backend() -> Option<StoreBackend> {
    if std::env::var("SUBATOMIC_S3_TESTS").as_deref() != Ok("1") {
        return None;
    }
    let store = object_store::aws::AmazonS3Builder::from_env().build().expect(
        "S3 integration tests require a working S3-compatible service; \
         check AWS_ENDPOINT, AWS_BUCKET_NAME, credentials, and bucket creation",
    );
    let backend = StoreBackend::Remote(Arc::new(store));
    assert!(matches!(backend, StoreBackend::Remote(_)), "must test the remote backend");
    Some(backend)
}

/// Unwrap the gate and bind the caller-named `store` + `s3`, or skip cleanly when
/// `SUBATOMIC_S3_TESTS` is unset.
macro_rules! require_s3 {
    ($store:ident, $inner:ident) => {
        let Some($store) = remote_backend() else {
            eprintln!(
                "skipping {}: set SUBATOMIC_S3_TESTS=1 with AWS_* env to run S3 integration tests",
                module_path!()
            );
            return;
        };
        let $inner = inner(&$store);
    };
}

/// Extract the inner object store; the backend is always [`StoreBackend::Remote`] here.
fn inner(store: &StoreBackend) -> Arc<dyn object_store::ObjectStore> {
    match store {
        StoreBackend::Remote(inner) => Arc::clone(inner),
        _ => panic!("S3 tests must run against the remote backend, not local storage"),
    }
}

async fn get_bytes(
    store: &Arc<dyn object_store::ObjectStore>,
    link: &Link,
) -> object_store::Result<Vec<u8>> {
    let got = store.get(&link.to_storepath()).await?;
    Ok(got.bytes().await?.to_vec())
}

/// Reader that yields `data` then fails once `fail_after` bytes were read.
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
    ) -> Poll<std::io::Result<()>> {
        if self.pos >= self.fail_after {
            return Poll::Ready(Err(std::io::Error::other("injected stream failure")));
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

#[tokio::test]
async fn s3_small_object_roundtrip() {
    require_s3!(store, s3);
    let link = LinkBuf::from(format!("ci-s3/{}/small-1-1.noarch.rpm", run_id()));
    let link = link.as_link();
    store.delete(link).await.expect("pre-cleanup");
    let tmp = tempfile::tempdir().expect("tempdir");
    let tmp_path = tmp.path().join("pkg.rpm");
    let data = test_data(16 * 1024, 7);

    let out = store
        .put_streamed(PutStreamedParams::new(link, &tmp_path, THRESHOLD, Ftmm::Sha256), &data[..])
        .await
        .expect("small S3 upload must succeed");
    assert!(!out.used_multipart, "small object must use a single PUT");
    assert_eq!(out.size, u64::try_from(data.len()).expect("len fits"), "size mismatch");
    assert_eq!(out.digest_hex, sha256_hex(&data));
    assert_eq!(out.ftmm, Ftmm::Sha256);

    let back = get_bytes(&s3, link).await.expect("uploaded object must be readable");
    assert_eq!(back, data, "round-tripped contents must be byte-identical");
    assert_eq!(sha256_hex(&back), out.digest_hex);

    store.delete(link).await.expect("cleanup");
}

#[tokio::test]
async fn s3_multipart_exact_boundaries() {
    require_s3!(store, s3);
    let run = run_id();
    let tmp = tempfile::tempdir().expect("tempdir");
    let tmp_path = tmp.path().join("pkg.rpm");

    // RustFS enforces the S3 standard (non-last parts >= 5 MiB, last part unconstrained) but
    // not R2's stricter same-size rule, so these layouts are all valid multipart objects.
    for (name, len) in [("5mib", PART_MIN), ("10mib", 2 * PART_MIN), ("10mib+1", 2 * PART_MIN + 1)]
    {
        let link = LinkBuf::from(format!("ci-s3/{run}/multipart-{name}.noarch.rpm"));
        let link = link.as_link();
        store.delete(link).await.expect("pre-cleanup");
        let data = test_data(len, 11);

        let out = store
            .put_streamed(
                PutStreamedParams::new(link, &tmp_path, THRESHOLD, Ftmm::Sha256),
                &data[..],
            )
            .await
            .expect("multipart S3 upload must succeed");
        assert!(out.used_multipart, "{name}: object above threshold must use multipart upload");
        assert_eq!(out.size, u64::try_from(len).expect("len fits"), "{name}: size mismatch");
        assert_eq!(out.digest_hex, sha256_hex(&data), "{name}: digest mismatch");

        let back = get_bytes(&s3, link).await.expect("uploaded object must be readable");
        assert_eq!(back, data, "{name}: round-tripped contents must be byte-identical");
        assert_eq!(sha256_hex(&back), out.digest_hex);

        store.delete(link).await.expect("cleanup");
    }
}

#[tokio::test]
async fn s3_abort_after_threshold_leaves_no_object() {
    require_s3!(store, s3);
    let link = LinkBuf::from(format!("ci-s3/{}/aborted-1-1.noarch.rpm", run_id()));
    let link = link.as_link();
    store.delete(link).await.expect("pre-cleanup");
    let tmp = tempfile::tempdir().expect("tempdir");
    let tmp_path = tmp.path().join("pkg.rpm");
    let data = test_data(200 * 1024, 13);
    // Fail well past the threshold so a multipart upload was started and must be aborted.
    let body = FailReader { data, pos: 0, fail_after: 128 * 1024 };

    let err = store
        .put_streamed(PutStreamedParams::new(link, &tmp_path, THRESHOLD, Ftmm::Sha256), body)
        .await
        .expect_err("failing stream must fail the upload");
    assert!(
        matches!(err, StreamErr::Source(_)),
        "stream failure must stay a client error, got: {err:?}"
    );

    // `AbortMultipartUpload` is invoked before the error is returned, so the object should be
    // absent immediately; retry briefly to tolerate server-side abort lag.
    let mut attempt = 0u32;
    loop {
        match s3.get(&link.to_storepath()).await {
            Err(object_store::Error::NotFound { .. }) => break,
            Err(e) => panic!("object lookup must not fail for other reasons: {e:?}"),
            Ok(_) if attempt < 5 => {
                attempt += 1;
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
            Ok(_) => panic!("aborted upload must leave no object behind"),
        }
    }

    // Best-effort cleanup; `delete` maps NotFound to success (StoreBackend::delete contract).
    store.delete(link).await.expect("cleanup must succeed on an already-absent object");
}

#[tokio::test]
async fn s3_multipart_writer_partial_poll_write_and_flush() {
    require_s3!(store, s3);
    let link = LinkBuf::from(format!("ci-s3/{}/writer-1-1.noarch.rpm", run_id()));
    let link = link.as_link();
    store.delete(link).await.expect("pre-cleanup");

    // Drive the real writer against the real store: at least two full 5 MiB parts plus a
    // final partial part, written through `poll_write` in odd-sized slices with interposed
    // flushes (flush must drain, never finalize; `shutdown` completes the upload).
    let upload = s3.put_multipart(&link.to_storepath()).await.expect("multipart init");
    let mut writer = MultipartUploadWriter::with_defaults(upload);
    let slice_sizes = [1usize, 7, 4093, 65_536];
    let mut expected = Vec::with_capacity(2 * PART_MIN + 65536);
    let mut slice_idx = 0usize;
    while expected.len() < 2 * PART_MIN {
        // seed varies deterministically per slice and always fits in u8.
        let seed = 101u8.wrapping_add(u8::try_from(slice_idx % 128).expect("seed always fits"));
        let chunk = test_data(slice_sizes[slice_idx % slice_sizes.len()], seed);
        writer.write_all(&chunk).await.expect("write to multipart writer");
        expected.extend_from_slice(&chunk);
        if slice_idx % 4 == 3 {
            writer.flush().await.expect("flush must not finalize the upload");
        }
        slice_idx += 1;
    }
    writer.shutdown().await.expect("shutdown must complete the upload");

    let back = get_bytes(&s3, link).await.expect("uploaded object must be readable");
    assert_eq!(back, expected, "round-tripped contents must be byte-identical");
    assert_eq!(sha256_hex(&back), sha256_hex(&expected));

    store.delete(link).await.expect("cleanup");
}

#[tokio::test]
async fn s3_checksum_algorithm_propagates() {
    require_s3!(store, s3);
    let link = LinkBuf::from(format!("ci-s3/{}/sha384-1-1.noarch.rpm", run_id()));
    let link = link.as_link();
    store.delete(link).await.expect("pre-cleanup");
    let tmp = tempfile::tempdir().expect("tempdir");
    let tmp_path = tmp.path().join("pkg.rpm");
    let data = test_data(64 * 1024, 19);

    let out = store
        .put_streamed(PutStreamedParams::new(link, &tmp_path, THRESHOLD, Ftmm::Sha384), &data[..])
        .await
        .expect("sha384 S3 upload must succeed");
    // The server compares `digest_hex` (stored hex) against the client-supplied checksum;
    // the algorithm itself must round-trip on the result.
    assert_eq!(out.ftmm, Ftmm::Sha384);
    assert_eq!(out.digest_hex, sha384_hex(&data));

    let back = get_bytes(&s3, link).await.expect("uploaded object must be readable");
    assert_eq!(sha384_hex(&back), sha384_hex(&data));

    store.delete(link).await.expect("cleanup");
}
