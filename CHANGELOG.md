# 0.5.0 (unreleased)

* Don't re-export bytes types
* Preliminary Windows support
* Renamed `EventLoop::register_opt` to `EventLoop::register`
* `EventLoopConfig` is now a builder instead of having public struct fields. It
  is also no longer `Copy`.

# 0.4.1 (July 21)

* [BUGFIX] Fix notify channel concurrency bug (#216)

# 0.4.0 (July 16)

* [BUGFIX] EventLoop::register requests all events, not just readable.
* [BUGFIX] Attempting to send a message to a shutdown event loop fails correctly.
* [FEATURE] Expose TCP shutdown
* [IMPROVEMENT] Coalesce readable & writable into `ready` event (#184)
* [IMPROVEMENT] Rename TryRead & TryWrite function names to avoid conflict with std.
* [IMPROVEMENT] Provide TCP and UDP types in mio (path to windows #155)
* [IMPROVEMENT] Use clock_ticks crate instead of time (path to windows #155)
* [IMPROVEMENT] Move unix specific features into mio::unix module
* [IMPROVEMENT] TcpListener sets SO_REUSEADDR by default
