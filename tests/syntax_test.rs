extern crate git2;
extern crate metal_git;

use git2::{DiffOptions, Patch};
use metal_git::diff::{self, DiffLine, FileDiff, LineKind, Reveal, Segment, Token};

fn diff_named(name: &str, old: &str, new: &str) -> FileDiff {
	let mut opts = DiffOptions::new();
	opts.context_lines(3);
	let patch = Patch::from_buffers(
		old.as_bytes(),
		Some(name.as_ref()),
		new.as_bytes(),
		Some(name.as_ref()),
		Some(&mut opts),
	)
	.unwrap();
	diff::file_from_patch(&patch).unwrap()
}

/// The text of each token of a line with its kind.
fn tokens<'a>(line: &'a DiffLine) -> Vec<(&'a str, Token)> {
	line.syntax
		.iter()
		.map(|(range, token)| (&line.content[range.clone()], *token))
		.collect()
}

fn token_of(line: &DiffLine, text: &str) -> Option<Token> {
	tokens(line)
		.into_iter()
		.find(|(t, _)| *t == text)
		.map(|(_, token)| token)
}

#[test]
pub fn lines_have_no_tokens_until_highlighted() {
	let mut file = diff_named("a.rs", "fn a() {}\n", "fn b() {}\n");
	assert!(!file.highlighted);
	assert!(file.hunks[0].lines.iter().all(|l| l.syntax.is_empty()));

	file.highlight();
	assert!(file.highlighted);
	assert!(file.hunks[0].lines.iter().all(|l| !l.syntax.is_empty()));
}

#[test]
pub fn tokens_of_a_rust_line_have_their_kinds() {
	let mut file = diff_named(
		"a.rs",
		"",
		"fn main() { let s = \"hi\"; let n = 42; } // done\n",
	);
	file.highlight();
	let line = &file.hunks[0].lines[0];

	assert_eq!(Some(Token::Keyword), token_of(line, "fn"));
	assert_eq!(Some(Token::Function), token_of(line, "main"));
	assert_eq!(Some(Token::String), token_of(line, "\"hi\""));
	assert_eq!(Some(Token::Constant), token_of(line, "42"));
	assert_eq!(Some(Token::Comment), token_of(line, "// done"));
	// Operators stay plain
	assert_eq!(None, token_of(line, "="));
}

#[test]
pub fn token_ranges_stay_within_the_content() {
	let mut file = diff_named("a.py", "", "x = 'abc'\n");
	file.highlight();
	let line = &file.hunks[0].lines[0];
	assert!(line.syntax.iter().all(|(r, _)| r.end <= line.content.len()));
	assert_eq!(Some(Token::String), token_of(line, "'abc'"));
}

#[test]
pub fn removed_and_added_lines_parse_as_their_own_versions() {
	// A comment opened on a removed line does not run into the added line
	let mut file = diff_named("a.c", "int a; /* x\n", "int b;\n");
	file.highlight();
	let lines = &file.hunks[0].lines;
	assert_eq!(LineKind::Removed, lines[0].kind);
	assert_eq!(LineKind::Added, lines[1].kind);
	assert_eq!(Some(Token::Keyword), token_of(&lines[1], "int"));
	assert!(tokens(&lines[1]).iter().all(|(_, t)| *t != Token::Comment));
}

#[test]
pub fn a_comment_over_lines_carries_on_in_its_version() {
	let mut file = diff_named("a.c", "/* a\nb */\nint x;\n", "/* a\nb */\nint y;\n");
	file.highlight();
	let lines = &file.hunks[0].lines;
	assert_eq!(LineKind::Context, lines[1].kind);
	assert_eq!(Some(Token::Comment), token_of(&lines[1], "b */"));
}

#[test]
pub fn files_are_found_by_name_and_first_line() {
	let mut makefile = diff_named("Makefile", "", "all:\n\techo hi # note\n");
	makefile.highlight();
	assert!(makefile.hunks[0].lines.iter().any(|l| !l.syntax.is_empty()));

	let mut script = diff_named("run", "", "#!/bin/sh\necho \"hi\"\n");
	script.highlight();
	assert_eq!(
		Some(Token::String),
		token_of(&script.hunks[0].lines[1], "\"hi\"")
	);
}

#[test]
pub fn files_of_no_known_language_stay_plain() {
	let mut file = diff_named("notes.unknownext", "", "fn main() {}\n");
	file.highlight();
	assert!(file.highlighted);
	assert!(file.hunks[0].lines.iter().all(|l| l.syntax.is_empty()));
}

#[test]
pub fn files_with_very_long_lines_stay_plain() {
	let long = format!("var a = \"{}\";\n", "x".repeat(diff::MAX_HIGHLIGHT_LINE_BYTES));
	let text = format!("var b = 1;\n{}", long);
	let mut file = diff_named("a.js", "", &text);
	file.highlight();
	assert!(file.hunks[0].lines.iter().all(|l| l.syntax.is_empty()));
}

#[test]
pub fn revealing_lines_highlights_the_file_again() {
	let old: String = (1..=20).map(|i| format!("let a{} = {};\n", i, i)).collect();
	let new = old.replace("let a20 = 20;", "let a20 = 21;");
	let mut file = diff_named("a.rs", &old, &new);
	file.highlight();
	file.old_content = Some(old.lines().map(|l| l.to_string()).collect());

	assert!(file.reveal(0, Reveal::All));
	assert!(!file.highlighted);
	file.highlight();
	let first = &file.hunks[0].lines[0];
	assert_eq!(Some(1), first.old_lineno);
	assert_eq!(Some(Token::Keyword), token_of(first, "let"));
}

#[test]
pub fn segments_cut_at_emphasis_and_tokens() {
	let line = DiffLine {
		kind: LineKind::Added,
		old_lineno: None,
		new_lineno: Some(1),
		content: "let n = 42;".to_string(),
		emphasis: vec![6..10],
		syntax: vec![(0..3, Token::Keyword), (8..10, Token::Constant)],
	};
	let segment = |range: std::ops::Range<usize>, emphasized, token| Segment {
		range,
		emphasized,
		token,
	};
	assert_eq!(
		vec![
			segment(0..3, false, Some(Token::Keyword)),
			segment(3..6, false, None),
			segment(6..8, true, None),
			segment(8..10, true, Some(Token::Constant)),
			segment(10..11, false, None),
		],
		line.segments()
	);
}

#[test]
pub fn segments_of_an_empty_line_are_none() {
	let line = DiffLine {
		kind: LineKind::Context,
		old_lineno: Some(1),
		new_lineno: Some(1),
		content: String::new(),
		emphasis: Vec::new(),
		syntax: Vec::new(),
	};
	assert!(line.segments().is_empty());
}
