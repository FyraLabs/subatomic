//! Repo hierarchy, which determines the path to RPMs and XML metadata files.

use futures::prelude::*;
use itertools::Itertools;
use kuchiyose::{Link, LinkBuf, store::StoreBackend};

// today years old when I realize many rust traits are named after English verbs
pub trait Hierarchize: Clone + std::fmt::Debug + Send + Sync {
    /// The repository root.
    ///
    /// This is usually the directory that contains the `repodata/` subdirectory.
    /// This is analogous to the directory the `baseurl` field represents in `dnf`.
    ///
    /// * [`StoreBackend::Local`] — an absolute filesystem path.
    /// * [`StoreBackend::Remote`] — a prefix inside the object store.
    ///
    /// Both are `Link` (UTF-8). For local, we require UTF-8 paths, which is
    /// the same restriction we already impose on hrefs.
    fn basedir(&self) -> &Link;

    /// Path to an RPM, **relative to [`Self::basedir`]**.
    ///
    /// This is what ends up as the `<location href>` and as the LMDB key.
    fn locate_relative(&self, filename: &crate::Kiri) -> Option<LinkBuf>;

    /// Every `.rpm` under the repo, **relative to [`Self::basedir`]**.
    ///
    /// Dispatches on the store: `Local` walks the filesystem (can use `jwalk`
    /// or `std::fs`, both much faster than `LocalFileSystem`), `Remote` lists
    /// the object store.
    fn iter_rpms(
        &self,
        store: &StoreBackend,
    ) -> impl Future<Output = impl Stream<Item = object_store::Result<LinkBuf>> + Send> + Send;
}

#[derive(Clone, Debug)]
pub struct Satm0FlatHierarchy {
    pub base: LinkBuf,
}

impl Hierarchize for Satm0FlatHierarchy {
    fn basedir(&self) -> &Link {
        self.base.as_link()
    }

    fn locate_relative(&self, filename: &crate::Kiri) -> Option<LinkBuf> {
        Some(LinkBuf::from(filename.as_str()))
    }

    fn iter_rpms(
        &self,
        store: &StoreBackend,
    ) -> impl Future<Output = impl Stream<Item = object_store::Result<LinkBuf>> + Send> + Send {
        let base = self.base.clone();
        async move {
            match store {
                StoreBackend::Local => Box::pin(futures::stream::iter(
                    jwalk::WalkDir::new(base.as_str())
                        .into_iter()
                        .filter_ok(|e| {
                            e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("rpm"))
                        })
                        .map_ok(move |e| {
                            e.path()
                                .strip_prefix(base.as_path())
                                .map_or_else(|_| LinkBuf::from(e.path()), LinkBuf::from)
                        })
                        .map(|r| {
                            r.map_err(|e| object_store::Error::Generic {
                                store: "local",
                                source: Box::new(e),
                            })
                        }),
                ))
                    as std::pin::Pin<Box<dyn Stream<Item = object_store::Result<LinkBuf>> + Send>>,
                StoreBackend::Remote(obj_store) => {
                    let prefix = base.to_storepath();
                    let base_len = self.base.as_str().len() + 1;
                    Box::pin(
                        obj_store
                            .list(Some(&prefix))
                            .map_ok(move |m| LinkBuf::from(&m.location.as_ref()[base_len..]))
                            .map(|r| r),
                    )
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct FedoraHierarchy {
    pub base: LinkBuf,
}

impl Hierarchize for FedoraHierarchy {
    fn basedir(&self) -> &Link {
        self.base.as_link()
    }

    fn locate_relative(&self, filename: &crate::Kiri) -> Option<LinkBuf> {
        let first = filename.as_str().chars().next().expect("empty Kiri");
        Some(LinkBuf::from(format!("Packages/{first}/{}", filename.as_str())))
    }

    fn iter_rpms(
        &self,
        store: &StoreBackend,
    ) -> impl Future<Output = impl Stream<Item = object_store::Result<LinkBuf>> + Send> + Send {
        let base = self.base.join("Packages");
        async move {
            match store {
                StoreBackend::Local => Box::pin(futures::stream::iter(
                    jwalk::WalkDir::new(base.as_str())
                        .into_iter()
                        .filter_ok(|e| {
                            e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("rpm"))
                        })
                        .map_ok(move |e| {
                            e.path()
                                .strip_prefix(base.as_path())
                                .map_or_else(|_| LinkBuf::from(e.path()), LinkBuf::from)
                        })
                        .map(|r| {
                            r.map_err(|e| object_store::Error::Generic {
                                store: "local",
                                source: Box::new(e),
                            })
                        }),
                ))
                    as std::pin::Pin<Box<dyn Stream<Item = object_store::Result<LinkBuf>> + Send>>,
                StoreBackend::Remote(obj_store) => {
                    let prefix = base.to_storepath();
                    let base_len = self.base.as_str().len() + 1;
                    Box::pin(
                        obj_store
                            .list(Some(&prefix))
                            .map_ok(move |m| LinkBuf::from(&m.location.as_ref()[base_len..]))
                            .map(|r| r),
                    )
                }
            }
        }
    }
}
