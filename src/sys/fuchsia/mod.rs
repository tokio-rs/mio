#![allow(warnings)]

use {io, poll, Event, Evented, Ready, Registration, Poll, PollOpt, Token};
use concurrent_hashmap::ConcHashMap;
use iovec::IoVec;
use iovec::unix as iovec;
use libc;
use magenta;
use magenta_sys;
use magenta::HandleBase;
use net2::TcpStreamExt;
use std::collections::hash_map::RandomState;
use std::cmp;
use std::fmt;
use std::io::{Read, Write};
use std::mem;
use std::net::{self, IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::ops::{Deref, DerefMut};
use std::os::unix::io::{AsRawFd, RawFd};
use std::sync::atomic::{AtomicBool, AtomicUsize, ATOMIC_USIZE_INIT, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

pub struct Awakener {
    /// Token and weak reference to the port on which Awakener was registered.
    ///
    /// When `Awakener::wakeup` is called, these are used to send a wakeup message to the port.
    inner: Mutex<Option<(Token, Weak<magenta::Port>)>>,
}

impl Awakener {
    /// Create a new `Awakener`.
    pub fn new() -> io::Result<Awakener> {
        Ok(Awakener {
            inner: Mutex::new(None)
        })
    }

    /// Send a wakeup signal to the `Selector` on which the `Awakener` was registered.
    pub fn wakeup(&self) -> io::Result<()> {
        let inner_locked = self.inner.lock().unwrap();
        let &(token, ref weak_port) =
            inner_locked.as_ref().expect("Called wakeup on unregistered awakener.");

        let port = weak_port.upgrade().expect("Tried to wakeup a closed port.");

        let status = 0; // arbitrary
        let packet = magenta::Packet::from_user_packet(
                        token.0 as u64, status, magenta::UserPacket::from_u8_array([0; 32]));

        port.queue(&packet).map_err(status_to_io_err)
    }

    pub fn cleanup(&self) {}
}

impl Evented for Awakener {
    fn register(&self,
                poll: &Poll,
                token: Token,
                _events: Ready,
                opts: PollOpt) -> io::Result<()>
    {
        let mut inner_locked = self.inner.lock().unwrap();
        if inner_locked.is_some() {
            panic!("Called register on already-registered Awakener.");
        }
        *inner_locked = Some((token, Arc::downgrade(&poll::selector(poll).port)));

        Ok(())
    }

    fn reregister(&self,
                  poll: &Poll,
                  token: Token,
                  _events: Ready,
                  opts: PollOpt) -> io::Result<()>
    {
        let mut inner_locked = self.inner.lock().unwrap();
        *inner_locked = Some((token, Arc::downgrade(&poll::selector(poll).port)));

        Ok(())
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()>
    {
        let mut inner_locked = self.inner.lock().unwrap();
        *inner_locked = None;

        Ok(())
    }
}

pub struct Events {
    /// The Fuchsia selector only handles one event at a time, so there's no reason to
    /// provide storage for multiple events.
    event_opt: Option<Event>
}

impl Events {
    pub fn with_capacity(_u: usize) -> Events { Events { event_opt: None } }
    pub fn len(&self) -> usize {
        if self.event_opt.is_some() { 1 } else { 0 }
    }
    pub fn capacity(&self) -> usize {
        1
    }
    pub fn is_empty(&self) -> bool {
        self.event_opt.is_none()
    }
    pub fn get(&self, idx: usize) -> Option<Event> {
        if idx == 0 { self.event_opt } else { None }
    }
    pub fn push_event(&mut self, event: Event) {
        assert!(::std::mem::replace(&mut self.event_opt, Some(event)).is_none(),
            "Only one event at a time can be pushed to Fuchsia `Events`.");
    }
}
impl fmt::Debug for Events {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Events {{ len: {} }}", self.len())
    }
}


/// Each Selector has a globally unique(ish) ID associated with it. This ID
/// gets tracked by `TcpStream`, `TcpListener`, etc... when they are first
/// registered with the `Selector`. If a type that is previously associated with
/// a `Selector` attempts to register itself with a different `Selector`, the
/// operation will return with an error. This matches windows behavior.
static NEXT_ID: AtomicUsize = ATOMIC_USIZE_INIT;

pub struct Selector {
    id: usize,

    /// Magenta object on which the handles have been registered, and on which events occur
    port: Arc<magenta::Port>,

    /// Whether or not `tokens_to_rereg` contains any elements. This is a best-effort attempt
    /// used to prevent having to lock `tokens_to_rereg` when it is empty.
    has_tokens_to_rereg: AtomicBool,

    /// List of `Token`s corresponding to registrations that need to be reregistered before the
    /// next `port::wait`. This is necessary to provide level-triggered behavior for
    /// `Async::repeating` registrations.
    ///
    /// When a level-triggered `Async::repeating` event is seen, its token is added to this list so
    /// that it will be reregistered before the next `port::wait` call, making `port::wait` return
    /// immediately if the signal was high during the reregistration.
    tokens_to_rereg: Mutex<Vec<Token>>,

    /// Map from tokens to weak references to `EventedFdInner`-- a structure describing a
    /// file handle, its associated `mxio` object, and its current registration.
    token_to_fd: ConcHashMap<Token, Weak<EventedFdInner>>,
}

impl Selector {
    pub fn new() -> io::Result<Selector> {
        let port = Arc::new(
            magenta::Port::create(magenta::PortOpts::V2)
                .map_err(status_to_io_err)?
        );

        // offset by 1 to avoid choosing 0 as the id of a selector
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed) + 1;

        let has_tokens_to_rereg = AtomicBool::new(false);
        let tokens_to_rereg = Mutex::new(Vec::new());
        let token_to_fd = ConcHashMap::<_, _, RandomState>::new();

        Ok(Selector {
            id,
            port,
            has_tokens_to_rereg,
            tokens_to_rereg,
            token_to_fd,
        })
    }

    pub fn id(&self) -> usize {
        self.id
    }

    /// Reregisters all registrations pointed to by the `tokens_to_rereg` list
    /// if `has_tokens_to_rereg`.
    fn reregister_handles(&self) -> io::Result<()> {
        if self.has_tokens_to_rereg.load(Ordering::Relaxed) {
            let mut tokens = self.tokens_to_rereg.lock().unwrap();
            for token in tokens.drain(0..) {
                if let Some(eventedfd) = self.token_to_fd.find(&token)
                                        .and_then(|h| h.get().upgrade()) {
                    eventedfd.rereg_for_level(&self.port);
                }
            }
            self.has_tokens_to_rereg.store(false, Ordering::Relaxed);
        }
        Ok(())
    }

    pub fn select(&self,
                  evts: &mut Events,
                  _awakener: Token,
                  timeout: Option<Duration>) -> io::Result<bool>
    {
        evts.event_opt = None;

        self.reregister_handles()?;

        let deadline = match timeout {
            Some(duration) => {
                let nanos = (duration.as_secs() * 1_000_000_000) +
                    (duration.subsec_nanos() as u64);

                magenta::deadline_after(nanos)
            },
            None => magenta::MX_TIME_INFINITE,
        };

        let packet = match self.port.wait(deadline) {
            Ok(packet) => packet,
            Err(magenta::Status::ErrTimedOut) => return Ok(false),
            Err(e) => return Err(status_to_io_err(e)),
        };

        let observed_signals = match packet.contents() {
            magenta::PacketContents::SignalOne(signal_packet) => {
                signal_packet.observed()
            },
            magenta::PacketContents::SignalRep(signal_packet) => {
                signal_packet.observed()
            },
            magenta::PacketContents::User(_user_packet) => {
                // User packets are only ever sent by an Awakener
                return Ok(true);
            },
        };

        let key = packet.key();
        let token = Token(key as usize);

        // Convert the signals to epoll events using __mxio_wait_end, and add to reregistration list
        // if necessary.
        let events: u32;
        {
            let handle = if let Some(handle) =
                self.token_to_fd
                    .find(&Token(key as usize))
                    .and_then(|h| h.get().upgrade()) {
                handle
            } else {
                // This handle is apparently in the process of removal-- it has been removed from
                // the list, but port_cancel has not yet been called
                return Ok(false);
            };

            events = unsafe {
                let mut events: u32 = mem::uninitialized();
                sys::__mxio_wait_end(handle.mxio, observed_signals, &mut events);
                events
            };

            // If necessary, queue to be reregistered before next port_await
            let needs_to_rereg = {
                let registration_lock = handle.registration.lock().unwrap();

                registration_lock
                    .as_ref()
                    .map(|r| &r.rereg_signals)
                    .is_some()
            };

            if needs_to_rereg {
                let mut tokens_to_rereg_lock = self.tokens_to_rereg.lock().unwrap();
                tokens_to_rereg_lock.push(token);
                self.has_tokens_to_rereg.store(true, Ordering::Relaxed);
            }
        }

        evts.event_opt = Some(Event::new(epoll_event_to_ready(events), token));

        Ok(false)
    }

    /// Register event interests for the given IO handle with the OS
    pub fn register(&self,
                    handle: &magenta::Handle,
                    fd: &EventedFd,
                    token: Token,
                    signals: magenta::Signals,
                    poll_opts: PollOpt)-> io::Result<()>
    {
        self.token_to_fd.insert(token, Arc::downgrade(&fd.inner));

        let wait_async_opts = poll_opts_to_wait_async(poll_opts);

        let wait_res = handle.wait_async(&self.port, token.0 as u64, signals, wait_async_opts)
            .map_err(status_to_io_err);

        if wait_res.is_err() {
            self.token_to_fd.remove(&token);
        }

        wait_res
    }

    /// Deregister event interests for the given IO handle with the OS
    pub fn deregister(&self, handle: &magenta::Handle, token: Token) -> io::Result<()> {
        self.token_to_fd.remove(&token);
        self.port.cancel(&*handle, token.0 as u64)
            .map_err(status_to_io_err)
    }
}

#[derive(Debug)]
pub struct TcpStream {
    io: DontDrop<net::TcpStream>,
    evented_fd: EventedFd,
}

impl TcpStream {
    pub fn connect(stream: net::TcpStream, addr: &SocketAddr) -> io::Result<TcpStream> {
        try!(set_nonblock(stream.as_raw_fd()));

        match stream.connect(addr) {
            Ok(..) => {}
            Err(ref e) if e.raw_os_error() == Some(libc::EINPROGRESS) => {}
            Err(e) => return Err(e),
        }

        let evented_fd = unsafe { EventedFd::new(stream.as_raw_fd()) };

        Ok(TcpStream {
            io: DontDrop::new(stream),
            evented_fd: evented_fd,
        })
    }

   pub fn from_stream(stream: net::TcpStream) -> TcpStream {
       let evented_fd = unsafe { EventedFd::new(stream.as_raw_fd()) };

        TcpStream {
            io: DontDrop::new(stream),
            evented_fd: evented_fd,
        }
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.io.peer_addr()
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.io.local_addr()
    }

    pub fn try_clone(&self) -> io::Result<TcpStream> {
        self.io.try_clone().map(|s| {
            let evented_fd = unsafe { EventedFd::new(s.as_raw_fd()) };
            TcpStream {
                io: DontDrop::new(s),
                evented_fd: evented_fd,
            }
        })
    }

    pub fn shutdown(&self, how: net::Shutdown) -> io::Result<()> {
        self.io.shutdown(how)
    }

    pub fn set_nodelay(&self, nodelay: bool) -> io::Result<()> {
        self.io.set_nodelay(nodelay)
    }

    pub fn nodelay(&self) -> io::Result<bool> {
        self.io.nodelay()
    }

    pub fn set_recv_buffer_size(&self, size: usize) -> io::Result<()> {
        self.io.set_recv_buffer_size(size)
    }

    pub fn recv_buffer_size(&self) -> io::Result<usize> {
        self.io.recv_buffer_size()
    }

    pub fn set_send_buffer_size(&self, size: usize) -> io::Result<()> {
        self.io.set_send_buffer_size(size)
    }

    pub fn send_buffer_size(&self) -> io::Result<usize> {
        self.io.send_buffer_size()
    }

    pub fn set_keepalive(&self, keepalive: Option<Duration>) -> io::Result<()> {
        self.io.set_keepalive(keepalive)
    }

    pub fn keepalive(&self) -> io::Result<Option<Duration>> {
        self.io.keepalive()
    }

    pub fn set_ttl(&self, ttl: u32) -> io::Result<()> {
        self.io.set_ttl(ttl)
    }

    pub fn ttl(&self) -> io::Result<u32> {
        self.io.ttl()
    }

    pub fn set_only_v6(&self, only_v6: bool) -> io::Result<()> {
        self.io.set_only_v6(only_v6)
    }

    pub fn only_v6(&self) -> io::Result<bool> {
        self.io.only_v6()
    }

    pub fn set_linger(&self, dur: Option<Duration>) -> io::Result<()> {
        self.io.set_linger(dur)
    }

    pub fn linger(&self) -> io::Result<Option<Duration>> {
        self.io.linger()
    }

    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.io.take_error()
    }

    pub fn readv(&self, bufs: &mut [&mut IoVec]) -> io::Result<usize> {
        unsafe {
            let slice = iovec::as_os_slice_mut(bufs);
            let len = cmp::min(<libc::c_int>::max_value() as usize, slice.len());
            let rc = libc::readv(self.io.as_raw_fd(),
                                 slice.as_ptr(),
                                 len as libc::c_int);
            if rc < 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(rc as usize)
            }
        }
    }

    pub fn writev(&self, bufs: &[&IoVec]) -> io::Result<usize> {
        unsafe {
            let slice = iovec::as_os_slice(bufs);
            let len = cmp::min(<libc::c_int>::max_value() as usize, slice.len());
            let rc = libc::writev(self.io.as_raw_fd(),
                                  slice.as_ptr(),
                                  len as libc::c_int);
            if rc < 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(rc as usize)
            }
        }
    }
}

