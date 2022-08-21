//! Windows specific networking functionality. Mirrors std::os::unix::net.

mod addr;
mod listener;
mod socket;
mod stream;

pub use self::addr::*;
pub use self::listener::*;
pub use self::stream::*;

use std::sync::Once;

/// Initialise the network stack for Windows.
pub(crate) fn init() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        // Let standard library call `WSAStartup` for us, we can't do it
        // ourselves because otherwise using any type in `std::net` would panic
        // when it tries to call `WSAStartup` a second time.
        drop(std::net::UdpSocket::bind("127.0.0.1:0"));
    });
}
