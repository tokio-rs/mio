use std::fmt;
use std::io::{self, Read, Write, Cursor};
use std::mem;
use std::net::{SocketAddrV4, SocketAddrV6};
use std::net::{self, SocketAddr, TcpStream, TcpListener};
use std::os::windows::prelude::*;
use std::sync::{Arc, Mutex, MutexGuard};

use net2::{self, TcpBuilder};
use net::tcp::Shutdown;
use wio::iocp::CompletionStatus;
use wio::net::*;
use winapi::*;

use {Evented, EventSet, PollOpt, Selector, Token};
use sys::windows::selector::{SelectorInner, Overlapped};
use sys::windows::{bad_state, wouldblock, Family};
use sys::windows::from_raw_arc::FromRawArc;

pub struct TcpSocket {
    /// Separately stored implementation to ensure that the `Drop`
    /// implementation on this type is only executed when it's actually dropped
    /// (many clones of this `imp` are made).
    imp: Imp,
}

#[derive(Clone)]
struct Imp {
    /// A stable address and synchronized access for all internals. This serves
    /// to ensure that all `Overlapped` pointers are valid for a long period of
    /// time as well as allowing completion callbacks to have access to the
    /// internals without having ownership.
    ///
    /// Note that the reference count also allows us "loan out" copies to
    /// completion ports while I/O is running to guarantee that this stays alive
    /// until the I/O completes. You'll notice a number of calls to
    /// `mem::forget` below, and these only happen on successful scheduling of
    /// I/O and are paired with `overlapped2arc!` macro invocations in the
    /// completion callbacks (to have a decrement match the increment).
    inner: FromRawArc<Io>,
}

struct Io {
    inner: Mutex<Inner>,
    read: Overlapped, // also used for connect/accept
    write: Overlapped,
}

struct Inner {
    socket: Socket,
    family: Family,
    iocp: Option<Arc<SelectorInner>>,
    deferred_connect: Option<SocketAddr>,
    bound: bool,
    read: State<Vec<u8>, Cursor<Vec<u8>>>,
    write: State<(Vec<u8>, usize), (Vec<u8>, usize)>,
    accept: State<TcpStream, TcpStream>,
    accept_buf: AcceptAddrsBuf,
}

/// Internal state transitions for this socket.
///
/// This enum keeps track of which `std::net` primitive we currently are.
/// Reusing `std::net` allows us to use the extension traits in `net2` and `wio`
/// along with not having to manage the literal socket creation ourselves.
enum Socket {
    Empty,                  // socket has been closed
    Building(TcpBuilder),   // not-connected nor not-listened socket
    Stream(TcpStream),      // accepted or connected socket
    Listener(TcpListener),  // listened socket
}

enum State<T, U> {
    Empty,              // no I/O operation in progress
    Pending(T),         // an I/O operation is in progress
    Ready(U),           // I/O has finished with this value
    Error(io::Error),   // there was an I/O error
}

impl TcpSocket {
    pub fn v4() -> io::Result<TcpSocket> {
        TcpBuilder::new_v4().map(|s| {
            TcpSocket::new(Socket::Building(s), Family::V4)
        })
    }

    pub fn v6() -> io::Result<TcpSocket> {
        TcpBuilder::new_v6().map(|s| {
            TcpSocket::new(Socket::Building(s), Family::V6)
        })
    }

    fn new(socket: Socket, fam: Family) -> TcpSocket {
        TcpSocket {
            imp: Imp {
                inner: FromRawArc::new(Io {
                    read: Overlapped::new(read_done),
                    write: Overlapped::new(write_done),
                    inner: Mutex::new(Inner {
                        socket: socket,
                        family: fam,
                        iocp: None,
                        deferred_connect: None,
                        bound: false,
                        accept: State::Empty,
                        read: State::Empty,
                        write: State::Empty,
                        accept_buf: AcceptAddrsBuf::new(),
                    }),
                }),
            },
        }
    }

    pub fn connect(&self, addr: &SocketAddr) -> io::Result<bool> {
        let mut me = self.inner();
        let me = &mut *me;
        if me.deferred_connect.is_some() {
            return Err(bad_state())
        }
        // If we haven't been registered defer the actual connect until we're
        // registered
        if me.iocp.is_none() {
            me.deferred_connect = Some(*addr);
            return Ok(false)
        }
        let (socket, connected) = match me.socket {
            Socket::Building(ref b) => {
                // connect_overlapped only works on bound sockets, so if we're
                // not bound yet go ahead and bind us
                if !me.bound {
                    try!(b.bind(&addr_any(me.family)));
                    me.bound = true;
                }
                let res = unsafe {
                    trace!("scheduling a connect");
                    try!(b.connect_overlapped(addr,
                                              self.imp.inner.read.get_mut()))
                };
                // see docs above on Imp.inner for rationale on forget
                mem::forget(self.imp.clone());
                res
            }
            _ => return Err(bad_state()),
        };
        me.socket = Socket::Stream(socket);
        Ok(connected)
    }

