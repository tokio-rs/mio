use super::{init, new_socket, socket_addr, InternalState};
use crate::{event, poll, Interest, Registry, Token};

use std::net::{self, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::os::windows::io::{AsRawSocket, FromRawSocket, IntoRawSocket, RawSocket};
use std::os::windows::raw::SOCKET as StdSocket; // winapi uses usize, stdlib uses u32/u64.
use std::{fmt, io};
use winapi::um::winsock2::{bind, closesocket, SOCKET_ERROR, SOCK_DGRAM};

pub struct UdpSocket {
    // This is `None` if the socket has not yet been registered.
    state: Option<Box<InternalState>>,
    inner: net::UdpSocket,
}

impl UdpSocket {
    pub fn bind(addr: SocketAddr) -> io::Result<UdpSocket> {
        init();
        new_socket(addr, SOCK_DGRAM).and_then(|socket| {
            let (raw_addr, raw_addr_length) = socket_addr(&addr);
            syscall!(
                bind(socket, raw_addr, raw_addr_length,),
                PartialEq::eq,
                SOCKET_ERROR
            )
            .map_err(|err| {
                // Close the socket if we hit an error, ignoring the error
                // from closing since we can't pass back two errors.
                let _ = unsafe { closesocket(socket) };
                err
            })
            .map(|_| UdpSocket {
                state: None,
                inner: unsafe { net::UdpSocket::from_raw_socket(socket as StdSocket) },
            })
        })
    }

    pub fn from_std(inner: net::UdpSocket) -> UdpSocket {
        UdpSocket { state: None, inner }
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }

    pub fn try_clone(&self) -> io::Result<UdpSocket> {
        self.inner
            .try_clone()
            .map(|inner| UdpSocket { state: None, inner })
    }

    pub fn send_to(&self, buf: &[u8], target: SocketAddr) -> io::Result<usize> {
        try_io!(self, send_to, buf, target)
    }

    pub fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        try_io!(self, recv_from, buf)
    }

    pub fn peek_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        try_io!(self, peek_from, buf)
    }

    pub fn send(&self, buf: &[u8]) -> io::Result<usize> {
        try_io!(self, send, buf)
    }

    pub fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        try_io!(self, recv, buf)
    }

    pub fn peek(&self, buf: &mut [u8]) -> io::Result<usize> {
        try_io!(self, peek, buf)
    }

    pub fn connect(&self, addr: SocketAddr) -> io::Result<()> {
        self.inner.connect(addr)
    }

    pub fn broadcast(&self) -> io::Result<bool> {
        self.inner.broadcast()
    }

    pub fn set_broadcast(&self, on: bool) -> io::Result<()> {
        self.inner.set_broadcast(on)
    }

    pub fn multicast_loop_v4(&self) -> io::Result<bool> {
        self.inner.multicast_loop_v4()
    }

    pub fn set_multicast_loop_v4(&self, on: bool) -> io::Result<()> {
        self.inner.set_multicast_loop_v4(on)
    }

    pub fn multicast_ttl_v4(&self) -> io::Result<u32> {
        self.inner.multicast_ttl_v4()
    }

    pub fn set_multicast_ttl_v4(&self, ttl: u32) -> io::Result<()> {
        self.inner.set_multicast_ttl_v4(ttl)
    }

    pub fn multicast_loop_v6(&self) -> io::Result<bool> {
        self.inner.multicast_loop_v6()
    }

    pub fn set_multicast_loop_v6(&self, on: bool) -> io::Result<()> {
        self.inner.set_multicast_loop_v6(on)
    }

    pub fn ttl(&self) -> io::Result<u32> {
        self.inner.ttl()
    }

    pub fn set_ttl(&self, ttl: u32) -> io::Result<()> {
        self.inner.set_ttl(ttl)
    }

    pub fn join_multicast_v4(&self, multiaddr: Ipv4Addr, interface: Ipv4Addr) -> io::Result<()> {
        self.inner.join_multicast_v4(&multiaddr, &interface)
    }

    pub fn join_multicast_v6(&self, multiaddr: &Ipv6Addr, interface: u32) -> io::Result<()> {
        self.inner.join_multicast_v6(multiaddr, interface)
    }

    pub fn leave_multicast_v4(&self, multiaddr: Ipv4Addr, interface: Ipv4Addr) -> io::Result<()> {
        self.inner.leave_multicast_v4(&multiaddr, &interface)
    }

    pub fn leave_multicast_v6(&self, multiaddr: &Ipv6Addr, interface: u32) -> io::Result<()> {
        self.inner.leave_multicast_v6(multiaddr, interface)
    }

    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.inner.take_error()
    }

    // Used by `try_io` to register after an I/O operation blocked.
    fn io_blocked_reregister(&self) -> io::Result<()> {
        self.state.as_ref().map_or(Ok(()), |state| {
            state
                .selector
                .reregister(&state.sock_state, state.token, state.interests)
        })
    }
}

impl event::Source for UdpSocket {
    fn register(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> io::Result<()> {
        if self.state.is_some() {
            Err(io::Error::from(io::ErrorKind::AlreadyExists))
        } else {
            poll::selector(registry)
                .register(self.inner.as_raw_socket(), token, interests)
                .map(|state| {
                    self.state = Some(Box::new(state));
                })
        }
    }

    fn reregister(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> io::Result<()> {
        match self.state.as_mut() {
            Some(state) => poll::selector(registry)
                .reregister(&state.sock_state, token, interests)
                .map(|()| {
                    state.token = token;
                    state.interests = interests;
                }),
            None => Err(io::Error::from(io::ErrorKind::NotFound)),
        }
    }

    fn deregister(&mut self, _registry: &Registry) -> io::Result<()> {
        match self.state.as_mut() {
            Some(state) => {
                let mut sock_state = state.sock_state.lock().unwrap();
                sock_state.mark_delete();
                Ok(())
            }
            None => Err(io::Error::from(io::ErrorKind::NotFound)),
        }
    }
}

impl fmt::Debug for UdpSocket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.inner, f)
    }
}

impl FromRawSocket for UdpSocket {
    unsafe fn from_raw_socket(rawsocket: RawSocket) -> UdpSocket {
        UdpSocket {
            state: None,
            inner: net::UdpSocket::from_raw_socket(rawsocket),
        }
    }
}

impl IntoRawSocket for UdpSocket {
    fn into_raw_socket(self) -> RawSocket {
        self.inner.as_raw_socket()
    }
}

impl AsRawSocket for UdpSocket {
    fn as_raw_socket(&self) -> RawSocket {
        self.inner.as_raw_socket()
    }
}
