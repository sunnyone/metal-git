extern crate git2;
extern crate metal_git;

use metal_git::long_running_operation::{LongRunningEvent, LongRunningOperation, LongRunningStatus};
use metal_git::push_operation::{self, ConfirmMode, PushOperation};
use metal_git::remote_operation::{FetchProgress, OutgoingCommits, PushOptions, PushPreview, PushSummary, PushTarget};

type Event = LongRunningEvent<FetchProgress, PushSummary, String>;

fn target(local_branch: &str, remote_name: &str, remote_branch: &str) -> PushTarget {
	PushTarget {
		local_branch: local_branch.to_string(),
		remote_name: remote_name.to_string(),
		remote_branch: remote_branch.to_string(),
	}
}

fn describe_success(pushed_commits: usize, created: bool, forced: bool) -> LongRunningStatus {
	PushOperation::describe(&Event::Succeeded(PushSummary {
		target: target("main", "origin", "main"),
		pushed_commits,
		created,
		forced,
	}))
}

fn succeeded(message: &str) -> LongRunningStatus {
	LongRunningStatus::Succeeded { message: message.to_string(), reload_history: true }
}

fn preview(remote_only_commits: usize) -> PushPreview {
	PushPreview {
		display_url: String::new(),
		remote_oid: None,
		upstream: None,
		outgoing: OutgoingCommits::default(),
		remote_only_commits,
	}
}

fn options(force_with_lease: bool) -> PushOptions {
	PushOptions { set_upstream: false, force_with_lease }
}

#[test]
pub fn started_message_cannot_be_cancelled() {
	assert_eq!(
		LongRunningStatus::Started { message: "Pushing...".to_string(), cancellable: false },
		PushOperation::describe(&Event::Started)
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
			message: "Pushing origin: 1/4 objects (2.0 KiB)".to_string(),
			fraction: Some(0.25),
		},
		PushOperation::describe(&Event::Progress(progress))
	);
}

#[test]
pub fn success_messages() {
	assert_eq!(succeeded("origin/main is already up to date"), describe_success(0, false, false));
	assert_eq!(succeeded("Pushed 1 commit from main to origin/main"), describe_success(1, false, false));
	assert_eq!(succeeded("Pushed 3 commits from main to origin/main"), describe_success(3, false, false));
	assert_eq!(succeeded("Pushed main to a new branch origin/main"), describe_success(3, true, false));
	assert_eq!(succeeded("Force pushed main to origin/main"), describe_success(3, false, true));
}

#[test]
pub fn failure_message() {
	assert_eq!(
		LongRunningStatus::Failed { message: "Push failed: rejected".to_string() },
		PushOperation::describe(&Event::Failed("rejected".to_string()))
	);
}

#[test]
pub fn usual_push_is_routine() {
	assert_eq!(ConfirmMode::Routine, push_operation::confirm_mode(&target("main", "origin", "main"), &options(false), "origin"));
}

#[test]
pub fn different_names_or_remote_need_care() {
	assert_eq!(ConfirmMode::Careful, push_operation::confirm_mode(&target("feature", "origin", "main"), &options(false), "origin"));
	assert_eq!(ConfirmMode::Careful, push_operation::confirm_mode(&target("main", "upstream", "main"), &options(false), "origin"));
}

#[test]
pub fn force_push_has_its_own_mode() {
	assert_eq!(ConfirmMode::Force, push_operation::confirm_mode(&target("main", "origin", "main"), &options(true), "origin"));
	assert_eq!(ConfirmMode::Force, push_operation::confirm_mode(&target("feature", "upstream", "main"), &options(true), "origin"));
}

#[test]
pub fn usual_push_has_no_warnings() {
	let warnings = push_operation::push_warnings(&target("main", "origin", "main"), &preview(0), &options(false), "origin");

	assert!(warnings.is_empty(), "{:?}", warnings);
}

#[test]
pub fn warnings_for_names_remote_and_rejection() {
	let warnings = push_operation::push_warnings(&target("feature", "upstream", "main"), &preview(2), &options(false), "origin");

	assert_eq!(
		vec![
			"The local and remote branch names differ. feature will be pushed to upstream/main.".to_string(),
			"This pushes to upstream, not to origin.".to_string(),
			"upstream/main has 2 commits that feature does not have. The push will be rejected; pull first.".to_string(),
		],
		warnings
	);
}

#[test]
pub fn force_push_does_not_warn_about_rejection() {
	let warnings = push_operation::push_warnings(&target("main", "origin", "main"), &preview(2), &options(true), "origin");

	assert!(warnings.is_empty(), "{:?}", warnings);
}
