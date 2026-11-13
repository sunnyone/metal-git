use git2::build::CheckoutBuilder;
use git2::{Branch, BranchType, Config, Cred, CredentialType, Error, FetchOptions, Oid, RemoteCallbacks, Repository, RepositoryState};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Debug)]
pub struct FetchProgress {
    pub remote_name: String,
    pub received_objects: usize,
    pub total_objects: usize,
    pub received_bytes: usize,
}

#[derive(Debug, Default)]
pub struct FetchSummary {
    pub remote_names: Vec<String>,
    pub received_objects: usize,
    pub received_bytes: usize,
}

#[derive(Debug)]
pub struct PullSummary {
    pub branch_name: String,
    pub upstream_name: String,
    pub fast_forwarded_commits: usize,
}

/// Fetches all remotes of the repository, like `git fetch --all`.
/// Setting `cancelled` to true aborts the fetch with an error.
pub fn fetch_all<F>(repo: &Repository, cancelled: &AtomicBool, mut on_progress: F) -> Result<FetchSummary, Error>
where
    F: FnMut(&FetchProgress),
{
    let mut summary = FetchSummary::default();

    let remote_names: Vec<String> = repo.remotes()?.iter().flatten().flatten().map(String::from).collect();
    for remote_name in remote_names {
        let (received_objects, received_bytes) = fetch_remote(repo, &remote_name, cancelled, &mut on_progress)?;
        summary.received_objects += received_objects;
        summary.received_bytes += received_bytes;
        summary.remote_names.push(remote_name);
    }

    Ok(summary)
}

/// Fetches the upstream of the current branch and fast-forwards to it, like `git pull --ff-only`.
/// Local changes are kept; the pull fails if the update would overwrite them.
pub fn pull_fast_forward<F>(repo: &Repository, cancelled: &AtomicBool, mut on_progress: F) -> Result<PullSummary, Error>
where
    F: FnMut(&FetchProgress),
{
    if repo.state() != RepositoryState::Clean {
        return Err(Error::from_str("Another Git operation is in progress"));
    }

    let head = repo.head()?;
    if !head.is_branch() {
        return Err(Error::from_str("HEAD is not on a branch"));
    }
    let branch_ref = head.name()?.to_string();
    let branch_name = head.shorthand()?.to_string();
    let branch = Branch::wrap(head);

    let no_upstream = || Error::from_str(&format!("Branch {} has no upstream", branch_name));
    let upstream = branch.upstream().map_err(|_| no_upstream())?;
    let upstream_name = upstream.name()?.ok_or_else(no_upstream)?.to_string();
    drop(upstream);

    let remote_name = repo.branch_upstream_remote(&branch_ref).map_err(|_| no_upstream())?;
    let remote_name = remote_name.as_str().map_err(|_| no_upstream())?.to_string();
    fetch_remote(repo, &remote_name, cancelled, &mut on_progress)?;

    if cancelled.load(Ordering::SeqCst) {
        return Err(Error::from_str("Pull cancelled"));
    }

    let current_oid = branch.get().target().ok_or_else(no_upstream)?;
    let upstream_oid = branch.upstream()?.get().target().ok_or_else(no_upstream)?;
    let annotated = repo.find_annotated_commit(upstream_oid)?;
    let (analysis, _) = repo.merge_analysis(&[&annotated])?;

    let summary = |fast_forwarded_commits| PullSummary {
        branch_name: branch_name.clone(),
        upstream_name: upstream_name.clone(),
        fast_forwarded_commits,
    };
    if analysis.is_up_to_date() {
        return Ok(summary(0));
    }
    if !analysis.is_fast_forward() {
        return Err(Error::from_str(&format!(
            "Cannot fast-forward {} to {}: the branches have diverged",
            branch_name, upstream_name
        )));
    }

    let (fast_forwarded_commits, _) = repo.graph_ahead_behind(upstream_oid, current_oid)?;

    // Update the working tree first, so that a conflict leaves the branch where it was
    let target = repo.find_commit(upstream_oid)?;
    repo.checkout_tree(target.as_object(), Some(CheckoutBuilder::new().safe()))?;
    repo.find_reference(&branch_ref)?.set_target(upstream_oid, "pull: fast-forward")?;

    Ok(summary(fast_forwarded_commits))
}

