pub mod hierarchy;
use std::collections::HashSet;

use crate::{prelude::*, repo::hierarchy::Hierarchize};
use futures::StreamExt;
use kuchiyose::link::LinkBuf;
use tokio::io::{AsyncRead, AsyncWriteExt};

#[derive(Clone, Debug)]
pub enum FragRequest {
    Cached,
    /// Request a new (cache-miss) package to be inserted. The checksum can be given if precomputed.
    Parse {
        csum: Option<String>,
    },
}

impl FragRequest {
    #[must_use]
    pub const fn parse() -> Self {
        Self::Parse { csum: None }
    }
    #[must_use]
    pub fn parse_with_csum(csum: String) -> Self {
        Self::Parse { csum: Some(csum) }
    }
    #[must_use]
    pub const fn cached() -> Self {
        Self::Cached
    }
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
                _ = tx.send((p.clone(), FragRequest::parse()));
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
        // FIXME: paths should be owned
        // PERF: feels pretty inefficient
        let mut bad_filenames: Vec<PathBuf> = Vec::new();
        let mut removed: Vec<LinkBuf> = Vec::new();

        let keys = self.cache.keys()?;
        let parsed: Vec<_> = keys
            .iter()
            .filter_map(|k| {
                let os = OsStr::from_bytes(k);
                let filename = std::path::Path::new(os).file_name()?;
                let p = kuchiyose::rpm::parse_filename(filename.as_bytes())?;
                Some((LinkBuf::from(k.as_slice()), p))
            })
            .collect();

        for path in paths {
            let filename = path.file_name().expect("bad filename");
            let Some(link) = self.cache.cfg.hier.locate_relative(filename) else {
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
                    .filter(|(l, _)| *l != link)
                    .map(|(l, _)| l.clone()),
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
        let repomd = self.cache.write_all(&self.comp_cfg)?;
        if let Some(sig) = &self.sig {
            let asc_link = self.cache.cfg.hier.basedir().join("repodata/repomd.xml.asc");
            let async_write = self.cache.cfg.store.writer(&asc_link).await?;
            // TODO: find async pgp?
            let mut asc_fd = tokio_util::io::SyncIoBridge::new(async_write);
            sig.sign(&repomd)?
                .to_armored_writer(&mut asc_fd, pgp::composed::ArmorOptions::default())?;
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
            expected_keys.insert(rel.as_bytes().to_vec());

            if incremental && self.cache.has(rel.as_bytes())? {
                ret.cached += 1;
                continue;
            }
            paths_to_add.push(rel.as_path().to_owned());
        }

        if !paths_to_add.is_empty() {
            // FIXME: we don't really need replace here, we will prune() anyway
            // FIXME: we don't modify stuff in store?
            self.add_replace(&paths_to_add).await?;
        }

        ret.repomd = self.cache.write_all(&self.comp_cfg)?;

        if incremental {
            let expected_refs: HashSet<_> = expected_keys.iter().map(|k| &**k).collect();
            ret.removed = self.cache.prune(&expected_refs)?;
        }

        Ok(ret)
    }

    /// Delete a list of packages from the cache and from the store.
    ///
    /// Returns the subset of `links` that were not in the cache (they are left alone
    /// in the store too).
    ///
    /// # Errors
    /// Propagates cache and store errors.
    pub async fn del(&self, links: &[LinkBuf]) -> Res<Vec<LinkBuf>> {
        let keys: Vec<&[u8]> = links.iter().map(LinkBuf::as_bytes).collect();
        let not_found = self.cache.delete_pkgs(&keys)?;
        let not_found: Vec<LinkBuf> =
            not_found.iter().map(|k| LinkBuf::from(OsStr::from_bytes(k))).collect();

        for link in links {
            if not_found.contains(link) {
                continue;
            }
            self.cache.cfg.store.delete(link.as_link()).await?;
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
        mut reader: R,
    ) -> Res<crate::repodata::repomd::Data>
    where
        R: AsyncRead + Send + Unpin,
    {
        let tmp_link = self.cache.cfg.hier.basedir().join("repodata").join(dt);
        let ftmm = self.cache.cfg.ftmm;
        let writer = self.cache.cfg.store.writer(tmp_link.as_link()).await?;

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
        let href = LinkBuf::from(format!(
            "{}/repodata/{sha_hex}-{dt}.zst",
            self.cache.cfg.hier.basedir().as_str().trim_end_matches('/')
        ));
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
        self.cache.write_custom_datatype(&data)?;
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
        self.cache.cfg.store.delete(data.location.href.as_link()).await?;
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
    pub removed: Vec<LinkBuf>,
}
