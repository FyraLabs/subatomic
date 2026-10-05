// NOTE: what features should we have in libsubatomic? Maybe this belongs to subatomic (server)?
use std::collections::HashSet;

use crate::{
    prelude::*,
    repodata::{Frag, FragEph},
};

#[derive(Debug)]
pub struct Repo {
    pub dir: PathBuf,
    pub cache: crate::repodata::RepoCache,
    pub sig: Option<crate::sig::Mgr>,
    pub use_appstream: bool = false,
}

impl Repo {
    /// Upsert comps file.
    ///
    /// Create `repodata/*-comps.xml.zst`, and add the comps to cache (separate from the fragments).
    ///
    /// # Errors
    /// IO and [`heed`] errors are propagated.
    #[deprecated = "use self.cache.update_custom_datatype()"]
    pub fn add_comps(&self, comps: &[u8]) -> Res<()> {
        self.cache.update_custom_datatype(
            crate::repodata::repomd::DataType::Custom("group".into(), "comps.xml".into()),
            comps,
        )
    }

    /// Delete comps file.
    ///
    /// If there is no comps in cache, do nothing, even if it exists in the filesystem.
    /// Otherwise, perform a linear search with [`std::fs::read_dir`] in `repodata/`, and delete the
    /// first file that contains `-comps.xml` in the name.
    ///
    /// Return whether the comps file was deleted. In other words, return false if there was no comps
    /// in the cache.
    ///
    /// # Errors
    /// Returns [`heed`] errors and IO errors if the file was found but could not be deleted.
    /// However, `Ok(true)` is returned if comps was in cache but the file does not exist.
    #[deprecated = "use self.cache.del_custom_datatype()"]
    pub fn del_comps(&self) -> Res<bool> {
        const FILENAME_MATCH: &[u8] = b"-comps.xml";
        if self.cache.del_custom_datatype("group")?.is_none() {
            return Ok(false);
        }
        if let Some(f) = std::fs::read_dir(&self.cache.repodata_dir)?
            .filter_ok(|f| {
                f.file_name().as_bytes().windows(FILENAME_MATCH.len()).contains(FILENAME_MATCH)
            })
            .next()
        {
            std::fs::remove_file(f?.path())?;
        } else {
            tracing::warn!("no comps file found but comps was in cache");
        }
        Ok(true)
    }

    fn add_one(&self, path: &&Path) -> Result<(AddPkgOutput, (Vec<u8>, FragEph)), rpm::Error> {
        let path_relative = path.strip_prefix(&self.dir).map_err(|e| {
            rpm::Error::Io(std::io::Error::other(format!(
                "{} should be in {}; cannot strip_prefix: {e}",
                path.display(),
                self.dir.display()
            )))
        })?;
        let mut ret = AddPkgOutput::default();
        let (pkg, mut rpmmeta) = crate::pkg::Package::open(path)?;
        if let Some(sig) = &self.sig {
            tracing::debug!("signing");
            // TODO: write back
            let sig = sig.sign_rpm(&mut rpmmeta.metadata)?;
            if let Err(e) = rpm::Package::apply_signature_in_place(path, sig.clone()) {
                let rpm::Error::InsufficientReservedSpace { .. } = e else {
                    return Err(e);
                };
                tracing::debug!("cannot apply signature in place, opening full file");
                let mut p = rpm::Package::open(path)?;
                p.apply_signature(sig.clone())?;
                p.write_file(path)?;
            }
            ret.sig = Some(sig);
        }
        let mut frag = FragEph::new(&pkg, &path_relative);
        if self.use_appstream {
            frag.app = Frag(Some(crate::pkg::Package::appstream_frag(&mut rpmmeta)?));
        }
        // We need the key (filename) and the fragment.
        Ok((ret, (path_relative.as_os_str().as_encoded_bytes().to_owned(), frag)))
    }

    pub fn add(&self, paths: &[&Path]) -> Res<Vec<AddPkgOutput>> {
        // TODO: use update_frags()
        let items: Vec<_> = paths
            .par_iter()
            .map(|rpm_path| self.add_one(rpm_path))
            .collect::<Result<Vec<_>, rpm::Error>>()?;
        let (rets, frags_with_keys): (Vec<_>, Vec<_>) = items.into_iter().unzip();
        self.cache.insert_fragments(frags_with_keys)?;
        Ok(rets)
    }

