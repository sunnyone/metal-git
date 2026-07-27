use git2::build::CheckoutBuilder;
use git2::{BranchType, Error, Oid, Repository};
#[cfg(unix)]
use std::ffi::OsStr;
use std::fs;
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BranchKind {
    Local,
    Remote,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BranchCandidate {
    pub name: String,
    pub kind: BranchKind,
    pub is_current: bool,
}

pub fn find_branch_candidates(
    repo: &Repository,
    commit_id: Oid,
) -> Result<Vec<BranchCandidate>, Error> {
    let local_candidates = collect_candidates(repo, commit_id, BranchType::Local)?;
    let remote_candidates = collect_candidates(repo, commit_id, BranchType::Remote)?;
    Ok(local_candidates
        .into_iter()
        .chain(remote_candidates)
        .collect())
}

pub fn switch_local_branch(repo: &Repository, branch_name: &str) -> Result<(), Error> {
    let branch = repo.find_branch(branch_name, BranchType::Local)?;
    let commit = branch.get().peel_to_commit()?;
    let tree = commit.tree()?;
    let reference_name = branch
        .get()
        .name()
        .ok_or_else(|| Error::from_str("Local branch reference name is not valid UTF-8"))?;
    ensure_branch_is_not_checked_out_elsewhere(repo, reference_name)?;
    let mut transaction = repo.transaction()?;
    transaction.lock_ref("HEAD")?;
    transaction.set_symbolic_target("HEAD", reference_name, None, "switch branch")?;
    repo.checkout_tree(tree.as_object(), Some(CheckoutBuilder::new().safe()))?;
    transaction.commit()
}

pub fn create_tracking_branch_and_switch(
    repo: &Repository,
    remote_branch_name: &str,
) -> Result<(), Error> {
    let remote_branch = repo.find_branch(remote_branch_name, BranchType::Remote)?;
    let local_branch_name = local_name_for_remote(repo, &remote_branch, remote_branch_name)?;
    let target = remote_branch.get().peel_to_commit()?;
    let mut local_branch = repo.branch(&local_branch_name, &target, false)?;
    if let Err(error) = local_branch.set_upstream(Some(remote_branch_name)) {
        return rollback_created_branch(&mut local_branch, error);
    }
    if let Err(error) = switch_local_branch(repo, &local_branch_name) {
        return rollback_created_branch(&mut local_branch, error);
    }
    Ok(())
}

fn collect_candidates(
    repo: &Repository,
    commit_id: Oid,
    branch_type: BranchType,
) -> Result<Vec<BranchCandidate>, Error> {
    repo.branches(Some(branch_type))?
        .map(|branch_result| -> Result<Option<BranchCandidate>, Error> {
            let (branch, _) = branch_result?;
            if branch.get().symbolic_target().is_some() {
                return Ok(None);
            }

            let Some(name) = branch.name()? else {
                return Ok(None);
            };
            let target = branch
                .get()
                .target()
                .ok_or_else(|| Error::from_str("Branch does not point directly to a commit"))?;
            if target != commit_id {
                return Ok(None);
            }

            Ok(Some(BranchCandidate {
                is_current: branch.is_head(),
                kind: match branch_type {
                    BranchType::Local => BranchKind::Local,
                    BranchType::Remote => BranchKind::Remote,
                },
                name: name.to_owned(),
            }))
        })
        .filter_map(Result::transpose)
        .collect()
}

fn rollback_created_branch(
    local_branch: &mut git2::Branch<'_>,
    operation_error: Error,
) -> Result<(), Error> {
    match local_branch.delete() {
        Ok(()) => Err(operation_error),
        Err(rollback_error) => Err(Error::from_str(&format!(
            "{operation_error}; failed to roll back created branch: {rollback_error}"
        ))),
    }
}

fn ensure_branch_is_not_checked_out_elsewhere(
    repo: &Repository,
    reference_name: &str,
) -> Result<(), Error> {
    let main_repo = if repo.is_worktree() {
        let main_repo = open_main_repository(repo)?;
        ensure_repository_does_not_use_branch(repo, &main_repo, reference_name)?;
        Some(main_repo)
    } else {
        None
    };
    let common_path = match &main_repo {
        Some(main_repo) => main_repo.path(),
        None => repo.path(),
    };

    for worktree_name in repo.worktrees()?.iter_bytes() {
        let worktree_repo = open_linked_worktree_repository(common_path, worktree_name)?;
        ensure_repository_does_not_use_branch(repo, &worktree_repo, reference_name)?;
    }
    Ok(())
}

fn open_main_repository(repo: &Repository) -> Result<Repository, Error> {
    let common_dir = fs::read_to_string(repo.path().join("commondir")).map_err(|error| {
        Error::from_str(&format!("Failed to read Git common directory: {error}"))
    })?;
    let common_dir =
        common_dir.trim_end_matches(|character| character == '\r' || character == '\n');
    Repository::open(repo.path().join(common_dir))
}

fn open_linked_worktree_repository(
    common_path: &Path,
    worktree_name: &[u8],
) -> Result<Repository, Error> {
    Repository::open(worktree_admin_path(common_path, worktree_name)?)
}

#[cfg(unix)]
fn worktree_admin_path(common_path: &Path, worktree_name: &[u8]) -> Result<PathBuf, Error> {
    Ok(common_path
        .join("worktrees")
        .join(OsStr::from_bytes(worktree_name)))
}

#[cfg(not(unix))]
fn worktree_admin_path(common_path: &Path, worktree_name: &[u8]) -> Result<PathBuf, Error> {
    let worktree_name = std::str::from_utf8(worktree_name)
        .map_err(|_| Error::from_str("Worktree name is not valid UTF-8 on this platform"))?;
    Ok(common_path.join("worktrees").join(worktree_name))
}

fn ensure_repository_does_not_use_branch(
    current_repo: &Repository,
    other_repo: &Repository,
    reference_name: &str,
) -> Result<(), Error> {
    if current_repo.path() == other_repo.path() {
        return Ok(());
    }
    if other_repo.find_reference("HEAD")?.symbolic_target() == Some(reference_name) {
        return Err(Error::from_str(
            "Branch is already checked out in another worktree",
        ));
    }
    Ok(())
}

fn local_name_for_remote(
    repo: &Repository,
    remote_branch: &git2::Branch<'_>,
    remote_branch_name: &str,
) -> Result<String, Error> {
    let reference_name = remote_branch
        .get()
        .name()
        .ok_or_else(|| Error::from_str("Remote branch reference name is not valid UTF-8"))?;
    let remote_name = repo.branch_remote_name(reference_name)?;
    let remote_name = remote_name
        .as_str()
        .ok_or_else(|| Error::from_str("Remote name is not valid UTF-8"))?;
    let prefix = format!("{remote_name}/");
    let local_name = remote_branch_name
        .strip_prefix(&prefix)
        .ok_or_else(|| Error::from_str("Remote branch name does not match its remote"))?;
    if local_name.is_empty() {
        return Err(Error::from_str(
            "Remote branch name must include a local branch name",
        ));
    }

    Ok(local_name.to_owned())
}
