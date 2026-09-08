use crate::prelude::*;

#[derive(Clone, Debug, Serialize)]
pub struct repomd { // FIXME: how to make roottag lowercase properly
    #[serde(rename = "@xmlns")]
    pub xmlns: &'static str = "http://linux.duke.edu/metadata/repo",
    #[serde(rename = "@xmlns:rpm")]
    pub xmlns_rpm: &'static str = "http://linux.duke.edu/metadata/rpm",
    pub revision: u64,
    #[serde(default)]
    pub data: Vec<Data>,
}

#[derive(Clone, Debug)]
pub enum DataType {
    Primary,
    Filelists,
    Other,
    // PrimaryZck,
    // FilelistsZck,
    // OtherZck,
    #[deprecated = "use Custom(\"group\", \"comps.xml\")"]
    Group,
    Appstream,
    /// (serialized [`Data::r#type`], uncompressed filename)
    Custom(String, String),
}

impl serde::Serialize for DataType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if let Self::Custom(t, s) = self {
            if s.starts_with(':') {
                serializer.serialize_str(t)
            } else {
                serializer.serialize_str(&format!("{t}:{s}"))
            }
        } else {
            serializer.serialize_str(self.as_type())
        }
    }
}

impl<'de> serde::Deserialize<'de> for DataType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(DataType::Primary)
    }
}

impl<'de> serde::de::Visitor<'de> for DataType {
    type Value = Self;

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(match v {
            "primary" => Self::Primary,
            "filelists" => Self::Filelists,
            "other" => Self::Other,
            "group" => Self::Group,
            "appstream" => Self::Appstream,
            any if let Some((typ, str)) = any.split_once(':') => {
                Self::Custom(typ.into(), str.into())
            }
            any => return Err(E::custom(format!("unknown datatype: {any}"))),
        })
    }

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("any string")
    }
}

impl DataType {
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Primary => "primary",
            Self::Filelists => "filelists",
            Self::Other => "other",
            Self::Group => "comps",
            Self::Appstream => "appstream",
            Self::Custom(_, s) => s,
        }
    }

    #[must_use]
    pub fn as_type(&self) -> &str {
        match self {
            Self::Primary => "primary",
            Self::Filelists => "filelists",
            Self::Other => "other",
            Self::Group => "group",
            Self::Appstream => "appstream",
            Self::Custom(k, _) => k,
        }
    }
}
impl std::fmt::Display for DataType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, Serialize, serde::Deserialize)]
pub struct Checksum {
    #[serde(rename = "@type")]
    pub r#type: CsumType = CsumType::Sha256,
    #[serde(rename = "$value")]
    pub sha: String, // NOTE: or [u8; 32] with hex-serde?
}

#[derive(Clone, Copy, Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CsumType {
    Sha256,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Data {
    #[serde(rename = "@type")]
    pub r#type: DataType,
    pub checksum: Checksum,
    pub open_checksum: Checksum,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // pub header_checksum: Option<Checksum>, // Only for ZCK types
    pub location: Location,
    pub timestamp: i64,
    pub size: u64,
    pub open_size: u64,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // pub header_size: Option<u64>, // Only for ZCK types
}

#[derive(Clone, Debug, Serialize, serde::Deserialize)]
pub struct Location {
    #[serde(rename = "@href")]
    pub href: String,
}

impl repomd {
    /// Generate and write the contents of `repomd.xml`.
    ///
    /// # Errors
    /// See [`quick_xml::se::to_writer`].
    ///
    /// # Panics
    /// Panick when we cannot obtain the current time epoch.
    #[allow(clippy::unwrap_in_result)]
    pub fn generate<W: std::io::Write>(
        writer: W,
        data: Vec<Data>,
    ) -> Result<quick_xml::se::WriteResult, quick_xml::SeError> {
        let repomd = Self {
            data,
            revision: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            ..
        };
        quick_xml::se::to_utf8_io_writer(writer, &repomd)
    }
}
