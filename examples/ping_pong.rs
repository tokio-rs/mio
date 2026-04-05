// You can run this example from the root of the mio repo:
// cargo run --example ping_pong --features="os-poll net"
//
// This example demonstrates mio's core ideas in a minimal, self-contained way:
//
// Event flow:
//
// 1. Listener becomes READABLE  → accept server_stream
// 2. Client becomes WRITABLE    → TCP handshake complete → send "ping"
// 3. server_stream becomes READABLE → read "ping", write "pong"
// 4. Client becomes READABLE    → read "pong", send next "ping"
// 5. Repeat ROUNDS times, then exit
//
// Key ideas:
// - Mio is event-driven: we react to readiness, we never block
// - Tokens identify which socket produced an event
// - WRITABLE on a freshly connected stream means "handshake finished"

use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interest, Poll, Token};
use std::io::{self, Read, Write};

const LISTENER: Token = Token(0);
const CLIENT: Token = Token(1);
const SERVER_STREAM: Token = Token(2);

const ROUNDS: u32 = 5;

#[cfg(not(target_os = "wasi"))]
fn main() -> io::Result<()> {
    env_logger::init();

    // Create a poll instance.
    let mut poll = Poll::new()?;
    // Create storage for events.
    let mut events = Events::with_capacity(16);

    // Bind to port 0 so the OS assigns a free port — no "address already
    // in use" errors when running the example repeatedly.
    let addr = "127.0.0.1:0".parse().unwrap();
    let mut listener = TcpListener::bind(addr)?;
    let server_addr = listener.local_addr()?;

    // Register the listener: notify us when a connection is ready to accept.
    poll.registry()
        .register(&mut listener, LISTENER, Interest::READABLE)?;

    // Start a non-blocking client connection.
    //
    // IMPORTANT: connect() returns immediately without waiting for the
    // TCP handshake. The first WRITABLE event signals it has completed.
    let mut client = TcpStream::connect(server_addr)?;
    poll.registry()
        .register(&mut client, CLIENT, Interest::READABLE | Interest::WRITABLE)?;

    let mut server_stream: Option<TcpStream> = None;
    let mut client_ready = false;
    let mut rounds_left = ROUNDS;

    println!("Client connecting to {server_addr}");
    println!("Ping-pong will run for {ROUNDS} rounds\n");

    loop {
        // Wait for OS readiness events. Guard against spurious EINTR.
        if let Err(err) = poll.poll(&mut events, None) {
            if interrupted(&err) {
                continue;
            }
            return Err(err);
        }

        for event in events.iter() {
            match event.token() {
                // ── Listener ────────────────────────────────────────────────
                LISTENER => {
                    // We only expect one connection in this example.
                    match listener.accept() {
                        Ok((mut conn, addr)) => {
                            println!("Accepted connection from: {addr}");
                            poll.registry().register(
                                &mut conn,
                                SERVER_STREAM,
                                Interest::READABLE,
                            )?;
                            server_stream = Some(conn);
                        }
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                        Err(e) => return Err(e),
                    }
                }

                // ── Client ──────────────────────────────────────────────────
                CLIENT => {
                    // WRITABLE fires once when the TCP handshake completes.
                    // Reregister for READABLE only — we write directly
                    // whenever we need to send, no WRITABLE event needed.
                    if event.is_writable() && !client_ready {
                        client_ready = true;
                        poll.registry()
                            .reregister(&mut client, CLIENT, Interest::READABLE)?;
                        client.write_all(b"ping")?;
                        println!("Client sent: \"ping\" ({rounds_left} rounds left)");
                    }

                    // READABLE fires when the server has replied.
                    if event.is_readable() {
                        let mut buf = [0; 64];
                        // Mio signals readiness — we still read manually.
                        match client.read(&mut buf) {
                            Ok(0) => return Ok(()), // server closed connection
                            Ok(n) => {
                                let msg =
                                    std::str::from_utf8(&buf[..n]).unwrap_or("(invalid utf-8)");
                                println!("Client received: \"{msg}\"");
                                rounds_left -= 1;

                                if rounds_left == 0 {
                                    println!("\nAll rounds complete!");
                                    return Ok(());
                                }

                                // Socket is already writable; send immediately.
                                client.write_all(b"ping")?;
                                println!("Client sent: \"ping\" ({rounds_left} rounds left)");
                            }
                            Err(ref e) if would_block(e) => {}
                            Err(ref e) if interrupted(e) => {}
                            Err(e) => return Err(e),
                        }
                    }
                }

                // ── Server stream ────────────────────────────────────────────
                SERVER_STREAM => {
                    if let Some(ref mut conn) = server_stream {
                        if event.is_readable() {
                            let mut buf = [0; 64];
                            match conn.read(&mut buf) {
                                Ok(0) => return Ok(()),
                                Ok(n) => {
                                    let msg =
                                        std::str::from_utf8(&buf[..n]).unwrap_or("(invalid utf-8)");
                                    println!("Server received: \"{msg}\" → replying \"pong\"");
                                    conn.write_all(b"pong")?;
                                }
                                Err(ref e) if would_block(e) => {}
                                Err(ref e) if interrupted(e) => {}
                                Err(e) => return Err(e),
                            }
                        }
                    }
                }

                // Sporadic events happen, we can safely ignore them.
                _ => {}
            }
        }
    }
}

fn would_block(err: &io::Error) -> bool {
    err.kind() == io::ErrorKind::WouldBlock
}

fn interrupted(err: &io::Error) -> bool {
    err.kind() == io::ErrorKind::Interrupted
}

#[cfg(target_os = "wasi")]
fn main() {
    panic!("can't bind to an address with wasi")
}
