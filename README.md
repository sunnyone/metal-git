# What's this?

GUI Git Frontend written in gtk-rs (gtk+ Rust wrapper) and git2-rs (libgit2 Rust wrapper).


# How to build?

Install build dependencies at first. This program requires gtk+3 and libgit2.

In Ubuntu 14.04 / 16.04, run this:

```
$ sudo apt install libgtk-3-dev libgit2-dev cmake
```

If build dependencies are properly installed, cargo should succeed.

```
$ cargo run
```

This program opens a git repository at the working directory.

If you want a binary, use cargo build --release.

# Author

Yoichi Imai <sunnyone41@gmail.com>
