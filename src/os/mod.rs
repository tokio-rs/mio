#[cfg(target_os = "linux")]
pub use self::epoll::{Events, Selector};

#[cfg(any(target_os = "macos", target_os = "ios"))]
pub use self::kqueue::{Events, Selector};

#[cfg(not(target_os = "linux"))]
pub use self::posix::Awakener;

#[cfg(target_os = "linux")]
pub use self::linux::Awakener;

#[cfg(target_os = "linux")]
mod epoll;

#[cfg(any(target_os = "macos", target_os = "ios"))]
mod kqueue;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(not(target_os = "linux"))]
mod posix;

pub mod event;

pub mod token;
