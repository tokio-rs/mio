#![cfg(all(feature = "os-poll", feature = "tcp"))]

use mio::net::TcpSocket;

mod util;
use util::{assert_socket_close_on_exec, assert_socket_non_blocking};

#[test]
fn it_works() {
    let socket = TcpSocket::new(libc::AF_INET).unwrap();
    assert_socket_close_on_exec(&socket);
    assert_socket_non_blocking(&socket);
}
