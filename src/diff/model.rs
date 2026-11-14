//! What changed in each file of a diff, as the views show it.

use std::ops::Range;

use super::emphasis::emphasize_changes;

/// Files with more lines than this start closed, so that a large commit opens
/// quickly. They are drawn when they are opened.
pub const MAX_OPEN_FILE_LINES: usize = 2_000;

/// Files start closed once the open ones would have more lines than this in total.
pub const MAX_OPEN_TOTAL_LINES: usize = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Context,
    Added,
    Removed,
}

#[derive(Clone, Debug)]
pub struct DiffLine {
    pub kind: LineKind,
    pub old_lineno: Option<u32>,
    pub new_lineno: Option<u32>,
    /// The line without its line break.
    pub content: String,
    /// Byte ranges of `content` that changed within a line pair.
    pub emphasis: Vec<Range<usize>>,
}

#[derive(Clone, Debug)]
pub struct DiffHunk {
    pub header: String,
    pub lines: Vec<DiffLine>,
    /// Indices into `lines` of each removed line and the added line it was
    /// paired with for emphasis, in order.
    pub pairs: Vec<(usize, usize)>,
}

/// A row of the old and the new side, either of which can be empty.
pub type SideBySideRow<'a> = (Option<&'a DiffLine>, Option<&'a DiffLine>);

impl DiffHunk {
    /// The lines as rows of an old and a new side. Context lines fill both
    /// sides. In a run of changes, paired lines share a row, and the lines
    /// between pairs fill the rows in order.
    pub fn side_by_side(&self) -> Vec<SideBySideRow<'_>> {
        let lines = &self.lines;
        let mut rows = Vec::with_capacity(lines.len());
        let mut pairs = self.pairs.iter().peekable();

        let mut i = 0;
        while i < lines.len() {
            if lines[i].kind == LineKind::Context {
                rows.push((Some(&lines[i]), Some(&lines[i])));
                i += 1;
                continue;
            }

            let start = i;
            while i < lines.len() && lines[i].kind != LineKind::Context {
                i += 1;
            }
            let of_kind =
                |kind| -> Vec<usize> { (start..i).filter(|&k| lines[k].kind == kind).collect() };
            let removed = of_kind(LineKind::Removed);
            let added = of_kind(LineKind::Added);

            let (mut r, mut a) = (0, 0);
            while let Some(&&(pair_r, pair_a)) = pairs.peek() {
                if pair_r >= i {
                    break;
                }
                pairs.next();
                let pair_r = removed.iter().position(|&l| l == pair_r).unwrap();
                let pair_a = added.iter().position(|&l| l == pair_a).unwrap();
                push_zipped(&mut rows, lines, &removed[r..pair_r], &added[a..pair_a]);
                rows.push((Some(&lines[removed[pair_r]]), Some(&lines[added[pair_a]])));
                r = pair_r + 1;
                a = pair_a + 1;
            }
            push_zipped(&mut rows, lines, &removed[r..], &added[a..]);
        }
        rows
    }
}

fn push_zipped<'a>(
    rows: &mut Vec<SideBySideRow<'a>>,
    lines: &'a [DiffLine],
    removed: &[usize],
    added: &[usize],
) {
    for k in 0..removed.len().max(added.len()) {
        let old = removed.get(k).map(|&l| &lines[l]);
        let new = added.get(k).map(|&l| &lines[l]);
        rows.push((old, new));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileStatus {
    Added,
    Deleted,
    Modified,
    Renamed,
    Copied,
    Other,
}

impl FileStatus {
    pub const ALL: [FileStatus; 6] = [
        FileStatus::Added,
        FileStatus::Deleted,
        FileStatus::Modified,
        FileStatus::Renamed,
        FileStatus::Copied,
        FileStatus::Other,
    ];

    /// The letter of the status, as `git status --short` and many Git clients
    /// show it.
    pub fn letter(self) -> &'static str {
        match self {
            FileStatus::Added => "A",
            FileStatus::Deleted => "D",
            FileStatus::Modified => "M",
            FileStatus::Renamed => "R",
            FileStatus::Copied => "C",
            FileStatus::Other => "?",
        }
    }
}

#[derive(Clone, Debug)]
pub struct FileDiff {
    pub old_path: Option<String>,
    pub new_path: Option<String>,
    pub status: FileStatus,
    pub binary: bool,
    pub hunks: Vec<DiffHunk>,
    /// Whether the changed words of the line pairs are marked yet. Marking them
    /// is left until the file is shown, as it is the costly part of a large diff.
    pub emphasized: bool,
}

impl FileDiff {
    /// A file with no previous version, such as an untracked file in the work tree.
    pub fn all_added(path: &str, text: &str) -> FileDiff {
        let lines = text
            .lines()
            .enumerate()
            .map(|(i, line)| DiffLine {
                kind: LineKind::Added,
                old_lineno: None,
                new_lineno: Some(i as u32 + 1),
                content: line.to_string(),
                emphasis: Vec::new(),
            })
            .collect::<Vec<_>>();
        let header = format!("@@ -0,0 +1,{} @@", lines.len());

        FileDiff {
            old_path: None,
            new_path: Some(path.to_string()),
            status: FileStatus::Added,
            binary: false,
            hunks: vec![DiffHunk {
                header,
                lines,
                pairs: Vec::new(),
            }],
            emphasized: true,
        }
    }

    pub fn display_path(&self) -> String {
        match (self.old_path.as_deref(), self.new_path.as_deref()) {
            (Some(old), Some(new)) if old != new => format!("{} → {}", old, new),
            (_, Some(new)) => new.to_string(),
            (Some(old), None) => old.to_string(),
            (None, None) => String::new(),
        }
    }

    /// The number of lines in all the hunks.
    pub fn line_count(&self) -> usize {
        self.hunks.iter().map(|hunk| hunk.lines.len()).sum()
    }

    /// Marks the changed words of the line pairs, unless they are marked already.
    pub fn emphasize(&mut self) {
        if self.emphasized {
            return;
        }
        for hunk in &mut self.hunks {
            hunk.pairs = emphasize_changes(&mut hunk.lines);
        }
        self.emphasized = true;
    }

    pub fn count_lines(&self, kind: LineKind) -> usize {
        self.hunks
            .iter()
            .flat_map(|hunk| &hunk.lines)
            .filter(|line| line.kind == kind)
            .count()
    }
}

/// Which files to show open at first: those up to `MAX_OPEN_FILE_LINES` lines,
/// in order, until the open ones reach `MAX_OPEN_TOTAL_LINES` lines in total.
/// A lone file is always open.
pub fn initially_open(files: &[FileDiff]) -> Vec<bool> {
    if files.len() == 1 {
        return vec![true];
    }
    let mut total = 0;
    files
        .iter()
        .map(|file| {
            let count = file.line_count();
            let open = count <= MAX_OPEN_FILE_LINES && total + count <= MAX_OPEN_TOTAL_LINES;
            if open {
                total += count;
            }
            open
        })
        .collect()
}
