//! Runs tests that use GTK on one thread.
//!
//! GTK belongs to the thread that initializes it, while the test harness runs
//! each test on a thread of its own, so a second test that used GTK on another
//! thread would fail to initialize it.

use std::panic::{self, AssertUnwindSafe};
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, OnceLock};
use std::thread;

type Job = Box<dyn FnOnce() + Send>;

fn gtk_thread() -> &'static Mutex<Sender<Job>> {
    static SENDER: OnceLock<Mutex<Sender<Job>>> = OnceLock::new();
    SENDER.get_or_init(|| {
        let (sender, receiver) = mpsc::channel::<Job>();
        thread::Builder::new()
            .name("gtk-test".into())
            .spawn(move || {
                gtk::init().unwrap();
                for job in receiver {
                    job();
                }
            })
            .unwrap();
        Mutex::new(sender)
    })
}

/// Runs a test on the GTK thread, one at a time, and fails the calling test if
/// it panics.
pub fn run(test: impl FnOnce() + Send + 'static) {
    let (done_sender, done) = mpsc::channel();
    let job: Job = Box::new(move || {
        let result = panic::catch_unwind(AssertUnwindSafe(test));
        let _ = done_sender.send(result);
    });
    gtk_thread().lock().unwrap().send(job).unwrap();
    if let Err(payload) = done.recv().unwrap() {
        panic::resume_unwind(payload);
    }
}
