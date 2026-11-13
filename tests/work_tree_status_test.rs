extern crate git2;
extern crate tempdir;

extern crate metal_git;

mod util;

use crate::util::test_repo::TestRepo;
use git2::Repository;
use metal_git::work_tree_status::{self, ChangeCounts};
use std::fs;
use std::path::Path;

fn counts(repo: &Repository) -> ChangeCounts {
	work_tree_status::count_changes(repo).unwrap()
}

fn stage(repo: &Repository, name: &str) {
	let mut index = repo.index().unwrap();
	index.add_path(Path::new(name)).unwrap();
	index.write().unwrap();
}

#[test]
pub fn clean_work_tree_has_no_changes() {
	let test_repo = TestRepo::single();
	let repo = Repository::open(test_repo.path()).unwrap();

	assert_eq!(ChangeCounts { changed_files: 0, staged_files: 0 }, counts(&repo));
}

#[test]
pub fn counts_untracked_modified_and_staged_files() {
	let test_repo = TestRepo::empty();
	let repo = Repository::open(test_repo.path()).unwrap();
	let workdir = repo.workdir().unwrap().to_path_buf();

	fs::write(workdir.join("a.txt"), "a\n").unwrap();
	fs::create_dir(workdir.join("dir")).unwrap();
	fs::write(workdir.join("dir/b.txt"), "b\n").unwrap();
	fs::write(workdir.join("dir/c.txt"), "c\n").unwrap();
	// Files in an untracked directory are counted one by one
	assert_eq!(ChangeCounts { changed_files: 3, staged_files: 0 }, counts(&repo));

	stage(&repo, "a.txt");
	assert_eq!(ChangeCounts { changed_files: 3, staged_files: 1 }, counts(&repo));

	// A staged file changed again counts once
	fs::write(workdir.join("a.txt"), "a2\n").unwrap();
	assert_eq!(ChangeCounts { changed_files: 3, staged_files: 1 }, counts(&repo));
}
