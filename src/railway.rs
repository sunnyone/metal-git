use std::collections::{BTreeSet, HashMap};
use std::collections::hash_map::Entry::{Occupied, Vacant};
use std::cell::RefCell;

use chrono::*;

use git2::{Error, Oid};
use crate::repository_manager::RepositoryManager;

#[derive(Clone, PartialEq)]
pub struct RailwayTrack {
    pub line_number: LineNumber,
    pub track_number: TrackNumber,
    pub is_active: bool,
    pub from_tracks: Vec<TrackNumber>,
    pub to_tracks: RefCell<Vec<TrackNumber>>,
    pub to_lines: Vec<LineNumber>,
}

#[derive(Clone, PartialEq)]
pub struct RailwayStation {
    pub tracks: Vec<RailwayTrack>,
    pub oid: Oid,
    pub subject: String,
    pub ref_names: Vec<String>,
    pub author_name: String,
    pub time: String,

    active_track_index: usize,
}

impl RailwayTrack {
    fn new(line_number: LineNumber,
           track_number: TrackNumber,
           is_active: bool,
           from_tracks: Vec<TrackNumber>,
           to_lines: Vec<LineNumber>)
           -> RailwayTrack {
        RailwayTrack {
            line_number,
            track_number,
            is_active,
            from_tracks,
            to_lines,
            to_tracks: RefCell::new(Vec::new()),
        }
    }

    pub fn dump_from_to(&self) -> String {
        let from_str = self.from_tracks
                           .iter()
                           .map(|x| format!("{}", x.0))
                           .collect::<Vec<_>>()
                           .join(",");
        let to_str = self.to_tracks
                         .borrow()
                         .iter()
                         .map(|x| format!("{}", x.0))
                         .collect::<Vec<_>>()
                         .join(",");

        let active_char = if self.is_active {
            '*'
        } else {
            ' '
        };
        format!("[{} => {}{} => {}]",
                from_str,
                self.track_number.0,
                active_char,
                to_str)
    }
}

impl RailwayStation {
    fn new(commit: &RailwayCommit, tracks: Vec<RailwayTrack>) -> RailwayStation {
        let active_track_index = tracks.iter().position(|x| x.is_active).unwrap();

        RailwayStation {
            tracks,
            active_track_index,
            oid: commit.oid,
            subject: commit.subject.clone(),
            ref_names: commit.ref_names.clone(),
            author_name: commit.author_name.clone(),
            time: commit.time.clone(),
        }
    }

    pub fn dump_tracks(&self) -> String {
        self.tracks.iter().map(|x| x.dump_from_to()).collect::<Vec<_>>().join(" | ")
    }

    pub fn active_track(&self) -> &RailwayTrack {
        self.tracks.get(self.active_track_index).unwrap()
    }
}

#[derive(PartialEq, Eq, Hash, PartialOrd, Ord, Clone, Debug, Copy)]
pub struct LineNumber(usize);
impl LineNumber {
    pub fn next_number(&self) -> LineNumber {
        LineNumber(self.0 + 1)
    }
}

#[derive(PartialEq, Eq, Hash, PartialOrd, Ord, Clone, Debug, Copy)]
pub struct TrackNumber(usize);
impl TrackNumber {
    pub fn as_usize(&self) -> usize {
        self.0
    }
    pub fn next_number(&self) -> TrackNumber {
        TrackNumber(self.0 + 1)
    }
    pub fn prev_number(&self) -> TrackNumber {
        TrackNumber(self.0 - 1)
    }
}

/// Commit information shown in a history row.
#[derive(Clone, PartialEq)]
pub struct RailwayCommit {
    pub oid: Oid,
    pub subject: String,
    pub ref_names: Vec<String>,
    pub author_name: String,
    pub time: String,
}

impl RailwayCommit {
    fn new(commit: &git2::Commit, ref_names: Vec<String>) -> RailwayCommit {
        let message = commit.message().unwrap_or("");
        let first_line = message.lines().next().unwrap_or("");

        let time = commit.time();
        let commit_time = FixedOffset::east_opt(time.offset_minutes() * 60).unwrap().timestamp_opt(time.seconds(), 0).unwrap();

        RailwayCommit {
            oid: commit.id(),
            subject: first_line.to_string(),
            ref_names,
            author_name: commit.author().name().unwrap_or("").to_string(),
            time: format!("{}", commit_time.format("%Y-%m-%d %H:%M:%S %Z")),
        }
    }
}

