use std::sync::{Arc, OnceLock};
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
#[non_exhaustive]
pub struct OtherMetan {
    db: OnceLock<Arc<super::FragDb>>,
}

#[async_trait::async_trait]
impl super::Metan for OtherMetan {
    fn mdtype(&self) -> &str {
        "other"
    }
    fn filename(&self) -> &str {
        "other"
    }
    fn db_count(&self) -> u32 {
        1
    }

    fn db_init<'s, 't, 'db>(
        &'s self,
        env: Arc<heed::Env<heed::WithoutTls>>,
        txn: &'t mut heed::RwTxn<'db>,
    ) -> heed::Result<()> {
        self.db.set(Arc::new(env.create_database(txn, Some("oth"))?)).expect("double db_init");
        Ok(())
    }

    fn save<'t, 'db>(
        &self,
        txn: &'t mut heed::RwTxn<'db>,
        pkg: &crate::pkg::MetanInput,
    ) -> Result<(), super::MetanError> {
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

        self.db.get().expect("db uninit").put(
            txn,
            &pkg.link.as_bytes(),
            quick_xml::se::to_string(&frag)?.as_bytes(),
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
        let db = self.db.get().expect("db uninit");
        let txn = env.read_txn()?;
        w.write_all(
            br#"<?xml version="1.0" encoding="UTF-8"?><otherdata xmlns="http://linux.duke.edu/metadata/other" packages=""#,
        ).await?;
        w.write_all(db.len(&*txn)?.to_string().as_bytes()).await?;
        w.write_all(b"\">").await?;

        let (tx, rx) = crossbeam_channel::bounded(16);
        let env2 = Arc::clone(&env);
        let db2 = Arc::clone(db);

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
