//! UDP for IOCP
//!
//! Note that most of this module is quite similar to the TCP module, so if
//! something seems odd you may also want to try the docs over there.

use std::fmt;
use std::io::prelude::*;
use std::io;
use std::mem;
use std::net::{self, SocketAddr};
use std::os::windows::prelude::*;
use std::sync::{Arc, Mutex, MutexGuard};

use net2::{UdpBuilder, UdpSocketExt};
use winapi::*;
use wio::iocp::CompletionStatus;
use wio::net::SocketAddrBuf;
use wio::net::UdpSocketExt as WioUdpSocketExt;

use {Evented, EventSet, IpAddr, PollOpt, Selector, Token};
use bytes::{Buf, MutBuf};
use sys::windows::selector::{SelectorInner, Overlapped};
use sys::windows::from_raw_arc::FromRawArc;
use sys::windows::{bad_state, wouldblock, Family};

pub struct UdpSocket {
    imp: Imp,
}

#[derive(Clone)]
struct Imp {
    inner: FromRawArc<Io>,
}

struct Io {
    read: Overlapped,
    write: Overlapped,
    inner: Mutex<Inner>,
}

struct Inner {
    socket: Socket,
    family: Family,
    iocp: Option<Arc<SelectorInner>>,
    read: State<Vec<u8>, Vec<u8>>,
    write: State<Vec<u8>, (Vec<u8>, usize)>,
    read_buf: SocketAddrBuf,
}

enum Socket {
    Empty,
    Building(UdpBuilder),
    Bound(net::UdpSocket),
}

enum State<T, U> {
    Empty,
    Pending(T),
    Ready(U),
    Error(io::Error),
}

impl UdpSocket {
    pub fn v4() -> io::Result<UdpSocket> {
        UdpBuilder::new_v4().map(|u| {
            UdpSocket::new(Socket::Building(u), Family::V4)
        })
    }

    /// Returns a new, unbound, non-blocking, IPv6 UDP socket
    pub fn v6() -> io::Result<UdpSocket> {
        UdpBuilder::new_v6().map(|u| {
            UdpSocket::new(Socket::Building(u), Family::V6)
        })
    }

    fn new(socket: Socket, fam: Family) -> UdpSocket {
        UdpSocket {
            imp: Imp {
                inner: FromRawArc::new(Io {
                    read: Overlapped::new(recv_done),
                    write: Overlapped::new(send_done),
                    inner: Mutex::new(Inner {
                        socket: socket,
                        family: fam,
                        iocp: None,
                        read: State::Empty,
                        write: State::Empty,
                        read_buf: SocketAddrBuf::new(),
                    }),
                }),
            },
        }
    }

    pub fn bind(&self, addr: &SocketAddr) -> io::Result<()> {
        let mut me = self.inner();
        let socket = try!(try!(me.socket.builder()).bind(addr));
        me.socket = Socket::Bound(socket);
        Ok(())
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        try!(self.inner().socket.socket()).local_addr()
    }

    pub fn try_clone(&self) -> io::Result<UdpSocket> {
        let me = self.inner();
        try!(me.socket.socket()).try_clone().map(|s| {
            UdpSocket::new(Socket::Bound(s), me.family)
        })
    }