impl<'a> Read for &'a TcpStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.io.inner_ref().read(buf)
    }
}

impl<'a> Write for &'a TcpStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.io.inner_ref().write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.io.inner_ref().flush()
    }
}

impl Evented for TcpStream {
    fn register(&self,
                poll: &Poll,
                token: Token,
                interest: Ready,
                opts: PollOpt) -> io::Result<()>
    {
        self.evented_fd.register(poll, token, interest, opts)
    }

    fn reregister(&self,
                  poll: &Poll,
                  token: Token,
                  interest: Ready,
                  opts: PollOpt) -> io::Result<()>
    {
        self.evented_fd.reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        self.evented_fd.deregister(poll)
    }
}

#[derive(Debug)]
pub struct TcpListener {
    io: DontDrop<net::TcpListener>,
    evented_fd: EventedFd,
}

impl TcpListener {
    pub fn new(inner: net::TcpListener, _addr: &SocketAddr) -> io::Result<TcpListener> {
        // TODO: replace
        try!(cvt(unsafe { libc::fcntl(inner.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK) }));

        let evented_fd = unsafe { EventedFd::new(inner.as_raw_fd()) };

        Ok(TcpListener {
            io: DontDrop::new(inner),
            evented_fd: evented_fd,
        })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.io.local_addr()
    }

