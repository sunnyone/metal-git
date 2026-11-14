//! The unchanged lines that a diff leaves out around its hunks, and revealing
//! them from the old version of the file.

use git2::{Error, Repository};
use std::ops::Range;
use std::rc::Rc;

use super::model::{DiffHunk, DiffLine, FileDiff, LineKind};
use crate::repository_manager::RepositoryManager;

/// The unchanged lines git shows around each change by default. A last hunk
/// that ends with fewer has reached the end of the file.
const DEFAULT_CONTEXT_LINES: usize = 3;

/// A run of unchanged lines of the old file that the hunks leave out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gap {
    /// The hunk the gap comes before, or the number of hunks for the gap after
    /// the last one
    pub before_hunk: usize,
    /// The first line of the gap in the old file
    pub old_start: u32,
    /// How many lines, unknown for the gap after the last hunk until the old
    /// version is read
    pub count: Option<u32>,
}

/// Which lines of a gap to reveal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reveal {
    /// Up to this many at the top of the gap, continuing the hunk above
    Down(u32),
    /// Up to this many at the bottom of the gap, leading into the hunk below
    Up(u32),
    All,
}

/// Lines from `start`, `count` of them, as git gives them: for none, `start`
/// is the line before, such as -5,0 for none after line 5.
fn span(start: u32, count: u32) -> Range<u32> {
    let first = if count == 0 { start + 1 } else { start };
    first..first + count
}

/// The start of a range as git gives it. See `span`.
fn start_of(range: &Range<u32>) -> u32 {
    if range.is_empty() {
        range.start - 1
    } else {
        range.start
    }
}

impl DiffHunk {
    pub fn old_range(&self) -> Range<u32> {
        span(self.old_start, self.old_lines)
    }

    pub fn new_range(&self) -> Range<u32> {
        span(self.new_start, self.new_lines)
    }

    /// Sets the lines the hunk covers, and its header to match. The text git
    /// adds after the ranges, such as the function around the hunk, is kept.
    fn set_ranges(&mut self, old: Range<u32>, new: Range<u32>) {
        self.old_start = start_of(&old);
        self.old_lines = old.len() as u32;
        self.new_start = start_of(&new);
        self.new_lines = new.len() as u32;
        let section = self.header.splitn(3, "@@").nth(2).unwrap_or("");
        self.header = format!(
            "@@ -{},{} +{},{} @@{}",
            self.old_start, self.old_lines, self.new_start, self.new_lines, section
        );
    }

    /// The unchanged lines at the end of the hunk.
    fn trailing_context(&self) -> usize {
        self.lines
            .iter()
            .rev()
            .take_while(|line| line.kind == LineKind::Context)
            .count()
    }

    /// Adds lines of the old file after the hunk, as unchanged lines.
    fn append_context(&mut self, old_lines: Range<u32>, content: &[String]) {
        let (old, new) = (self.old_range(), self.new_range());
        let offset = new.end as i64 - old.end as i64;
        self.lines
            .extend(context_lines(old_lines.clone(), offset, content));
        let count = old_lines.len() as u32;
        self.set_ranges(old.start..old_lines.end, new.start..new.end + count);
    }

    /// Adds lines of the old file before the hunk, as unchanged lines.
    fn prepend_context(&mut self, old_lines: Range<u32>, content: &[String]) {
        let (old, new) = (self.old_range(), self.new_range());
        let offset = new.start as i64 - old.start as i64;
        let count = old_lines.len();
        self.lines
            .splice(0..0, context_lines(old_lines.clone(), offset, content));
        for (removed, added) in &mut self.pairs {
            *removed += count;
            *added += count;
        }
        let new_start = (old_lines.start as i64 + offset) as u32;
        self.set_ranges(old_lines.start..old.end, new_start..new.end);
    }

    /// Adds the lines of the hunk right after this one.
    fn merge(&mut self, next: DiffHunk) {
        let offset = self.lines.len();
        let (old, new) = (self.old_range(), self.new_range());
        let (next_old, next_new) = (next.old_range(), next.new_range());
        self.lines.extend(next.lines);
        self.pairs.extend(
            next.pairs
                .into_iter()
                .map(|(removed, added)| (removed + offset, added + offset)),
        );
        self.set_ranges(old.start..next_old.end, new.start..next_new.end);
    }
}

