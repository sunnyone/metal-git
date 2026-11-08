use git2::{Config, Cred, CredentialType, Error, FetchOptions, RemoteCallbacks, Repository};
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

/// Fetches all remotes of the repository, like `git fetch --all`.
/// Setting `cancelled` to true aborts the fetch with an error.
pub fn fetch_all<F>(repo: &Repository, cancelled: &AtomicBool, mut on_progress: F) -> Result<FetchSummary, Error>
where
    F: FnMut(&FetchProgress),
{
    let mut summary = FetchSummary::default();

    let remote_names: Vec<String> = repo.remotes()?.iter().flatten().map(String::from).collect();
    for remote_name in remote_names {
        if cancelled.load(Ordering::SeqCst) {
            return Err(Error::from_str("Fetch cancelled"));
        }

        let mut remote = repo.find_remote(&remote_name)?;

        {
            let mut credentials = CredentialProvider::new(repo.config()?);
            let mut callbacks = RemoteCallbacks::new();
            callbacks.credentials(move |url, username, allowed| credentials.next(url, username, allowed));
            callbacks.sideband_progress(|_| !cancelled.load(Ordering::SeqCst));
            callbacks.update_tips(|_, _, _| !cancelled.load(Ordering::SeqCst));
            callbacks.transfer_progress(|stats| {
                on_progress(&FetchProgress {
                    remote_name: remote_name.clone(),
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
        summary.received_objects += stats.received_objects();
        summary.received_bytes += stats.received_bytes();
        summary.remote_names.push(remote_name);
    }

    Ok(summary)
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
