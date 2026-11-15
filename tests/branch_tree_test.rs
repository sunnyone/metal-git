extern crate git2;
extern crate metal_git;
extern crate tempdir;

mod util;
use crate::util::branch_test_repo::BranchTestRepo;
use crate::util::test_repo::TestRepo;
use metal_git::branch_tree::{collect_branch_tree, contains_scope, natural_cmp, BranchTreeKind, BranchTreeNode};
use metal_git::railway::HistoryScope;
use std::cmp::Ordering;

/// The labels of every row, indented by two spaces per level.
fn labels(nodes: &[BranchTreeNode]) -> Vec<String> {
    fn walk(nodes: &[BranchTreeNode], depth: usize, out: &mut Vec<String>) {
        for node in nodes {
            out.push(format!("{}{}", "  ".repeat(depth), node.label));
            walk(&node.children, depth + 1, out);
        }
    }
    let mut out = Vec::new();
    walk(nodes, 0, &mut out);
    out
}

fn tag(test_repo: &TestRepo, name: &str) {
    let repo = test_repo.repository_manager().open().unwrap();
    let target = repo.find_object(test_repo.tip_oid("master"), None).unwrap();
    repo.tag_lightweight(name, &target, false).unwrap();
}

#[test]
pub fn references_come_before_folders_and_names_are_sorted_naturally() {
    let test_repo = TestRepo::branches_with_distinct_files();
    let master = test_repo.tip_oid("master");
    for name in ["issue-10", "Issue-2", "topic/login", "topic/diff", "user/me/fix/x"] {
        test_repo.create_local_branch(name, master);
    }
    test_repo.create_remote_branch_for_remote("upstream", "main", master);
    test_repo.create_remote_branch_for_remote("backup", "main", master);
    test_repo.create_remote_branch("origin/feature/login", master);
    test_repo.create_remote_branch("origin/main", master);
    for name in ["v0.2.0", "v0.10.0", "v0.9.1"] {
        tag(&test_repo, name);
    }

    let nodes = collect_branch_tree(&test_repo.repository_manager().open().unwrap()).unwrap();

    assert_eq!(
        vec![
            "All Branches",
            "Current Branch (master)",
            "Local",
            "  feature",
            "  Issue-2",
            "  issue-10",
            "  master",
            "  topic/",
            "    diff",
            "    login",
            // A folder with only one folder in it shares its row
            "  user/me/fix/",
            "    x",
            "Remote",
            // origin comes first among the remotes
            "  origin/",
            "    main",
            "    feature/",
            "      login",
            "  backup/",
            "    main",
            "  upstream/",
            "    main",
            "Tags",
            "  v0.10.0",
            "  v0.9.1",
            "  v0.2.0",
        ],
        labels(&nodes)
    );
}

#[test]
pub fn rows_carry_the_scope_they_show() {
    let test_repo = TestRepo::branches_with_distinct_files();
    test_repo.create_local_branch("user/me/fix/x", test_repo.tip_oid("master"));
    test_repo.create_remote_branch("origin/main", test_repo.tip_oid("master"));

    let nodes = collect_branch_tree(&test_repo.repository_manager().open().unwrap()).unwrap();

    assert_eq!(HistoryScope::AllBranches, nodes[0].scope);
    assert_eq!(HistoryScope::CurrentBranch, nodes[1].scope);
    let local = &nodes[2];
    assert_eq!((BranchTreeKind::Header, HistoryScope::Prefix("refs/heads/".to_owned())), (local.kind, local.scope.clone()));
    let folder = local.children.iter().find(|node| node.kind == BranchTreeKind::Folder).unwrap();
    assert_eq!(HistoryScope::Prefix("refs/heads/user/me/fix/".to_owned()), folder.scope);
    assert_eq!(HistoryScope::Ref("refs/heads/user/me/fix/x".to_owned()), folder.children[0].scope);
    let master = local.children.iter().find(|node| node.label == "master").unwrap();
    assert!(master.is_head);
    assert_eq!(Some("master"), master.branch_name());
    let origin = &nodes[3].children[0];
    assert_eq!(HistoryScope::Prefix("refs/remotes/origin/".to_owned()), origin.scope);
    assert_eq!((BranchTreeKind::RemoteBranch, Some("origin/main")), (origin.children[0].kind, origin.children[0].branch_name()));
    assert!(contains_scope(&nodes, &HistoryScope::Ref("refs/heads/user/me/fix/x".to_owned())));
    assert!(!contains_scope(&nodes, &HistoryScope::Ref("refs/heads/gone".to_owned())));
}

#[test]
pub fn headers_without_references_and_symbolic_references_are_left_out() {
    let test_repo = TestRepo::branches_with_distinct_files();
    let repo = test_repo.repository_manager().open().unwrap();
    repo.reference_symbolic("refs/remotes/origin/HEAD", "refs/heads/master", false, "test").unwrap();

    let nodes = collect_branch_tree(&repo).unwrap();

    assert_eq!(
        vec!["All Branches", "Current Branch (master)", "Local", "  feature", "  master"],
        labels(&nodes)
    );
}

#[test]
pub fn detached_head_is_named_head() {
    let test_repo = TestRepo::branches_with_distinct_files();
    let repo = test_repo.repository_manager().open().unwrap();
    repo.set_head_detached(test_repo.tip_oid("feature")).unwrap();

    let nodes = collect_branch_tree(&repo).unwrap();

    assert_eq!("Current Branch (HEAD)", nodes[1].label);
}

#[test]
pub fn natural_order_ignores_case_and_compares_numbers_by_value() {
    assert_eq!(Ordering::Less, natural_cmp("issue-2", "issue-10"));
    assert_eq!(Ordering::Less, natural_cmp("Alpha", "beta"));
    assert_eq!(Ordering::Greater, natural_cmp("v0.10.0", "v0.9.1"));
    assert_eq!(Ordering::Less, natural_cmp("v1", "v1.0"));
}
