#[derive(Debug, Clone)]
pub struct Repo {
    pub id: String,
    pub cache: crate::repodata::RepoCache,
    pub sig: Option<crate::sig::Mgr>,
}

impl Repo {
    pub fn add_comps(&self) {
        // self.cache.
        todo!()
    }
}
