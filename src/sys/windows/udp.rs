use super::selector::SockState;
use super::InternalState;
use super::{new_socket, socket_addr};
use crate::poll;
use crate::sys::windows::init;
use crate::{event, Interests, Registry, Token};

use std::net::{self, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::os::windows::io::{AsRawSocket, FromRawSocket, IntoRawSocket, RawSocket};
use std::os::windows::raw::SOCKET as StdSocket; // winapi uses usize, stdlib uses u32/u64.
use std::sync::{Arc, Mutex};
use std::{fmt, io};
use winapi::um::winsock2::{bind, closesocket, SOCKET_ERROR, SOCK_DGRAM};

pub struct UdpSocket {
    internal: Arc<Mutex<Option<InternalState>>>,
    io: net::UdpSocket,
}

macro_rules! wouldblock {
    ($self:ident, $method:ident, $($args:expr),* )  => {{
        let result = $self.io.$method($($args),*);
        if let Err(ref e) = result {
            if e.kind() == io::ErrorKind::WouldBlock {
                let internal = $self.internal.lock().unwrap();
                if internal.is_some() {
                    let selector = internal.as_ref().unwrap().selector.clone();
                    let token = internal.as_ref().unwrap().token;
                    let interests = internal.as_ref().unwrap().interests;
                    drop(internal);
                    selector.reregister(
                        $self,
                        token,
                        interests,
                    )?;
                }
            }
        }
        result
    }};
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
                internal: Arc::new(Mutex::new(None)),
                io: unsafe { net::UdpSocket::from_raw_socket(socket as StdSocket) },
            })
        })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.io.local_addr()
    }

    pub fn try_clone(&self) -> io::Result<UdpSocket> {
        self.io.try_clone().map(|io| UdpSocket {
            internal: Arc::new(Mutex::new(None)),
            io,
        })
    }

    pub fn send_to(&self, buf: &[u8], target: SocketAddr) -> io::Result<usize> {
        wouldblock!(self, send_to, buf, target)
    }

    pub fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        wouldblock!(self, recv_from, buf)
    }

    pub fn peek_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        wouldblock!(self, peek_from, buf)
    }

    pub fn send(&self, buf: &[u8]) -> io::Result<usize> {
        wouldblock!(self, send, buf)
    }

    pub fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        wouldblock!(self, recv, buf)
    }

    pub fn peek(&self, buf: &mut [u8]) -> io::Result<usize> {
        wouldblock!(self, peek, buf)
    }

    pub fn connect(&self, addr: SocketAddr) -> io::Result<()> {
        self.io.connect(addr)
    }

    pub fn broadcast(&self) -> io::Result<bool> {
        self.io.broadcast()
    }

    pub fn set_broadcast(&self, on: bool) -> io::Result<()> {
        self.io.set_broadcast(on)
    }

    pub fn multicast_loop_v4(&self) -> io::Result<bool> {
        self.io.multicast_loop_v4()
    }

    pub fn set_multicast_loop_v4(&self, on: bool) -> io::Result<()> {
        self.io.set_multicast_loop_v4(on)
    }

    pub fn multicast_ttl_v4(&self) -> io::Result<u32> {
        self.io.multicast_ttl_v4()
    }

    pub fn set_multicast_ttl_v4(&self, ttl: u32) -> io::Result<()> {
        self.io.set_multicast_ttl_v4(ttl)
    }

    pub fn multicast_loop_v6(&self) -> io::Result<bool> {
        self.io.multicast_loop_v6()
    }

    pub fn set_multicast_loop_v6(&self, on: bool) -> io::Result<()> {
        self.io.set_multicast_loop_v6(on)
    }

    pub fn ttl(&self) -> io::Result<u32> {
        self.io.ttl()
    }

    pub fn set_ttl(&self, ttl: u32) -> io::Result<()> {
        self.io.set_ttl(ttl)
    }

    pub fn join_multicast_v4(&self, multiaddr: Ipv4Addr, interface: Ipv4Addr) -> io::Result<()> {
        self.io.join_multicast_v4(&multiaddr, &interface)
    }

    pub fn join_multicast_v6(&self, multiaddr: &Ipv6Addr, interface: u32) -> io::Result<()> {
        self.io.join_multicast_v6(multiaddr, interface)
    }

    pub fn leave_multicast_v4(&self, multiaddr: Ipv4Addr, interface: Ipv4Addr) -> io::Result<()> {
        self.io.leave_multicast_v4(&multiaddr, &interface)
    }

    pub fn leave_multicast_v6(&self, multiaddr: &Ipv6Addr, interface: u32) -> io::Result<()> {
        self.io.leave_multicast_v6(multiaddr, interface)
    }

    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.io.take_error()
    }
}

impl super::SocketState for UdpSocket {
    fn get_sock_state(&self) -> Option<Arc<Mutex<SockState>>> {
        let internal = self.internal.lock().unwrap();
        match &*internal {
            Some(internal) => match &internal.sock_state {
                Some(arc) => Some(arc.clone()),
                None => None,
            },
            None => None,
        }
    }
    fn set_sock_state(&self, sock_state: Option<Arc<Mutex<SockState>>>) {
        let mut internal = self.internal.lock().unwrap();
        match &mut *internal {
            Some(internal) => {
                internal.sock_state = sock_state;
            }
            None => {}
        };
    }
}

impl event::Source for UdpSocket {
    fn register(&self, registry: &Registry, token: Token, interests: Interests) -> io::Result<()> {
        {
            let mut internal = self.internal.lock().unwrap();
            if internal.is_none() {
                *internal = Some(InternalState::new(
                    poll::selector(registry).clone_inner(),
                    token,
                    interests,
                ));
            }
        }
        let result = poll::selector(registry).register(self, token, interests);
        match result {
            Ok(_) => {}
            Err(_) => {
                let mut internal = self.internal.lock().unwrap();
                *internal = None;
            }
        }
        result
    }

    fn reregister(
        &self,
        registry: &Registry,
        token: Token,
        interests: Interests,
    ) -> io::Result<()> {
        let result = poll::selector(registry).reregister(self, token, interests);
        match result {
            Ok(_) => {
                let mut internal = self.internal.lock().unwrap();
                internal.as_mut().unwrap().token = token;
                internal.as_mut().unwrap().interests = interests;
            }
            Err(_) => {}
        };
        result
    }

    fn deregister(&self, registry: &Registry) -> io::Result<()> {
        let result = poll::selector(registry).deregister(self);
        match result {
            Ok(_) => {
                let mut internal = self.internal.lock().unwrap();
                *internal = None;
            }
            Err(_) => {}
        };
        result
    }
}

impl fmt::Debug for UdpSocket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.io, f)
    }
}

impl FromRawSocket for UdpSocket {
    unsafe fn from_raw_socket(rawsocket: RawSocket) -> UdpSocket {
        UdpSocket {
            internal: Arc::new(Mutex::new(None)),
            io: net::UdpSocket::from_raw_socket(rawsocket),
        }
    }
}

impl IntoRawSocket for UdpSocket {
    fn into_raw_socket(self) -> RawSocket {
        self.io.as_raw_socket()
    }
}

impl AsRawSocket for UdpSocket {
    fn as_raw_socket(&self) -> RawSocket {
        self.io.as_raw_socket()
    }
}
