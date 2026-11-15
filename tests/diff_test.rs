extern crate git2;
extern crate metal_git;

use git2::{DiffOptions, Patch};
use metal_git::diff::{self, FileDiff, FileStatus, LineKind};

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
	diff::file_from_patch(&patch).unwrap()
}

fn emphasized<'a>(content: &'a str, line: &diff::DiffLine) -> Vec<&'a str> {
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

fn file_of_lines(name: &str, count: usize) -> FileDiff {
	FileDiff::all_added(name, &"x\n".repeat(count))
}

#[test]
pub fn files_from_a_diff_mark_their_changed_words_only_when_emphasized() {
	let patch = "diff --git a/a.rs b/a.rs\n\
		--- a/a.rs\n\
		+++ b/a.rs\n\
		@@ -1 +1 @@\n\
		-let total = 1;\n\
		+let total = 2;\n";
	let diff = git2::Diff::from_buffer(patch.as_bytes()).unwrap();
	let mut files = diff::files_from_diff(&diff).unwrap();
	let file = &mut files[0];

	assert!(!file.emphasized);
	assert!(file.hunks[0].lines.iter().all(|l| l.emphasis.is_empty()));
	assert!(file.hunks[0].pairs.is_empty());

	file.emphasize();
	assert!(file.emphasized);
	let lines = &file.hunks[0].lines;
	assert_eq!(vec!["2"], emphasized(&lines[1].content, &lines[1]));
	assert_eq!(vec![(0, 1)], file.hunks[0].pairs);
}

#[test]
pub fn line_count_adds_up_the_lines_of_every_hunk() {
	let file = diff_buffers("a\nb\nc\nd\ne\nf\ng\nh\n", "A\nb\nc\nd\ne\nf\ng\nH\n", 0);

	assert_eq!(2, file.hunks.len());
	assert_eq!(4, file.line_count());
}

#[test]
pub fn initially_open_closes_files_over_the_line_limit() {
	let files = vec![
		file_of_lines("small.rs", 10),
		file_of_lines("large.rs", diff::MAX_OPEN_FILE_LINES + 1),
		file_of_lines("limit.rs", diff::MAX_OPEN_FILE_LINES),
	];

	assert_eq!(vec![true, false, true], diff::initially_open(&files));
}

#[test]
pub fn initially_open_closes_files_once_the_total_would_pass_the_limit() {
	// Files that fill the total but for 5 lines
	let size = diff::MAX_OPEN_FILE_LINES;
	let mut sizes = vec![size; diff::MAX_OPEN_TOTAL_LINES / size];
	let filled: usize = sizes.iter().sum();
	if diff::MAX_OPEN_TOTAL_LINES - filled < 5 {
		*sizes.last_mut().unwrap() -= 5;
	}
	let left = diff::MAX_OPEN_TOTAL_LINES - sizes.iter().sum::<usize>();
	let fitting = sizes.len();

	let mut files: Vec<FileDiff> = sizes.iter().enumerate().map(|(i, &n)| file_of_lines(&format!("{}.rs", i), n)).collect();
	files.push(file_of_lines("over.rs", left + 1));
	files.push(file_of_lines("small.rs", left));

	let open = diff::initially_open(&files);
	assert!(open[..fitting].iter().all(|&o| o));
	assert!(!open[fitting]);
	// A later file that still fits in what is left opens
	assert!(open[fitting + 1]);
}

#[test]
pub fn initially_open_always_opens_a_lone_file() {
	let files = vec![file_of_lines("huge.rs", diff::MAX_OPEN_TOTAL_LINES * 2)];

	assert_eq!(vec![true], diff::initially_open(&files));
}

#[test]
pub fn statuses_show_the_letters_of_git_status() {
	let letters: Vec<&str> = FileStatus::ALL.iter().map(|s| s.letter()).collect();

	assert_eq!(vec!["A", "D", "M", "R", "C", "?"], letters);
}