/// Unchanged lines of the old file, numbered in the new file `offset` lines
/// apart.
fn context_lines(
    old_lines: Range<u32>,
    offset: i64,
    content: &[String],
) -> impl Iterator<Item = DiffLine> + '_ {
    old_lines.map(move |n| DiffLine {
        kind: LineKind::Context,
        old_lineno: Some(n),
        new_lineno: Some((n as i64 + offset) as u32),
        content: content.get(n as usize - 1).cloned().unwrap_or_default(),
        emphasis: Vec::new(),
    })
}

impl FileDiff {
    /// The runs of unchanged lines left out before, between and after the hunks.
    pub fn gaps(&self) -> Vec<Gap> {
        let mut gaps = Vec::new();
        // The line of the old file after the previous hunk
        let mut next_line = 1;
        for (i, hunk) in self.hunks.iter().enumerate() {
            let range = hunk.old_range();
            if range.start > next_line {
                gaps.push(Gap {
                    before_hunk: i,
                    old_start: next_line,
                    count: Some(range.start - next_line),
                });
            }
            next_line = range.end;
        }

        let Some(last) = self.hunks.last() else {
            return gaps;
        };
        let count = match &self.old_content {
            Some(content) => {
                let end = content.len() as u32 + 1;
                if end <= next_line {
                    return gaps;
                }
                Some(end - next_line)
            }
            None if self.old_id.is_some() && last.trailing_context() >= DEFAULT_CONTEXT_LINES => {
                None
            }
            None => return gaps,
        };
        gaps.push(Gap {
            before_hunk: self.hunks.len(),
            old_start: next_line,
            count,
        });
        gaps
    }

    /// Reveals lines of the gap before a hunk, or after the last one, as
    /// unchanged lines of the hunks around it. Hunks join once the gap between
    /// them is gone. The old version must be read first, into `old_content`.
    /// Returns whether any line was revealed.
    pub fn reveal(&mut self, before_hunk: usize, how: Reveal) -> bool {
        let Some(gap) = self
            .gaps()
            .into_iter()
            .find(|g| g.before_hunk == before_hunk)
        else {
            return false;
        };
        // Taken out while the hunks change, and put back
        let Some(content) = self.old_content.take() else {
            return false;
        };
        let revealed = self.reveal_gap(&gap, how, &content);
        self.old_content = Some(content);
        revealed
    }

    fn reveal_gap(&mut self, gap: &Gap, how: Reveal, content: &[String]) -> bool {
        let before_hunk = gap.before_hunk;
        let Some(count) = gap.count else {
            return false;
        };
        let (start, end) = (gap.old_start, gap.old_start + count);
        let above = before_hunk.checked_sub(1);
        let below = (before_hunk < self.hunks.len()).then_some(before_hunk);

        let (to_above, to_below) = match (how, above, below) {
            (Reveal::Down(n), Some(_), _) => (start..end.min(start + n), end..end),
            (Reveal::Up(n), _, Some(_)) => (start..start, end.saturating_sub(n).max(start)..end),
            (Reveal::All, Some(_), _) => (start..end, end..end),
            (Reveal::All, None, Some(_)) => (start..start, start..end),
            _ => return false,
        };
        let closed = to_above.len() + to_below.len() == count as usize;

        if let (Some(above), false) = (above, to_above.is_empty()) {
            self.hunks[above].append_context(to_above, content);
        }
        if let (Some(below), false) = (below, to_below.is_empty()) {
            self.hunks[below].prepend_context(to_below, content);
        }
        if let (true, Some(above), Some(below)) = (closed, above, below) {
            let next = self.hunks.remove(below);
            self.hunks[above].merge(next);
        }
        true
    }
}

/// Reads the lines of the old version of a file, for `FileDiff::reveal`.
pub type OldContentReader = Rc<dyn Fn(&FileDiff) -> Result<Vec<String>, Error>>;

/// An `OldContentReader` that reads from the repository.
pub fn old_content_reader(repository_manager: &Rc<RepositoryManager>) -> OldContentReader {
    let repository_manager = repository_manager.clone();
    Rc::new(move |file| read_old_content(&repository_manager.open()?, file))
}

/// Reads the lines of the old version of a file, for `FileDiff::reveal`.
fn read_old_content(repo: &Repository, file: &FileDiff) -> Result<Vec<String>, Error> {
    let Some(id) = file.old_id else {
        return Ok(Vec::new());
    };
    let blob = repo.find_blob(id)?;
    Ok(String::from_utf8_lossy(blob.content())
        .lines()
        .map(|line| line.to_string())
        .collect())
}
