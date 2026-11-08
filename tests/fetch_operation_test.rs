extern crate metal_git;

use metal_git::fetch_operation::FetchOperation;
use metal_git::long_running_operation::{LongRunningEvent, LongRunningOperation, LongRunningStatus};
use metal_git::remote_operation::{FetchProgress, FetchSummary};

type Event = LongRunningEvent<FetchProgress, FetchSummary, String>;

fn describe_progress(received_objects: usize, total_objects: usize, received_bytes: usize) -> LongRunningStatus {
	FetchOperation::describe(&Event::Progress(FetchProgress {
		remote_name: "origin".to_string(),
		received_objects,
		total_objects,
		received_bytes,
	}))
}

fn describe_success(remote_names: &[&str], received_objects: usize, received_bytes: usize) -> LongRunningStatus {
	FetchOperation::describe(&Event::Succeeded(FetchSummary {
		remote_names: remote_names.iter().map(|name| name.to_string()).collect(),
		received_objects,
		received_bytes,
	}))
}

fn succeeded(message: &str) -> LongRunningStatus {
	LongRunningStatus::Succeeded { message: message.to_string(), reload_history: true }
}

#[test]
pub fn started_message() {
	assert_eq!(
		LongRunningStatus::Started { message: "Fetching...".to_string(), cancellable: true },
		FetchOperation::describe(&Event::Started)
	);
}

#[test]
pub fn progress_shows_objects_and_fraction() {
	assert_eq!(
		LongRunningStatus::Progress {
			message: "Fetching origin: 45/120 objects (1.2 MiB)".to_string(),
			fraction: Some(0.375),
		},
		describe_progress(45, 120, 1_258_291)
	);
}

#[test]
pub fn progress_without_total_has_no_fraction() {
	assert_eq!(
		LongRunningStatus::Progress {
			message: "Fetching origin: 0/0 objects (512 B)".to_string(),
			fraction: None,
		},
		describe_progress(0, 0, 512)
	);
}

#[test]
pub fn success_messages() {
	assert_eq!(succeeded("No remotes to fetch"), describe_success(&[], 0, 0));
	assert_eq!(succeeded("Fetched origin: already up to date"), describe_success(&["origin"], 0, 0));
	assert_eq!(
		succeeded("Fetched big, origin: 12 objects (2.0 KiB)"),
		describe_success(&["big", "origin"], 12, 2048)
	);
}

#[test]
pub fn failure_cancel_and_panic_use_common_messages() {
	assert_eq!(
		LongRunningStatus::Failed { message: "Fetch failed: no route".to_string() },
		FetchOperation::describe(&Event::Failed("no route".to_string()))
	);
	assert_eq!(
		LongRunningStatus::Cancelled { message: "Fetch cancelled".to_string() },
		FetchOperation::describe(&Event::Cancelled)
	);
	assert_eq!(
		LongRunningStatus::Failed { message: "Fetch stopped unexpectedly".to_string() },
		FetchOperation::describe(&Event::Panicked)
	);
}
