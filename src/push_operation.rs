use crate::background_task::CancelToken;
use crate::fetch_operation::describe_transfer;
use crate::long_running_operation::{describe_common, LongRunningEvent, LongRunningOperation, LongRunningStatus};
use crate::remote_operation::{self, FetchProgress, PushOptions, PushPreview, PushRequest, PushSummary, PushTarget};
use std::path::PathBuf;

pub struct PushOperation {
    pub git_dir: PathBuf,
    pub request: PushRequest,
}

impl LongRunningOperation for PushOperation {
    const NAME: &'static str = "Push";
    type Progress = FetchProgress;
    type Output = PushSummary;
    type Error = String;

    fn run(self, cancel: &CancelToken, report: &dyn Fn(FetchProgress)) -> Result<PushSummary, String> {
        git2::Repository::open(&self.git_dir)
            .and_then(|repo| {
                remote_operation::push(&repo, &self.request, cancel.as_flag(), |progress| report(progress.clone()))
            })
            .map_err(|err| err.message().to_string())
    }

    fn describe(event: &LongRunningEvent<FetchProgress, PushSummary, String>) -> LongRunningStatus {
        match event {
            // libgit2 cannot stop a push while it is uploading
            LongRunningEvent::Started => LongRunningStatus::Started {
                message: "Pushing...".to_string(),
                cancellable: false,
            },
            LongRunningEvent::Progress(progress) => describe_transfer("Pushing", progress),
            LongRunningEvent::Succeeded(summary) => LongRunningStatus::Succeeded {
                message: describe_summary(summary),
                reload_history: true,
            },
            other => describe_common(Self::NAME, other),
        }
    }
}

fn describe_summary(summary: &PushSummary) -> String {
    let destination = summary.target.remote_ref_display();
    let local = &summary.target.local_branch;
    if summary.forced {
        format!("Force pushed {} to {}", local, destination)
    } else if summary.created {
        format!("Pushed {} to a new branch {}", local, destination)
    } else {
        match summary.pushed_commits {
            0 => format!("{} is already up to date", destination),
            1 => format!("Pushed 1 commit from {} to {}", local, destination),
            count => format!("Pushed {} commits from {} to {}", count, local, destination),
        }
    }
}

/// How carefully the push dialog asks before pushing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfirmMode {
    /// The usual push: Enter pushes.
    Routine,
    /// Something differs from the usual push, so Enter does nothing.
    Careful,
    /// A force push: Enter does nothing, and a second confirmation follows.
    Force,
}

pub fn confirm_mode(target: &PushTarget, options: &PushOptions, default_remote: &str) -> ConfirmMode {
    if options.force_with_lease {
        ConfirmMode::Force
    } else if target.local_branch != target.remote_branch || target.remote_name != default_remote {
        ConfirmMode::Careful
    } else {
        ConfirmMode::Routine
    }
}

/// Warnings shown above the options, in the order they are listed.
pub fn push_warnings(target: &PushTarget, preview: &PushPreview, options: &PushOptions, default_remote: &str) -> Vec<String> {
    let destination = target.remote_ref_display();
    let mut warnings = Vec::new();
    if target.local_branch != target.remote_branch {
        warnings.push(format!(
            "The local and remote branch names differ. {} will be pushed to {}.",
            target.local_branch, destination
        ));
    }
    if target.remote_name != default_remote {
        warnings.push(format!("This pushes to {}, not to {}.", target.remote_name, default_remote));
    }
    if preview.remote_only_commits > 0 && !options.force_with_lease {
        warnings.push(format!(
            "{} has {} that {} does not have. The push will be rejected; pull first.",
            destination,
            commit_count_text(preview.remote_only_commits),
            target.local_branch
        ));
    }
    warnings
}

pub fn commit_count_text(count: usize) -> String {
    if count == 1 {
        "1 commit".to_string()
    } else {
        format!("{} commits", count)
    }
}
