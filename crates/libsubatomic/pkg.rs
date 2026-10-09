//! Module that contains shared struct implementations used in [`crate::repodata`] and a minimal
//! [`Package`] struct.

use sha2::Digest;

use crate::prelude::*;
pub(crate) use crate::repodata::MetanInput;

pub type ParsePathOutput<'a> = kuchiyose::rpm::ParsePathOutput<'a>;

#[deprecated = "use kuchiyose::rpm::parse_filename"]
#[must_use]
pub fn parse_filename(filename: &[u8]) -> Option<ParsePathOutput<'_>> {
    kuchiyose::rpm::parse_filename(filename)
}

// Minimum representation for an RPM package.
#[derive(Clone, Debug)]
pub struct Package {
    pub name: String,
    pub arch: String,
    pub version: Version,
    pub checksum: String,
    pub summary: String,
    pub description: String,
    pub packager: Option<String>,
    pub url: Option<String>,
    pub time: Time,
    pub size: Size,
    /// Other metadata
    ///
    /// WARN: we are using `format.files` to store all files, but in
    /// [`crate::repodata::primary::PrimaryMetadata`] they are stored only if
    /// [`FileEntry::is_primary()`].
    pub format: Format,
    pub changelog: Vec<Changelog>,
    pub appstream_frag: Vec<u8> = Vec::new(),
}
impl Package {
    #[must_use]
    pub fn is_appstream_file(path: &Path) -> bool {
        path.starts_with("/usr/share/metainfo/") && path.extension().is_some_and(|ext| ext == "xml")
    }

    /// Generate appstream fragment for this rpm package using [`crate::repodata::appstream::transform`].
    ///
    /// # Performance
    /// This operation is slightly expensive and requires decompressing specific files in the archive.
    /// This requires a linear search against the full list of files in the rpm. Documentation from
    /// [`rpm::PackageReader::next_file`] suggests only wanted files are decompressed.
    ///
    /// # Errors
    /// RPM errors are propagated. If parsing an appstream xml file failed, no errors will be
    /// returned and a warning ([`tracing::warn!`]) will be issued instead.
    pub fn appstream_frag(rpm: &mut rpm::PackageReader) -> Result<Vec<u8>, rpm::Error> {
        // PERF: do we need this search beforehand?
        if !rpm.metadata.get_file_entries()?.into_iter().any(|f| Self::is_appstream_file(&f.path()))
        {
            return Ok(Vec::new());
        }
        let mut appstream_frag = Vec::new();
        let pkgname = rpm.metadata.get_name()?.to_owned();
        while let Some(mut f) = rpm.next_file()? {
            if Self::is_appstream_file(&f.metadata.path()) {
                let size = f.metadata.size();
                if let Err(e) = crate::repodata::appstream::transform(
                    &pkgname,
                    std::io::BufReader::new(&mut f),
                    // TODO: what to do if size too large in mem?
                    Some(size),
                    &mut appstream_frag,
                ) {
                    tracing::warn!(
                        pkgname,
                        path = %f.metadata.path().display(),
                        ?e,
                        "cannot parse appstream xml"
                    );
                }
            }
            f.finish()?;
        }
        Ok(appstream_frag)
    }

