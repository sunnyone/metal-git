//! Diffs as data, apart from how they are drawn: what changed in each file,
//! read from the repository and from git2, with the changed words marked.

mod commit;
mod emphasis;
mod from_git;
mod model;

pub use commit::{
    diff_trees, list_commit_diff_files, ListCommitDiffFileEntry, ListCommitDiffResult,
};
pub use emphasis::{changed_ranges, emphasize_changes};
pub use from_git::{file_from_patch, files_from_diff, status_of};
pub use model::{
    initially_open, DiffHunk, DiffLine, FileDiff, FileStatus, LineKind, SideBySideRow,
    MAX_OPEN_FILE_LINES, MAX_OPEN_TOTAL_LINES,
};
