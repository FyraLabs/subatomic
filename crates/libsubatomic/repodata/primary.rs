use crate::prelude::*;

#[derive(Clone, Debug, Serialize)]
#[serde(rename = "metadata")]
pub struct PrimaryMetadata {
    #[serde(rename = "@xmlns")]
    pub xmlns: &'static str = "http://linux.duke.edu/metadata/common",
    #[serde(rename = "@xmlns:rpm")]
    pub xmlns_rpm: &'static str = "http://linux.duke.edu/metadata/rpm",
    #[serde(rename = "@packages")]
    pub packages: u64,
    #[serde(rename = "package")]
    pub packages_list: Vec<Package>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename = "package")]
pub struct Package {
    #[serde(rename = "@type")]
    pub package_type: &'static str = "rpm",
    pub name: String,
    pub arch: String,
    pub version: Version,
    pub checksum: PackageChecksum,
    pub summary: String,
    pub description: String,
    pub packager: String,
    pub url: String,
    pub time: Time,
    pub size: Size,
    pub location: PackageLocation,
    pub format: Format,
}

#[derive(Clone, Debug, Serialize)]
pub struct Version {
    #[serde(rename = "@epoch")]
    pub epoch: String,
    #[serde(rename = "@ver")]
    pub ver: String,
    #[serde(rename = "@rel")]
    pub rel: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct PackageChecksum {
    #[serde(rename = "@type")]
    pub checksum_type: &'static str = "YES",
    #[serde(rename = "@pkgid")]
    pub pkgid: &'static str = "YES",
    #[serde(rename = "$text")]
    pub value: String,
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
pub struct PackageLocation {
    #[serde(rename = "@href")]
    pub href: String,
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
    pub files: Vec<String>,
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

#[derive(Clone, Debug, Serialize)]
pub struct Entry {
    #[serde(rename = "@name")]
    pub name: String,
    #[serde(rename = "@flags", default, skip_serializing_if = "Option::is_none")]
    pub flags: Option<String>,
    #[serde(rename = "@epoch", default, skip_serializing_if = "Option::is_none")]
    pub epoch: Option<String>,
    #[serde(rename = "@ver", default, skip_serializing_if = "Option::is_none")]
    pub ver: Option<String>,
    #[serde(rename = "@rel", default, skip_serializing_if = "Option::is_none")]
    pub rel: Option<String>,
}
