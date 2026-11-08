extern crate metal_git;

mod util;

use std::collections::BTreeSet;
#[cfg(unix)]
use std::ffi::OsString;
#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::os::unix::ffi::OsStringExt;
use std::path::Path;

use git2::{BranchType, ErrorCode, Repository, WorktreeAddOptions};
use metal_git::branch::{
    create_tracking_branch_and_switch, find_branch_candidates, switch_local_branch,
    BranchCandidate, BranchKind,
};

use crate::util::branch_test_repo::BranchTestRepo;
use crate::util::test_repo::TestRepo;

fn candidate(name: &str, kind: BranchKind, is_current: bool) -> BranchCandidate {
    BranchCandidate {
        name: name.to_string(),
        kind,
        is_current,
    }
}

fn candidate_set(candidates: Vec<BranchCandidate>) -> BTreeSet<(String, &'static str, bool)> {
    candidates
        .into_iter()
        .map(|candidate| {
            let kind = match candidate.kind {
                BranchKind::Local => "local",
                BranchKind::Remote => "remote",
            };
            (candidate.name, kind, candidate.is_current)
        })
        .collect()
}

fn add_linked_worktree(
    repo: &Repository,
    worktree_name: &str,
    branch_name: &str,
    path: &Path,
) -> Repository {
    let branch = repo
        .find_branch(branch_name, BranchType::Local)
        .unwrap()
        .into_reference();
    let mut options = WorktreeAddOptions::new();
    options.reference(Some(&branch));
    repo.worktree(worktree_name, path, Some(&options)).unwrap();
    Repository::open(path).unwrap()
}

fn index_tree(repo: &Repository) -> git2::Oid {
    repo.index().unwrap().write_tree().unwrap()
}

fn assert_worktree_conflict(result: Result<(), git2::Error>) {
    assert_eq!(
        "Branch is already checked out in another worktree",
        result.unwrap_err().message()
    );
}

#[cfg(unix)]
fn rename_worktree_admin(
    repo: &Repository,
    original_name: &str,
    invalid_name: Vec<u8>,
) -> Repository {
    let worktrees_path = repo.path().join("worktrees");
    let renamed_path = worktrees_path.join(OsString::from_vec(invalid_name));
    fs::rename(worktrees_path.join(original_name), &renamed_path).unwrap();
    Repository::open(renamed_path).unwrap()
}

#[test]
fn should_include_local_branch_when_commit_is_its_tip() {
    let test_repo = TestRepo::flat_two();
    let tip = test_repo.tip_oid("master");
    test_repo.create_local_branch("feature", tip);
    let repo = test_repo.repository_manager().open().unwrap();

    let candidates = find_branch_candidates(&repo, tip).unwrap();

    assert!(candidates.contains(&candidate("feature", BranchKind::Local, false)));
}

#[test]
fn should_exclude_local_branch_when_commit_is_not_its_tip() {
    let test_repo = TestRepo::flat_two();
    let tip = test_repo.tip_oid("master");
    let parent = test_repo
        .repository_manager()
        .open()
        .unwrap()
        .find_commit(tip)
        .unwrap()
        .parent_id(0)
        .unwrap();
    test_repo.create_local_branch("older", parent);
    let repo = test_repo.repository_manager().open().unwrap();

    let candidates = find_branch_candidates(&repo, tip).unwrap();

    assert!(candidates.contains(&candidate("master", BranchKind::Local, true)));
    assert!(candidates.iter().all(|candidate| candidate.name != "older"));
}

#[test]
fn should_include_remote_branch_when_commit_is_its_tip() {
    let test_repo = TestRepo::flat_two();
    let tip = test_repo.tip_oid("master");
    test_repo.create_remote_branch("origin/feature", tip);
    let repo = test_repo.repository_manager().open().unwrap();

    let candidates = find_branch_candidates(&repo, tip).unwrap();

    assert!(candidates.contains(&candidate("origin/feature", BranchKind::Remote, false,)));
}

