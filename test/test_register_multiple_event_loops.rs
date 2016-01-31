use localhost;
use mio::*;
use mio::tcp::*;
use mio::udp::*;
use std::io::ErrorKind;

struct MyHandler;

impl Handler for MyHandler {
    type Timeout = ();
    type Message = ();
}

#[test]
fn test_tcp_register_multiple_event_loops() {
    let addr = localhost();
    let listener = TcpListener::bind(&addr).unwrap();

    let mut event_loop_1 = EventLoop::<MyHandler>::new().unwrap();
    event_loop_1.register(&listener, Token(0), EventSet::all(), PollOpt::edge()).unwrap();

    let mut event_loop_2 = EventLoop::<MyHandler>::new().unwrap();

    // Try registering the same socket with the initial one
    let res = event_loop_2.register(&listener, Token(0), EventSet::all(), PollOpt::edge());
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().kind(), ErrorKind::Other);

    // Try cloning the socket and registering it again
    let listener2 = listener.try_clone().unwrap();
    let res = event_loop_2.register(&listener2, Token(0), EventSet::all(), PollOpt::edge());
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().kind(), ErrorKind::Other);

    // Try the stream
    let stream = TcpStream::connect(&addr).unwrap();

    event_loop_1.register(&stream, Token(1), EventSet::all(), PollOpt::edge()).unwrap();

    let res = event_loop_2.register(&stream, Token(1), EventSet::all(), PollOpt::edge());
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().kind(), ErrorKind::Other);

    // Try cloning the socket and registering it again
    let stream2 = stream.try_clone().unwrap();
    let res = event_loop_2.register(&stream2, Token(1), EventSet::all(), PollOpt::edge());
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().kind(), ErrorKind::Other);
}

#[test]
fn test_udp_register_multiple_event_loops() {
    let addr = localhost();
    let socket = UdpSocket::bound(&addr).unwrap();

    let mut event_loop_1 = EventLoop::<MyHandler>::new().unwrap();
    event_loop_1.register(&socket, Token(0), EventSet::all(), PollOpt::edge()).unwrap();

    let mut event_loop_2 = EventLoop::<MyHandler>::new().unwrap();

    // Try registering the same socket with the initial one
    let res = event_loop_2.register(&socket, Token(0), EventSet::all(), PollOpt::edge());
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().kind(), ErrorKind::Other);

    // Try cloning the socket and registering it again
    let socket2 = socket.try_clone().unwrap();
    let res = event_loop_2.register(&socket2, Token(0), EventSet::all(), PollOpt::edge());
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().kind(), ErrorKind::Other);
}
