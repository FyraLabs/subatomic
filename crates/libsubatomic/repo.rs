use crate::prelude::*;

use sha2::Digest;

#[derive(Debug)]
pub struct Repo {
    pub id: String,
    pub cache: crate::repodata::RepoCache,
    pub dir: std::path::PathBuf,
    pub sig: Option<crate::sig::Mgr>,
}

impl Repo {
    /// Upsert comps file.
    ///
    /// Create `repodata/*-comps.xml.zst`, and add the comps to cache (separate from the fragments).
    ///
    /// # Errors
    /// IO and [`heed`] errors are propagated.
    pub fn add_comps(&self, comps: &[u8]) -> Res<()> {
        let fd = std::fs::File::create(self.dir.join("repodata/comps.xml.zst"))?;
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
            self.dir.join("repodata/comps.xml.zst"),
            self.dir.join(format!("{}-comps.xml.zst", data.checksum.sha)),
        )?;
        Ok(())
    }

    /// Delete comps file.
    ///
    /// If there is no comps in cache, do nothing, even if it exists in the filesystem.
    /// Otherwise, perform a linear search with [`std::fs::read_dir`] in `repodata/`, and delete the
    /// first file that contains `-comps.xml` in the name.
    ///
    /// # Errors
    /// `Ok(())` will be returned if there is no comps in cache; otherwise, `Ok(())` will be
    /// returned if no comps file is found. Returns [`heed`] errors and IO errors if the file is
    /// found but cannot be deleted.
    pub fn del_comps(&self) -> Res<()> {
        const FILENAME_MATCH: &[u8] = b"-comps.xml";
        if !self.cache.del_comps()? {
            return Ok(());
        }
        for f in std::fs::read_dir(self.dir.join("repodata"))? {
            let f = f?;
            if f.file_name()
                .as_encoded_bytes()
                .windows(FILENAME_MATCH.len())
                .contains(FILENAME_MATCH)
            {
                std::fs::remove_file(f.path())?;
                break;
            }
        }
        Ok(())
    }

    #[tracing::instrument]
    fn add_one<'a>(
        &self,
        path: &&'a Path,
    ) -> Result<(AddPkgOutput, crate::pkg::Package, &'a OsStr), rpm::Error> {
        let mut ret = AddPkgOutput::default();
        let (pkg, rpmmeta) = crate::pkg::Package::open(path)?;
        if let Some(sig) = &self.sig {
            tracing::debug!("signing");
            let sig = sig.sign_rpm(&rpmmeta)?;
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
        Ok((ret, pkg, name))
    }

    /// Add packages to the cache.
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
    pub fn add(&self, paths: &[&Path]) -> Res<impl Iterator<Item = AddPkgOutput>> {
        let pkgs = paths
            .par_iter()
            .map(|rpm_path| self.add_one(rpm_path))
            .collect::<Result<Vec<_>, rpm::Error>>()?;
        self.cache.insert_pkgs(pkgs.iter().map(|(_, pkg, name)| (pkg, *name)))?;
        Ok(pkgs.into_iter().map(|(ret, _, _)| ret))
    }

    /// Trigger repository generation. Generate all XML files in `repodata/`.
    ///
    /// This is analogous to running the `createrepo` command. The repository metadata is generated
    /// according to [`Self::cache`]. If [`Self::sig`] is [`Some`], also generate `repomd.xml.asc`.
    ///
    /// # Errors
    /// IO errors and possibly [`pgp`] errors.
    #[doc(alias = "createrepo")]
    pub fn generate(&self) -> Res<()> {
        let repomd = self.cache.write_all(&self.dir)?; // for now use self.dir as tempdir
        if let Some(sig) = &self.sig {
            let mut asc_fd = std::fs::File::create(self.dir.join("repomd.xml.asc"))?;
            sig.sign(&repomd)?
                .to_armored_writer(&mut asc_fd, pgp::composed::ArmorOptions::default())?;
        }
        Ok(())
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
    pub fn del<'a>(&self, ids: &'a [&'a str]) -> Res<Vec<&'a &'a str>> {
        let not_found = self.cache.delete_pkgs(ids)?;
        ids.iter()
            .filter(|f| !not_found.contains(f))
            .par_bridge()
            .try_for_each(|p| std::fs::remove_file(self.dir.join(p)))?;
        Ok(not_found)
    }
}

#[derive(Clone, Debug, Default)]
pub struct AddPkgOutput {
    pub sig: Option<Vec<u8>>,
}
