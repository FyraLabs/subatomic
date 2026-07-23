use serde::Serialize;

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
    pub version: FilelistsVersion,
    #[serde(rename = "file")]
    pub files: Vec<FileEntry>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FilelistsVersion {
    #[serde(rename = "@epoch")]
    pub epoch: String,
    #[serde(rename = "@ver")]
    pub ver: String,
    #[serde(rename = "@rel")]
    pub rel: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct FileEntry {
    #[serde(rename = "@type", default, skip_serializing_if = "Option::is_none")]
    pub file_type: Option<String>, // "dir" for directories, None for regular files
    #[serde(rename = "$text")]
    pub path: String,
}
