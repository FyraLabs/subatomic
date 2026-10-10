use std::os::unix::ffi::OsStrExt;

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("bad filename: `Path::file_name` returned `None`")]
    BadFileName,
    #[error("not utf-8: {0}")]
    NotUtf8(#[from] std::str::Utf8Error),
}

/// Bytestr guaranteed to be a valid utf-8 filename.
///
/// This is equivalent to in Nim:
/// ```nim
/// type Kiri = distinct string
/// ```
/// but with the guarantees that the inner `str` must be a valid filename.
#[repr(transparent)]
#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Kiri(str);

impl Kiri {
    /// Treat `bytes` as a valid filename and cast directly.
    pub unsafe fn from_bytes_unchecked(bytes: &[u8]) -> &Self {
        // SAFETY: `Kiri` is `#[repr(transparent)]` over `str` and `[u8]`.
        unsafe { &*(std::ptr::from_ref(bytes) as *const Kiri) }
    }

    /// Treat `s` as a valid filename and cast directly.
    pub unsafe fn from_str_unchecked(s: &str) -> &Self {
        // SAFETY: `Kiri` is `#[repr(transparent)]` over `str` and `[u8]`.
        unsafe { &*(std::ptr::from_ref(s) as *const Kiri) }
    }

    /// Check that the given string is a valid utf-8 filename and cast.
    pub fn from_bytes(bytes: &[u8]) -> Result<&Self, Error> {
        Self::from_str(core::str::from_utf8(bytes)?)
    }

    /// Check that the given string is a valid filename and cast.
    pub fn from_str(s: &str) -> Result<&Self, Error> {
        if s == "." || s == ".." || s.contains('/') || s.is_empty() {
            return Err(Error::BadFileName);
        }
        // SAFETY: confirmed by above that s is a valid filename
        Ok(unsafe { Self::from_str_unchecked(s) })
    }

    pub fn from_link(l: &crate::Link) -> Result<&Self, Error> {
        let bytes = l.as_path().file_name().ok_or(Error::BadFileName)?.as_bytes();
        // SAFETY: `l` is valid utf-8
        Ok(unsafe { Self::from_str_unchecked(str::from_utf8_unchecked(bytes)) })
    }

    /// Obtain from a path.
    pub fn from_path(p: &std::path::Path) -> Result<&Self, Error> {
        // bytes must be valid filename
        let bytes = p.file_name().ok_or(Error::BadFileName)?.as_bytes();
        // SAFETY: str::from_utf8(bytes) must be valid utf-8 filename
        Ok(unsafe { Self::from_str_unchecked(str::from_utf8(bytes)?) })
    }

    /// Parse the input as a path and obtain the filename.
    pub fn new<S: AsRef<std::ffi::OsStr>>(s: &S) -> Result<&Self, Error> {
        Self::from_path(std::path::Path::new(s))
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Owned [`Kiri`].
#[repr(transparent)]
#[derive(Clone, Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Kirifuda(String);

impl Kirifuda {
    pub fn to_string(&self) -> String {
        self.0.to_owned()
    }
    pub fn into_string(self) -> String {
        self.0
    }

    pub fn from_string(s: String) -> Result<Self, Error> {
        if s == "." || s == ".." || s.contains('/') || s.is_empty() {
            return Err(Error::BadFileName);
        }
        // SAFETY: confirmed by above that s is a valid filename
        Ok(unsafe { Self::from_string_unchecked(s) })
    }

    pub unsafe fn from_string_unchecked(s: String) -> Self {
        Self(s)
    }
}

impl std::ops::Deref for Kirifuda {
    type Target = Kiri;

    fn deref(&self) -> &Self::Target {
        // SAFETY: Kirifuda guarantees it is also a valid Kiri
        unsafe { Kiri::from_str_unchecked(&*self.0) }
    }
}

impl std::borrow::Borrow<Kiri> for Kirifuda {
    fn borrow(&self) -> &Kiri {
        unsafe { Kiri::from_str_unchecked(&*self.0) }
    }
}

impl ToOwned for Kiri {
    type Owned = Kirifuda;

    fn to_owned(&self) -> Self::Owned {
        Kirifuda(self.0.to_owned())
    }
}

impl AsRef<Kiri> for Kiri {
    fn as_ref(&self) -> &Kiri {
        self
    }
}

impl AsRef<Kiri> for Kirifuda {
    fn as_ref(&self) -> &Kiri {
        self
    }
}
