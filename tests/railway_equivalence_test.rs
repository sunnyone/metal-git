extern crate git2;
extern crate metal_git;
extern crate tempdir;

// Checks collect_tree against a straightforward reference implementation of the
// railway layout (the original per-row algorithm) on generated histories.

mod util;
use crate::util::test_repo::TestRepo;
use git2::{Commit, Oid, Repository, Signature, Sort};
use metal_git::railway;
use metal_git::repository_manager::RepositoryManager;
use std::collections::HashMap;
use tempdir::TempDir;

/// Original layout algorithm: assigns every line a track per row, compacting track
/// numbers when lines end. Returns RailwayStation::dump_tracks()-formatted rows.
fn reference_dumps(repo: &Repository) -> Vec<String> {
    struct Track {
        track: usize,
        is_active: bool,
        from_tracks: Vec<usize>,
        to_lines: Vec<usize>,
        to_tracks: Vec<usize>,
    }

    let mut revwalk = repo.revwalk().unwrap();
    revwalk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME).unwrap();
    revwalk.push_head().unwrap();

    let mut max_line = 0;
    let mut oid_line = HashMap::<Oid, usize>::new();
    let mut line_track = HashMap::<usize, usize>::new();
    let mut rows: Vec<Vec<Track>> = Vec::new();

    let mut take = |oid: &Oid, oid_line: &mut HashMap<Oid, usize>, line_track: &mut HashMap<usize, usize>| {
        let line = oid_line.remove(oid).unwrap_or_else(|| {
            max_line += 1;
            max_line - 1
        });
        if !line_track.contains_key(&line) {
            let track = line_track.values().max().map(|x| x + 1).unwrap_or(0);
            line_track.insert(line, track);
        }
        line
    };

    for oid in revwalk {
        let oid = oid.unwrap();
        let mut prev_to = HashMap::new();
        if let Some(last) = rows.last_mut() {
            let keep = last.iter().flat_map(|t| t.to_lines.iter().copied()).collect::<Vec<_>>();
            let unused = line_track.keys().copied().filter(|l| !keep.contains(l)).collect::<Vec<_>>();
            for unused_line in &unused {
                let unused_track = line_track[unused_line];
                for track in line_track.values_mut() {
                    if unused_track < *track {
                        *track -= 1;
                    }
                }
            }
            for unused_line in &unused {
                line_track.remove(unused_line);
            }
            for track in last.iter_mut() {
                track.to_tracks = track.to_lines.iter().map(|l| line_track[l]).collect();
                for (line, to_track) in track.to_lines.iter().zip(&track.to_tracks) {
                    prev_to.insert(*line, *to_track);
                }
            }
        }

        let commit = repo.find_commit(oid).unwrap();
        let active = take(&oid, &mut oid_line, &mut line_track);
        let mut is_first_non_merge = true;
        let mut active_to = Vec::new();
        for parent in commit.parent_ids() {
            let parent_line = if oid_line.contains_key(&parent) {
                take(&parent, &mut oid_line, &mut line_track)
            } else if is_first_non_merge {
                is_first_non_merge = false;
                active
            } else {
                take(&parent, &mut oid_line, &mut line_track)
            };
            oid_line.insert(parent, parent_line);
            active_to.push(parent_line);
        }

        let mut lines = line_track.iter().map(|(l, t)| (*l, *t)).collect::<Vec<_>>();
        lines.sort_by_key(|(_, t)| *t);
        rows.push(
            lines
                .into_iter()
                .map(|(line, track)| {
                    let from_tracks = prev_to.get(&line).map(|t| vec![*t]).unwrap_or_default();
                    let is_active = line == active;
                    let to_lines = if is_active {
                        active_to.clone()
                    } else if from_tracks.is_empty() {
                        Vec::new()
                    } else {
                        vec![line]
                    };
                    Track { track, is_active, from_tracks, to_lines, to_tracks: Vec::new() }
                })
                .collect(),
        );
    }

    let join = |v: &[usize]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",");
    rows.iter()
        .map(|row| {
            row.iter()
                .map(|t| {
                    format!(
                        "[{} => {}{} => {}]",
                        join(&t.from_tracks),
                        t.track,
                        if t.is_active { '*' } else { ' ' },
                        join(&t.to_tracks)
                    )
                })
                .collect::<Vec<_>>()
                .join(" | ")
        })
        .collect()
}

fn collect_tree_dumps(repository_manager: &RepositoryManager) -> Vec<String> {
    let graph = railway::collect_tree(repository_manager).unwrap();
    (0..graph.len()).map(|index| graph.station(index).unwrap().dump_tracks()).collect()
}