    pub fn try_clone(&self) -> io::Result<TcpListener> {
        self.io.try_clone().map(|io| {
            let evented_fd = unsafe { EventedFd::new(io.as_raw_fd()) };
            TcpListener {
                io: DontDrop::new(io),
                evented_fd: evented_fd,
            }
        })
    }

    pub fn accept(&self) -> io::Result<(TcpStream, SocketAddr)> {
        self.io.accept().and_then(|(s, a)| {
            set_nonblock(s.as_raw_fd())?;
            let evented_fd = unsafe { EventedFd::new(s.as_raw_fd()) };
            Ok((TcpStream {
                io: DontDrop::new(s),
                evented_fd: evented_fd,
            }, a))
        })
    }

    #[allow(deprecated)]
    pub fn set_only_v6(&self, only_v6: bool) -> io::Result<()> {
        self.io.set_only_v6(only_v6)
    }

    #[allow(deprecated)]
    pub fn only_v6(&self) -> io::Result<bool> {
        self.io.only_v6()
    }

    pub fn set_ttl(&self, ttl: u32) -> io::Result<()> {
        self.io.set_ttl(ttl)
    }

    pub fn ttl(&self) -> io::Result<u32> {
        self.io.ttl()
    }

    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.io.take_error()
    }
}

impl Evented for TcpListener {
    fn register(&self,
                poll: &Poll,
                token: Token,
                interest: Ready,
                opts: PollOpt) -> io::Result<()>
    {
        self.evented_fd.register(poll, token, interest, opts)
    }

