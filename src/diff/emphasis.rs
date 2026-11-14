//! Marking the words that changed within pairs of removed and added lines.

use similar::{Algorithm, DiffTag};
use std::ops::Range;

use super::model::{DiffLine, LineKind};

/// Changed lines are emphasized word by word only when at least this share of
/// the pair is unchanged. Otherwise the whole line reads as replaced.
const MIN_EMPHASIS_SIMILARITY: f32 = 0.5;

/// Runs larger than this many line pairs are paired in order instead of by
/// similarity, to keep the comparison cheap.
const MAX_ALIGNED_PAIRS: usize = 10_000;

/// Pairs the lines of each run of removed lines with those of the run of added
/// lines right after it, and marks the words that differ within each pair.
/// Returns the indices of the pairs in order.
pub fn emphasize_changes(lines: &mut [DiffLine]) -> Vec<(usize, usize)> {
    let mut pairs = Vec::new();
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
            pairs.push((removed_start + r, added_start + a));
        }

        if i == removed_start {
            i += 1;
        }
    }
    pairs
}

type RangePair = (Vec<Range<usize>>, Vec<Range<usize>>);

/// Pairs removed lines with added lines, keeping their order, so that the pairs
/// are as similar as possible in total. Lines without a similar enough partner
/// are left out. Returns the indices of each pair with its changed ranges, in
/// order.
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
    pairs.reverse();
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
