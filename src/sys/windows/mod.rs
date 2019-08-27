use std::io;
use std::mem::size_of_val;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::sync::{Arc, Mutex, Once};
use winapi::ctypes::c_int;
use winapi::shared::ws2def::SOCKADDR;
use winapi::um::winsock2::{
    ioctlsocket, socket, FIONBIO, INVALID_SOCKET, PF_INET, PF_INET6, SOCKET,
};

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

use crate::{Interests, Token};

struct InternalState {
    selector: Arc<SelectorInner>,
    token: Token,
    interests: Interests,
    sock_state: Option<Arc<Mutex<SockState>>>,
}

impl InternalState {
    fn new(selector: Arc<SelectorInner>, token: Token, interests: Interests) -> InternalState {
        InternalState {
            selector,
            token,
            interests,
            sock_state: None,
        }
    }
}

impl Drop for InternalState {
    fn drop(&mut self) {
        if let Some(sock_state) = self.sock_state.as_ref() {
            let mut sock_state = sock_state.lock().unwrap();
            sock_state.mark_delete();
        }
    }
}

/// Initialise the network stack for Windows.
fn init() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        // Let standard library call `WSAStartup` for us, we can't do it
        // ourselves because otherwise using any type in `std::net` would panic
        // when it tries to call `WSAStartup` a second time.
        drop(std::net::UdpSocket::bind("127.0.0.1:0"));
    });
}

/// Create a new non-blocking socket.
fn new_socket(addr: SocketAddr, socket_type: c_int) -> io::Result<SOCKET> {
    let domain = match addr {
        SocketAddr::V4(..) => PF_INET,
        SocketAddr::V6(..) => PF_INET6,
    };

    syscall!(
        socket(domain, socket_type, 0),
        PartialEq::eq,
        INVALID_SOCKET
    )
    .and_then(|socket| {
        syscall!(ioctlsocket(socket, FIONBIO, &mut 1), PartialEq::ne, 0).map(|_| socket as SOCKET)
    })
}

fn socket_addr(addr: &SocketAddr) -> (*const SOCKADDR, c_int) {
    match addr {
        SocketAddr::V4(ref addr) => (
            addr as *const _ as *const SOCKADDR,
            size_of_val(addr) as c_int,
        ),
        SocketAddr::V6(ref addr) => (
            addr as *const _ as *const SOCKADDR,
            size_of_val(addr) as c_int,
        ),
    }
}

fn inaddr_any(other: SocketAddr) -> SocketAddr {
    match other {
        SocketAddr::V4(..) => {
            let any = Ipv4Addr::new(0, 0, 0, 0);
            let addr = SocketAddrV4::new(any, 0);
            SocketAddr::V4(addr)
        }
        SocketAddr::V6(..) => {
            let any = Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0);
            let addr = SocketAddrV6::new(any, 0, 0, 0);
            SocketAddr::V6(addr)
        }
    }
}
