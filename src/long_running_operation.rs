use crate::background_task::{CancelToken, TaskOutcome, TaskRunner};
use futures::channel::mpsc;
use futures::StreamExt;
use std::fmt::Display;

#[derive(Clone, Debug, PartialEq)]
pub enum LongRunningStatus {
    Started { message: String, cancellable: bool },
    Progress { message: String, fraction: Option<f64> },
    Succeeded { message: String, reload_history: bool },
    Cancelled { message: String },
    Failed { message: String },
}

impl LongRunningStatus {
    pub fn is_running(&self) -> bool {
        matches!(self, LongRunningStatus::Started { .. } | LongRunningStatus::Progress { .. })
    }

    pub fn reload_history(&self) -> bool {
        matches!(self, LongRunningStatus::Succeeded { reload_history: true, .. })
    }
}

#[derive(Clone)]
pub struct LongRunningStatusSender(mpsc::UnboundedSender<LongRunningStatus>);

impl LongRunningStatusSender {
    pub fn send(&self, status: LongRunningStatus) {
        let _ = self.0.unbounded_send(status);
    }
}

pub struct LongRunningStatusReceiver(mpsc::UnboundedReceiver<LongRunningStatus>);

impl LongRunningStatusReceiver {
    /// Consecutive queued `Progress` messages are merged into the latest one.
    pub async fn next(&mut self) -> Option<LongRunningStatus> {
        let mut status = self.0.next().await?;
        while matches!(status, LongRunningStatus::Progress { .. }) {
            match self.0.try_recv() {
                Ok(newer) => status = newer,
                Err(_) => break,
            }
        }
        Some(status)
    }
}

pub fn status_channel() -> (LongRunningStatusSender, LongRunningStatusReceiver) {
    let (sender, receiver) = mpsc::unbounded();
    (LongRunningStatusSender(sender), LongRunningStatusReceiver(receiver))
}

/// What happened to an operation.
pub enum LongRunningEvent<P, T, E> {
    Started,
    Progress(P),
    Succeeded(T),
    Failed(E),
    Cancelled,
    Panicked,
}

/// Slow work whose state is reported as events and shown as status messages.
pub trait LongRunningOperation: Send + 'static {
    const NAME: &'static str;
    type Progress: Send + 'static;
    type Output: Send + 'static;
    type Error: Display + Send + 'static;

    fn run(self, cancel: &CancelToken, report: &dyn Fn(Self::Progress)) -> Result<Self::Output, Self::Error>;

    fn describe(event: &LongRunningEvent<Self::Progress, Self::Output, Self::Error>) -> LongRunningStatus;
}

/// Default wording, for the events an operation does not describe itself.
pub fn describe_common<P, T, E: Display>(name: &str, event: &LongRunningEvent<P, T, E>) -> LongRunningStatus {
    match event {
        LongRunningEvent::Started => LongRunningStatus::Started {
            message: format!("{}...", name),
            cancellable: true,
        },
        LongRunningEvent::Progress(_) => LongRunningStatus::Progress {
            message: format!("{}...", name),
            fraction: None,
        },
        LongRunningEvent::Succeeded(_) => LongRunningStatus::Succeeded {
            message: format!("{} finished", name),
            reload_history: false,
        },
        LongRunningEvent::Failed(error) => LongRunningStatus::Failed {
            message: format!("{} failed: {}", name, error),
        },
        LongRunningEvent::Cancelled => LongRunningStatus::Cancelled {
            message: format!("{} cancelled", name),
        },
        LongRunningEvent::Panicked => LongRunningStatus::Failed {
            message: format!("{} stopped unexpectedly", name),
        },
    }
}

/// Returns false without sending anything if a task is already running.
pub fn start_operation<O: LongRunningOperation>(
    runner: &TaskRunner,
    operation: O,
    status: &LongRunningStatusSender,
) -> bool {
    if runner.is_running() {
        return false;
    }

    status.send(O::describe(&LongRunningEvent::Started));

    // Events differ per operation, so convert them before the shared channel
    let worker_status = status.clone();
    let finished_status = status.clone();
    runner.start(
        move |cancel| {
            operation.run(cancel, &|progress| {
                worker_status.send(O::describe(&LongRunningEvent::Progress(progress)));
            })
        },
        move |outcome| {
            let event = match outcome {
                TaskOutcome::Finished(Ok(output)) => LongRunningEvent::Succeeded(output),
                TaskOutcome::Finished(Err(error)) => LongRunningEvent::Failed(error),
                TaskOutcome::Cancelled => LongRunningEvent::Cancelled,
                TaskOutcome::Panicked => LongRunningEvent::Panicked,
            };
            finished_status.send(O::describe(&event));
        },
    )
}
