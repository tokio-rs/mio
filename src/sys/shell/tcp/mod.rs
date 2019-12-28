use std::io;
use std::net::{self, SocketAddr};

mod listener;
pub(crate) use self::listener::TcpListener;

pub fn connect(_: SocketAddr) -> io::Result<net::TcpStream> {
    os_required!();
}
