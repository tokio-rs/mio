//! Utilities for non-blocking IO programs

pub use self::mpmc_bounded_queue::Queue as BoundedQueue;
pub use self::slab::Slab;

mod mpmc_bounded_queue;
mod slab;