    pub fn bind(&self, addr: &SocketAddr) -> io::Result<()> {
        let mut me = self.inner();
        try!(try!(me.socket.builder()).bind(addr));
        me.bound = true;
        Ok(())
    }

    pub fn listen(&self, backlog: usize) -> io::Result<()> {
        let mut me = self.inner();
        let listener = try!(try!(me.socket.builder()).listen(backlog as i32));
        me.socket = Socket::Listener(listener);
        Ok(())
    }

    pub fn accept(&self) -> io::Result<Option<TcpSocket>> {
        let mut me = self.inner();
        try!(me.socket.listener());
        let ret = match mem::replace(&mut me.accept, State::Empty) {
            State::Empty => return Ok(None),
            State::Pending(t) => {
                me.accept = State::Pending(t);
                return Ok(None)
            }
            State::Ready(s) => {
                Ok(Some(TcpSocket::new(Socket::Stream(s), me.family)))
            }
            State::Error(e) => Err(e),
        };
        drop(me);
        self.imp.schedule_read();
        return ret
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        try!(self.inner().socket.stream()).peer_addr()
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        match self.inner().socket {
            Socket::Stream(ref s) => s.local_addr(),
            Socket::Listener(ref s) => s.local_addr(),
            Socket::Empty |
            Socket::Building(..) => Err(bad_state()),
        }
    }

