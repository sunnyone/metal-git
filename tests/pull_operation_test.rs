extern crate metal_git;

use metal_git::long_running_operation::{LongRunningEvent, LongRunningOperation, LongRunningStatus};
use metal_git::pull_operation::PullOperation;
use metal_git::remote_operation::{FetchProgress, PullSummary};

type Event = LongRunningEvent<FetchProgress, PullSummary, String>;

fn describe_success(fast_forwarded_commits: usize) -> LongRunningStatus {
	PullOperation::describe(&Event::Succeeded(PullSummary {
		branch_name: "main".to_string(),
		upstream_name: "origin/main".to_string(),
		fast_forwarded_commits,
	}))
}

fn succeeded(message: &str) -> LongRunningStatus {
	LongRunningStatus::Succeeded { message: message.to_string(), reload_history: true }
}

#[test]
pub fn started_message() {
	assert_eq!(
		LongRunningStatus::Started { message: "Pulling...".to_string() },
		PullOperation::describe(&Event::Started)
	);
}

#[test]
pub fn progress_message() {
	let progress = FetchProgress {
		remote_name: "origin".to_string(),
		received_objects: 1,
		total_objects: 4,
		received_bytes: 2048,
	};

	assert_eq!(
		LongRunningStatus::Progress {
			message: "Pulling origin: 1/4 objects (2.0 KiB)".to_string(),
			fraction: Some(0.25),
		},
		PullOperation::describe(&Event::Progress(progress))
	);
}

#[test]
pub fn success_messages() {
	assert_eq!(succeeded("main is already up to date with origin/main"), describe_success(0));
	assert_eq!(succeeded("Pulled 1 commit from origin/main into main"), describe_success(1));
	assert_eq!(succeeded("Pulled 3 commits from origin/main into main"), describe_success(3));
}

#[test]
pub fn failure_uses_common_message() {
	assert_eq!(
		LongRunningStatus::Failed { message: "Pull failed: diverged".to_string() },
		PullOperation::describe(&Event::Failed("diverged".to_string()))
	);
}
