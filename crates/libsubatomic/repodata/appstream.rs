//! Handle appstream xml serialization & deserialization.
use quick_xml::events::{BytesText, Event};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;

use crate::prelude::*;

/// Transform package appstream xml to repodata appstream fragment.
///
/// Current implementation only adds the `<pkgname />` tag.
///
/// If `filesize` is given, allocate a buffer with that size. The caller is responsible for making
/// sure `filesize` is an acceptable size.
///
/// # Errors
/// Return errors when the given xml (`reader`) cannot be parsed.
///
/// # Panics
///
/// Panic when [`std::io::Error`] is raised by writing to `out`.
pub fn transform<R: std::io::BufRead>(
    pkgname: &str,
    reader: R,
    filesize: Option<usize>,
    out: &mut Vec<u8>,
) -> quick_xml::Result<()> {
    let mut reader = quick_xml::Reader::from_reader(reader);
    reader.config_mut().trim_text(true);
    let mut buf = filesize.map_or_else(Vec::new, Vec::with_capacity);
    let mut writer = quick_xml::Writer::new(out);
    let Err(e) = try {
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) if e.name().as_ref() == "component" => {
                    writer.write_event(Event::Start(e))?;
                    writer.create_element("pkgname").write_text_content(BytesText::new(pkgname))?;
                }
                Ok(Event::Eof) => return Ok(()),
                Ok(Event::Decl(_) | Event::PI(_) | Event::DocType(_)) => {}
                Ok(e) => writer.write_event(e)?,
                Err(e) => return Err(e),
            }
            buf.clear();
        }
    };
    panic!("unexpected io error during appstream::transform: {e}");
}

#[must_use]
pub fn is_appstream_file(path: &Path) -> bool {
    path.starts_with("/usr/share/metainfo/") && path.extension().is_some_and(|ext| ext == "xml")
}

/// Generate appstream fragment for this rpm package using [`crate::repodata::appstream::transform`].
///
/// # Performance
/// This operation is slightly expensive and requires decompressing specific files in the archive.
/// This requires a linear search against the full list of files in the rpm. Documentation from
/// [`rpm::PackageReader::next_file`] suggests only wanted files are decompressed.
///
/// # Errors
/// RPM errors are propagated. If parsing an appstream xml file failed, no errors will be
/// returned and a warning ([`tracing::warn!`]) will be issued instead.
pub fn appstream_frag(rpm: &mut rpm::PackageReader) -> Result<Vec<u8>, rpm::Error> {
    // PERF: do we need this search beforehand?
    if !rpm.metadata.get_file_entries()?.into_iter().any(|f| is_appstream_file(&f.path())) {
        return Ok(Vec::new());
    }
    let mut appstream_frag = Vec::new();
    let pkgname = rpm.metadata.get_name()?.to_owned();
    while let Some(mut f) = rpm.next_file()? {
        if is_appstream_file(&f.metadata.path()) {
            let size = f.metadata.size();
            if let Err(e) = crate::repodata::appstream::transform(
                &pkgname,
                std::io::BufReader::new(&mut f),
                // TODO: what to do if size too large in mem?
                Some(size),
                &mut appstream_frag,
            ) {
                tracing::warn!(
                    pkgname,
                    path = %f.metadata.path().display(),
                    ?e,
                    "cannot parse appstream xml"
                );
            }
        }
        f.finish()?;
    }
    Ok(appstream_frag)
}

#[derive(Debug, Default)]
pub struct AppstreamMetan {
    /// Used as the `origin=` attribute on `<components />`.
    pub repo: String,
    db: super::MetanDb<crate::cache::FragDb> = super::MetanDb::new("app"),
}

type Msg = Option<(Vec<u8>, Vec<u8>)>;

#[async_trait::async_trait]
impl super::Metan for AppstreamMetan {
    fn mdtype(&self) -> &'static str {
        "appstream"
    }
    fn filename(&self) -> &'static str {
        "appstream.xml"
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
        let mut reader = rpm::PackageReader::open(&pkg.path)?;
        let frag = appstream_frag(&mut reader)?;
        let msg: Msg = (!frag.is_empty()).then_some((pkg.filename.clone(), frag));
        Ok(Box::new(msg))
    }

    fn save(
        &self,
        txn: &mut heed::RwTxn<'_>,
        computed: &super::MetanComputed,
    ) -> Result<(), super::MetanError> {
        let computed: &Msg = computed.downcast_ref().expect("bad cast");
        if let Some((filename, frag)) = computed {
            self.db.put(txn, filename, frag)?;
        }
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
        let href = format!("repodata/{}-appstream.xml.{}", csum.sha, comp_ext).into();
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

        // Note: no `packages=` count on the appstream envelope — just `origin`.
        w.write_all(
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?><components origin="{}" version="0.14">"#,
                self.repo
            )
            .as_bytes(),
        )
        .await?;

        let (tx, rx) = crossbeam_channel::bounded(16);
        let env2 = Arc::clone(&env);
        let db2 = self.db.arc();

        let task = tokio::task::spawn_blocking(move || {
            let txn = env2.read_txn()?;
            let it = db2.iter(&txn)?;
            for frag in it.map(|r| r.map(|(_, v)| v)) {
                // `txn` is !Send; we copy the bytes out.
                tx.send(frag?.to_vec()).ok();
            }
            heed::Result::Ok(())
        });

        for frag in rx {
            w.write_all(&frag).await?;
        }
        task.await.expect("cannot join")?;

        w.write_all(b"</components>").await?;
        drop(txn); // hold the read txn until we're done iterating
        Ok(())
    }
}