fn fetch_remote<F>(
    repo: &Repository,
    remote_name: &str,
    cancelled: &AtomicBool,
    on_progress: &mut F,
) -> Result<(usize, usize), Error>
where
    F: FnMut(&FetchProgress),
{
    if cancelled.load(Ordering::SeqCst) {
        return Err(Error::from_str("Fetch cancelled"));
    }

    let mut remote = repo.find_remote(remote_name)?;

    {
        let mut credentials = CredentialProvider::new(repo.config()?);
        let mut callbacks = RemoteCallbacks::new();
        callbacks.credentials(move |url, username, allowed| credentials.next(url, username, allowed));
        callbacks.sideband_progress(|_| !cancelled.load(Ordering::SeqCst));
        callbacks.update_tips(|_, _, _| !cancelled.load(Ordering::SeqCst));
        callbacks.transfer_progress(|stats| {
            on_progress(&FetchProgress {
                remote_name: remote_name.to_string(),
                received_objects: stats.received_objects(),
                total_objects: stats.total_objects(),
                received_bytes: stats.received_bytes(),
            });
            !cancelled.load(Ordering::SeqCst)
        });

        let mut options = FetchOptions::new();
        options.remote_callbacks(callbacks);
        remote.fetch::<&str>(&[], Some(&mut options), None)?;
    }

    let stats = remote.stats();
    Ok((stats.received_objects(), stats.received_bytes()))
}

/// Tries each credential type once, since libgit2 calls the callback again when one is rejected.
struct CredentialProvider {
    config: Config,
    tried_ssh_agent: bool,
    tried_credential_helper: bool,
    tried_default: bool,
}

impl CredentialProvider {
    fn new(config: Config) -> CredentialProvider {
        CredentialProvider {
            config,
            tried_ssh_agent: false,
            tried_credential_helper: false,
            tried_default: false,
        }
    }

    fn next(&mut self, url: &str, username: Option<&str>, allowed: CredentialType) -> Result<Cred, Error> {
        if allowed.contains(CredentialType::USERNAME) {
            return Cred::username(username.unwrap_or("git"));
        }

        if allowed.contains(CredentialType::SSH_KEY) && !self.tried_ssh_agent {
            self.tried_ssh_agent = true;
            return Cred::ssh_key_from_agent(username.unwrap_or("git"));
        }

        if allowed.contains(CredentialType::USER_PASS_PLAINTEXT) && !self.tried_credential_helper {
            self.tried_credential_helper = true;
            return Cred::credential_helper(&self.config, url, username);
        }

        if allowed.contains(CredentialType::DEFAULT) && !self.tried_default {
            self.tried_default = true;
            return Cred::default();
        }

        Err(Error::from_str(&format!("Authentication failed for {}", mask_url_credentials(url))))
    }
}

/// The branch to push and where it goes: `local_branch` -> `remote_name`/`remote_branch`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PushTarget {
    pub local_branch: String,
    pub remote_name: String,
    pub remote_branch: String,
}

impl PushTarget {
    /// The remote branch as it is shown in the history, like `origin/main`.
    pub fn remote_ref_display(&self) -> String {
        format!("{}/{}", self.remote_name, self.remote_branch)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PushOptions {
    pub set_upstream: bool,
    pub force_with_lease: bool,
}

/// Everything the push needs, fixed when the user confirms it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PushRequest {
    pub target: PushTarget,
    pub options: PushOptions,
    /// The remote branch as last fetched, or None when it did not exist.
    /// A force push is refused unless the remote branch is still here.
    pub expected_remote_oid: Option<Oid>,
}

#[derive(Debug)]
pub struct PushSummary {
    pub target: PushTarget,
    pub pushed_commits: usize,
    pub created: bool,
    pub forced: bool,
}

/// Commits that are pushed, newest first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OutgoingCommits {
    /// Counted up to `OUTGOING_COUNT_LIMIT`.
    pub count: usize,
    pub count_is_capped: bool,
    /// At most `OUTGOING_LIST_LIMIT` commits.
    pub commits: Vec<OutgoingCommit>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutgoingCommit {
    pub short_id: String,
    pub summary: String,
}

pub const OUTGOING_COUNT_LIMIT: usize = 10_000;
pub const OUTGOING_LIST_LIMIT: usize = 100;

/// What the push dialog shows for a target. Reads local refs only, without network access.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PushPreview {
    pub display_url: String,
    /// The remote branch as last fetched; None when it does not exist on the remote yet.
    pub remote_oid: Option<Oid>,
    /// The current upstream of the local branch, like `origin/main`.
    pub upstream: Option<String>,
    pub outgoing: OutgoingCommits,
    /// Commits on the remote branch that the local branch does not have.
    /// A normal push is rejected when this is not zero, and a force push removes them.
    pub remote_only_commits: usize,
}

