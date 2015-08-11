extern crate mio;
extern crate bytes;

use mio::{TryRead, TryWrite};
use mio::tcp::*;
use mio::util::Slab;
use bytes::{Buf, Take};
use std::mem;
use std::net::SocketAddr;
use std::io::Cursor;

// The token used to register the TCP listner socket. There will only be a
// single listener so to make things simpler, we just track the token as a
// constant.
const SERVER: mio::Token = mio::Token(0);

// The Pong server. Consists of a listener socket as well as well as a `Slab`
// containing all the state for the client connections.
struct Pong {
    server: TcpListener,
    connections: Slab<Connection>,
}

impl Pong {
    // Initialize a new `Pong` server from the given TCP listener socket
    fn new(server: TcpListener) -> Pong {
        // Token `0` is reserved for the server socket. Tokens 1+ are used for
        // client connections. The slab is initialized to return Tokens
        // starting at 1.
        let slab = Slab::new_starting_at(mio::Token(1), 1024);

        Pong {
            server: server,
            connections: slab,
        }
    }
}

impl mio::Handler for Pong {
    type Timeout = (); // Timeouts are not used in this example
    type Message = (); // Cross thread notifications are not used in this example

    // Called by the `EventLoop` when a socket is ready to be operated on. All
    // socket events are filtered through this function.
    fn ready(&mut self, event_loop: &mut mio::EventLoop<Pong>, token: mio::Token, events: mio::EventSet) {
        println!("socket is ready; token={:?}; events={:?}", token, events);

        match token {
            SERVER => {
                // The server socket is ready to be operated on. In this case,
                // it means that there is a pending connection ready to be
                // accepted. This is represented by a `readable` event.
                assert!(events.is_readable());

                match self.server.accept() {
                    Ok(Some(socket)) => {
                        // A new client connection has been established. A
                        // `Connection` instance will be created to represent
                        // this connection. The socket will then be registered
                        // with the `EventLoop` so that we get notified when
                        // the socket has pending data.
                        println!("accepted a new client socket");

                        // Currently, if the `Slab` is full (all slots are in
                        // use), the `unwrap()` will cause a panic.
                        //
                        // TODO: Gracefully handle the case where the
                        // connection `Slab` is full. Ideally, this would be
                        // done by de-registering interest in the server socket
                        // since we cannot handle any more connections.
                        let token = self.connections
                            .insert_with(|token| Connection::new(socket, token))
                            .unwrap();

                        // Register the connection with the event loop. Only
                        // request readable events. The strategy is to read a
                        // full line of input before attempting to write to the
                        // socket, so we don't want to receive `writable`
                        // events yet since there is nothing to do.
                        event_loop.register_opt(
                            &self.connections[token].socket,
                            token,
                            mio::EventSet::readable(),
                            mio::PollOpt::edge() | mio::PollOpt::oneshot()).unwrap();
                    }
                    Ok(None) => {
                        // It's important to always handle this case. Even if
                        // mio indicates that there is a pending connection,
                        // the actual `accept()` can return with `Ok(None)`.
                        // This could be because another thread accepted the
                        // connection, or simply because mio can fire off
                        // spurious events.
                        println!("the server socket wasn't actually ready");
                    }
                    Err(e) => {
                        // Something unexpected happened. For now, just
                        // shutdown the server.
                        //
                        // TODO: Gracefully handle errors.
                        println!("encountered error while accepting connection; err={:?}", e);
                        event_loop.shutdown();
                    }
                }
            }
            _ => {
                self.connections[token].ready(event_loop, events);

                // If handling the event resulted in a closed socket, then
                // remove the socket from the Slab. This will result in all
                // resources being freed.
                if self.connections[token].is_closed() {
                    let _ = self.connections.remove(token);
                }
            }
        }
    }
}

// The structure tracking state associated with a client connection.
#[derive(Debug)]
struct Connection {
    // The TCP socket
    socket: TcpStream,
    // The token that was used to register the socket with the `EventLoop`
    token: mio::Token,
    // The state of the connection + the byte buffers used to store data that
    // has been read from the client.
    state: State,
}

impl Connection {
    fn new(socket: TcpStream, token: mio::Token) -> Connection {
        Connection {
            socket: socket,
            token: token,
            state: State::Reading(vec![]),
        }
    }

