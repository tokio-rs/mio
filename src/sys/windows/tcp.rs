use std::fmt;
use std::io::{self, Read, Write, Cursor};
use std::mem;
use std::net::{self, SocketAddr};
use std::os::windows::prelude::*;
use std::sync::{Mutex, MutexGuard};

use net2::{self, TcpBuilder};
use net::tcp::Shutdown;
use miow::iocp::CompletionStatus;
use miow::net::*;
use winapi::*;

use {Evented, EventSet, PollOpt, Selector, Token};
use event::IoEvent;
use sys::windows::selector::{Overlapped, Registration};
use sys::windows::{wouldblock, Family};
use sys::windows::from_raw_arc::FromRawArc;

pub struct TcpStream {
    /// Separately stored implementation to ensure that the `Drop`
    /// implementation on this type is only executed when it's actually dropped
    /// (many clones of this `imp` are made).
    imp: StreamImp,
}

pub struct TcpListener {
    imp: ListenerImp,
}

#[derive(Clone)]
struct StreamImp {
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
    inner: FromRawArc<StreamIo>,
}

#[derive(Clone)]
struct ListenerImp {
    inner: FromRawArc<ListenerIo>,
}

struct StreamIo {
    inner: Mutex<StreamInner>,
    read: Overlapped, // also used for connect
    write: Overlapped,
}

struct ListenerIo {
    inner: Mutex<ListenerInner>,
    accept: Overlapped,
}

struct StreamInner {
    socket: net::TcpStream,
    iocp: Registration,
    deferred_connect: Option<SocketAddr>,
    read: State<Vec<u8>, Cursor<Vec<u8>>>,
    write: State<(Vec<u8>, usize), (Vec<u8>, usize)>,
}

struct ListenerInner {
    socket: net::TcpListener,
    family: Family,
    iocp: Registration,
    accept: State<net::TcpStream, (net::TcpStream, SocketAddr)>,
    accept_buf: AcceptAddrsBuf,
}

enum State<T, U> {
    Empty,              // no I/O operation in progress
    Pending(T),         // an I/O operation is in progress
    Ready(U),           // I/O has finished with this value
    Error(io::Error),   // there was an I/O error
}

impl TcpStream {
    fn new(socket: net::TcpStream,
           deferred_connect: Option<SocketAddr>) -> TcpStream {
        TcpStream {
            imp: StreamImp {
                inner: FromRawArc::new(StreamIo {
                    read: Overlapped::new(read_done),
                    write: Overlapped::new(write_done),
                    inner: Mutex::new(StreamInner {
                        socket: socket,
                        iocp: Registration::new(),
                        deferred_connect: deferred_connect,
                        read: State::Empty,
                        write: State::Empty,
                    }),
                }),
            },
        }
    }

    pub fn connect(socket: net::TcpStream, addr: &SocketAddr)
                   -> io::Result<TcpStream> {
        Ok(TcpStream::new(socket, Some(*addr)))
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.inner().socket.peer_addr()
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner().socket.local_addr()
    }

    pub fn try_clone(&self) -> io::Result<TcpStream> {
        self.inner().socket.try_clone().map(|s| TcpStream::new(s, None))
    }

    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        self.inner().socket.shutdown(how)
    }

    pub fn set_nodelay(&self, nodelay: bool) -> io::Result<()> {
        net2::TcpStreamExt::set_nodelay(&self.inner().socket, nodelay)
    }

    pub fn set_keepalive(&self, seconds: Option<u32>) -> io::Result<()> {
        let dur = seconds.map(|s| s * 1000);
        net2::TcpStreamExt::set_keepalive_ms(&self.inner().socket, dur)
    }

    pub fn take_socket_error(&self) -> io::Result<()> {
        net2::TcpStreamExt::take_error(&self.inner().socket).and_then(|e| {
            match e {
                Some(e) => Err(e),
                None => Ok(())
            }
        })
    }

    fn inner(&self) -> MutexGuard<StreamInner> {
        self.imp.inner()
    }

    fn post_register(&self, interest: EventSet, me: &mut StreamInner) {
        if interest.is_readable() {
            self.imp.schedule_read(me);
        }

        // At least with epoll, if a socket is registered with an interest in
        // writing and it's immediately writable then a writable event is
        // generated immediately, so do so here.
        if interest.is_writable() {
            if let State::Empty = me.write {
                me.iocp.defer(EventSet::writable());
            }
        }
    }
}