#[test]
fn should_exclude_remote_branch_when_commit_is_not_its_tip() {
    let test_repo = TestRepo::flat_two();
    let tip = test_repo.tip_oid("master");
    let parent = test_repo
        .repository_manager()
        .open()
        .unwrap()
        .find_commit(tip)
        .unwrap()
        .parent_id(0)
        .unwrap();
    test_repo.create_remote_branch("origin/older", parent);
    let repo = test_repo.repository_manager().open().unwrap();

    let candidates = find_branch_candidates(&repo, tip).unwrap();

    assert!(candidates.contains(&candidate("master", BranchKind::Local, true)));
    assert!(candidates
        .iter()
        .all(|candidate| candidate.name != "origin/older"));
}

#[test]
fn should_include_every_branch_when_multiple_branches_share_the_tip() {
    let test_repo = TestRepo::flat_two();
    let tip = test_repo.tip_oid("master");
    test_repo.create_local_branch("feature", tip);
    test_repo.create_local_branch("release", tip);
    test_repo.create_remote_branch("origin/feature", tip);
    let repo = test_repo.repository_manager().open().unwrap();

    let candidates = candidate_set(find_branch_candidates(&repo, tip).unwrap());

    assert_eq!(
        BTreeSet::from([
            ("feature".to_string(), "local", false),
            ("master".to_string(), "local", true),
            ("origin/feature".to_string(), "remote", false),
            ("release".to_string(), "local", false),
        ]),
        candidates,
    );
}

#[test]
fn should_include_current_local_branch_and_mark_it_current() {
    let test_repo = TestRepo::flat_two();
    let tip = test_repo.tip_oid("master");
    let repo = test_repo.repository_manager().open().unwrap();

    let candidates = find_branch_candidates(&repo, tip).unwrap();

    assert!(candidates.contains(&candidate("master", BranchKind::Local, true)));
}

#[test]
fn should_return_no_candidates_when_commit_is_not_any_branch_tip() {
    let test_repo = TestRepo::flat_two();
    let tip = test_repo.tip_oid("master");
    let parent = test_repo
        .repository_manager()
        .open()
        .unwrap()
        .find_commit(tip)
        .unwrap()
        .parent_id(0)
        .unwrap();
    let repo = test_repo.repository_manager().open().unwrap();

    let candidates = find_branch_candidates(&repo, parent).unwrap();

    assert!(candidates.is_empty());
}

#[cfg(unix)]
#[test]
fn should_skip_non_utf8_branch_name_without_rejecting_other_candidates() {
    let test_repo = TestRepo::flat_two();
    let tip = test_repo.tip_oid("master");
    let repo = test_repo.repository_manager().open().unwrap();
    let invalid_name = OsString::from_vec(b"invalid-\xff".to_vec());
    let invalid_ref = repo.path().join("refs/heads").join(invalid_name);
    fs::write(invalid_ref, format!("{tip}\n")).unwrap();

    let candidates = find_branch_candidates(&repo, tip).unwrap();

    assert_eq!(
        vec![candidate("master", BranchKind::Local, true)],
        candidates
    );
}

#[test]
fn should_switch_head_to_selected_local_branch() {
    let test_repo = TestRepo::flat_two();
    let tip = test_repo.tip_oid("master");
    test_repo.create_local_branch("feature", tip);
    let repo = test_repo.repository_manager().open().unwrap();

    switch_local_branch(&repo, "feature").unwrap();

    assert_eq!("feature", test_repo.head_shorthand());
}

#[test]
fn should_update_worktree_when_switching_local_branch() {
    let test_repo = TestRepo::branches_with_distinct_files();
    let repo = test_repo.repository_manager().open().unwrap();

    switch_local_branch(&repo, "feature").unwrap();

    assert_eq!("feature\n", test_repo.file_contents("state.txt"));
}

#[test]
fn should_keep_current_branch_when_safe_checkout_rejects_local_changes() {
    let test_repo = TestRepo::branches_with_distinct_files();
    test_repo.write_file("state.txt", "local change\n");
    let repo = test_repo.repository_manager().open().unwrap();

    let result = switch_local_branch(&repo, "feature");

    assert!(result.is_err());
    assert_eq!("master", test_repo.head_shorthand());
    assert_eq!("local change\n", test_repo.file_contents("state.txt"));
}