fn assert_same_layout(repository_manager: &RepositoryManager) {
    let repo = repository_manager.open().unwrap();
    let expected = reference_dumps(&repo);
    let actual = collect_tree_dumps(repository_manager);
    assert_eq!(expected.len(), actual.len());
    for (index, (expected, actual)) in expected.iter().zip(&actual).enumerate() {
        assert_eq!(expected, actual, "row {index}");
    }
}

/// Small deterministic random number generator so generated histories are stable.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self, bound: usize) -> usize {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 33) % bound as u64) as usize
    }
}

/// Builds a history with parallel branches, merges, octopus merges and optionally
/// commits whose time is older than their parent's (clock skew).
fn generated_repo(
    seed: u64,
    commits: usize,
    max_branches: usize,
    merge_percent: usize,
    clock_skew: bool,
) -> (TempDir, std::rc::Rc<RepositoryManager>) {
    let temp_dir = TempDir::new("railway-equivalence").unwrap();
    let repo = Repository::init(temp_dir.path()).unwrap();
    let tree_oid = repo.treebuilder(None).unwrap().write().unwrap();
    let tree = repo.find_tree(tree_oid).unwrap();
    let mut rng = Lcg(seed);
    let mut time = 1_600_000_000i64;

    let mut commit = |parents: &[Oid], message: &str, rng: &mut Lcg| -> Oid {
        time += 60;
        let commit_time = if clock_skew && rng.next(10) == 0 { time - 3600 } else { time };
        let signature = Signature::new("test", "test@example.com", &git2::Time::new(commit_time, 0)).unwrap();
        let parents = parents.iter().map(|p| repo.find_commit(*p).unwrap()).collect::<Vec<_>>();
        let parent_refs = parents.iter().collect::<Vec<&Commit>>();
        repo.commit(None, &signature, &signature, message, &tree, &parent_refs).unwrap()
    };

    let mut main = commit(&[], "root", &mut rng);
    let mut branches: Vec<Oid> = Vec::new();
    for n in 0..commits {
        let roll = rng.next(100);
        if branches.len() < max_branches && roll < 15 {
            branches.push(commit(&[main], &format!("start {n}"), &mut rng));
        } else if !branches.is_empty() && roll < 15 + merge_percent {
            let tip = branches.swap_remove(rng.next(branches.len()));
            main = commit(&[main, tip], &format!("merge {n}"), &mut rng);
        } else if branches.len() >= 2 && roll < 18 + merge_percent {
            let a = branches.swap_remove(rng.next(branches.len()));
            let b = branches.swap_remove(rng.next(branches.len()));
            main = commit(&[main, a, b], &format!("octopus {n}"), &mut rng);
        } else if !branches.is_empty() && roll < 85 {
            let index = rng.next(branches.len());
            branches[index] = commit(&[branches[index]], &format!("work {n}"), &mut rng);
        } else {
            main = commit(&[main], &format!("main {n}"), &mut rng);
        }
    }
    for (n, tip) in branches.clone().into_iter().enumerate() {
        main = commit(&[main, tip], &format!("final merge {n}"), &mut rng);
    }
    repo.reference("refs/heads/master", main, true, "test head").unwrap();
    repo.set_head("refs/heads/master").unwrap();

    let repository_manager = RepositoryManager::new();
    repository_manager.set_work_dir_path(temp_dir.path().to_str().unwrap());
    (temp_dir, repository_manager)
}

#[test]
fn should_match_reference_layout_for_fixture_repositories() {
    for test_repo in [
        TestRepo::single(),
        TestRepo::flat_two(),
        TestRepo::two_parent_two_child(),
        TestRepo::branch_merge_branch_merge(),
    ] {
        assert_same_layout(&test_repo.repository_manager());
    }
}

#[test]
fn should_match_reference_layout_for_generated_histories() {
    for seed in 1..=20 {
        let (_temp_dir, repository_manager) = generated_repo(seed, 200, 12, 10, false);
        assert_same_layout(&repository_manager);
    }
}

#[test]
fn should_match_reference_layout_with_many_lanes() {
    let (_temp_dir, repository_manager) = generated_repo(42, 1500, 150, 1, false);
    assert_same_layout(&repository_manager);
}

#[test]
fn should_match_reference_layout_with_clock_skew() {
    for seed in 100..=110 {
        let (_temp_dir, repository_manager) = generated_repo(seed, 200, 12, 10, true);
        assert_same_layout(&repository_manager);
    }
}