    fn reregister(&self,
                  poll: &Poll,
                  token: Token,
                  interest: Ready,
                  opts: PollOpt) -> io::Result<()>
    {
        self.evented_fd.reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        self.evented_fd.deregister(poll)
    }
}

#[derive(Debug)]
pub struct UdpSocket {
    io: DontDrop<net::UdpSocket>,
    evented_fd: EventedFd,
}

impl UdpSocket {
    pub fn new(socket: net::UdpSocket) -> io::Result<UdpSocket> {
        // Set non-blocking (workaround since the std version doesn't work due to a temporary bug in fuchsia-- TODO replace this
        set_nonblock(socket.as_raw_fd())?;

        let evented_fd = unsafe { EventedFd::new(socket.as_raw_fd()) };

        Ok(UdpSocket {
            io: DontDrop::new(socket),
            evented_fd: evented_fd,
        })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.io.local_addr()
    }

    pub fn try_clone(&self) -> io::Result<UdpSocket> {
        self.io.try_clone().and_then(|io| {
            UdpSocket::new(io)
        })
    }

    pub fn send_to(&self, buf: &[u8], target: &SocketAddr) -> io::Result<usize> {
        self.io.send_to(buf, target)
    }

    pub fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        recv_from(unsafe { self.io.as_raw_fd() }, buf)
    }

    pub fn send(&self, buf: &[u8]) -> io::Result<usize> {
        self.io.send(buf)
    }

    pub fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        self.io.recv(buf)
    }

    pub fn connect(&self, addr: SocketAddr)
                     -> io::Result<()> {
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

    pub fn join_multicast_v4(&self,
                             multiaddr: &Ipv4Addr,
                             interface: &Ipv4Addr) -> io::Result<()> {
        self.io.join_multicast_v4(multiaddr, interface)
    }

    pub fn join_multicast_v6(&self,
                             multiaddr: &Ipv6Addr,
                             interface: u32) -> io::Result<()> {
        self.io.join_multicast_v6(multiaddr, interface)
    }

    pub fn leave_multicast_v4(&self,
                              multiaddr: &Ipv4Addr,
                              interface: &Ipv4Addr) -> io::Result<()> {
        self.io.leave_multicast_v4(multiaddr, interface)
    }

    pub fn leave_multicast_v6(&self,
                              multiaddr: &Ipv6Addr,
                              interface: u32) -> io::Result<()> {
        self.io.leave_multicast_v6(multiaddr, interface)
    }

    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.io.take_error()
    }
}

