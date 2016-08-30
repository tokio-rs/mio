//! Networking primitives
//!
pub mod tcp;
pub mod udp;

use {poll, Poll};
use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Used to associate an IO type with a Selector
#[derive(Debug)]
struct SelectorId {
    id: AtomicUsize,
}

impl SelectorId {
    fn new() -> SelectorId {
        SelectorId {
            id: AtomicUsize::new(0),
        }
    }

    fn associate_selector(&self, poll: &Poll) -> io::Result<()> {
        let selector_id = self.id.load(Ordering::SeqCst);

        if selector_id != 0 && selector_id != poll::selector(poll).id() {
            Err(io::Error::new(io::ErrorKind::Other, "socket already registered"))
        } else {
            self.id.store(poll::selector(poll).id(), Ordering::SeqCst);
            Ok(())
        }
    }
}

impl Clone for SelectorId {
    fn clone(&self) -> SelectorId {
        SelectorId {
            id: AtomicUsize::new(self.id.load(Ordering::SeqCst)),
        }
    }
}