fn tree_of<'repo>(repo: &'repo git2::Repository, files: &[(&str, &str)]) -> git2::Tree<'repo> {
	let mut builder = repo.treebuilder(None).unwrap();
	for (name, content) in files {
		let blob = repo.blob(content.as_bytes()).unwrap();
		builder.insert(name, blob, 0o100644).unwrap();
	}
	repo.find_tree(builder.write().unwrap()).unwrap()
}

#[test]
pub fn diff_trees_finds_renamed_files() {
	let dir = tempdir::TempDir::new("diff-trees").unwrap();
	let repo = git2::Repository::init(dir.path()).unwrap();
	let body: String = (0..20).map(|i| format!("line {}\n", i)).collect();
	let changed = body.replace("line 3\n", "line three\n");
	let old = tree_of(&repo, &[("a.txt", &body), ("b.txt", &body), ("kept.txt", "kept\n")]);
	let new = tree_of(&repo, &[("moved.txt", &body), ("c.txt", &changed), ("kept.txt", "kept\n")]);

	let diff = diff::diff_trees(&repo, Some(&old), Some(&new), diff::DiffSettings::default()).unwrap();
	let files = diff::files_from_diff(&diff).unwrap();

	let summary: Vec<(FileStatus, String, usize)> =
		files.iter().map(|f| (f.status, f.display_path(), f.line_count())).collect();
	assert_eq!(
		// In the order of the new paths
		vec![
			(FileStatus::Renamed, "b.txt → c.txt".to_string(), 8),
			(FileStatus::Renamed, "a.txt → moved.txt".to_string(), 0),
		],
		summary
	);
	assert!(files.iter().all(|f| !f.binary));
}

fn numbered_lines(count: usize) -> String {
	(1..=count).map(|i| format!("line {}\n", i)).collect()
}

/// A diff of a 30-line file with lines 5 and 25 changed, each paired for
/// emphasis, and the old version read.
fn file_with_two_hunks() -> FileDiff {
	let old = numbered_lines(30);
	let new = old.replace("line 5\n", "line 50\n").replace("line 25\n", "line 250\n");
	let mut file = diff_buffers(&old, &new, 3);
	file.old_content = Some(old.lines().map(|l| l.to_string()).collect());
	file
}

fn old_numbers(hunk: &diff::DiffHunk) -> Vec<u32> {
	hunk.lines.iter().filter_map(|l| l.old_lineno).collect()
}

/// The ranges of a hunk header, without the text git adds after them.
fn ranges_of(header: &str) -> &str {
	let end = header.rfind("@@").unwrap() + 2;
	&header[..end]
}

#[test]
pub fn gaps_are_the_lines_before_between_and_after_the_hunks() {
	let mut file = file_with_two_hunks();
	let content = file.old_content.take();

	assert_eq!(
		vec![
			diff::Gap { before_hunk: 0, old_start: 1, count: Some(1) },
			diff::Gap { before_hunk: 1, old_start: 9, count: Some(13) },
			// The last hunk ends with full context, so lines may follow
			diff::Gap { before_hunk: 2, old_start: 29, count: None },
		],
		file.gaps()
	);

	file.old_content = content;
	assert_eq!(Some(2), file.gaps()[2].count);
}

#[test]
pub fn no_gap_follows_a_hunk_that_reaches_the_end_of_the_file() {
	let old = numbered_lines(10);
	let new = old.replace("line 9\n", "line nine\n");
	let file = diff_buffers(&old, &new, 3);

	assert!(file.gaps().iter().all(|gap| gap.before_hunk == 0));
}

#[test]
pub fn reveal_down_continues_the_hunk_above() {
	let mut file = file_with_two_hunks();

	assert!(file.reveal(1, diff::Reveal::Down(4)));

	let hunk = &file.hunks[0];
	assert_eq!((2..=12).collect::<Vec<_>>(), old_numbers(hunk));
	assert_eq!("line 12", hunk.lines.last().unwrap().content);
	assert_eq!(LineKind::Context, hunk.lines.last().unwrap().kind);
	assert_eq!("@@ -2,11 +2,11 @@", ranges_of(&hunk.header));
	assert_eq!(diff::Gap { before_hunk: 1, old_start: 13, count: Some(9) }, file.gaps()[1]);
}

