//! Primitives for working with UDP
//!
//! The types provided in this module are non-blocking by default and are
//! designed to be portable across all supported Mio platforms. As long as the
//! [portability guidelines] are followed, the behavior should be identical no
//! matter the target platform.
//!
/// [portability guidelines]: ../struct.Poll.html#portability

use {io, sys, Ready, Poll, PollOpt, Token};
use event::Evented;
use poll::SelectorId;
use std::net::{self, Ipv4Addr, Ipv6Addr, SocketAddr};

/// A User Datagram Protocol socket.
///
/// This is an implementation of a bound UDP socket. This supports both IPv4 and
/// IPv6 addresses, and there is no corresponding notion of a server because UDP
/// is a datagram protocol.
///
/// # Examples
///
/// ```
/// # use std::error::Error;
/// #
/// # fn try_main() -> Result<(), Box<Error>> {
/// // An Echo program:
/// // SENDER -> sends a message.
/// // ECHOER -> listens and prints the message received.
///
/// use mio::net::UdpSocket;
/// use mio::{Events, Ready, Poll, PollOpt, Token};
/// use std::time::Duration;
///
/// const SENDER: Token = Token(0x4c4f4f43);
/// const ECHOER: Token = Token(0x54535552);
///
/// // This operation will fail if the address is in use, so we select different ports for each
/// // socket.
/// let sender_socket = UdpSocket::bind(&"127.0.0.1:7777".parse()?)?;
/// let echoer_socket = UdpSocket::bind(&"127.0.0.1:11100".parse()?)?;
///
/// // If we do not use connect here, SENDER and ECHOER would need to call send_to and recv_from
/// // respectively.
/// sender_socket.connect("127.0.0.1:11100".parse()?)?;
///
/// // We need a Poll to check if SENDER is ready to be written into, and if ECHOER is ready to be
/// // read from.
/// let poll = Poll::new()?;
///
/// // We register our sockets here so that we can check if they are ready to be written/read.
/// poll.register(&sender_socket, SENDER, Ready::writable(), PollOpt::edge())?;
/// poll.register(&echoer_socket, ECHOER, Ready::readable(), PollOpt::edge())?;
///
/// // We keep buffer small to prevent fragmentation.
/// let msg_to_send = [9; 9];
/// let mut buffer = [0; 9];
/// let mut events = Events::with_capacity(128);
/// loop {
///     poll.poll(&mut events, Some(Duration::from_millis(100)))?;
///     for event in events.iter() {
///         match event.token() {
///             // Our SENDER is ready to be written into.
///             SENDER => {
///                 let bytes_sent = sender_socket.send(&msg_to_send)?;
///                 assert_eq!(bytes_sent, 9);
///                 println!("sent {:?} -> {:?} bytes", msg_to_send, bytes_sent);
///             },
///             // Our ECHOER is ready to be read from.
///             ECHOER => {
///                 let num_recv = echoer_socket.recv(&mut buffer)?;
///                 println!("echo {:?} -> {:?}", buffer, num_recv);
///                 buffer = [0; 9];
///                 # return Ok(());
///             }
///             _ => unreachable!()
///         }
///     }
/// }
/// #
/// #   Ok(())
/// # }
/// #
/// # fn main() {
/// #   try_main().unwrap();
/// # }
/// ```
#[derive(Debug)]
pub struct UdpSocket {
    sys: sys::UdpSocket,
    selector_id: SelectorId,
}

