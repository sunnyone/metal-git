use std::collections::{HashMap, HashSet};
use std::collections::hash_map::Entry::{Occupied, Vacant};
use std::cell::RefCell;

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

#[derive(PartialEq)]
pub struct RailwayStation {
    pub tracks: Vec<RailwayTrack>,
    pub oid: Oid,
    pub subject: String,
    pub ref_names: Vec<String>,

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
            line_number: line_number,
            track_number: track_number,
            is_active: is_active,
            from_tracks: from_tracks,
            to_lines: to_lines,
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
    fn new(commit: &git2::Commit,
           ref_names: Vec<String>,
           tracks: Vec<RailwayTrack>)
           -> RailwayStation {
        let active_track_index = tracks.iter().position(|x| x.is_active).unwrap();

        let mut message_lines = commit.message().unwrap_or("").lines();
        let first_line = message_lines.next().unwrap_or("");

        RailwayStation {
            tracks: tracks,
            active_track_index: active_track_index,
            oid: commit.id(),
            subject: first_line.to_string(),
            ref_names: ref_names,
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

struct TrackLineMap {
    max_line_number: LineNumber,
    oid_line_map: HashMap<Oid, LineNumber>,
    line_track_map: HashMap<LineNumber, TrackNumber>,
}

impl TrackLineMap {
    fn new() -> TrackLineMap {
        TrackLineMap {
            max_line_number: LineNumber(0),
            oid_line_map: HashMap::new(),
            line_track_map: HashMap::new(),
        }
    }

    fn acquire_line_number(&mut self) -> LineNumber {
        let line_number = self.max_line_number;
        self.max_line_number.0 += 1;
        line_number
    }

    fn is_oid_assigned(&self, oid: &Oid) -> bool {
        self.oid_line_map.get(oid).is_some()
    }

    fn take_line_number_or_aquire(&mut self, oid: &Oid) -> LineNumber {
        let line_number = match self.oid_line_map.remove(oid) {
            Some(line_number) => line_number,
            None => self.acquire_line_number(), // this require mut
        };

        self.assign_track_number_if_required(&line_number);

        line_number
    }

    fn set_next_oid(&mut self, line_number: LineNumber, oid: Oid) {
        self.oid_line_map.insert(oid, line_number);
    }

    fn assign_track_number_if_required(&mut self, line_number: &LineNumber) {
        if self.line_track_map.get(line_number).is_some() {
            return;
        }

        let track_number = self.line_track_map
                               .values()
                               .max()
                               .map(|x| (*x).next_number())
                               .unwrap_or_else(|| TrackNumber(0));
        self.line_track_map.insert(*line_number, track_number);

        return;
    }

    fn line_track_numbers(&self) -> Vec<(&LineNumber, &TrackNumber)> {
        let mut line_track_vec = self.line_track_map.iter().collect::<Vec<_>>();
        line_track_vec.sort_by(|a, b| a.1.cmp(b.1));

        line_track_vec
    }

    fn convert_line_to_track(&self, line: &LineNumber) -> Option<TrackNumber> {
        self.line_track_map.get(line).map(|x| *x)
    }

    fn vacuum_unused_track_numbers<'a, T>(&mut self, to_line_numbers: T)
        where T: Iterator<Item = &'a LineNumber>
    {
        let to_lines = to_line_numbers.map(|x| *x).collect::<HashSet<_>>();
        let all_lines = self.line_track_map.keys().map(|x| *x).collect::<HashSet<_>>();
        let unused_lines = all_lines.difference(&to_lines).map(|x| x).collect::<Vec<_>>();

        // TODO: performance improvement
        for unused_line in &unused_lines {
            let unused_track = *self.line_track_map.get(unused_line).unwrap();

            for (_line, track) in self.line_track_map.iter_mut() {
                if unused_track.0 < track.0 {
                    *track = track.prev_number();
                }
            }
        }

        for unused_line in unused_lines {
            self.line_track_map.remove(unused_line);
        }
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
            .get(oid)
            .map(|x| x.clone())
            .unwrap_or_else(|| Vec::new())
    }
}

pub fn collect_tree(repository_manager: &RepositoryManager) -> Result<Vec<RailwayStation>, Error> {
    let repo = repository_manager.open()?;

    let ref_table = RefTable::collect(&repo)?;

    let mut revwalk = repo.revwalk()?;

    revwalk.set_sorting(git2::Sort::TIME)?;
    revwalk.push_head()?;

    let mut track_line_map = TrackLineMap::new();

    let mut stations = Vec::<RailwayStation>::new();
    for oid in revwalk {
        let oid = oid?;
        let mut prev_to_map = HashMap::new();
        if let Some(last_station) = stations.last() {
            track_line_map.vacuum_unused_track_numbers(last_station.tracks
                                                                   .iter()
                                                                   .flat_map(|x| &x.to_lines));

            for track in &last_station.tracks {
                let mut to_tracks = Vec::new();

                for to_line in track.to_lines.iter() {
                    let to_track = track_line_map.convert_line_to_track(&to_line).unwrap();

                    prev_to_map.insert(*to_line, to_track);
                    to_tracks.push(to_track);
                }

                *track.to_tracks.borrow_mut() = to_tracks;
            }
        }


        let commit = repo.find_commit(oid)?;
        let active_line_number = track_line_map.take_line_number_or_aquire(&oid);

        let mut is_first_non_merge = true;
        let mut active_to_line_numbers = Vec::<LineNumber>::new();
        for parent_id in commit.parent_ids() {
            let parent_line_number = if track_line_map.is_oid_assigned(&parent_id) {
                track_line_map.take_line_number_or_aquire(&parent_id)
            } else if is_first_non_merge {
                is_first_non_merge = false;
                // first parent uses this active line
                active_line_number
            } else {
                track_line_map.take_line_number_or_aquire(&parent_id)
            };

            track_line_map.set_next_oid(parent_line_number, parent_id);
            active_to_line_numbers.push(parent_line_number);
        }

        let tracks = track_line_map.line_track_numbers()
                                   .iter()
                                   .map(|&(line_number, track_number)| {
                                       let prev_to_track = prev_to_map.get(&line_number);
                                       let mut from_lines = Vec::<LineNumber>::new();
                                       let mut from_tracks = Vec::<TrackNumber>::new();
                                       if let Some(prev_to_track) = prev_to_track {
                                           from_lines.push(*line_number);
                                           from_tracks.push(*prev_to_track);
                                       }

                                       if *line_number == active_line_number {
                                           RailwayTrack::new(*line_number,
                                                             *track_number,
                                                             true,
                                                             from_tracks,
                                                             active_to_line_numbers.clone())
                                       } else {
                                           RailwayTrack::new(*line_number,
                                                             *track_number,
                                                             false,
                                                             from_tracks.clone(),
                                                             from_lines)
                                       }
                                   })
                                   .collect::<Vec<_>>();

        let ref_names = ref_table.get_names_for_oid(&oid);
        stations.push(RailwayStation::new(&commit, ref_names, tracks));
    }

    // 	for station in stations.iter() {
    // 		println!("{}", station.dump_tracks());
    // 	}
    Ok(stations)
}
