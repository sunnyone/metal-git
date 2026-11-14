extern crate git2;
extern crate metal_git;
extern crate tempdir;

use git2::{DiffOptions, Patch};
use metal_git::diff::{self, FileDiff};
use metal_git::line_notes::{self, LineNote, Side};
use tempdir::TempDir;

fn diff_buffers(old: &str, new: &str) -> FileDiff {
	let mut opts = DiffOptions::new();
	let patch = Patch::from_buffers(
		old.as_bytes(),
		Some("a.rs".as_ref()),
		new.as_bytes(),
		Some("a.rs".as_ref()),
		Some(&mut opts),
	)
	.unwrap();
	diff::file_from_patch(&patch).unwrap()
}

fn note(side: Side, line: u32, anchor: &str) -> LineNote {
	LineNote {
		id: 1,
		path: "a.rs".to_string(),
		side,
		line,
		anchor: anchor.to_string(),
		text: "check".to_string(),
		done: false,
	}
}

#[test]
pub fn notes_are_saved_with_any_characters_and_read_back() {
	let dir = TempDir::new("line-notes").unwrap();
	let path = line_notes::notes_path(dir.path());

	let first = line_notes::add_note(&path, "src/a b.rs", Side::New, 3, "let x\t= 1;", "two\nlines \\ tab\t").unwrap();
	let second = line_notes::add_note(&path, "a.rs", Side::Old, 7, "", "removed?").unwrap();

	assert_eq!((1, 2), (first.id, second.id));
	assert_eq!(vec![first, second], line_notes::load_notes(&path));
}

#[test]
pub fn notes_are_marked_done_and_deleted() {
	let dir = TempDir::new("line-notes").unwrap();
	let path = line_notes::notes_path(dir.path());
	line_notes::add_note(&path, "a.rs", Side::New, 1, "a", "one").unwrap();
	line_notes::add_note(&path, "a.rs", Side::New, 2, "b", "two").unwrap();

	line_notes::set_note_done(&path, 1, true).unwrap();
	line_notes::delete_note(&path, 2).unwrap();

	let notes = line_notes::load_notes(&path);
	assert_eq!(1, notes.len());
	assert!(notes[0].done);
}

#[test]
pub fn a_missing_file_has_no_notes() {
	let dir = TempDir::new("line-notes").unwrap();

	assert!(line_notes::load_notes(&dir.path().join("missing")).is_empty());
}

#[test]
pub fn a_note_finds_its_line_by_side_number_and_content() {
	let file = diff_buffers("a\nb\nc\n", "a\nB\nc\n");

	let removed = line_notes::find_line(&file, &note(Side::Old, 2, "b"));
	let added = line_notes::find_line(&file, &note(Side::New, 2, "B"));
	let context = line_notes::find_line(&file, &note(Side::New, 3, "c"));

	let content = |(h, l): (usize, usize)| file.hunks[h].lines[l].content.clone();
	assert_eq!(Some("b".to_string()), removed.map(content));
	assert_eq!(Some("B".to_string()), added.map(content));
	assert_eq!(Some("c".to_string()), context.map(content));
}

#[test]
pub fn a_note_follows_its_line_once_it_moved() {
	let file = diff_buffers("a\nb\nc\n", "x\ny\na\nB\nc\n");

	let found = line_notes::find_line(&file, &note(Side::New, 2, "B")).unwrap();

	assert_eq!(Some(4), file.hunks[found.0].lines[found.1].new_lineno);
}

#[test]
pub fn a_note_on_a_line_that_changed_or_another_file_is_not_found() {
	let file = diff_buffers("a\nb\nc\n", "a\nB\nc\n");
	let mut other_file = note(Side::New, 2, "B");
	other_file.path = "b.rs".to_string();

	assert_eq!(None, line_notes::find_line(&file, &note(Side::New, 2, "changed")));
	assert_eq!(None, line_notes::find_line(&file, &other_file));
}
