use git2::{Error, Repository, StatusOptions};

/// The options the commit window lists files with: untracked files are included, one by one.
pub fn changed_files_status_options() -> StatusOptions {
    let mut opts = StatusOptions::new();
    opts.include_untracked(true).recurse_untracked_dirs(true);
    opts
}

/// How many files the commit window would show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChangeCounts {
    /// Files with any change, staged or not. A file changed in both places counts once.
    pub changed_files: usize,
    /// Files with staged changes.
    pub staged_files: usize,
}

pub fn count_changes(repo: &Repository) -> Result<ChangeCounts, Error> {
    if repo.is_bare() {
        return Err(Error::from_str("cannot report status on bare repository"));
    }

    let statuses = repo.statuses(Some(&mut changed_files_status_options()))?;
    let mut counts = ChangeCounts::default();
    for status in statuses.iter() {
        let staged = status.head_to_index().is_some();
        if staged || status.index_to_workdir().is_some() {
            counts.changed_files += 1;
        }
        if staged {
            counts.staged_files += 1;
        }
    }
    Ok(counts)
}
