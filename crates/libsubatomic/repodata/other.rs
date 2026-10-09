use std::sync::Arc;
use tokio::io::AsyncWriteExt;

use crate::{
    pkg::{Changelog, Version},
    prelude::*,
};

#[expect(dead_code, reason = "for reference")]
#[derive(Clone, Debug, Serialize)]
#[serde(rename = "otherdata")]
struct OtherMetadata<'a> {
    #[serde(rename = "@xmlns")]
    pub xmlns: &'static str = "http://linux.duke.edu/metadata/other",
    #[serde(rename = "@packages")]
    pub packages: u64,
    #[serde(rename = "package")]
    pub packages_list: Vec<OtherPackage<'a>>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename = "package")]
struct OtherPackage<'a> {
    #[serde(rename = "@pkgid")]
    pub pkgid: &'a str,
    #[serde(rename = "@name")]
    pub name: &'a str,
    #[serde(rename = "@arch")]
    pub arch: &'a str,
    pub version: &'a Version,
    #[serde(rename = "changelog", default)]
    pub changelogs: &'a [Changelog],
}

#[derive(Debug, Default)]
pub struct OtherMetan {
    db: super::MetanDb<super::FragDb> = super::MetanDb::new("oth"),
}

type Msg = (Vec<u8>, std::string::String);

#[async_trait::async_trait]
impl super::Metan for OtherMetan {
    fn mdtype(&self) -> &'static str {
        "other"
    }
    fn filename(&self) -> &'static str {
        "other.xml"
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

        let version = Version {
            epoch: rpm.get_epoch().unwrap_or(0).into(),
            ver: rpm.get_version()?.into(),
            rel: rpm.get_release()?.into(),
        };
        let changelogs: Vec<Changelog> =
            rpm.get_changelog_entries()?.into_iter().map(Into::into).collect();

        let frag = OtherPackage {
            pkgid: &pkg.csum,
            name: rpm.get_name()?,
            arch: rpm.get_arch()?,
            version: &version,
            changelogs: &changelogs,
        };
        let xml = quick_xml::se::to_string(&frag)?;
        let msg: Msg = (pkg.filename.clone(), xml);
        Ok(Box::new(msg))
    }

    fn save(
        &self,
        txn: &mut heed::RwTxn<'_>,
        computed: &super::MetanComputed,
    ) -> Result<(), super::MetanError> {
        let computed: &Msg = computed.downcast_ref().expect("bad cast");
        self.db.put(txn, &computed.0, computed.1.as_bytes())?;
        Ok(())
    }

    fn del(&self, txn: &mut heed::RwTxn<'_>, path: &[u8]) -> heed::Result<()> {
        self.db.delete(txn, path)?;
        Ok(())
    }

    fn on_ready<'db>(
        &self,
        ready: super::MetanReady,
    ) -> std::io::Result<Option<super::repomd::Data>> {
        let super::MetanGeneration { csum, osum, comp_ext, timestamp, size, open_size } =
            ready.generation.expect("no generation");
        let href = format!("repodata/{}-other.xml.{comp_ext}", csum.sha).into();
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

    fn on_post_repomd<'db>(
        &self,
        _: Arc<heed::Env<heed::WithoutTls>>,
        _: &super::repomd::repomd,
    ) -> std::io::Result<()> {
        Ok(())
    }

    async fn on_generate<'t, 'db>(
        &self,
        env: Arc<heed::Env<heed::WithoutTls>>,
        mut w: std::pin::Pin<Box<dyn tokio::io::AsyncWrite + Send + 't>>,
    ) -> Result<(), super::MetanError> {
        let txn = env.read_txn()?;
        w.write_all(
            br#"<?xml version="1.0" encoding="UTF-8"?><otherdata xmlns="http://linux.duke.edu/metadata/other" packages=""#,
        ).await?;
        w.write_all(self.db.len(&txn)?.to_string().as_bytes()).await?;
        w.write_all(b"\">").await?;

        let (tx, rx) = crossbeam_channel::bounded(16);
        let env2 = Arc::clone(&env);
        let db2 = self.db.arc();

        let task = tokio::task::spawn_blocking(move || {
            let txn = env2.read_txn()?;
            let it = db2.iter(&txn)?;
            for frag in it.map(|r| r.map(|(_, v)| v)) {
                tx.send(frag?.to_vec()).ok();
            }
            heed::Result::Ok(())
        });

        for frag in rx {
            w.write_all(&frag).await?;
        }
        task.await.expect("cannot join")?;

        w.write_all(b"</otherdata>").await?;
        Ok(())
    }
}
