#[cfg(target_os = "linux")]
pub use self::epoll::{Events, Selector};

#[cfg(target_os = "macos")]
#[cfg(target_os = "ios")]
pub use self::kqueue::{Events, Selector};

#[cfg(unix)]
pub use self::posix::*;

#[cfg(windows)]
pub use self::windows::*;

#[cfg(target_os = "linux")]
mod epoll;

#[cfg(target_os = "macos")]
#[cfg(target_os = "ios")]
mod kqueue;

#[cfg(unix)]
mod posix;

#[cfg(windows)]
mod windows;
