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

## Ubuntu packages

On Ubuntu 22.04 or later:

```bash
$ sudo apt install libgtk-4-dev libgit2-dev libglib2.0-dev-bin cmake
```

## Author

Yoichi Imai <sunnyone41@gmail.com>
