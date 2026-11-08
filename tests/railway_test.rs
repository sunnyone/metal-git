extern crate git2;
extern crate tempdir;
extern crate metal_git;

mod util;
use crate::util::branch_test_repo::BranchTestRepo;
use crate::util::test_repo::TestRepo;
use metal_git::railway;

#[test]
pub fn collect_tree_flat_two() {
    let test_repo = TestRepo::flat_two();

    let graph = railway::collect_tree(&test_repo.repository_manager(), railway::HistoryScope::CurrentBranch).unwrap();
    let stations = (0..graph.len()).map(|i| graph.station(i).unwrap()).collect::<Vec<_>>();

    println!("Stations:");
    for station in stations.iter() {
        println!("{}", station.dump_tracks());
    }

    assert_eq!(2, stations.len());
    let (b, a) = (&stations[0], &stations[1]);
    assert_eq!("B", b.subject);
    assert_eq!("A", a.subject);

    assert_eq!("[ => 0* => 0]", b.dump_tracks());
    assert_eq!("[0 => 0* => ]", a.dump_tracks());
}


#[test]
pub fn collect_tree_two_parent_two_child() {
    let test_repo = TestRepo::two_parent_two_child();

    let graph = railway::collect_tree(&test_repo.repository_manager(), railway::HistoryScope::CurrentBranch).unwrap();
    let stations = (0..graph.len()).map(|i| graph.station(i).unwrap()).collect::<Vec<_>>();

    println!("Stations:");
    for station in stations.iter() {
        println!("{}", station.dump_tracks());
    }

    assert_eq!(4, stations.len());
    let (d, c, b, a) = (&stations[0], &stations[1], &stations[2], &stations[3]);
    assert_eq!("D", d.subject);
    assert_eq!("C", c.subject);
    assert_eq!("B", b.subject);
    assert_eq!("A", a.subject);

    assert_eq!("[ => 0* => 0,1] | [ => 1  => ]", d.dump_tracks());
    assert_eq!("[0 => 0* => 0] | [1 => 1  => 1]", c.dump_tracks());
    assert_eq!("[0 => 0  => 0] | [1 => 1* => 0]", b.dump_tracks());
    assert_eq!("[0 => 0* => ]", a.dump_tracks());
}

#[test]
pub fn collect_tree_branch_merge_branch_merge() {
    let test_repo = TestRepo::branch_merge_branch_merge();
    // test_repo.set_debug();

    let graph = railway::collect_tree(&test_repo.repository_manager(), railway::HistoryScope::CurrentBranch).unwrap();
    let stations = (0..graph.len()).map(|i| graph.station(i).unwrap()).collect::<Vec<_>>();

    println!("Stations:");
    for station in stations.iter() {
        println!("{}", station.dump_tracks());
    }

    assert_eq!(6, stations.len());
    let (f, e, d, c, b, a) = (&stations[0],
                              &stations[1],
                              &stations[2],
                              &stations[3],
                              &stations[4],
                              &stations[5]);
    assert_eq!("F", f.subject);
    assert_eq!("E", e.subject);
    assert_eq!("D", d.subject);
    assert_eq!("C", c.subject);
    assert_eq!("B", b.subject);
    assert_eq!("A", a.subject);

    assert_eq!("[ => 0* => 0,1,2] | [ => 1  => ] | [ => 2  => ]",
               f.dump_tracks());
    assert_eq!("[0 => 0  => 0] | [1 => 1  => 1] | [2 => 2* => 0]",
               e.dump_tracks());
    assert_eq!("[0 => 0* => 0,2] | [1 => 1  => 1] | [ => 2  => ]",
               d.dump_tracks());
    assert_eq!("[0 => 0  => 0] | [1 => 1* => 0] | [2 => 2  => 1]",
               c.dump_tracks());
    assert_eq!("[0 => 0  => 0] | [1 => 1* => 0]", b.dump_tracks());
    assert_eq!("[0 => 0* => ]", a.dump_tracks());
}

fn commit_at<'a>(graph: &'a railway::RailwayGraph, oid: git2::Oid) -> Option<&'a railway::RailwayCommit> {
    (0..graph.len()).map(|i| graph.commit(i).unwrap()).find(|commit| commit.oid == oid)
}

fn collect(test_repo: &TestRepo, scope: railway::HistoryScope) -> railway::RailwayGraph {
    railway::collect_tree(&test_repo.repository_manager(), scope).unwrap()
}

#[test]
pub fn current_branch_scope_hides_other_branches() {
    let test_repo = TestRepo::branches_with_distinct_files();

    let graph = collect(&test_repo, railway::HistoryScope::CurrentBranch);

    assert!(commit_at(&graph, test_repo.tip_oid("master")).is_some());
    assert!(commit_at(&graph, test_repo.tip_oid("feature")).is_none());
}

