//! Module that contains shared struct implementations used in [`crate::repodata`].

use crate::prelude::*;
pub use crate::repodata::MetanInput;

#[derive(Clone, Debug, Serialize)]
pub struct Version {
    #[serde(rename = "@epoch")]
    pub epoch: u64,
    #[serde(rename = "@ver")]
    pub ver: String,
    #[serde(rename = "@rel")]
    pub rel: String,
}

impl Version {
    #[must_use]
    pub fn parse(value: &str) -> Self {
        let (epoch, value) = (value.split_once(':'))
            .and_then(|(e, v)| Some((e.parse().ok()?, v)))
            .unwrap_or((0, value));
        let (ver, rel) = value.split_once('-').unwrap_or((value, ""));
        let (ver, rel) = (ver.into(), rel.into());
        Self { epoch, ver, rel }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Time {
    #[serde(rename = "@file")]
    pub file: u64,
    #[serde(rename = "@build")]
    pub build: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Size {
    #[serde(rename = "@package")]
    pub package: u64,
    #[serde(rename = "@installed")]
    pub installed: u64,
    // archive size seems to be optional on some packages when testing with terra44 dataset,
    // so we can avoid serializing it if it's not present
    #[serde(rename = "@archive", skip_serializing_if = "Option::is_none")]
    pub archive: Option<u64>,
}

/*
#[derive(Clone, Debug, Serialize)]
pub struct HeaderRange {
    #[serde(rename = "@start")]
    pub start: u64,
    #[serde(rename = "@end")]
    pub end: u64,
}*/

#[derive(Clone, Debug, Serialize)]
pub struct Dependencies {
    #[serde(rename = "rpm:entry", default)]
    pub entries: Vec<Entry>,
}

impl Dependencies {
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn from_requires(value: Vec<rpm::Dependency>) -> Self {
        Self {
            entries: value
                .into_iter()
                .filter(|dependency| !dependency.flags.contains(rpm::DependencyFlags::RPMLIB))
                .map(Into::into)
                .collect(),
        }
    }
}

impl From<Vec<rpm::Dependency>> for Dependencies {
    fn from(value: Vec<rpm::Dependency>) -> Self {
        Self { entries: value.into_iter().map(Into::into).collect() }
    }
}

#[cfg(test)]
mod dependency_tests {
    use super::{Dependencies, Entry};

    #[test]
    fn excludes_rpmlib_requirements() {
        let dependencies = Dependencies::from_requires(vec![
            rpm::Dependency::rpmlib("CompressedFileNames", "3.0.4-1"),
            rpm::Dependency::greater_eq("glibc", "2.40"),
        ]);

        assert_eq!(dependencies.entries.len(), 1);
        assert_eq!(dependencies.entries[0].name, "glibc");
    }

    /// repodata spells comparators as words; libsolv does not understand the
    /// human-readable sense form (`=`, `<=`, ...) returned by
    /// `DependencyFlags::comparator_str`, which makes providers look missing.
    #[test]
    fn serializes_repodata_comparator_symbols() {
        let cases = [
            (rpm::Dependency::eq("dep", "1-1"), "EQ"),
            (rpm::Dependency::less("dep", "1-1"), "LT"),
            (rpm::Dependency::less_eq("dep", "1-1"), "LE"),
            (rpm::Dependency::greater("dep", "1-1"), "GT"),
            (rpm::Dependency::greater_eq("dep", "1-1"), "GE"),
            (rpm::Dependency::any("dep"), ""),
        ];

        for (dependency, expected) in cases {
            let entry = Entry::from(dependency);
            assert_eq!(entry.flags, expected, "for {}", entry.name);
        }
    }

    /// A versioned dependency must carry `epoch`; createrepo_c always emits it
    /// and consumers rely on its presence.
    #[test]
    fn keeps_the_full_version_including_epoch() {
        let entry = Entry::from(rpm::Dependency::eq("kernel", "5.15.147-17"));
        assert_eq!(entry.flags, "EQ");
        assert_eq!(entry.ver.as_deref(), Some("5.15.147"));
        assert_eq!(entry.rel.as_deref(), Some("17"));
    }
}

#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_zero(&n: &u64) -> bool {
    n == 0
}

#[derive(Clone, Debug, Serialize)]
pub struct Entry {
    #[serde(rename = "@name")]
    pub name: String,
    #[serde(rename = "@flags", default, skip_serializing_if = "str::is_empty")]
    pub flags: &'static str = "",
    #[serde(rename = "@epoch", default, skip_serializing_if = "is_zero")]
    pub epoch: u64 = 0,
    #[serde(rename = "@ver", default, skip_serializing_if = "Option::is_none")]
    pub ver: Option<String> = None,
    #[serde(rename = "@rel", default, skip_serializing_if = "Option::is_none")]
    pub rel: Option<String> = None,
}

/// Map RPM dependency flags to the symbolic comparator used by repodata
const fn repodata_flags(flags: rpm::DependencyFlags) -> &'static str {
    use rpm::DependencyFlags as F;
    const SENSE: F = F::LESS.union(F::GREATER).union(F::EQUAL);

    match flags.intersection(SENSE) {
        f if f.contains(F::LESS) && f.contains(F::EQUAL) => "LE",
        f if f.contains(F::GREATER) && f.contains(F::EQUAL) => "GE",
        f if f.contains(F::LESS) => "LT",
        f if f.contains(F::GREATER) => "GT",
        F::EQUAL => "EQ",
        _ => "",
    }
}

impl From<rpm::Dependency> for Entry {
    fn from(rpm::Dependency { name, flags, version }: rpm::Dependency) -> Self {
        let name = name.into();
        let flags = repodata_flags(flags);
        if flags.is_empty() {
            return Self { name, .. };
        }
        let Version { epoch, ver, rel } = Version::parse(&version);
        let (ver, rel) = (Some(ver), Some(rel));
        Self { name, flags, epoch, ver, rel }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct FileEntry {
    #[serde(rename = "@type", default, skip_serializing_if = "FileType::is_normal")]
    pub file_type: FileType = FileType::Normal,
    #[serde(rename = "$text")]
    pub path: PathBuf,
}
impl<'a> From<rpm::FileEntry<'a>> for FileEntry {
    fn from(value: rpm::FileEntry<'a>) -> Self {
        Self {
            file_type: if value.flags().contains(rpm::FileFlags::GHOST) {
                FileType::Ghost
            } else if value.file_type() == rpm::FileType::Dir {
                FileType::Dir
            } else {
                FileType::Normal
            },
            path: value.path(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FileType {
    #[default]
    Normal,
    Dir,
    Ghost,
}

impl FileType {
    #[allow(clippy::trivially_copy_pass_by_ref)]
    #[must_use]
    pub const fn is_normal(&self) -> bool {
        matches!(self, Self::Normal)
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Changelog {
    #[serde(rename = "@author")]
    pub author: String,
    #[serde(rename = "@date")]
    pub date: u64,
    #[serde(rename = "$text")]
    pub text: String,
}

impl From<rpm::ChangelogEntry> for Changelog {
    fn from(rpm::ChangelogEntry { name, timestamp, description }: rpm::ChangelogEntry) -> Self {
        Self { author: name.into(), date: timestamp, text: description.into() }
    }
}