#[test]
fn should_preserve_repository_state_when_head_is_locked_during_local_switch() {
    let test_repo = TestRepo::branches_with_distinct_files();
    let repo = test_repo.repository_manager().open().unwrap();
    let index_tree_before = repo.index().unwrap().write_tree().unwrap();
    test_repo.lock_head();

    let result = switch_local_branch(&repo, "feature");

    let index_tree_after = repo.index().unwrap().write_tree().unwrap();
    assert_eq!(ErrorCode::Locked, result.unwrap_err().code());
    assert_eq!("master", test_repo.head_shorthand());
    assert_eq!("master\n", test_repo.file_contents("state.txt"));
    assert_eq!(index_tree_before, index_tree_after);
}

#[test]
fn should_reject_branch_checked_out_in_another_worktree_without_changing_either_worktree() {
    let test_repo = TestRepo::branches_with_distinct_files();
    let repo = test_repo.repository_manager().open().unwrap();
    let linked_root = tempdir::TempDir::new("linked-worktree").unwrap();
    let linked_path = linked_root.path().join("feature");
    let linked_repo = add_linked_worktree(&repo, "feature-worktree", "feature", &linked_path);
    let main_index_before = index_tree(&repo);
    let linked_index_before = index_tree(&linked_repo);

    let result = switch_local_branch(&repo, "feature");

    assert_worktree_conflict(result);
    assert_eq!("master", test_repo.head_shorthand());
    assert_eq!("master\n", test_repo.file_contents("state.txt"));
    assert_eq!("feature", linked_repo.head().unwrap().shorthand().unwrap());
    assert_eq!(
        "feature\n",
        std::fs::read_to_string(linked_path.join("state.txt")).unwrap()
    );
    assert_eq!(main_index_before, index_tree(&repo));
    assert_eq!(linked_index_before, index_tree(&linked_repo));
}

#[test]
fn should_reject_main_branch_from_linked_worktree_without_changing_either_worktree() {
    let test_repo = TestRepo::branches_with_distinct_files();
    let main_repo = test_repo.repository_manager().open().unwrap();
    let linked_root = tempdir::TempDir::new("linked-to-main").unwrap();
    let linked_path = linked_root.path().join("feature");
    let linked_repo = add_linked_worktree(&main_repo, "feature-worktree", "feature", &linked_path);
    let main_index_before = index_tree(&main_repo);
    let linked_index_before = index_tree(&linked_repo);

    let result = switch_local_branch(&linked_repo, "master");

    assert_worktree_conflict(result);
    assert_eq!("master", main_repo.head().unwrap().shorthand().unwrap());
    assert_eq!("master\n", test_repo.file_contents("state.txt"));
    assert_eq!("feature", linked_repo.head().unwrap().shorthand().unwrap());
    assert_eq!(
        "feature\n",
        std::fs::read_to_string(linked_path.join("state.txt")).unwrap()
    );
    assert_eq!(main_index_before, index_tree(&main_repo));
    assert_eq!(linked_index_before, index_tree(&linked_repo));
}

#[test]
fn should_reject_branch_from_another_linked_worktree_without_changing_any_worktree() {
    let test_repo = TestRepo::branches_with_distinct_files();
    let main_repo = test_repo.repository_manager().open().unwrap();
    let master_tip = test_repo.tip_oid("master");
    test_repo.create_local_branch("release", master_tip);
    let linked_root = tempdir::TempDir::new("linked-to-linked").unwrap();
    let feature_path = linked_root.path().join("feature");
    let release_path = linked_root.path().join("release");
    let feature_repo =
        add_linked_worktree(&main_repo, "feature-worktree", "feature", &feature_path);
    let release_repo =
        add_linked_worktree(&main_repo, "release-worktree", "release", &release_path);
    let main_index_before = index_tree(&main_repo);
    let feature_index_before = index_tree(&feature_repo);
    let release_index_before = index_tree(&release_repo);

    let result = switch_local_branch(&feature_repo, "release");

    assert_worktree_conflict(result);
    assert_eq!("master", main_repo.head().unwrap().shorthand().unwrap());
    assert_eq!("feature", feature_repo.head().unwrap().shorthand().unwrap());
    assert_eq!("release", release_repo.head().unwrap().shorthand().unwrap());
    assert_eq!("master\n", test_repo.file_contents("state.txt"));
    assert_eq!(
        "feature\n",
        std::fs::read_to_string(feature_path.join("state.txt")).unwrap()
    );
    assert_eq!(
        "master\n",
        std::fs::read_to_string(release_path.join("state.txt")).unwrap()
    );
    assert_eq!(main_index_before, index_tree(&main_repo));
    assert_eq!(feature_index_before, index_tree(&feature_repo));
    assert_eq!(release_index_before, index_tree(&release_repo));
}