impl Evented for UdpSocket {
    fn register(&self,
                poll: &Poll,
                token: Token,
                interest: Ready,
                opts: PollOpt) -> io::Result<()>
    {
        self.evented_fd.register(poll, token, interest, opts)
    }

    fn reregister(&self,
                  poll: &Poll,
                  token: Token,
                  interest: Ready,
                  opts: PollOpt) -> io::Result<()>
    {
        self.evented_fd.reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        self.evented_fd.deregister(poll)
    }
}

/// Properties of an `EventedFd`'s current registration
#[derive(Debug)]
struct EventedFdRegistration {
    token: Token,
    handle: DontDrop<magenta::Handle>,
    rereg_signals: Option<(magenta::Signals, magenta::WaitAsyncOpts)>,
}

/// An event-ed file descriptor. The file descriptor is owned by this structure.
#[derive(Debug)]
struct EventedFdInner {
    /// Properties of the current registration.
    registration: Mutex<Option<EventedFdRegistration>>,

    /// Owned file descriptor.
    fd: RawFd,

    /// Owned `mxio_t` ponter.
    mxio: *const sys::mxio_t,
}

impl EventedFdInner {
   pub fn rereg_for_level(&self, port: &magenta::Port) {
       let registration_opt = self.registration.lock().unwrap();
       if let Some(ref registration) = *registration_opt {
           if let Some((rereg_signals, rereg_opts)) = registration.rereg_signals {
               let _res =
                   registration
                       .handle.inner_ref()
                       .wait_async(port,
                                   registration.token.0 as u64,
                                   rereg_signals,
                                   rereg_opts);
           }
       }
   }
}

