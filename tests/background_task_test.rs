extern crate metal_git;

use gtk::glib;
use metal_git::background_task::{CancelToken, TaskOutcome, TaskRunner};
use std::cell::RefCell;
use std::future::Future;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

fn block_on<F: Future>(future: F) -> F::Output {
	let context = glib::MainContext::new();
	context.with_thread_default(|| context.block_on(future)).unwrap()
}

// Resolves with the outcome passed to on_finished
fn run<T: Send + 'static, E: Send + 'static>(
	runner: &TaskRunner,
	work: impl FnOnce(&CancelToken) -> Result<T, E> + Send + 'static,
) -> impl Future<Output = TaskOutcome<T, E>> {
	let (sender, receiver) = futures::channel::oneshot::channel();
	assert!(runner.start(work, move |outcome| {
		let _ = sender.send(outcome);
	}));
	async move { receiver.await.unwrap() }
}

fn wait_for_cancel(token: &CancelToken) -> Result<(), ()> {
	while !token.is_cancelled() {
		std::thread::sleep(Duration::from_millis(1));
	}
	Err(())
}

#[test]
pub fn finished_with_result() {
	block_on(async {
		let runner = TaskRunner::new();
		let outcome = run(&runner, |_| Ok::<_, ()>(42)).await;

		assert!(matches!(outcome, TaskOutcome::Finished(Ok(42))));
		assert!(!runner.is_running());
	});
}

#[test]
pub fn second_start_is_rejected_while_running() {
	block_on(async {
		let runner = TaskRunner::new();
		let (release, wait) = mpsc::channel::<()>();
		let task = run(&runner, move |_| {
			let _ = wait.recv();
			Ok::<_, ()>(())
		});

		assert!(runner.is_running());
		assert!(!runner.start(|_| Ok::<_, ()>(()), |_| {}));

		release.send(()).unwrap();
		assert!(matches!(task.await, TaskOutcome::Finished(Ok(()))));
	});
}

#[test]
pub fn error_after_cancel_is_cancelled() {
	block_on(async {
		let runner = TaskRunner::new();
		let task = run(&runner, wait_for_cancel);

		runner.cancel();

		assert!(matches!(task.await, TaskOutcome::Cancelled));
	});
}

#[test]
pub fn success_after_cancel_is_finished() {
	block_on(async {
		let runner = TaskRunner::new();
		let task = run(&runner, |token| {
			let _ = wait_for_cancel(token);
			Ok::<_, ()>(1)
		});

		runner.cancel();

		assert!(matches!(task.await, TaskOutcome::Finished(Ok(1))));
	});
}

#[test]
pub fn error_without_cancel_is_finished() {
	block_on(async {
		let runner = TaskRunner::new();
		let outcome = run(&runner, |_| Err::<(), _>("failed")).await;

		assert!(matches!(outcome, TaskOutcome::Finished(Err("failed"))));
	});
}

#[test]
pub fn panic_is_reported() {
	block_on(async {
		let runner = TaskRunner::new();
		let outcome = run(&runner, |_| -> Result<(), ()> { panic!("expected panic in test") }).await;

		assert!(matches!(outcome, TaskOutcome::Panicked));
		assert!(!runner.is_running());
	});
}

#[test]
pub fn shutdown_waits_for_cleanup() {
	block_on(async {
		let runner = TaskRunner::new();
		let finished = Rc::new(RefCell::new(false));
		let f = finished.clone();
		assert!(runner.start(wait_for_cancel, move |_| {
			*f.borrow_mut() = true;
		}));

		runner.shutdown(Duration::from_secs(10)).await;

		assert!(*finished.borrow());
	});
}

#[test]
pub fn shutdown_gives_up_after_timeout() {
	block_on(async {
		let runner = TaskRunner::new();
		let (release, wait) = mpsc::channel::<()>();
		let task = run(&runner, move |_| {
			let _ = wait.recv();
			Ok::<_, ()>(())
		});

		runner.shutdown(Duration::from_millis(20)).await;
		assert!(runner.is_running());

		release.send(()).unwrap();
		task.await;
	});
}

#[test]
pub fn shutdown_without_task_returns() {
	block_on(TaskRunner::new().shutdown(Duration::from_secs(10)));
}
