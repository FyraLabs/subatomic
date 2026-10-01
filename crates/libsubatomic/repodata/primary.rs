use std::sync::Arc;

use tokio::io::AsyncWriteExt;

use crate::{
    pkg::{Dependencies, FileEntry, Size, Time, Version},
    prelude::*,
};

#[derive(Clone, Debug, Serialize)]
#[serde(rename = "metadata")]
pub struct PrimaryMetadata<'a> {
    #[serde(rename = "@xmlns")]
    pub xmlns: &'static str = "http://linux.duke.edu/metadata/common",
    #[serde(rename = "@xmlns:rpm")]
    pub xmlns_rpm: &'static str = "http://linux.duke.edu/metadata/rpm",
    #[serde(rename = "@packages")]
    pub packages: u64,
    #[serde(rename = "package")]
    pub packages_list: Vec<Package<'a>>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename = "package")]
pub struct Package<'a> {
    #[serde(rename = "@type")]
    pub package_type: &'static str = "rpm",
    pub name: &'a str,
    pub arch: &'a str,
    pub version: &'a Version,
    pub checksum: PackageChecksum<'a>,
    pub summary: &'a str,
    pub description: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub packager: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<&'a str>,
    pub time: &'a Time,
    pub size: &'a Size,
    pub location: PackageLocation<'a>,
    pub format: PrimaryFormat<'a>,
}

/// Zero-copy view of [`crate::pkg::Format`] for primary.xml serialization.
///
/// `primary.xml` only includes a subset of files (see [`FileEntry::is_primary`]). The original
/// code cloned the entire `Format` (including all dependency vectors and the full file list) just
/// to filter the files. This struct borrows everything from the source `Format` to avoid that
/// clone, while still presenting the same serialized shape to `quick_xml`.
#[derive(Clone, Debug, Serialize)]
pub struct PrimaryFormat<'a> {
    #[serde(rename = "rpm:license")]
    pub license: &'a str,
    #[serde(rename = "rpm:vendor", skip_serializing_if = "Option::is_none")]
    pub vendor: Option<&'a str>,
    #[serde(rename = "rpm:group", skip_serializing_if = "Option::is_none")]
    pub group: Option<&'a str>,
    #[serde(rename = "rpm:buildhost", skip_serializing_if = "Option::is_none")]
    pub buildhost: Option<&'a str>,
    #[serde(rename = "rpm:sourcerpm", skip_serializing_if = "Option::is_none")]
    pub sourcerpm: Option<&'a str>,
    // TODO: impl, and tbh do we really need this
    // #[serde(rename = "rpm:header-range")]
    // pub header_range: HeaderRange,
    #[serde(rename = "rpm:requires", default, skip_serializing_if = "deps_is_empty")]
    pub requires: &'a Dependencies,
    #[serde(rename = "rpm:provides", default, skip_serializing_if = "deps_is_empty")]
    pub provides: &'a Dependencies,
    #[serde(rename = "rpm:conflicts", default, skip_serializing_if = "deps_is_empty")]
    pub conflicts: &'a Dependencies,
    #[serde(rename = "rpm:obsoletes", default, skip_serializing_if = "deps_is_empty")]
    pub obsoletes: &'a Dependencies,
    #[serde(rename = "rpm:recommends", default, skip_serializing_if = "deps_is_empty")]
    pub recommends: &'a Dependencies,
    #[serde(rename = "rpm:suggests", default, skip_serializing_if = "deps_is_empty")]
    pub suggests: &'a Dependencies,
    #[serde(rename = "rpm:supplements", default, skip_serializing_if = "deps_is_empty")]
    pub supplements: &'a Dependencies,
    #[serde(rename = "rpm:enhances", default, skip_serializing_if = "deps_is_empty")]
    pub enhances: &'a Dependencies,
    #[serde(rename = "file", default)]
    pub files: Vec<FileEntry>,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
const fn deps_is_empty(d: &&Dependencies) -> bool {
    d.is_empty()
}

