extern crate git2;
extern crate tempdir;

extern crate metal_git;

mod util;

use git2::Repository;
use metal_git::remote_operation;
use std::sync::atomic::{AtomicBool, Ordering};
use crate::util::test_repo::TestRepo;

fn add_remote(local: &Repository, name: &str, remote: &TestRepo) {
	local.remote(name, remote.path().to_str().unwrap()).unwrap();
}

#[test]
pub fn fetch_all_remotes() {
	let local_test_repo = TestRepo::empty();
	let origin = TestRepo::flat_two();
	let upstream = TestRepo::two_parent_two_child();

	let local = Repository::open(local_test_repo.path()).unwrap();
	add_remote(&local, "origin", &origin);
	add_remote(&local, "upstream", &upstream);

	let summary = remote_operation::fetch_all(&local, &AtomicBool::new(false), |_| {}).unwrap();

	assert_eq!(vec!["origin", "upstream"], summary.remote_names);
	assert!(summary.received_objects > 0);
	assert!(local.find_reference("refs/remotes/origin/master").is_ok());
	assert!(local.find_reference("refs/remotes/upstream/master").is_ok());
	assert!(local.find_reference("refs/remotes/upstream/branch1").is_ok());
}

#[test]
pub fn fetch_again_receives_nothing() {
	let local_test_repo = TestRepo::empty();
	let origin = TestRepo::flat_two();

	let local = Repository::open(local_test_repo.path()).unwrap();
	add_remote(&local, "origin", &origin);

	remote_operation::fetch_all(&local, &AtomicBool::new(false), |_| {}).unwrap();
	let summary = remote_operation::fetch_all(&local, &AtomicBool::new(false), |_| {}).unwrap();

	assert_eq!(0, summary.received_objects);
}

#[test]
pub fn fetch_without_remotes() {
	let local_test_repo = TestRepo::flat_two();
	let local = Repository::open(local_test_repo.path()).unwrap();

	let summary = remote_operation::fetch_all(&local, &AtomicBool::new(false), |_| {}).unwrap();

	assert!(summary.remote_names.is_empty());
}

#[test]
pub fn fetch_cancelled() {
	let local_test_repo = TestRepo::empty();
	let origin = TestRepo::flat_two();

	let local = Repository::open(local_test_repo.path()).unwrap();
	add_remote(&local, "origin", &origin);

	let result = remote_operation::fetch_all(&local, &AtomicBool::new(true), |_| {});

	assert!(result.is_err());
	assert!(local.find_reference("refs/remotes/origin/master").is_err());
}

#[test]
pub fn fetch_cancelled_during_transfer() {
	let local_test_repo = TestRepo::empty();
	let origin = TestRepo::flat_two();

	let local = Repository::open(local_test_repo.path()).unwrap();
	add_remote(&local, "origin", &origin);

	let cancelled = AtomicBool::new(false);
	let result = remote_operation::fetch_all(&local, &cancelled, |_| cancelled.store(true, Ordering::SeqCst));

	assert!(result.is_err());
	assert!(local.find_reference("refs/remotes/origin/master").is_err());
}
