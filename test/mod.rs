#[macro_use]
extern crate log;

pub use ports::localhost;

mod test_close_on_drop;
mod test_double_register;
mod test_drop_cancels_interest_and_shuts_down;
mod test_echo_server;
mod test_evented;
mod test_interests;
mod test_local_addr_ready;
mod test_multicast;
mod test_poll;
mod test_register_deregister;
mod test_register_multiple_event_loops;
mod test_reregister_without_poll;
mod test_smoke;
mod test_tcp;
mod test_tcp_shutdown;
mod test_udp_socket;
mod test_waker;
mod test_write_then_drop;

use bytes::{Buf, BufMut};
use mio::{Events, Poll};
use std::io::{self, Read, Write};
use std::time::Duration;

pub trait TryRead {
    fn try_read_buf<B: BufMut>(&mut self, buf: &mut B) -> io::Result<Option<usize>>
    where
        Self: Sized,
    {
        // Reads the length of the slice supplied by buf.mut_bytes into the buffer
        // This is not guaranteed to consume an entire datagram or segment.
        // If your protocol is msg based (instead of continuous stream) you should
        // ensure that your buffer is large enough to hold an entire segment (1532 bytes if not jumbo
        // frames)
        let res = self.try_read(unsafe { buf.bytes_mut() });

        if let Ok(Some(cnt)) = res {
            unsafe {
                buf.advance_mut(cnt);
            }
        }

        res
    }

    fn try_read(&mut self, buf: &mut [u8]) -> io::Result<Option<usize>>;
}

pub trait TryWrite {
    fn try_write_buf<B: Buf>(&mut self, buf: &mut B) -> io::Result<Option<usize>>
    where
        Self: Sized,
    {
        let res = self.try_write(buf.bytes());

        if let Ok(Some(cnt)) = res {
            buf.advance(cnt);
        }

        res
    }

    fn try_write(&mut self, buf: &[u8]) -> io::Result<Option<usize>>;
}

impl<T: Read> TryRead for T {
    fn try_read(&mut self, dst: &mut [u8]) -> io::Result<Option<usize>> {
        self.read(dst).map_non_block()
    }
}

impl<T: Write> TryWrite for T {
    fn try_write(&mut self, src: &[u8]) -> io::Result<Option<usize>> {
        self.write(src).map_non_block()
    }
}

/*
 *
 * ===== Helpers =====
 *
 */

/// A helper trait to provide the map_non_block function on Results.
trait MapNonBlock<T> {
    /// Maps a `Result<T>` to a `Result<Option<T>>` by converting
    /// operation-would-block errors into `Ok(None)`.
    fn map_non_block(self) -> io::Result<Option<T>>;
}

impl<T> MapNonBlock<T> for io::Result<T> {
    fn map_non_block(self) -> io::Result<Option<T>> {
        use std::io::ErrorKind::WouldBlock;

        match self {
            Ok(value) => Ok(Some(value)),
            Err(err) => {
                if let WouldBlock = err.kind() {
                    Ok(None)
                } else {
                    Err(err)
                }
            }
        }
    }
}

mod ports {
    use std::net::SocketAddr;
    use std::str::FromStr;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering::SeqCst;

    // Helper for getting a unique port for the task run
    // TODO: Reuse ports to not spam the system
    static mut NEXT_PORT: AtomicUsize = AtomicUsize::new(0);
    const FIRST_PORT: usize = 18080;

    fn next_port() -> usize {
        unsafe {
            // If the atomic was never used, set it to the initial port
            NEXT_PORT.compare_and_swap(0, FIRST_PORT, SeqCst);

            // Get and increment the port list
            NEXT_PORT.fetch_add(1, SeqCst)
        }
    }

    pub fn localhost() -> SocketAddr {
        let s = format!("127.0.0.1:{}", next_port());
        FromStr::from_str(&s).unwrap()
    }
}

pub fn sleep_ms(ms: u64) {
    use std::thread;
    thread::sleep(Duration::from_millis(ms));
}

pub fn expect_no_events(poll: &mut Poll, events: &mut Events) {
    poll.poll(events, Some(Duration::from_millis(50)))
        .expect("unable to poll");
    assert_eq!(
        events.iter().count(),
        0,
        "received events, but didn't expect any"
    );
}
