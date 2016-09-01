# 0.6.0 (unreleased)

* Shift primary API towards `Poll`
* `EventLoop` and types to `deprecated` mod
* Increase minimum supported Rust version to 1.3.0
* Deprecate unix domain socket implementation in favor of using version
  in `std`
* Remove various types now included in `std`
* Updated TCP & UDP APIs to match the versions in `std`
* Enable implementing `Evented` for any type via `Registration`
* Rename `IoEvent` -> `Event`
* Access `Event` data via functions vs. public fields.
* Expose `Events` as a public type that is passed into `Poll`
* Use `std::time::Duration` for all APIs that require a time duration.
* Polled events are now retrieved via `Events` type.
* Implement `std::error::Error` for `TimerError`
* Relax `Send` bound on notify messages.
* Remove `Clone` impl for `Timeout` (future proof)
* Remove `mio::prelude`
* Remove `mio::util`
* Remove dependency on bytes

# 0.5.0 (December 3, 2015)

* Windows support (#239)
* NetBSD support (#306)
* Android support (#295)
* Don't re-export bytes types
* Renamed `EventLoop::register_opt` to `EventLoop::register` (#257)
* `EventLoopConfig` is now a builder instead of having public struct fields. It
  is also no longer `Copy`. (#259)
* `TcpSocket` is no longer exported in the public API (#262)
* Integrate with net2. (#262)
* `TcpListener` now returns the remote peer address from `accept` as well (#275)
* The `UdpSocket::{send_to, recv_from}` methods are no longer generic over `Buf`
  or `MutBuf` but instead take slices directly. The return types have also been
  updated to return the number of bytes transferred. (#260)
* Fix bug with kqueue wher ean error on registration prevented the
  changelist from getting flushed (#276)
* Support sending/receiving FDs over UNIX sockets (#291)
* Mio's socket types are permanently associated with an EventLoop (#308)
* Reduce unnecessary poll wakeups (#314)


# 0.4.1 (July 21, 2015)

* [BUGFIX] Fix notify channel concurrency bug (#216)

# 0.4.0 (July 16, 2015)

* [BUGFIX] EventLoop::register requests all events, not just readable.
* [BUGFIX] Attempting to send a message to a shutdown event loop fails correctly.
* [FEATURE] Expose TCP shutdown
* [IMPROVEMENT] Coalesce readable & writable into `ready` event (#184)
* [IMPROVEMENT] Rename TryRead & TryWrite function names to avoid conflict with std.
* [IMPROVEMENT] Provide TCP and UDP types in mio (path to windows #155)
* [IMPROVEMENT] Use clock_ticks crate instead of time (path to windows #155)
* [IMPROVEMENT] Move unix specific features into mio::unix module
* [IMPROVEMENT] TcpListener sets SO_REUSEADDR by default
