use {io};
use sys::unix::nix;
use std::os::unix::io::AsRawFd;

pub trait Socket : AsRawFd {
    /// Returns the value for the `SO_LINGER` socket option.
    fn linger(&self) -> io::Result<usize> {
        let linger = try!(nix::getsockopt(self.as_raw_fd(), nix::sockopt::Linger)
            .map_err(super::from_nix_error));

        if linger.l_onoff > 0 {
            Ok(linger.l_onoff as usize)
        } else {
            Ok(0)
        }
    }

    /// Sets the value for the `SO_LINGER` socket option
    fn set_linger(&self, dur_s: usize) -> io::Result<()> {
        let linger = nix::linger {
            l_onoff: (if dur_s > 0 { 1 } else { 0 }) as nix::c_int,
            l_linger: dur_s as nix::c_int
        };

        nix::setsockopt(self.as_raw_fd(), nix::sockopt::Linger, &linger)
            .map_err(super::from_nix_error)
    }

    fn set_reuseaddr(&self, val: bool) -> io::Result<()> {
        nix::setsockopt(self.as_raw_fd(), nix::sockopt::ReuseAddr, &val)
            .map_err(super::from_nix_error)
    }

    fn set_reuseport(&self, val: bool) -> io::Result<()> {
        nix::setsockopt(self.as_raw_fd(), nix::sockopt::ReusePort, &val)
            .map_err(super::from_nix_error)
    }

    fn set_tcp_nodelay(&self, val: bool) -> io::Result<()> {
        nix::setsockopt(self.as_raw_fd(),  nix::sockopt::TcpNoDelay, &val)
            .map_err(super::from_nix_error)
    }

    /// Sets the `SO_RCVTIMEO` socket option to the supplied number of
    /// milliseconds.
    ///
    /// This function is hardcoded to milliseconds until Rust std includes a
    /// stable duration type.
    fn set_read_timeout_ms(&self, val: usize) -> io::Result<()> {
        let t = nix::TimeVal::milliseconds(val as i64);
        nix::setsockopt(self.as_raw_fd(), nix::sockopt::ReceiveTimeout, &t)
            .map_err(super::from_nix_error)
    }

    /// Sets the `SO_SNDTIMEO` socket option to the supplied number of
    /// milliseconds.
    ///
    /// This function is hardcoded to milliseconds until Rust std includes a
    /// stable duration type.
    fn set_write_timeout_ms(&self, val: usize) -> io::Result<()> {
        let t = nix::TimeVal::milliseconds(val as i64);
        nix::setsockopt(self.as_raw_fd(), nix::sockopt::SendTimeout, &t)
            .map_err(super::from_nix_error)
    }
}
