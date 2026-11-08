//! Opt-in timings from application entry through the first history frame.
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::Instant;

static START: OnceLock<Instant> = OnceLock::new();
static ACTIVE: AtomicBool = AtomicBool::new(false);

pub fn init() {
    if std::env::var_os("METAL_GIT_STARTUP_LOG").as_deref() == Some(std::ffi::OsStr::new("1")) {
        START.get_or_init(Instant::now);
        ACTIVE.store(true, Ordering::Relaxed);
        write("begin", "startup", None, None);
    }
}

pub fn enabled() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}

fn write(event: &str, stage: &str, started: Option<Instant>, count: Option<usize>) {
    let Some(start) = START.get() else { return };
    let now = Instant::now();
    let mut line = format!(
        "[metal-git-startup] pid={} elapsed_ms={:.3} event={} stage={}",
        std::process::id(),
        now.duration_since(*start).as_secs_f64() * 1000.0,
        event,
        stage,
    );
    if let Some(started) = started {
        line.push_str(&format!(
            " duration_ms={:.3}",
            now.duration_since(started).as_secs_f64() * 1000.0
        ));
    }
    if let Some(count) = count {
        line.push_str(&format!(" count={count}"));
    }
    // Diagnostics must not crash the application if stderr is closed.
    let _ = writeln!(std::io::stderr().lock(), "{line}");
}

pub fn finish(count: usize) {
    if ACTIVE.swap(false, Ordering::Relaxed) {
        write(
            "end",
            "startup.first_history_frame",
            START.get().copied(),
            Some(count),
        );
    }
}

pub fn error(stage: &str) {
    if enabled() {
        write("error", stage, None, None);
    }
}

pub struct Span {
    stage: &'static str,
    started: Option<Instant>,
    count: Option<usize>,
}

impl Span {
    pub fn new(stage: &'static str) -> Self {
        let started = enabled().then(Instant::now);
        if started.is_some() {
            write("begin", stage, None, None);
        }
        Self {
            stage,
            started,
            count: None,
        }
    }

    pub fn set_count(&mut self, count: usize) {
        self.count = Some(count);
    }

    pub fn progress(&mut self, count: usize) {
        self.set_count(count);
        if self.started.is_some() && count % 1000 == 0 {
            write("progress", self.stage, self.started, self.count);
        }
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        if self.started.is_some() {
            write("end", self.stage, self.started, self.count);
        }
    }
}

pub fn measure<T>(stage: &'static str, f: impl FnOnce() -> T) -> T {
    let _span = Span::new(stage);
    f()
}

pub fn measure_result<T, E>(stage: &'static str, f: impl FnOnce() -> Result<T, E>) -> Result<T, E> {
    let _span = Span::new(stage);
    let result = f();
    if result.is_err() {
        error(stage);
    }
    result
}
