#![allow(unused_variables)]

use {io, Event, Evented, Ready, Poll, PollOpt, Token};
use iovec::IoVec;
use std::fmt;
use std::io::{Read, Write};
use std::net::{self, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

pub struct Awakener;
impl Awakener {
    pub fn new() -> io::Result<Awakener> {
        unimplemented!()
    }
    pub fn wakeup(&self) -> io::Result<()> {
        unimplemented!()
    }
    pub fn cleanup(&self) {
        unimplemented!()
    }
}

impl Evented for Awakener {
    fn register(&self,
                poll: &Poll,
                token: Token,
                events: Ready,
                opts: PollOpt) -> io::Result<()>
    {
        unimplemented!()
    }

    fn reregister(&self,
                  poll: &Poll,
                  token: Token,
                  events: Ready,
                  opts: PollOpt) -> io::Result<()>
    {
        unimplemented!()
    }

    fn deregister(&self, _poll: &Poll) -> io::Result<()>
    {
        unimplemented!()
    }
}

pub struct Events;
impl Events {
    pub fn with_capacity(u: usize) -> Events {
        unimplemented!()
    }
    pub fn len(&self) -> usize {
        unimplemented!()
    }
    pub fn capacity(&self) -> usize {
         unimplemented!()
    }
    pub fn is_empty(&self) -> bool {
        unimplemented!()
    }
    pub fn get(&self, idx: usize) -> Option<Event> {
        unimplemented!()
    }
    pub fn push_event(&mut self, event: Event) {
        unimplemented!()
    }
}
impl fmt::Debug for Events {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Events {{ len: {} }}", self.len())
    }
}

pub struct Selector;
impl Selector {
    pub fn new() -> io::Result<Selector> {
        unimplemented!()
    }

    pub fn id(&self) -> usize {
        unimplemented!()
    }

    pub fn select(&self,
                    evts: &mut Events,
                    awakener: Token,
                    timeout: Option<Duration>) -> io::Result<bool>
    {
        unimplemented!()
    }

    // Unix only:
    /*
    pub fn register(&self,
                    fd: RawFd,
                    token: Token,
                    interests: Ready,
                    opts: PollOpt) -> io::Result<()>
    {
        unimplemented!()
    }

    pub fn reregister(&self,
                    fd: RawFd,
                    token: Token,
                    interests: Ready,
                    opts: PollOpt) -> io::Result<()>
    {
        unimplemented!()
    }

    pub fn deregister(&self, fd: RawFd) -> io::Result<()> {
        unimplemented!()
    }
    */
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
