#[cfg(target_env = "p2")]
mod p2;

#[cfg(target_env = "p2")]
pub(crate) use p2::*;

#[cfg(not(target_env = "p2"))]
mod p1;

#[cfg(not(target_env = "p2"))]
pub(crate) use p1::*;
