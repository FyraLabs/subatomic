//! Repository-relative paths.
//!
//! [`Link`] and [`LinkBuf`] are the borrowed and owned forms of a path inside a
//! repository. They are to repository paths what [`std::path::Path`] and
//! [`std::path::PathBuf`] are to filesystem paths.
//!
//! Unlike [`std::path::Path`], a `Link` is always UTF-8. This is because RPM repository
//! metadata (XML `href` attributes, `repomd.xml` locations, object-store keys) is UTF-8.
//!
//! Do not use `Link`/`LinkBuf` for paths that must be local (e.g. a temporary RPM file
//! being parsed). Use `std::path::{Path, PathBuf}` there.

use std::borrow::Borrow;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// A borrowed, repository-relative path.
#[repr(transparent)]
pub struct Link(str);

/// An owned, repository-relative path.
#[expect(clippy::unsafe_derive_deserialize)]
#[derive(
    Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct LinkBuf(String);

impl LinkBuf {
    #[inline]
    #[must_use]
    pub const fn new() -> Self {
        Self(String::new())
    }

    /// Cheap borrow as `&Link`.
    #[inline]
    #[must_use]
    pub const fn as_link(&self) -> &Link {
        // SAFETY: `Link` is `#[repr(transparent)]` over `str`.
        unsafe { &*(std::ptr::from_ref(self.0.as_str()) as *const Link) }
    }

    #[inline]
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[inline]
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    #[inline]
    #[must_use]
    pub fn as_path(&self) -> &Path {
        Path::new(&self.0)
    }

    #[inline]
    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }

    /// Allocates a new `object_store::path::Path`.
    #[must_use]
    pub fn to_storepath(&self) -> object_store::path::Path {
        object_store::path::Path::from(self.0.as_str())
    }

    #[must_use]
    pub fn from_storepath(p: &object_store::path::Path) -> Self {
        Self(p.as_ref().to_owned())
    }

    pub(crate) fn push<L: AsRef<Link>>(&mut self, path: L) {
        // PERF: char is slower than bytes but if you ask me whether this matters…
        while self.0.ends_with('/') {
            self.0.pop();
        }
        self.0.push('/');
        // NOTE: we differ from std's impl by the sense that `path.starts_with('/')` is allowed?
        self.0.push_str(path.as_ref().as_str());
    }

    // pub(crate) fn pop(&mut self) {
    //     // PERF: feels stupid
    //     while self.0.ends_with('/') {
    //         self.0.pop();
    //     }
    //     while !self.0.ends_with('/') {
    //         self.0.pop();
    //     }
    // }
}

impl Link {
    #[must_use]
    pub const fn new(str: &str) -> &Self {
        // SAFETY: `Link` is `#[repr(transparent)]` over `str`.
        unsafe { &*(std::ptr::from_ref(str) as *const Self) }
    }

    #[inline]
    #[must_use]
    pub const fn as_str(&self) -> &str {
        &self.0
    }

    #[inline]
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    #[inline]
    #[must_use]
    pub fn as_path(&self) -> &Path {
        Path::new(&self.0)
    }

    /// Allocates a new [`object_store::path::Path`].
    #[must_use]
    pub fn to_storepath(&self) -> object_store::path::Path {
        object_store::path::Path::from(&self.0)
    }

    #[must_use]
    pub fn to_linkbuf(&self) -> LinkBuf {
        LinkBuf(self.0.to_owned())
    }

    #[must_use]
    pub fn join<L: AsRef<Self>>(&self, path: L) -> LinkBuf {
        let path = path.as_ref();
        let mut buf = self.to_linkbuf();
        buf.push(path);
        buf
    }

    #[must_use]
    pub fn parent(&self) -> Option<&Self> {
        Some(Self::new(self.0.trim_suffix('/').rsplit_once('/')?.0))
    }
}

impl PartialEq for Link {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl Eq for Link {}
impl PartialOrd for Link {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Link {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}
impl std::hash::Hash for Link {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl std::ops::Deref for Link {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

impl std::ops::Deref for LinkBuf {
    type Target = Link;
    fn deref(&self) -> &Link {
        self.as_link()
    }
}

impl ToOwned for Link {
    type Owned = LinkBuf;
    fn to_owned(&self) -> LinkBuf {
        LinkBuf(self.0.to_owned())
    }
}

impl Borrow<Link> for LinkBuf {
    fn borrow(&self) -> &Link {
        self.as_link()
    }
}

impl AsRef<Self> for Link {
    fn as_ref(&self) -> &Self {
        self
    }
}
impl AsRef<Link> for LinkBuf {
    fn as_ref(&self) -> &Link {
        self.as_link()
    }
}
impl AsRef<str> for Link {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
impl AsRef<str> for LinkBuf {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
impl AsRef<[u8]> for Link {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}
impl AsRef<[u8]> for LinkBuf {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}
impl AsRef<Path> for Link {
    fn as_ref(&self) -> &Path {
        Path::new(&self.0)
    }
}
impl AsRef<Path> for LinkBuf {
    fn as_ref(&self) -> &Path {
        Path::new(&self.0)
    }
}
impl AsRef<OsStr> for Link {
    fn as_ref(&self) -> &OsStr {
        OsStr::new(&self.0)
    }
}
impl AsRef<OsStr> for LinkBuf {
    fn as_ref(&self) -> &OsStr {
        OsStr::new(&self.0)
    }
}
impl AsRef<Link> for String {
    fn as_ref(&self) -> &Link {
        Link::new(self)
    }
}
impl AsRef<Link> for str {
    fn as_ref(&self) -> &Link {
        Link::new(self)
    }
}

impl std::fmt::Display for Link {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::fmt::Display for LinkBuf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::fmt::Debug for Link {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.0, f)
    }
}
impl std::fmt::Debug for LinkBuf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.0, f)
    }
}

impl serde::Serialize for Link {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl From<&str> for LinkBuf {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}
impl From<String> for LinkBuf {
    fn from(s: String) -> Self {
        Self(s)
    }
}
impl From<&String> for LinkBuf {
    fn from(s: &String) -> Self {
        Self(s.clone())
    }
}
impl From<&Link> for LinkBuf {
    fn from(l: &Link) -> Self {
        l.to_linkbuf()
    }
}
impl From<&OsStr> for LinkBuf {
    fn from(s: &OsStr) -> Self {
        Self(s.to_str().expect("invalid utf-8").to_owned())
    }
}
impl From<&[u8]> for LinkBuf {
    fn from(s: &[u8]) -> Self {
        Self(core::str::from_utf8(s).expect("invalid utf-8").to_owned())
    }
}
impl From<&Path> for LinkBuf {
    fn from(p: &Path) -> Self {
        Self(p.to_str().expect("invalid utf-8").to_owned())
    }
}
impl From<PathBuf> for LinkBuf {
    fn from(p: PathBuf) -> Self {
        Self(p.to_str().expect("invalid utf-8").to_owned())
    }
}
impl From<&object_store::path::Path> for LinkBuf {
    fn from(p: &object_store::path::Path) -> Self {
        Self(p.as_ref().to_owned())
    }
}
impl From<object_store::path::Path> for LinkBuf {
    fn from(p: object_store::path::Path) -> Self {
        Self(p.into())
    }
}
impl From<&Link> for object_store::path::Path {
    fn from(l: &Link) -> Self {
        l.to_storepath()
    }
}
impl From<&LinkBuf> for object_store::path::Path {
    fn from(l: &LinkBuf) -> Self {
        l.to_storepath()
    }
}
