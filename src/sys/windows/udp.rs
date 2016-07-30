//! UDP for IOCP
//!
//! Note that most of this module is quite similar to the TCP module, so if
//! something seems odd you may also want to try the docs over there.

use std::fmt;
use std::io::prelude::*;
use std::io;
use std::mem;
use std::net::{self, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::os::windows::prelude::*;
use std::sync::{Mutex, MutexGuard};

#[allow(unused_imports)]
use net2::{UdpBuilder, UdpSocketExt};
use winapi::*;
use miow::iocp::CompletionStatus;
use miow::net::SocketAddrBuf;
use miow::net::UdpSocketExt as MiowUdpSocketExt;

use {Evented, EventSet, Poll, PollOpt, Token};
use poll;
use sys::windows::bad_state;
use sys::windows::from_raw_arc::FromRawArc;
use sys::windows::selector::{Overlapped, Registration};

pub struct UdpSocket {
    imp: Imp,
    registration: Mutex<Option<poll::Registration>>,
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
    iocp: Registration,
    read: State<Vec<u8>, Vec<u8>>,
    write: State<Vec<u8>, (Vec<u8>, usize)>,
    read_buf: SocketAddrBuf,
}

enum Socket {
    Empty,
    Bound(net::UdpSocket),
}

enum State<T, U> {
    Empty,
    Pending(T),
    Ready(U),
    Error(io::Error),
}

impl UdpSocket {
    pub fn new(socket: net::UdpSocket) -> io::Result<UdpSocket> {
        Ok(UdpSocket {
            registration: Mutex::new(None),
            imp: Imp {
                inner: FromRawArc::new(Io {
                    read: Overlapped::new(recv_done),
                    write: Overlapped::new(send_done),
                    inner: Mutex::new(Inner {
                        socket: Socket::Bound(socket),
                        iocp: Registration::new(),
                        read: State::Empty,
                        write: State::Empty,
                        read_buf: SocketAddrBuf::new(),
                    }),
                }),
            },
        })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        try!(self.inner().socket.socket()).local_addr()
    }

    pub fn try_clone(&self) -> io::Result<UdpSocket> {
        let me = self.inner();
        try!(me.socket.socket()).try_clone().and_then(UdpSocket::new)
    }

    /// Note that unlike `TcpStream::write` this function will not attempt to
    /// continue writing `buf` until its entirely written.
    ///
    /// TODO: This... may be wrong in the long run. We're reporting that we
    ///       successfully wrote all of the bytes in `buf` but it's possible
    ///       that we don't actually end up writing all of them!
    pub fn send_to(&self, buf: &[u8], target: &SocketAddr)
                   -> io::Result<Option<usize>> {
        let mut me = self.inner();
        let me = &mut *me;

        match me.write {
            State::Empty => {}
            _ => return Ok(None),
        }

        let s = try!(me.socket.socket());
        if !me.iocp.registered() {
            return Ok(None)
        }

        let interest = me.iocp.readiness();
        me.iocp.set_readiness(interest & !EventSet::writable());

        let mut owned_buf = me.iocp.get_buffer(64 * 1024);
        let amt = try!(owned_buf.write(buf));
        try!(unsafe {
            trace!("scheduling a send");
            s.send_to_overlapped(&owned_buf, target,
                                 self.imp.inner.write.get_mut())
        });
        me.write = State::Pending(owned_buf);
        mem::forget(self.imp.clone());
        Ok(Some(amt))
    }

    pub fn recv_from(&self, mut buf: &mut [u8])
                     -> io::Result<Option<(usize, SocketAddr)>> {
        let mut me = self.inner();
        match mem::replace(&mut me.read, State::Empty) {
            State::Empty => Ok(None),
            State::Pending(b) => { me.read = State::Pending(b); Ok(None) }
            State::Ready(data) => {
                // If we weren't provided enough space to receive the message
                // then don't actually read any data, just return an error.
                if buf.len() < data.len() {
                    me.read = State::Ready(data);
                    Err(io::Error::from_raw_os_error(WSAEMSGSIZE as i32))
                } else {
                    let r = if let Some(addr) = me.read_buf.to_socket_addr() {
                        buf.write(&data).unwrap();
                        Ok(Some((data.len(), addr)))
                    } else {
                        Err(io::Error::new(io::ErrorKind::Other,
                                           "failed to parse socket address"))
                    };
                    me.iocp.put_buffer(data);
                    self.imp.schedule_read(&mut me);
                    r
                }
            }
            State::Error(e) => {
                self.imp.schedule_read(&mut me);
                Err(e)
            }
        }
    }

    pub fn broadcast(&self) -> io::Result<bool> {
        try!(self.inner().socket.socket()).broadcast()
    }

    pub fn set_broadcast(&self, on: bool) -> io::Result<()> {
        try!(self.inner().socket.socket()).set_broadcast(on)
    }

    pub fn multicast_loop_v4(&self) -> io::Result<bool> {
        try!(self.inner().socket.socket()).multicast_loop_v4()
    }

    pub fn set_multicast_loop_v4(&self, on: bool) -> io::Result<()> {
        try!(self.inner().socket.socket()).set_multicast_loop_v4(on)
    }

    pub fn multicast_ttl_v4(&self) -> io::Result<u32> {
        try!(self.inner().socket.socket()).multicast_ttl_v4()
    }

    pub fn set_multicast_ttl_v4(&self, ttl: u32) -> io::Result<()> {
        try!(self.inner().socket.socket()).set_multicast_ttl_v4(ttl)
    }

    pub fn multicast_loop_v6(&self) -> io::Result<bool> {
        try!(self.inner().socket.socket()).multicast_loop_v6()
    }

    pub fn set_multicast_loop_v6(&self, on: bool) -> io::Result<()> {
        try!(self.inner().socket.socket()).set_multicast_loop_v6(on)
    }

    pub fn ttl(&self) -> io::Result<u32> {
        try!(self.inner().socket.socket()).ttl()
    }

    pub fn set_ttl(&self, ttl: u32) -> io::Result<()> {
        try!(self.inner().socket.socket()).set_ttl(ttl)
    }

    pub fn join_multicast_v4(&self,
                             multiaddr: &Ipv4Addr,
                             interface: &Ipv4Addr) -> io::Result<()> {
        try!(self.inner().socket.socket()).join_multicast_v4(multiaddr, interface)
    }

    pub fn join_multicast_v6(&self,
                             multiaddr: &Ipv6Addr,
                             interface: u32) -> io::Result<()> {
        try!(self.inner().socket.socket()).join_multicast_v6(multiaddr, interface)
    }

    pub fn leave_multicast_v4(&self,
                              multiaddr: &Ipv4Addr,
                              interface: &Ipv4Addr) -> io::Result<()> {
        try!(self.inner().socket.socket()).leave_multicast_v4(multiaddr, interface)
    }

    pub fn leave_multicast_v6(&self,
                              multiaddr: &Ipv6Addr,
                              interface: u32) -> io::Result<()> {
        try!(self.inner().socket.socket()).leave_multicast_v6(multiaddr, interface)
    }

    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        try!(self.inner().socket.socket()).take_error()
    }

    fn inner(&self) -> MutexGuard<Inner> {
        self.imp.inner()
    }

    fn post_register(&self, interest: EventSet, me: &mut Inner) {
        if interest.is_readable() {
            self.imp.schedule_read(me);
        }
        // See comments in TcpSocket::post_register for what's going on here
        if interest.is_writable() {
            if let State::Empty = me.write {
                self.imp.add_readiness(me, EventSet::writable());
            }
        }
    }
}

