use std::sync::{Arc, Mutex, Once};

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
