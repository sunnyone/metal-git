# Metal Git

## What's this?

Metal Git is a GUI Git frontend written in Rust with `gtk4-rs` and `git2-rs`.

The application opens the Git repository in the current working directory.

## Caution

This program is now in experimental stage. It may destroy your files or repositories, so use carefully.

## Build and run

Install build dependencies first. This project requires GTK 4, libgit2, CMake, and `glib-compile-resources`.

Use a recent Rust toolchain, then run:

```bash
$ cargo run
```

To build a release binary:

```bash
$ cargo build --release
$ sudo cp target/release/metal-git /usr/local/bin
```

If `glib-compile-resources` is missing, install the GLib development tools package provided by your distribution.

## Startup timing log

To diagnose slow startup in a large repository, run the binary from that repository
with timing logs enabled and save stderr:

```bash
cd /path/to/large-repository
METAL_GIT_STARTUP_LOG=1 /path/to/metal-git 2>startup.log
```

Use the same build (preferably `--release`) when comparing runs. Logging is disabled
unless `METAL_GIT_STARTUP_LOG=1` is set; no log file is created automatically.

Each `[metal-git-startup]` line contains the process ID, `elapsed_ms` since application
entry, an `event` (`begin`, `progress`, `error`, or `end`), and a `stage`.
`duration_ms` measures time within a stage using a monotonic clock. Nested stages
include their children, so their durations should not be added together. `end`
means the scope exited; an `error` event marks a failed operation.

Stages cover GTK activation, window construction, repository discovery, reference
enumeration, revision walking and graph construction, history row creation, initial
commit/diff loading, and waiting for the first history frame. Reference, commit,
and row stages report `count`, with progress every 1,000 items. The
`history.revwalk_first_commit` stage isolates work done before libgit2 yields its
first commit.

The final `startup.first_history_frame` event reports total startup time and the
number of history rows. It is emitted after GTK finishes the first frame with the
history list mapped, including layout and painting (not the display compositor's
presentation time). Timing stops there, so later refreshes and selections do not
produce startup logs. A zero row count can indicate empty or failed history loading;
check preceding `error` events. If startup stalls, the last unmatched `begin` or
`progress` event identifies the stage still in progress.

## Ubuntu packages

On Ubuntu 22.04 or later:

```bash
$ sudo apt install libgtk-4-dev libgit2-dev libglib2.0-dev-bin cmake
```

## Author

Yoichi Imai <sunnyone41@gmail.com>