#[cfg(unix)]
#[test]
fn should_switch_when_non_utf8_worktree_name_uses_an_unrelated_branch() {
    let test_repo = TestRepo::branches_with_distinct_files();
    let main_repo = test_repo.repository_manager().open().unwrap();
    let master_tip = test_repo.tip_oid("master");
    test_repo.create_local_branch("release", master_tip);
    let linked_root = tempdir::TempDir::new("non-utf8-unrelated").unwrap();
    let release_path = linked_root.path().join("release");
    add_linked_worktree(&main_repo, "release-worktree", "release", &release_path);
    let release_repo =
        rename_worktree_admin(&main_repo, "release-worktree", b"release-\xff".to_vec());

    switch_local_branch(&main_repo, "feature").unwrap();

    assert_eq!("feature", main_repo.head().unwrap().shorthand().unwrap());
    assert_eq!("feature\n", test_repo.file_contents("state.txt"));
    assert_eq!("release", release_repo.head().unwrap().shorthand().unwrap());
    assert_eq!(
        "master\n",
        std::fs::read_to_string(release_path.join("state.txt")).unwrap()
    );
}

#[cfg(unix)]
#[test]
fn should_reject_target_branch_used_by_a_non_utf8_named_worktree() {
    let test_repo = TestRepo::branches_with_distinct_files();
    let main_repo = test_repo.repository_manager().open().unwrap();
    let linked_root = tempdir::TempDir::new("non-utf8-target").unwrap();
    let feature_path = linked_root.path().join("feature");
    add_linked_worktree(&main_repo, "feature-worktree", "feature", &feature_path);
    let feature_repo =
        rename_worktree_admin(&main_repo, "feature-worktree", b"feature-\xff".to_vec());
    let main_index_before = index_tree(&main_repo);
    let feature_index_before = index_tree(&feature_repo);

    let result = switch_local_branch(&main_repo, "feature");

    assert_worktree_conflict(result);
    assert_eq!("master", main_repo.head().unwrap().shorthand().unwrap());
    assert_eq!("master\n", test_repo.file_contents("state.txt"));
    assert_eq!("feature", feature_repo.head().unwrap().shorthand().unwrap());
    assert_eq!(
        "feature\n",
        std::fs::read_to_string(feature_path.join("state.txt")).unwrap()
    );
    assert_eq!(main_index_before, index_tree(&main_repo));
    assert_eq!(feature_index_before, index_tree(&feature_repo));
}

#[test]
fn should_create_local_branch_from_remote_tip() {
    let test_repo = TestRepo::flat_two();
    let tip = test_repo.tip_oid("master");
    test_repo.create_remote_branch("origin/feature", tip);
    let repo = test_repo.repository_manager().open().unwrap();

    create_tracking_branch_and_switch(&repo, "origin/feature").unwrap();

    assert_eq!(tip, test_repo.branch_target("feature"));
}

#[test]
fn should_set_selected_remote_as_upstream() {
    let test_repo = TestRepo::flat_two();
    let tip = test_repo.tip_oid("master");
    test_repo.create_remote_branch("origin/feature", tip);
    let repo = test_repo.repository_manager().open().unwrap();

    create_tracking_branch_and_switch(&repo, "origin/feature").unwrap();

    assert_eq!("origin/feature", test_repo.upstream_name("feature"));
}

#[test]
fn should_switch_to_created_tracking_branch() {
    let test_repo = TestRepo::flat_two();
    let tip = test_repo.tip_oid("master");
    test_repo.create_remote_branch("origin/feature", tip);
    let repo = test_repo.repository_manager().open().unwrap();

    create_tracking_branch_and_switch(&repo, "origin/feature").unwrap();

    assert_eq!("feature", test_repo.head_shorthand());
}

#[test]
fn should_remove_only_remote_prefix_from_nested_branch_name() {
    let test_repo = TestRepo::flat_two();
    let tip = test_repo.tip_oid("master");
    test_repo.create_remote_branch("origin/group/feature", tip);
    let repo = test_repo.repository_manager().open().unwrap();

    create_tracking_branch_and_switch(&repo, "origin/group/feature").unwrap();

    assert_eq!(tip, test_repo.branch_target("group/feature"));
}

