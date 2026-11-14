//! Notes left on lines of a diff, kept per repository as a checklist to go
//! through before pushing.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::diff::{DiffLine, FileDiff, LineKind};
use crate::repository_manager::RepositoryManager;

/// Which version of a file the line of a note is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    /// A removed line, which only the old version has
    Old,
    New,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineNote {
    pub id: u64,
    /// The path of the file in the version of `side`
    pub path: String,
    pub side: Side,
    /// The line number in the version of `side`
    pub line: u32,
    /// The content of the line, to find it again once it has moved
    pub anchor: String,
    pub text: String,
    pub done: bool,
}

/// Where the notes of the repository whose git directory is `git_dir` are kept.
/// The git directory is not tracked, so the notes stay out of commits.
pub fn notes_path(git_dir: &Path) -> PathBuf {
    git_dir.join("metal-git").join("line-notes")
}

/// Gives where the notes of the current repository are kept, if it opens.
pub type NotesPath = Rc<dyn Fn() -> Option<PathBuf>>;

pub fn notes_path_of(repository_manager: &Rc<RepositoryManager>) -> NotesPath {
    let repository_manager = repository_manager.clone();
    Rc::new(move || {
        repository_manager
            .open()
            .ok()
            .map(|repo| notes_path(repo.path()))
    })
}

/// A missing or unreadable file means there are no notes. Lines that cannot
/// be read are left out.
pub fn load_notes(path: &Path) -> Vec<LineNote> {
    fs::read_to_string(path)
        .map(|text| text.lines().filter_map(parse_note).collect())
        .unwrap_or_default()
}

pub fn save_notes(path: &Path, notes: &[LineNote]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text: String = notes
        .iter()
        .map(|note| format!("{}\n", format_note(note)))
        .collect();
    fs::write(path, text)
}

/// Adds a note and returns it with its id. The file is read again first, so
/// that the notes another window changed in the meantime are kept, as with
/// the other changes below.
pub fn add_note(
    path: &Path,
    file_path: &str,
    side: Side,
    line: u32,
    anchor: &str,
    text: &str,
) -> io::Result<LineNote> {
    let mut notes = load_notes(path);
    let note = LineNote {
        id: notes.iter().map(|note| note.id).max().unwrap_or(0) + 1,
        path: file_path.to_string(),
        side,
        line,
        anchor: anchor.to_string(),
        text: text.to_string(),
        done: false,
    };
    notes.push(note.clone());
    save_notes(path, &notes)?;
    Ok(note)
}

pub fn set_note_done(path: &Path, id: u64, done: bool) -> io::Result<()> {
    let mut notes = load_notes(path);
    for note in notes.iter_mut().filter(|note| note.id == id) {
        note.done = done;
    }
    save_notes(path, &notes)
}

pub fn delete_note(path: &Path, id: u64) -> io::Result<()> {
    let mut notes = load_notes(path);
    notes.retain(|note| note.id != id);
    save_notes(path, &notes)
}

/// The side and line number a note on a line of a diff takes: a removed line
/// is in the old version, and any other in the new one.
pub fn side_of(line: &DiffLine) -> Option<(Side, u32)> {
    match line.kind {
        LineKind::Removed => line.old_lineno.map(|n| (Side::Old, n)),
        _ => line.new_lineno.map(|n| (Side::New, n)),
    }
}

/// Where the line of a note is in a file, as indices of a hunk and of a line
/// in it: the line on the side and number of the note if it still has its
/// content, or else the line with its content nearest to that number.
pub fn find_line(file: &FileDiff, note: &LineNote) -> Option<(usize, usize)> {
    let path = match note.side {
        Side::Old => file.old_path.as_deref(),
        Side::New => file.new_path.as_deref(),
    };
    if path != Some(note.path.as_str()) {
        return None;
    }

    let candidates = file
        .hunks
        .iter()
        .enumerate()
        .flat_map(|(h, hunk)| {
            hunk.lines
                .iter()
                .enumerate()
                .map(move |(l, line)| (h, l, line))
        })
        .filter_map(|(h, l, line)| match side_of(line) {
            Some((side, n)) if side == note.side && line.content == note.anchor => Some((h, l, n)),
            _ => None,
        });
    candidates
        .min_by_key(|&(_, _, n)| n.abs_diff(note.line))
        .map(|(h, l, _)| (h, l))
}

/// The notes of a file by where their lines are, as `find_line` gives it, in
/// the order they were added.
pub fn notes_by_line<'a>(
    notes: &'a [LineNote],
    file: &FileDiff,
) -> HashMap<(usize, usize), Vec<&'a LineNote>> {
    let mut by_line: HashMap<(usize, usize), Vec<&LineNote>> = HashMap::new();
    for note in notes {
        if let Some(at) = find_line(file, note) {
            by_line.entry(at).or_default().push(note);
        }
    }
    by_line
}

fn format_note(note: &LineNote) -> String {
    let side = match note.side {
        Side::Old => "old",
        Side::New => "new",
    };
    [
        note.id.to_string(),
        (if note.done { "done" } else { "open" }).to_string(),
        side.to_string(),
        note.line.to_string(),
        escape(&note.path),
        escape(&note.anchor),
        escape(&note.text),
    ]
    .join("\t")
}

fn parse_note(text: &str) -> Option<LineNote> {
    let fields: Vec<&str> = text.split('\t').collect();
    let [id, done, side, line, path, anchor, note_text] = fields.as_slice() else {
        return None;
    };
    Some(LineNote {
        id: id.parse().ok()?,
        done: *done == "done",
        side: match *side {
            "old" => Side::Old,
            "new" => Side::New,
            _ => return None,
        },
        line: line.parse().ok()?,
        path: unescape(path),
        anchor: unescape(anchor),
        text: unescape(note_text),
    })
}

/// Escapes the characters that separate fields and notes in the file.
fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => escaped.push_str("\\\\"),
            '\t' => escaped.push_str("\\t"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            c => escaped.push(c),
        }
    }
    escaped
}

fn unescape(text: &str) -> String {
    let mut unescaped = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            unescaped.push(c);
            continue;
        }
        match chars.next() {
            Some('t') => unescaped.push('\t'),
            Some('n') => unescaped.push('\n'),
            Some('r') => unescaped.push('\r'),
            Some(other) => unescaped.push(other),
            None => unescaped.push('\\'),
        }
    }
    unescaped
}
