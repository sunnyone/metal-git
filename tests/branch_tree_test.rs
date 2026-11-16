extern crate git2;
extern crate metal_git;
extern crate tempdir;

mod util;
use crate::util::branch_test_repo::BranchTestRepo;
use crate::util::test_repo::TestRepo;
use metal_git::branch_tree::{collect_branch_tree, filter_branch_tree, natural_cmp, BranchTreeKind, BranchTreeNode};
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
            "Current Branch",
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
pub fn references_come_before_folders_inside_folders_too() {
    let test_repo = TestRepo::branches_with_distinct_files();
    let master = test_repo.tip_oid("master");
    for name in ["topic/c", "topic/ui/sidebar", "topic/a", "topic/api/v2/auth", "topic/ui/search", "topic/b"] {
        test_repo.create_local_branch(name, master);
    }

    let nodes = collect_branch_tree(&test_repo.repository_manager().open().unwrap()).unwrap();

    let labels = labels(&nodes);
    let local = labels.iter().position(|label| label == "Local").unwrap();
    assert_eq!(
        vec![
            "Local",
            "  feature",
            "  master",
            "  topic/",
            "    a",
            "    b",
            "    c",
            // A folder with only one folder in it shares its row, and still comes after the references
            "    api/v2/",
            "      auth",
            "    ui/",
            "      search",
            "      sidebar",
        ],
        labels[local..]
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
}

#[test]
pub fn headers_without_references_and_symbolic_references_are_left_out() {
    let test_repo = TestRepo::branches_with_distinct_files();
    let repo = test_repo.repository_manager().open().unwrap();
    repo.reference_symbolic("refs/remotes/origin/HEAD", "refs/heads/master", false, "test").unwrap();

    let nodes = collect_branch_tree(&repo).unwrap();

    assert_eq!(
        vec!["All Branches", "Current Branch", "Local", "  feature", "  master"],
        labels(&nodes)
    );
}

#[test]
pub fn natural_order_ignores_case_and_compares_numbers_by_value() {
    assert_eq!(Ordering::Less, natural_cmp("issue-2", "issue-10"));
    assert_eq!(Ordering::Less, natural_cmp("Alpha", "beta"));
    assert_eq!(Ordering::Greater, natural_cmp("v0.10.0", "v0.9.1"));
    assert_eq!(Ordering::Less, natural_cmp("v1", "v1.0"));
}

/// Each row as in `labels`, with the highlighted part of its label in brackets.
fn highlighted_labels(nodes: &[BranchTreeNode]) -> Vec<String> {
    fn walk(nodes: &[BranchTreeNode], depth: usize, out: &mut Vec<String>) {
        for node in nodes {
            let label = match &node.highlight {
                Some(range) => format!(
                    "{}[{}]{}",
                    &node.label[..range.start],
                    &node.label[range.clone()],
                    &node.label[range.end..]
                ),
                None => node.label.clone(),
            };
            out.push(format!("{}{}", "  ".repeat(depth), label));
            walk(&node.children, depth + 1, out);
        }
    }
    let mut out = Vec::new();
    walk(nodes, 0, &mut out);
    out
}

fn filter_test_tree() -> Vec<BranchTreeNode> {
    let test_repo = TestRepo::branches_with_distinct_files();
    let master = test_repo.tip_oid("master");
    for name in ["Issue-2", "issue-10", "topic/login", "topic/diff", "user/me/fix/login"] {
        test_repo.create_local_branch(name, master);
    }
    test_repo.create_remote_branch("origin/topic/login", master);
    test_repo.create_remote_branch("origin/main", master);
    tag(&test_repo, "v1.0");
    collect_branch_tree(&test_repo.repository_manager().open().unwrap()).unwrap()
}

#[test]
pub fn filter_keeps_the_matching_rows_under_their_folders_and_headers() {
    let nodes = filter_branch_tree(&filter_test_tree(), "login");

    // All Branches, Current Branch and Tags have nothing to show
    assert_eq!(
        vec![
            "Local",
            "  topic/",
            "    [login]",
            "  user/me/fix/",
            "    [login]",
            "Remote",
            "  origin/",
            "    topic/",
            "      [login]",
        ],
        highlighted_labels(&nodes)
    );
}

#[test]
pub fn filter_matches_names_with_their_folders() {
    let tree = filter_test_tree();

    // A matching folder keeps every row in it
    assert_eq!(
        vec!["Local", "  [topic]/", "    diff", "    login", "Remote", "  origin/", "    [topic]/", "      login"],
        highlighted_labels(&filter_branch_tree(&tree, "topic"))
    );
    // A folder shared by other rows is highlighted only when its own name
    // matches, so a match across a folder highlights the part in the row
    assert_eq!(
        vec!["Local", "  topic/", "    [lo]gin", "Remote", "  origin/", "    topic/", "      [lo]gin"],
        highlighted_labels(&filter_branch_tree(&tree, "c/lo"))
    );
    assert_eq!(vec!["Remote", "  origin/", "    [m]ain"], highlighted_labels(&filter_branch_tree(&tree, "origin/m")));
}

#[test]
pub fn filter_ignores_case_and_can_match_nothing() {
    let tree = filter_test_tree();

    assert_eq!(
        vec!["Local", "  [Issue]-2", "  [issue]-10"],
        highlighted_labels(&filter_branch_tree(&tree, "ISSUE"))
    );
    assert_eq!(vec!["Tags", "  [v1].0"], highlighted_labels(&filter_branch_tree(&tree, "V1")));
    assert!(filter_branch_tree(&tree, "nothing-like-this").is_empty());
}
