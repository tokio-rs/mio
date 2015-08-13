// A client for the echo server. Connects to the server with multiple sockets.

extern crate mio;
extern crate bytes;

use mio::{TryRead, TryWrite};
use mio::tcp::TcpStream;
use mio::util::Slab;
use bytes::Buf;
use std::{mem, str};
use std::io::Cursor;
use std::net::SocketAddr;

// For simplicity, we are hardcoding the data to send to the Pong server. Each
// string will be sent on a separate connection.
const MESSAGES: &'static [&'static str] = &[
    // These two lines are sent on a socket.
    "Hello world, this is a message.\n
     Another line sent for the server.",

    // This single line is sent on a socket.
    "This line is sent on another connection.",
];

// Pong server client. Manages multiple connections where each connection sends
// and receives its own data.
struct Ping {
    connections: Slab<Connection>,
}

impl Ping {
    fn new() -> Ping {
        Ping {
            // Allocate a slab that is able to hold exactly the right number of
            // connections.
            connections: Slab::new(MESSAGES.len()),
        }
    }
}

impl mio::Handler for Ping {
    type Timeout = ();
    type Message = ();

    fn ready(&mut self, event_loop: &mut mio::EventLoop<Ping>, token: mio::Token, events: mio::EventSet) {
        println!("socket is ready; token={:?}; events={:?}", token, events);
        self.connections[token].ready(event_loop, events);

        // If handling the event resulted in a closed socket, then
        // remove the socket from the Slab. This will result in all
        // resources being freed.
        if self.connections[token].is_closed() {
            let _ = self.connections.remove(token);

            if self.connections.is_empty() {
                event_loop.shutdown();
            }
        }
    }
}

struct Connection {
    // The connection's TCP socket 
    socket: TcpStream,
    // The token used to register this connection with the EventLoop
    token: mio::Token,
    // The current state of the connection (reading or writing)
    state: State,
    // Remaining lines to send to the server
    remaining: Vec<Vec<u8>>,
}

