pub mod hierarchy;
use std::collections::HashSet;

use crate::{prelude::*, repo::hierarchy::Hierarchize};
use futures::StreamExt;
use kuchiyose::link::LinkBuf;
use tokio::io::{AsyncRead, AsyncWriteExt};

#[derive(Debug)]
pub enum FragRequest {
    Cached,
    /// Request a new (cache-miss) package to be inserted.
    Put(Vec<crate::repodata::MetanComputed>),
}

#[derive(Debug)]
pub struct Repo<H: Hierarchize> {
    /// Temporary directory used when the store is remote.
    ///
    /// `libsubatomic` needs to parse rpm files locally. Temporary rpm files will be stored in this
    /// directory.
    ///
    /// This value is ignored when the store is local.
    pub tempdir: Option<PathBuf> = None,
    pub cache: crate::cache::Cache<H>,
    pub sig: Option<crate::sig::Mgr> = None,
    pub comp_cfg: kuchiyose::comp::CompConfig,
}

impl<H: Hierarchize> Repo<H> {
    /// Enqueue local RPMs for insertion into the cache.
    ///
    /// This is synchronous; the caller must supply files that are already on disk.
    /// For remote stores, download to `self.tempdir` first.
    ///
    /// # Errors
    /// Propagates channel and cache errors.
    fn add(&self, paths: &[PathBuf]) -> Res<()> {
        let (tx, rx) = crossbeam_channel::bounded(num_cpus::get() * 20);
        let cache = &self.cache;
        std::thread::scope(|s| {
            let handle = s.spawn(|| cache.update_frags(&rx));
            paths.par_iter().for_each(|p| {
                let Ok(computed) = cache
                    .compute(p, crate::cache::ComputeInput::default())
                    .inspect_err(|err| tracing::error!(?err, "cannot compute frag"))
                else {
                    return;
                };
                _ = tx.send((p.clone(), FragRequest::Put(computed)));
            });
            drop(tx);
            let (_, _) = handle.join().expect("worker thread panicked")?;
            Ok(())
        })
    }

    /// Upsert packages, removing previous versions of the same (name, arch).
    ///
    /// # Errors
    /// Propagates cache and store errors.
    async fn add_replace(&self, paths: &[PathBuf]) -> Res<AddReplaceOutput> {
        let mut bad_filenames: Vec<PathBuf> = Vec::new();
        let mut removed: Vec<Vec<u8>> = Vec::new();

        let keys = self.cache.keys()?; // keys are filenames
        let parsed: Vec<_> = keys
            .iter()
            .filter_map(|k| {
                let p = kuchiyose::rpm::parse_filename(k)?;
                Some((k.clone(), p))
            })
            .collect();

        for path in paths {
            let filename = path.file_name().expect("bad filename");
            let Some(_link) = self.cache.cfg.hier.locate_relative(filename) else {
                bad_filenames.push(path.clone());
                continue;
            };
            let Some(crate::pkg::ParsePathOutput { name, arch, .. }) =
                kuchiyose::rpm::parse_filename(filename.as_bytes())
            else {
                bad_filenames.push(path.clone());
                continue;
            };
            removed.extend(
                parsed
                    .iter()
                    .filter(|(_, k)| k.name == name && k.arch == arch)
                    .filter(|(k, _)| k.as_slice() != filename.as_bytes())
                    .map(|(k, _)| k.clone()),
            );
        }

        if !removed.is_empty() {
            self.del(&removed).await?;
        }
        self.add(paths)?;
        Ok(AddReplaceOutput { bad_filenames, removed })
    }

    /// Trigger repository generation.
    ///
    /// # Errors
    /// Propagates IO, cache, and possibly [`pgp`] errors.
    #[doc(alias = "createrepo")]
    pub async fn generate(&self) -> Res<Vec<u8>> {
        let repomd = self.cache.write_all(&self.comp_cfg).await?;
        if let Some(sig) = &self.sig {
            let asc_link = self.cache.cfg.hier.basedir().join("repodata/repomd.xml.asc");
            let mut writer = self.cache.cfg.store.writer(&asc_link).await?;
            let mut buf = Vec::new();
            sig.sign(&repomd)?
                .to_armored_writer(&mut buf, pgp::composed::ArmorOptions::default())?;
            writer.write_all(&buf).await?;
        }
        Ok(repomd)
    }

    /// Re-scan the store, upsert what has changed, prune what has gone, then generate.
    ///
    /// # Errors
    /// Propagates store, cache, and IO errors.
    ///
    /// # Panics
    /// The function panics if it encounters `..` as a file name.
    pub async fn regenerate(&self, incremental: bool) -> Res<RegenerateOutput> {
        let stream = self.cache.cfg.hier.iter_rpms(&self.cache.cfg.store).await;
        let mut stream = std::pin::pin!(stream);

        let mut expected_keys: HashSet<Vec<u8>> = HashSet::new();
        let mut paths_to_add: Vec<PathBuf> = Vec::new();
        let mut ret = RegenerateOutput::default();

        while let Some(rel) = stream.next().await {
            let rel = rel?;
            if try {
                expected_keys.insert(rel.as_path().file_name()?.as_bytes().to_owned());
            }
            .is_none()
            {
                tracing::error!(?rel, "iter_rpms gave bad filename");
                continue;
            }

            if incremental && self.cache.has(rel.as_bytes())? {
                ret.cached += 1;
                continue;
            }
            paths_to_add.push(rel.as_path().to_owned());
        }

        if !paths_to_add.is_empty() {
            self.add_replace(&paths_to_add).await?;
        }

        ret.repomd = self.cache.write_all(&self.comp_cfg).await?;

        if incremental {
            let expected_refs: HashSet<_> = expected_keys.iter().map(|k| &**k).collect();
            ret.removed = self.cache.prune(&expected_refs)?;
        }

        Ok(ret)
    }

