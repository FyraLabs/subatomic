use crate::{
    pkg::{Changelog, Version},
    prelude::*,
};

#[derive(Clone, Debug, Serialize)]
#[serde(rename = "otherdata")]
pub struct OtherMetadata<'a> {
    #[serde(rename = "@xmlns")]
    pub xmlns: &'static str = "http://linux.duke.edu/metadata/other",
    #[serde(rename = "@packages")]
    pub packages: u64,
    #[serde(rename = "package")]
    pub packages_list: Vec<OtherPackage<'a>>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename = "package")]
pub struct OtherPackage<'a> {
    #[serde(rename = "@pkgid")]
    pub pkgid: &'a str,
    #[serde(rename = "@name")]
    pub name: &'a str,
    #[serde(rename = "@arch")]
    pub arch: &'a str,
    pub version: Version,
    #[serde(rename = "changelog", default)]
    pub changelogs: &'a [Changelog],
}

impl<'a> OtherPackage<'a> {
    pub fn from_pkg(p: &'a crate::pkg::Package) -> Self {
        Self {
            pkgid: &p.checksum,
            name: &p.name,
            arch: &p.arch,
            version: p.version.clone(),
            changelogs: &p.changelog,
        }
    }
}
