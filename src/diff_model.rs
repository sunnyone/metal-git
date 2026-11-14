use git2::{Delta, Diff, Error, Patch};
use similar::{Algorithm, DiffTag};
use std::ops::Range;

/// Changed lines are emphasized word by word only when at least this share of
/// the pair is unchanged. Otherwise the whole line reads as replaced.
const MIN_EMPHASIS_SIMILARITY: f32 = 0.5;

/// Runs larger than this many line pairs are paired in order instead of by
/// similarity, to keep the comparison cheap.
const MAX_ALIGNED_PAIRS: usize = 10_000;

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

#[derive(Clone, Debug)]
pub struct FileDiff {
    pub old_path: Option<String>,
    pub new_path: Option<String>,
    pub status: FileStatus,
    pub binary: bool,
    pub hunks: Vec<DiffHunk>,
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
            hunks: vec![DiffHunk { header, lines }],
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

    pub fn count_lines(&self, kind: LineKind) -> usize {
        self.hunks
            .iter()
            .flat_map(|hunk| &hunk.lines)
            .filter(|line| line.kind == kind)
            .count()
    }
}

pub fn files_from_diff(diff: &Diff) -> Result<Vec<FileDiff>, Error> {
    (0..diff.deltas().len())
        .map(|index| {
            let delta = diff.get_delta(index).unwrap();
            match Patch::from_diff(diff, index)? {
                Some(patch) => file_from_patch(&patch),
                None => Ok(FileDiff {
                    old_path: path_of(delta.old_file()),
                    new_path: path_of(delta.new_file()),
                    status: status_of(delta.status()),
                    binary: true,
                    hunks: Vec::new(),
                }),
            }
        })
        .collect()
}

pub fn file_from_patch(patch: &Patch) -> Result<FileDiff, Error> {
    let delta = patch.delta();
    let mut hunks = Vec::new();

    for hunk_index in 0..patch.num_hunks() {
        let (hunk, line_count) = patch.hunk(hunk_index)?;
        let mut lines = Vec::with_capacity(line_count);

        for line_index in 0..line_count {
            let line = patch.line_in_hunk(hunk_index, line_index)?;
            let kind = match line.origin() {
                ' ' => LineKind::Context,
                '+' => LineKind::Added,
                '-' => LineKind::Removed,
                // The "no newline at end of file" markers
                _ => continue,
            };
            lines.push(DiffLine {
                kind,
                old_lineno: line.old_lineno(),
                new_lineno: line.new_lineno(),
                content: strip_line_break(&String::from_utf8_lossy(line.content())).to_string(),
                emphasis: Vec::new(),
            });
        }

        emphasize_changes(&mut lines);
        hunks.push(DiffHunk {
            header: String::from_utf8_lossy(hunk.header())
                .trim_end()
                .to_string(),
            lines,
        });
    }

    Ok(FileDiff {
        old_path: path_of(delta.old_file()),
        new_path: path_of(delta.new_file()),
        status: status_of(delta.status()),
        binary: delta.flags().is_binary(),
        hunks,
    })
}

/// Pairs the lines of each run of removed lines with those of the run of added
/// lines right after it, and marks the words that differ within each pair.
pub fn emphasize_changes(lines: &mut [DiffLine]) {
    let mut i = 0;
    while i < lines.len() {
        let removed_start = i;
        while i < lines.len() && lines[i].kind == LineKind::Removed {
            i += 1;
        }
        let added_start = i;
        while i < lines.len() && lines[i].kind == LineKind::Added {
            i += 1;
        }

        let removed: Vec<&str> = lines[removed_start..added_start]
            .iter()
            .map(|l| l.content.as_str())
            .collect();
        let added: Vec<&str> = lines[added_start..i]
            .iter()
            .map(|l| l.content.as_str())
            .collect();
        for (r, a, (old, new)) in pair_lines(&removed, &added) {
            lines[removed_start + r].emphasis = old;
            lines[added_start + a].emphasis = new;
        }

        if i == removed_start {
            i += 1;
        }
    }
}

type RangePair = (Vec<Range<usize>>, Vec<Range<usize>>);

/// Pairs removed lines with added lines, keeping their order, so that the pairs
/// are as similar as possible in total. Lines without a similar enough partner
/// are left out. Returns the indices of each pair with its changed ranges.
fn pair_lines(removed: &[&str], added: &[&str]) -> Vec<(usize, usize, RangePair)> {
    let (n, m) = (removed.len(), added.len());
    if n * m > MAX_ALIGNED_PAIRS {
        return (0..n.min(m))
            .filter_map(|k| compare(removed[k], added[k]).map(|(_, ranges)| (k, k, ranges)))
            .collect();
    }

    let mut matches: Vec<Vec<Option<(f32, RangePair)>>> = removed
        .iter()
        .map(|old| added.iter().map(|new| compare(old, new)).collect())
        .collect();

    // best[r][a]: the highest total similarity pairing the first r removed
    // lines with the first a added lines
    let mut best = vec![vec![0.0f32; m + 1]; n + 1];
    for r in 1..=n {
        for a in 1..=m {
            let mut score = best[r - 1][a].max(best[r][a - 1]);
            if let Some((similarity, _)) = &matches[r - 1][a - 1] {
                score = score.max(best[r - 1][a - 1] + similarity);
            }
            best[r][a] = score;
        }
    }

    let mut pairs = Vec::new();
    let (mut r, mut a) = (n, m);
    while r > 0 && a > 0 {
        if best[r][a] == best[r - 1][a] {
            r -= 1;
        } else if best[r][a] == best[r][a - 1] {
            a -= 1;
        } else {
            let (_, ranges) = matches[r - 1][a - 1].take().unwrap();
            pairs.push((r - 1, a - 1, ranges));
            r -= 1;
            a -= 1;
        }
    }
    pairs
}

