//! Repodata generation and XML type definitions
//!
//! This module contains repodata XML type definitions and the helper functions required to generate
//! those XML files.

pub mod filelists;
pub mod other;
pub mod primary;
pub mod repomd;

use crate::prelude::*;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default)]
pub struct RepoCache {
    fragments: BTreeMap<String, RepoCacheFragment>,
}

#[derive(Clone, Debug, Default)]
pub struct RepoCacheFragment {
    pub primary: String,
    pub filelists: String,
    pub other: String,
}

// TODO: probably should write!() to something like a file instead of String?
impl RepoCache {
    pub fn generate_primary_xml(&self) -> String {
        let mut xml = String::from(
            r#"<?xml version="1.0" encoding="UTF-8"?><metadata xmlns="http://linux.duke.edu/metadata/common" xmlns:rpm="http://linux.duke.edu/metadata/rpm" packages="#,
        );
        xml.push_str(&self.fragments.len().to_string());
        xml.push_str(r#">"#);
        (self.fragments.values())
            .for_each(|RepoCacheFragment { primary, .. }| xml.push_str(&primary));
        xml.push_str("</metadata>");
        xml
    }

    pub fn generate_filelists_xml(&self) -> String {
        let mut xml = String::from(
            r#"<?xml version="1.0" encoding="UTF-8"?><filelists xmlns="http://linux.duke.edu/metadata/filelists" packages="#,
        );
        xml.push_str(&self.fragments.len().to_string());
        xml.push_str(r#">"#);
        (self.fragments.values())
            .for_each(|RepoCacheFragment { filelists, .. }| xml.push_str(&filelists));
        xml.push_str("</filelists>");
        xml
    }

    pub fn generate_other_xml(&self) -> String {
        let mut xml = String::from(
            r#"<?xml version="1.0" encoding="UTF-8"?><otherdata xmlns="http://linux.duke.edu/metadata/other" packages="#,
        );
        xml.push_str(&self.fragments.len().to_string());
        xml.push_str(r#">"#);
        (self.fragments.values()).for_each(|RepoCacheFragment { other, .. }| xml.push_str(&other));
        xml.push_str("</otherdata>");
        xml
    }
}
