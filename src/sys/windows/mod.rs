use std::sync::{Arc, Mutex};

mod afd;
pub mod event;
mod selector;
mod tcp;
mod udp;
mod waker;

pub use event::{Event, Events};
pub use selector::{Selector, SelectorInner, SockState};
pub use tcp::{TcpListener, TcpStream};
pub use udp::UdpSocket;
pub use waker::Waker;

pub trait GenericSocket {
    fn get_sock_state(&self) -> Option<Arc<Mutex<SockState>>>;
    fn set_sock_state(&self, sock_state: Option<Arc<Mutex<SockState>>>);
}