impl PushPreview {
    pub fn creates_branch(&self) -> bool {
        self.remote_oid.is_none()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PushButtonState {
    NoBranch,
    NoRemote,
    /// The branch has no upstream, so pushing it creates a new remote branch.
    NoUpstream { branch: String },
    Ahead { branch: String, upstream: String, count: usize },
    UpToDate { branch: String, upstream: String },
}

/// The state of the pull button, for the branch HEAD is on. The counts are against the
/// upstream as last fetched; the pull itself fetches again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PullButtonState {
    NoBranch,
    NoUpstream { branch: String },
    /// The upstream has commits the branch does not have, and a fast-forward can take them.
    Behind { branch: String, upstream: String, count: usize },
    /// Both have commits the other does not have, so a fast-forward is not possible.
    Diverged { branch: String, upstream: String, behind: usize, ahead: usize },
    UpToDate { branch: String, upstream: String },
}

pub fn local_branch_names(repo: &Repository) -> Result<Vec<String>, Error> {
    let mut names = Vec::new();
    for branch in repo.branches(Some(BranchType::Local))? {
        let (branch, _) = branch?;
        if let Ok(Some(name)) = branch.name() {
            names.push(name.to_string());
        }
    }
    names.sort();
    Ok(names)
}

pub fn remote_names(repo: &Repository) -> Result<Vec<String>, Error> {
    Ok(repo.remotes()?.iter().flatten().flatten().map(String::from).collect())
}

/// The branch HEAD is on, or None when HEAD is detached or unborn.
pub fn current_branch_name(repo: &Repository) -> Option<String> {
    let head = repo.head().ok()?;
    if !head.is_branch() {
        return None;
    }
    head.shorthand().ok().map(String::from)
}

/// Where `local_branch` is pushed unless the user changes it: its upstream,
/// or else a branch of the same name on `origin` (or the first remote).
pub fn default_push_target(repo: &Repository, local_branch: &str) -> Result<PushTarget, Error> {
    let branch_ref = format!("refs/heads/{}", local_branch);
    let upstream_remote = repo.branch_upstream_remote(&branch_ref).ok()
        .and_then(|buf| buf.as_str().ok().map(String::from));
    let upstream_merge = repo.branch_upstream_merge(&branch_ref).ok()
        .and_then(|buf| buf.as_str().ok().map(String::from));

    if let (Some(remote_name), Some(merge)) = (upstream_remote, upstream_merge) {
        if let Some(remote_branch) = merge.strip_prefix("refs/heads/") {
            return Ok(PushTarget {
                local_branch: local_branch.to_string(),
                remote_name,
                remote_branch: remote_branch.to_string(),
            });
        }
    }

    let remotes = remote_names(repo)?;
    let remote_name = if remotes.iter().any(|name| name == "origin") {
        "origin".to_string()
    } else {
        remotes.into_iter().next().ok_or_else(|| Error::from_str("No remotes are configured"))?
    };
    Ok(PushTarget {
        local_branch: local_branch.to_string(),
        remote_name,
        remote_branch: local_branch.to_string(),
    })
}

pub fn is_valid_branch_name(name: &str) -> bool {
    !name.is_empty() && git2::Reference::is_valid_name(&format!("refs/heads/{}", name))
}

/// Hides the password in a URL like `https://user:token@host/repo.git`.
///
/// Strings that are not absolute URLs (scp-like `git@host:repo`, local paths) and
/// URLs without a password are returned unchanged, so they are not normalized.
pub fn mask_url_credentials(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(mut parsed) if parsed.password().is_some() => {
            // Fails only for URLs that cannot have credentials, which have no password.
            let _ = parsed.set_password(Some("***"));
            parsed.to_string()
        }
        _ => url.to_string(),
    }
}

pub fn preview_push(repo: &Repository, target: &PushTarget) -> Result<PushPreview, Error> {
    let remote = repo.find_remote(&target.remote_name)?;
    let url = match remote.pushurl() {
        Ok(Some(push_url)) => Some(push_url),
        _ => remote.url().ok(),
    };
    let display_url = url.map(mask_url_credentials).unwrap_or_default();

    let local_oid = local_branch_oid(repo, &target.local_branch)?;
    let tracking_ref = tracking_ref_name(&remote, &target.remote_branch);
    let remote_oid = tracking_ref.as_deref().and_then(|name| repo.refname_to_id(name).ok());

    let upstream = repo
        .find_branch(&target.local_branch, BranchType::Local)?
        .upstream()
        .ok()
        .and_then(|upstream| upstream.name().ok().flatten().map(String::from));

    let outgoing = outgoing_commits(repo, local_oid, remote_oid, &target.remote_name)?;
    let remote_only_commits = match remote_oid {
        Some(remote_oid) => repo.graph_ahead_behind(local_oid, remote_oid)?.1,
        None => 0,
    };

    Ok(PushPreview { display_url, remote_oid, upstream, outgoing, remote_only_commits })
}

