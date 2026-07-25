use crate::prelude::*;

use std::io::Write;

use sha2::Digest;

#[derive(Debug)]
pub struct Repo {
    pub id: String,
    pub cache: crate::repodata::RepoCache,
    pub dir: std::path::PathBuf,
    pub sig: Option<crate::sig::Mgr>,
}

impl Repo {
    pub fn add_comps(&self, comps: &[u8]) -> std::io::Result<()> {
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
        self.cache.write_comps(&data).expect("can't write comps to cache");
        std::fs::rename(
            self.dir.join("repodata/comps.xml.zst"),
            self.dir.join(format!("{}-comps.xml.zst", data.checksum.sha)),
        )?;
        Ok(())
    }
    pub fn del_comps(&self) -> std::io::Result<()> {
        const FILENAME_MATCH: &[u8] = b"-comps.xml";
        if !self.cache.del_comps().expect("heed failed") {
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
    pub fn add(&self, paths: &[&Path]) -> Result<impl Iterator<Item = AddPkgOutput>, rpm::Error> {
        let pkgs = paths
            .par_iter()
            .map(|rpm_path| {
                let mut ret = AddPkgOutput::default();
                let (pkg, rpmmeta) = crate::pkg::Package::open(rpm_path)?;
                if let Some(sig) = &self.sig {
                    ret.sig = Some(sig.sign_rpm(&rpmmeta)?);
                }
                let name = rpm_path.file_name().expect("rpm no filename");
                Ok((ret, pkg, name))
            })
            .collect::<Result<Vec<_>, rpm::Error>>()?;
        self.cache.insert_pkgs(pkgs.iter().map(|(_, pkg, name)| (pkg, *name))).expect("heed err");
        Ok(pkgs.into_iter().map(|(ret, _, _)| ret))
    }
    #[doc(alias = "createrepo")]
    pub fn generate(&self) -> std::io::Result<()> {
        let repomd = self.cache.write_all(&self.dir)?; // for now use self.dir as tempdir
        if let Some(sig) = &self.sig {
            let mut asc_fd = std::fs::File::create(self.dir.join("repomd.xml.asc"))?;
            sig.sign(&repomd)
                .expect("")
                .to_armored_writer(&mut asc_fd, pgp::composed::ArmorOptions::default())
                .expect("");
        }
        Ok(())
    }
    /// Delete a list of packages by their filenames.
    pub fn del<'a>(&self, ids: &'a [&'a str]) -> std::io::Result<Vec<&'a &'a str>> {
        let not_found = self.cache.delete_pkgs(ids).expect("");
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
