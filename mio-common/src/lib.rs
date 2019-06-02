//! Crate of types shared by Mio and the platform specific crates.

// This crate is needed to prevent cyclic depedencies, where e.g. mio would
// depend on mio-kqueue for the `Selector` and mio-kqueue would depend on mio
// for the `Token`.
//
// When writing documentation in the crate always write it from the main crate
// (mio) perspective, e.g. in examples don't use `mio_common::SomeType`, but
// `mio::SomeType`.

mod event;
mod interests;
mod poll_opt;
mod ready;
mod token;

pub use event::Event;
pub use interests::Interests;
pub use poll_opt::PollOpt;
pub use ready::Ready;
pub use token::Token;
