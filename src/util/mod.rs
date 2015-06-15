//! Utilities for non-blocking IO programs

pub use self::mpmc_bounded_queue::Queue as BoundedQueue;

mod mpmc_bounded_queue;

pub type Slab<T> = ::slab::Slab<T, ::Token>;
