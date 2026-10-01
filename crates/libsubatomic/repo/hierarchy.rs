//! Repo hierarchy, which determines the path to RPMs and XML metadata files.

use futures::prelude::*;
use std::{ffi::OsStr, os::unix::ffi::OsStrExt};

// today years old when I realize many rust traits are named after English verbs
pub trait Hierarchize: Clone + std::fmt::Debug {
    /// The root / base directory. All files MUST reside in this directory.
    ///
    /// This is usually the directory that contains the `repodata/` subdirectory.
    /// This is analogous to the directory the `baseurl` field represents in `dnf`.
    ///
    /// Returns a valid path in [`object_store::ObjectStore`].
    fn basedir(&self) -> &str;

    /// Obtain the expected relative path to an rpm package in the repository.
    ///
    /// The `filename` SHOULD be obtained by means analogous to [`Path::file_name`], i.e. it MUST
    /// not contain the `/` symbol.
    fn locate_relative(&self, filename: impl AsRef<OsStr>) -> Option<impl AsRef<OsStr>>;

    fn iter_rpms(
        &self,
        store: &impl object_store::ObjectStore,
    ) -> impl Future<Output = impl Stream<Item = object_store::Result<object_store::ObjectMeta>>> + Send;
}

#[derive(Clone, Debug)]
pub struct Satm0FlatHierarchy {
    pub base: object_store::path::Path,
}

impl Hierarchize for Satm0FlatHierarchy {
    fn basedir(&self) -> &str {
        self.base.as_ref()
    }
    fn locate_relative(&self, filename: impl AsRef<OsStr>) -> Option<impl AsRef<OsStr>> {
        Some(filename)
    }

    fn iter_rpms(
        &self,
        store: &impl object_store::ObjectStore,
    ) -> impl Future<Output = impl Stream<Item = object_store::Result<object_store::ObjectMeta>>>
    {
        future::ready(
            store
                .list(Some(&self.base))
                .try_filter(|obj| future::ready(obj.location.extension() == Some("rpm"))),
        )
    }
}

#[derive(Clone, Debug)]
pub struct FedoraHierarchy {
    pub base: object_store::path::Path,
}

impl Hierarchize for FedoraHierarchy {
    fn basedir(&self) -> &str {
        self.base.as_ref()
    }
    fn locate_relative(&self, filename: impl AsRef<OsStr>) -> Option<impl AsRef<OsStr>> {
        // Packages/{filename[0]}/{filename}
        Some(
            std::path::Path::new(self.base.as_ref())
                .join("Packages")
                .join(OsStr::from_bytes([*filename.as_ref().as_bytes().first()?].as_slice()))
                .join(filename.as_ref()),
        )
    }

    fn iter_rpms(
        &self,
        store: &impl object_store::ObjectStore,
    ) -> impl Future<Output = impl Stream<Item = object_store::Result<object_store::ObjectMeta>>>
    {
        future::ready(
            store
                .list(Some(&self.base.clone().join("Packages")))
                .try_filter(|obj| future::ready(obj.location.extension() == Some("rpm"))),
        )
    }
}
