// A client for the echo server. Connects to the server with multiple sockets.
extern crate mio;

use mio::tcp::TcpStream;
use mio::util::Slab;
use std::io::Cursor;
use std::net::SocketAddr;

// For simplicity, we are hardcoding the data to send to the Pong server. Each
// string will be sent on a separate connection.
const MESSAGES: &'static [&'static str] = &[
    "Hello world, this is a message",
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
    }

    fn write(&mut self, event_loop: &mut mio::EventLoop<Ping>) {
    }

    fn is_closed(&self) -> bool {
        match self.state {
            State::Closed => true,
            _ => false,
        }
    }
}

enum State {
    Reading,
    Writing(Cursor<Vec<u8>>),
    Closed,
}

fn run(address: SocketAddr) {
    // Create a new event loop, panic if this fails.
    let mut event_loop = mio::EventLoop::new().unwrap();

    // Create Ping struct
    let mut ping = Ping::new();

    // Create a separate Connection struct for each entry in `Messages`.
    for message in MESSAGES {
        // Split the string into individual lines
        let mut lines: Vec<Vec<u8>> = message.split('\n')
            .map(|l| {
                let mut bytes = l.as_bytes().to_vec();
                bytes.push('\n');
                bytes
            })
            .collect();

        if lines.is_empty() {
            // There isn't anything to do, so skip this connection
            continue;
        }

        let current_line = lines.remove(0);

        // Open a socket
        let socket = match TcpStream::connect(&address) {
            Ok(socket) => socket,
            Err(e) => {
                println!("failed to create socket; err={:?}", e);
                continue;
            }
        };

        // Create the `Connection` instance
        let token = ping.connections.insert_with(|token| {
            // Register the socket with the event loop
            event_loop.register_opt(
                &socket,
                token,
                mio::EventSet::writable(),
                mio::PollOpt::edge() | mio::PollOpt::oneshot()).unwrap();

            // Create the connection
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
