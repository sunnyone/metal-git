extern crate metal_git;

use git2::{Commit, Repository, Signature};
use metal_git::background_task::CancelToken;
use metal_git::branch::{BranchCandidate, BranchKind};
use metal_git::long_running_operation::{LongRunningEvent, LongRunningOperation, LongRunningStatus};
use metal_git::switch_branch_operation::SwitchBranchOperation;
use std::path::Path;
use tempdir::TempDir;

type Event = LongRunningEvent<String, String, String>;

fn commit(repo: &Repository, message: &str, parents: &[&Commit<'_>]) -> git2::Oid {
	let signature = Signature::now("test", "test@example.com").unwrap();
	let tree_oid = repo.treebuilder(None).unwrap().write().unwrap();
	let tree = repo.find_tree(tree_oid).unwrap();
	repo.commit(Some("HEAD"), &signature, &signature, message, &tree, parents).unwrap()
}

fn repo_with_feature_branch() -> (TempDir, Repository) {
	let temp_dir = TempDir::new("switch-branch-operation").unwrap();
	let repo = Repository::init(temp_dir.path()).unwrap();
	repo.set_head("refs/heads/master").unwrap();
	let oid = commit(&repo, "root", &[]);
	repo.branch("feature", &repo.find_commit(oid).unwrap(), false).unwrap();
	(temp_dir, repo)
}

fn candidate(name: &str, kind: BranchKind) -> BranchCandidate {
	BranchCandidate { name: name.to_string(), kind, is_current: false }
}

fn run(git_dir: &Path, candidate: BranchCandidate) -> Result<String, String> {
	let operation = SwitchBranchOperation { git_dir: git_dir.to_path_buf(), candidate };
	operation.run(&CancelToken::default(), &|_| {})
}

fn head_name(repo: &Repository) -> String {
	repo.head().unwrap().shorthand().unwrap().to_string()
}

#[test]
pub fn switches_to_local_branch() {
	let (_temp_dir, repo) = repo_with_feature_branch();

	let result = run(repo.path(), candidate("feature", BranchKind::Local));

	assert_eq!(Ok("feature".to_string()), result);
	assert_eq!("feature", head_name(&repo));
}

#[test]
pub fn remote_branch_reports_the_created_local_branch() {
	let (_temp_dir, repo) = repo_with_feature_branch();
	let oid = repo.head().unwrap().target().unwrap();
	repo.remote("origin", "https://example.invalid/repository.git").unwrap();
	repo.reference("refs/remotes/origin/topic", oid, false, "test").unwrap();

	let result = run(repo.path(), candidate("origin/topic", BranchKind::Remote));

	assert_eq!(Ok("topic".to_string()), result);
	assert_eq!("topic", head_name(&repo));
}

#[test]
pub fn switches_a_linked_worktree_from_its_git_dir() {
	let (_temp_dir, repo) = repo_with_feature_branch();
	let linked_root = TempDir::new("switch-branch-linked").unwrap();
	let worktree = repo.worktree("linked", &linked_root.path().join("linked"), None).unwrap();
	let linked_repo = Repository::open_from_worktree(&worktree).unwrap();

	let result = run(linked_repo.path(), candidate("feature", BranchKind::Local));

	assert_eq!(Ok("feature".to_string()), result);
	assert_eq!("feature", head_name(&linked_repo));
	assert_eq!("master", head_name(&repo));
}

#[test]
pub fn missing_branch_is_an_error() {
	let (_temp_dir, repo) = repo_with_feature_branch();

	assert!(run(repo.path(), candidate("missing", BranchKind::Local)).is_err());
	assert_eq!("master", head_name(&repo));
}

#[test]
pub fn started_is_not_cancellable() {
	assert_eq!(
		LongRunningStatus::Started { message: "Switching branch...".to_string(), cancellable: false },
		SwitchBranchOperation::describe(&Event::Started)
	);
}

#[test]
pub fn progress_names_the_branch() {
	assert_eq!(
		LongRunningStatus::Progress { message: "Switching to feature...".to_string(), fraction: None },
		SwitchBranchOperation::describe(&Event::Progress("feature".to_string()))
	);
}

#[test]
pub fn success_reloads_history() {
	assert_eq!(
		LongRunningStatus::Succeeded { message: "Switched to feature".to_string(), reload_history: true },
		SwitchBranchOperation::describe(&Event::Succeeded("feature".to_string()))
	);
}

#[test]
pub fn failure_uses_common_message() {
	assert_eq!(
		LongRunningStatus::Failed { message: "Switch branch failed: conflict".to_string() },
		SwitchBranchOperation::describe(&Event::Failed("conflict".to_string()))
	);
}
