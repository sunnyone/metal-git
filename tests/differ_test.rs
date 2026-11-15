extern crate git2;
extern crate metal_git;
extern crate tempdir;

use git2::{Oid, Repository, Signature};
use metal_git::diff::{DiffTarget, Differ, FileDiff, FileStatus, LineKind};
use metal_git::repository_manager::RepositoryManager;
use std::fs;
use std::path::Path;
use tempdir::TempDir;

/// A repository in a temporary directory, with a differ that reads it.
struct Fixture {
	dir: TempDir,
	repo: Repository,
	differ: Differ,
}

impl Fixture {
	fn new() -> Fixture {
		let dir = TempDir::new("differ").unwrap();
		let repo = Repository::init(dir.path()).unwrap();
		let repository_manager = RepositoryManager::new();
		repository_manager.set_work_dir_path(dir.path().to_str().unwrap());
		let differ = repository_manager.differ();
		Fixture { dir, repo, differ }
	}

	fn write(&self, name: &str, content: &str) {
		fs::write(self.dir.path().join(name), content).unwrap();
	}

	fn stage(&self, name: &str) {
		let mut index = self.repo.index().unwrap();
		index.add_path(Path::new(name)).unwrap();
		index.write().unwrap();
	}

	/// Writes and stages files, and commits them on HEAD.
	fn commit(&self, files: &[(&str, &str)]) -> Oid {
		for (name, content) in files {
			self.write(name, content);
			self.stage(name);
		}
		let mut index = self.repo.index().unwrap();
		let tree = self.repo.find_tree(index.write_tree().unwrap()).unwrap();
		let signature = Signature::now("test", "test@example.com").unwrap();
		let parent = self.repo.head().ok().and_then(|head| head.peel_to_commit().ok());
		let parents: Vec<&git2::Commit> = parent.iter().collect();
		self.repo
			.commit(Some("HEAD"), &signature, &signature, "commit", &tree, &parents)
			.unwrap()
	}

	fn files(&self, target: DiffTarget) -> Vec<FileDiff> {
		self.differ.files(&target).unwrap()
	}
}

fn paths_and_statuses(files: &[FileDiff]) -> Vec<(String, FileStatus)> {
	files.iter().map(|f| (f.display_path(), f.status)).collect()
}

fn changed_lines(file: &FileDiff) -> Vec<(LineKind, String)> {
	file.hunks
		.iter()
		.flat_map(|h| &h.lines)
		.filter(|l| l.kind != LineKind::Context)
		.map(|l| (l.kind, l.content.clone()))
		.collect()
}

#[test]
pub fn a_commit_is_diffed_against_its_parent_in_the_order_of_its_file_list() {
	let fixture = Fixture::new();
	fixture.commit(&[("a.txt", "one\ntwo\n")]);
	let oid = fixture.commit(&[("a.txt", "one\nTWO\n"), ("b.txt", "new\n")]);

	let files = fixture.files(DiffTarget::Commit(oid));
	let list = fixture.differ.commit_file_list(oid).unwrap();

	assert_eq!(
		vec![("a.txt".to_string(), FileStatus::Modified), ("b.txt".to_string(), FileStatus::Added)],
		paths_and_statuses(&files)
	);
	let list_paths: Vec<Option<String>> = list.files.iter().map(|f| f.new_file_path.clone()).collect();
	assert_eq!(vec![Some("a.txt".to_string()), Some("b.txt".to_string())], list_paths);
}

#[test]
pub fn a_work_tree_file_is_diffed_against_the_index_alone() {
	let fixture = Fixture::new();
	fixture.commit(&[("a.txt", "one\n"), ("b.txt", "kept\n")]);
	fixture.write("a.txt", "one\nadded\n");
	fixture.write("b.txt", "changed too\n");

	let files = fixture.files(DiffTarget::WorkTree("a.txt".to_string()));

	assert_eq!(1, files.len());
	assert_eq!(vec![(LineKind::Added, "added".to_string())], changed_lines(&files[0]));
}

#[test]
pub fn an_untracked_work_tree_file_is_all_added() {
	let fixture = Fixture::new();
	fixture.commit(&[("a.txt", "one\n")]);
	fixture.write("new.txt", "first\nsecond\n");

	let files = fixture.files(DiffTarget::WorkTree("new.txt".to_string()));

	assert_eq!(vec![("new.txt".to_string(), FileStatus::Added)], paths_and_statuses(&files));
	assert_eq!(
		vec![(LineKind::Added, "first".to_string()), (LineKind::Added, "second".to_string())],
		changed_lines(&files[0])
	);
}

#[test]
pub fn an_index_file_is_diffed_against_head() {
	let fixture = Fixture::new();
	fixture.commit(&[("a.txt", "one\n")]);
	fixture.write("a.txt", "one\nstaged\n");
	fixture.stage("a.txt");
	fixture.write("a.txt", "one\nstaged\nnot staged\n");

	let files = fixture.files(DiffTarget::Index("a.txt".to_string()));

	assert_eq!(vec![(LineKind::Added, "staged".to_string())], changed_lines(&files[0]));
}

#[test]
pub fn the_old_version_of_a_file_is_read_for_its_unchanged_lines() {
	let fixture = Fixture::new();
	fixture.commit(&[("a.txt", "one\ntwo\nthree\n")]);
	let oid = fixture.commit(&[("a.txt", "one\nTWO\nthree\n")]);

	let files = fixture.files(DiffTarget::Commit(oid));

	assert_eq!(
		vec!["one".to_string(), "two".to_string(), "three".to_string()],
		fixture.differ.old_content(&files[0]).unwrap()
	);
}