#[test]
fn should_remove_the_complete_remote_name_when_remote_name_contains_slashes() {
    let test_repo = TestRepo::flat_two();
    let tip = test_repo.tip_oid("master");
    test_repo.create_remote_branch_for_remote("team/origin", "feature", tip);
    let repo = test_repo.repository_manager().open().unwrap();

    create_tracking_branch_and_switch(&repo, "team/origin/feature").unwrap();

    assert_eq!(tip, test_repo.branch_target("feature"));
    assert_eq!("team/origin/feature", test_repo.upstream_name("feature"));
    assert_eq!("feature", test_repo.head_shorthand());
}

#[test]
fn should_roll_back_tracking_branch_when_safe_checkout_rejects_local_changes() {
    let test_repo = TestRepo::branches_with_distinct_files();
    let remote_tip = test_repo.tip_oid("feature");
    test_repo.create_remote_branch("origin/remote-feature", remote_tip);
    test_repo.write_file("state.txt", "local change\n");
    let repo = test_repo.repository_manager().open().unwrap();
    let index_tree_before = repo.index().unwrap().write_tree().unwrap();

    let result = create_tracking_branch_and_switch(&repo, "origin/remote-feature");

    let index_tree_after = repo.index().unwrap().write_tree().unwrap();
    assert!(result.is_err());
    assert_eq!("master", test_repo.head_shorthand());
    assert_eq!("local change\n", test_repo.file_contents("state.txt"));
    assert_eq!(index_tree_before, index_tree_after);
    assert!(repo
        .find_branch("remote-feature", BranchType::Local)
        .is_err());
}

#[test]
fn should_preserve_repository_state_when_head_is_locked_during_tracking_switch() {
    let test_repo = TestRepo::branches_with_distinct_files();
    let remote_tip = test_repo.tip_oid("feature");
    test_repo.create_remote_branch("origin/remote-feature", remote_tip);
    let repo = test_repo.repository_manager().open().unwrap();
    let index_tree_before = repo.index().unwrap().write_tree().unwrap();
    test_repo.lock_head();

    let result = create_tracking_branch_and_switch(&repo, "origin/remote-feature");

    let index_tree_after = repo.index().unwrap().write_tree().unwrap();
    assert_eq!(ErrorCode::Locked, result.unwrap_err().code());
    assert_eq!("master", test_repo.head_shorthand());
    assert_eq!("master\n", test_repo.file_contents("state.txt"));
    assert_eq!(index_tree_before, index_tree_after);
    assert!(repo
        .find_branch("remote-feature", BranchType::Local)
        .is_err());
}

#[test]
fn should_not_overwrite_existing_local_branch_when_remote_name_collides() {
    let test_repo = TestRepo::flat_two();
    let remote_tip = test_repo.tip_oid("master");
    let existing_tip = test_repo
        .repository_manager()
        .open()
        .unwrap()
        .find_commit(remote_tip)
        .unwrap()
        .parent_id(0)
        .unwrap();
    test_repo.create_local_branch("feature", existing_tip);
    test_repo.create_remote_branch("origin/feature", remote_tip);
    let repo = test_repo.repository_manager().open().unwrap();

    let result = create_tracking_branch_and_switch(&repo, "origin/feature");

    assert!(result.is_err());
    assert_eq!(existing_tip, test_repo.branch_target("feature"));
    assert_eq!("master", test_repo.head_shorthand());
}

#[test]
fn should_reject_second_remote_when_remote_branches_map_to_same_local_name() {
    let test_repo = TestRepo::flat_two();
    let tip = test_repo.tip_oid("master");
    test_repo.create_remote_branch("origin/feature", tip);
    test_repo.create_remote_branch("upstream/feature", tip);
    let repo = test_repo.repository_manager().open().unwrap();
    create_tracking_branch_and_switch(&repo, "origin/feature").unwrap();

    let result = create_tracking_branch_and_switch(&repo, "upstream/feature");

    assert!(result.is_err());
    assert_eq!("origin/feature", test_repo.upstream_name("feature"));
    assert_eq!("feature", test_repo.head_shorthand());
}