/// The byte ranges that differ between two lines, or none for both when the
/// lines have too little in common for the ranges to help.
pub fn changed_ranges(old: &str, new: &str) -> RangePair {
    compare(old, new)
        .map(|(_, ranges)| ranges)
        .unwrap_or_default()
}

/// The similarity of two lines and the byte ranges that differ between them,
/// when the lines have enough in common.
fn compare(old: &str, new: &str) -> Option<(f32, RangePair)> {
    let old_tokens = tokenize(old);
    let new_tokens = tokenize(new);
    let old_words: Vec<&str> = old_tokens.iter().map(|r| &old[r.clone()]).collect();
    let new_words: Vec<&str> = new_tokens.iter().map(|r| &new[r.clone()]).collect();

    let ops = similar::capture_diff_slices(Algorithm::Myers, &old_words, &new_words);
    let similarity = similarity(&ops, &old_words, &new_words);
    if similarity < MIN_EMPHASIS_SIMILARITY {
        return None;
    }

    let mut old_ranges = Vec::new();
    let mut new_ranges = Vec::new();
    for op in &ops {
        let (tag, old_range, new_range) = op.as_tag_tuple();
        if tag == DiffTag::Equal {
            continue;
        }
        push_merged(&mut old_ranges, byte_range(&old_tokens, old_range));
        push_merged(&mut new_ranges, byte_range(&new_tokens, new_range));
    }
    Some((similarity, (old_ranges, new_ranges)))
}

/// The share of non-whitespace characters that the two lines have in common.
/// Whitespace is left out so that indentation and spacing alone do not make
/// two unrelated lines look alike.
fn similarity(ops: &[similar::DiffOp], old_words: &[&str], new_words: &[&str]) -> f32 {
    let weight = |words: &[&str]| -> usize {
        words
            .iter()
            .map(|w| w.chars().filter(|c| !c.is_whitespace()).count())
            .sum()
    };
    let common: usize = ops
        .iter()
        .filter(|op| op.tag() == DiffTag::Equal)
        .map(|op| weight(&old_words[op.old_range()]))
        .sum();
    let total = weight(old_words) + weight(new_words);
    if total == 0 {
        return 1.0;
    }
    2.0 * common as f32 / total as f32
}

/// Splits a line into runs of word characters, runs of whitespace, and single
/// other characters, as byte ranges.
fn tokenize(text: &str) -> Vec<Range<usize>> {
    #[derive(PartialEq)]
    enum Class {
        Word,
        Space,
        Other,
    }
    let class_of = |c: char| {
        if c.is_alphanumeric() || c == '_' {
            Class::Word
        } else if c.is_whitespace() {
            Class::Space
        } else {
            Class::Other
        }
    };

    let mut tokens: Vec<Range<usize>> = Vec::new();
    let mut last_class = None;
    for (i, c) in text.char_indices() {
        let class = class_of(c);
        let joins = class != Class::Other && last_class.as_ref() == Some(&class);
        match tokens.last_mut() {
            Some(last) if joins => last.end = i + c.len_utf8(),
            _ => tokens.push(i..i + c.len_utf8()),
        }
        last_class = Some(class);
    }
    tokens
}

fn byte_range(tokens: &[Range<usize>], token_range: Range<usize>) -> Option<Range<usize>> {
    if token_range.is_empty() {
        return None;
    }
    Some(tokens[token_range.start].start..tokens[token_range.end - 1].end)
}

fn push_merged(ranges: &mut Vec<Range<usize>>, range: Option<Range<usize>>) {
    let Some(range) = range else {
        return;
    };
    match ranges.last_mut() {
        Some(last) if last.end == range.start => last.end = range.end,
        _ => ranges.push(range),
    }
}

fn strip_line_break(text: &str) -> &str {
    let text = text.strip_suffix('\n').unwrap_or(text);
    text.strip_suffix('\r').unwrap_or(text)
}

fn path_of(file: git2::DiffFile) -> Option<String> {
    file.path().and_then(|p| p.to_str()).map(|s| s.to_string())
}

fn status_of(delta: Delta) -> FileStatus {
    match delta {
        Delta::Added | Delta::Untracked => FileStatus::Added,
        Delta::Deleted => FileStatus::Deleted,
        Delta::Modified => FileStatus::Modified,
        Delta::Renamed => FileStatus::Renamed,
        Delta::Copied => FileStatus::Copied,
        _ => FileStatus::Other,
    }
}
