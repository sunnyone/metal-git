extern crate git2;
extern crate tempdir;
extern crate metal_git;

mod util;
use crate::util::test_repo::TestRepo;
use metal_git::railway;

#[test]
pub fn collect_tree_flat_two() {
    let test_repo = TestRepo::flat_two();

    let stations = railway::collect_tree(&test_repo.repository_manager()).unwrap();

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

    let stations = railway::collect_tree(&test_repo.repository_manager()).unwrap();

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

    let stations = railway::collect_tree(&test_repo.repository_manager()).unwrap();

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