impl Drop for EventedFdInner {
    fn drop(&mut self) {
        unsafe {
            sys::__mxio_release(self.mxio);
            let _ = libc::close(self.fd);
        }
    }
}

// `EventedInner` must be manually declared `Send + Sync` because it contains a `RawFd` and a
// `*const sys::mxio_t`. These are only used to make thread-safe system calls, so accessing
// them is entirely thread-safe.
//
// Note: one minor exception to this are the calls to `libc::close` and `__mxio_release`, which
// happen on `Drop`. These accesses are safe because `drop` can only be called at most once from
// a single thread, and after it is called no other functions can be called on the `EventedFdInner`.
unsafe impl Sync for EventedFdInner {}
unsafe impl Send for EventedFdInner {}

#[derive(Clone, Debug)]
struct EventedFd {
    pub inner: Arc<EventedFdInner>
}

impl EventedFd {
    unsafe fn new(fd: RawFd) -> Self {
        let mxio = sys::__mxio_fd_to_io(fd);
        assert!(mxio != ::std::ptr::null(), "FileDescriptor given to EventedFd must be valid.");

        EventedFd {
            inner: Arc::new(EventedFdInner {
                registration: Mutex::new(None),
                fd: fd,
                mxio: mxio,
            })
        }
    }
}

impl Evented for EventedFd {
     fn register(&self,
                poll: &Poll,
                token: Token,
                interest: Ready,
                opts: PollOpt) -> io::Result<()>
    {
        let epoll_events = ioevent_to_epoll(interest, opts);

        let (handle, raw_handle, signals) = unsafe {
            let mut raw_handle: sys::mx_handle_t = mem::uninitialized();
            let mut signals: sys::mx_signals_t = mem::uninitialized();
            sys::__mxio_wait_begin(self.inner.mxio, epoll_events, &mut raw_handle, &mut signals);

            // We don't have ownership of the handle, so we can't drop it
            let handle = DontDrop::new(magenta::Handle::from_raw(raw_handle));
            (handle, raw_handle, signals)
        };


        let needs_rereg = opts.is_level() && !opts.is_oneshot();

        {
            let mut registration_lock = self.inner.registration.lock().unwrap();
            if registration_lock.is_some() {
                panic!("Called register on an already registered file descriptor.");
            }
            *registration_lock = Some(EventedFdRegistration {
                token: token,
                handle: DontDrop::new(unsafe { magenta::Handle::from_raw(raw_handle) }),
                rereg_signals: if needs_rereg {
                    Some((signals, poll_opts_to_wait_async(opts)))
                } else {
                    None
                },
            })
        }

        let registered = poll::selector(poll)
            .register(handle.inner_ref(), self, token, signals, opts);

        if registered.is_err() {
            let mut registration_lock = self.inner.registration.lock().unwrap();
            *registration_lock = None;
        }

        registered
    }