impl StreamImp {
    fn inner(&self) -> MutexGuard<StreamInner> {
        self.inner.inner.lock().unwrap()
    }

    fn schedule_connect(&self, addr: &SocketAddr, me: &mut StreamInner)
                        -> io::Result<()> {
        unsafe {
            trace!("scheduling a connect");
            try!(me.socket.connect_overlapped(addr, self.inner.read.get_mut()));
        }
        // see docs above on StreamImp.inner for rationale on forget
        mem::forget(self.clone());
        Ok(())
    }

    /// Issues a "read" operation for this socket, if applicable.
    ///
    /// This is intended to be invoked from either a completion callback or a
    /// normal context. The function is infallible because errors are stored
    /// internally to be returned later.
    ///
    /// It is required that this function is only called after the handle has
    /// been registered with an event loop.
    fn schedule_read(&self, me: &mut StreamInner) {
        match me.read {
            State::Empty => {}
            _ => return,
        }

        me.iocp.unset_readiness(EventSet::readable());

        let mut buf = me.iocp.get_buffer(64 * 1024);
        let res = unsafe {
            trace!("scheduling a read");
            let cap = buf.capacity();
            buf.set_len(cap);
            me.socket.read_overlapped(&mut buf, self.inner.read.get_mut())
        };
        match res {
            Ok(_) => {
                // see docs above on StreamImp.inner for rationale on forget
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
                me.iocp.defer(set);
                me.iocp.put_buffer(buf);
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
    fn schedule_write(&self, buf: Vec<u8>, pos: usize,
                      me: &mut StreamInner) {

        // About to write, clear any pending level triggered events
        me.iocp.unset_readiness(EventSet::writable());

        trace!("scheduling a write");
        let err = unsafe {
            me.socket.write_overlapped(&buf[pos..], self.inner.write.get_mut())
        };
        match err {
            Ok(_) => {
                // see docs above on StreamImp.inner for rationale on forget
                me.write = State::Pending((buf, pos));
                mem::forget(self.clone());
            }
            Err(e) => {
                me.write = State::Error(e);
                me.iocp.defer(EventSet::writable());
                me.iocp.put_buffer(buf);
            }
        }
    }

    /// Pushes an event for this socket onto the selector its registered for.
    ///
    /// When an event is generated on this socket, if it happened after the
    /// socket was closed then we don't want to actually push the event onto our
    /// selector as otherwise it's just a spurious notification.
    fn push(&self, me: &mut StreamInner, set: EventSet,
            into: &mut Vec<IoEvent>) {
        if me.socket.as_raw_socket() != INVALID_SOCKET {
            me.iocp.push_event(set, into);
        }
    }
}

fn read_done(status: &CompletionStatus, dst: &mut Vec<IoEvent>) {
    let me2 = StreamImp {
        inner: unsafe { overlapped2arc!(status.overlapped(), StreamIo, read) },
    };

    let mut me = me2.inner();
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

            return me2.push(&mut me, e, dst)
        }
        s => me.read = s,
    }

    // If a read didn't complete, then the connect must have just finished.
    trace!("finished a connect");
    me2.push(&mut me, EventSet::writable(), dst);
    me2.schedule_read(&mut me);
}

fn write_done(status: &CompletionStatus, dst: &mut Vec<IoEvent>) {
    trace!("finished a write {}", status.bytes_transferred());
    let me2 = StreamImp {
        inner: unsafe { overlapped2arc!(status.overlapped(), StreamIo, write) },
    };
    let mut me = me2.inner();
    let (buf, pos) = match mem::replace(&mut me.write, State::Empty) {
        State::Pending(pair) => pair,
        _ => unreachable!(),
    };
    let new_pos = pos + (status.bytes_transferred() as usize);
    if new_pos == buf.len() {
        me2.push(&mut me, EventSet::writable(), dst);
    } else {
        me2.schedule_write(buf, new_pos, &mut me);
    }
}

impl Read for TcpStream {
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
                    me.iocp.put_buffer(cursor.into_inner());
                    self.imp.schedule_read(&mut me);
                } else {
                    me.read = State::Ready(cursor);
                }
                Ok(amt)
            }
            State::Error(e) => {
                self.imp.schedule_read(&mut me);
                Err(e)
            }
        }
    }
}

