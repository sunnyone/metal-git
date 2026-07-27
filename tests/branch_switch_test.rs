extern crate metal_git;

mod util;

use git2::BranchType;
use metal_git::branch::{create_tracking_branch_and_switch, switch_local_branch};

use crate::util::branch_test_repo::BranchTestRepo;
use crate::util::test_repo::TestRepo;

const MODIFIED_CONFLICT_PATH: &str = "state.txt";
const UNTRACKED_CONFLICT_PATH: &str = "incoming.txt";

fn index_tree(test_repo: &TestRepo) -> git2::Oid {
    test_repo
        .repository_manager()
        .open()
        .unwrap()
        .index()
        .unwrap()
        .write_tree()
        .unwrap()
}

#[test]
fn should_include_modified_file_path_when_local_branch_switch_conflicts() {
    let test_repo = TestRepo::branches_with_distinct_files();
    test_repo.write_file(MODIFIED_CONFLICT_PATH, "local change\n");
    let repo = test_repo.repository_manager().open().unwrap();

    let error = switch_local_branch(&repo, "feature").unwrap_err();

    assert!(
        error.message().contains(MODIFIED_CONFLICT_PATH),
        "checkout error did not identify the modified file: {error}"
    );
}

#[test]
fn should_preserve_head_worktree_and_index_when_modified_file_conflicts() {
    let test_repo = TestRepo::branches_with_distinct_files();
    test_repo.write_file(MODIFIED_CONFLICT_PATH, "local change\n");
    let index_before = index_tree(&test_repo);
    let repo = test_repo.repository_manager().open().unwrap();

    let result = switch_local_branch(&repo, "feature");

    assert!(result.is_err());
    assert_eq!("master", test_repo.head_shorthand());
    assert_eq!(
        "local change\n",
        test_repo.file_contents(MODIFIED_CONFLICT_PATH)
    );
    assert_eq!(index_before, index_tree(&test_repo));
}

#[test]
fn should_include_untracked_file_path_when_local_branch_switch_conflicts() {
    let test_repo = TestRepo::branches_with_target_only_file();
    test_repo.write_file(UNTRACKED_CONFLICT_PATH, "untracked contents\n");
    let repo = test_repo.repository_manager().open().unwrap();

    let error = switch_local_branch(&repo, "feature").unwrap_err();

    assert!(
        error.message().contains(UNTRACKED_CONFLICT_PATH),
        "checkout error did not identify the untracked file: {error}"
    );
}

#[test]
fn should_preserve_untracked_file_and_current_branch_when_switch_conflicts() {
    let test_repo = TestRepo::branches_with_target_only_file();
    test_repo.write_file(UNTRACKED_CONFLICT_PATH, "untracked contents\n");
    let repo = test_repo.repository_manager().open().unwrap();

    let result = switch_local_branch(&repo, "feature");

    assert!(result.is_err());
    assert_eq!("master", test_repo.head_shorthand());
    assert_eq!(
        "untracked contents\n",
        test_repo.file_contents(UNTRACKED_CONFLICT_PATH)
    );
}

#[test]
fn should_propagate_conflicting_file_path_and_roll_back_tracking_branch() {
    let test_repo = TestRepo::branches_with_distinct_files();
    let remote_tip = test_repo.tip_oid("feature");
    test_repo.create_remote_branch("origin/remote-feature", remote_tip);
    test_repo.write_file(MODIFIED_CONFLICT_PATH, "local change\n");
    let repo = test_repo.repository_manager().open().unwrap();

    let error = create_tracking_branch_and_switch(&repo, "origin/remote-feature").unwrap_err();

    assert!(
        error.message().contains(MODIFIED_CONFLICT_PATH),
        "tracking switch error did not preserve the conflict path: {error}"
    );
    assert!(repo
        .find_branch("remote-feature", BranchType::Local)
        .is_err());
}