    /// Delete a list of packages by filename.
    ///
    /// Return a list of packages not found in the cache.
    ///
    /// # Errors
    /// Propagates cache and store errors.
    pub async fn del(&self, filenames: &[Vec<u8>]) -> Res<Vec<Vec<u8>>> {
        let keys: Vec<&[u8]> = filenames.iter().map(Vec::as_slice).collect();
        let not_found = self.cache.delete_pkgs(&keys)?;
        let not_found: Vec<Vec<u8>> = not_found.into_iter().map(<[u8]>::to_vec).collect();

        for filename in filenames {
            if not_found.contains(filename) {
                continue;
            }
            let Some(link) = self.cache.cfg.hier.locate_relative(OsStr::from_bytes(filename))
            else {
                continue;
            };
            let link = self.cache.cfg.hier.basedir().join(&link);
            self.cache.cfg.store.delete(&link).await?;
        }
        Ok(not_found)
    }

    /// Add or replace a custom datatype (e.g. `"group"` for `comps.xml`).
    ///
    /// The content is streamed through the compression + checksum pipeline and stored
    /// under `repodata/{checksum}-{dt}.zst`. A matching `<data type="{dt}">` entry is
    /// recorded in the cache so it appears in the next `repomd.xml`.
    ///
    /// `reader` may be any [`AsyncRead`] — a slice, a file, a stream.
    ///
    /// # Errors
    /// Propagates store, cache, and compression errors.
    pub async fn write_custom<R>(
        &self,
        dt: &str,
        filename_suffix: &str,
        mut reader: R,
    ) -> Res<crate::repodata::repomd::Data>
    where
        R: AsyncRead + Send + Unpin,
    {
        let tmp_link = self.cache.cfg.hier.basedir().join("repodata").join(dt);
        let ftmm = self.cache.cfg.ftmm;
        let writer = self.cache.cfg.store.writer(&tmp_link).await?;

        let mut inner_mochi = kuchiyose::comp::Mochi::new(writer, ftmm);
        let (open_size, open_checksum) = {
            let mut w = self.comp_cfg.to_mochi(&mut inner_mochi, ftmm);
            tokio::io::copy(&mut reader, &mut w).await?;
            w.shutdown().await?;
            (w.size, w.ftmm.finalize())
        };
        let size = inner_mochi.size;
        let checksum = inner_mochi.ftmm.finalize();

        let sha_hex = hex::encode(&checksum).into();
        let href = format!("repodata/{sha_hex}-{filename_suffix}.{}", self.comp_cfg.ext());
        let href = LinkBuf::from(href);
        let link = self.cache.cfg.hier.basedir().join(&href);
        self.cache.cfg.store.rename(&tmp_link, &link).await?;
        let data = crate::repodata::repomd::Data {
            r#type: dt.into(),
            checksum: crate::repodata::repomd::Checksum { r#type: ftmm, sha: sha_hex },
            open_checksum: crate::repodata::repomd::Checksum {
                r#type: ftmm,
                sha: hex::encode(&open_checksum).into(),
            },
            location: crate::repodata::repomd::Location { href },
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time underflow")
                .as_secs()
                .try_into()
                .expect("time underflow"),
            size,
            open_size,
        };
        if let Some(old) = self.cache.write_custom_datatype(&data)? {
            let href = self.cache.cfg.hier.basedir().join(old.location.href);
            self.cache.cfg.store.delete(&href).await?;
        }
        Ok(data)
    }

    /// Remove a custom datatype from the cache and from the store.
    ///
    /// # Errors
    /// Propagates cache and store errors.
    pub async fn del_custom(&self, dt: &str) -> Res<Option<crate::repodata::repomd::Data>> {
        let Some(data) = self.cache.del_custom_datatype(dt)? else {
            return Ok(None);
        };
        let link = self.cache.cfg.hier.basedir().join(&data.location.href);
        self.cache.cfg.store.delete(&link).await?;
        Ok(Some(data))
    }
}

#[derive(Debug, Default)]
pub struct RegenerateOutput {
    pub skipped: Vec<(PathBuf, rpm::Error)> = Vec::new(),
    pub cached: usize = 0,
    pub removed: u64 = 0,
    pub repomd: Vec<u8> = Vec::new(),
}

#[derive(Clone, Debug)]
pub struct AddReplaceOutput {
    pub bad_filenames: Vec<PathBuf>,
    pub removed: Vec<Vec<u8>>,
}
