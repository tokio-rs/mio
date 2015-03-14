use mio::Socket;
use super::localhost;
use std::io::Read;
use std::net::{TcpListener, TcpStream};
use std::thread;

#[test]
pub fn test_sock_to() {
    let addr = localhost();
    let srv = TcpListener::bind(&addr).unwrap();

    let t = thread::scoped(move || {
        let mut buf = [0; 1024];

        let (mut s, _) = srv.accept().unwrap();

        s.set_read_timeout_ms(50).unwrap();

        assert!(s.read(&mut buf).is_err());
    });

    let mut cli = TcpStream::connect(&addr).unwrap();

    cli.set_read_timeout_ms(50).unwrap();

    let mut buf = [0; 1024];
    let res = cli.read(&mut buf);
    assert!(res.is_err());

    drop(t);
}
