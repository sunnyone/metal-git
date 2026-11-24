//! Diffs as data, apart from how they are drawn: what changed in each file,
//! read from the repository and from git2, with the changed words marked.
//! The windows take them through a `Differ`.

mod commit;
mod context;
mod differ;
mod emphasis;
mod from_git;
mod model;
mod syntax;

pub use commit::{diff_trees, ListCommitDiffFileEntry, ListCommitDiffResult};
pub use context::{Gap, Reveal};
pub use differ::{DiffTarget, Differ};
pub use emphasis::{changed_ranges, emphasize_changes};
pub use from_git::{file_from_patch, files_from_diff, status_of, DiffSettings};
pub use model::{
    initially_open, DiffHunk, DiffLine, FileDiff, FileStatus, LineKind, SideBySideRow,
    MAX_OPEN_FILE_LINES, MAX_OPEN_TOTAL_LINES,
};
pub use syntax::{Segment, Token, MAX_HIGHLIGHT_LINE_BYTES};
