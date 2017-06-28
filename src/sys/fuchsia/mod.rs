#![allow(warnings)]

use {io, poll, Event, Evented, Ready, Registration, Poll, PollOpt, Token};
use magenta;
use iovec::IoVec;
use std::fmt;
use std::io::{Read, Write};
use std::net::{self, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::atomic::{AtomicUsize, ATOMIC_USIZE_INIT, Ordering};
use std::time::Duration;

pub struct Awakener {
    reader: magenta::Channel,
    writer: magenta::Channel,
}

impl Awakener {
    pub fn new() -> io::Result<Awakener> {
        let (reader, writer) =
            magenta::Channel::create(magenta::ChannelOpts::Normal).map_err(status_to_io_err)?;

        let message_buf = magenta::MessageBuf::new();

        Ok(Awakener {
            reader,
            writer,
        })
    }

    pub fn wakeup(&self) -> io::Result<()> {
        let opts = 0;
        match self.writer.write(&[1], &mut Vec::new(), opts).map_err(status_to_io_err) {
            Ok(()) => Ok(()),
            Err(e) => {
                if e.kind() == io::ErrorKind::WouldBlock {
                    Ok(())
                } else {
                    Err(e)
                }
            }
        }
    }

    pub fn cleanup(&self) {
        let opts = 0;
        let mut message_buf = magenta::MessageBuf::new();

        // TODO: allow passing slices in Channel::read so that we can avoid the allocation here
        message_buf.ensure_capacity_bytes(1);
        let _res = self.reader.read(opts, &mut message_buf);
    }
}

impl Evented for Awakener {
    fn register(&self,
                poll: &Poll,
                token: Token,
                events: Ready,
                opts: PollOpt) -> io::Result<()>
    {
        poll::selector(poll).register(&self.reader, token, events, opts)
    }

    fn reregister(&self,
                  poll: &Poll,
                  token: Token,
                  events: Ready,
                  opts: PollOpt) -> io::Result<()>
    {
        self.register(poll, token, events, opts)
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()>
    {
        // TODO: what to obout missing token?
        Ok(())
    }
}

/// The Fuchsia selector only handles one event at a time, so there's no reason to
/// provide storage for multiple events.
pub struct Events {
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
    port: magenta::Port,
}

impl Selector {
    pub fn new() -> io::Result<Selector> {
        let port = magenta::Port::create(magenta::PortOpts::V2).map_err(status_to_io_err)?;

        // offset by 1 to avoid choosing 0 as the id of a selector
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed) + 1;

        Ok(Selector {
            id,
            port,
        })
    }

    pub fn id(&self) -> usize {
        self.id
    }

    pub fn select(&self,
                  evts: &mut Events,
                  awakener: Token,
                  timeout: Option<Duration>) -> io::Result<bool>
    {
        evts.event_opt = None;

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

        let key = packet.key();

        if key == awakener.0 as u64 {
            return Ok(true);
        }

        let observed_signals = match packet.contents() {
            magenta::PacketContents::SignalOne(signal_packet) => {
                signal_packet.observed()
            },
            magenta::PacketContents::SignalRep(signal_packet) => {
                signal_packet.observed()
            },
            magenta::PacketContents::User(_user_packet) => {
                panic!("User packets should not be sent to mio ports");
            },
        };

        let readable = if magenta::MX_SIGNAL_NONE != (observed_signals & (
            magenta::MX_CHANNEL_READABLE |
            magenta::MX_SOCKET_READABLE
        )) {
            Ready::readable()
        } else {
            Ready::none()
        };

        let writable = if magenta::MX_SIGNAL_NONE != (observed_signals & (
            magenta::MX_CHANNEL_WRITABLE |
            magenta::MX_SOCKET_WRITABLE
        )) {
            Ready::writable()
        } else {
            Ready::none()
        };

        evts.event_opt = Some(Event::new(readable | writable, Token(key as usize)));

        Ok(false)
    }

    /// Register event interests for the given IO handle with the OS
    pub fn register<H: magenta::HandleBase>(&self,
                                            handle: &H,
                                            token: Token,
                                            interest: Ready,
                                            poll_opts: PollOpt) -> io::Result<()>
    {
        let signals =
            (if interest.is_readable() {
                magenta::MX_CHANNEL_READABLE |
                magenta::MX_SOCKET_READABLE
                } else { magenta::MX_SIGNAL_NONE })
                |
            (if interest.is_writable() {
                magenta::MX_CHANNEL_WRITABLE |
                magenta::MX_SOCKET_WRITABLE
                } else { magenta::MX_SIGNAL_NONE });

        let wait_async_opts = if poll_opts.is_oneshot() {
            magenta::WaitAsyncOpts::Once
        } else {
            magenta::WaitAsyncOpts::Repeating
        };

        // TODO: correctly handle level-triggered repeating events

        handle.wait_async(&self.port, token.0 as u64, signals, wait_async_opts)
            .map_err(status_to_io_err)
    }

    /// Deregister event interests for the given IO handle with the OS
    pub fn deregister<H: magenta::HandleBase>(&self, handle: &H, token: Token) -> io::Result<()> {
        self.port.cancel(&*handle, token.0 as u64)
            .map_err(status_to_io_err)
    }
}

#[derive(Debug)]
pub struct TcpStream;
impl TcpStream {
    pub fn connect(stream: net::TcpStream, addr: &SocketAddr) -> io::Result<TcpStream> {
        unimplemented!()
    }

    pub fn from_stream(stream: net::TcpStream) -> TcpStream {
        unimplemented!()
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        unimplemented!()
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        unimplemented!()
    }

    pub fn try_clone(&self) -> io::Result<TcpStream> {
        unimplemented!()
    }

    pub fn shutdown(&self, how: net::Shutdown) -> io::Result<()> {
        unimplemented!()
    }

    pub fn set_nodelay(&self, nodelay: bool) -> io::Result<()> {
        unimplemented!()
    }

    pub fn nodelay(&self) -> io::Result<bool> {
        unimplemented!()
    }

    pub fn set_recv_buffer_size(&self, size: usize) -> io::Result<()> {
        unimplemented!()
    }

    pub fn recv_buffer_size(&self) -> io::Result<usize> {
        unimplemented!()
    }

    pub fn set_send_buffer_size(&self, size: usize) -> io::Result<()> {
        unimplemented!()
    }

    pub fn send_buffer_size(&self) -> io::Result<usize> {
        unimplemented!()
    }

    pub fn set_keepalive(&self, keepalive: Option<Duration>) -> io::Result<()> {
        unimplemented!()
    }

    pub fn keepalive(&self) -> io::Result<Option<Duration>> {
        unimplemented!()
    }

    pub fn set_ttl(&self, ttl: u32) -> io::Result<()> {
        unimplemented!()
    }

    pub fn ttl(&self) -> io::Result<u32> {
        unimplemented!()
    }

    pub fn set_only_v6(&self, only_v6: bool) -> io::Result<()> {
        unimplemented!()
    }

    pub fn only_v6(&self) -> io::Result<bool> {
        unimplemented!()
    }

    pub fn set_linger(&self, dur: Option<Duration>) -> io::Result<()> {
        unimplemented!()
    }

    pub fn linger(&self) -> io::Result<Option<Duration>> {
        unimplemented!()
    }

    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        unimplemented!()
    }

    pub fn readv(&self, bufs: &mut [&mut IoVec]) -> io::Result<usize> {
        unimplemented!()
    }

    pub fn writev(&self, bufs: &[&IoVec]) -> io::Result<usize> {
        unimplemented!()
    }
}

