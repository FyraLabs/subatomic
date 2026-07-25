use crate::{
    pkg::{Format, Size, Time, Version},
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
    pub packager: &'a str,
    pub url: &'a str,
    pub time: &'a Time,
    pub size: &'a Size,
    pub location: PackageLocation<'a>,
    pub format: Format,
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
    pub href: &'a OsStr,
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
        path: &'a OsStr,
    ) -> Self {
        let mut format = format.clone();
        format.files =
            format.files.into_iter().filter(super::super::pkg::FileEntry::is_primary).collect();
        Self {
            name,
            arch,
            version,
            checksum: PackageChecksum { value: checksum, .. },
            summary,
            description,
            packager,
            url,
            time,
            size,
            location: PackageLocation { href: path },
            format,
            ..
        }
    }
}
