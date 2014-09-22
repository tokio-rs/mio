# MIO - Metal IO

MIO is a lightweight IO library for Rust with a focus on adding as
little overhead as possible over the OS abstractions.

## Features

* Event loop backed by epoll, kqueue.
* Zero allocations at runtime
* Non-blocking TCP sockets
* High performance timer system
* Thread safe message channel for cross thread communication

__Coming soon__

* UDP and Unix domain sockets
* Signal handling

## Non goals

The following are specifically omitted from MIO and are left to the user
or higher level libraries.

* File operations
* Thread pools / multi-threaded reactor

## Platforms

Currently, MIO only supports Linux and Darwin. However, Windows support
will be coming soon. The goal is to support all platforms that Rust
supports.
