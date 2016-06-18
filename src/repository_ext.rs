
extern crate git2;

use std::path::{Path, PathBuf};

pub trait RepositoryExt {
    fn get_full_path(&self, path_in_repository: &Path) -> Option<PathBuf>;
}

impl RepositoryExt for git2::Repository {
    fn get_full_path(&self, path_in_repository: &Path) -> Option<PathBuf> {
        self.workdir().map(|x| x.join(path_in_repository))
    }
}