impl Imp {
    fn inner(&self) -> MutexGuard<Inner> {
        self.inner.inner.lock().unwrap()
    }

    fn schedule_read(&self, me: &mut Inner) {
        match me.read {
            State::Empty => {}
            _ => return,
        }
        let socket = match me.socket {
            Socket::Empty => return,
            Socket::Bound(ref s) => s,
        };

        let interest = me.iocp.readiness();
        me.iocp.set_readiness(interest & !EventSet::readable());

        let mut buf = me.iocp.get_buffer(64 * 1024);
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
                self.add_readiness(me, EventSet::readable());
                me.iocp.put_buffer(buf);
            }
        }
    }

    // See comments in tcp::StreamImp::push
    fn add_readiness(&self, me: &Inner, set: EventSet) {
        if let Socket::Empty = me.socket {
            return
        }
        me.iocp.set_readiness(set | me.iocp.readiness());
    }
}

impl Evented for UdpSocket {
    fn register(&self, poll: &Poll, token: Token,
                interest: EventSet, opts: PollOpt) -> io::Result<()> {
        let mut me = self.inner();
        {
            let me = &mut *me;
            let socket = match me.socket {
                Socket::Bound(ref s) => s as &AsRawSocket,
                Socket::Empty => return Err(bad_state()),
            };
            try!(me.iocp.register_socket(socket, poll, token, interest, opts,
                                         &self.registration));
        }
        self.post_register(interest, &mut me);
        Ok(())
    }

    fn reregister(&self, poll: &Poll, token: Token,
                  interest: EventSet, opts: PollOpt) -> io::Result<()> {
        let mut me = self.inner();
        {
            let me = &mut *me;
            let socket = match me.socket {
                Socket::Bound(ref s) => s as &AsRawSocket,
                Socket::Empty => return Err(bad_state()),
            };
            try!(me.iocp.reregister_socket(socket, poll, token, interest,
                                           opts, &self.registration));
        }
        self.post_register(interest, &mut me);
        Ok(())
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        self.inner().iocp.deregister(poll, &self.registration)
    }
}

impl fmt::Debug for UdpSocket {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        "UdpSocket { ... }".fmt(f)
    }
}

impl Drop for UdpSocket {
    fn drop(&mut self) {
        let mut inner = self.inner();

        inner.socket = Socket::Empty;

        // Then run any finalization code including level notifications
        inner.iocp.set_readiness(EventSet::none());
    }
}

impl Socket {
    fn socket(&self) -> io::Result<&net::UdpSocket> {
        match *self {
            Socket::Bound(ref s) => Ok(s),
            Socket::Empty => Err(bad_state()),
        }
    }
}

fn send_done(status: &CompletionStatus) {
    trace!("finished a send {}", status.bytes_transferred());
    let me2 = Imp {
        inner: unsafe { overlapped2arc!(status.overlapped(), Io, write) },
    };
    let mut me = me2.inner();
    me.write = State::Empty;
    me2.add_readiness(&mut me, EventSet::writable());
}

fn recv_done(status: &CompletionStatus) {
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
    me2.add_readiness(&mut me, EventSet::readable());
}
