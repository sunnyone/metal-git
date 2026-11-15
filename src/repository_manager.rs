use std::rc::Rc;
use git2::{Repository, Error};
use std::cell::RefCell;

use crate::diff::Differ;

pub struct RepositoryManager {
    work_dir_path: RefCell<String>,
}

impl RepositoryManager {
    pub fn new() -> Rc<RepositoryManager> {
        Rc::new(RepositoryManager { work_dir_path: RefCell::new("".to_string()) })
    }

    pub fn set_work_dir_path(&self, work_dir_path: &str) {
        *self.work_dir_path.borrow_mut() = work_dir_path.to_string();
    }

    /// Takes the diffs of this repository.
    pub fn differ(self: &Rc<Self>) -> Differ {
        Differ::new(self.clone())
    }

    pub fn open(&self) -> Result<Repository, Error> {
        // TODO: check the path is set
        git2::Repository::discover(self.work_dir_path.borrow().as_str())
    }
}
