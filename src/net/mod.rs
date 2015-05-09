//! Networking primitives
//!
use std::net::AddrParseError;
use std::str::FromStr;

pub mod tcp;
pub mod udp;

#[cfg(unix)]
pub mod unix;

/// An IP address, either a IPv4 or IPv6 address.
///
/// Once `std::net::IpAddr` is stable, this will go away.
pub enum IpAddr {
    V4(Ipv4Addr),
    V6(Ipv6Addr),
}

pub use std::net::Ipv4Addr;
pub use std::net::Ipv6Addr;

impl FromStr for IpAddr {
    type Err = AddrParseError;

    fn from_str(s: &str) -> Result<IpAddr, AddrParseError> {
        s.parse()
            .map(|ip: Ipv4Addr| IpAddr::V4(ip))
            .or_else(|_| {
                s.parse().map(|ip: Ipv6Addr| IpAddr::V6(ip))
            })
    }
}
