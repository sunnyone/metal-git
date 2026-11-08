use futures::channel::oneshot;
use futures::future::{self, FutureExt, Shared};
use gtk::{gio, glib};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    pub fn as_flag(&self) -> &AtomicBool {
        &self.0
    }
}

pub enum TaskOutcome<T, E> {
    Finished(Result<T, E>),
    Cancelled,
    Panicked,
}

struct RunningTask {
    token: CancelToken,
    finished: Shared<oneshot::Receiver<()>>,
}

/// Runs blocking work off the main thread, one task at a time.
/// Must be used from a thread that owns its GLib main context.
#[derive(Default)]
pub struct TaskRunner {
    running: Rc<RefCell<Option<RunningTask>>>,
}

impl TaskRunner {
    pub fn new() -> Rc<TaskRunner> {
        Rc::new(TaskRunner::default())
    }

    /// Returns false without running `work` if a task is already running.
    pub fn start<T, E, W, OnFinished>(&self, work: W, on_finished: OnFinished) -> bool
    where
        T: Send + 'static,
        E: Send + 'static,
        W: FnOnce(&CancelToken) -> Result<T, E> + Send + 'static,
        OnFinished: FnOnce(TaskOutcome<T, E>) + 'static,
    {
        if self.is_running() {
            return false;
        }

        let token = CancelToken::default();
        let (finished_sender, finished_receiver) = oneshot::channel();
        self.running.replace(Some(RunningTask {
            token: token.clone(),
            finished: finished_receiver.shared(),
        }));

        let worker_token = token.clone();
        let join_handle = gio::spawn_blocking(move || work(&worker_token));

        let running = self.running.clone();
        glib::spawn_future_local(async move {
            let outcome = match join_handle.await {
                Ok(Err(_)) if token.is_cancelled() => TaskOutcome::Cancelled,
                Ok(result) => TaskOutcome::Finished(result),
                Err(_) => TaskOutcome::Panicked,
            };
            running.replace(None);
            on_finished(outcome);
            let _ = finished_sender.send(());
        });

        true
    }

    pub fn cancel(&self) {
        if let Some(task) = self.running.borrow().as_ref() {
            task.token.cancel();
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.borrow().is_some()
    }

    /// Cancels the running task and waits until it finishes or `timeout` elapses.
    pub async fn shutdown(&self, timeout: Duration) {
        let finished = match self.running.borrow().as_ref() {
            Some(task) => {
                task.token.cancel();
                task.finished.clone()
            }
            None => return,
        };

        future::select(finished, glib::timeout_future(timeout)).await;
    }
}