/// The state of the toolbar button, for the branch HEAD is on.
pub fn push_button_state(repo: &Repository) -> PushButtonState {
    let Some(branch_name) = current_branch_name(repo) else {
        return PushButtonState::NoBranch;
    };
    if remote_names(repo).map_or(true, |names| names.is_empty()) {
        return PushButtonState::NoRemote;
    }

    let upstream = repo
        .find_branch(&branch_name, BranchType::Local)
        .and_then(|branch| branch.upstream());
    let Ok(upstream) = upstream else {
        return PushButtonState::NoUpstream { branch: branch_name };
    };
    let upstream_name = upstream.name().ok().flatten().unwrap_or("").to_string();

    let count = match (local_branch_oid(repo, &branch_name), upstream.get().target()) {
        (Ok(local_oid), Some(upstream_oid)) => {
            repo.graph_ahead_behind(local_oid, upstream_oid).map_or(0, |(ahead, _)| ahead)
        }
        _ => 0,
    };
    if count == 0 {
        PushButtonState::UpToDate { branch: branch_name, upstream: upstream_name }
    } else {
        PushButtonState::Ahead { branch: branch_name, upstream: upstream_name, count }
    }
}

/// The state of the pull button, for the branch HEAD is on.
pub fn pull_button_state(repo: &Repository) -> PullButtonState {
    let Some(branch_name) = current_branch_name(repo) else {
        return PullButtonState::NoBranch;
    };

    let upstream = repo
        .find_branch(&branch_name, BranchType::Local)
        .and_then(|branch| branch.upstream());
    let Ok(upstream) = upstream else {
        return PullButtonState::NoUpstream { branch: branch_name };
    };
    let upstream_name = upstream.name().ok().flatten().unwrap_or("").to_string();

    let (ahead, behind) = match (local_branch_oid(repo, &branch_name), upstream.get().target()) {
        (Ok(local_oid), Some(upstream_oid)) => {
            repo.graph_ahead_behind(local_oid, upstream_oid).unwrap_or((0, 0))
        }
        _ => (0, 0),
    };
    match (behind, ahead) {
        (0, _) => PullButtonState::UpToDate { branch: branch_name, upstream: upstream_name },
        (count, 0) => PullButtonState::Behind { branch: branch_name, upstream: upstream_name, count },
        (behind, ahead) => {
            PullButtonState::Diverged { branch: branch_name, upstream: upstream_name, behind, ahead }
        }
    }
}

