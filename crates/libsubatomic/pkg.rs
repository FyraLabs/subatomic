//! Module that contains shared struct implementations used in [`crate::repodata`] and a minimal
//! [`Package`] struct.

use sha2::Digest;
use std::io::BufReader;

use crate::prelude::*;

#[derive(Clone, Debug, Default)]
pub struct ParsePathOutput<'a> {
    pub name: &'a [u8],
    pub epoch: u64,
    pub ver: &'a [u8],
    pub rel: &'a [u8],
    pub arch: &'a [u8],
}

#[must_use]
pub fn parse_filename(filename: &[u8]) -> Option<ParsePathOutput<'_>> {
    let (nevr, arch) = filename.strip_suffix(b".rpm")?.rsplit_once(|&b| b == b'.')?;
    let (nev, rel) = nevr.rsplit_once(|&b| b == b'-')?;
    let (name, ev) = nev.rsplit_once(|&b| b == b'-')?;
    let (epoch, ver) = ev
        .rsplit_once(|&b| b == b':')
        .and_then(|(ep, ver)| Some((atoi::atoi(ep)?, ver)))
        .unwrap_or((0, ev));
    Some(ParsePathOutput { name, epoch, ver, rel, arch })
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

    /// Parse a file
    pub fn parse(
        mut f: std::fs::File,
        checksum: String,
    ) -> Result<(Self, rpm::PackageReader), rpm::Error> {
        let meta = f.metadata()?;
        let btime = epoch!(meta.created()?);
        let header_range = Self::get_header_byte_range(&mut f)?;
        f.seek(std::io::SeekFrom::Start(0))?;
        let rpm = rpm::PackageReader::parse(BufReader::new(f))?;
        let m = &rpm.metadata;

        Ok((
            Self {
                name: m.get_name()?.into(),
                arch: m.get_arch()?.into(),
                version: Version {
                    epoch: m.get_epoch().unwrap_or(0).into(),
                    ver: m.get_version()?.into(),
                    rel: m.get_release()?.into(),
                },
                checksum,
                summary: m.get_summary().unwrap_or_default().into(),
                description: m.get_description().unwrap_or_default().into(),
                packager: m.get_packager().ok().map(Into::into),
                url: m.get_url().ok().map(Into::into),
                time: Time { file: btime, build: m.get_build_time()? },
                size: Size {
                    package: meta.size(),
                    installed: m.get_installed_size()?,
                    archive: m
                        .header
                        .get_entry_data_as_u64(rpm::IndexTag::RPMTAG_ARCHIVESIZE)
                        .or_else(|_e| {
                            m.header
                                .get_entry_data_as_u32(rpm::IndexTag::RPMTAG_ARCHIVESIZE)
                                .map(u64::from)
                        })
                        .ok(),
                },
                format: Format {
                    license: m.get_license().unwrap_or_default().into(),
                    vendor: m.get_vendor().ok().map(Into::into),
                    group: m.get_group().ok().map(Into::into),
                    buildhost: m.get_build_host().ok().map(Into::into),
                    sourcerpm: m.get_source_rpm().ok().map(Into::into),
                    header_range,
                    requires: Dependencies::from_requires(m.get_requires()?),
                    provides: Dependencies::from(m.get_provides()?),
                    conflicts: Dependencies::from(m.get_conflicts()?),
                    obsoletes: Dependencies::from(m.get_obsoletes()?),
                    recommends: Dependencies::from(m.get_recommends()?),
                    suggests: Dependencies::from(m.get_suggests()?),
                    supplements: Dependencies::from(m.get_supplements()?),
                    enhances: Dependencies::from(m.get_enhances()?),
                    files: m.get_file_entries()?.into_iter().map(Into::into).collect(),
                },
                changelog: m.get_changelog_entries()?.into_iter().map(Into::into).collect(),
                appstream_frag: Vec::new(),
            },
            rpm,
        ))
    }

    /// Open an `.rpm` package.
    ///
    /// # Errors
    /// IO errors and RPM errors may be returned.
    pub fn open(path: &Path) -> Result<(Self, rpm::PackageReader), rpm::Error> {
        let rpm = rpm::PackageReader::open(path)?;
        let m = &rpm.metadata;
        let mut f = std::fs::File::open(path)?;
        let reader = BufReader::new(&mut f);
        let checksum = sha256_digest(reader)?;
        let meta = f.metadata()?;
        let btime = epoch!(meta.created()?);

        Ok((
            Self {
                name: m.get_name()?.into(),
                arch: m.get_arch()?.into(),
                version: Version {
                    epoch: m.get_epoch().unwrap_or(0).into(),
                    ver: m.get_version()?.into(),
                    rel: m.get_release()?.into(),
                },
                checksum,
                summary: m.get_summary().unwrap_or_default().into(),
                description: m.get_description().unwrap_or_default().into(),
                packager: m.get_packager().ok().map(Into::into),
                url: m.get_url().ok().map(Into::into),
                time: Time { file: btime, build: m.get_build_time()? },
                size: Size {
                    package: meta.size(),
                    installed: m.get_installed_size()?,
                    archive: m
                        .header
                        .get_entry_data_as_u64(rpm::IndexTag::RPMTAG_ARCHIVESIZE)
                        .or_else(|_e| {
                            m.header
                                .get_entry_data_as_u32(rpm::IndexTag::RPMTAG_ARCHIVESIZE)
                                .map(u64::from)
                        })
                        .ok(),
                },
                format: Format {
                    license: m.get_license().unwrap_or_default().into(),
                    vendor: m.get_vendor().ok().map(Into::into),
                    group: m.get_group().ok().map(Into::into),
                    buildhost: m.get_build_host().ok().map(Into::into),
                    sourcerpm: m.get_source_rpm().ok().map(Into::into),
                    header_range: Self::get_header_byte_range(&mut f)?,
                    requires: Dependencies::from_requires(m.get_requires()?),
                    provides: Dependencies::from(m.get_provides()?),
                    conflicts: Dependencies::from(m.get_conflicts()?),
                    obsoletes: Dependencies::from(m.get_obsoletes()?),
                    recommends: Dependencies::from(m.get_recommends()?),
                    suggests: Dependencies::from(m.get_suggests()?),
                    supplements: Dependencies::from(m.get_supplements()?),
                    enhances: Dependencies::from(m.get_enhances()?),
                    files: m.get_file_entries()?.into_iter().map(Into::into).collect(),
                },
                changelog: m.get_changelog_entries()?.into_iter().map(Into::into).collect(),
                appstream_frag: Vec::new(),
            },
            rpm,
        ))
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

    fn from_requires(value: Vec<rpm::Dependency>) -> Self {
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
            || p.windows(BIN.len()).any(|w| w == BIN)
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
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};

    #[test]
    fn parse_filename_basic() {
        let out = parse_filename(b"bash-5.2.15-1.fc39.x86_64.rpm").unwrap();
        assert_eq!(out.name, b"bash");
        assert_eq!(out.epoch, 0);
        assert_eq!(out.ver, b"5.2.15");
        assert_eq!(out.rel, b"1.fc39");
        assert_eq!(out.arch, b"x86_64");
    }

    #[test]
    fn parse_filename_with_epoch() {
        // epoch is embedded as "epoch:version" before the split
        let out = parse_filename(b"pkgname-2:1.0-3.el9.noarch.rpm").unwrap();
        assert_eq!(out.name, b"pkgname");
        assert_eq!(out.epoch, 2);
        assert_eq!(out.ver, b"1.0");
        assert_eq!(out.rel, b"3.el9");
        assert_eq!(out.arch, b"noarch");
    }

    #[test]
    fn parse_filename_dashes_in_name() {
        // rsplit_once('-') from the right means multi-dash names still work
        // as long as version/release themselves contain no dashes
        let out = parse_filename(b"python3-pip-23.0-1.fc39.noarch.rpm").unwrap();
        assert_eq!(out.name, b"python3-pip");
        assert_eq!(out.epoch, 0);
        assert_eq!(out.ver, b"23.0");
        assert_eq!(out.rel, b"1.fc39");
        assert_eq!(out.arch, b"noarch");
    }

    #[test]
    fn parse_filename_src_rpm() {
        let out = parse_filename(b"kernel-6.5.0-1.fc39.src.rpm").unwrap();
        assert_eq!(out.name, b"kernel");
        assert_eq!(out.epoch, 0);
        assert_eq!(out.ver, b"6.5.0");
        assert_eq!(out.rel, b"1.fc39");
        assert_eq!(out.arch, b"src");
    }

    #[test]
    fn parse_filename_missing_rpm_suffix_returns_none() {
        assert!(parse_filename(b"bash-5.2.15-1.fc39.x86_64").is_none());
    }

    #[test]
    fn parse_filename_no_arch_returns_none() {
        // needs at least one '.' to split arch off
        assert!(parse_filename(b"bash.rpm").is_none());
    }

    #[test]
    fn parse_filename_too_few_dashes_returns_none() {
        // needs at least 2 '-' to split name/version/release
        assert!(parse_filename(b"bash-5.2.15.fc39.x86_64.rpm").is_none());
    }

    #[test]
    fn parse_filename_epoch_non_numeric_treated_as_ver() {
        // pkg.rs:23 atoi failure falls back to epoch 0 and keeps colon in ver
        let out = parse_filename(b"pkg-abc:1.0-1.noarch.rpm").unwrap();
        assert_eq!(out.name, b"pkg");
        assert_eq!(out.epoch, 0);
        assert_eq!(out.ver, b"abc:1.0");
        assert_eq!(out.rel, b"1");
        assert_eq!(out.arch, b"noarch");
    }

    #[test]
    fn parse_filename_epoch_zero_explicit() {
        let out = parse_filename(b"pkg-0:1.0-1.noarch.rpm").unwrap();
        assert_eq!(out.epoch, 0);
        assert_eq!(out.ver, b"1.0");
        assert_eq!(out.rel, b"1");
    }

    #[test]
    fn parse_filename_same_name_arch_different_rel() {
        // essential for repo.rs:125 dedup: same name+arch, different rel
        let a = parse_filename(b"terra-release-44-4.noarch.rpm").unwrap();
        let b = parse_filename(b"terra-release-44-5.noarch.rpm").unwrap();
        assert_eq!(a.name, b.name);
        assert_eq!(a.arch, b.arch);
        assert_eq!(a.ver, b.ver);
        assert_ne!(a.rel, b.rel);
        assert_eq!(a.name, b"terra-release");
    }

    // ── Version::parse ──────────────────────────────────────────────
    #[test]
    fn version_parse_simple() {
        let v = Version::parse("1.0-1");
        assert_eq!(v.epoch, 0);
        assert_eq!(v.ver, "1.0");
        assert_eq!(v.rel, "1");
    }

    #[test]
    fn version_parse_with_epoch() {
        let v = Version::parse("2:1.0-3.el9");
        assert_eq!(v.epoch, 2);
        assert_eq!(v.ver, "1.0");
        assert_eq!(v.rel, "3.el9");
    }

    #[test]
    fn version_parse_epoch_zero_explicit() {
        let v = Version::parse("0:2.5-1");
        assert_eq!(v.epoch, 0);
        assert_eq!(v.ver, "2.5");
        assert_eq!(v.rel, "1");
    }

    #[test]
    fn version_parse_non_numeric_epoch_fallback() {
        // "abc:1.0-1" -> epoch parse fails, fallback to 0 and keep colon in ver
        let v = Version::parse("abc:1.0-1");
        assert_eq!(v.epoch, 0);
        assert_eq!(v.ver, "abc:1.0");
        assert_eq!(v.rel, "1");
    }

    #[test]
    fn version_parse_no_rel() {
        let v = Version::parse("1.0");
        assert_eq!(v.epoch, 0);
        assert_eq!(v.ver, "1.0");
        assert_eq!(v.rel, "");
    }

    #[test]
    fn version_parse_no_rel_with_epoch() {
        let v = Version::parse("1:2.0");
        assert_eq!(v.epoch, 1);
        assert_eq!(v.ver, "2.0");
        assert_eq!(v.rel, "");
    }

    #[test]
    fn version_parse_empty() {
        let v = Version::parse("");
        assert_eq!(v.epoch, 0);
        assert_eq!(v.ver, "");
        assert_eq!(v.rel, "");
    }

    #[test]
    fn version_parse_trailing_dash() {
        let v = Version::parse("1.0-");
        assert_eq!(v.ver, "1.0");
        assert_eq!(v.rel, "");
    }

    #[test]
    fn version_parse_multiple_dashes_only_first_splits() {
        // only first '-' after epoch splits ver/rel
        let v = Version::parse("1.0-1-2");
        assert_eq!(v.ver, "1.0");
        assert_eq!(v.rel, "1-2");
    }

    #[test]
    fn version_parse_colon_only() {
        let v = Version::parse(":");
        assert_eq!(v.epoch, 0);
        assert_eq!(v.ver, ":");
        assert_eq!(v.rel, "");
    }

    #[test]
    fn version_parse_large_epoch() {
        let v = Version::parse("4294967295:1.0-1");
        assert_eq!(v.epoch, 4294967295);
        assert_eq!(v.ver, "1.0");
    }

    // ── FileEntry::is_primary ───────────────────────────────────────
    #[test]
    fn is_primary_etc() {
        assert!(FileEntry::new("/etc/foo").is_primary());
        assert!(FileEntry::new("/etc/").is_primary());
        assert!(FileEntry::new("/etc/passwd").is_primary());
        assert!(!FileEntry::new("/etc").is_primary());
        assert!(!FileEntry::new("/etcfoo").is_primary());
    }

    #[test]
    fn is_primary_sendmail() {
        assert!(FileEntry::new("/usr/lib/sendmail").is_primary());
        assert!(!FileEntry::new("/usr/lib/sendmail/foo").is_primary());
        assert!(!FileEntry::new("/usr/lib/sendmail2").is_primary());
    }

    #[test]
    fn is_primary_bin_variants() {
        assert!(FileEntry::new("/usr/bin/bash").is_primary());
        assert!(FileEntry::new("/bin/ls").is_primary());
        assert!(FileEntry::new("/opt/bin/foo").is_primary());
        assert!(FileEntry::new("/usr/local/bin/app").is_primary());
        assert!(FileEntry::new("bin/foo").is_primary()); // contains bin/
        assert!(FileEntry::new("/a/bin/b").is_primary());
    }

    #[test]
    fn is_primary_negative() {
        assert!(!FileEntry::new("/usr/share/doc/foo").is_primary());
        assert!(!FileEntry::new("/var/lib/foo").is_primary());
        assert!(!FileEntry::new("/usr/libexec/foo").is_primary());
        assert!(!FileEntry::new("/tmp/foo").is_primary());
        assert!(!FileEntry::new("/usr/lib/sendmailfoo").is_primary());
        // "binary" contains "bin" but not "bin/" -> not primary
        assert!(!FileEntry::new("/usr/binary/foo").is_primary());
        assert!(!FileEntry::new("/usr/lib/foo").is_primary());
        assert!(!FileEntry::new("/opt/lib/foo").is_primary());
    }

    #[test]
    fn is_primary_sbin_is_primary() {
        // "sbin/" contains "bin/" substring -> considered primary (createrepo_c uses strstr("bin/"))
        assert!(FileEntry::new("/sbin/foo").is_primary());
        assert!(FileEntry::new("/usr/sbin/foo").is_primary());
    }

    #[test]
    fn is_primary_short_paths_no_panic() {
        // previously impl did `0..p.len()-4` which panics on short paths
        assert!(!FileEntry::new("/").is_primary());
        assert!(!FileEntry::new("/a").is_primary());
        assert!(!FileEntry::new("").is_primary());
        assert!(!FileEntry::new("ab").is_primary());
        assert!(!FileEntry::new("/ab").is_primary());
        assert!(!FileEntry::new("/bin").is_primary()); // no trailing slash, no "bin/"
    }

    #[test]
    fn is_primary_bin_at_edges() {
        assert!(FileEntry::new("bin/").is_primary());
        assert!(FileEntry::new("/bin/").is_primary());
        assert!(FileEntry::new("a/bin/b").is_primary());
    }

    // ── sha256_digest ───────────────────────────────────────────────
    #[test]
    fn sha256_empty() {
        let h = sha256_digest(Cursor::new(b"")).unwrap();
        assert_eq!(h, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }

    #[test]
    fn sha256_hello() {
        let h = sha256_digest(Cursor::new(b"hello")).unwrap();
        assert_eq!(h, "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
    }

    #[test]
    fn sha256_abc() {
        let h = sha256_digest(Cursor::new(b"abc")).unwrap();
        assert_eq!(h, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn sha256_large_multi_chunk() {
        // larger than internal 10240 buffer to exercise loop
        let data = vec![b'a'; 25_000];
        let h = sha256_digest(Cursor::new(&data)).unwrap();
        // precomputed: sha256 of 25k 'a's
        let expected = {
            use sha2::Digest as _;
            let mut hasher = sha2::Sha256::new();
            hasher.update(&data);
            hex::encode(hasher.finalize())
        };
        assert_eq!(h, expected);
    }

    #[test]
    fn sha256_reader_yields_same_as_direct() {
        let data = b"The quick brown fox jumps over the lazy dog";
        let h1 = sha256_digest(Cursor::new(data)).unwrap();
        let h2 = {
            use sha2::Digest as _;
            let mut hasher = sha2::Sha256::new();
            hasher.update(data);
            hex::encode(hasher.finalize())
        };
        assert_eq!(h1, h2);
        assert_eq!(h1, "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592");
    }

    // ── get_header_byte_range ───────────────────────────────────────
    #[test]
    fn header_range_on_real_rpm() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../random-rpm-examples/terra-release-44-4.noarch.rpm");
        let mut f = std::fs::File::open(&path).unwrap();
        let range = Package::get_header_byte_range(&mut f).unwrap();
        let meta = f.metadata().unwrap();
        assert!(range.start >= 112);
        assert!(range.end > range.start);
        assert!(range.end <= meta.len());
        // idempotent
        let mut f2 = std::fs::File::open(&path).unwrap();
        let range2 = Package::get_header_byte_range(&mut f2).unwrap();
        assert_eq!(range.start, range2.start);
        assert_eq!(range.end, range2.end);
    }

    #[test]
    fn header_range_truncated_file_errors() {
        let mut tmp = tempfile::tempfile().unwrap();
        // file shorter than 106 bytes -> read_exact fails
        tmp.write_all(&[0u8; 50]).unwrap();
        tmp.seek(std::io::SeekFrom::Start(0)).unwrap();
        let res = Package::get_header_byte_range(&mut tmp);
        assert!(res.is_err());
    }

    #[test]
    fn header_range_synthetic() {
        // Craft minimal RPM-like header structure:
        // at 104: sigindex=1 (0x01), sigdata=0x00 => sigsize=16, hdrstart=128
        // at hdrstart+8 (136): hdrindex=2 (0x02), hdrdata=0x00 => hdrsize=48, hdrend=176
        let mut tmp = tempfile::tempfile().unwrap();
        // ensure file large enough
        tmp.set_len(200).unwrap();
        tmp.seek(std::io::SeekFrom::Start(104)).unwrap();
        tmp.write_all(&[1, 0]).unwrap();
        tmp.seek(std::io::SeekFrom::Start(136)).unwrap();
        tmp.write_all(&[2, 0]).unwrap();
        let range = Package::get_header_byte_range(&mut tmp).unwrap();
        assert_eq!(range.start, 128);
        assert_eq!(range.end, 176);
    }

    #[test]
    fn header_range_synthetic_with_padding() {
        // sigindex=0, sigdata=5 => sigsize=5, pad=3, hdrstart=120
        // hdrindex=0, hdrdata=1 => hdrsize=17, hdrend=137
        let mut tmp = tempfile::tempfile().unwrap();
        tmp.set_len(200).unwrap();
        tmp.seek(std::io::SeekFrom::Start(104)).unwrap();
        tmp.write_all(&[0, 5]).unwrap();
        // hdrstart = 112+5+3=120, so hdr fields at 128
        tmp.seek(std::io::SeekFrom::Start(128)).unwrap();
        tmp.write_all(&[0, 1]).unwrap();
        let range = Package::get_header_byte_range(&mut tmp).unwrap();
        assert_eq!(range.start, 120);
        assert_eq!(range.end, 137);
    }

    #[test]
    fn header_range_synthetic_no_padding() {
        // sigsize 16 already 8-aligned => no padding
        let mut tmp = tempfile::tempfile().unwrap();
        tmp.set_len(300).unwrap();
        tmp.seek(std::io::SeekFrom::Start(104)).unwrap();
        tmp.write_all(&[1, 0]).unwrap(); // 16
        // hdrstart 128, hdr at 136
        tmp.seek(std::io::SeekFrom::Start(136)).unwrap();
        tmp.write_all(&[0, 8]).unwrap(); // hdrsize 24
        let range = Package::get_header_byte_range(&mut tmp).unwrap();
        assert_eq!(range.start, 128);
        assert_eq!(range.end, 152); // 128+24
    }
}
