#![feature(globs)]
#![feature(phase)]

extern crate mio;

#[phase(plugin, link)]
extern crate log;

pub use ports::localhost;

mod test_close_on_drop;
mod test_echo_server;
mod test_notify;
mod test_timer;
mod test_udp_socket;
mod test_udp_socket_connectionless;
mod test_register_deregister;
mod test_unix_echo_server;

mod ports {
    use std::sync::atomic::{AtomicUint, SeqCst, INIT_ATOMIC_UINT};

    // Helper for getting a unique port for the task run
    // TODO: Reuse ports to not spam the system
    static mut NEXT_PORT: AtomicUint = INIT_ATOMIC_UINT;
    const FIRST_PORT: uint = 18080;

    fn next_port() -> uint {
        unsafe {
            // If the atomic was never used, set it to the initial port
            NEXT_PORT.compare_and_swap(0, FIRST_PORT, SeqCst);

            // Get and increment the port list
            NEXT_PORT.fetch_add(1, SeqCst)
        }
    }

    pub fn localhost() -> String {
        format!("127.0.0.1:{}", next_port())
    }
}
