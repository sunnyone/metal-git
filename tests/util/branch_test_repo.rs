#![allow(dead_code)]

use std::fs;
use std::path::Path;

use git2::build::CheckoutBuilder;
use git2::{BranchType, Commit, Oid, Repository, Signature};

use super::test_repo::TestRepo;

pub trait BranchTestRepo {
    fn branches_with_distinct_files() -> TestRepo;
    fn branches_with_target_only_file() -> TestRepo;
    fn tip_oid(&self, branch_name: &str) -> Oid;
    fn create_local_branch(&self, branch_name: &str, target: Oid);
    fn create_remote_branch(&self, branch_name: &str, target: Oid);
    fn create_remote_branch_for_remote(&self, remote_name: &str, branch_name: &str, target: Oid);
    fn lock_head(&self);
    fn head_shorthand(&self) -> String;
    fn branch_target(&self, branch_name: &str) -> Oid;
    fn upstream_name(&self, branch_name: &str) -> String;
    fn write_file(&self, filename: &str, contents: &str);
    fn file_contents(&self, filename: &str) -> String;
}

impl BranchTestRepo for TestRepo {
    fn branches_with_distinct_files() -> TestRepo {
        let test_repo = TestRepo::empty();
        let repo = test_repo.repository_manager().open().unwrap();
        let base = commit_file(&repo, "master", "master\n", &[]);
        let _ = commit_file(&repo, "feature", "feature\n", &[&base]);
        repo.set_head("refs/heads/master").unwrap();
        repo.checkout_head(Some(CheckoutBuilder::new().force()))
            .unwrap();
        test_repo
    }

    fn branches_with_target_only_file() -> TestRepo {
        let test_repo = TestRepo::empty();
        let repo = test_repo.repository_manager().open().unwrap();
        let base = commit_file(&repo, "master", "master\n", &[]);
        let _ = commit_named_file(&repo, "feature", "incoming.txt", "feature file\n", &[&base]);
        repo.set_head("refs/heads/master").unwrap();
        repo.checkout_head(Some(CheckoutBuilder::new().force()))
            .unwrap();
        test_repo
    }

    fn tip_oid(&self, branch_name: &str) -> Oid {
        self.repository_manager()
            .open()
            .unwrap()
            .find_branch(branch_name, BranchType::Local)
            .unwrap()
            .get()
            .target()
            .unwrap()
    }

    fn create_local_branch(&self, branch_name: &str, target: Oid) {
        let repo = self.repository_manager().open().unwrap();
        let commit = repo.find_commit(target).unwrap();
        repo.branch(branch_name, &commit, false).unwrap();
    }

    fn create_remote_branch(&self, branch_name: &str, target: Oid) {
        let (remote_name, remote_branch_name) = branch_name.split_once('/').unwrap();
        self.create_remote_branch_for_remote(remote_name, remote_branch_name, target);
    }

    fn create_remote_branch_for_remote(&self, remote_name: &str, branch_name: &str, target: Oid) {
        let repo = self.repository_manager().open().unwrap();
        if repo.find_remote(remote_name).is_err() {
            repo.remote(remote_name, "https://example.invalid/repository.git")
                .unwrap();
        }
        repo.reference(
            &format!("refs/remotes/{remote_name}/{branch_name}"),
            target,
            false,
            "test remote-tracking branch",
        )
        .unwrap();
    }

    fn lock_head(&self) {
        let repo = self.repository_manager().open().unwrap();
        fs::write(repo.path().join("HEAD.lock"), b"locked").unwrap();
    }

    fn head_shorthand(&self) -> String {
        self.repository_manager()
            .open()
            .unwrap()
            .head()
            .unwrap()
            .shorthand()
            .unwrap()
            .to_string()
    }

    fn branch_target(&self, branch_name: &str) -> Oid {
        self.repository_manager()
            .open()
            .unwrap()
            .find_branch(branch_name, BranchType::Local)
            .unwrap()
            .get()
            .target()
            .unwrap()
    }

    fn upstream_name(&self, branch_name: &str) -> String {
        self.repository_manager()
            .open()
            .unwrap()
            .find_branch(branch_name, BranchType::Local)
            .unwrap()
            .upstream()
            .unwrap()
            .name()
            .unwrap()
            .unwrap()
            .to_string()
    }

    fn write_file(&self, filename: &str, contents: &str) {
        fs::write(self.path().join(filename), contents).unwrap();
    }

    fn file_contents(&self, filename: &str) -> String {
        fs::read_to_string(self.path().join(filename)).unwrap()
    }
}

fn commit_file<'repo, 'parents>(
    repo: &'repo Repository,
    branch_name: &str,
    contents: &str,
    parents: &'parents [&'parents Commit],
) -> Commit<'repo> {
    commit_named_file(repo, branch_name, "state.txt", contents, parents)
}

fn commit_named_file<'repo, 'parents>(
    repo: &'repo Repository,
    branch_name: &str,
    filename: &str,
    contents: &str,
    parents: &'parents [&'parents Commit],
) -> Commit<'repo> {
    fs::write(repo.workdir().unwrap().join(filename), contents).unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new(filename)).unwrap();
    index.write().unwrap();
    let tree_oid = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_oid).unwrap();
    let signature = Signature::new(
        "test commit",
        "test@example.com",
        &git2::Time::new(1_700_000_000, 0),
    )
    .unwrap();
    let commit_oid = repo
        .commit(None, &signature, &signature, branch_name, &tree, parents)
        .unwrap();
    let commit = repo.find_commit(commit_oid).unwrap();
    repo.branch(branch_name, &commit, false).unwrap();
    commit
}
