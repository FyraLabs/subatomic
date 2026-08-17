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
        if self.cache.del_comps()?.is_none() {
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
            let sig = sig.sign_rpm(&rpmmeta.metadata)?;
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
        let mut frag = FragEph::new(&pkg, path.as_os_str());
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