/// Rows between stored snapshots of the lines present in a row.
const SNAPSHOT_INTERVAL: usize = 64;

/// History graph stored as line intervals.
///
/// A line is a run of the graph drawn in one column. Line numbers only grow, and
/// each line is present in one contiguous range of rows. Because new lines always
/// take the rightmost track and tracks are compacted without reordering when a
/// line ends, the track of a line in a row is its rank among the lines present in
/// that row. Rows are therefore built on demand from the intervals instead of
/// storing every track of every row.
#[derive(Default)]
pub struct RailwayGraph {
    commits: Vec<RailwayCommit>,
    /// Line the commit's dot is on.
    active_lines: Vec<LineNumber>,
    /// Lines the commit connects to below, in parent order. parent_line_offsets[i]
    /// ..parent_line_offsets[i + 1] indexes parent_lines for row i.
    parent_line_offsets: Vec<usize>,
    parent_lines: Vec<LineNumber>,
    /// Lines first present in row i are first_lines[i]..first_lines[i + 1].
    first_lines: Vec<usize>,
    /// Line that is present in row i for the last time, if any. Only the active
    /// line of a row can end there.
    ended_lines: Vec<Option<LineNumber>>,
    lane_counts: Vec<usize>,
    line_first_rows: Vec<usize>,
    /// Sorted lines present in row k * SNAPSHOT_INTERVAL.
    snapshots: Vec<Vec<LineNumber>>,
}

impl RailwayGraph {
    pub fn len(&self) -> usize {
        self.commits.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commits.is_empty()
    }

    pub fn commits(&self) -> &[RailwayCommit] {
        &self.commits
    }

    pub fn commit(&self, index: usize) -> Option<&RailwayCommit> {
        self.commits.get(index)
    }

    /// Number of tracks in a row.
    pub fn lane_count(&self, index: usize) -> usize {
        self.lane_counts.get(index).copied().unwrap_or(0)
    }

    /// Builds the tracks of a row.
    pub fn station(&self, index: usize) -> Option<RailwayStation> {
        let commit = self.commits.get(index)?;
        let lines = self.lines_in_row(index);
        let next_lines = if index + 1 < self.len() {
            let mut next_lines = lines.clone();
            self.advance_row(&mut next_lines, index + 1);
            Some(next_lines)
        } else {
            None
        };

        let active_line = self.active_lines[index];
        let parent_lines = &self.parent_lines[self.parent_line_offsets[index]..self.parent_line_offsets[index + 1]];
        let tracks = lines
            .iter()
            .enumerate()
            .map(|(track, line)| {
                let track_number = TrackNumber(track);
                let is_continued = self.line_first_rows[line.0] < index;
                let from_tracks = if is_continued { vec![track_number] } else { Vec::new() };
                let is_active = *line == active_line;
                let to_lines = if is_active {
                    parent_lines.to_vec()
                } else if is_continued {
                    vec![*line]
                } else {
                    Vec::new()
                };
                let mut railway_track = RailwayTrack::new(*line, track_number, is_active, from_tracks, Vec::new());
                if let Some(next_lines) = next_lines.as_ref() {
                    *railway_track.to_tracks.borrow_mut() = to_lines
                        .iter()
                        .map(|to_line| TrackNumber(next_lines.binary_search(to_line).expect("line continues to the next row")))
                        .collect();
                }
                railway_track.to_lines = to_lines;
                railway_track
            })
            .collect();

        Some(RailwayStation::new(commit, tracks))
    }

    fn lines_in_row(&self, index: usize) -> Vec<LineNumber> {
        let snapshot_row = index / SNAPSHOT_INTERVAL * SNAPSHOT_INTERVAL;
        let mut lines = self.snapshots[index / SNAPSHOT_INTERVAL].clone();
        for row in snapshot_row + 1..=index {
            self.advance_row(&mut lines, row);
        }
        lines
    }

    /// Turns the lines present in row - 1 into the lines present in row.
    fn advance_row(&self, lines: &mut Vec<LineNumber>, row: usize) {
        if let Some(ended_line) = self.ended_lines[row - 1] {
            if let Ok(position) = lines.binary_search(&ended_line) {
                lines.remove(position);
            }
        }
        lines.extend((self.first_lines[row]..self.first_lines[row + 1]).map(LineNumber));
    }
}

