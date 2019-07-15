mod afd;
pub mod event;
mod selector;
mod sourcerawsocket;
mod tcp;
mod udp;
mod waker;

pub use event::Event;
pub use selector::{Selector, SelectorInner};
pub use sourcerawsocket::SourceRawSocket;
pub use tcp::{TcpListener, TcpStream};
pub use udp::UdpSocket;
pub use waker::Waker;

pub type Events = Vec<Event>;
