use serde::Serialize;

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
    pub version: OtherVersion,
    #[serde(rename = "changelog", default)]
    pub changelogs: Vec<Changelog>,
}

#[derive(Clone, Debug, Serialize)]
pub struct OtherVersion {
    #[serde(rename = "@epoch")]
    pub epoch: String,
    #[serde(rename = "@ver")]
    pub ver: String,
    #[serde(rename = "@rel")]
    pub rel: String,
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
