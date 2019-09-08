use mio::net::UdpSocket;
use mio::{Events, Interests, Poll, Token};
use std::io;

const IN: Token = Token(0);

fn main() -> io::Result<()> {
    // Set up a new socket on port 9000 to listen on.
    let socket = UdpSocket::bind("0.0.0.0:9000".parse().unwrap())?;
    // Initialize poller.
    let mut poll = Poll::new()?;
    // Register our socket with the token IN (defined above) and an interest
    // in being `READABLE`.
    poll.registry().register(&socket, IN, Interests::READABLE)?;

    // Prepare a buffer for the number of events we can handle at a time.
    // Someone might wat to echo really fast so lets give it some size.
    let mut events = Events::with_capacity(1024);
    // Initialize a buffer for the UDP datagram
    let mut buf = [0; 65535];
    // Main loop
    loop {
        // Poll if we have events waiting for us on the socket.
        poll.poll(&mut events, None)?;
        // If we do iterate throuigh them
        for event in events.iter() {
            // Validate the token we registered our socket with,
            // in this example it will only ever be one but we
            // make sure it's valid none the less.
            match event.token() {
                IN => loop {
                    // In this loop we receive from the socket as long as we
                    // can read data
                    match socket.recv_from(&mut buf) {
                        Ok((n, from_addr)) => {
                            // Send the data right back from where it came from.
                            socket.send_to(&buf[..n], from_addr)?;
                        }
                        Err(e) => {
                            // If we failed to receive data we have two cases
                            if e.kind() == io::ErrorKind::WouldBlock {
                                // If the reason was `WouldBlock` we know
                                // our socket has no more data to give so
                                // we can return to the poll to wait politely.
                                break;
                            } else {
                                // If it was any other kind of error, something
                                // went wrong and we terminate with an error.
                                return Err(e);
                            }
                        }
                    }
                },
                // We only have IN as a token, so this should never ever be hit
                _ => unreachable!(),
            }
        }
    }
}
