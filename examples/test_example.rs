use std::io;

use mio::{Poll, Events, Interest, Token};
use mio::unix::pipe;
use std::io::Read;

const PIPE_RECV: Token = Token(0);

fn main() -> io::Result<()> {
    env_logger::init();

// Same setup as in the example above.
    let mut poll = Poll::new()?;
    let mut events = Events::with_capacity(8);
    let (sender, mut receiver) = pipe::new()?;
    poll.registry().register(&mut receiver, PIPE_RECV, Interest::READABLE)?;
// Drop the sender.
    drop(sender);
    poll.poll(&mut events, None)?;
    for event in events.iter() {
        log::info!("Got event {:?}", event);
        match event.token() {
            PIPE_RECV if event.is_read_closed() => {
                // Detected that the sender was dropped.
                println!("Sender dropped!");
                return Ok(());
            },
            PIPE_RECV => {
                // Some platforms don't support detecting that the write end has been closed
                println!("Receiving end is readable, but doesn't know the write has been closed!");
                let mut buf = [0u8; 1];
                assert_eq!(receiver.read(&mut buf).ok(), Some(0));
                return Ok(());
            }
            _ => unreachable!(),
        }
    }
    unreachable!();
    }