#[derive(Clone, Debug, Serialize)]
pub struct PackageChecksum<'a> {
    #[serde(rename = "@type")]
    pub checksum_type: &'static str = "sha256", // FIXME: unhardcode
    #[serde(rename = "@pkgid")]
    pub pkgid: &'static str = "YES",
    #[serde(rename = "$text")]
    pub value: &'a str,
}

#[derive(Clone, Debug, Serialize)]
pub struct PackageLocation<'a> {
    #[serde(rename = "@href")]
    pub href: &'a [u8],
}

impl<'a> Package<'a> {
    #[must_use]
    pub fn from_pkg(
        crate::pkg::Package {
            name,
            arch,
            version,
            checksum,
            summary,
            description,
            packager,
            url,
            time,
            size,
            format,
            ..
        }: &'a crate::pkg::Package,
        path: &'a [u8],
    ) -> Self {
        let files = format.files.iter().cloned().filter(|f| f.is_primary()).collect();
        Self {
            name,
            arch,
            version,
            checksum: PackageChecksum { value: checksum, .. },
            summary,
            description,
            packager: packager.as_deref(),
            url: url.as_deref(),
            time,
            size,
            location: PackageLocation { href: path },
            format: PrimaryFormat {
                license: &format.license,
                vendor: format.vendor.as_deref(),
                group: format.group.as_deref(),
                buildhost: format.buildhost.as_deref(),
                sourcerpm: format.sourcerpm.as_deref(),
                // header_range: format.header_range.clone(),
                requires: &format.requires,
                provides: &format.provides,
                conflicts: &format.conflicts,
                obsoletes: &format.obsoletes,
                recommends: &format.recommends,
                suggests: &format.suggests,
                supplements: &format.supplements,
                enhances: &format.enhances,
                files,
            },
            ..
        }
    }
}

pub(crate) struct PrimaryMetan {
    db: std::sync::OnceLock<Arc<super::FragDb>>,
}

