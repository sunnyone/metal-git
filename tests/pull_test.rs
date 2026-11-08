extern crate git2;
extern crate tempdir;

extern crate metal_git;

use git2::{Oid, Repository, Signature};
use metal_git::remote_operation::{self, PullSummary};
use std::fs;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use tempdir::TempDir;

struct PullRepos {
	_origin_dir: TempDir,
	_local_dir: TempDir,
	origin: Repository,
	local: Repository,
}

fn commit_file(repo: &Repository, name: &str, content: &str) -> Oid {
	fs::write(repo.workdir().unwrap().join(name), content).unwrap();
	let mut index = repo.index().unwrap();
	index.add_path(Path::new(name)).unwrap();
	index.write().unwrap();
	let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
	let signature = Signature::now("test", "test@example.com").unwrap();
	let parent = repo.head().ok().map(|head| head.peel_to_commit().unwrap());
	let parents: Vec<_> = parent.iter().collect();
	repo.commit(Some("HEAD"), &signature, &signature, name, &tree, &parents).unwrap()
}

fn pull_repos() -> PullRepos {
	let origin_dir = TempDir::new("pull-origin").unwrap();
	let origin = Repository::init(origin_dir.path()).unwrap();
	origin.set_head("refs/heads/master").unwrap();
	commit_file(&origin, "a.txt", "1\n");

	let local_dir = TempDir::new("pull-local").unwrap();
	let local = Repository::clone(origin_dir.path().to_str().unwrap(), local_dir.path()).unwrap();

	PullRepos { _origin_dir: origin_dir, _local_dir: local_dir, origin, local }
}

fn pull(repo: &Repository) -> Result<PullSummary, git2::Error> {
	remote_operation::pull_fast_forward(repo, &AtomicBool::new(false), |_| {})
}

fn head(repo: &Repository) -> Oid {
	repo.head().unwrap().target().unwrap()
}

fn read(repo: &Repository, name: &str) -> String {
	fs::read_to_string(repo.workdir().unwrap().join(name)).unwrap()
}

#[test]
pub fn pull_fast_forwards_to_upstream() {
	let repos = pull_repos();
	commit_file(&repos.origin, "a.txt", "2\n");
	let tip = commit_file(&repos.origin, "b.txt", "new\n");

	let summary = pull(&repos.local).unwrap();

	assert_eq!(2, summary.fast_forwarded_commits);
	assert_eq!("master", summary.branch_name);
	assert_eq!("origin/master", summary.upstream_name);
	assert_eq!(tip, head(&repos.local));
	assert_eq!("2\n", read(&repos.local, "a.txt"));
	assert_eq!("new\n", read(&repos.local, "b.txt"));
	assert!(repos.local.statuses(None).unwrap().is_empty());
}

#[test]
pub fn pull_when_up_to_date_changes_nothing() {
	let repos = pull_repos();
	let before = head(&repos.local);

	let summary = pull(&repos.local).unwrap();

	assert_eq!(0, summary.fast_forwarded_commits);
	assert_eq!(before, head(&repos.local));
}

#[test]
pub fn pull_when_local_is_ahead_changes_nothing() {
	let repos = pull_repos();
	let local_tip = commit_file(&repos.local, "local.txt", "local\n");

	let summary = pull(&repos.local).unwrap();

	assert_eq!(0, summary.fast_forwarded_commits);
	assert_eq!(local_tip, head(&repos.local));
}

#[test]
pub fn pull_rejects_diverged_branches() {
	let repos = pull_repos();
	commit_file(&repos.origin, "a.txt", "remote\n");
	let local_tip = commit_file(&repos.local, "local.txt", "local\n");

	let err = pull(&repos.local).unwrap_err();

	assert!(err.message().contains("diverged"), "{}", err.message());
	assert_eq!(local_tip, head(&repos.local));
}

#[test]
pub fn pull_requires_an_upstream() {
	let repos = pull_repos();
	let mut branch = repos.local.find_branch("master", git2::BranchType::Local).unwrap();
	branch.set_upstream(None).unwrap();

	let err = pull(&repos.local).unwrap_err();

	assert!(err.message().contains("no upstream"), "{}", err.message());
}

#[test]
pub fn pull_requires_a_branch() {
	let repos = pull_repos();
	repos.local.set_head_detached(head(&repos.local)).unwrap();

	let err = pull(&repos.local).unwrap_err();

	assert!(err.message().contains("not on a branch"), "{}", err.message());
}

#[test]
pub fn pull_keeps_unrelated_local_changes() {
	let repos = pull_repos();
	commit_file(&repos.origin, "b.txt", "new\n");
	fs::write(repos.local.workdir().unwrap().join("a.txt"), "edited\n").unwrap();

	pull(&repos.local).unwrap();

	assert_eq!("edited\n", read(&repos.local, "a.txt"));
	assert_eq!("new\n", read(&repos.local, "b.txt"));
}

#[test]
pub fn pull_refuses_to_overwrite_local_changes() {
	let repos = pull_repos();
	commit_file(&repos.origin, "a.txt", "remote\n");
	fs::write(repos.local.workdir().unwrap().join("a.txt"), "edited\n").unwrap();
	let before = head(&repos.local);

	assert!(pull(&repos.local).is_err());

	assert_eq!(before, head(&repos.local));
	assert_eq!("edited\n", read(&repos.local, "a.txt"));
}

#[test]
pub fn pull_cancelled_changes_nothing() {
	let repos = pull_repos();
	commit_file(&repos.origin, "a.txt", "2\n");
	let before = head(&repos.local);

	let result = remote_operation::pull_fast_forward(&repos.local, &AtomicBool::new(true), |_| {});

	assert!(result.is_err());
	assert_eq!(before, head(&repos.local));
	assert_eq!("1\n", read(&repos.local, "a.txt"));
}