    fn ready(&mut self, event_loop: &mut mio::EventLoop<Pong>, events: mio::EventSet) {
        println!("    connection-state={:?}", self.state);

        match self.state {
            State::Reading(..) => {
                assert!(events.is_readable(), "unexpected events; events={:?}", events);
                self.read(event_loop)
            }
            State::Writing(..) => {
                assert!(events.is_writable(), "unexpected events; events={:?}", events);
                self.write(event_loop)
            }
            _ => unimplemented!(),
        }
    }

    fn read(&mut self, event_loop: &mut mio::EventLoop<Pong>) {
        match self.socket.try_read_buf(self.state.mut_read_buf()) {
            Ok(Some(0)) => {
                // If there is any data buffered up, attempt to write it back
                // to the client. Either the socket is currently closed, in
                // which case writing will result in an error, or the client
                // only shutdown half of the socket and is still expecting to
                // receive the buffered data back. See
                // test_handling_client_shutdown() for an illustration
                println!("    read 0 bytes from client; buffered={}", self.state.read_buf().len());

                match self.state.read_buf().len() {
                    n if n > 0 => {
                        // Transition to a writing state even if a new line has
                        // not yet been received.
                        self.state.transition_to_writing(n);

                        // Re-register the socket with the event loop. This
                        // will notify us when the socket becomes writable.
                        self.reregister(event_loop);
                    }
                    _ => self.state = State::Closed,
                }
            }
            Ok(Some(n)) => {
                println!("read {} bytes", n);

                // Look for a new line. If a new line is received, then the
                // state is transitioned from `Reading` to `Writing`.
                self.state.try_transition_to_writing();

                // Re-register the socket with the event loop. The current
                // state is used to determine whether we are currently reading
                // or writing.
                self.reregister(event_loop);
            }
            Ok(None) => {
                self.reregister(event_loop);
            }
            Err(e) => {
                panic!("got an error trying to read; err={:?}", e);
            }
        }
    }

    fn write(&mut self, event_loop: &mut mio::EventLoop<Pong>) {
        // TODO: handle error
        match self.socket.try_write_buf(self.state.mut_write_buf()) {
            Ok(Some(_)) => {
                // If the entire line has been written, transition back to the
                // reading state
                self.state.try_transition_to_reading();

                // Re-register the socket with the event loop.
                self.reregister(event_loop);
            }
            Ok(None) => {
                // The socket wasn't actually ready, re-register the socket
                // with the event loop
                self.reregister(event_loop);
            }
            Err(e) => {
                panic!("got an error trying to write; err={:?}", e);
            }
        }
    }

    fn reregister(&self, event_loop: &mut mio::EventLoop<Pong>) {
        // Maps the current client state to the mio `EventSet` that will provide us
        // with the notifications that we want. When we are currently reading from
        // the client, we want `readable` socket notifications. When we are writing
        // to the client, we want `writable` notifications.
        let event_set = match self {
            State::Reading(..) => mio::EventSet::readable(),
            State::Writing(..) => mio::EventSet::writable(),
            _ => mio::EventSet::none(),
        };

        event_loop.reregister(&self.socket, self.token, event_set, mio::PollOpt::oneshot())
            .unwrap();
    }

    fn is_closed(&self) -> bool {
        match self.state {
            State::Closed => true,
            _ => false,
        }
    }
}

// The current state of the client connection
#[derive(Debug)]
enum State {
    // We are currently reading data from the client into the `Vec<u8>`. This
    // is done until we see a new line.
    Reading(Vec<u8>),
    // We are currently writing the contents of the `Vec<u8>` up to and
    // including the new line.
    Writing(Take<Cursor<Vec<u8>>>),
    // The socket is closed.
    Closed,
}

impl State {
    fn mut_read_buf(&mut self) -> &mut Vec<u8> {
        match *self {
            State::Reading(ref mut buf) => buf,
            _ => panic!("connection not in reading state"),
        }
    }

    fn read_buf(&self) -> &[u8] {
        match *self {
            State::Reading(ref buf) => buf,
            _ => panic!("connection not in reading state"),
        }
    }

    fn write_buf(&self) -> &Take<Cursor<Vec<u8>>> {
        match *self {
            State::Writing(ref buf) => buf,
            _ => panic!("connection not in writing state"),
        }
    }

    fn mut_write_buf(&mut self) -> &mut Take<Cursor<Vec<u8>>> {
        match *self {
            State::Writing(ref mut buf) => buf,
            _ => panic!("connection not in writing state"),
        }
    }