impl Connection {
    fn ready(&mut self, event_loop: &mut mio::EventLoop<Ping>, events: mio::EventSet) {
        println!("    connection-state={:?}", self.state);

        // Check the current state of the connection and handle the event
        // appropriately.
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

    fn read(&mut self, event_loop: &mut mio::EventLoop<Ping>) {
        match self.socket.try_read_buf(self.state.mut_read_buf()) {
            Ok(Some(0)) => {
                // The socket (or at least the read half) is closed. There is
                // nothing more that can be done, so just close the socket.
                self.state = State::Closed;
            }
            Ok(Some(n)) => {
                println!("read {} bytes", n);

                // Check for a newline, if there is a newline, then print the
                // received data. Otherwise, stay in the reading state.
                self.state.try_transition_to_writing(&mut self.remaining);

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

    fn write(&mut self, event_loop: &mut mio::EventLoop<Ping>) {
        match self.socket.try_write_buf(self.state.mut_write_buf()) {
            Ok(Some(_)) => {
                // If the entire buffer has been written, transition to the
                // reading state.
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
                panic!("got an error trying to read; err={:?}", e);
            }
        }
    }

    fn reregister(&self, event_loop: &mut mio::EventLoop<Ping>) {
        // Maps the current client state to the mio `EventSet` that will provide us
        // with the notifications that we want. When we are currently reading from
        // the client, we want `readable` socket notifications. When we are writing
        // to the client, we want `writable` notifications.
        let event_set = match self.state {
            State::Reading(..) => mio::EventSet::readable(),
            State::Writing(..) => mio::EventSet::writable(),
            _ => return,
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

#[derive(Debug)]
enum State {
    Reading(Vec<u8>),
    Writing(Cursor<Vec<u8>>),
    Closed,
}

impl State {
    fn try_transition_to_reading(&mut self) {
        if !self.write_buf().has_remaining() {
            self.transition_to_reading();
        }
    }

    fn transition_to_reading(&mut self) {
        let mut buf = mem::replace(self, State::Closed)
            .unwrap_write_buf()
            .into_inner();

        buf.clear();

        *self = State::Reading(buf);
    }

    fn try_transition_to_writing(&mut self, remaining: &mut Vec<Vec<u8>>) {
        match self.read_buf().last() {
            Some(&c) if c == b'\n' => {
                // Wrap in a scope to work around borrow checker
                {
                    // Get a string back
                    let s = str::from_utf8(self.read_buf()).unwrap();
                    println!("Got from server: {}", s);
                }

                self.transition_to_writing(remaining);
            }
            _ => {}
        }
    }

    fn transition_to_writing(&mut self, remaining: &mut Vec<Vec<u8>>) {
        if remaining.is_empty() {
            *self = State::Closed;
            return;
        }

        let line = remaining.remove(0);
        *self = State::Writing(Cursor::new(line));
    }

    fn read_buf(&self) -> &[u8] {
        match *self {
            State::Reading(ref buf) => buf,
            _ => panic!("connection not in reading state"),
        }
    }

    fn mut_read_buf(&mut self) -> &mut Vec<u8> {
        match *self {
            State::Reading(ref mut buf) => buf,
            _ => panic!("connection not in reading state"),
        }
    }

    fn write_buf(&self) -> &Cursor<Vec<u8>> {
        match *self {
            State::Writing(ref buf) => buf,
            _ => panic!("connection not in writing state"),
        }
    }

    fn mut_write_buf(&mut self) -> &mut Cursor<Vec<u8>> {
        match *self {
            State::Writing(ref mut buf) => buf,
            _ => panic!("connection not in writing state"),
        }
    }

    fn unwrap_write_buf(self) -> Cursor<Vec<u8>> {
        match self {
            State::Writing(buf) => buf,
            _ => panic!("connection not in writing state"),
        }
    }
}

fn run(address: SocketAddr) {
    // Create a new event loop, panic if this fails.
    let mut event_loop = mio::EventLoop::new().unwrap();

    let mut ping = Ping::new();

    // Create a separate Connection struct for each entry in `Messages`.
    for message in MESSAGES {
        // Split the string into individual lines
        let mut lines: Vec<Vec<u8>> = message.split('\n')
            .map(|l| {
                let mut bytes = l.as_bytes().to_vec();
                bytes.push(b'\n');
                bytes
            })
            .collect();

        if lines.is_empty() {
            // There isn't anything to do, so skip this connection
            continue;
        }

        let current_line = lines.remove(0);

        // Open a new socket and connect to the remote address. The connect
        // will (most likely) not complete immediately. Since sockets are
        // non-blocking, connects are handled by initiating the operation then
        // waiting for the socket to become readable or writiable.
        //
        // In our case, once the connection is established, we want to write to
        // the socket, so we listen for writable notifications. If the connect
        // fails, we will receive a writable notification, but once we attempt
        // to write, the write will fail. EventSet::error() will also be set on
        // the `events` argument to our `ready()` function.
        let socket = match TcpStream::connect(&address) {
            Ok(socket) => socket,
            Err(e) => {
                // If the connect fails here, then usually there is something
                // wrong locally. Though, on some operating systems, attempting
                // to connect to a localhost address completes immediately.
                println!("failed to create socket; err={:?}", e);
                continue;
            }
        };

        // Create a `Connection` instance representing the socket and store it
        // on our `Ping` client struct.
        ping.connections.insert_with(|token| {
            // Register the socket with the event loop
            event_loop.register_opt(
                &socket,
                token,
                mio::EventSet::writable(),
                mio::PollOpt::edge() | mio::PollOpt::oneshot()).unwrap();

            Connection {
                socket: socket,
                token: token,
                state: State::Writing(Cursor::new(current_line)),
                remaining: lines,
            }
        });
    }

    // Start the event loop
    event_loop.run(&mut ping).unwrap();
}

pub fn main() {
    run("127.0.0.1:6567".parse().unwrap());
}