    fn reregister(&self,
                  poll: &Poll,
                  token: Token,
                  interest: Ready,
                  opts: PollOpt) -> io::Result<()>
    {
        self.deregister(poll)?;
        self.register(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        let mut registration_lock = self.inner.registration.lock().unwrap();
        let old_registration = registration_lock.take()
            .expect("Tried to deregister on unregistered handle.");

        poll::selector(poll)
            .deregister(old_registration.handle.inner_ref(), old_registration.token)
    }
}

mod sys {
    #![allow(non_camel_case_types)]
    use libc;
    use std::os::unix::io::RawFd;
    pub use magenta_sys::{mx_handle_t, mx_signals_t};

    // 17 fn pointers we don't need for mio :)
    pub type mxio_ops_t = [usize; 17];

    pub type atomic_int_fast32_t = usize; // TODO: https://github.com/rust-lang/libc/issues/631

    #[repr(C)]
    pub struct mxio_t {
        pub ops: *const mxio_ops_t,
        pub magic: u32,
        pub refcount: atomic_int_fast32_t,
        pub dupcount: u32,
        pub flags: u32,
    }

    #[link(name="mxio")]
    extern {
        pub fn __mxio_fd_to_io(fd: RawFd) -> *const mxio_t;
        pub fn __mxio_release(io: *const mxio_t);

        pub fn __mxio_wait_begin(
            io: *const mxio_t,
            events: u32,
            handle_out: &mut mx_handle_t,
            signals_out: &mut mx_signals_t,
        );
        pub fn __mxio_wait_end(
            io: *const mxio_t,
            signals: mx_signals_t,
            events_out: &mut u32,
        );
    }
}

/*
// Unix only:
EventedFd
    register
    reregister
    deregister
pipe -> ::io::Result<(Io, Io)>
set_nonblock(fd: libc::c_int)
Io
    pub fn try_clone(&self) -> io::Result<Io>
    From FromRawFd:
        unsafe fn from_raw_fd(fd: RawFd) -> Io
    From IntoRawFd:
        fn into_raw_fd(self) -> RawFd
    From AsRawFd:
        fn as_raw_fd(&self) -> RawFd
    From Evented:
        register
        reregeister
        deregister
    From Read:
        fn read(&mut self, dst: &mut [u8]) -> io:Result<usize>
    From Write:
        fn write(&mut self, src: &[u8]) -> io::Result<usize>
        fn flush(&mut self) -> io::Result<()>

// Windows only:
Overlapped
Binding
*/

/// Utility type to prevent the type inside of it from being dropped.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
struct DontDrop<T>(Option<T>);

impl<T> DontDrop<T> {
    fn new(t: T) -> DontDrop<T> {
        DontDrop(Some(t))
    }

    fn inner_ref(&self) -> &T {
        self.0.as_ref().unwrap()
    }

    fn inner_mut(&mut self) -> &mut T {
        self.0.as_mut().unwrap()
    }
}

impl<T> Deref for DontDrop<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.inner_ref()
    }
}

impl<T> DerefMut for DontDrop<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner_mut()
    }
}

impl<T> Drop for DontDrop<T> {
    fn drop(&mut self) {
        let inner = self.0.take();
        mem::forget(inner);
    }
}

