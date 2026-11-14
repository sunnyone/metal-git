extern crate git2;
extern crate metal_git;

use git2::{DiffOptions, Patch};
use metal_git::diff_model::{self, FileDiff, FileStatus, LineKind};

fn diff_buffers(old: &str, new: &str, context_lines: u32) -> FileDiff {
	let mut opts = DiffOptions::new();
	opts.context_lines(context_lines);
	let patch = Patch::from_buffers(
		old.as_bytes(),
		Some("a.rs".as_ref()),
		new.as_bytes(),
		Some("a.rs".as_ref()),
		Some(&mut opts),
	)
	.unwrap();
	diff_model::file_from_patch(&patch).unwrap()
}

fn emphasized<'a>(content: &'a str, line: &diff_model::DiffLine) -> Vec<&'a str> {
	line.emphasis.iter().map(|r| &content[r.clone()]).collect()
}

#[test]
pub fn lines_carry_kinds_and_line_numbers() {
	let file = diff_buffers("a\nb\nc\n", "a\nB\nc\nd\n", 3);

	assert_eq!(1, file.hunks.len());
	let lines = &file.hunks[0].lines;
	let summary: Vec<_> = lines
		.iter()
		.map(|l| (l.kind, l.old_lineno, l.new_lineno, l.content.as_str()))
		.collect();
	assert_eq!(
		vec![
			(LineKind::Context, Some(1), Some(1), "a"),
			(LineKind::Removed, Some(2), None, "b"),
			(LineKind::Added, None, Some(2), "B"),
			(LineKind::Context, Some(3), Some(3), "c"),
			(LineKind::Added, None, Some(4), "d"),
		],
		summary
	);
	assert_eq!(2, file.count_lines(LineKind::Added));
	assert_eq!(1, file.count_lines(LineKind::Removed));
}

#[test]
pub fn emphasizes_only_the_changed_words_of_a_line_pair() {
	let file = diff_buffers("let total = price * 2;\n", "let total = price * count;\n", 3);

	let lines = &file.hunks[0].lines;
	assert_eq!(vec!["2"], emphasized(&lines[0].content, &lines[0]));
	assert_eq!(vec!["count"], emphasized(&lines[1].content, &lines[1]));
}

#[test]
pub fn does_not_emphasize_lines_with_little_in_common() {
	let file = diff_buffers("fn main() {}\n", "struct Point { x: i32 }\n", 3);

	for line in &file.hunks[0].lines {
		assert!(line.emphasis.is_empty(), "{:?}", line);
	}
}

#[test]
pub fn pairs_runs_of_removed_and_added_lines_in_order() {
	let file = diff_buffers("x = 1\ny = 2\n", "x = 10\ny = 20\nz = 30\n", 3);

	let lines = &file.hunks[0].lines;
	let kinds: Vec<_> = lines.iter().map(|l| l.kind).collect();
	assert_eq!(
		vec![LineKind::Removed, LineKind::Removed, LineKind::Added, LineKind::Added, LineKind::Added],
		kinds
	);
	assert_eq!(vec!["1"], emphasized(&lines[0].content, &lines[0]));
	assert_eq!(vec!["10"], emphasized(&lines[2].content, &lines[2]));
	assert_eq!(vec!["20"], emphasized(&lines[3].content, &lines[3]));
	assert!(lines[4].emphasis.is_empty());
}

#[test]
pub fn pairs_a_changed_line_with_its_counterpart_after_an_inserted_line() {
	let file = diff_buffers("let a = price * 2;\n", "// Double the price\nlet a = price * 3;\n", 3);

	let lines = &file.hunks[0].lines;
	assert_eq!(vec!["2"], emphasized(&lines[0].content, &lines[0]));
	assert!(lines[1].emphasis.is_empty());
	assert_eq!(vec!["3"], emphasized(&lines[2].content, &lines[2]));
}