#[test]
pub fn all_branches_scope_includes_local_branches() {
    let test_repo = TestRepo::branches_with_distinct_files();

    let graph = collect(&test_repo, railway::HistoryScope::AllBranches);

    assert!(commit_at(&graph, test_repo.tip_oid("master")).is_some());
    assert!(commit_at(&graph, test_repo.tip_oid("feature")).is_some());
}

#[test]
pub fn all_branches_scope_includes_commits_only_on_a_remote_branch() {
    let test_repo = TestRepo::branches_with_distinct_files();
    let feature = test_repo.tip_oid("feature");
    test_repo.create_remote_branch("origin/topic", feature);
    let repo = test_repo.repository_manager().open().unwrap();
    repo.find_branch("feature", git2::BranchType::Local).unwrap().delete().unwrap();

    let graph = collect(&test_repo, railway::HistoryScope::AllBranches);

    assert_eq!(vec!["origin/topic".to_string()], commit_at(&graph, feature).unwrap().ref_names);
}

#[test]
pub fn all_branches_scope_includes_and_labels_commits_only_on_an_annotated_tag() {
    let test_repo = TestRepo::branches_with_distinct_files();
    let feature = test_repo.tip_oid("feature");
    let repo = test_repo.repository_manager().open().unwrap();
    let signature = git2::Signature::now("test", "test@example.com").unwrap();
    let target = repo.find_object(feature, None).unwrap();
    repo.tag("v1", &target, &signature, "release", false).unwrap();
    repo.find_branch("feature", git2::BranchType::Local).unwrap().delete().unwrap();

    let graph = collect(&test_repo, railway::HistoryScope::AllBranches);

    assert_eq!(vec!["v1".to_string()], commit_at(&graph, feature).unwrap().ref_names);
}

#[test]
pub fn all_branches_scope_excludes_stash() {
    let test_repo = TestRepo::branches_with_distinct_files();
    let feature = test_repo.tip_oid("feature");
    let repo = test_repo.repository_manager().open().unwrap();
    repo.reference("refs/stash", feature, false, "test").unwrap();
    repo.find_branch("feature", git2::BranchType::Local).unwrap().delete().unwrap();

    let graph = collect(&test_repo, railway::HistoryScope::AllBranches);

    assert!(commit_at(&graph, feature).is_none());
}

#[test]
pub fn only_the_head_commit_names_the_current_branch() {
    let test_repo = TestRepo::branches_with_distinct_files();

    let graph = collect(&test_repo, railway::HistoryScope::AllBranches);

    let master = commit_at(&graph, test_repo.tip_oid("master")).unwrap();
    let feature = commit_at(&graph, test_repo.tip_oid("feature")).unwrap();
    assert_eq!(Some("master".to_string()), master.head_ref_name);
    assert_eq!(None, feature.head_ref_name);
}

#[test]
pub fn detached_head_is_labelled_and_shown() {
    let test_repo = TestRepo::branches_with_distinct_files();
    let feature = test_repo.tip_oid("feature");
    let repo = test_repo.repository_manager().open().unwrap();
    repo.set_head_detached(feature).unwrap();

    let graph = collect(&test_repo, railway::HistoryScope::CurrentBranch);

    let head = commit_at(&graph, feature).unwrap();
    assert_eq!(Some("HEAD".to_string()), head.head_ref_name);
    assert!(head.ref_names.contains(&"HEAD".to_string()));
}

#[test]
pub fn all_branches_scope_keeps_children_above_a_parent_with_the_same_time() {
    let temp_dir = tempdir::TempDir::new("railway-same-time").unwrap();
    let repo = git2::Repository::init(temp_dir.path()).unwrap();
    repo.set_head("refs/heads/master").unwrap();
    let signature = git2::Signature::new("test", "test@example.com", &git2::Time::new(1000, 0)).unwrap();
    let tree_oid = repo.treebuilder(None).unwrap().write().unwrap();
    let tree = repo.find_tree(tree_oid).unwrap();
    let root = repo.commit(Some("HEAD"), &signature, &signature, "root", &tree, &[]).unwrap();
    let root_commit = repo.find_commit(root).unwrap();
    let mut children = Vec::new();
    for index in 0..8 {
        let name = format!("branch{}", index);
        repo.branch(&name, &root_commit, false).unwrap();
        let oid = repo
            .commit(Some(&format!("refs/heads/{}", name)), &signature, &signature, &name, &tree, &[&root_commit])
            .unwrap();
        children.push(oid);
    }
    let repository_manager = metal_git::repository_manager::RepositoryManager::new();
    repository_manager.set_work_dir_path(temp_dir.path().to_str().unwrap());

    let graph = railway::collect_tree(&repository_manager, railway::HistoryScope::AllBranches).unwrap();

    assert_eq!(9, graph.len());
    assert_eq!(root, graph.commit(8).unwrap().oid);
}