/// Convert from magenta::Status to io::Error.
///
/// Note: these conversions are done on a "best-effort" basis and may not necessarily reflect
/// exactly equivalent error types.
fn status_to_io_err(status: magenta::Status) -> io::Error {
    use magenta::Status;

    match status {
        Status::ErrInterruptedRetry => io::ErrorKind::Interrupted,
        Status::ErrBadHandle => io::ErrorKind::BrokenPipe,
        Status::ErrTimedOut => io::ErrorKind::TimedOut,
        Status::ErrShouldWait => io::ErrorKind::WouldBlock,
        Status::ErrPeerClosed => io::ErrorKind::ConnectionAborted,
        Status::ErrNotFound => io::ErrorKind::NotFound,
        Status::ErrAlreadyExists => io::ErrorKind::AlreadyExists,
        Status::ErrAlreadyBound => io::ErrorKind::AddrInUse,
        Status::ErrUnavailable => io::ErrorKind::AddrNotAvailable,
        Status::ErrAccessDenied => io::ErrorKind::PermissionDenied,
        Status::ErrIoRefused => io::ErrorKind::ConnectionRefused,
        Status::ErrIoDataIntegrity => io::ErrorKind::InvalidData,

        Status::ErrBadPath |
        Status::ErrInvalidArgs |
        Status::ErrOutOfRange |
        Status::ErrWrongType => io::ErrorKind::InvalidInput,

        Status::UnknownOther |
        Status::ErrNext |
        Status::ErrStop |
        Status::ErrNoSpace |
        Status::ErrFileBig |
        Status::ErrNotFile |
        Status::ErrNotDir |
        Status::ErrIoDataLoss |
        Status::ErrIo |
        Status::ErrCanceled |
        Status::ErrBadState |
        Status::ErrBufferTooSmall |
        Status::ErrBadSyscall |
        Status::NoError |
        Status::ErrInternal |
        Status::ErrNotSupported |
        Status::ErrNoResources |
        Status::ErrNoMemory |
        Status::ErrCallFailed
        => io::ErrorKind::Other
    }.into()
}


/// Workaround until fuchsia's recv_from is fixed
fn recv_from(fd: RawFd, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
    let flags = 0;

    let n = cvt(unsafe {
        libc::recv(fd,
                   buf.as_mut_ptr() as *mut libc::c_void,
                   buf.len(),
                   flags)
    })?;

    // random address-- we don't use it
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
    Ok((n as usize, addr))
}

fn set_nonblock(fd: RawFd) -> io::Result<()> {
    cvt(unsafe { libc::fcntl(fd, libc::F_SETFL, libc::O_NONBLOCK) }).map(|_| ())
}

// Everything below is copied from sys::unix:

fn ioevent_to_epoll(interest: Ready, opts: PollOpt) -> u32 {
    use event_imp::ready_from_usize;
    const HUP: usize   = 0b01000;

    let mut kind = 0;

    if interest.is_readable() {
        kind |= libc::EPOLLIN;
    }

    if interest.is_writable() {
        kind |= libc::EPOLLOUT;
    }

    if interest.contains(ready_from_usize(HUP)) {
        kind |= libc::EPOLLRDHUP;
    }

    if opts.is_edge() {
        kind |= libc::EPOLLET;
    }

    if opts.is_oneshot() {
        kind |= libc::EPOLLONESHOT;
    }

    if opts.is_level() {
        kind &= !libc::EPOLLET;
    }

    kind as u32
}

fn epoll_event_to_ready(epoll: u32) -> Ready {
    let epoll = epoll as i32; // casts the bits directly
    let mut kind = Ready::empty();

    if (epoll & libc::EPOLLIN) != 0 || (epoll & libc::EPOLLPRI) != 0 {
        kind = kind | Ready::readable();
    }

    if (epoll & libc::EPOLLOUT) != 0 {
        kind = kind | Ready::writable();
    }

    kind

    /* TODO:: support?
    // EPOLLHUP - Usually means a socket error happened
    if (epoll & libc::EPOLLERR) != 0 {
        kind = kind | UnixReady::error();
    }

    if (epoll & libc::EPOLLRDHUP) != 0 || (epoll & libc::EPOLLHUP) != 0 {
        kind = kind | UnixReady::hup();
    }
    */
}

fn poll_opts_to_wait_async(poll_opts: PollOpt) -> magenta::WaitAsyncOpts {
    if poll_opts.is_oneshot() {
        magenta::WaitAsyncOpts::Once
    } else {
        magenta::WaitAsyncOpts::Repeating
    }
}

trait IsMinusOne {
    fn is_minus_one(&self) -> bool;
}

impl IsMinusOne for i32 {
    fn is_minus_one(&self) -> bool { *self == -1 }
}

impl IsMinusOne for isize {
    fn is_minus_one(&self) -> bool { *self == -1 }
}

fn cvt<T: IsMinusOne>(t: T) -> ::io::Result<T> {
    use std::io;

    if t.is_minus_one() {
        Err(io::Error::last_os_error())
    } else {
        Ok(t)
    }
}
