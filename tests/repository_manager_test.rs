extern crate git2;
extern crate tempdir;

extern crate metal_git;

mod util;

use metal_git::repository_manager;
use crate::util::test_repo::TestRepo;

#[test]
pub fn open() {
	let test_repo = TestRepo::flat_two();
	
	let r = repository_manager::RepositoryManager::new();
    r.set_work_dir_path(test_repo.path().to_str().unwrap());
    
	let _ = r.open();
	// expect not to panic
}
