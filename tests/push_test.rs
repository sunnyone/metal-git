extern crate git2;
extern crate tempdir;

extern crate metal_git;

use git2::{BranchType, Oid, Repository, Signature};
use metal_git::remote_operation::{
	self, PushButtonState, PushOptions, PushRequest, PushSummary, PushTarget,
};
use std::fs;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use tempdir::TempDir;

struct PushRepos {
	_origin_dir: TempDir,
	_local_dir: TempDir,
	_other_dir: TempDir,
	origin: Repository,
	local: Repository,
	/// Another clone, to move the remote branch behind the local clone's back.
	other: Repository,
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

fn push_repos() -> PushRepos {
	let origin_dir = TempDir::new("push-origin").unwrap();
	let origin = Repository::init_bare(origin_dir.path()).unwrap();
	origin.set_head("refs/heads/master").unwrap();

	let seed_dir = TempDir::new("push-seed").unwrap();
	let seed = Repository::init(seed_dir.path()).unwrap();
	seed.set_head("refs/heads/master").unwrap();
	commit_file(&seed, "a.txt", "1\n");
	seed.remote("origin", origin_dir.path().to_str().unwrap()).unwrap()
		.push(&["refs/heads/master:refs/heads/master"], None).unwrap();

	let local_dir = TempDir::new("push-local").unwrap();
	let local = Repository::clone(origin_dir.path().to_str().unwrap(), local_dir.path()).unwrap();
	let other_dir = TempDir::new("push-other").unwrap();
	let other = Repository::clone(origin_dir.path().to_str().unwrap(), other_dir.path()).unwrap();

	PushRepos { _origin_dir: origin_dir, _local_dir: local_dir, _other_dir: other_dir, origin, local, other }
}

fn target(local_branch: &str, remote_branch: &str) -> PushTarget {
	PushTarget {
		local_branch: local_branch.to_string(),
		remote_name: "origin".to_string(),
		remote_branch: remote_branch.to_string(),
	}
}

fn request(repo: &Repository, target: PushTarget, options: PushOptions) -> PushRequest {
	let preview = remote_operation::preview_push(repo, &target).unwrap();
	PushRequest { target, options, expected_remote_oid: preview.remote_oid }
}

fn push(repo: &Repository, request: &PushRequest) -> Result<PushSummary, git2::Error> {
	remote_operation::push(repo, request, &AtomicBool::new(false), |_| {})
}

fn remote_tip(origin: &Repository, branch: &str) -> Option<Oid> {
	origin.refname_to_id(&format!("refs/heads/{}", branch)).ok()
}

fn upstream_name(repo: &Repository, branch: &str) -> Option<String> {
	repo.find_branch(branch, BranchType::Local).unwrap()
		.upstream().ok()
		.and_then(|upstream| upstream.name().unwrap().map(String::from))
}

/// Moves origin/master forward from the other clone, so the local clone falls behind.
fn advance_remote(repos: &PushRepos) -> Oid {
	let tip = commit_file(&repos.other, "other.txt", "other\n");
	repos.other.find_remote("origin").unwrap()
		.push(&["refs/heads/master:refs/heads/master"], None).unwrap();
	tip
}

#[test]
pub fn push_fast_forwards_the_remote_branch() {
	let repos = push_repos();
	commit_file(&repos.local, "b.txt", "b\n");
	let tip = commit_file(&repos.local, "c.txt", "c\n");

	let summary = push(&repos.local, &request(&repos.local, target("master", "master"), PushOptions::default())).unwrap();

	assert_eq!(2, summary.pushed_commits);
	assert!(!summary.created);
	assert!(!summary.forced);
	assert_eq!(Some(tip), remote_tip(&repos.origin, "master"));
	assert_eq!(tip, repos.local.refname_to_id("refs/remotes/origin/master").unwrap());
}

#[test]
pub fn push_creates_the_branch_and_sets_upstream() {
	let repos = push_repos();
	let head = repos.local.head().unwrap().peel_to_commit().unwrap();
	repos.local.branch("feature", &head, false).unwrap();
	repos.local.set_head("refs/heads/feature").unwrap();
	let tip = commit_file(&repos.local, "feature.txt", "f\n");

	let options = PushOptions { set_upstream: true, force_with_lease: false };
	let summary = push(&repos.local, &request(&repos.local, target("feature", "feature"), options)).unwrap();

	assert!(summary.created);
	assert_eq!(1, summary.pushed_commits);
	assert_eq!(Some(tip), remote_tip(&repos.origin, "feature"));
	assert_eq!(Some("origin/feature".to_string()), upstream_name(&repos.local, "feature"));
}

#[test]
pub fn push_to_a_branch_with_another_name() {
	let repos = push_repos();
	let tip = commit_file(&repos.local, "b.txt", "b\n");

	push(&repos.local, &request(&repos.local, target("master", "release"), PushOptions::default())).unwrap();

	assert_eq!(Some(tip), remote_tip(&repos.origin, "release"));
	assert_eq!(Some("origin/master".to_string()), upstream_name(&repos.local, "master"));
}

#[test]
pub fn push_is_rejected_when_the_remote_has_other_commits() {
	let repos = push_repos();
	let remote_tip_before = advance_remote(&repos);
	repos.local.find_remote("origin").unwrap().fetch::<&str>(&[], None, None).unwrap();
	commit_file(&repos.local, "local.txt", "local\n");

	let err = push(&repos.local, &request(&repos.local, target("master", "master"), PushOptions::default())).unwrap_err();

	assert!(err.message().contains("Pull first"), "{}", err.message());
	assert_eq!(Some(remote_tip_before), remote_tip(&repos.origin, "master"));
}

#[test]
pub fn push_is_rejected_when_the_remote_moved_after_the_last_fetch() {
	let repos = push_repos();
	let remote_tip_before = advance_remote(&repos);
	commit_file(&repos.local, "local.txt", "local\n");

	let err = push(&repos.local, &request(&repos.local, target("master", "master"), PushOptions::default())).unwrap_err();

	assert!(err.message().contains("Pull first"), "{}", err.message());
	assert_eq!(Some(remote_tip_before), remote_tip(&repos.origin, "master"));
}

#[test]
pub fn force_push_overwrites_the_fetched_remote_branch() {
	let repos = push_repos();
	advance_remote(&repos);
	repos.local.find_remote("origin").unwrap().fetch::<&str>(&[], None, None).unwrap();
	let tip = commit_file(&repos.local, "local.txt", "local\n");

	let options = PushOptions { set_upstream: false, force_with_lease: true };
	let summary = push(&repos.local, &request(&repos.local, target("master", "master"), options)).unwrap();

	assert!(summary.forced);
	assert_eq!(Some(tip), remote_tip(&repos.origin, "master"));
}

#[test]
pub fn force_push_stops_when_the_remote_moved_after_the_last_fetch() {
	let repos = push_repos();
	let remote_tip_before = advance_remote(&repos);
	commit_file(&repos.local, "local.txt", "local\n");

	let options = PushOptions { set_upstream: false, force_with_lease: true };
	let err = push(&repos.local, &request(&repos.local, target("master", "master"), options)).unwrap_err();

	assert!(err.message().contains("changed since your last fetch"), "{}", err.message());
	assert_eq!(Some(remote_tip_before), remote_tip(&repos.origin, "master"));
}

#[test]
pub fn push_cancelled_before_start_changes_nothing() {
	let repos = push_repos();
	let before = remote_tip(&repos.origin, "master");
	commit_file(&repos.local, "b.txt", "b\n");
	let request = request(&repos.local, target("master", "master"), PushOptions::default());

	let result = remote_operation::push(&repos.local, &request, &AtomicBool::new(true), |_| {});

	assert!(result.is_err());
	assert_eq!(before, remote_tip(&repos.origin, "master"));
}

#[test]
pub fn push_rejects_an_invalid_remote_branch_name() {
	let repos = push_repos();

	let err = push(&repos.local, &request(&repos.local, target("master", "bad..name"), PushOptions::default())).unwrap_err();

	assert!(err.message().contains("not a valid branch name"), "{}", err.message());
}

#[test]
pub fn preview_lists_outgoing_commits_newest_first() {
	let repos = push_repos();
	commit_file(&repos.local, "b.txt", "b\n");
	commit_file(&repos.local, "c.txt", "c\n");

	let preview = remote_operation::preview_push(&repos.local, &target("master", "master")).unwrap();

	assert_eq!(2, preview.outgoing.count);
	assert!(!preview.outgoing.count_is_capped);
	let summaries: Vec<_> = preview.outgoing.commits.iter().map(|c| c.summary.as_str()).collect();
	assert_eq!(vec!["c.txt", "b.txt"], summaries);
	assert_eq!(Some("origin/master".to_string()), preview.upstream);
	assert!(!preview.creates_branch());
	assert_eq!(0, preview.remote_only_commits);
}

#[test]
pub fn preview_of_a_new_branch_counts_commits_not_on_the_remote() {
	let repos = push_repos();
	commit_file(&repos.local, "b.txt", "b\n");

	let preview = remote_operation::preview_push(&repos.local, &target("master", "feature")).unwrap();

	assert!(preview.creates_branch());
	assert_eq!(1, preview.outgoing.count);
}

#[test]
pub fn preview_counts_commits_only_on_the_remote() {
	let repos = push_repos();
	advance_remote(&repos);
	repos.local.find_remote("origin").unwrap().fetch::<&str>(&[], None, None).unwrap();
	commit_file(&repos.local, "local.txt", "local\n");

	let preview = remote_operation::preview_push(&repos.local, &target("master", "master")).unwrap();

	assert_eq!(1, preview.remote_only_commits);
	assert_eq!(1, preview.outgoing.count);
}

#[test]
pub fn default_target_follows_the_upstream() {
	let repos = push_repos();

	assert_eq!(target("master", "master"), remote_operation::default_push_target(&repos.local, "master").unwrap());
}

#[test]
pub fn default_target_without_upstream_uses_the_same_name_on_origin() {
	let repos = push_repos();
	let head = repos.local.head().unwrap().peel_to_commit().unwrap();
	repos.local.branch("feature", &head, false).unwrap();

	assert_eq!(target("feature", "feature"), remote_operation::default_push_target(&repos.local, "feature").unwrap());
}

#[test]
pub fn button_state_follows_the_current_branch() {
	let repos = push_repos();
	assert_eq!(
		PushButtonState::UpToDate { branch: "master".to_string(), upstream: "origin/master".to_string() },
		remote_operation::push_button_state(&repos.local)
	);

	commit_file(&repos.local, "b.txt", "b\n");
	assert_eq!(
		PushButtonState::Ahead { branch: "master".to_string(), upstream: "origin/master".to_string(), count: 1 },
		remote_operation::push_button_state(&repos.local)
	);

	let head = repos.local.head().unwrap().peel_to_commit().unwrap();
	repos.local.branch("feature", &head, false).unwrap();
	repos.local.set_head("refs/heads/feature").unwrap();
	assert_eq!(
		PushButtonState::NoUpstream { branch: "feature".to_string() },
		remote_operation::push_button_state(&repos.local)
	);

	repos.local.set_head_detached(head.id()).unwrap();
	assert_eq!(PushButtonState::NoBranch, remote_operation::push_button_state(&repos.local));
}

#[test]
pub fn button_state_without_remotes() {
	let repos = push_repos();
	repos.local.remote_delete("origin").unwrap();

	assert_eq!(PushButtonState::NoRemote, remote_operation::push_button_state(&repos.local));
}

#[test]
pub fn url_credentials_are_masked() {
	assert_eq!("https://user:***@github.com/a/b.git", remote_operation::mask_url_credentials("https://user:secret@github.com/a/b.git"));
	assert_eq!("https://token@github.com/a/b.git", remote_operation::mask_url_credentials("https://token@github.com/a/b.git"));
	assert_eq!("https://github.com/a/b.git", remote_operation::mask_url_credentials("https://github.com/a/b.git"));
	assert_eq!("git@github.com:a/b.git", remote_operation::mask_url_credentials("git@github.com:a/b.git"));
	assert_eq!("/srv/repo.git", remote_operation::mask_url_credentials("/srv/repo.git"));
}

#[test]
pub fn branch_names_are_validated() {
	assert!(remote_operation::is_valid_branch_name("main"));
	assert!(remote_operation::is_valid_branch_name("feature/push"));
	assert!(!remote_operation::is_valid_branch_name(""));
	assert!(!remote_operation::is_valid_branch_name("bad..name"));
	assert!(!remote_operation::is_valid_branch_name("with space"));
}
