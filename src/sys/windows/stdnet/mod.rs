//! Implementation of blocking UDS types for windows, mirrors std::os::unix::net.
mod addr;
mod listener;
mod socket;
mod stream;

pub(crate) use self::addr::SocketAddr;
pub(crate) use self::listener::UnixListener;
pub(crate) use self::stream::UnixStream;

cfg_os_poll! {
    pub(self) use self::addr::socket_addr;

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
}
