use std::sync::Arc;

use tokio::io::AsyncWriteExt;

use crate::{
    pkg::{Dependencies, FileEntry, Size, Time, Version},
    prelude::*,
};

#[expect(dead_code, reason = "for reference")]
#[derive(Clone, Debug, Serialize)]
#[serde(rename = "metadata")]
struct PrimaryMetadata<'a> {
    #[serde(rename = "@xmlns")]
    pub xmlns: &'static str = "http://linux.duke.edu/metadata/common",
    #[serde(rename = "@xmlns:rpm")]
    pub xmlns_rpm: &'static str = "http://linux.duke.edu/metadata/rpm",
    #[serde(rename = "@packages")]
    pub packages: u64,
    #[serde(rename = "package")]
    pub packages_list: Vec<Package<'a>>,
}

#[allow(clippy::struct_field_names)]
#[derive(Clone, Debug, Serialize)]
#[serde(rename = "package")]
struct Package<'a> {
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
struct PrimaryFormat<'a> {
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
struct PackageChecksum<'a> {
    #[serde(rename = "@type")]
    pub checksum_type: &'static str,
    #[serde(rename = "@pkgid")]
    pub pkgid: &'static str = "YES",
    #[serde(rename = "$text")]
    pub value: &'a str,
}

#[derive(Clone, Debug, Serialize)]
struct PackageLocation<'a> {
    #[serde(rename = "@href")]
    pub href: &'a str,
}

// https://github.com/madonuko/createrepo_nim/blob/719b99a469101c61441623f9fecfd3c7d977fbcb/src/rpm.nim#L160
// https://github.com/rpm-software-management/createrepo_c/blob/5cf41fe5d703901d78078ed18c67ab667e446c1a/src/misc.c#L248
fn get_header_byte_range(f: &mut std::fs::File) -> std::io::Result<HeaderRange> {
    f.seek(std::io::SeekFrom::Start(104))?;
    let mut bytes = [0u8; 2];
    f.read_exact(&mut bytes)?;
    let sigindex = bytes[0].to_be();
    let sigdata = bytes[1].to_be();
    let sigindexsize = sigindex * 16;
    let sigsize = u64::from(sigdata) + u64::from(sigindexsize);
    let mut disttoboundary = sigsize % 8;
    if disttoboundary != 0 {
        disttoboundary = 8 - disttoboundary;
    }
    let hdrstart: u64 = 112 + sigsize + disttoboundary;

    f.seek(std::io::SeekFrom::Start(hdrstart + 8))?;
    f.read_exact(&mut bytes)?;
    let hdrindex = u64::from(bytes[0].to_be());
    let hdrdata = u64::from(bytes[1].to_be());
    let hdrindexsize = hdrindex * 16;
    let hdrsize = hdrdata + hdrindexsize + 16;
    let hdrend = hdrstart + hdrsize;
    if hdrend < hdrstart {
        return Err(std::io::Error::other(format!(
            "sanity check fail (hdrend {hdrend} < hdrstart {hdrstart})"
        )));
    }
    Ok(HeaderRange { start: hdrstart, end: hdrend })
}

#[derive(Clone, Debug, Serialize)]
struct HeaderRange {
    #[serde(rename = "@start")]
    pub start: u64,
    #[serde(rename = "@end")]
    pub end: u64,
}

#[derive(Debug, Default)]
pub struct PrimaryMetan {
    db: super::MetanDb<super::FragDb> = super::MetanDb::new("pri"),
}

type Msg = (crate::Kirifuda, std::string::String);

