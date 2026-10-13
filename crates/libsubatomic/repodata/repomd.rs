use crate::prelude::*;
use kuchiyose::ftmm::Ftmm;
use kuchiyose::link::LinkBuf;

#[derive(Clone, Debug, Serialize)]
#[expect(non_camel_case_types)]
pub struct repomd { // FIXME: how to make roottag lowercase properly
    #[serde(rename = "@xmlns")]
    pub xmlns: &'static str = "http://linux.duke.edu/metadata/repo",
    #[serde(rename = "@xmlns:rpm")]
    pub xmlns_rpm: &'static str = "http://linux.duke.edu/metadata/rpm",
    pub revision: u64,
    #[serde(default)]
    pub data: Vec<Data>,
}

#[non_exhaustive]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Checksum {
    #[serde(rename = "@type")]
    pub r#type: Ftmm,
    #[serde(rename = "$value")]
    pub sha: String,
}

#[non_exhaustive]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Data {
    #[serde(rename = "@type")]
    pub r#type: String,
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

#[non_exhaustive]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Location {
    #[serde(rename = "@href")]
    pub href: LinkBuf,
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
