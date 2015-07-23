# MIO - Metal IO

MIO is a lightweight IO library for Rust with a focus on adding as
little overhead as possible over the OS abstractions.

[![Build Status](https://travis-ci.org/carllerche/mio.svg?branch=master)](https://travis-ci.org/carllerche/mio)
[![crates.io](http://meritbadge.herokuapp.com/mio)](https://crates.io/crates/mio)

- API documentation: [master](http://rustdoc.s3-website-us-east-1.amazonaws.com/mio/master/mio/), [v0.3](http://rustdoc.s3-website-us-east-1.amazonaws.com/mio/v0.3.x/mio/)

## Usage

To use `mio`, first add this to your `Cargo.toml`:

```toml
[dependencies]
mio = "0.3.0"
```

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

__Eventually__

* Signal handling
* Windows support

## Non goals

The following are specifically omitted from MIO and are left to the user
or higher level libraries.

* File operations
* Thread pools / multi-threaded event loop

## Platforms

Currently, MIO only supports Linux and Darwin. The goal is to support
all platforms that support Rust and the readiness IO model.

## Reading

Please submit PRs containing links to MIO resources.

* [My Basic Understanding of mio and Asynchronous IO](http://hermanradtke.com/2015/07/12/my-basic-understanding-of-mio-and-async-io.html)
* [Creating A Multi-echo Server using Rust and mio](http://hermanradtke.com/2015/07/22/creating-a-multi-echo-server-using-rust-and-mio.html)

## Community

A group of mio users hang out in the #mio channel on the Mozilla IRC
server (irc.mozilla.org). This can be a good place to go for questions.
