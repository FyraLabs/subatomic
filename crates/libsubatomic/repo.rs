use std::collections::HashSet;

use crate::prelude::*;

use sha2::Digest;

#[derive(Clone, Debug)]
pub struct Repo {
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
    pub fn add_comps(&self, comps: &[u8]) -> Res<()> {
        let fd = std::fs::File::create(self.cache.dir.join("repodata/comps.xml.zst"))?;
        let mut rw = crate::repodata::RepoWriter {
            comp: crate::repodata::RepoWriterComp::Zstd(zstd::Encoder::new(
                crate::repodata::RepoWriterCompInner {
                    fd,
                    csum: crate::repodata::RepoWriterCsum::Sha256(sha2::Sha256::new()),
                    ..
                },
                0,
            )?),
            osum: crate::repodata::RepoWriterCsum::Sha256(sha2::Sha256::new()),
            ..
        };
        rw.write_all(comps)?;
        let (data, _) = rw.into_data(crate::repodata::repomd::DataType::Group)?;
        self.cache.write_comps(&data)?;
        std::fs::rename(
            self.cache.dir.join("repodata/comps.xml.zst"),
            self.cache.dir.join(format!("{}-comps.xml.zst", data.checksum.sha)),
        )?;
        Ok(())
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
    pub fn del_comps(&self) -> Res<bool> {
        const FILENAME_MATCH: &[u8] = b"-comps.xml";
        if !self.cache.del_comps()? {
            return Ok(false);
        }
        if let Some(f) = std::fs::read_dir(self.cache.dir.join("repodata"))?
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

    #[tracing::instrument]
    fn add_one<'a>(
        &self,
        path: &&'a Path,
    ) -> Result<(AddPkgOutput, (crate::pkg::Package, &'a OsStr)), rpm::Error> {
        let mut ret = AddPkgOutput::default();
        let (pkg, rpmmeta) = crate::pkg::Package::open(path)?;
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
        let name = path.file_name().expect("rpm no filename");
        Ok((ret, (pkg, name)))
    }

    /// Upsert packages to the cache.
    ///
    /// `paths` should be a list of fs paths to the rpm packages in the correct directory. This
    /// function will not move or modify any of the package files.
    ///
    /// The output [`AddPkgOutput`] may include signatures signed by [`Self::sig`].
    /// These signatures are applied to the rpm files, in place if possible. See
    /// [`rpm::Package::apply_signature_in_place`].
    ///
    /// # Panics
    /// Invalid filename (path terminates in `/..`) will cause a panic. See [`Path::file_name`].
    #[tracing::instrument]
    pub fn add<'a, 'b, 'c>(&'a self, paths: &'b [&'c Path]) -> Res<Vec<AddPkgOutput>> {
        let pkgs = paths
            .par_iter()
            .map(|rpm_path| self.add_one(rpm_path))
            .collect::<Result<Vec<_>, rpm::Error>>()?;
        let (rets, pkgs): (Vec<_>, Vec<_>) = pkgs.into_iter().unzip();
        self.cache.insert_pkgs(pkgs.iter().map(|(pkg, name)| (pkg, *name)))?;
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
            let prev_versions =
                parsed_keys.iter().filter(|(_, k)| k.name == name && k.arch == arch);
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
            let mut asc_fd = std::fs::File::create(self.cache.dir.join("repomd.xml.asc"))?;
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
        let rpm_paths: Vec<PathBuf> = std::fs::read_dir(&self.cache.dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("rpm")))
            .collect();
        let mut expected_keys: HashSet<Vec<u8>> = HashSet::with_capacity(rpm_paths.len());
        let mut ret = RegenerateOutput { .. };
        for path in rpm_paths {
            let key = path.file_name().expect("bad filename").as_bytes();
            expected_keys.insert(key.to_owned());

            if incremental && self.cache.has(key)? {
                ret.cached += 1;
                continue;
            }

            match crate::pkg::Package::open(&path) {
                Ok((pkg, _)) => {
                    self.cache.insert(key, &pkg, path.as_os_str())?;
                    ret.parsed += 1;
                }
                Err(e) => {
                    ret.skipped.push((path, e));
                }
            }
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
        let Self { cache, sig, use_appstream } = self;
        Ok(Self { cache: cache.compact()?, sig, use_appstream })
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
    pub fn del<'a>(&self, ids: &'a [&'a [u8]]) -> Res<Vec<&'a &'a [u8]>> {
        let not_found = self.cache.delete_pkgs(ids)?;
        ids.iter()
            .filter(|f| !not_found.contains(f))
            .par_bridge()
            .try_for_each(|&p| std::fs::remove_file(self.cache.dir.join(OsStr::from_bytes(p))))?;
        Ok(not_found)
    }
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
