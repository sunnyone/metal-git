use crate::background_task::CancelToken;
use crate::long_running_operation::{describe_common, LongRunningEvent, LongRunningOperation, LongRunningStatus};
use crate::remote_operation::{self, FetchProgress, FetchSummary};
use std::path::PathBuf;

pub struct FetchOperation {
    pub git_dir: PathBuf,
}

impl LongRunningOperation for FetchOperation {
    const NAME: &'static str = "Fetch";
    type Progress = FetchProgress;
    type Output = FetchSummary;
    type Error = String;

    fn run(self, cancel: &CancelToken, report: &dyn Fn(FetchProgress)) -> Result<FetchSummary, String> {
        git2::Repository::open(&self.git_dir)
            .and_then(|repo| {
                remote_operation::fetch_all(&repo, cancel.as_flag(), |progress| report(progress.clone()))
            })
            .map_err(|err| err.message().to_string())
    }

    fn describe(event: &LongRunningEvent<FetchProgress, FetchSummary, String>) -> LongRunningStatus {
        match event {
            LongRunningEvent::Started => LongRunningStatus::Started {
                message: "Fetching...".to_string(),
            },
            LongRunningEvent::Progress(progress) => LongRunningStatus::Progress {
                message: format!(
                    "Fetching {}: {}/{} objects ({})",
                    progress.remote_name,
                    progress.received_objects,
                    progress.total_objects,
                    format_bytes(progress.received_bytes),
                ),
                fraction: if progress.total_objects > 0 {
                    Some(progress.received_objects as f64 / progress.total_objects as f64)
                } else {
                    None
                },
            },
            LongRunningEvent::Succeeded(summary) => LongRunningStatus::Succeeded {
                message: describe_summary(summary),
                reload_history: true,
            },
            other => describe_common(Self::NAME, other),
        }
    }
}

fn describe_summary(summary: &FetchSummary) -> String {
    if summary.remote_names.is_empty() {
        "No remotes to fetch".to_string()
    } else if summary.received_objects == 0 {
        format!("Fetched {}: already up to date", summary.remote_names.join(", "))
    } else {
        format!(
            "Fetched {}: {} objects ({})",
            summary.remote_names.join(", "),
            summary.received_objects,
            format_bytes(summary.received_bytes),
        )
    }
}

fn format_bytes(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
    }
}
