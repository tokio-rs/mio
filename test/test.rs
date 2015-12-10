extern crate mio;
extern crate bytes;

#[macro_use]
extern crate log;
extern crate env_logger;
extern crate tempdir;

pub use ports::localhost;

mod test_battery;
mod test_close_on_drop;
mod test_echo_server;
mod test_multicast;
mod test_notify;
mod test_register_deregister;
mod test_register_multiple_event_loops;
mod test_tcp_level;
mod test_tick;
mod test_timer;
mod test_udp_level;
mod test_udp_socket;

// ===== Unix only tests =====
#[cfg(unix)]
mod test_unix_echo_server;
#[cfg(unix)]
mod test_unix_pass_fd;

mod ports {
    use std::net::SocketAddr;
    use std::str::FromStr;
    use std::sync::atomic::{AtomicUsize, ATOMIC_USIZE_INIT};
    use std::sync::atomic::Ordering::SeqCst;

    // Helper for getting a unique port for the task run
    // TODO: Reuse ports to not spam the system
    static mut NEXT_PORT: AtomicUsize = ATOMIC_USIZE_INIT;
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

#[allow(deprecated)]
pub fn sleep_ms(ms: u64) {
    use std::thread;
    thread::sleep_ms(ms as u32);
}
