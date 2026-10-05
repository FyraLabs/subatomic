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

#[cfg(test)]
mod tests {
    use super::*;
    // prelude aliases String to smartstring, so use std::string::String explicitly in helpers
    fn ser(dt: &DataType) -> ::std::string::String {
        serde_json::to_string(dt).unwrap()
    }

    fn de(s: &str) -> Result<DataType, serde_json::Error> {
        serde_json::from_str(s)
    }

    #[test]
    fn datatype_serialize_known() {
        assert_eq!(ser(&DataType::Primary), "\"primary\"");
        assert_eq!(ser(&DataType::Filelists), "\"filelists\"");
        assert_eq!(ser(&DataType::Other), "\"other\"");
        assert_eq!(ser(&DataType::Appstream), "\"appstream\"");
        #[allow(deprecated)]
        {
            assert_eq!(ser(&DataType::Group), "\"group\"");
        }
    }

    #[test]
    fn datatype_serialize_custom() {
        let dt = DataType::Custom("mytype".into(), "myfile.xml".into());
        assert_eq!(ser(&dt), "\"mytype:myfile.xml\"");
    }

    #[test]
    fn datatype_serialize_custom_leading_colon_special() {
        // if filename starts with ':', serialize emits only the type (used after extend_custom_datatypes)
        let dt = DataType::Custom("group".into(), ":comps.xml".into());
        assert_eq!(ser(&dt), "\"group\"");
    }

    #[test]
    fn datatype_deserialize_known() {
        assert!(matches!(de("\"primary\"").unwrap(), DataType::Primary));
        assert!(matches!(de("\"filelists\"").unwrap(), DataType::Filelists));
        assert!(matches!(de("\"other\"").unwrap(), DataType::Other));
        assert!(matches!(de("\"appstream\"").unwrap(), DataType::Appstream));
        #[allow(deprecated)]
        {
            assert!(matches!(de("\"group\"").unwrap(), DataType::Group));
        }
    }

    #[test]
    fn datatype_deserialize_custom() {
        let dt = de("\"mytype:myfile.xml\"").unwrap();
        match dt {
            DataType::Custom(t, s) => {
                assert_eq!(t, "mytype");
                assert_eq!(s, "myfile.xml");
            }
            _ => panic!("expected custom"),
        }
    }

    #[test]
    fn datatype_deserialize_custom_empty_value() {
        let dt = de("\"mytype:\"").unwrap();
        match dt {
            DataType::Custom(t, s) => {
                assert_eq!(t, "mytype");
                assert_eq!(s, "");
            }
            _ => panic!("expected custom"),
        }
    }

    #[test]
    fn datatype_deserialize_custom_preserves_extra_colon() {
        // split_once on first ':' only
        let dt = de("\"a:b:c\"").unwrap();
        match dt {
            DataType::Custom(t, s) => {
                assert_eq!(t, "a");
                assert_eq!(s, "b:c");
            }
            _ => panic!("expected custom"),
        }
    }

    #[test]
    fn datatype_deserialize_unknown_errors() {
        assert!(de("\"unknown\"").is_err());
        assert!(de("\"PRIMARY\"").is_err()); // case sensitive
        assert!(de("\"\"").is_err());
    }

    #[test]
    fn datatype_roundtrip_known() {
        for dt in [DataType::Primary, DataType::Filelists, DataType::Other, DataType::Appstream] {
            let s = ser(&dt);
            let back: DataType = de(&s).unwrap();
            assert_eq!(ser(&back), s);
        }
        #[allow(deprecated)]
        {
            let dt = DataType::Group;
            let s = ser(&dt);
            let back: DataType = de(&s).unwrap();
            assert_eq!(ser(&back), s);
        }
    }

    #[test]
    fn datatype_roundtrip_custom() {
        let cases = [
            DataType::Custom("foo".into(), "bar.xml".into()),
            DataType::Custom("group".into(), "comps.xml".into()),
            DataType::Custom("a".into(), "b:c".into()),
        ];
        for dt in cases {
            let s = ser(&dt);
            let back: DataType = de(&s).unwrap();
            match (&dt, &back) {
                (DataType::Custom(t1, s1), DataType::Custom(t2, s2)) => {
                    assert_eq!(t1, t2);
                    assert_eq!(s1, s2);
                }
                _ => panic!("mismatch"),
            }
        }
    }

    #[test]
    fn datatype_as_str_vs_as_type() {
        assert_eq!(DataType::Primary.as_str(), "primary");
        assert_eq!(DataType::Primary.as_type(), "primary");
        assert_eq!(DataType::Filelists.as_str(), "filelists");
        #[allow(deprecated)]
        {
            assert_eq!(DataType::Group.as_str(), "comps");
            assert_eq!(DataType::Group.as_type(), "group");
        }
        let c = DataType::Custom("mytype".into(), "myfile.xml".into());
        assert_eq!(c.as_str(), "myfile.xml");
        assert_eq!(c.as_type(), "mytype");
        assert_eq!(format!("{c}"), "myfile.xml");
    }

    #[test]
    fn datatype_quick_xml_roundtrip_via_data() {
        // ensure Data with DataType survives quick_xml ser/de (repomd context)
        let data = Data {
            r#type: DataType::Primary,
            checksum: Checksum { sha: "abc".into(), r#type: CsumType::Sha256 },
            open_checksum: Checksum { sha: "def".into(), r#type: CsumType::Sha256 },
            location: Location { href: "repodata/abc-primary.xml.zst".into() },
            timestamp: 1234567890,
            size: 100,
            open_size: 200,
        };
        let xml = quick_xml::se::to_string(&data).unwrap();
        let back: Data = quick_xml::de::from_str(&xml).unwrap();
        assert!(matches!(back.r#type, DataType::Primary));
    }

    #[test]
    fn datatype_custom_quick_xml_roundtrip() {
        let data = Data {
            r#type: DataType::Custom("mytype".into(), "myfile.xml".into()),
            checksum: Checksum { sha: "abc".into(), r#type: CsumType::Sha256 },
            open_checksum: Checksum { sha: "def".into(), r#type: CsumType::Sha256 },
            location: Location { href: "repodata/abc-myfile.xml.zst".into() },
            timestamp: 0,
            size: 1,
            open_size: 1,
        };
        let xml = quick_xml::se::to_string(&data).unwrap();
        // custom type is serialized as "mytype:myfile.xml" in @type attribute
        assert!(xml.contains("mytype:myfile.xml"), "xml={xml}");
        let back: Data = quick_xml::de::from_str(&xml).unwrap();
        match back.r#type {
            DataType::Custom(t, s) => {
                assert_eq!(t, "mytype");
                assert_eq!(s, "myfile.xml");
            }
            _ => panic!("expected custom"),
        }
    }
}
