use crate::{
    pkg::{Dependencies, FileEntry, HeaderRange, Size, Time, Version},
    prelude::*,
};

#[derive(Clone, Debug, Serialize)]
#[serde(rename = "metadata")]
pub struct PrimaryMetadata<'a> {
    #[serde(rename = "@xmlns")]
    pub xmlns: &'static str = "http://linux.duke.edu/metadata/common",
    #[serde(rename = "@xmlns:rpm")]
    pub xmlns_rpm: &'static str = "http://linux.duke.edu/metadata/rpm",
    #[serde(rename = "@packages")]
    pub packages: u64,
    #[serde(rename = "package")]
    pub packages_list: Vec<Package<'a>>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename = "package")]
pub struct Package<'a> {
    #[serde(rename = "@type")]
    pub package_type: &'static str = "rpm",
    pub name: &'a str,
    pub arch: &'a str,
    pub version: &'a Version,
    pub checksum: PackageChecksum<'a>,
    pub summary: &'a str,
    pub description: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub packager: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<&'a str>,
    pub time: &'a Time,
    pub size: &'a Size,
    pub location: PackageLocation<'a>,
    pub format: PrimaryFormat<'a>,
}

/// Zero-copy view of [`crate::pkg::Format`] for primary.xml serialization.
///
/// `primary.xml` only includes a subset of files (see [`FileEntry::is_primary`]). The original
/// code cloned the entire `Format` (including all dependency vectors and the full file list) just
/// to filter the files. This struct borrows everything from the source `Format` to avoid that
/// clone, while still presenting the same serialized shape to `quick_xml`.
#[derive(Clone, Debug, Serialize)]
pub struct PrimaryFormat<'a> {
    #[serde(rename = "rpm:license")]
    pub license: &'a str,
    #[serde(rename = "rpm:vendor", skip_serializing_if = "Option::is_none")]
    pub vendor: Option<&'a str>,
    #[serde(rename = "rpm:group", skip_serializing_if = "Option::is_none")]
    pub group: Option<&'a str>,
    #[serde(rename = "rpm:buildhost", skip_serializing_if = "Option::is_none")]
    pub buildhost: Option<&'a str>,
    #[serde(rename = "rpm:sourcerpm", skip_serializing_if = "Option::is_none")]
    pub sourcerpm: Option<&'a str>,
    #[serde(rename = "rpm:header-range")]
    pub header_range: HeaderRange,
    #[serde(rename = "rpm:requires", default, skip_serializing_if = "deps_is_empty")]
    pub requires: &'a Dependencies,
    #[serde(rename = "rpm:provides", default, skip_serializing_if = "deps_is_empty")]
    pub provides: &'a Dependencies,
    #[serde(rename = "rpm:conflicts", default, skip_serializing_if = "deps_is_empty")]
    pub conflicts: &'a Dependencies,
    #[serde(rename = "rpm:obsoletes", default, skip_serializing_if = "deps_is_empty")]
    pub obsoletes: &'a Dependencies,
    #[serde(rename = "rpm:recommends", default, skip_serializing_if = "deps_is_empty")]
    pub recommends: &'a Dependencies,
    #[serde(rename = "rpm:suggests", default, skip_serializing_if = "deps_is_empty")]
    pub suggests: &'a Dependencies,
    #[serde(rename = "rpm:supplements", default, skip_serializing_if = "deps_is_empty")]
    pub supplements: &'a Dependencies,
    #[serde(rename = "rpm:enhances", default, skip_serializing_if = "deps_is_empty")]
    pub enhances: &'a Dependencies,
    #[serde(rename = "file", default)]
    pub files: Vec<&'a FileEntry>,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
const fn deps_is_empty(d: &&Dependencies) -> bool {
    d.is_empty()
}

#[derive(Clone, Debug, Serialize)]
pub struct PackageChecksum<'a> {
    #[serde(rename = "@type")]
    pub checksum_type: &'static str = "YES",
    #[serde(rename = "@pkgid")]
    pub pkgid: &'static str = "YES",
    #[serde(rename = "$text")]
    pub value: &'a str,
}

#[derive(Clone, Debug, Serialize)]
pub struct PackageLocation<'a> {
    #[serde(rename = "@href")]
    pub href: &'a std::path::Path,
}

impl<'a> Package<'a> {
    #[must_use]
    pub fn from_pkg(
        crate::pkg::Package {
            name,
            arch,
            version,
            checksum,
            summary,
            description,
            packager,
            url,
            time,
            size,
            format,
            ..
        }: &'a crate::pkg::Package,
        path: &'a std::path::Path,
    ) -> Self {
        let files = format.files.iter().filter(|f| f.is_primary()).collect();
        Self {
            name,
            arch,
            version,
            checksum: PackageChecksum { value: checksum, .. },
            summary,
            description,
            packager: packager.as_deref(),
            url: url.as_deref(),
            time,
            size,
            location: PackageLocation { href: path },
            format: PrimaryFormat {
                license: &format.license,
                vendor: format.vendor.as_deref(),
                group: format.group.as_deref(),
                buildhost: format.buildhost.as_deref(),
                sourcerpm: format.sourcerpm.as_deref(),
                header_range: format.header_range.clone(),
                requires: &format.requires,
                provides: &format.provides,
                conflicts: &format.conflicts,
                obsoletes: &format.obsoletes,
                recommends: &format.recommends,
                suggests: &format.suggests,
                supplements: &format.supplements,
                enhances: &format.enhances,
                files,
            },
            ..
        }
    }
}