    // Looks for a new line, if there is one the state is transitioned to
    // writing
    fn try_transition_to_writing(&mut self) {
        if let Some(pos) = self.read_buf().iter().position(|b| *b == b'\n') {
            self.transition_to_writing(pos + 1);
        }
    }

    fn transition_to_writing(&mut self, pos: usize) {
        // First, remove the current read buffer, replacing it with an
        // empty Vec<u8>.
        let buf = mem::replace(self, State::Closed)
            .unwrap_read_buf();

        // Wrap in `Cursor`, this allows Vec<u8> to act as a readable
        // buffer
        let buf = Cursor::new(buf);

        // Transition the state to `Writing`, limiting the buffer to the
        // new line (inclusive).
        *self = State::Writing(Take::new(buf, pos));
    }

    // If the buffer being written back to the client has been consumed, switch
    // back to the reading state. However, there already might be another line
    // in the read buffer, so `try_transition_to_writing` is called as a final
    // step.
    fn try_transition_to_reading(&mut self) {
        if !self.write_buf().has_remaining() {
            let cursor = mem::replace(self, State::Closed)
                .unwrap_write_buf()
                .into_inner();

            let pos = cursor.position();
            let mut buf = cursor.into_inner();

            // Drop all data that has been written to the client
            drain_to(&mut buf, pos as usize);

            *self = State::Reading(buf);

            // Check for any new lines that have already been read.
            self.try_transition_to_writing();
        }
    }

    fn unwrap_read_buf(self) -> Vec<u8> {
        match self {
            State::Reading(buf) => buf,
            _ => panic!("connection not in reading state"),
        }
    }

    fn unwrap_write_buf(self) -> Take<Cursor<Vec<u8>>> {
        match self {
            State::Writing(buf) => buf,
            _ => panic!("connection not in writing state"),
        }
    }
}

pub fn start(address: SocketAddr) {
    // Create a new non-blocking socket bound to the given address. All sockets
    // created by mio are set to non-blocking mode.
    let server = TcpListener::bind(&address).unwrap();

    // Create a new `EventLoop`. 
    let mut event_loop = mio::EventLoop::new().unwrap();

    // Register the server socket with the event loop.
    event_loop.register(&server, SERVER).unwrap();

    // Create a new `Pong` instance that will track the state of the server.
    let mut pong = Pong::new(server);

    // Run the `Pong` server
    println!("running pingpong server; port=6567");
    event_loop.run(&mut pong).unwrap();
}

pub fn main() {
    start("0.0.0.0:6567".parse().unwrap());
}

fn drain_to(vec: &mut Vec<u8>, count: usize) {
    // A very inefficient implementation. A better implementation could be
    // built using `Vec::drain()`, but the API is currently unstable.
    for _ in 0..count {
        vec.remove(0);
    }
}

/*
 *
 * ===== TESTS =====
 *
 */

#[cfg(test)]
mod test {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{Shutdown, TcpStream};

    #[test]
    pub fn test_basic_echoing() {
        start_server();

        let mut sock = BufReader::new(TcpStream::connect(HOST).unwrap());
        let mut recv = String::new();

        sock.get_mut().write_all(b"hello world\n").unwrap();
        sock.read_line(&mut recv).unwrap();

        assert_eq!(recv, "hello world\n");

        recv.clear();

        sock.get_mut().write_all(b"this is a line\n").unwrap();
        sock.read_line(&mut recv).unwrap();

        assert_eq!(recv, "this is a line\n");
    }

    #[test]
    pub fn test_handling_client_shutdown() {
        start_server();

        let mut sock = TcpStream::connect(HOST).unwrap();

        sock.write_all(b"hello world").unwrap();
        sock.shutdown(Shutdown::Write).unwrap();

        let mut recv = vec![];
        sock.read_to_end(&mut recv).unwrap();

        assert_eq!(recv, b"hello world");
    }

    const HOST: &'static str = "0.0.0.0:13254";

    fn start_server() {
        use std::thread;
        use std::sync::{Once, ONCE_INIT};

        static INIT: Once = ONCE_INIT;

        INIT.call_once(|| {
            thread::spawn(|| {
                super::start(HOST.parse().unwrap())
            });

            while let Err(_) = TcpStream::connect(HOST) {
                // Loop
            }
        });
    }
}
