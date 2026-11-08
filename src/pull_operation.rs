use crate::background_task::CancelToken;
use crate::fetch_operation::describe_transfer;
use crate::long_running_operation::{describe_common, LongRunningEvent, LongRunningOperation, LongRunningStatus};
use crate::remote_operation::{self, FetchProgress, PullSummary};
use std::path::PathBuf;

pub struct PullOperation {
    pub git_dir: PathBuf,
}

impl LongRunningOperation for PullOperation {
    const NAME: &'static str = "Pull";
    type Progress = FetchProgress;
    type Output = PullSummary;
    type Error = String;

    fn run(self, cancel: &CancelToken, report: &dyn Fn(FetchProgress)) -> Result<PullSummary, String> {
        git2::Repository::open(&self.git_dir)
            .and_then(|repo| {
                remote_operation::pull_fast_forward(&repo, cancel.as_flag(), |progress| report(progress.clone()))
            })
            .map_err(|err| err.message().to_string())
    }

    fn describe(event: &LongRunningEvent<FetchProgress, PullSummary, String>) -> LongRunningStatus {
        match event {
            LongRunningEvent::Started => LongRunningStatus::Started {
                message: "Pulling...".to_string(),
                cancellable: true,
            },
            LongRunningEvent::Progress(progress) => describe_transfer("Pulling", progress),
            LongRunningEvent::Succeeded(summary) => LongRunningStatus::Succeeded {
                message: describe_summary(summary),
                reload_history: true,
            },
            other => describe_common(Self::NAME, other),
        }
    }
}

fn describe_summary(summary: &PullSummary) -> String {
    match summary.fast_forwarded_commits {
        0 => format!("{} is already up to date with {}", summary.branch_name, summary.upstream_name),
        1 => format!("Pulled 1 commit from {} into {}", summary.upstream_name, summary.branch_name),
        count => format!("Pulled {} commits from {} into {}", count, summary.upstream_name, summary.branch_name),
    }
}