    pub fn try_clone(&self) -> io::Result<TcpSocket> {
        let me = self.inner();
        match me.socket {
            Socket::Stream(ref s) => s.try_clone().map(|s| {
                TcpSocket::new(Socket::Stream(s), me.family)
            }),
            Socket::Listener(ref s) => s.try_clone().map(|s| {
                TcpSocket::new(Socket::Listener(s), me.family)
            }),
            Socket::Empty |
            Socket::Building(..) => Err(bad_state()),
        }
    }

    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        try!(self.inner().socket.stream()).shutdown(match how {
            Shutdown::Read => net::Shutdown::Read,
            Shutdown::Write => net::Shutdown::Write,
            Shutdown::Both => net::Shutdown::Both,
        })
    }

    /*
     *
     * ===== Socket Options =====
     *
     */

    pub fn set_reuseaddr(&self, val: bool) -> io::Result<()> {
        try!(self.inner().socket.builder()).reuse_address(val).map(|_| ())
    }

    pub fn take_socket_error(&self) -> io::Result<()> {
        unimplemented!();
    }

    pub fn set_nodelay(&self, nodelay: bool) -> io::Result<()> {
        net2::TcpStreamExt::set_nodelay(try!(self.inner().socket.stream()),
                                        nodelay)
    }

    pub fn set_keepalive(&self, seconds: Option<u32>) -> io::Result<()> {
        let dur = seconds.map(|s| s * 1000);
        net2::TcpStreamExt::set_keepalive_ms(try!(self.inner().socket.stream()),
                                             dur)
    }

    fn inner(&self) -> MutexGuard<Inner> {
        self.imp.inner()
    }

    fn post_register(&self, interest: EventSet, selector: &SelectorInner) {
        if interest.is_readable() {
            self.imp.schedule_read();
        }

        // At least with epoll, if a socket is registered with an interest in
        // writing and it's immediately writable then a writable event is
        // generated immediately, so do so here.
        if interest.is_writable() {
            let me = self.inner();
            if let State::Empty = me.write {
                if let Socket::Stream(..) = me.socket {
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

    /// Issues a "read" operation for this socket, if applicable.
    ///
    /// This is intended to be invoked from either a completion callback or a
    /// normal context. The function is infallible because errors are stored
    /// internally to be returned later.
    ///
    /// It is required that this function is only called after the handle has
    /// been registered with an event loop.
    fn schedule_read(&self) {
        let mut me = self.inner();
        let me = &mut *me;
        let iocp = me.iocp.as_ref().unwrap();
        match me.socket {
            Socket::Empty |
            Socket::Building(..) => {}

            Socket::Listener(ref l) => {
                match me.accept {
                    State::Empty => {}
                    _ => return
                }
                let accept_buf = &mut me.accept_buf;
                let res = match me.family {
                    Family::V4 => TcpBuilder::new_v4(),
                    Family::V6 => TcpBuilder::new_v6(),
                }.and_then(|builder| unsafe {
                    trace!("scheduling an accept");
                    l.accept_overlapped(&builder, accept_buf,
                                        self.inner.read.get_mut())
                });
                match res {
                    Ok((socket, _)) => {
                        // see docs above on Imp.inner for rationale on forget
                        me.accept = State::Pending(socket);
                        mem::forget(self.clone());
                    }
                    Err(e) => {
                        me.accept = State::Error(e);
                        iocp.defer(me.socket.handle(), EventSet::readable());
                    }
                }
            }

            Socket::Stream(ref s) => {
                match me.read {
                    State::Empty => {}
                    _ => return,
                }
                let mut buf = iocp.get_buffer(64 * 1024);
                let res = unsafe {
                    trace!("scheduling a read");
                    let cap = buf.capacity();
                    buf.set_len(cap);
                    s.read_overlapped(&mut buf, self.inner.read.get_mut())
                };
                match res {
                    Ok(_) => {
                        // see docs above on Imp.inner for rationale on forget
                        me.read = State::Pending(buf);
                        mem::forget(self.clone());
                    }
                    Err(e) => {
                        // Like above, be sure to indicate that hup has happened
                        // whenever we get `ECONNRESET`
                        let mut set = EventSet::readable();
                        if e.raw_os_error() == Some(WSAECONNRESET as i32) {
                            set = set | EventSet::hup();
                        }
                        me.read = State::Error(e);
                        iocp.defer(me.socket.handle(), set);
                        iocp.put_buffer(buf);
                    }
                }
            }
        }
    }

    /// Similar to `schedule_read`, except that this issues, well, writes.
    ///
    /// This function will continually attempt to write the entire contents of
    /// the buffer `buf` until they have all been written. The `pos` argument is
    /// the current offset within the buffer up to which the contents have
    /// already been written.
    ///
    /// A new writable event (e.g. allowing another write) will only happen once
    /// the buffer has been written completely (or hit an error).
    fn schedule_write(&self, buf: Vec<u8>, pos: usize) {
        trace!("scheduling a write");
        let mut me = self.inner();
        let err = match me.socket.stream() {
            Ok(s) => unsafe {
                s.write_overlapped(&buf[pos..], self.inner.write.get_mut())
            },
            Err(..) => return,
        };
        match err {
            Ok(_) => {
                // see docs above on Imp.inner for rationale on forget
                me.write = State::Pending((buf, pos));
                mem::forget(self.clone());
            }
            Err(e) => {
                me.write = State::Error(e);
                let iocp = me.iocp.as_ref().unwrap();
                iocp.defer(me.socket.handle(), EventSet::writable());
                iocp.put_buffer(buf);
            }
        }
    }
}

impl Socket {
    fn builder(&self) -> io::Result<&TcpBuilder> {
        match *self {
            Socket::Building(ref s) => Ok(s),
            _ => Err(bad_state()),
        }
    }

    fn listener(&self) -> io::Result<&TcpListener> {
        match *self {
            Socket::Listener(ref s) => Ok(s),
            _ => Err(bad_state()),
        }
    }

    fn stream(&self) -> io::Result<&TcpStream> {
        match *self {
            Socket::Stream(ref s) => Ok(s),
            _ => Err(bad_state()),
        }
    }

    fn handle(&self) -> HANDLE {
        match *self {
            Socket::Stream(ref s) => s.as_raw_socket() as HANDLE,
            Socket::Listener(ref l) => l.as_raw_socket() as HANDLE,
            Socket::Building(ref b) => b.as_raw_socket() as HANDLE,
            Socket::Empty => INVALID_HANDLE_VALUE,
        }
    }
}

fn read_done(status: &CompletionStatus, push: &mut FnMut(HANDLE, EventSet)) {
    let me2 = Imp {
        inner: unsafe { overlapped2arc!(status.overlapped(), Io, read) },
    };

    let mut me = me2.inner();
    match mem::replace(&mut me.accept, State::Empty) {
        State::Pending(s) => {
            trace!("finished an accept");
            me.accept = State::Ready(s);
            return push(me.socket.handle(), EventSet::readable())
        }
        s => me.accept = s,
    }

    match mem::replace(&mut me.read, State::Empty) {
        State::Pending(mut buf) => {
            trace!("finished a read: {}", status.bytes_transferred());
            unsafe {
                buf.set_len(status.bytes_transferred() as usize);
            }
            me.read = State::Ready(Cursor::new(buf));

            // If we transferred 0 bytes then be sure to indicate that hup
            // happened.
            let mut e = EventSet::readable();
            if status.bytes_transferred() == 0 {
                e = e | EventSet::hup();
            }
            return push(me.socket.handle(), e)
        }
        s => me.read = s,
    }

    // If neither an accept nor a read completed, then the connect must have
    // just finished.
    trace!("finished a connect");
    let handle = me.socket.handle();
    drop(me);
    me2.schedule_read();
    push(handle, EventSet::writable());

}

fn write_done(status: &CompletionStatus, push: &mut FnMut(HANDLE, EventSet)) {
    trace!("finished a write {}", status.bytes_transferred());
    let me2 = Imp {
        inner: unsafe { overlapped2arc!(status.overlapped(), Io, write) },
    };
    let mut me = me2.inner();
    let (buf, pos) = match mem::replace(&mut me.write, State::Empty) {
        State::Pending(pair) => pair,
        _ => unreachable!(),
    };
    let new_pos = pos + (status.bytes_transferred() as usize);
    if new_pos == buf.len() {
        push(me.socket.handle(), EventSet::writable());
    } else {
        drop(me);
        me2.schedule_write(buf, new_pos);
    }
}

fn addr_any(family: Family) -> SocketAddr {
    match family {
        Family::V4 => {
            let addr = SocketAddrV4::new(super::ipv4_any(), 0);
            SocketAddr::V4(addr)
        }
        Family::V6 => {
            let addr = SocketAddrV6::new(super::ipv6_any(), 0, 0, 0);
            SocketAddr::V6(addr)
        }
    }
}

impl Read for TcpSocket {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut me = self.inner();
        match mem::replace(&mut me.read, State::Empty) {
            State::Empty => Err(wouldblock()),
            State::Pending(buf) => {
                me.read = State::Pending(buf);
                Err(wouldblock())
            }
            State::Ready(mut cursor) => {
                let amt = try!(cursor.read(buf));
                // Once the entire buffer is written we need to schedule the
                // next read operation.
                if cursor.position() as usize == cursor.get_ref().len() {
                    me.iocp.as_ref().map(|s| s.put_buffer(cursor.into_inner()));
                    drop(me);
                    self.imp.schedule_read();
                } else {
                    me.read = State::Ready(cursor);
                }
                Ok(amt)
            }
            State::Error(e) => {
                drop(me);
                self.imp.schedule_read();
                Err(e)
            }
        }
    }
}

impl Write for TcpSocket {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut intermediate = {
            let mut me = self.inner();
            let me = &mut *me;
            match me.write {
                State::Empty => {}
                _ => return Err(wouldblock())
            }
            try!(me.socket.stream());
            match me.iocp {
                Some(ref s) => s.get_buffer(64 * 1024),
                None => return Err(wouldblock()),
            }
        };
        let amt = try!(intermediate.write(buf));
        self.imp.schedule_write(intermediate, 0);
        Ok(amt)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Evented for TcpSocket {
    fn register(&self, selector: &mut Selector, token: Token,
                interest: EventSet, opts: PollOpt) -> io::Result<()> {
        let mut me = self.inner();
        let selector = selector.inner();
        match me.socket {
            Socket::Stream(ref s) => {
                try!(selector.register_socket(s, token, interest, opts));
            }
            Socket::Listener(ref l) => {
                try!(selector.register_socket(l, token, interest, opts));
            }
            Socket::Building(ref b) => {
                try!(selector.register_socket(b, token, interest, opts));
            }
            Socket::Empty => return Err(bad_state()),
        }
        me.iocp = Some(selector.clone());

        // If we were connected before being registered process that request
        // here and go along our merry ways. Note that the callback for a
        // successful connect will worry about generating writable/readable
        // events and scheduling a new read.
        let addr = me.deferred_connect.take();
        drop(me);
        if let Some(addr) = addr {
            return self.connect(&addr).map(|_| ())
        }
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
        assert!(me.deferred_connect.is_none());
        match me.socket {
            Socket::Stream(ref s) => {
                try!(selector.reregister_socket(s, token, interest, opts));
            }
            Socket::Listener(ref l) => {
                try!(selector.reregister_socket(l, token, interest, opts));
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
            Socket::Stream(ref s) => selector.deregister_socket(s),
            Socket::Listener(ref l) => selector.deregister_socket(l),
            Socket::Building(ref b) => selector.deregister_socket(b),
            Socket::Empty => Err(bad_state()),
        }
    }
}

impl fmt::Debug for TcpSocket {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        "TcpSocket { ... }".fmt(f)
    }
}

impl Drop for TcpSocket {
    fn drop(&mut self) {
        // When the `TcpSocket` itself is dropped then we close the internal
        // handle (e.g. call `closesocket`). This will cause all pending I/O
        // operations to forcibly finish and we'll get notifications for all of
        // them and clean up the rest of our internal state (yay!)
        self.inner().socket = Socket::Empty;
    }
}
