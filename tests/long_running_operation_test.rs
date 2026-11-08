extern crate metal_git;

use gtk::glib;
use metal_git::background_task::{CancelToken, TaskRunner};
use metal_git::long_running_operation::{
	describe_common, start_operation, status_channel, LongRunningEvent, LongRunningOperation, LongRunningStatus,
	LongRunningStatusReceiver, LongRunningStatusSender,
};
use std::future::Future;
use std::sync::mpsc;
use std::time::Duration;

fn block_on<F: Future>(future: F) -> F::Output {
	let context = glib::MainContext::new();
	context.with_thread_default(|| context.block_on(future)).unwrap()
}

fn progress(message: &str) -> LongRunningStatus {
	LongRunningStatus::Progress { message: message.to_string(), fraction: None }
}

fn started() -> LongRunningStatus {
	LongRunningStatus::Started { message: "Testing...".to_string(), cancellable: true }
}

fn succeeded() -> LongRunningStatus {
	LongRunningStatus::Succeeded { message: "Tested".to_string(), reload_history: true }
}

enum Behavior {
	Succeed,
	Fail,
	WaitForCancel,
	Panic,
	WaitFor(mpsc::Receiver<()>),
}

struct TestOperation {
	behavior: Behavior,
}

impl LongRunningOperation for TestOperation {
	const NAME: &'static str = "Test";
	type Progress = &'static str;
	type Output = ();
	type Error = String;

	fn run(self, cancel: &CancelToken, report: &dyn Fn(&'static str)) -> Result<(), String> {
		match self.behavior {
			Behavior::Succeed => {
				report("working");
				Ok(())
			}
			Behavior::Fail => Err("broken".to_string()),
			Behavior::WaitForCancel => {
				while !cancel.is_cancelled() {
					std::thread::sleep(Duration::from_millis(1));
				}
				Err("aborted".to_string())
			}
			Behavior::Panic => panic!("expected panic in test"),
			Behavior::WaitFor(receiver) => {
				let _ = receiver.recv();
				Ok(())
			}
		}
	}

	fn describe(event: &LongRunningEvent<&'static str, (), String>) -> LongRunningStatus {
		match event {
			LongRunningEvent::Started => started(),
			LongRunningEvent::Progress(message) => progress(message),
			LongRunningEvent::Succeeded(()) => succeeded(),
			other => describe_common(Self::NAME, other),
		}
	}
}

async fn until_finished(receiver: &mut LongRunningStatusReceiver) -> Vec<LongRunningStatus> {
	let mut statuses = Vec::new();
	while let Some(status) = receiver.next().await {
		let running = status.is_running();
		statuses.push(status);
		if !running {
			break;
		}
	}
	statuses
}

fn start(runner: &TaskRunner, behavior: Behavior, status: &LongRunningStatusSender) {
	assert!(start_operation(runner, TestOperation { behavior }, status));
}

#[test]
pub fn success_starts_and_ends_in_order() {
	block_on(async {
		let (sender, mut receiver) = status_channel();
		start(&TaskRunner::new(), Behavior::Succeed, &sender);

		let statuses = until_finished(&mut receiver).await;

		assert_eq!(Some(&started()), statuses.first());
		assert_eq!(Some(&succeeded()), statuses.last());
	});
}

#[test]
pub fn failure_includes_the_error() {
	block_on(async {
		let (sender, mut receiver) = status_channel();
		start(&TaskRunner::new(), Behavior::Fail, &sender);

		let statuses = until_finished(&mut receiver).await;

		assert_eq!(
			Some(&LongRunningStatus::Failed { message: "Test failed: broken".to_string() }),
			statuses.last()
		);
	});
}

#[test]
pub fn cancel_reports_cancelled() {
	block_on(async {
		let (sender, mut receiver) = status_channel();
		let runner = TaskRunner::new();
		start(&runner, Behavior::WaitForCancel, &sender);

		runner.cancel();
		let statuses = until_finished(&mut receiver).await;

		assert_eq!(
			Some(&LongRunningStatus::Cancelled { message: "Test cancelled".to_string() }),
			statuses.last()
		);
	});
}

#[test]
pub fn panic_reports_failed() {
	block_on(async {
		let (sender, mut receiver) = status_channel();
		start(&TaskRunner::new(), Behavior::Panic, &sender);

		let statuses = until_finished(&mut receiver).await;

		assert_eq!(
			Some(&LongRunningStatus::Failed { message: "Test stopped unexpectedly".to_string() }),
			statuses.last()
		);
	});
}

#[test]
pub fn second_operation_is_rejected_without_status() {
	block_on(async {
		let (sender, mut receiver) = status_channel();
		let runner = TaskRunner::new();
		let (release, wait) = mpsc::channel();
		start(&runner, Behavior::WaitFor(wait), &sender);

		assert!(!start_operation(&runner, TestOperation { behavior: Behavior::Succeed }, &sender));

		release.send(()).unwrap();
		assert_eq!(vec![started(), succeeded()], until_finished(&mut receiver).await);
	});
}

#[test]
pub fn queued_progress_is_merged_into_the_latest() {
	block_on(async {
		let (sender, mut receiver) = status_channel();
		sender.send(progress("1"));
		sender.send(progress("2"));
		sender.send(progress("3"));

		assert_eq!(Some(progress("3")), receiver.next().await);
	});
}

#[test]
pub fn final_status_after_progress_is_not_lost() {
	block_on(async {
		let (sender, mut receiver) = status_channel();
		sender.send(progress("1"));
		sender.send(progress("2"));
		sender.send(succeeded());
		sender.send(started());

		assert_eq!(Some(succeeded()), receiver.next().await);
		assert_eq!(Some(started()), receiver.next().await);
	});
}

#[test]
pub fn started_is_not_merged() {
	block_on(async {
		let (sender, mut receiver) = status_channel();
		sender.send(started());
		sender.send(progress("1"));

		assert_eq!(Some(started()), receiver.next().await);
		assert_eq!(Some(progress("1")), receiver.next().await);
	});
}

#[test]
pub fn running_and_reload_flags() {
	assert!(started().is_running());
	assert!(progress("1").is_running());
	assert!(!succeeded().is_running());
	assert!(succeeded().reload_history());
	assert!(!LongRunningStatus::Succeeded { message: String::new(), reload_history: false }.reload_history());
	assert!(!LongRunningStatus::Failed { message: String::new() }.reload_history());
}

#[test]
pub fn common_wording_for_undescribed_events() {
	type Event = LongRunningEvent<(), (), String>;

	assert_eq!(
		LongRunningStatus::Started { message: "Test...".to_string(), cancellable: true },
		describe_common("Test", &Event::Started)
	);
	assert_eq!(
		LongRunningStatus::Progress { message: "Test...".to_string(), fraction: None },
		describe_common("Test", &Event::Progress(()))
	);
	assert_eq!(
		LongRunningStatus::Succeeded { message: "Test finished".to_string(), reload_history: false },
		describe_common("Test", &Event::Succeeded(()))
	);
}