#[async_trait::async_trait]
impl super::Metan for PrimaryMetan {
    fn mdtype(&self) -> &'static str {
        "primary"
    }
    fn filename(&self) -> &'static str {
        "primary.xml"
    }
    fn db_count(&self) -> u32 {
        1
    }
    fn db_init(
        &self,
        env: Arc<heed::Env<heed::WithoutTls>>,
        txn: &mut heed::RwTxn<'_>,
    ) -> heed::Result<()> {
        self.db.init(env, txn)?;
        Ok(())
    }
    fn compute(
        &self,
        pkg: &crate::pkg::MetanInput,
    ) -> Result<super::MetanComputed, super::MetanError> {
        let rpm = &pkg.metadata;
        let p = Package {
            location: PackageLocation { href: pkg.link.as_str() },
            name: rpm.get_name()?,
            arch: rpm.get_arch()?,
            version: &Version {
                epoch: rpm.get_epoch().unwrap_or(0).into(),
                ver: rpm.get_version()?.into(),
                rel: rpm.get_release()?.into(),
            },
            checksum: PackageChecksum {
                checksum_type: pkg.csum_type.as_str(),
                value: &pkg.csum,
                ..
            },
            summary: rpm.get_summary().unwrap_or_default(),
            description: rpm.get_description().unwrap_or_default(),
            packager: rpm.get_packager().ok(),
            url: rpm.get_url().ok(),
            time: &Time { file: epoch!(pkg.fmeta.created()?), build: rpm.get_build_time()? },
            size: &Size {
                package: pkg.fmeta.size(),
                installed: rpm.get_installed_size()?,
                archive: rpm
                    .header
                    .get_entry_data_as_u64(rpm::IndexTag::RPMTAG_PAYLOADSIZEALT)
                    .or_else(|_| {
                        rpm.header.get_entry_data_as_u64(rpm::IndexTag::RPMTAG_LONGARCHIVESIZE)
                    })
                    .ok(),
            },
            format: PrimaryFormat {
                license: rpm.get_license().unwrap_or_default(),
                vendor: rpm.get_vendor().ok(),
                group: rpm.get_group().ok(),
                buildhost: rpm.get_build_host().ok(),
                sourcerpm: rpm.get_source_rpm().ok(),
                // header_range: get_header_byte_range(&mut f)?,
                requires: &Dependencies::from_requires(rpm.get_requires()?),
                provides: &Dependencies::from(rpm.get_provides()?),
                conflicts: &Dependencies::from(rpm.get_conflicts()?),
                obsoletes: &Dependencies::from(rpm.get_obsoletes()?),
                recommends: &Dependencies::from(rpm.get_recommends()?),
                suggests: &Dependencies::from(rpm.get_suggests()?),
                supplements: &Dependencies::from(rpm.get_supplements()?),
                enhances: &Dependencies::from(rpm.get_enhances()?),
                files: rpm
                    .get_file_entries()?
                    .into_iter()
                    .filter(is_primary)
                    .map(Into::into)
                    .collect(),
            },
            ..
        };
        let xml = quick_xml::se::to_string(&p)?;
        let msg: Msg = (pkg.filename.clone(), xml);
        Ok(super::MetanComputed(Box::new(msg)))
    }

    fn save(
        &self,
        txn: &mut heed::RwTxn<'_>,
        computed: &super::MetanComputed,
    ) -> Result<(), super::MetanError> {
        let computed: &Msg = computed.0.downcast_ref().expect("bad cast");
        self.db.put(txn, computed.0.as_bytes(), computed.1.as_bytes())?;
        Ok(())
    }

    fn del(&self, txn: &mut heed::RwTxn<'_>, path: &crate::Kiri) -> heed::Result<()> {
        self.db.delete(txn, path.as_bytes())?;
        Ok(())
    }

    fn on_ready(&self, ready: super::MetanReady) -> std::io::Result<Option<super::repomd::Data>> {
        let super::MetanGeneration { csum, osum, comp_ext, timestamp, size, open_size, .. } =
            ready.generation.expect("no generation");
        let href = format!("repodata/{}-primary.xml.{comp_ext}", csum.sha).into();
        Ok(Some(super::repomd::Data {
            r#type: self.mdtype().into(),
            checksum: csum,
            open_checksum: osum,
            location: super::repomd::Location { href },
            timestamp,
            size,
            open_size,
        }))
    }

    fn on_post_repomd(
        &self,
        _: Arc<heed::Env<heed::WithoutTls>>,
        _: &super::repomd::repomd,
    ) -> std::io::Result<()> {
        Ok(())
    }

    async fn on_generate<'t, 'db>(
        &self,
        env: std::sync::Arc<heed::Env<heed::WithoutTls>>,
        mut w: std::pin::Pin<Box<dyn tokio::io::AsyncWrite + Send + 't>>,
    ) -> Result<(), super::MetanError> {
        let txn = env.read_txn()?;
        w.write_all(
                br#"<?xml version="1.0" encoding="UTF-8"?><metadata xmlns="http://linux.duke.edu/metadata/common" xmlns:rpm="http://linux.duke.edu/metadata/rpm" packages=""#
            ).await?;
        w.write_all(self.db.len(&txn)?.to_string().as_bytes()).await?;
        w.write_all(b"\">").await?;

        let (tx, rx) = crossbeam_channel::bounded(num_cpus::get() * 2);
        let env2 = std::sync::Arc::clone(&env);
        let db2 = self.db.arc();

        let task = tokio::task::spawn_blocking(move || {
            let txn = env2.read_txn()?;
            let it = db2.iter(&txn)?;
            for frag in it.map(|r| r.map(|(_, v)| v)) {
                // PERF: need to clone here unfortunately, `txn` is !Send and the lifetime of `frag`
                // `&'1 [u8]` is from `txn`.
                tx.send(frag?.to_vec()).expect("tx closed");
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

//? cr_is_primary
pub fn is_primary(f: &rpm::FileEntry<'_>) -> bool {
    inner_is_primary(f.path().as_os_str().as_bytes())
}

fn inner_is_primary(p: &[u8]) -> bool {
    p.starts_with(b"/etc/")
        || p == b"/usr/lib/sendmail"
        || memchr::memmem::find(p, b"bin/").is_some()
}

#[cfg(test)]
mod test {
    use super::*;

    // ── is_primary ──────────────────────────────────────────────────
    #[test]
    fn is_primary_etc() {
        assert!(inner_is_primary(b"/etc/foo"));
        assert!(inner_is_primary(b"/etc/"));
        assert!(inner_is_primary(b"/etc/passwd"));
        assert!(!inner_is_primary(b"/etc"));
        assert!(!inner_is_primary(b"/etcfoo"));
    }

    #[test]
    fn is_primary_sendmail() {
        assert!(inner_is_primary(b"/usr/lib/sendmail"));
        assert!(!inner_is_primary(b"/usr/lib/sendmail/foo"));
        assert!(!inner_is_primary(b"/usr/lib/sendmail2"));
    }

    #[test]
    fn is_primary_bin_variants() {
        assert!(inner_is_primary(b"/usr/bin/bash"));
        assert!(inner_is_primary(b"/bin/ls"));
        assert!(inner_is_primary(b"/opt/bin/foo"));
        assert!(inner_is_primary(b"/usr/local/bin/app"));
        assert!(inner_is_primary(b"bin/foo")); // contains bin/
        assert!(inner_is_primary(b"/a/bin/b"));
    }

    #[test]
    fn is_primary_negative() {
        assert!(!inner_is_primary(b"/usr/share/doc/foo"));
        assert!(!inner_is_primary(b"/var/lib/foo"));
        assert!(!inner_is_primary(b"/usr/libexec/foo"));
        assert!(!inner_is_primary(b"/tmp/foo"));
        assert!(!inner_is_primary(b"/usr/lib/sendmailfoo"));
        // "binary" contains "bin" but not "bin/" -> not primary
        assert!(!inner_is_primary(b"/usr/binary/foo"));
        assert!(!inner_is_primary(b"/usr/lib/foo"));
        assert!(!inner_is_primary(b"/opt/lib/foo"));
    }

    #[test]
    fn is_primary_sbin_is_primary() {
        // "sbin/" contains "bin/" substring -> considered primary (createrepo_c uses strstr("bin/"))
        assert!(inner_is_primary(b"/sbin/foo"));
        assert!(inner_is_primary(b"/usr/sbin/foo"));
    }

    #[test]
    fn is_primary_short_paths_no_panic() {
        // previously impl did `0..p.len()-4` which panics on short paths
        assert!(!inner_is_primary(b"/"));
        assert!(!inner_is_primary(b"/a"));
        assert!(!inner_is_primary(b""));
        assert!(!inner_is_primary(b"ab"));
        assert!(!inner_is_primary(b"/ab"));
        assert!(!inner_is_primary(b"/bin")); // no trailing slash, no "bin/"
    }

    #[test]
    fn is_primary_bin_at_edges() {
        assert!(inner_is_primary(b"bin/"));
        assert!(inner_is_primary(b"/bin/"));
        assert!(inner_is_primary(b"a/bin/b"));
    }

    // ── get_header_byte_range ───────────────────────────────────────
    #[test]
    fn header_range_on_real_rpm() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../random-rpm-examples/terra-release-44-4.noarch.rpm");
        let mut f = std::fs::File::open(&path).unwrap();
        let range = get_header_byte_range(&mut f).unwrap();
        let meta = f.metadata().unwrap();
        assert!(range.start >= 112);
        assert!(range.end > range.start);
        assert!(range.end <= meta.len());
        // idempotent
        let mut f2 = std::fs::File::open(&path).unwrap();
        let range2 = get_header_byte_range(&mut f2).unwrap();
        assert_eq!(range.start, range2.start);
        assert_eq!(range.end, range2.end);
    }

    #[test]
    fn header_range_truncated_file_errors() {
        let mut tmp = tempfile::tempfile().unwrap();
        // file shorter than 106 bytes -> read_exact fails
        tmp.write_all(&[0u8; 50]).unwrap();
        tmp.seek(std::io::SeekFrom::Start(0)).unwrap();
        let res = get_header_byte_range(&mut tmp);
        assert!(res.is_err());
    }

    #[test]
    fn header_range_synthetic() {
        // Craft minimal RPM-like header structure:
        // at 104: sigindex=1 (0x01), sigdata=0x00 => sigsize=16, hdrstart=128
        // at hdrstart+8 (136): hdrindex=2 (0x02), hdrdata=0x00 => hdrsize=48, hdrend=176
        let mut tmp = tempfile::tempfile().unwrap();
        // ensure file large enough
        tmp.set_len(200).unwrap();
        tmp.seek(std::io::SeekFrom::Start(104)).unwrap();
        tmp.write_all(&[1, 0]).unwrap();
        tmp.seek(std::io::SeekFrom::Start(136)).unwrap();
        tmp.write_all(&[2, 0]).unwrap();
        let range = get_header_byte_range(&mut tmp).unwrap();
        assert_eq!(range.start, 128);
        assert_eq!(range.end, 176);
    }

    #[test]
    fn header_range_synthetic_with_padding() {
        // sigindex=0, sigdata=5 => sigsize=5, pad=3, hdrstart=120
        // hdrindex=0, hdrdata=1 => hdrsize=17, hdrend=137
        let mut tmp = tempfile::tempfile().unwrap();
        tmp.set_len(200).unwrap();
        tmp.seek(std::io::SeekFrom::Start(104)).unwrap();
        tmp.write_all(&[0, 5]).unwrap();
        // hdrstart = 112+5+3=120, so hdr fields at 128
        tmp.seek(std::io::SeekFrom::Start(128)).unwrap();
        tmp.write_all(&[0, 1]).unwrap();
        let range = get_header_byte_range(&mut tmp).unwrap();
        assert_eq!(range.start, 120);
        assert_eq!(range.end, 137);
    }

    #[test]
    fn header_range_synthetic_no_padding() {
        // sigsize 16 already 8-aligned => no padding
        let mut tmp = tempfile::tempfile().unwrap();
        tmp.set_len(300).unwrap();
        tmp.seek(std::io::SeekFrom::Start(104)).unwrap();
        tmp.write_all(&[1, 0]).unwrap(); // 16
        // hdrstart 128, hdr at 136
        tmp.seek(std::io::SeekFrom::Start(136)).unwrap();
        tmp.write_all(&[0, 8]).unwrap(); // hdrsize 24
        let range = get_header_byte_range(&mut tmp).unwrap();
        assert_eq!(range.start, 128);
        assert_eq!(range.end, 152); // 128+24
    }
}
