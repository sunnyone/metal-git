//! Reading the files of a git2 diff into `FileDiff`s.

use git2::{Delta, Diff, Error, Patch};

use super::model::{DiffHunk, DiffLine, FileDiff, FileStatus, LineKind};

/// The files of a diff, with their changed words not marked yet. See
/// `FileDiff::emphasize`.
pub fn files_from_diff(diff: &Diff) -> Result<Vec<FileDiff>, Error> {
    (0..diff.deltas().len())
        .map(|index| {
            let delta = diff.get_delta(index).unwrap();
            match Patch::from_diff(diff, index)? {
                Some(patch) => file_from_patch_unemphasized(&patch),
                None => Ok(FileDiff {
                    old_path: path_of(delta.old_file()),
                    new_path: path_of(delta.new_file()),
                    status: status_of(delta.status()),
                    binary: delta.flags().is_binary(),
                    hunks: Vec::new(),
                    emphasized: true,
                }),
            }
        })
        .collect()
}

pub fn file_from_patch(patch: &Patch) -> Result<FileDiff, Error> {
    let mut file = file_from_patch_unemphasized(patch)?;
    file.emphasize();
    Ok(file)
}

fn file_from_patch_unemphasized(patch: &Patch) -> Result<FileDiff, Error> {
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

        hunks.push(DiffHunk {
            header: String::from_utf8_lossy(hunk.header())
                .trim_end()
                .to_string(),
            lines,
            pairs: Vec::new(),
        });
    }

    Ok(FileDiff {
        old_path: path_of(delta.old_file()),
        new_path: path_of(delta.new_file()),
        status: status_of(delta.status()),
        binary: delta.flags().is_binary(),
        hunks,
        emphasized: false,
    })
}

fn strip_line_break(text: &str) -> &str {
    let text = text.strip_suffix('\n').unwrap_or(text);
    text.strip_suffix('\r').unwrap_or(text)
}

fn path_of(file: git2::DiffFile) -> Option<String> {
    file.path().and_then(|p| p.to_str()).map(|s| s.to_string())
}

pub fn status_of(delta: Delta) -> FileStatus {
    match delta {
        Delta::Added | Delta::Untracked => FileStatus::Added,
        Delta::Deleted => FileStatus::Deleted,
        Delta::Modified => FileStatus::Modified,
        Delta::Renamed => FileStatus::Renamed,
        Delta::Copied => FileStatus::Copied,
        _ => FileStatus::Other,
    }
}