/// Pushes a local branch to a remote branch, like
/// `git push [--force-with-lease] [-u] <remote> <local>:<remote_branch>`.
pub fn push<F>(
    repo: &Repository,
    request: &PushRequest,
    cancelled: &AtomicBool,
    mut on_progress: F,
) -> Result<PushSummary, Error>
where
    F: FnMut(&FetchProgress),
{
    let target = &request.target;
    let force = request.options.force_with_lease;
    if !is_valid_branch_name(&target.remote_branch) {
        return Err(Error::from_str(&format!("{} is not a valid branch name", target.remote_branch)));
    }
    if cancelled.load(Ordering::SeqCst) {
        return Err(Error::from_str("Push cancelled"));
    }

    let local_oid = local_branch_oid(repo, &target.local_branch)?;
    let mut remote = repo.find_remote(&target.remote_name)?;
    let remote_ref = format!("refs/heads/{}", target.remote_branch);
    let refspec = format!(
        "{}refs/heads/{}:{}",
        if force { "+" } else { "" },
        target.local_branch,
        remote_ref
    );

    let tracking_oid = tracking_ref_name(&remote, &target.remote_branch)
        .and_then(|name| repo.refname_to_id(&name).ok());
    let pushed_commits = outgoing_commits(repo, local_oid, tracking_oid, &target.remote_name)?.count;

    let remote_display = target.remote_ref_display();
    let mut lease_broken = false;
    let mut remote_was_missing = false;
    let mut rejection: Option<String> = None;
    let push_result = {
        let mut credentials = CredentialProvider::new(repo.config()?);
        let mut callbacks = RemoteCallbacks::new();
        callbacks.credentials(move |url, username, allowed| credentials.next(url, username, allowed));
        callbacks.sideband_progress(|_| !cancelled.load(Ordering::SeqCst));
        callbacks.push_transfer_progress(|current, total, bytes| {
            on_progress(&FetchProgress {
                remote_name: target.remote_name.clone(),
                received_objects: current,
                total_objects: total,
                received_bytes: bytes,
            });
        });
        callbacks.push_negotiation(|updates| {
            if cancelled.load(Ordering::SeqCst) {
                return Err(Error::from_str("Push cancelled"));
            }
            for update in updates {
                if update.dst_refname().ok() != Some(remote_ref.as_str()) {
                    continue;
                }
                remote_was_missing = update.src().is_zero();
                let remote_now = if update.src().is_zero() { None } else { Some(update.src()) };
                if force && remote_now != request.expected_remote_oid {
                    lease_broken = true;
                    return Err(Error::from_str("lease broken"));
                }
            }
            Ok(())
        });
        callbacks.push_update_reference(|_, status| {
            if let Some(status) = status {
                rejection = Some(status.to_string());
            }
            Ok(())
        });

        let mut options = git2::PushOptions::new();
        options.remote_callbacks(callbacks);
        remote.push(&[refspec.as_str()], Some(&mut options))
    };

    if lease_broken {
        return Err(Error::from_str(&format!(
            "{} has changed since your last fetch. Fetch and check the history before force pushing.",
            remote_display
        )));
    }
    if let Err(error) = push_result {
        if error.code() == git2::ErrorCode::NotFastForward {
            return Err(non_fast_forward_error(target));
        }
        return Err(error);
    }
    if let Some(status) = rejection {
        if status.contains("fast-forward") || status.contains("fetch first") {
            return Err(non_fast_forward_error(target));
        }
        return Err(Error::from_str(&format!("The remote rejected the push to {}: {}", remote_display, status)));
    }

    if request.options.set_upstream {
        repo.find_branch(&target.local_branch, BranchType::Local)?
            .set_upstream(Some(&remote_display))?;
    }

    Ok(PushSummary {
        target: target.clone(),
        pushed_commits,
        created: remote_was_missing,
        forced: force,
    })
}

fn non_fast_forward_error(target: &PushTarget) -> Error {
    Error::from_str(&format!(
        "{} has commits that {} does not have. Pull first, then push again.",
        target.remote_ref_display(),
        target.local_branch
    ))
}

fn local_branch_oid(repo: &Repository, branch_name: &str) -> Result<Oid, Error> {
    repo.find_branch(branch_name, BranchType::Local)?
        .get()
        .target()
        .ok_or_else(|| Error::from_str(&format!("Branch {} has no commit", branch_name)))
}

/// The remote-tracking ref for a branch on the remote, like `refs/remotes/origin/main`.
fn tracking_ref_name(remote: &git2::Remote, remote_branch: &str) -> Option<String> {
    let remote_ref = format!("refs/heads/{}", remote_branch);
    remote
        .refspecs()
        .filter(|spec| spec.direction() == git2::Direction::Fetch)
        .find(|spec| spec.src_matches(&remote_ref))
        .and_then(|spec| spec.transform(&remote_ref).ok())
        .and_then(|buf| buf.as_str().ok().map(String::from))
}

/// Commits reachable from `local_oid` but not from the remote branch, or, when the
/// remote branch does not exist yet, from any branch already fetched from that remote.
fn outgoing_commits(
    repo: &Repository,
    local_oid: Oid,
    remote_oid: Option<Oid>,
    remote_name: &str,
) -> Result<OutgoingCommits, Error> {
    let mut walk = repo.revwalk()?;
    walk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::TIME)?;
    walk.push(local_oid)?;
    match remote_oid {
        Some(remote_oid) => walk.hide(remote_oid)?,
        None => walk.hide_glob(&format!("refs/remotes/{}/*", remote_name))?,
    }

    let mut outgoing = OutgoingCommits::default();
    for oid in walk {
        let oid = oid?;
        if outgoing.count == OUTGOING_COUNT_LIMIT {
            outgoing.count_is_capped = true;
            break;
        }
        outgoing.count += 1;
        if outgoing.commits.len() < OUTGOING_LIST_LIMIT {
            let commit = repo.find_commit(oid)?;
            outgoing.commits.push(OutgoingCommit {
                short_id: oid.to_string()[..7].to_string(),
                summary: commit.summary().ok().flatten().unwrap_or("").to_string(),
            });
        }
    }
    Ok(outgoing)
}
