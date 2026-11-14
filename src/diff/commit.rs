//! The diff of a commit against its parent, read from the repository.

use git2::{Diff, DiffFindOptions, Error, Oid, Repository, Tree};

use super::from_git::status_of;
use super::model::FileStatus;
use crate::repository_manager::RepositoryManager;

/// The diff between two trees, with files that were renamed found as renamed
/// rather than as one deleted and one added. The file list and the diff of a
/// commit both come from it, so that they have the same files in the same order.
pub fn diff_trees<'repo>(
    repo: &'repo Repository,
    old: Option<&Tree>,
    new: Option<&Tree>,
) -> Result<Diff<'repo>, Error> {
    let mut diff = repo.diff_tree_to_tree(old, new, None)?;
    let mut opts = DiffFindOptions::new();
    opts.renames(true);
    diff.find_similar(Some(&mut opts))?;
    Ok(diff)
}

pub struct ListCommitDiffFileEntry {
    pub new_file_path: Option<String>,
    pub old_file_path: Option<String>,
    pub status: FileStatus,
}

pub struct ListCommitDiffResult {
    pub current_oid: Oid,
    pub parent_oid: Oid,
    pub files: Vec<ListCommitDiffFileEntry>,
}

pub fn list_commit_diff_files(
    repository_manager: &RepositoryManager,
    oid: Oid,
) -> Result<ListCommitDiffResult, Error> {
    let repo = repository_manager.open()?;
    let current_commit = repo.find_commit(oid)?;
    // TODO: treat multiple commit
    let parent_commit = current_commit.parent(0)?;

    let parent_tree = parent_commit.tree()?;
    let current_tree = current_commit.tree()?;
    let diff = diff_trees(&repo, Some(&parent_tree), Some(&current_tree))?;

    let mut files: Vec<ListCommitDiffFileEntry> = Vec::new();
    diff.foreach(
        &mut |delta, _| {
            let file_entry = ListCommitDiffFileEntry {
                old_file_path: delta
                    .old_file()
                    .path()
                    .and_then(|p| p.to_str())
                    .map(|s| s.to_string()),
                new_file_path: delta
                    .new_file()
                    .path()
                    .and_then(|p| p.to_str())
                    .map(|s| s.to_string()),
                status: status_of(delta.status()),
            };
            files.push(file_entry);
            true
        },
        None,
        None,
        None,
    )?;

    Ok(ListCommitDiffResult {
        current_oid: oid,
        parent_oid: parent_commit.id(),
        files,
    })
}

impl ListCommitDiffFileEntry {
    pub fn format_file_move(&self) -> String {
        let old_file_path = self.old_file_path.as_deref();
        let new_file_path = self.new_file_path.as_deref();

        if let Some(new_file) = new_file_path {
            if let Some(old_file) = old_file_path {
                if new_file != old_file {
                    return format!("{} -> {}", old_file, new_file);
                }
            }

            return new_file.to_owned();
        }

        "".to_string()
    }
}