    pub fn send_to<B: Buf>(&self, buf: &mut B, target: &SocketAddr)
                           -> io::Result<Option<()>> {
        match self._send_to(buf.bytes(), target) {
            Ok(n) => { buf.advance(n); Ok(Some(())) }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Note that unlike `TcpStream::write` this function will not attempt to
    /// continue writing `buf` until its entirely written.
    ///
    /// TODO: This... may be wrong in the long run. We're reporting that we
    ///       successfully wrote all of the bytes in `buf` but it's possible
    ///       that we don't actually end up writing all of them!
    fn _send_to(&self, buf: &[u8], target: &SocketAddr) -> io::Result<usize> {
        let mut me = self.inner();
        let me = &mut *me;
        match me.write {
            State::Empty => {}
            _ => return Err(wouldblock())
        }
        let s = try!(me.socket.socket());
        let iocp = match me.iocp {
            Some(ref s) => s,
            None => return Err(wouldblock()),
        };
        let mut owned_buf = iocp.get_buffer(64 * 1024);
        let amt = try!(owned_buf.write(buf));
        try!(unsafe {
            trace!("scheduling a send");
            s.send_to_overlapped(&owned_buf, target,
                                 self.imp.inner.write.get_mut())
        });
        me.write = State::Pending(owned_buf);
        mem::forget(self.imp.clone());
        Ok(amt)
    }

    pub fn recv_from<B: MutBuf>(&self, buf: &mut B)
                                -> io::Result<Option<SocketAddr>> {
        let mut me = self.inner();
        match mem::replace(&mut me.read, State::Empty) {
            State::Empty => Ok(None),
            State::Pending(b) => { me.read = State::Pending(b); Ok(None) }
            State::Ready(data) => {
                // If we weren't provided enough space to receive the message
                // then don't actually read any data, just return an error.
                if buf.remaining() < data.len() {
                    me.read = State::Ready(data);
                    Err(io::Error::from_raw_os_error(WSAEMSGSIZE as i32))
                } else {
                    let r = if let Some(addr) = me.read_buf.to_socket_addr() {
                        buf.write_slice(&data);
                        Ok(Some(addr))
                    } else {
                        Err(io::Error::new(io::ErrorKind::Other,
                                           "failed to parse socket address"))
                    };
                    me.iocp.as_ref().map(|i| i.put_buffer(data));
                    drop(me);
                    self.imp.schedule_read();
                    r
                }
            }
            State::Error(e) => {
                drop(me);
                self.imp.schedule_read();
                Err(e)
            }
        }
    }

    pub fn set_broadcast(&self, on: bool) -> io::Result<()> {
        try!(self.inner().socket.socket()).set_broadcast(on)
    }

    pub fn set_multicast_loop(&self, on: bool) -> io::Result<()> {
        let me = self.inner();
        let socket = try!(me.socket.socket());
        match me.family {
            Family::V4 => socket.set_multicast_loop_v4(on),
            Family::V6 => socket.set_multicast_loop_v6(on),
        }
    }

    pub fn join_multicast(&self, multi: &IpAddr) -> io::Result<()> {
        let me = self.inner();
        let socket = try!(me.socket.socket());
        match *multi {
            IpAddr::V4(ref v4) => {
                socket.join_multicast_v4(v4, &super::ipv4_any())
            }
            IpAddr::V6(ref v6) => {
                socket.join_multicast_v6(v6, 0)
            }
        }
    }

    pub fn leave_multicast(&self, multi: &IpAddr) -> io::Result<()> {
        let me = self.inner();
        let socket = try!(me.socket.socket());
        match *multi {
            IpAddr::V4(ref v4) => {
                socket.leave_multicast_v4(v4, &super::ipv4_any())
            }
            IpAddr::V6(ref v6) => socket.leave_multicast_v6(v6, 0),
        }
    }

    pub fn set_multicast_time_to_live(&self, ttl: i32) -> io::Result<()> {
        try!(self.inner().socket.socket()).set_multicast_ttl_v4(ttl as u32)
    }

    fn inner(&self) -> MutexGuard<Inner> {
        self.imp.inner()
    }

    fn post_register(&self, interest: EventSet, selector: &SelectorInner) {
        if interest.is_readable() {
            self.imp.schedule_read();
        }
        // See comments in TcpSocket::post_register for what's going on here
        if interest.is_writable() {
            let me = self.inner();
            if let State::Empty = me.write {
                if let Socket::Bound(..) = me.socket {
                    selector.defer(me.socket.handle(), EventSet::writable());
                }
            }
        }
    }
}

impl Imp {
    fn inner(&self) -> MutexGuard<Inner> {
        self.inner.inner.lock().unwrap()
    }

    fn schedule_read(&self) {
        let mut me = self.inner();
        let me = &mut *me;
        match me.read {
            State::Empty => {}
            _ => return,
        }
        let iocp = me.iocp.as_ref().unwrap();
        let socket = match me.socket {
            Socket::Empty |
            Socket::Building(..) => return,
            Socket::Bound(ref s) => s,
        };
        let mut buf = iocp.get_buffer(64 * 1024);
        let res = unsafe {
            trace!("scheduling a read");
            let cap = buf.capacity();
            buf.set_len(cap);
            socket.recv_from_overlapped(&mut buf, &mut me.read_buf,
                                        self.inner.read.get_mut())
        };
        match res {
            Ok(_) => {
                me.read = State::Pending(buf);
                mem::forget(self.clone());
            }
            Err(e) => {
                me.read = State::Error(e);
                iocp.defer(me.socket.handle(), EventSet::readable());
                iocp.put_buffer(buf);
            }
        }
    }
}

impl Evented for UdpSocket {
    fn register(&self, selector: &mut Selector, token: Token,
                interest: EventSet, opts: PollOpt) -> io::Result<()> {
        let mut me = self.inner();
        let selector = selector.inner();
        match me.socket {
            Socket::Bound(ref s) => {
                try!(selector.register_socket(s, token, interest, opts));
            }
            Socket::Building(ref b) => {
                try!(selector.register_socket(b, token, interest, opts));
            }
            Socket::Empty => return Err(bad_state()),
        }
        me.iocp = Some(selector.clone());
        drop(me);
        self.post_register(interest, selector);
        Ok(())
    }

    fn reregister(&self, selector: &mut Selector, token: Token,
                  interest: EventSet, opts: PollOpt) -> io::Result<()> {
        let me = self.inner();
        let selector = selector.inner();
        // TODO: assert that me.iocp == selector?
        if me.iocp.is_none() {
            return Err(bad_state())
        }
        match me.socket {
            Socket::Bound(ref s) => {
                try!(selector.reregister_socket(s, token, interest, opts));
            }
            Socket::Building(ref b) => {
                try!(selector.reregister_socket(b, token, interest, opts));
            }
            Socket::Empty => return Err(bad_state()),
        }
        drop(me);
        self.post_register(interest, selector);
        Ok(())
    }

    fn deregister(&self, selector: &mut Selector) -> io::Result<()> {
        let me = self.inner();
        let selector = selector.inner();
        // TODO: assert that me.iocp == selector?
        if me.iocp.is_none() {
            return Err(bad_state())
        }
        match me.socket {
            Socket::Bound(ref s) => selector.deregister_socket(s),
            Socket::Building(ref b) => selector.deregister_socket(b),
            Socket::Empty => Err(bad_state()),
        }
    }
}

impl fmt::Debug for UdpSocket {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        "UdpSocket { ... }".fmt(f)
    }
}

impl Drop for UdpSocket {
    fn drop(&mut self) {
        self.inner().socket = Socket::Empty;
    }
}

impl Socket {
    fn builder(&self) -> io::Result<&UdpBuilder> {
        match *self {
            Socket::Building(ref s) => Ok(s),
            _ => Err(bad_state()),
        }
    }

