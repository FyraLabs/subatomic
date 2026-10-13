//! Repository management via [`Repo`].
//!
//! This module defines [`Repo`], which is the main way of interacting with a repository. While it
//! internally uses [`crate::Cache`], [`Repo`] contains some helpers that you most likely need,
//! such as updating custom datatypes and signing `repomd`.

pub mod hierarchy;

use crate::{prelude::*, repo::hierarchy::Hierarchize};
use kuchiyose::link::LinkBuf;
use tokio::io::{AsyncRead, AsyncWriteExt};

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

    /// Force struct constructions to use the `MyStruct { fields, .. }` notation.
    #[expect(private_interfaces)]
    pub non_exhaustive: crate::NonExhaustive = crate::NonExhaustive,
}

impl<H: Hierarchize> Repo<H> {
    /*
    /// Enqueue local RPMs for insertion into the cache.
    ///
    /// This is synchronous; the caller must supply files that are already on disk.
    /// For remote stores, download to `self.tempdir` first.
    ///
    /// WARN: any packages not in `paths` are removed.
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
    async fn add_replace(&self, links: &[LinkBuf]) -> Res<AddReplaceOutput> {
        let mut bad_filenames = Vec::new();
        let mut removed: Vec<crate::Kirifuda> = Vec::new();

        let keys = self.cache.keys()?; // keys are filenames
        let parsed = keys
            .into_iter()
            .filter_map(|k| {
                let p = kuchiyose::rpm::parse_filename(&k)?;
                Some((k, p))
            })
            .collect_vec();

        for link in links {
            let kiri = crate::Kiri::from_link(link)?;
            let Some(crate::pkg::ParsePathOutput { name, arch, .. }) =
                kuchiyose::rpm::parse_filename(kiri)
            else {
                bad_filenames.push(link.to_owned());
                continue;
            };
            removed.extend(
                parsed
                    .iter()
                    .filter(|(_, k)| k.name == name && k.arch == arch)
                    .filter(|(k, _)| &**k != kiri)
                    .map(|(k, _)| k.clone()),
            );
        }

        if !removed.is_empty() {
            self.del(&removed).await?;
        }
        self.add(links)?;
        Ok(AddReplaceOutput { bad_filenames, removed })
    }*/

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
            let buf = sig.sign(&repomd)?.to_armored_bytes(pgp::composed::ArmorOptions::default())?;
            writer.write_all(&buf).await?;
            writer.shutdown().await?;
        }
        Ok(repomd)
    }

    /*
    /// Re-scan the store, upsert what has changed, prune what has gone, then generate.
    ///
    /// # Errors
    /// Propagates store, cache, and IO errors.
    ///
    /// # Panics
    /// The function panics if it encounters `..` as a file name.
    async fn regenerate(&self, incremental: bool) -> Res<RegenerateOutput> {
        let stream = self.cache.cfg.hier.iter_rpms(&self.cache.cfg.store).await;
        let mut stream = std::pin::pin!(stream);

        let mut expected_keys: HashSet<crate::Kirifuda> = HashSet::new();
        let mut links_to_add = Vec::new();
        let mut ret = RegenerateOutput::default();

        let txn = self.cache.env.read_txn()?;

        while let Some(rel) = stream.next().await {
            let rel = rel?;
            let Ok(kiri) = crate::Kiri::from_path(rel.as_path())
                .inspect_err(|err| tracing::error!(?err, ?rel, "iter_rpms gave bad filename"))
            else {
                continue;
            };
            expected_keys.insert(kiri.to_owned());

            if incremental && self.cache.epo.get(&txn, kiri.as_bytes())?.is_some() {
                ret.cached += 1;
                continue;
            }
            links_to_add.push(rel);
        }

        if !links_to_add.is_empty() {
            self.add_replace(&links_to_add).await?;
        }

        ret.repomd = self.cache.write_all(&self.comp_cfg).await?;

        if incremental {
            ret.removed = self.cache.prune(&expected_keys)?;
        }

        Ok(ret)
    }
    */

    /// Delete a list of packages by filename.
    ///
    /// Return a list of packages not found in the cache.
    ///
    /// # Errors
    /// Propagates cache and store errors.
    pub async fn del<'a, I, K>(&self, filenames: I) -> Res<Vec<K>>
    where
        I: IntoIterator<Item = K> + Copy,
        K: AsRef<crate::Kiri> + 'a + std::cmp::PartialEq,
    {
        let not_found = self.cache.delete_pkgs(filenames)?;

        for filename in filenames {
            if not_found.contains(&filename) {
                continue;
            }
            let Some(link) = self.cache.cfg.hier.locate_relative(filename.as_ref()) else {
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

#[non_exhaustive]
#[derive(Debug, Default)]
pub struct RegenerateOutput {
    pub skipped: Vec<(PathBuf, rpm::Error)>,
    pub cached: usize,
    pub removed: u64,
    pub repomd: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pkg::parse_filename;
    use std::fs;
    use tempfile::TempDir;

    fn test_rpm_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../random-rpm-examples/terra-release-44-4.noarch.rpm")
    }

    fn make_repo() -> (TempDir, TempDir, Repo) {
        let dir = TempDir::new().unwrap();
        let cache_dir = TempDir::new().unwrap();
        let repodata_dir = dir.path().join("repodata");
        fs::create_dir_all(&repodata_dir).unwrap();
        let cache =
            crate::repodata::RepoCache::new("testrepo", cache_dir.path(), &repodata_dir).unwrap();
        let repo = Repo { dir: dir.path().to_owned(), cache, sig: None, use_appstream: false };
        (dir, cache_dir, repo)
    }

    fn copy_rpm(src: &Path, dst_dir: &Path, new_name: &str) -> PathBuf {
        let dst = dst_dir.join(new_name);
        fs::copy(src, &dst).unwrap();
        dst
    }

    // helper that replicates the dedup filtering inside add_replace without touching
    // the filesystem or rpm parsing, to keep some tests CC=gcc compatible.
    fn compute_removed(keys: &[Vec<u8>], paths: &[&Path]) -> (Vec<Vec<u8>>, Vec<PathBuf>) {
        let parsed_keys = keys.iter().map(|k| (k, parse_filename(k).unwrap())).collect::<Vec<_>>();
        let mut removed = Vec::new();
        let mut bad = Vec::new();
        for p in paths {
            let filename = p.file_name().unwrap().as_bytes();
            let Some(out) = parse_filename(filename) else {
                bad.push(p.to_path_buf());
                continue;
            };
            for (k, pk) in &parsed_keys {
                if pk.name == out.name && pk.arch == out.arch && k.as_slice() != filename {
                    removed.push((*k).clone());
                }
            }
        }
        (removed, bad)
    }

    // ── pure dedup logic (works with CC=gcc) ────────────────────────
    #[test]
    fn dedup_removes_prev_same_name_arch() {
        let keys = vec![b"terra-release-44-4.noarch.rpm".to_vec()];
        let p2 = Path::new("terra-release-44-5.noarch.rpm");
        let (removed, bad) = compute_removed(&keys, &[p2]);
        assert!(bad.is_empty());
        assert_eq!(removed, vec![b"terra-release-44-4.noarch.rpm".to_vec()]);
    }

    #[test]
    fn dedup_keeps_different_arch() {
        let keys = vec![b"myapp-1.0-1.x86_64.rpm".to_vec()];
        let p2 = Path::new("myapp-1.0-1.aarch64.rpm");
        let (removed, _) = compute_removed(&keys, &[p2]);
        assert!(removed.is_empty());
    }

    #[test]
    fn dedup_keeps_different_name() {
        let keys = vec![b"foo-1.0-1.noarch.rpm".to_vec()];
        let p2 = Path::new("bar-1.0-1.noarch.rpm");
        let (removed, _) = compute_removed(&keys, &[p2]);
        assert!(removed.is_empty());
    }

    #[test]
    fn dedup_same_filename_not_removed() {
        let keys = vec![b"dup-1.0-1.noarch.rpm".to_vec()];
        let p = Path::new("dup-1.0-1.noarch.rpm");
        let (removed, _) = compute_removed(&keys, &[p]);
        assert!(removed.is_empty());
    }

    #[test]
    fn dedup_bad_filename_collected() {
        let keys = vec![];
        let bad = Path::new("bad.rpm");
        let (removed, bad_list) = compute_removed(&keys, &[bad]);
        assert!(removed.is_empty());
        assert_eq!(bad_list, vec![bad.to_path_buf()]);
    }

    #[test]
    fn dedup_with_epoch_still_matches_name_arch() {
        let keys = vec![b"pkg-1:1.0-1.noarch.rpm".to_vec()];
        let p = Path::new("pkg-2:1.0-1.noarch.rpm");
        let (removed, _) = compute_removed(&keys, &[p]);
        assert_eq!(removed, vec![b"pkg-1:1.0-1.noarch.rpm".to_vec()]);
    }

    // ── integration tests requiring CC=clang (rpm/zstd) ─────────────
    #[test]
    fn add_replace_removes_prev_same_name_arch() {
        let (dir, _cache_dir, repo) = make_repo();
        let src = test_rpm_path();
        let p1 = copy_rpm(&src, dir.path(), "terra-release-44-4.noarch.rpm");
        let p1_slice = [p1.as_path()];
        repo.add(&p1_slice).unwrap();
        assert_eq!(repo.cache.keys().unwrap().len(), 1);

        let p2 = copy_rpm(&src, dir.path(), "terra-release-44-5.noarch.rpm");
        let p2_slice = [p2.as_path()];
        let out = repo.add_replace(&p2_slice).unwrap();
        assert_eq!(out.bad_filenames.len(), 0);
        assert_eq!(out.removed, vec![b"terra-release-44-4.noarch.rpm".to_vec()]);
        assert_eq!(out.added.len(), 1);
        let keys = repo.cache.keys().unwrap();
        assert_eq!(keys, vec![b"terra-release-44-5.noarch.rpm".to_vec()]);
        assert!(!dir.path().join("terra-release-44-4.noarch.rpm").exists());
    }

    #[test]
    fn add_replace_keeps_different_arch() {
        let (dir, _cache_dir, repo) = make_repo();
        let src = test_rpm_path();
        let p1 = copy_rpm(&src, dir.path(), "myapp-1.0-1.x86_64.rpm");
        repo.add(&[p1.as_path()]).unwrap();
        let p2 = copy_rpm(&src, dir.path(), "myapp-1.0-1.aarch64.rpm");
        let p2_slice = [p2.as_path()];
        let out = repo.add_replace(&p2_slice).unwrap();
        assert!(out.removed.is_empty());
        assert_eq!(repo.cache.keys().unwrap().len(), 2);
    }

    #[test]
    fn add_replace_keeps_different_name() {
        let (dir, _cache_dir, repo) = make_repo();
        let src = test_rpm_path();
        let p1 = copy_rpm(&src, dir.path(), "foo-1.0-1.noarch.rpm");
        repo.add(&[p1.as_path()]).unwrap();
        let p2 = copy_rpm(&src, dir.path(), "bar-1.0-1.noarch.rpm");
        let p2_slice = [p2.as_path()];
        let out = repo.add_replace(&p2_slice).unwrap();
        assert!(out.removed.is_empty());
        assert_eq!(repo.cache.keys().unwrap().len(), 2);
    }

    #[test]
    fn add_replace_same_filename_not_removed() {
        let (dir, _cache_dir, repo) = make_repo();
        let src = test_rpm_path();
        let p1 = copy_rpm(&src, dir.path(), "dup-1.0-1.noarch.rpm");
        repo.add(&[p1.as_path()]).unwrap();
        let p1_slice = [p1.as_path()];
        let out = repo.add_replace(&p1_slice).unwrap();
        assert!(out.removed.is_empty());
    }

    #[test]
    fn add_replace_bad_filename_collected() {
        let (dir, _cache_dir, repo) = make_repo();
        let src = test_rpm_path();
        let bad_path = dir.path().join("bad.rpm");
        fs::copy(&src, &bad_path).unwrap();
        let bad_slice = [bad_path.as_path()];
        let out = repo.add_replace(&bad_slice).unwrap();
        assert_eq!(out.bad_filenames.len(), 1);
        assert_eq!(out.bad_filenames[0] as &Path, bad_path.as_path());
    }

    #[test]
    fn add_replace_removes_multiple_old_versions() {
        let (dir, _cache_dir, repo) = make_repo();
        let src = test_rpm_path();
        let p1 = copy_rpm(&src, dir.path(), "multi-1.0-1.noarch.rpm");
        let p2 = copy_rpm(&src, dir.path(), "multi-1.0-2.noarch.rpm");
        repo.add(&[p1.as_path(), p2.as_path()]).unwrap();
        let p3 = copy_rpm(&src, dir.path(), "multi-1.0-3.noarch.rpm");
        let p3_slice = [p3.as_path()];
        let out = repo.add_replace(&p3_slice).unwrap();
        assert_eq!(out.removed.len(), 2);
        assert!(out.removed.contains(&b"multi-1.0-1.noarch.rpm".to_vec()));
        assert!(out.removed.contains(&b"multi-1.0-2.noarch.rpm".to_vec()));
        assert_eq!(repo.cache.keys().unwrap(), vec![b"multi-1.0-3.noarch.rpm".to_vec()]);
    }

    #[test]
    fn add_replace_mixed_good_and_bad() {
        let (dir, _cache_dir, repo) = make_repo();
        let src = test_rpm_path();
        let p1 = copy_rpm(&src, dir.path(), "alpha-1.0-1.noarch.rpm");
        repo.add(&[p1.as_path()]).unwrap();
        let p2 = copy_rpm(&src, dir.path(), "alpha-1.0-2.noarch.rpm");
        let bad_valid = dir.path().join("bad.rpm");
        fs::copy(&src, &bad_valid).unwrap();
        let mixed = [p2.as_path(), bad_valid.as_path()];
        let out = repo.add_replace(&mixed).unwrap();
        assert_eq!(out.bad_filenames.len(), 1);
        assert_eq!(out.removed, vec![b"alpha-1.0-1.noarch.rpm".to_vec()]);
        let mut keys = repo.cache.keys().unwrap();
        keys.sort();
        assert!(keys.contains(&b"alpha-1.0-2.noarch.rpm".to_vec()));
        assert!(keys.contains(&b"bad.rpm".to_vec()));
    }

    #[test]
    fn del_removes_existing_package() {
        let (dir, _cache_dir, repo) = make_repo();
        let src = test_rpm_path();

        let rpm = copy_rpm(&src, dir.path(), "testpkg-1.0-1.noarch.rpm");
        repo.add(&[rpm.as_path()]).unwrap();

        assert!(rpm.exists());
        assert_eq!(repo.cache.keys().unwrap().len(), 1);

        let id: &[u8] = b"testpkg-1.0-1.noarch.rpm";
        let binding = [id];
        let not_found = repo.del(&binding).unwrap();

        assert!(not_found.is_empty());
        assert!(!rpm.exists());
        assert!(repo.cache.keys().unwrap().is_empty());
    }
    #[test]
    fn datatypes_without_appstream() {
        let (_, _, repo) = make_repo();

        let datatypes = repo.datatypes();

        assert_eq!(datatypes.len(), 3);
        assert!(matches!(datatypes[0], crate::repodata::repomd::DataType::Primary));
        assert!(matches!(datatypes[1], crate::repodata::repomd::DataType::Filelists));
        assert!(matches!(datatypes[2], crate::repodata::repomd::DataType::Other));
    }

    #[test]
    fn datatypes_with_appstream() {
        let (_, _, mut repo) = make_repo();
        repo.use_appstream = true;

        let datatypes = repo.datatypes();

        assert_eq!(datatypes.len(), 4);
        assert!(matches!(datatypes[0], crate::repodata::repomd::DataType::Primary));
        assert!(matches!(datatypes[1], crate::repodata::repomd::DataType::Filelists));
        assert!(matches!(datatypes[2], crate::repodata::repomd::DataType::Other));
        assert!(matches!(datatypes[3], crate::repodata::repomd::DataType::Appstream));
    }
    #[test]
    fn del_returns_missing_package() {
        let (_dir, _cache_dir, repo) = make_repo();

        let id: &[u8] = b"missing-1.0-1.noarch.rpm";
        let ids = [id];

        let not_found = repo.del(&ids).unwrap();

        assert_eq!(not_found, vec![id]);
    }
    #[test]
    fn regenerate_adds_packages_to_cache() {
        let (dir, _cache_dir, repo) = make_repo();
        let src = test_rpm_path();

        let rpm = copy_rpm(&src, dir.path(), "testpkg-1.0-1.noarch.rpm");

        assert!(repo.cache.keys().unwrap().is_empty());

        let out = repo.regenerate(false).unwrap();

        assert_eq!(out.parsed, 1);
        assert_eq!(out.cached, 0);
        assert!(out.removed == 0);

        assert_eq!(repo.cache.keys().unwrap(), vec![b"testpkg-1.0-1.noarch.rpm".to_vec()]);
        assert!(rpm.exists());
    }
    #[test]
    fn regenerate_incremental_uses_cached_package() {
        let (dir, _cache_dir, repo) = make_repo();
        let src = test_rpm_path();

        let rpm = copy_rpm(&src, dir.path(), "testpkg-1.0-1.noarch.rpm");

        repo.regenerate(false).unwrap();

        let out = repo.regenerate(true).unwrap();

        assert_eq!(out.parsed, 0);
        assert_eq!(out.cached, 1);
        assert_eq!(out.removed, 0);
        assert!(out.skipped.is_empty());

        assert_eq!(repo.cache.keys().unwrap(), vec![b"testpkg-1.0-1.noarch.rpm".to_vec()]);
        assert!(rpm.exists());
    }

    #[test]
    fn regenerate_incremental_removes_deleted_package() {
        let (dir, _cache_dir, repo) = make_repo();
        let src = test_rpm_path();

        let rpm = copy_rpm(&src, dir.path(), "testpkg-1.0-1.noarch.rpm");

        repo.regenerate(false).unwrap();
        assert_eq!(repo.cache.keys().unwrap().len(), 1);

        fs::remove_file(&rpm).unwrap();

        let out = repo.regenerate(true).unwrap();

        assert_eq!(out.parsed, 0);
        assert_eq!(out.cached, 0);
        assert_eq!(out.removed, 1);
        assert!(out.skipped.is_empty());

        assert!(repo.cache.keys().unwrap().is_empty());
    }

    #[test]
    fn regenerate_incremental_adds_new_and_keeps_cached() {
        let (dir, _cache_dir, repo) = make_repo();
        let src = test_rpm_path();

        let rpm1 = copy_rpm(&src, dir.path(), "testpkg-1.0-1.noarch.rpm");

        repo.regenerate(false).unwrap();

        let rpm2 = copy_rpm(&src, dir.path(), "otherpkg-1.0-1.noarch.rpm");

        let out = repo.regenerate(true).unwrap();

        assert_eq!(out.parsed, 1);
        assert_eq!(out.cached, 1);
        assert_eq!(out.removed, 0);
        assert!(out.skipped.is_empty());

        let mut keys = repo.cache.keys().unwrap();
        keys.sort();

        assert_eq!(
            keys,
            vec![b"otherpkg-1.0-1.noarch.rpm".to_vec(), b"testpkg-1.0-1.noarch.rpm".to_vec(),]
        );

        assert!(rpm1.exists());
        assert!(rpm2.exists());
    }

    #[test]
    fn regenerate_ignores_non_rpm_files() {
        let (dir, _cache_dir, repo) = make_repo();
        let src = test_rpm_path();

        let rpm = copy_rpm(&src, dir.path(), "valid-1.0-1.noarch.rpm");
        // Non-RPM files that must be ignored by regenerate().
        fs::write(dir.path().join("notes.txt"), b"hello").unwrap();
        fs::write(dir.path().join("archive.zip"), b"PK").unwrap();
        fs::write(dir.path().join("valid-1.0-1.noarch.rpm.bak"), b"backup").unwrap();
        // Also ensure .rpm extension is case-insensitive but .rpm.bak is not counted.
        fs::write(dir.path().join("README"), b"no extension").unwrap();

        let out = repo.regenerate(false).unwrap();

        assert_eq!(out.parsed, 1);
        assert_eq!(out.cached, 0);
        assert_eq!(repo.cache.keys().unwrap(), vec![b"valid-1.0-1.noarch.rpm".to_vec()]);
        assert!(rpm.exists());
        // Non-RPM files still exist on filesystem and were not ingested.
        assert!(dir.path().join("notes.txt").exists());
        assert!(dir.path().join("archive.zip").exists());
        assert!(dir.path().join("valid-1.0-1.noarch.rpm.bak").exists());
    }

    #[test]
    fn del_cache_miss_does_not_delete_filesystem_file() {
        let (dir, _cache_dir, repo) = make_repo();
        let src = test_rpm_path();

        // Create a file on the filesystem WITHOUT adding it to the cache.
        // This simulates an orphan file that is not tracked.
        let orphan_path = dir.path().join("orphan-1.0-1.noarch.rpm");
        fs::copy(&src, &orphan_path).unwrap();
        assert!(orphan_path.exists());
        assert!(repo.cache.keys().unwrap().is_empty());

        let id: &[u8] = b"orphan-1.0-1.noarch.rpm";
        let ids = [id];
        let not_found = repo.del(&ids).unwrap();

        // Cache miss: file must NOT be deleted, and id returned as not_found.
        assert_eq!(not_found, vec![id]);
        assert!(orphan_path.exists(), "filesystem file must survive cache miss");
        assert!(repo.cache.keys().unwrap().is_empty());
    }

    #[test]
    fn del_mixed_found_and_missing_only_deletes_found() {
        let (dir, _cache_dir, repo) = make_repo();
        let src = test_rpm_path();

        let tracked = copy_rpm(&src, dir.path(), "tracked-1.0-1.noarch.rpm");
        repo.add(&[tracked.as_path()]).unwrap();
        assert_eq!(repo.cache.keys().unwrap().len(), 1);

        // Orphan file not in cache.
        let orphan_path = dir.path().join("orphan-1.0-1.noarch.rpm");
        fs::copy(&src, &orphan_path).unwrap();

        let tracked_id: &[u8] = b"tracked-1.0-1.noarch.rpm";
        let orphan_id: &[u8] = b"orphan-1.0-1.noarch.rpm";
        let ids = [tracked_id, orphan_id];
        let not_found = repo.del(&ids).unwrap();

        // Only orphan should be reported as not_found; tracked must be deleted.
        assert_eq!(not_found, vec![orphan_id]);
        assert!(!tracked.exists());
        assert!(orphan_path.exists());
        assert!(repo.cache.keys().unwrap().is_empty());
    }

    #[test]
    fn add_outside_repo_dir_returns_strip_prefix_error() {
        let (dir, _cache_dir, repo) = make_repo();
        let src = test_rpm_path();

        // Create an RPM file outside the repository directory.
        let outside_dir = TempDir::new().unwrap();
        let outside_rpm = copy_rpm(&src, outside_dir.path(), "outside-1.0-1.noarch.rpm");
        assert!(outside_rpm.exists());
        // Sanity: outside path is not under repo.dir
        assert!(!outside_rpm.starts_with(&repo.dir));

        let result = repo.add(&[outside_rpm.as_path()]);

        assert!(result.is_err(), "adding RPM outside repo dir must fail");
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("should be in") || err_msg.contains("strip_prefix"),
            "error should mention strip_prefix/should be in, got: {err_msg}"
        );
        // Repo cache unchanged, and original outside file untouched.
        assert!(repo.cache.keys().unwrap().is_empty());
        assert!(outside_rpm.exists());
        assert!(!dir.path().join("outside-1.0-1.noarch.rpm").exists());
    }
}

/*
#[non_exhaustive]
#[derive(Clone, Debug, Default)]
struct AddReplaceOutput {
    pub bad_filenames: Vec<LinkBuf>,
    pub removed: Vec<crate::Kirifuda>,
}*/
