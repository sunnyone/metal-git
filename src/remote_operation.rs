use git2::build::CheckoutBuilder;
use git2::{Branch, Config, Cred, CredentialType, Error, FetchOptions, RemoteCallbacks, Repository, RepositoryState};
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

        Err(Error::from_str(&format!("Authentication failed for {}", url)))
    }
}