#[async_trait::async_trait]
impl super::Metan for PrimaryMetan {
    fn mdtype(&self) -> &str {
        "primary"
    }
    fn filename(&self) -> &str {
        "primary"
    }
    fn db_count(&self) -> usize {
        1
    }
    fn db_init<'s, 't, 'db>(
        &'s self,
        env: Arc<heed::Env<heed::WithoutTls>>,
        txn: &'t mut heed::RwTxn<'db>,
    ) -> heed::Result<()> {
        self.db.set(Arc::new(env.create_database(txn, Some("primary"))?)).expect("double db_init");
        Ok(())
    }
    fn save<'t, 'db>(
        &self,
        txn: &'t mut heed::RwTxn<'db>,
        pkg: &mut crate::pkg::MetanPkg<'_>,
    ) -> Result<(), super::MetanError> {
        let rpm = &pkg.rpm.metadata;
        self.db.get().expect("db uninit").put(
            txn,
            pkg.path,
            quick_xml::se::to_string(&Package {
                location: PackageLocation { href: pkg.path },
                name: rpm.get_name()?,
                arch: rpm.get_arch()?,
                version: &Version {
                    epoch: rpm.get_epoch().unwrap_or(0).into(),
                    ver: rpm.get_version()?.into(),
                    rel: rpm.get_release()?.into(),
                },
                checksum: PackageChecksum { checksum_type: pkg.csum_type, value: &pkg.csum, .. },
                summary: rpm.get_summary().unwrap_or_default().into(),
                description: rpm.get_description().unwrap_or_default().into(),
                packager: rpm.get_packager().ok().map(Into::into),
                url: rpm.get_url().ok().map(Into::into),
                time: &Time { file: epoch!(pkg.fmeta.created()?), build: rpm.get_build_time()? },
                size: &Size {
                    package: pkg.fmeta.size(),
                    installed: rpm.get_installed_size()?,
                    archive: rpm
                        .header
                        .get_entry_data_as_u64(rpm::IndexTag::RPMTAG_ARCHIVESIZE)
                        .or_else(|_e| {
                            rpm.header
                                .get_entry_data_as_u32(rpm::IndexTag::RPMTAG_ARCHIVESIZE)
                                .map(u64::from)
                        })
                        .ok(),
                },
                format: PrimaryFormat {
                    license: rpm.get_license().unwrap_or_default().into(),
                    vendor: rpm.get_vendor().ok().map(Into::into),
                    group: rpm.get_group().ok().map(Into::into),
                    buildhost: rpm.get_build_host().ok().map(Into::into),
                    sourcerpm: rpm.get_source_rpm().ok().map(Into::into),
                    // header_range: Self::get_header_byte_range(&mut f)?,
                    requires: &Dependencies::from(rpm.get_requires()?),
                    provides: &Dependencies::from(rpm.get_provides()?),
                    conflicts: &Dependencies::from(rpm.get_conflicts()?),
                    obsoletes: &Dependencies::from(rpm.get_obsoletes()?),
                    recommends: &Dependencies::from(rpm.get_recommends()?),
                    suggests: &Dependencies::from(rpm.get_suggests()?),
                    supplements: &Dependencies::from(rpm.get_supplements()?),
                    enhances: &Dependencies::from(rpm.get_enhances()?),
                    files: rpm.get_file_entries()?.into_iter().map(Into::into).collect(),
                },
                ..
            })?
            .as_bytes(),
        )?;
        Ok(())
    }
    fn del<'t, 'db>(&self, txn: &'t mut heed::RwTxn<'db>, path: &[u8]) -> heed::Result<()> {
        self.db.get().expect("db uninit").delete(txn, path)?;
        Ok(())
    }

    fn on_ready<'db>(
        &self,
        ready: super::MetanReady,
    ) -> std::io::Result<Option<super::repomd::Data>> {
        let super::MetanGeneration { csum, osum, comp_ext, timestamp, size, open_size } =
            ready.generation.expect("no generation");
        let href = format!("repodata/{}-primary.xml.{comp_ext}", csum.sha).into();
        Ok(Some(super::repomd::Data {
            r#type: "primary".into(),
            checksum: csum,
            open_checksum: osum,
            location: super::repomd::Location { href },
            timestamp,
            size,
            open_size,
        }))
    }

    fn on_post_repomd<'db>(
        &self,
        _: Arc<heed::Env<heed::WithoutTls>>,
        _: &super::repomd::repomd,
    ) -> std::io::Result<()> {
        Ok(())
    }

    async fn on_generate<'t, 'db>(
        &self,
        env: std::sync::Arc<heed::Env<heed::WithoutTls>>,
        mut w: std::pin::Pin<Box<dyn tokio::io::AsyncWrite + Send>>,
    ) -> Result<(), super::MetanError> {
        let db = self.db.get().expect("db uninit");
        let txn = env.read_txn()?;
        w.write_all(
                br#"<?xml version="1.0" encoding="UTF-8"?><metadata xmlns="http://linux.duke.edu/metadata/common" xmlns:rpm="http://linux.duke.edu/metadata/rpm" packages=""#
            ).await?;
        w.write_all(db.len(&*txn)?.to_string().as_bytes()).await?;
        w.write_all(b"\">").await?;

        let (tx, rx) = crossbeam_channel::bounded(16);
        let env2 = std::sync::Arc::clone(&env);
        let db2 = Arc::clone(db);

        let task = tokio::task::spawn_blocking(move || {
            let txn = env2.read_txn()?;
            let it = db2.iter(&txn)?;
            for frag in it.map(|r| r.map(|(_, v)| v)) {
                // PERF: need to clone here unfortunately, `txn` is !Send and the lifetime of `frag`
                // `&'1 [u8]` is from `txn`.
                tx.send(frag?.to_vec());
            }
            heed::Result::Ok(())
        });

        for frag in rx {
            w.write_all(&frag).await?;
        }
        task.await.expect("cannot join")?;

        w.write_all(b"</metadata>").await?;
        Ok(())
    }
}