impl<'a> Read for &'a TcpStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        unimplemented!()
    }
}

impl<'a> Write for &'a TcpStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        unimplemented!()
    }
    fn flush(&mut self,) -> io::Result<()> {
        unimplemented!()
    }
}

impl Evented for TcpStream {
    fn register(&self,
                poll: &Poll,
                token: Token,
                interest: Ready,
                opts: PollOpt) -> io::Result<()>
    {
        unimplemented!()
    }

    fn reregister(&self,
                poll: &Poll,
                token: Token,
                interest: Ready,
                opts: PollOpt) -> io::Result<()>
    {
        unimplemented!()
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        unimplemented!()
    }
}

#[derive(Debug)]
pub struct TcpListener;

impl TcpListener {
    pub fn new(inner: net::TcpListener, _addr: &SocketAddr) -> io::Result<TcpListener> {
        unimplemented!()
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        unimplemented!()
    }

    pub fn try_clone(&self) -> io::Result<TcpListener> {
        unimplemented!()
    }

    pub fn accept(&self) -> io::Result<(TcpStream, SocketAddr)> {
        unimplemented!()
    }

    pub fn set_only_v6(&self, only_v6: bool) -> io::Result<()> {
        unimplemented!()
    }

    pub fn only_v6(&self) -> io::Result<bool> {
        unimplemented!()
    }

