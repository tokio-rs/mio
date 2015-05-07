#[cfg(unix)]
pub use self::unix::{Awakener, Events, Selector};

#[cfg(unix)]
mod unix;