impl UdpSocket {
    /// Creates a UDP socket from the given address.
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::error::Error;
    /// #
    /// # fn try_main() -> Result<(), Box<Error>> {
    /// use mio::net::UdpSocket;
    /// use mio::{Events, Ready, Poll, PollOpt, Token};
    /// use std::time::Duration;
    ///
    /// const SENDER: Token = Token(0x4c4f4f43);
    ///
    /// // We must bind it to an open address.
    /// let socket = match UdpSocket::bind(&"127.0.0.1:7777".parse()?) {
    ///     Ok(socket) => socket,
    ///     Err(fail) => {
    ///         // We panic! here, but you could try to bind it again on another address.
    ///         panic!("Failed to bind socket. {:?}", fail);
    ///     }
    /// };
    ///
    /// // Our socket was created, now set up a Poll before using it.
    /// let poll = Poll::new()?;
    /// poll.register(&socket, SENDER, Ready::writable(), PollOpt::edge())?;
    ///
    /// let mut events = Events::with_capacity(128);
    /// loop {
    ///     poll.poll(&mut events, Some(std::time::Duration::from_millis(100)))?;
    ///     for event in events.iter() {
    ///         match event.token() {
    ///             SENDER => {
    ///                 let bytes_sent = socket.send_to(&[9; 9], &"127.0.0.1:11100".parse()?)?;
    ///                 println!("SENDER sent {:?} bytes.", bytes_sent);
    ///                 assert_eq!(bytes_sent, 9);
    ///                 # return Ok(());
    ///             }
    ///             _ => unreachable!()
    ///         }
    ///     }
    /// }
    /// #
    /// #   Ok(())
    /// # }
    /// #
    /// # fn main() {
    /// #   try_main().unwrap();
    /// # }
    /// ```
    pub fn bind(addr: &SocketAddr) -> io::Result<UdpSocket> {
        let socket = net::UdpSocket::bind(addr)?;
        UdpSocket::from_socket(socket)
    }

    /// Creates a new mio-wrapped socket from an underlying and bound std
    /// socket.
    ///
    /// This function requires that `socket` has previously been bound to an
    /// address to work correctly, and returns an I/O object which can be used
    /// with mio to send/receive UDP messages.
    ///
    /// This can be used in conjunction with net2's `UdpBuilder` interface to
    /// configure a socket before it's handed off to mio, such as setting
    /// options like `reuse_address` or binding to multiple addresses.
    pub fn from_socket(socket: net::UdpSocket) -> io::Result<UdpSocket> {
        Ok(UdpSocket {
            sys: sys::UdpSocket::new(socket)?,
            selector_id: SelectorId::new(),
        })
    }

