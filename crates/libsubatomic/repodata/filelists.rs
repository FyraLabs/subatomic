use crate::prelude::*;

#[derive(Clone, Debug, Serialize)]
#[serde(rename = "filelists")]
pub struct FilelistsMetadata<'a> {
    #[serde(rename = "@xmlns")]
    pub xmlns: &'static str = "http://linux.duke.edu/metadata/filelists",
    #[serde(rename = "@packages")]
    pub packages: u64,
    #[serde(rename = "package")]
    pub packages_list: Vec<FilelistsPackage<'a>>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename = "package")]
pub struct FilelistsPackage<'a> {
    #[serde(rename = "@pkgid")]
    pub pkgid: &'a str,
    #[serde(rename = "@name")]
    pub name: &'a str,
    #[serde(rename = "@arch")]
    pub arch: &'a str,
    pub version: crate::pkg::Version,
    #[serde(rename = "file")]
    pub files: Vec<crate::pkg::FileEntry>,
}

impl<'a> FilelistsPackage<'a> {
    pub fn from_pkg(p: &'a crate::pkg::Package) -> Self {
        Self {
            pkgid: &p.checksum,
            name: &p.name,
            arch: &p.arch,
            version: p.version.clone(),
            files: p.format.files.clone(),
        }
    }
}
