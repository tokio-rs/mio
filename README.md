# MIO - Metal IO

MIO is a lightweight IO library for Rust with a focus on adding as
little overhead as possible over the OS abstractions.

## Usage

To use `mio`, first add this to your `Cargo.toml`:

```toml
[dependencies.mio]
git = "https://github.com/carllerche/mio"
```

`mio` is on [Crates.io](http://crates.io/crates/mio), but is not often updated.

Then, add this to your crate root:

```rust
extern crate mio;
```

## Features

* Event loop backed by epoll, kqueue.
* Zero allocations at runtime
* Non-blocking TCP, UDP and Unix domain sockets
* High performance timer system
* Thread safe message channel for cross thread communication

__Coming soon__

* Signal handling

## Non goals

The following are specifically omitted from MIO and are left to the user
or higher level libraries.

* File operations
* Thread pools / multi-threaded event loop

## Platforms

Currently, MIO only supports Linux and Darwin. However, Windows support
will be coming soon. The goal is to support all platforms that Rust
supports.