#[test]
pub fn does_not_pair_lines_of_a_replaced_function_that_share_only_names() {
	let old = "for item in &self.items {\n    println!(\"{}\", item.name);\n}\n";
	let new = "self.items.is_empty()\n";
	let file = diff_buffers(old, new, 3);

	for line in &file.hunks[0].lines {
		assert!(line.emphasis.is_empty(), "{:?}", line);
	}
}

#[test]
pub fn emphasis_ranges_respect_multibyte_characters() {
	let file = diff_buffers("名前 = 太郎\n", "名前 = 花子\n", 3);

	let lines = &file.hunks[0].lines;
	assert_eq!(vec!["太郎"], emphasized(&lines[0].content, &lines[0]));
	assert_eq!(vec!["花子"], emphasized(&lines[1].content, &lines[1]));
}

#[test]
pub fn keeps_carriage_returns_out_of_the_content() {
	let file = diff_buffers("a\r\n", "b\r\n", 3);

	assert_eq!("a", file.hunks[0].lines[0].content);
	assert_eq!("b", file.hunks[0].lines[1].content);
}

#[test]
pub fn all_added_numbers_every_line_as_new() {
	let file = FileDiff::all_added("new.txt", "one\ntwo\n");

	assert_eq!(FileStatus::Added, file.status);
	let lines = &file.hunks[0].lines;
	assert_eq!(2, lines.len());
	assert!(lines.iter().all(|l| l.kind == LineKind::Added && l.old_lineno.is_none()));
	assert_eq!(Some(2), lines[1].new_lineno);
}

fn side_by_side_contents(file: &FileDiff) -> Vec<(Option<&str>, Option<&str>)> {
	file.hunks[0]
		.side_by_side()
		.into_iter()
		.map(|(old, new)| (old.map(|l| l.content.as_str()), new.map(|l| l.content.as_str())))
		.collect()
}

#[test]
pub fn side_by_side_puts_context_lines_on_both_sides() {
	let file = diff_buffers("a\nb\nc\n", "a\nB\nc\n", 3);

	assert_eq!(
		vec![(Some("a"), Some("a")), (Some("b"), Some("B")), (Some("c"), Some("c"))],
		side_by_side_contents(&file)
	);
}

#[test]
pub fn side_by_side_puts_paired_lines_on_one_row() {
	let file = diff_buffers("let a = price * 2;\n", "// Double the price\nlet a = price * 3;\n", 3);

	assert_eq!(
		vec![
			(None, Some("// Double the price")),
			(Some("let a = price * 2;"), Some("let a = price * 3;")),
		],
		side_by_side_contents(&file)
	);
}

#[test]
pub fn side_by_side_fills_rows_in_order_between_pairs() {
	let file = diff_buffers("fn a() {}\nlet x = 1;\n", "struct P;\nenum Q {}\nlet x = 10;\nconst R: u8 = 0;\n", 3);

	assert_eq!(
		vec![
			(Some("fn a() {}"), Some("struct P;")),
			(None, Some("enum Q {}")),
			(Some("let x = 1;"), Some("let x = 10;")),
			(None, Some("const R: u8 = 0;")),
		],
		side_by_side_contents(&file)
	);
}

#[test]
pub fn side_by_side_keeps_every_line_once() {
	let old: String = (1..=30).map(|i| format!("line {}\n", i)).collect();
	let new = old
		.replace("line 3\n", "")
		.replace("line 10\n", "line ten\nextra\n")
		.replace("line 20\n", "line 20 changed\n");
	let file = diff_buffers(&old, &new, 3);

	for hunk in &file.hunks {
		let rows = hunk.side_by_side();
		let olds = rows.iter().filter(|(old, _)| old.is_some()).count();
		let news = rows.iter().filter(|(_, new)| new.is_some()).count();
		let removed = hunk.lines.iter().filter(|l| l.kind != LineKind::Added).count();
		let added = hunk.lines.iter().filter(|l| l.kind != LineKind::Removed).count();
		assert_eq!((removed, added), (olds, news));
	}
}