/// Assigns lines to commits while walking the history.
struct LineAssigner<'a> {
    graph: &'a mut RailwayGraph,
    oid_line_map: HashMap<Oid, LineNumber>,
    present_lines: BTreeSet<LineNumber>,
}

impl<'a> LineAssigner<'a> {
    fn take_line_number_or_acquire(&mut self, oid: &Oid, row: usize) -> LineNumber {
        if let Some(line_number) = self.oid_line_map.remove(oid) {
            return line_number;
        }

        let line_number = LineNumber(self.graph.line_first_rows.len());
        self.graph.line_first_rows.push(row);
        self.present_lines.insert(line_number);
        line_number
    }

    fn add_commit(&mut self, commit: &git2::Commit, ref_names: Vec<String>) {
        let row = self.graph.commits.len();
        let oid = commit.id();
        self.graph.first_lines.push(self.graph.line_first_rows.len());

        let active_line_number = self.take_line_number_or_acquire(&oid, row);

        let mut is_first_non_merge = true;
        for parent_id in commit.parent_ids() {
            let parent_line_number = if self.oid_line_map.contains_key(&parent_id) {
                self.take_line_number_or_acquire(&parent_id, row)
            } else if is_first_non_merge {
                is_first_non_merge = false;
                // first parent uses this active line
                active_line_number
            } else {
                self.take_line_number_or_acquire(&parent_id, row)
            };

            self.oid_line_map.insert(parent_id, parent_line_number);
            self.graph.parent_lines.push(parent_line_number);
        }

        self.graph.active_lines.push(active_line_number);
        self.graph.parent_line_offsets.push(self.graph.parent_lines.len());
        self.graph.lane_counts.push(self.present_lines.len());
        if row.is_multiple_of(SNAPSHOT_INTERVAL) {
            self.graph.snapshots.push(self.present_lines.iter().copied().collect());
        }

        let parent_lines = &self.graph.parent_lines[self.graph.parent_line_offsets[row]..];
        if parent_lines.contains(&active_line_number) {
            self.graph.ended_lines.push(None);
        } else {
            self.present_lines.remove(&active_line_number);
            self.graph.ended_lines.push(Some(active_line_number));
        }

        self.graph.commits.push(RailwayCommit::new(commit, ref_names));
    }

    fn finish(self) {
        self.graph.first_lines.push(self.graph.line_first_rows.len());
    }
}

struct RefTable {
    oid_table: HashMap<Oid, Vec<String>>,
}

impl RefTable {
    fn collect(repo: &git2::Repository) -> Result<RefTable, Error> {
        let mut table = HashMap::<Oid, Vec<String>>::new();
        let refs = repo.references()?;
        for r in refs {
            let r = r?;
            if let Some(oid) = r.target() {
                if let Some(shorthand) = r.shorthand() {
                    match table.entry(oid) {
                        Occupied(mut entry) => {
                            entry.get_mut().push(shorthand.to_owned());
                        }
                        Vacant(entry) => {
                            entry.insert(vec![shorthand.to_owned()]);
                        }
                    }
                }
            }
        }

        Ok(RefTable { oid_table: table })
    }

    fn get_names_for_oid(&self, oid: &Oid) -> Vec<String> {
        self.oid_table
            .get(oid).cloned()
            .unwrap_or_else(Vec::new)
    }
}

pub fn collect_tree(repository_manager: &RepositoryManager) -> Result<RailwayGraph, Error> {
    let repo = repository_manager.open()?;

    let ref_table = RefTable::collect(&repo)?;

    let mut revwalk = repo.revwalk()?;

    revwalk.set_sorting(git2::Sort::TIME)?;
    revwalk.push_head()?;

    let mut graph = RailwayGraph::default();
    graph.parent_line_offsets.push(0);
    let mut assigner = LineAssigner {
        graph: &mut graph,
        oid_line_map: HashMap::new(),
        present_lines: BTreeSet::new(),
    };

    for oid in revwalk {
        let oid = oid?;
        let commit = repo.find_commit(oid)?;
        let ref_names = ref_table.get_names_for_oid(&oid);
        assigner.add_commit(&commit, ref_names);
    }
    assigner.finish();

    Ok(graph)
}
