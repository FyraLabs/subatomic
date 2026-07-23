use crate::{
    pkg::{Changelog, Version},
    prelude::*,
};

#[derive(Clone, Debug, Serialize)]
#[serde(rename = "otherdata")]
pub struct OtherMetadata {
    #[serde(rename = "@xmlns")]
    pub xmlns: &'static str = "http://linux.duke.edu/metadata/other",
    #[serde(rename = "@packages")]
    pub packages: u64,
    #[serde(rename = "package")]
    pub packages_list: Vec<OtherPackage>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename = "package")]
pub struct OtherPackage {
    #[serde(rename = "@pkgid")]
    pub pkgid: String,
    #[serde(rename = "@name")]
    pub name: String,
    #[serde(rename = "@arch")]
    pub arch: String,
    pub version: Version,
    #[serde(rename = "changelog", default)]
    pub changelogs: Vec<Changelog>,
}