    /// Returns the socket address that this socket was created from.
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::error::Error;
    /// #
    /// # fn try_main() -> Result<(), Box<Error>> {
    /// use mio::net::UdpSocket;
    /// use mio::{Events, Ready, Poll, PollOpt, Token};
    /// use std::time::Duration;
    ///
    /// const SENDER: Token = Token(0x4c4f4f43);
    /// const ECHOER: Token = Token(0x54535552);
    /// #
    /// # let hidden_addr = "127.0.0.1:7777".parse()?;
    /// // We don't know which address another_socket is using.
    /// let another_socket = UdpSocket::bind(&hidden_addr)?;
    ///
    /// // We check to see if this address isn't being used by another_socket.
    /// let dangerous_addr = "127.0.0.1:7777".parse()?;
    /// let fallback_addr = "127.0.0.1:11100".parse()?;
    /// let socket = if another_socket.local_addr()? == dangerous_addr {
    ///     UdpSocket::bind(&fallback_addr)?
    /// } else {
    ///     UdpSocket::bind(&dangerous_addr)?
    /// };
    ///
    /// // We now have multiple working sockets to use.
    ///
    /// another_socket.connect(socket.local_addr()?)?;
    ///
    /// let poll = Poll::new()?;
    /// poll.register(&another_socket, SENDER, Ready::writable(), PollOpt::edge())?;
    /// poll.register(&socket, ECHOER, Ready::readable(), PollOpt::edge())?;
    ///
    /// let mut buffer = [0; 9];
    /// let mut events = Events::with_capacity(128);
    /// loop {
    ///     poll.poll(&mut events, Some(Duration::from_millis(100)))?;
    ///     for event in events.iter() {
    ///         match event.token() {
    ///             SENDER => {
    ///                 let bytes_sent = another_socket.send(&[9; 9])?;
    ///                 println!("SENDER sent {:?} bytes", bytes_sent);
    ///                 assert_eq!(bytes_sent, 9);
    ///             },
    ///             ECHOER => {
    ///                 let num_recv = socket.recv(&mut buffer)?;
    ///                 println!("ECHOER received {:?} -> {:?} bytes", buffer, num_recv);
    ///                 # return Ok(());
    ///             }
    ///             _ => unreachable!()
    ///         }
    ///     }
    /// }
    /// #
    /// #   Ok(())
    /// # }
    /// #
    /// # fn main() {
    /// #   try_main().unwrap();
    /// # }
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.sys.local_addr()
    }

    /// Creates a new independently owned handle to the underlying socket.
    ///
    /// The returned `UdpSocket` is a reference to the same socket that this
    /// object references. Both handles will read and write the same port, and
    /// options set on one socket will be propagated to the other.
    pub fn try_clone(&self) -> io::Result<UdpSocket> {
        self.sys.try_clone()
            .map(|s| {
                UdpSocket {
                    sys: s,
                    selector_id: self.selector_id.clone(),
                }
            })
    }

    /// Sends data on the socket to the given address. On success, returns the
    /// number of bytes written.
    ///
    /// Address type can be any implementor of `ToSocketAddrs` trait. See its
    /// documentation for concrete examples.
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::error::Error;
    /// #
    /// # fn try_main() -> Result<(), Box<Error>> {
    /// use mio::net::UdpSocket;
    /// use mio::{Events, Ready, Poll, PollOpt, Token};
    ///
    /// const SENDER: Token = Token(0x4c4f4f43);
    ///
    /// let socket = UdpSocket::bind(&"127.0.0.1:7777".parse()?)?;
    ///
    /// let poll = Poll::new()?;
    /// poll.register(&socket, SENDER, Ready::writable(), PollOpt::edge())?;
    ///
    /// let mut events = Events::with_capacity(128);
    /// loop {
    ///     poll.poll(&mut events, Some(std::time::Duration::from_millis(100)))?;
    ///
    ///     // We must check if the socket is writable before calling send_to,
    ///     // or we could run into a WouldBlock error.
    ///     for event in events.iter() {
    ///         match event.token() {
    ///             SENDER => {
    ///                 let bytes_sent = socket.send_to(&[9; 9], &"127.0.0.1:11100".parse()?)?;
    ///                 println!("SENDER sent {:?} bytes.", bytes_sent);
    ///                 assert_eq!(bytes_sent, 9);
    ///                 # return Ok(());
    ///             }
    ///             _ => unreachable!()
    ///         }
    ///     }
    /// }
    /// #
    /// #   Ok(())
    /// # }
    /// #
    /// # fn main() {
    /// #   try_main().unwrap();
    /// # }
    /// ```
    pub fn send_to(&self, buf: &[u8], target: &SocketAddr) -> io::Result<usize> {
        self.sys.send_to(buf, target)
    }

    /// Receives data from the socket. On success, returns the number of bytes
    /// read and the address from whence the data came.
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::error::Error;
    /// #
    /// # fn try_main() -> Result<(), Box<Error>> {
    /// use mio::net::UdpSocket;
    /// use mio::{Events, Ready, Poll, PollOpt, Token};
    /// use std::time::Duration;
    ///
    /// # const SENDER: Token = Token(0x4c4f4f43);
    /// const RECEIVER: Token = Token(0x52494952);
    ///
    /// # let sender_socket = UdpSocket::bind(&"127.0.0.1:7777".parse()?)?;
    /// let socket = UdpSocket::bind(&"127.0.0.1:11100".parse()?)?;
    ///
    /// let poll = Poll::new()?;
    /// # poll.register(&sender_socket, SENDER, Ready::writable(), PollOpt::edge())?;
    /// poll.register(&socket, RECEIVER, Ready::readable(), PollOpt::edge())?;
    ///
    /// let mut buf = [0; 9];
    /// let mut events = Events::with_capacity(128);
    /// loop {
    ///     poll.poll(&mut events, Some(Duration::from_millis(100)))?;
    ///     for event in events.iter() {
    ///         match event.token() {
    ///             # SENDER => {
    ///                 # sender_socket.send_to(&[9; 9], &"127.0.0.1:11100".parse()?)?;
    ///             # }
    ///             RECEIVER => {
    ///                 let (num_recv, from) = socket.recv_from(&mut buf)?;
    ///                 println!("Received {:?} -> {:?} bytes from {:?}", buf, num_recv, from);
    ///                 # return Ok(());
    ///             }
    ///             _ => unreachable!()
    ///         }
    ///     }
    /// }
    /// #
    /// #   Ok(())
    /// # }
    /// #
    /// # fn main() {
    /// #   try_main().unwrap();
    /// # }
    /// ```
    pub fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        self.sys.recv_from(buf)
    }

    /// Sends data on the socket to the address previously bound via connect(). On success,
    /// returns the number of bytes written.
    pub fn send(&self, buf: &[u8]) -> io::Result<usize> {
        self.sys.send(buf)
    }

    /// Receives data from the socket previously bound with connect(). On success, returns
    /// the number of bytes read and the address from whence the data came.
    pub fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        self.sys.recv(buf)
    }

    /// Connects the UDP socket setting the default destination for `send()`
    /// and limiting packets that are read via `recv` from the address specified
    /// in `addr`.
    pub fn connect(&self, addr: SocketAddr) -> io::Result<()> {
        self.sys.connect(addr)
    }

    /// Sets the value of the `SO_BROADCAST` option for this socket.
    ///
    /// When enabled, this socket is allowed to send packets to a broadcast
    /// address.
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::error::Error;
    /// #
    /// # fn try_main() -> Result<(), Box<Error>> {
    /// use mio::net::UdpSocket;
    /// use mio::{Events, Ready, Poll, PollOpt, Token};
    ///
    /// const BROADCASTER: Token = Token(0x4f494d);
    ///
    /// // This is a special broadcast address for the local network, the router won't forward
    /// // transmissions sent here to other networks.
    /// let broadcast_addr = "255.255.255.255:11100".parse()?;
    /// let broadcast_socket = UdpSocket::bind(&"127.0.0.1:7777".parse()?)?;
    ///
    /// // SO_BROADCAST is set to false by default.
    /// if broadcast_socket.broadcast()? == false {
    ///     broadcast_socket.set_broadcast(true)?;
    /// }
    ///
    /// let poll = Poll::new()?;
    /// let mut events = Events::with_capacity(128);
    ///
    /// // The socket for this example doesn't need to be readable, as we are only writing into it.
    /// // We make it readable here for convenience.
    /// poll.register(&broadcast_socket, BROADCASTER, Ready::readable() | Ready::writable(),
    ///     PollOpt::edge())?;
    ///
    /// loop {
    ///     poll.poll(&mut events, Some(std::time::Duration::from_millis(100)))?;
    ///
    ///     // We must check if the socket is writable before calling send_to,
    ///     // or we could run into a WouldBlock error.
    ///     for event in events.iter() {
    ///         match event.token() {
    ///             BROADCASTER => {
    ///                 // Your firewall and/or your router might have a rule 
    ///                 // blocking broadcast transmissions.
    ///                 let bytes_sent = broadcast_socket.send_to(&[9; 9], &broadcast_addr)?;
    ///                 println!("BROADCASTER sent: {:?} bytes.", bytes_sent);
    ///                 assert_eq!(bytes_sent, 9);
    ///                 # return Ok(());
    ///             }
    ///             _ => unreachable!()
    ///         }
    ///     }
    /// }
    /// #
    /// #   Ok(())
    /// # }
    /// #
    /// # fn main() {
    /// #   try_main().unwrap();
    /// # }
    /// ```
    pub fn set_broadcast(&self, on: bool) -> io::Result<()> {
        self.sys.set_broadcast(on)
    }

    /// Gets the value of the `SO_BROADCAST` option for this socket.
    ///
    /// For more information about this option, see
    /// [`set_broadcast`][link].
    ///
    /// [link]: #method.set_broadcast
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::error::Error;
    /// #
    /// # fn try_main() -> Result<(), Box<Error>> {
    /// use mio::net::UdpSocket;
    /// # use mio::{Events, Ready, Poll, PollOpt, Token};
    ///
    /// const BROADCASTER: Token = Token(0x4f494d);
    ///
    /// // This is a special broadcast address for the local network, the router won't forward
    /// // transmissions sent here to other networks.
    /// let broadcast_addr = "255.255.255.255:11100".parse()?;
    /// let broadcast_socket = UdpSocket::bind(&"127.0.0.1:7777".parse()?)?;
    ///
    /// // SO_BROADCAST is set to false by default.
    /// if broadcast_socket.broadcast()? == false {
    ///     broadcast_socket.set_broadcast(true)?;
    /// }
    /// #
    /// // Set up Poll, check if the socket is writable and begin transmission.
    /// let poll = Poll::new()?;
    /// let mut events = Events::with_capacity(128);
    /// 
    /// poll.register(&broadcast_socket, BROADCASTER, Ready::writable(),
    ///     PollOpt::edge())?;
    ///
    /// loop {
    ///     poll.poll(&mut events, Some(std::time::Duration::from_millis(100)))?;
    ///     for event in events.iter() {
    ///         match event.token() {
    ///             BROADCASTER => {
    ///                 // This operation will fail if socket.broadcast() == false.
    ///                 let bytes_sent = broadcast_socket.send_to(&[9; 9], &broadcast_addr)?;
    ///                 println!("BROADCASTER sent: {:?} bytes.", bytes_sent);
    ///                 assert_eq!(bytes_sent, 9);
    ///                 # return Ok(());
    ///             }
    ///             _ => unreachable!()
    ///         }
    ///     }
    /// }
    /// #
    /// #   Ok(())
    /// # }
    /// #
    /// # fn main() {
    /// #   try_main().unwrap();
    /// # }
    /// ```
    pub fn broadcast(&self) -> io::Result<bool> {
        self.sys.broadcast()
    }

    /// Sets the value of the `IP_MULTICAST_LOOP` option for this socket.
    ///
    /// If enabled, multicast packets will be looped back to the local socket.
    /// Note that this may not have any affect on IPv6 sockets.
    pub fn set_multicast_loop_v4(&self, on: bool) -> io::Result<()> {
        self.sys.set_multicast_loop_v4(on)
    }

    /// Gets the value of the `IP_MULTICAST_LOOP` option for this socket.
    ///
    /// For more information about this option, see
    /// [`set_multicast_loop_v4`][link].
    ///
    /// [link]: #method.set_multicast_loop_v4
    pub fn multicast_loop_v4(&self) -> io::Result<bool> {
        self.sys.multicast_loop_v4()
    }

    /// Sets the value of the `IP_MULTICAST_TTL` option for this socket.
    ///
    /// Indicates the time-to-live value of outgoing multicast packets for
    /// this socket. The default value is 1 which means that multicast packets
    /// don't leave the local network unless explicitly requested.
    ///
    /// Note that this may not have any affect on IPv6 sockets.
    pub fn set_multicast_ttl_v4(&self, ttl: u32) -> io::Result<()> {
        self.sys.set_multicast_ttl_v4(ttl)
    }

    /// Gets the value of the `IP_MULTICAST_TTL` option for this socket.
    ///
    /// For more information about this option, see
    /// [`set_multicast_ttl_v4`][link].
    ///
    /// [link]: #method.set_multicast_ttl_v4
    pub fn multicast_ttl_v4(&self) -> io::Result<u32> {
        self.sys.multicast_ttl_v4()
    }

    /// Sets the value of the `IPV6_MULTICAST_LOOP` option for this socket.
    ///
    /// Controls whether this socket sees the multicast packets it sends itself.
    /// Note that this may not have any affect on IPv4 sockets.
    pub fn set_multicast_loop_v6(&self, on: bool) -> io::Result<()> {
        self.sys.set_multicast_loop_v6(on)
    }

    /// Gets the value of the `IPV6_MULTICAST_LOOP` option for this socket.
    ///
    /// For more information about this option, see
    /// [`set_multicast_loop_v6`][link].
    ///
    /// [link]: #method.set_multicast_loop_v6
    pub fn multicast_loop_v6(&self) -> io::Result<bool> {
        self.sys.multicast_loop_v6()
    }

    /// Sets the value for the `IP_TTL` option on this socket.
    ///
    /// This value sets the time-to-live field that is used in every packet sent
    /// from this socket.
    pub fn set_ttl(&self, ttl: u32) -> io::Result<()> {
        self.sys.set_ttl(ttl)
    }

    /// Gets the value of the `IP_TTL` option for this socket.
    ///
    /// For more information about this option, see [`set_ttl`][link].
    ///
    /// [link]: #method.set_ttl
    pub fn ttl(&self) -> io::Result<u32> {
        self.sys.ttl()
    }

    /// Executes an operation of the `IP_ADD_MEMBERSHIP` type.
    ///
    /// This function specifies a new multicast group for this socket to join.
    /// The address must be a valid multicast address, and `interface` is the
    /// address of the local interface with which the system should join the
    /// multicast group. If it's equal to `INADDR_ANY` then an appropriate
    /// interface is chosen by the system.
    pub fn join_multicast_v4(&self,
                             multiaddr: &Ipv4Addr,
                             interface: &Ipv4Addr) -> io::Result<()> {
        self.sys.join_multicast_v4(multiaddr, interface)
    }

    /// Executes an operation of the `IPV6_ADD_MEMBERSHIP` type.
    ///
    /// This function specifies a new multicast group for this socket to join.
    /// The address must be a valid multicast address, and `interface` is the
    /// index of the interface to join/leave (or 0 to indicate any interface).
    pub fn join_multicast_v6(&self,
                             multiaddr: &Ipv6Addr,
                             interface: u32) -> io::Result<()> {
        self.sys.join_multicast_v6(multiaddr, interface)
    }

    /// Executes an operation of the `IP_DROP_MEMBERSHIP` type.
    ///
    /// For more information about this option, see
    /// [`join_multicast_v4`][link].
    ///
    /// [link]: #method.join_multicast_v4
    pub fn leave_multicast_v4(&self,
                              multiaddr: &Ipv4Addr,
                              interface: &Ipv4Addr) -> io::Result<()> {
        self.sys.leave_multicast_v4(multiaddr, interface)
    }

    /// Executes an operation of the `IPV6_DROP_MEMBERSHIP` type.
    ///
    /// For more information about this option, see
    /// [`join_multicast_v6`][link].
    ///
    /// [link]: #method.join_multicast_v6
    pub fn leave_multicast_v6(&self,
                              multiaddr: &Ipv6Addr,
                              interface: u32) -> io::Result<()> {
        self.sys.leave_multicast_v6(multiaddr, interface)
    }

    /// Sets the value for the `IPV6_V6ONLY` option on this socket.
    ///
    /// If this is set to `true` then the socket is restricted to sending and
    /// receiving IPv6 packets only. In this case two IPv4 and IPv6 applications
    /// can bind the same port at the same time.
    ///
    /// If this is set to `false` then the socket can be used to send and
    /// receive packets from an IPv4-mapped IPv6 address.
    pub fn set_only_v6(&self, only_v6: bool) -> io::Result<()> {
        self.sys.set_only_v6(only_v6)
    }

    /// Gets the value of the `IPV6_V6ONLY` option for this socket.
    ///
    /// For more information about this option, see [`set_only_v6`][link].
    ///
    /// [link]: #method.set_only_v6
    pub fn only_v6(&self) -> io::Result<bool> {
        self.sys.only_v6()
    }

    /// Get the value of the `SO_ERROR` option on this socket.
    ///
    /// This will retrieve the stored error in the underlying socket, clearing
    /// the field in the process. This can be useful for checking errors between
    /// calls.
    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.sys.take_error()
    }
}

impl Evented for UdpSocket {
    fn register(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()> {
        self.selector_id.associate_selector(poll)?;
        self.sys.register(poll, token, interest, opts)
    }

    fn reregister(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()> {
        self.sys.reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        self.sys.deregister(poll)
    }
}

/*
 *
 * ===== UNIX ext =====
 *
 */

#[cfg(all(unix, not(target_os = "fuchsia")))]
use std::os::unix::io::{IntoRawFd, AsRawFd, FromRawFd, RawFd};

#[cfg(all(unix, not(target_os = "fuchsia")))]
impl IntoRawFd for UdpSocket {
    fn into_raw_fd(self) -> RawFd {
        self.sys.into_raw_fd()
    }
}

#[cfg(all(unix, not(target_os = "fuchsia")))]
impl AsRawFd for UdpSocket {
    fn as_raw_fd(&self) -> RawFd {
        self.sys.as_raw_fd()
    }
}

#[cfg(all(unix, not(target_os = "fuchsia")))]
impl FromRawFd for UdpSocket {
    unsafe fn from_raw_fd(fd: RawFd) -> UdpSocket {
        UdpSocket {
            sys: FromRawFd::from_raw_fd(fd),
            selector_id: SelectorId::new(),
        }
    }
}

