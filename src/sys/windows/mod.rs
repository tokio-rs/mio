mod afd;
pub mod event;
mod selector;
mod tcp;
mod udp;
mod waker;

pub use event::Event;
pub use selector::{Selector, SelectorInner};
pub use tcp::{TcpListener, TcpStream};
pub use udp::UdpSocket;
pub use waker::Waker;

pub type Events = Vec<Event>;
//pub use self::tcp::{TcpListener, TcpStream};
//pub use self::udp::UdpSocket;
