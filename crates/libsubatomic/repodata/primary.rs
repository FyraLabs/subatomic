use crate::{
    pkg::{Format, Size, Time, Version},
    prelude::*,
};

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
pub struct PackageChecksum {
    #[serde(rename = "@type")]
    pub checksum_type: &'static str = "YES",
    #[serde(rename = "@pkgid")]
    pub pkgid: &'static str = "YES",
    #[serde(rename = "$text")]
    pub value: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct PackageLocation {
    #[serde(rename = "@href")]
    pub href: String,
}