    /// Upsert packages and remove their old versions.
    ///
    /// See [`Self::add`] for more info.
    ///
    /// # Panics
    /// Panics if any cache keys are invalid (cannot be parsed by [`crate::pkg::parse_filename`]),
    /// or a file with the name `..` is encountered.
    #[tracing::instrument]
    pub fn add_replace<'a, 'b, 'c>(
        &'a self,
        paths: &'b [&'c Path],
    ) -> Res<AddReplaceOutput<'b, 'c>> {
        // TODO: use update_frags()
        let mut bad_filenames: Vec<&'b &'c Path> = Vec::new();
        let mut removed = Vec::new();
        let keys = self.cache.keys()?;
        let parsed_keys = keys
            .iter()
            .map(|k| (k, crate::pkg::parse_filename(k).expect("can't parse cache keys")))
            .collect_vec();
        for path in paths {
            let filename = path.file_name().expect("bad filename").as_bytes();
            let Some(crate::pkg::ParsePathOutput { name, arch, .. }) =
                crate::pkg::parse_filename(filename)
            else {
                bad_filenames.push(path);
                continue;
            };
            let prev_versions = (parsed_keys.iter())
                .filter(|(_, k)| k.name == name && k.arch == arch)
                .filter(|(k, _)| *k != filename);
            removed.extend(prev_versions.map(|(k, _)| (*k).clone()));
        }
        let to_remove = removed.iter().map(|k| &**k).collect_vec();
        let not_found = self.del(&to_remove)?;
        debug_assert!(not_found.is_empty());
        let added = self.add(paths)?;
        Ok(AddReplaceOutput { bad_filenames, removed, added })
    }

    /// A list of datatypes representing what XML files should be generated in `repodata/`.
    pub fn datatypes(&self) -> Vec<crate::repodata::repomd::DataType> {
        use crate::repodata::repomd::DataType;
        let mut dts = vec![DataType::Primary, DataType::Filelists, DataType::Other];
        dts.extend(self.use_appstream.then_some(DataType::Appstream));
        dts
    }

    /// Trigger repository generation. Generate all XML files in `repodata/`.
    ///
    /// This is analogous to running the `createrepo` command. The repository metadata is generated
    /// according to [`Self::cache`]. If [`Self::sig`] is [`Some`], also generate `repomd.xml.asc`.
    ///
    /// # Errors
    /// IO errors and possibly [`pgp`] errors.
    #[doc(alias = "createrepo")]
    pub fn generate(&self) -> Res<Vec<u8>> {
        let repomd = self.cache.write_all(&self.datatypes())?; // for now use self.cache.dir as tempdir
        if let Some(sig) = &self.sig {
            let mut asc_fd = std::fs::File::create(self.cache.repodata_dir.join("repomd.xml.asc"))?;
            sig.sign(&repomd)?
                .to_armored_writer(&mut asc_fd, pgp::composed::ArmorOptions::default())?;
        }
        Ok(repomd)
    }

    /// Invalidate the cache and regenerate XML files in `repodata/`.
    ///
    /// In most cases you should try to use [`Self::generate`] instead. This operation is way more
    /// expensive than the usual generate method which reads from the cache. However, this operation
    /// should still be way faster than `createrepo_c`.
    ///
    /// Upsert all `.rpm` files in [`Self::dir`] into the cache in parallel, then remove ones that
    /// do not exist, then run [`Self::generate`].
    ///
    /// # Panics
    /// The function panics if it encounters `..` as a file name.
    pub fn regenerate(&self, incremental: bool) -> Res<RegenerateOutput> {
        let rpm_paths: Vec<PathBuf> = std::fs::read_dir(&self.dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("rpm")))
            .collect();

        let mut expected_keys = HashSet::with_capacity(rpm_paths.len());
        let mut paths_to_add = Vec::new();
        let mut ret = RegenerateOutput::default();

        for path in &rpm_paths {
            let key = path.file_name().expect("bad filename").as_bytes();
            expected_keys.insert(key.to_owned());

            if incremental && self.cache.has(key)? {
                ret.cached += 1;
                continue;
            }
            paths_to_add.push(path.as_path());
        }

        if !paths_to_add.is_empty() {
            let results = self.add_replace(&paths_to_add)?;
            // TODO: save the result or something
            ret.parsed = results.added.len();
        }

        ret.repomd = self.cache.write_all(&self.datatypes())?;

        if incremental {
            let expected_refs: HashSet<_> = expected_keys.iter().map(|k| &**k).collect();
            ret.removed = self.cache.prune(&expected_refs)?;
        }

        Ok(ret)
    }

    /// Compacts the cache using [`crate::repodata::RepoCache::compact`].
    ///
    /// # Errors
    /// Errors are propagated.
    pub fn compact_cache(self) -> Res<Self> {
        let Self { dir, cache, sig, use_appstream } = self;
        Ok(Self { dir, cache: cache.compact()?, sig, use_appstream })
    }

    /// Delete a list of packages by their filenames.
    ///
    /// Package files (`.rpm`) and cache records are deleted.
    ///
    /// Return a list of filenames not found in the cache. They are not removed even if they exist
    /// in the filesystem.
    ///
    /// # Errors
    /// Propagate [`heed`] and IO errors.
    pub fn del<'a>(&self, ids: &'a [&'a [u8]]) -> Res<Vec<&'a [u8]>> {
        let not_found = self.cache.delete_pkgs(ids)?;
        ids.iter()
            .filter(|f| !not_found.contains(f))
            .par_bridge()
            .try_for_each(|&p| std::fs::remove_file(self.dir.join(OsStr::from_bytes(p))))?;
        Ok(not_found)
    }

    // /// Resign all packages.
    // ///
    // /// # Errors
    // /// Propagate [`pgp`] signing and IO errors.
    // pub fn resign_all(&self) -> Res<()> {
    //     todo!()
    // }
}

#[derive(Clone, Debug, Default)]
pub struct AddPkgOutput {
    pub sig: Option<Vec<u8>>,
}
#[derive(Debug, Default)]
pub struct RegenerateOutput {
    pub parsed: usize = 0,
    pub skipped: Vec<(PathBuf, rpm::Error)> = Vec::new(),
    pub cached: usize = 0,
    pub removed: u64 = 0,
    pub repomd: Vec<u8> = Vec::new(),
}

#[derive(Clone, Debug)]
pub struct AddReplaceOutput<'a, 'b> {
    pub bad_filenames: Vec<&'a &'b Path>,
    pub removed: Vec<Vec<u8>>,
    pub added: Vec<AddPkgOutput>,
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
