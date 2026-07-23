//! Module that contains shared struct implementations used in [`crate::repodata`] and a minimal
//! [`Package`] struct.

use std::{
    io::{Read, Seek},
    os::unix::fs::MetadataExt,
    path::PathBuf,
    time::UNIX_EPOCH,
};

use sha2::Digest;

use crate::prelude::*;

// Minimum representation for an RPM package.
#[derive(Clone, Debug)]
pub struct Package {
    pub name: String,
    pub arch: String,
    pub version: Version,
    pub checksum: String,
    pub summary: String,
    pub description: String,
    pub packager: String,
    pub url: String,
    pub time: Time,
    pub size: Size,
    /// Other metadata
    ///
    /// WARN: we are using `format.files` to store all files, but in
    /// [`crate::repodata::primary::PrimaryMetadata`] they are stored only if
    /// [`FileEntry::is_primary()`].
    pub format: Format,
}
impl TryFrom<&std::path::Path> for Package {
    type Error = rpm::Error;

    fn try_from(value: &std::path::Path) -> Result<Self, Self::Error> {
        let rpm = rpm::PackageMetadata::open(value)?;
        let mut f = std::fs::File::open(value)?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)?;
        let csum = hex::encode(sha2::Sha256::digest(&buf));
        let meta = f.metadata()?;
        let btime = meta.created()?.duration_since(UNIX_EPOCH).expect("time overflow").as_secs();
        Ok(Self {
            name: rpm.get_name()?.into(),
            arch: rpm.get_arch()?.into(),
            version: Version {
                epoch: rpm.get_epoch().unwrap_or(0).into(),
                ver: rpm.get_version()?.into(),
                rel: rpm.get_release()?.into(),
            },
            checksum: csum.into(),
            summary: rpm.get_summary()?.into(),
            description: rpm.get_description()?.into(),
            packager: rpm.get_packager()?.into(),
            url: rpm.get_url()?.into(),
            time: Time { file: btime, build: rpm.get_build_time()?.into() },
            size: Size {
                package: meta.size(),
                installed: rpm.get_installed_size()?.into(),
                archive: rpm
                    .header
                    .get_entry_data_as_u64(rpm::IndexTag::RPMTAG_ARCHIVESIZE)
                    .or_else(|_e| {
                        rpm.header
                            .get_entry_data_as_u32(rpm::IndexTag::RPMTAG_ARCHIVESIZE)
                            .map(|v| v as u64)
                    })?,
            },
            format: Format {
                license: rpm.get_license()?.into(),
                vendor: rpm.get_vendor()?.into(),
                group: rpm.get_group()?.into(),
                buildhost: rpm.get_build_host()?.into(),
                sourcerpm: rpm.get_source_rpm()?.into(),
                header_range: Package::get_header_byte_range(f)?,
                requires: Dependencies::from(rpm.get_requires()?),
                provides: Dependencies::from(rpm.get_provides()?),
                conflicts: Dependencies::from(rpm.get_conflicts()?),
                obsoletes: Dependencies::from(rpm.get_obsoletes()?),
                recommends: Dependencies::from(rpm.get_recommends()?),
                suggests: Dependencies::from(rpm.get_suggests()?),
                supplements: Dependencies::from(rpm.get_supplements()?),
                enhances: Dependencies::from(rpm.get_enhances()?),
                files: rpm.get_file_entries()?.into_iter().map(Into::into).collect(),
            },
        })
    }
}
impl Package {
    // https://github.com/madonuko/createrepo_nim/blob/719b99a469101c61441623f9fecfd3c7d977fbcb/src/rpm.nim#L160
    // https://github.com/rpm-software-management/createrepo_c/blob/5cf41fe5d703901d78078ed18c67ab667e446c1a/src/misc.c#L248
    fn get_header_byte_range(mut f: std::fs::File) -> std::io::Result<HeaderRange> {
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
            return Err(std::io::Error::other(
                "sanity check fail on {path} (hdrend {hdrend} < hdrstart {hdrstart})",
            ));
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
    pub fn parse(value: &str) -> Self {
        let (epoch, value) = (value.split_once(':'))
            .and_then(|(e, v)| Some((e.parse().ok()?, v)))
            .unwrap_or((0, value));
        let (ver, rel) = value.split_once("-").unwrap_or((value, ""));
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
    #[serde(rename = "@archive")]
    pub archive: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Format {
    #[serde(rename = "rpm:license")]
    pub license: String,
    #[serde(rename = "rpm:vendor")]
    pub vendor: String,
    #[serde(rename = "rpm:group")]
    pub group: String,
    #[serde(rename = "rpm:buildhost")]
    pub buildhost: String,
    #[serde(rename = "rpm:sourcerpm")]
    pub sourcerpm: String,
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
}
impl From<Vec<rpm::Dependency>> for Dependencies {
    fn from(value: Vec<rpm::Dependency>) -> Self {
        Self { entries: value.into_iter().map(Into::into).collect() }
    }
}

fn is_zero(&n: &u64) -> bool {
    n == 0
}

#[derive(Clone, Debug, Default, Serialize)]
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
impl From<rpm::Dependency> for Entry {
    fn from(rpm::Dependency { name, flags, version }: rpm::Dependency) -> Self {
        let name = name.into();
        let flags = flags.comparator_str();
        if flags.is_empty() {
            return Entry { name, .. };
        };
        let Version { epoch, ver, rel } = Version::parse(&version);
        let (ver, rel) = (Some(ver), Some(rel));
        Entry { name, flags, epoch, ver, rel }
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct FileEntry {
    #[serde(rename = "@type", default, skip_serializing_if = "FileType::is_normal")]
    pub file_type: FileType = FileType::Normal,
    #[serde(rename = "$text")]
    pub path: PathBuf,
}
impl FileEntry {
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into(), .. }
    }
    // https://github.com/rpm-software-management/createrepo_c/blob/5cf41fe5d703901d78078ed18c67ab667e446c1a/src/misc.h#L111
    #[must_use]
    pub fn is_primary(&self) -> bool {
        const BIN: &'static [u8] = b"bin/";

        let p = self.path.as_os_str().as_encoded_bytes();

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
    pub date: i64,
    #[serde(rename = "$text")]
    pub text: String,
}