impl Write for TcpStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut me = self.inner();
        let me = &mut *me;

        match me.write {
            State::Empty => {}
            _ => return Err(wouldblock())
        }

        if me.iocp.port().is_none() {
            return Err(wouldblock())
        }

        let mut intermediate = me.iocp.get_buffer(64 * 1024);
        let amt = try!(intermediate.write(buf));
        self.imp.schedule_write(intermediate, 0, me);
        Ok(amt)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Evented for TcpStream {
    fn register(&self, selector: &mut Selector, token: Token,
                interest: EventSet, opts: PollOpt) -> io::Result<()> {
        let mut me = self.inner();
        let me = &mut *me;
        try!(me.iocp.register_socket(&me.socket, selector, token, interest,
                                     opts));

        // If we were connected before being registered process that request
        // here and go along our merry ways. Note that the callback for a
        // successful connect will worry about generating writable/readable
        // events and scheduling a new read.
        if let Some(addr) = me.deferred_connect.take() {
            return self.imp.schedule_connect(&addr, me).map(|_| ())
        }
        self.post_register(interest, me);
        Ok(())
    }

    fn reregister(&self, selector: &mut Selector, token: Token,
                  interest: EventSet, opts: PollOpt) -> io::Result<()> {
        let mut me = self.inner();
        {
            let me = &mut *me;
            try!(me.iocp.reregister_socket(&me.socket, selector, token,
                                           interest, opts));
        }
        self.post_register(interest, &mut me);
        Ok(())
    }

    fn deregister(&self, selector: &mut Selector) -> io::Result<()> {
        self.inner().iocp.checked_deregister(selector)
    }
}

impl fmt::Debug for TcpStream {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        "TcpStream { ... }".fmt(f)
    }
}

impl Drop for TcpStream {
    fn drop(&mut self) {
        let mut inner = self.inner();
        // When the `TcpSocket` itself is dropped then we close the internal
        // handle (e.g. call `closesocket`). This will cause all pending I/O
        // operations to forcibly finish and we'll get notifications for all of
        // them and clean up the rest of our internal state (yay!).
        //
        // This is achieved by replacing our socket with an invalid one, so all
        // further operations will return an error (but no further operations
        // should be done anyway).
        inner.socket = unsafe {
            net::TcpStream::from_raw_socket(INVALID_SOCKET)
        };

        // Then run any finalization code including level notifications
        inner.iocp.deregister();
    }
}

impl TcpListener {
    pub fn new(socket: net::TcpListener, addr: &SocketAddr)
               -> io::Result<TcpListener> {
        Ok(TcpListener::new_family(socket, match *addr {
            SocketAddr::V4(..) => Family::V4,
            SocketAddr::V6(..) => Family::V6,
        }))
    }

    fn new_family(socket: net::TcpListener, family: Family) -> TcpListener {
        TcpListener {
            imp: ListenerImp {
                inner: FromRawArc::new(ListenerIo {
                    accept: Overlapped::new(accept_done),
                    inner: Mutex::new(ListenerInner {
                        socket: socket,
                        iocp: Registration::new(),
                        accept: State::Empty,
                        accept_buf: AcceptAddrsBuf::new(),
                        family: family,
                    }),
                }),
            },
        }
    }

    pub fn accept(&self) -> io::Result<Option<(TcpStream, SocketAddr)>> {
        let mut me = self.inner();

        let ret = match mem::replace(&mut me.accept, State::Empty) {
            State::Empty => return Ok(None),
            State::Pending(t) => {
                me.accept = State::Pending(t);
                return Ok(None)
            }
            State::Ready((s, a)) => {
                Ok(Some((TcpStream::new(s, None), a)))
            }
            State::Error(e) => Err(e),
        };

        self.imp.schedule_accept(&mut me);

        return ret
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner().socket.local_addr()
    }