#[test]
pub fn reveal_up_leads_into_the_hunk_below_and_keeps_its_pairs() {
	let mut file = file_with_two_hunks();

	assert!(file.reveal(1, diff::Reveal::Up(4)));

	let hunk = &file.hunks[1];
	assert_eq!(Some(18), hunk.lines[0].old_lineno);
	assert_eq!("@@ -18,11 +18,11 @@", ranges_of(&hunk.header));
	let (removed, added) = hunk.pairs[0];
	assert_eq!("line 25", hunk.lines[removed].content);
	assert_eq!("line 250", hunk.lines[added].content);
}

#[test]
pub fn revealing_a_whole_gap_joins_the_hunks_around_it() {
	let mut file = file_with_two_hunks();

	assert!(file.reveal(1, diff::Reveal::All));

	assert_eq!(1, file.hunks.len());
	let hunk = &file.hunks[0];
	assert_eq!("@@ -2,27 +2,27 @@", ranges_of(&hunk.header));
	assert_eq!((2..=28).collect::<Vec<_>>(), old_numbers(hunk));
	assert_eq!(2, hunk.pairs.len());
	let rows = hunk.side_by_side();
	assert_eq!(27, rows.len());
}

#[test]
pub fn revealing_in_steps_joins_the_hunks_once_the_gap_is_gone() {
	let mut file = file_with_two_hunks();

	file.reveal(1, diff::Reveal::Down(10));
	assert_eq!(2, file.hunks.len());
	file.reveal(1, diff::Reveal::Up(10));

	assert_eq!(1, file.hunks.len());
	assert_eq!((2..=28).collect::<Vec<_>>(), old_numbers(&file.hunks[0]));
}

#[test]
pub fn revealed_lines_are_numbered_in_the_new_file_after_added_lines() {
	let old = numbered_lines(30);
	let new = format!("added 1\nadded 2\n{}", old.replace("line 20\n", "line twenty\n"));
	let mut file = diff_buffers(&old, &new, 3);
	file.old_content = Some(old.lines().map(|l| l.to_string()).collect());

	assert!(file.reveal(1, diff::Reveal::Down(2)));

	let last = file.hunks[0].lines.last().unwrap();
	assert_eq!((Some(5), Some(7)), (last.old_lineno, last.new_lineno));
	assert_eq!("line 5", last.content);
}

#[test]
pub fn reveal_reaches_the_start_and_the_end_of_the_file() {
	let mut file = file_with_two_hunks();

	assert!(file.reveal(0, diff::Reveal::All));
	assert!(file.reveal(2, diff::Reveal::All));

	assert_eq!(Some(1), file.hunks[0].lines[0].old_lineno);
	assert_eq!(Some(30), file.hunks.last().unwrap().lines.last().unwrap().old_lineno);
	assert!(file.gaps().iter().all(|gap| gap.before_hunk == 1));
}

#[test]
pub fn reveal_needs_the_old_version() {
	let mut file = file_with_two_hunks();
	file.old_content = None;

	assert!(!file.reveal(1, diff::Reveal::All));
	assert_eq!(2, file.hunks.len());
}

#[test]
pub fn ignoring_whitespace_keeps_every_file_in_order() {
	let dir = tempdir::TempDir::new("diff-trees").unwrap();
	let repo = git2::Repository::init(dir.path()).unwrap();
	let old = tree_of(&repo, &[("a.txt", "x = 1\ny = 2\n"), ("b.txt", "old\n")]);
	let new = tree_of(&repo, &[("a.txt", "x  =  1\ny = 2\n"), ("b.txt", "new\n")]);
	let settings = diff::DiffSettings { ignore_whitespace: true };

	let diff = diff::diff_trees(&repo, Some(&old), Some(&new), settings).unwrap();
	let files = diff::files_from_diff(&diff).unwrap();

	let summary: Vec<(String, usize)> = files.iter().map(|f| (f.display_path(), f.hunks.len())).collect();
	assert_eq!(vec![("a.txt".to_string(), 0), ("b.txt".to_string(), 1)], summary);
}