    // https://github.com/madonuko/createrepo_nim/blob/719b99a469101c61441623f9fecfd3c7d977fbcb/src/rpm.nim#L160
    // https://github.com/rpm-software-management/createrepo_c/blob/5cf41fe5d703901d78078ed18c67ab667e446c1a/src/misc.c#L248
    fn get_header_byte_range(f: &mut std::fs::File) -> std::io::Result<HeaderRange> {
        f.seek(std::io::SeekFrom::Start(104))?;
        let mut bytes = [0u8; 2];
        f.read_exact(&mut bytes)?;
        let sigindex = bytes[0].to_be();
        let sigdata = bytes[1].to_be();
        let sigindexsize = sigindex * 16;
        let sigsize = u64::from(sigdata) + u64::from(sigindexsize);
        let mut disttoboundary = sigsize % 8;
        if disttoboundary != 0 {
            disttoboundary = 8 - disttoboundary;
        }
        let hdrstart: u64 = 112 + sigsize + disttoboundary;

        f.seek(std::io::SeekFrom::Start(hdrstart + 8))?;
        f.read_exact(&mut bytes)?;
        let hdrindex = u64::from(bytes[0].to_be());
        let hdrdata = u64::from(bytes[1].to_be());
        let hdrindexsize = hdrindex * 16;
        let hdrsize = hdrdata + hdrindexsize + 16;
        let hdrend = hdrstart + hdrsize;
        if hdrend < hdrstart {
            return Err(std::io::Error::other(format!(
                "sanity check fail (hdrend {hdrend} < hdrstart {hdrstart})"
            )));
        }
        Ok(HeaderRange { start: hdrstart, end: hdrend })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Version {
    #[serde(rename = "@epoch")]
    pub epoch: u64,
    #[serde(rename = "@ver")]
    pub ver: String,
    #[serde(rename = "@rel")]
    pub rel: String,
}

impl Version {
    #[must_use]
    pub fn parse(value: &str) -> Self {
        let (epoch, value) = (value.split_once(':'))
            .and_then(|(e, v)| Some((e.parse().ok()?, v)))
            .unwrap_or((0, value));
        let (ver, rel) = value.split_once('-').unwrap_or((value, ""));
        let (ver, rel) = (ver.into(), rel.into());
        Self { epoch, ver, rel }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Time {
    #[serde(rename = "@file")]
    pub file: u64,
    #[serde(rename = "@build")]
    pub build: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Size {
    #[serde(rename = "@package")]
    pub package: u64,
    #[serde(rename = "@installed")]
    pub installed: u64,
    // archive size seems to be optional on some packages when testing with terra44 dataset,
    // so we can avoid serializing it if it's not present
    #[serde(rename = "@archive", skip_serializing_if = "Option::is_none")]
    pub archive: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Format {
    #[serde(rename = "rpm:license")]
    pub license: String,
    #[serde(rename = "rpm:vendor", skip_serializing_if = "Option::is_none")]
    pub vendor: Option<String>,
    #[serde(rename = "rpm:group", skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(rename = "rpm:buildhost", skip_serializing_if = "Option::is_none")]
    pub buildhost: Option<String>,
    #[serde(rename = "rpm:sourcerpm", skip_serializing_if = "Option::is_none")]
    pub sourcerpm: Option<String>,
    #[serde(rename = "rpm:header-range")]
    pub header_range: HeaderRange,
    #[serde(rename = "rpm:requires", default, skip_serializing_if = "Dependencies::is_empty")]
    pub requires: Dependencies,
    #[serde(rename = "rpm:provides", default, skip_serializing_if = "Dependencies::is_empty")]
    pub provides: Dependencies,
    #[serde(rename = "rpm:conflicts", default, skip_serializing_if = "Dependencies::is_empty")]
    pub conflicts: Dependencies,
    #[serde(rename = "rpm:obsoletes", default, skip_serializing_if = "Dependencies::is_empty")]
    pub obsoletes: Dependencies,
    #[serde(rename = "rpm:recommends", default, skip_serializing_if = "Dependencies::is_empty")]
    pub recommends: Dependencies,
    #[serde(rename = "rpm:suggests", default, skip_serializing_if = "Dependencies::is_empty")]
    pub suggests: Dependencies,
    #[serde(rename = "rpm:supplements", default, skip_serializing_if = "Dependencies::is_empty")]
    pub supplements: Dependencies,
    #[serde(rename = "rpm:enhances", default, skip_serializing_if = "Dependencies::is_empty")]
    pub enhances: Dependencies,
    #[serde(rename = "file", default)]
    pub files: Vec<FileEntry>,
}

#[derive(Clone, Debug, Serialize)]
pub struct HeaderRange {
    #[serde(rename = "@start")]
    pub start: u64,
    #[serde(rename = "@end")]
    pub end: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Dependencies {
    #[serde(rename = "rpm:entry", default)]
    pub entries: Vec<Entry>,
}

impl Dependencies {
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn from_requires(value: Vec<rpm::Dependency>) -> Self {
        Self {
            entries: value
                .into_iter()
                .filter(|dependency| !dependency.flags.contains(rpm::DependencyFlags::RPMLIB))
                .map(Into::into)
                .collect(),
        }
    }
}

impl From<Vec<rpm::Dependency>> for Dependencies {
    fn from(value: Vec<rpm::Dependency>) -> Self {
        Self { entries: value.into_iter().map(Into::into).collect() }
    }
}

#[cfg(test)]
mod dependency_tests {
    use super::{Dependencies, Entry};

    #[test]
    fn excludes_rpmlib_requirements() {
        let dependencies = Dependencies::from_requires(vec![
            rpm::Dependency::rpmlib("CompressedFileNames", "3.0.4-1"),
            rpm::Dependency::greater_eq("glibc", "2.40"),
        ]);

        assert_eq!(dependencies.entries.len(), 1);
        assert_eq!(dependencies.entries[0].name, "glibc");
    }

    /// repodata spells comparators as words; libsolv does not understand the
    /// human-readable sense form (`=`, `<=`, ...) returned by
    /// `DependencyFlags::comparator_str`, which makes providers look missing.
    #[test]
    fn serializes_repodata_comparator_symbols() {
        let cases = [
            (rpm::Dependency::eq("dep", "1-1"), "EQ"),
            (rpm::Dependency::less("dep", "1-1"), "LT"),
            (rpm::Dependency::less_eq("dep", "1-1"), "LE"),
            (rpm::Dependency::greater("dep", "1-1"), "GT"),
            (rpm::Dependency::greater_eq("dep", "1-1"), "GE"),
            (rpm::Dependency::any("dep"), ""),
        ];

        for (dependency, expected) in cases {
            let entry = Entry::from(dependency);
            assert_eq!(entry.flags, expected, "for {}", entry.name);
        }
    }

    /// A versioned dependency must carry `epoch`; createrepo_c always emits it
    /// and consumers rely on its presence.
    #[test]
    fn keeps_the_full_version_including_epoch() {
        let entry = Entry::from(rpm::Dependency::eq("kernel", "5.15.147-17"));
        assert_eq!(entry.flags, "EQ");
        assert_eq!(entry.ver.as_deref(), Some("5.15.147"));
        assert_eq!(entry.rel.as_deref(), Some("17"));
    }
}

#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_zero(&n: &u64) -> bool {
    n == 0
}

#[derive(Clone, Debug, Serialize)]
pub struct Entry {
    #[serde(rename = "@name")]
    pub name: String,
    #[serde(rename = "@flags", default, skip_serializing_if = "str::is_empty")]
    pub flags: &'static str = "",
    #[serde(rename = "@epoch", default, skip_serializing_if = "is_zero")]
    pub epoch: u64 = 0,
    #[serde(rename = "@ver", default, skip_serializing_if = "Option::is_none")]
    pub ver: Option<String> = None,
    #[serde(rename = "@rel", default, skip_serializing_if = "Option::is_none")]
    pub rel: Option<String> = None,
}

/// Map RPM dependency flags to the symbolic comparator used by repodata
const fn repodata_flags(flags: rpm::DependencyFlags) -> &'static str {
    use rpm::DependencyFlags as F;
    const SENSE: F = F::LESS.union(F::GREATER).union(F::EQUAL);

    match flags.intersection(SENSE) {
        f if f.contains(F::LESS) && f.contains(F::EQUAL) => "LE",
        f if f.contains(F::GREATER) && f.contains(F::EQUAL) => "GE",
        f if f.contains(F::LESS) => "LT",
        f if f.contains(F::GREATER) => "GT",
        F::EQUAL => "EQ",
        _ => "",
    }
}

impl From<rpm::Dependency> for Entry {
    fn from(rpm::Dependency { name, flags, version }: rpm::Dependency) -> Self {
        let name = name.into();
        let flags = repodata_flags(flags);
        if flags.is_empty() {
            return Self { name, .. };
        }
        let Version { epoch, ver, rel } = Version::parse(&version);
        let (ver, rel) = (Some(ver), Some(rel));
        Self { name, flags, epoch, ver, rel }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct FileEntry {
    #[serde(rename = "@type", default, skip_serializing_if = "FileType::is_normal")]
    pub file_type: FileType = FileType::Normal,
    #[serde(rename = "$text")]
    pub path: PathBuf,
}

impl FileEntry {
    #[must_use]
    pub fn new<I: Into<PathBuf>>(path: I) -> Self {
        Self { path: path.into(), .. }
    }
    // https://github.com/rpm-software-management/createrepo_c/blob/5cf41fe5d703901d78078ed18c67ab667e446c1a/src/misc.h#L111
    #[must_use]
    pub fn is_primary(&self) -> bool {
        const BIN: &[u8] = b"bin/";

        let p = self.path.as_os_str().as_bytes();

        p.starts_with(b"/etc/")
            || p == b"/usr/lib/sendmail"
            || 'b: {
                for i in 0..p.len() - BIN.len() {
                    if &p[i..i + BIN.len()] == BIN {
                        break 'b true;
                    }
                }
                false
            }
    }
}

impl<'a> From<rpm::FileEntry<'a>> for FileEntry {
    fn from(value: rpm::FileEntry<'a>) -> Self {
        Self {
            file_type: if value.flags().contains(rpm::FileFlags::GHOST) {
                FileType::Ghost
            } else if value.file_type() == rpm::FileType::Dir {
                FileType::Dir
            } else {
                FileType::Normal
            },
            path: value.path(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FileType {
    #[default]
    Normal,
    Dir,
    Ghost,
}

impl FileType {
    #[must_use]
    pub const fn is_normal(&self) -> bool {
        matches!(self, Self::Normal)
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Changelog {
    #[serde(rename = "@author")]
    pub author: String,
    #[serde(rename = "@date")]
    pub date: u64,
    #[serde(rename = "$text")]
    pub text: String,
}

impl From<rpm::ChangelogEntry> for Changelog {
    fn from(rpm::ChangelogEntry { name, timestamp, description }: rpm::ChangelogEntry) -> Self {
        Self { author: name.into(), date: timestamp, text: description.into() }
    }
}

#[deprecated]
pub fn sha256_digest<R: Read>(mut reader: R) -> std::io::Result<String> {
    let mut hasher = sha2::Sha256::new();
    let mut buffer = [0; 10240];

    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }

    Ok(hex::encode(hasher.finalize()).into())
}
