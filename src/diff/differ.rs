//! Taking the diffs the windows show, all in one place.

use git2::{Error, Oid, Repository};
use std::fs;
use std::path::Path;
use std::rc::Rc;

use super::commit::{diff_trees, list_commit_diff_files, ListCommitDiffResult};
use super::context::read_old_content;
use super::from_git::{files_from_diff, DiffSettings};
use super::model::FileDiff;
use crate::repository_manager::RepositoryManager;

/// What to take the diff of.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiffTarget {
    /// A commit against its first parent
    Commit(Oid),
    /// A file of the work tree against the index, or all of it for a file
    /// that is not tracked yet
    WorkTree(String),
    /// A file of the index against HEAD
    Index(String),
}

/// Takes diffs from the repository. It holds only the repository manager, so
/// it is cheap to clone.
#[derive(Clone)]
pub struct Differ {
    repository_manager: Rc<RepositoryManager>,
}

impl Differ {
    pub fn new(repository_manager: Rc<RepositoryManager>) -> Differ {
        Differ { repository_manager }
    }

    /// The files of the diff of a target, taken as the settings say, with their
    /// changed words not marked yet. See `FileDiff::emphasize`.
    pub fn files(
        &self,
        target: &DiffTarget,
        settings: DiffSettings,
    ) -> Result<Vec<FileDiff>, Error> {
        let repo = self.repository_manager.open()?;
        let mut files = match target {
            DiffTarget::Commit(oid) => {
                let commit = repo.find_commit(*oid)?;
                let parent_tree = commit.parent(0)?.tree()?;
                let tree = commit.tree()?;
                files_from_diff(&diff_trees(
                    &repo,
                    Some(&parent_tree),
                    Some(&tree),
                    settings,
                )?)?
            }
            DiffTarget::WorkTree(path) => {
                // A file not tracked yet has no old version, so the settings
                // do not change it
                if repo.status_file(Path::new(path))? == git2::Status::WT_NEW {
                    return Ok(vec![untracked_file(&repo, path)]);
                }
                let mut options = settings.options();
                options.pathspec(path);
                files_from_diff(&repo.diff_index_to_workdir(None, Some(&mut options))?)?
            }
            DiffTarget::Index(path) => {
                let mut options = settings.options();
                options.pathspec(path);
                let head_tree = repo.head()?.peel_to_tree()?;
                files_from_diff(&repo.diff_tree_to_index(
                    Some(&head_tree),
                    None,
                    Some(&mut options),
                )?)?
            }
        };
        for file in &mut files {
            file.whitespace_ignored = settings.ignore_whitespace;
        }
        Ok(files)
    }

    /// The files a commit changed, in the order of `files` for the commit.
    pub fn commit_file_list(&self, oid: Oid) -> Result<ListCommitDiffResult, Error> {
        list_commit_diff_files(&self.repository_manager.open()?, oid)
    }

    /// Reads the lines of the old version of a file, for `FileDiff::reveal`.
    pub fn old_content(&self, file: &FileDiff) -> Result<Vec<String>, Error> {
        read_old_content(&self.repository_manager.open()?, file)
    }
}

/// A file that is not tracked yet, as all added. One that cannot be read
/// shows why in place of its lines.
fn untracked_file(repo: &Repository, path: &str) -> FileDiff {
    let text = match repo.workdir() {
        Some(workdir) => fs::read_to_string(workdir.join(path))
            .unwrap_or_else(|err| format!("This file is not browsable: {}", err)),
        None => "This file is not browsable: the repository has no work tree".to_string(),
    };
    FileDiff::all_added(path, &text)
}