    pub fn set_ttl(&self, ttl: u32) -> io::Result<()> {
        unimplemented!()
    }

    pub fn ttl(&self) -> io::Result<u32> {
        unimplemented!()
    }

    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        unimplemented!()
    }
}

impl Evented for TcpListener {
    fn register(&self,
                poll: &Poll,
                token: Token,
                interest: Ready,
                opts: PollOpt) -> io::Result<()> {
        unimplemented!()
    }

    fn reregister(&self,
                poll: &Poll,
                token: Token,
                interest: Ready,
                opts: PollOpt) -> io::Result<()> {
        unimplemented!()
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        unimplemented!()
    }
}

#[derive(Debug)]
pub struct UdpSocket;

impl UdpSocket {
    pub fn new(socket: net::UdpSocket) -> io::Result<UdpSocket> {
        unimplemented!()
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        unimplemented!()
    }

    pub fn try_clone(&self) -> io::Result<UdpSocket> {
        unimplemented!()
    }

    pub fn send_to(&self, buf: &[u8], target: &SocketAddr) -> io::Result<usize> {
        unimplemented!()
    }

    pub fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        unimplemented!()
    }

    pub fn send(&self, buf: &[u8]) -> io::Result<usize> {
        unimplemented!()
    }

    pub fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        unimplemented!()
    }

    pub fn connect(&self, addr: SocketAddr) -> io::Result<()> {
        unimplemented!()
    }

    pub fn broadcast(&self) -> io::Result<bool> {
        unimplemented!()
    }

    pub fn set_broadcast(&self, on: bool) -> io::Result<()> {
        unimplemented!()
    }

    pub fn multicast_loop_v4(&self) -> io::Result<bool> {
        unimplemented!()
    }

    pub fn set_multicast_loop_v4(&self, on: bool) -> io::Result<()> {
        unimplemented!()
    }

    pub fn multicast_ttl_v4(&self) -> io::Result<u32> {
        unimplemented!()
    }

    pub fn set_multicast_ttl_v4(&self, ttl: u32) -> io::Result<()> {
        unimplemented!()
    }

    pub fn multicast_loop_v6(&self) -> io::Result<bool> {
        unimplemented!()
    }

    pub fn set_multicast_loop_v6(&self, on: bool) -> io::Result<()> {
        unimplemented!()
    }

    pub fn ttl(&self) -> io::Result<u32> {
        unimplemented!()
    }

    pub fn set_ttl(&self, ttl: u32) -> io::Result<()> {
        unimplemented!()
    }

    pub fn join_multicast_v4(&self,
                             multiaddr: &Ipv4Addr,
                             interface: &Ipv4Addr) -> io::Result<()> {
        unimplemented!()
    }

    pub fn join_multicast_v6(&self,
                             multiaddr: &Ipv6Addr,
                             interface: u32) -> io::Result<()> {
        unimplemented!()
    }

    pub fn leave_multicast_v4(&self,
                              multiaddr: &Ipv4Addr,
                              interface: &Ipv4Addr) -> io::Result<()> {
        unimplemented!()
    }

    pub fn leave_multicast_v6(&self,
                              multiaddr: &Ipv6Addr,
                              interface: u32) -> io::Result<()> {
        unimplemented!()
    }

    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        unimplemented!()
    }
}

impl Evented for UdpSocket {
    fn register(&self,
                poll: &Poll,
                token: Token,
                interest: Ready,
                opts: PollOpt) -> io::Result<()>
    {
        unimplemented!()
    }

    fn reregister(&self,
                  poll: &Poll,
                  token: Token,
                  interest: Ready,
                  opts: PollOpt) -> io::Result<()>
    {
        unimplemented!()
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        unimplemented!()
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
