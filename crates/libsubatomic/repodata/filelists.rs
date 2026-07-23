use crate::{
    pkg::{FileEntry, Version},
    prelude::*,
};

#[derive(Clone, Debug, Serialize)]
#[serde(rename = "filelists")]
pub struct FilelistsMetadata {
    #[serde(rename = "@xmlns")]
    pub xmlns: &'static str = "http://linux.duke.edu/metadata/filelists",
    #[serde(rename = "@packages")]
    pub packages: u64,
    #[serde(rename = "package")]
    pub packages_list: Vec<FilelistsPackage>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename = "package")]
pub struct FilelistsPackage {
    #[serde(rename = "@pkgid")]
    pub pkgid: String,
    #[serde(rename = "@name")]
    pub name: String,
    #[serde(rename = "@arch")]
    pub arch: String,
    pub version: Version,
    #[serde(rename = "file")]
    pub files: Vec<FileEntry>,
}
