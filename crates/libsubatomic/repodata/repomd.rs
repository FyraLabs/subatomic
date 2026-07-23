use crate::prelude::*;

#[derive(Clone, Debug, Serialize)]
pub struct Repomd {
    #[serde(rename = "@xmlns")]
    pub xmlns: &'static str = "http://linux.duke.edu/metadata/repo",
    #[serde(rename = "@xmlns:rpm")]
    pub xmlns_rpm: &'static str = "http://linux.duke.edu/metadata/rpm",
    pub revision: i64,
    #[serde(default)]
    pub data: Vec<Data>,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DataType {
    Primary,
    Filelists,
    Other,
    PrimaryZck,
    FilelistsZck,
    OtherZck,
}

#[derive(Clone, Debug, Serialize)]
pub struct Checksum {
    #[serde(rename = "@type")]
    pub r#type: &'static str = "sha256",
    pub sha: String, // NOTE: or [u8; 32] with hex-serde?
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Data {
    #[serde(rename = "@type")]
    pub r#type: DataType,
    pub checksum: Checksum,
    pub open_checksum: Checksum,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header_checksum: Option<Checksum>, // Only for ZCK types
    pub location: Location,
    pub timestamp: i64,
    pub size: u64,
    pub open_size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header_size: Option<u64>, // Only for ZCK types
}

#[derive(Clone, Debug, Serialize)]
pub struct Location {
    #[serde(rename = "@href")]
    pub href: String,
}
