//! Crate of types shared by Mio and the platform specific crates.

// This crate is needed to prevent cyclic depedencies, where e.g. mio would
// depend on mio-kqueue for the `Selector` and mio-kqueue would depend on mio
// for the `Token`.

mod token;

pub use token::Token;