    pub fn try_clone(&self) -> io::Result<TcpListener> {
        let inner = self.inner();
        inner.socket.try_clone().map(|s| {
            TcpListener::new_family(s, inner.family)
        })
    }

    pub fn take_socket_error(&self) -> io::Result<()> {
        net2::TcpListenerExt::take_error(&self.inner().socket).and_then(|e| {
            match e {
                Some(e) => Err(e),
                None => Ok(())
            }
        })
    }

    fn inner(&self) -> MutexGuard<ListenerInner> {
        self.imp.inner()
    }
}

impl ListenerImp {
    fn inner(&self) -> MutexGuard<ListenerInner> {
        self.inner.inner.lock().unwrap()
    }

    fn schedule_accept(&self, me: &mut ListenerInner) {
        match me.accept {
            State::Empty => {}
            _ => return
        }

        me.iocp.unset_readiness(EventSet::readable());

        let res = match me.family {
            Family::V4 => TcpBuilder::new_v4(),
            Family::V6 => TcpBuilder::new_v6(),
        }.and_then(|builder| unsafe {
            trace!("scheduling an accept");
            me.socket.accept_overlapped(&builder, &mut me.accept_buf,
                                        self.inner.accept.get_mut())
        });
        match res {
            Ok((socket, _)) => {
                // see docs above on StreamImp.inner for rationale on forget
                me.accept = State::Pending(socket);
                mem::forget(self.clone());
            }
            Err(e) => {
                me.accept = State::Error(e);
                me.iocp.defer(EventSet::readable());
            }
        }
    }

    // See comments in StreamImp::push
    fn push(&self, me: &mut ListenerInner, set: EventSet,
            into: &mut Vec<IoEvent>) {
        if me.socket.as_raw_socket() != INVALID_SOCKET {
            me.iocp.push_event(set, into);
        }
    }
}

fn accept_done(status: &CompletionStatus, dst: &mut Vec<IoEvent>) {
    let me2 = ListenerImp {
        inner: unsafe { overlapped2arc!(status.overlapped(), ListenerIo, accept) },
    };

    let mut me = me2.inner();
    let socket = match mem::replace(&mut me.accept, State::Empty) {
        State::Pending(s) => s,
        _ => unreachable!(),
    };
    trace!("finished an accept");
    me.accept = match me.accept_buf.parse(&me.socket) {
        Ok(buf) => State::Ready((socket, buf.remote().unwrap())),
        Err(e) => State::Error(e),
    };
    me2.push(&mut me, EventSet::readable(), dst);
}

impl Evented for TcpListener {
    fn register(&self, selector: &mut Selector, token: Token,
                interest: EventSet, opts: PollOpt) -> io::Result<()> {
        let mut me = self.inner();
        let me = &mut *me;
        try!(me.iocp.register_socket(&me.socket, selector, token, interest,
                                     opts));
        self.imp.schedule_accept(me);
        Ok(())
    }

    fn reregister(&self, selector: &mut Selector, token: Token,
                  interest: EventSet, opts: PollOpt) -> io::Result<()> {
        let mut me = self.inner();
        let me = &mut *me;
        try!(me.iocp.reregister_socket(&me.socket, selector, token,
                                       interest, opts));
        self.imp.schedule_accept(me);
        Ok(())
    }

    fn deregister(&self, selector: &mut Selector) -> io::Result<()> {
        self.inner().iocp.checked_deregister(selector)
    }
}

impl fmt::Debug for TcpListener {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        "TcpListener { ... }".fmt(f)
    }
}

impl Drop for TcpListener {
    fn drop(&mut self) {
        let mut inner = self.inner();

        // See comments in TcpStream
        inner.socket = unsafe {
            net::TcpListener::from_raw_socket(INVALID_SOCKET)
        };

        // Then run any finalization code including level notifications
        inner.iocp.deregister();
    }
}
