//! Utilities for non-blocking IO programs

pub use self::mpmc_bounded_queue::Queue as BoundedQueue;
pub use self::slab::Index;

use token::Token;
pub type Slab<T> = slab::Slab<T, Token>;

mod mpmc_bounded_queue;
mod slab;
