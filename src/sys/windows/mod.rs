use std::sync::{Arc, Mutex};

/// Helper macro to execute a system call that returns an `io::Result`.
//
// Macro must be defined before any modules that uses them.
macro_rules! syscall {
    ($fn: ident ( $($arg: expr),* $(,)* ), $err_test: path, $err_value: expr) => {{
        let res = unsafe { $fn($($arg, )*) };
        if $err_test(&res, &$err_value) {
            Err(io::Error::last_os_error())
        } else {
            Ok(res)
        }
    }};
}

mod afd;
pub mod event;
mod io_status_block;
mod selector;
mod tcp;
mod udp;
mod waker;

pub use event::{Event, Events};
pub use selector::{Selector, SelectorInner, SockState};
pub use tcp::{TcpListener, TcpStream};
pub use udp::UdpSocket;
pub use waker::Waker;

pub trait SocketState {
    fn get_sock_state(&self) -> Option<Arc<Mutex<SockState>>>;
    fn set_sock_state(&self, sock_state: Option<Arc<Mutex<SockState>>>);
}