    fn socket(&self) -> io::Result<&net::UdpSocket> {
        match *self {
            Socket::Bound(ref s) => Ok(s),
            _ => Err(bad_state()),
        }
    }

    fn handle(&self) -> HANDLE {
        match *self {
            Socket::Bound(ref s) => s.as_raw_socket() as HANDLE,
            Socket::Building(ref b) => b.as_raw_socket() as HANDLE,
            Socket::Empty => INVALID_HANDLE_VALUE,
        }
    }
}

fn send_done(status: &CompletionStatus, push: &mut FnMut(HANDLE, EventSet)) {
    trace!("finished a send {}", status.bytes_transferred());
    let me2 = Imp {
        inner: unsafe { overlapped2arc!(status.overlapped(), Io, write) },
    };
    let mut me = me2.inner();
    me.write = State::Empty;
    push(me.socket.handle(), EventSet::writable());
}

fn recv_done(status: &CompletionStatus, push: &mut FnMut(HANDLE, EventSet)) {
    trace!("finished a recv {}", status.bytes_transferred());
    let me2 = Imp {
        inner: unsafe { overlapped2arc!(status.overlapped(), Io, read) },
    };
    let mut me = me2.inner();
    let mut buf = match mem::replace(&mut me.read, State::Empty) {
        State::Pending(buf) => buf,
        _ => unreachable!(),
    };
    unsafe {
        buf.set_len(status.bytes_transferred() as usize);
    }
    me.read = State::Ready(buf);
    push(me.socket.handle(), EventSet::readable());
}
