//! # Notes
//!
//! As with WASIp1, WASIp2 is single-threaded, meaning there is no support for
//! `Waker`, nor is there support for (de)registering descriptors while polling.

// # Implementation Notes
//
// This implementation currently uses a mix of POSIX-style APIs (provided by
// `wasi-libc` via the `libc` crate) and WASIp2-native APIs (provided by the
// `wasi` crate).
//
// Alternatively, we could implement `Selector` using only POSIX APIs,
// e.g. `poll(2)`.  However, that would add an extra layer of abstraction to
// support and debug, as well as make it impossible to support polling
// `wasi:io/poll/pollable` objects which cannot be represented as POSIX file
// descriptors (e.g. timer events, DNS queries, HTTP requests, etc.).
//
// Another approach would be to use _only_ the WASIp2 APIs and bypass
// `wasi-libc` entirely.  However, that would break interoperability with both
// Rust `std` and e.g. C libraries which expect to work with file descriptors.
//
// Since `wasi-libc` does not yet provide a public API for converting between
// file descriptors and WASIp2 resource handles, we currently use a non-public
// API (see the `netc` module below) to do so.  Once
// https://github.com/WebAssembly/wasi-libc/issues/542 is addressed, we'll be
// able to switch to a public API.
//
// TODO #1: Add a public, WASIp2-only API for registering
// `wasi::io::poll::Pollable`s directly (i.e. those which do not correspond to
// any `wasi-libc` file descriptor, such as `wasi:http` requests).
//
// TODO #2: Add support for binding, listening, and accepting.  This would
// involve adding cases for `TCP_SOCKET_STATE_UNBOUND`,
// `TCP_SOCKET_STATE_BOUND`, and `TCP_SOCKET_STATE_LISTENING` to the `match`
// statements in `Selector::select`.
//
// TODO #3: Add support for UDP sockets.  This would involve adding cases for
// the `UDP_SOCKET_STATE_*` tags to the `match` statements in
// `Selector::select`.

use crate::{Interest, Token};
use netc as libc;
use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};
use std::io;
use std::mem::ManuallyDrop;
use std::ops::Deref;
use std::os::fd::RawFd;
use std::ptr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use wasi::{
    clocks::monotonic_clock,
    io::poll::{self, Pollable},
    sockets::{network::ErrorCode, tcp::TcpSocket},
};

#[cfg(all(debug_assertions, feature = "net"))]
use std::sync::atomic::{AtomicUsize, Ordering};

#[cfg(feature = "net")]
use {
    crate::Registry,
    std::{ffi::c_int, net::SocketAddr},
};

#[allow(unused_macros)]
macro_rules! syscall {
    ($fn: ident ( $($arg: expr),* $(,)* ) ) => {{
        let res = unsafe { libc::$fn($($arg, )*) };
        if res == -1 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(res)
        }
    }};
}

cfg_net! {
    pub(crate) mod tcp {
        use std::io;
        use std::os::fd::AsRawFd;
        use std::net::{self, SocketAddr};
        use std::ffi::c_int;
        use super::{new_socket, socket_addr, netc as libc};

        pub(crate) fn accept(listener: &net::TcpListener) -> io::Result<(net::TcpStream, SocketAddr)> {
            let (stream, addr) = listener.accept()?;
            stream.set_nonblocking(true)?;
            Ok((stream, addr))
        }

        pub(crate) fn new_for_addr(address: SocketAddr) -> io::Result<c_int> {
            let domain = match address {
                SocketAddr::V4(_) => libc::AF_INET,
                SocketAddr::V6(_) => libc::AF_INET6,
            };
            new_socket(domain, libc::SOCK_STREAM)
        }

        pub(crate) fn connect(socket: &net::TcpStream, addr: SocketAddr) -> io::Result<()> {
            let (raw_addr, raw_addr_length) = socket_addr(&addr);

            match syscall!(connect(
                socket.as_raw_fd(),
                raw_addr.as_ptr(),
                raw_addr_length
            )) {
                Err(err) if err.raw_os_error() != Some(libc::EINPROGRESS) => Err(err),
                _ => Ok(()),
            }
        }
    }
}

#[cfg(all(debug_assertions, feature = "net"))]
static NEXT_ID: AtomicUsize = AtomicUsize::new(1);

#[derive(Debug, Copy, Clone)]
struct Subscription {
    token: Token,
    interests: Option<Interest>,
}

pub(crate) struct Selector {
    #[cfg(all(debug_assertions, feature = "net"))]
    id: usize,
    subscriptions: Arc<Mutex<HashMap<RawFd, Subscription>>>,
}

impl Selector {
    pub(crate) fn new() -> io::Result<Selector> {
        Ok(Selector {
            #[cfg(all(debug_assertions, feature = "net"))]
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            subscriptions: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    #[cfg(all(debug_assertions, feature = "net"))]
    pub(crate) fn id(&self) -> usize {
        self.id
    }

    pub(crate) fn try_clone(&self) -> io::Result<Selector> {
        Ok(Selector {
            #[cfg(all(debug_assertions, feature = "net"))]
            id: self.id,
            subscriptions: self.subscriptions.clone(),
        })
    }

    pub(crate) fn select(&self, events: &mut Events, timeout: Option<Duration>) -> io::Result<()> {
        events.clear();

        let mut subscriptions = self.subscriptions.lock().unwrap();

        let mut states = Vec::new();
        for (fd, subscription) in subscriptions.deref() {
            let mut entry_ref = ptr::null_mut();
            let entry = unsafe {
                if libc::descriptor_table_get_ref(*fd, &mut entry_ref) {
                    *entry_ref
                } else {
                    return Err(io::Error::from_raw_os_error(libc::EBADF));
                }
            };

            let readable = subscription
                .interests
                .map(|v| v.is_readable())
                .unwrap_or(false);

            let writable = subscription
                .interests
                .map(|v| v.is_writable())
                .unwrap_or(false);

            match entry.tag {
                libc::descriptor_table_entry_tag_t::DESCRIPTOR_TABLE_ENTRY_TCP_SOCKET => {
                    let socket = unsafe { entry.value.tcp_socket };
                    match socket.state.tag {
                        libc::tcp_socket_state_tag_t::TCP_SOCKET_STATE_CONNECTING => {
                            if readable || writable {
                                states.push((
                                    ManuallyDrop::new(unsafe {
                                        Pollable::from_handle(socket.socket_pollable)
                                    }),
                                    *fd,
                                    socket,
                                    *subscription,
                                    subscription.interests.unwrap(),
                                ));
                            }
                        }

                        libc::tcp_socket_state_tag_t::TCP_SOCKET_STATE_CONNECTED => {
                            if writable {
                                states.push((
                                    ManuallyDrop::new(unsafe {
                                        Pollable::from_handle(
                                            socket.state.value.connected.output_pollable,
                                        )
                                    }),
                                    *fd,
                                    socket,
                                    *subscription,
                                    Interest::WRITABLE,
                                ));
                            }

                            if readable {
                                states.push((
                                    ManuallyDrop::new(unsafe {
                                        Pollable::from_handle(
                                            socket.state.value.connected.input_pollable,
                                        )
                                    }),
                                    *fd,
                                    socket,
                                    *subscription,
                                    Interest::READABLE,
                                ));
                            }
                        }

                        _ => return Err(io::Error::from_raw_os_error(libc::EBADF)),
                    }
                }

                _ => return Err(io::Error::from_raw_os_error(libc::EBADF)),
            }
        }

        let timeout = timeout.map(|timeout| {
            monotonic_clock::subscribe_duration(timeout.as_nanos().try_into().unwrap())
        });

        let mut pollables = states
            .iter()
            .map(|(pollable, ..)| pollable.deref())
            .chain(&timeout)
            .collect::<Vec<_>>();

        #[cfg(debug_assertions)]
        if pollables.is_empty() {
            warn!("calling mio::Poll::poll with empty pollables; this likely not what you want");
        }

        for index in poll::poll(&pollables) {
            let index = usize::try_from(index).unwrap();
            if timeout.is_none() || index != pollables.len() - 1 {
                let (_, fd, socket, subscription, interests) = &states[index];

                let mut push_event = || {
                    events.push(Event {
                        token: subscription.token,
                        interests: *interests,
                    })
                };

                if socket.state.tag == libc::tcp_socket_state_tag_t::TCP_SOCKET_STATE_CONNECTING {
                    let socket_resource =
                        ManuallyDrop::new(unsafe { TcpSocket::from_handle(socket.socket) });

                    let socket_ptr = || unsafe {
                        let mut entry_ref = ptr::null_mut();
                        if libc::descriptor_table_get_ref(*fd, &mut entry_ref) {
                            &mut (*entry_ref).value.tcp_socket
                        } else {
                            unreachable!();
                        }
                    };

                    match socket_resource.finish_connect() {
                        Ok((rx, tx)) => {
                            let socket_ptr = socket_ptr();
                            socket_ptr.state = libc::tcp_socket_state_t {
                                tag: libc::tcp_socket_state_tag_t::TCP_SOCKET_STATE_CONNECTED,
                                value: libc::tcp_socket_state_value_t {
                                    connected: libc::tcp_socket_state_connected_t {
                                        input_pollable: rx.subscribe().take_handle(),
                                        input: rx.take_handle(),
                                        output_pollable: tx.subscribe().take_handle(),
                                        output: tx.take_handle(),
                                    },
                                },
                            };
                            push_event();
                        }
                        Err(ErrorCode::WouldBlock) => {}
                        Err(error) => {
                            let socket_ptr = socket_ptr();
                            socket_ptr.state = libc::tcp_socket_state_t {
                                tag: libc::tcp_socket_state_tag_t::TCP_SOCKET_STATE_CONNECT_FAILED,
                                value: libc::tcp_socket_state_value_t {
                                    connect_failed: libc::tcp_socket_state_connect_failed_t {
                                        error_code: error as u8,
                                    },
                                },
                            };
                            push_event();
                        }
                    }
                } else {
                    // Emulate edge-triggering by deregistering interest in `interests`; `IoSourceState::do_io`
                    // will re-register if/when appropriate.
                    let fd_interests = &mut subscriptions.get_mut(fd).unwrap().interests;
                    *fd_interests = (*fd_interests).and_then(|v| v.remove(*interests));
                    push_event();
                }
            }
        }

        Ok(())
    }
}

pub(crate) type Events = Vec<Event>;

#[derive(Debug, Copy, Clone)]
pub(crate) struct Event {
    token: Token,
    interests: Interest,
}

pub(crate) mod event {
    use std::fmt;

    use crate::sys::Event;
    use crate::Token;

    pub(crate) fn token(event: &Event) -> Token {
        event.token
    }

    pub(crate) fn is_readable(event: &Event) -> bool {
        event.interests.is_readable()
    }

    pub(crate) fn is_writable(event: &Event) -> bool {
        event.interests.is_writable()
    }

    pub(crate) fn is_error(_: &Event) -> bool {
        false
    }

    pub(crate) fn is_read_closed(_: &Event) -> bool {
        false
    }

    pub(crate) fn is_write_closed(_: &Event) -> bool {
        false
    }

    pub(crate) fn is_priority(_: &Event) -> bool {
        // Not supported.
        false
    }

    pub(crate) fn is_aio(_: &Event) -> bool {
        // Not supported.
        false
    }

    pub(crate) fn is_lio(_: &Event) -> bool {
        // Not supported.
        false
    }

    pub(crate) fn debug_details(f: &mut fmt::Formatter<'_>, event: &Event) -> fmt::Result {
        use std::fmt::Debug;
        event.fmt(f)
    }
}

cfg_os_poll! {
    cfg_io_source! {
        struct Registration {
            subscriptions: Arc<Mutex<HashMap<RawFd, Subscription>>>,
            token: Token,
            interests: Interest,
            fd: RawFd,
        }

        pub(crate) struct IoSourceState {
            registration: Option<Registration>
        }

        impl IoSourceState {
            pub(crate) fn new() -> Self {
                IoSourceState { registration: None }
            }

            pub(crate) fn do_io<T, F, R>(&self, f: F, io: &T) -> io::Result<R>
            where
                F: FnOnce(&T) -> io::Result<R>,
            {
                let result = f(io);

                self.registration.as_ref().map(|registration| {
                    *registration.subscriptions.lock().unwrap().get_mut(&registration.fd).unwrap() =
                        Subscription {
                            token: registration.token,
                            interests: Some(registration.interests)
                        };
                });

                result
            }

            pub fn register(
                &mut self,
                registry: &Registry,
                token: Token,
                interests: Interest,
                fd: RawFd,
            ) -> io::Result<()> {
                if self.registration.is_some() {
                    Err(io::ErrorKind::AlreadyExists.into())
                } else {
                    let subscriptions = registry.selector().subscriptions.clone();
                    subscriptions.lock().unwrap().insert(fd, Subscription { token, interests: Some(interests) });
                    self.registration = Some(Registration {
                        subscriptions, token, interests, fd
                    });
                    Ok(())
                }
            }

            pub fn reregister(
                &mut self,
                _registry: &Registry,
                token: Token,
                interests: Interest,
                fd: RawFd,
            ) -> io::Result<()> {
                if let Some(registration) = &self.registration {
                    *registration.subscriptions.lock().unwrap().get_mut(&fd).unwrap() = Subscription {
                        token,
                        interests: Some(interests)
                    };
                    Ok(())
                } else {
                    Err(io::ErrorKind::NotFound.into())
                }
            }

            pub fn deregister(&mut self, _registry: &Registry, fd: RawFd) -> io::Result<()> {
                if let Some(registration) = self.registration.take() {
                    registration.subscriptions.lock().unwrap().remove(&fd);
                }
                Ok(())
            }
        }

        impl Drop for IoSourceState {
            fn drop(&mut self) {
                if let Some(registration) = self.registration.take() {
                    registration.subscriptions.lock().unwrap().remove(&registration.fd);
                }
            }
        }
    }
}

/// Create a new non-blocking socket.
#[cfg(feature = "net")]
pub(crate) fn new_socket(domain: c_int, socket_type: c_int) -> io::Result<c_int> {
    let socket_type = socket_type | libc::SOCK_NONBLOCK;

    let socket = syscall!(socket(domain, socket_type, 0))?;

    Ok(socket)
}

#[cfg(feature = "net")]
#[repr(C)]
pub(crate) union SocketAddrCRepr {
    v4: libc::sockaddr_in,
    v6: libc::sockaddr_in6,
}

#[cfg(feature = "net")]
impl SocketAddrCRepr {
    pub(crate) fn as_ptr(&self) -> *const libc::sockaddr {
        self as *const _ as *const libc::sockaddr
    }
}

/// Converts a Rust `SocketAddr` into the system representation.
#[cfg(feature = "net")]
pub(crate) fn socket_addr(addr: &SocketAddr) -> (SocketAddrCRepr, libc::socklen_t) {
    match addr {
        SocketAddr::V4(ref addr) => {
            // `s_addr` is stored as BE on all machine and the array is in BE order.
            // So the native endian conversion method is used so that it's never swapped.
            let sin_addr = libc::in_addr {
                s_addr: u32::from_ne_bytes(addr.ip().octets()),
            };

            let sockaddr_in = libc::sockaddr_in {
                sin_family: libc::AF_INET as libc::sa_family_t,
                sin_port: addr.port().to_be(),
                sin_addr,
            };

            let sockaddr = SocketAddrCRepr { v4: sockaddr_in };
            let socklen = std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t;
            (sockaddr, socklen)
        }
        SocketAddr::V6(ref addr) => {
            let sockaddr_in6 = libc::sockaddr_in6 {
                sin6_family: libc::AF_INET6 as libc::sa_family_t,
                sin6_port: addr.port().to_be(),
                sin6_addr: libc::in6_addr {
                    s6_addr: addr.ip().octets(),
                },
                sin6_flowinfo: addr.flowinfo(),
                sin6_scope_id: addr.scope_id(),
            };

            let sockaddr = SocketAddrCRepr { v6: sockaddr_in6 };
            let socklen = std::mem::size_of::<libc::sockaddr_in6>() as libc::socklen_t;
            (sockaddr, socklen)
        }
    }
}

#[allow(non_camel_case_types, dead_code)]
mod netc {
    pub use libc::*;

    // TODO: This should be defined in `libc` but isn't as of v0.2.159:
    pub const SOCK_NONBLOCK: c_int = 0x4000;

    // The remainder of this module represents the non-public `wasi-libc` API
    // for converting between POSIX file descriptors and WASIp2 resource
    // handles.  Once https://github.com/WebAssembly/wasi-libc/issues/542 has
    // been addressed we'll be able to switch to a public API.

    #[repr(C)]
    #[derive(Copy, Clone, Eq, PartialEq, Debug)]
    pub enum descriptor_table_entry_tag_t {
        DESCRIPTOR_TABLE_ENTRY_TCP_SOCKET,
        DESCRIPTOR_TABLE_ENTRY_UDP_SOCKET,
    }

    #[repr(C)]
    #[derive(Copy, Clone, Eq, PartialEq, Debug)]
    pub enum tcp_socket_state_tag_t {
        TCP_SOCKET_STATE_UNBOUND,
        TCP_SOCKET_STATE_BOUND,
        TCP_SOCKET_STATE_CONNECTING,
        TCP_SOCKET_STATE_CONNECTED,
        TCP_SOCKET_STATE_CONNECT_FAILED,
        TCP_SOCKET_STATE_LISTENING,
    }

    #[repr(C)]
    #[derive(Copy, Clone)]
    pub struct tcp_socket_state_connected_t {
        pub input: u32,
        pub input_pollable: u32,
        pub output: u32,
        pub output_pollable: u32,
    }

    #[repr(C)]
    #[derive(Copy, Clone)]
    pub struct tcp_socket_state_connect_failed_t {
        pub error_code: u8,
    }

    #[repr(C)]
    #[derive(Copy, Clone)]
    pub union tcp_socket_state_value_t {
        pub connected: tcp_socket_state_connected_t,
        pub connect_failed: tcp_socket_state_connect_failed_t,
    }

    #[repr(C)]
    #[derive(Copy, Clone)]
    pub struct tcp_socket_state_t {
        pub tag: tcp_socket_state_tag_t,
        pub value: tcp_socket_state_value_t,
    }

    #[repr(C)]
    #[derive(Copy, Clone)]
    pub struct tcp_socket_t {
        pub socket: u32,
        pub socket_pollable: u32,
        pub blocking: bool,
        pub fake_nodelay: bool,
        pub family: u8,
        pub state: tcp_socket_state_t,
    }

    #[repr(C)]
    #[derive(Copy, Clone, Eq, PartialEq, Debug)]
    pub enum udp_socket_state_tag_t {
        UDP_SOCKET_STATE_UNBOUND,
        UDP_SOCKET_STATE_BOUND_NOSTREAMS,
        UDP_SOCKET_STATE_BOUND_STREAMING,
        UDP_SOCKET_STATE_CONNECTED,
    }

    #[repr(C)]
    #[derive(Copy, Clone)]
    pub struct udp_socket_streams_t {
        pub incoming: u32,
        pub incoming_pollable: u32,
        pub outgoing: u32,
        pub outgoing_pollable: u32,
    }

    #[repr(C)]
    #[derive(Copy, Clone)]
    pub struct udp_socket_state_bound_streaming_t {
        pub streams: udp_socket_streams_t,
    }

    #[repr(C)]
    #[derive(Copy, Clone)]
    pub struct udp_socket_state_connected_t {
        pub streams: udp_socket_streams_t,
    }

    #[repr(C)]
    #[derive(Copy, Clone)]
    pub union udp_socket_state_value_t {
        pub bound_streaming: udp_socket_state_bound_streaming_t,
        pub connected: udp_socket_state_connected_t,
    }

    #[repr(C)]
    #[derive(Copy, Clone)]
    pub struct udp_socket_state_t {
        pub tag: udp_socket_state_tag_t,
        pub value: udp_socket_state_value_t,
    }

    #[repr(C)]
    #[derive(Copy, Clone)]
    pub struct udp_socket_t {
        pub socket: u32,
        pub socket_pollable: u32,
        pub blocking: bool,
        pub family: u8,
        pub state: udp_socket_state_t,
    }

    #[repr(C)]
    #[derive(Copy, Clone)]
    pub union descriptor_table_entry_value_t {
        pub tcp_socket: tcp_socket_t,
        pub udp_socket: udp_socket_t,
    }

    #[repr(C)]
    #[derive(Copy, Clone)]
    pub struct descriptor_table_entry_t {
        pub tag: descriptor_table_entry_tag_t,
        pub value: descriptor_table_entry_value_t,
    }

    extern "C" {
        pub fn descriptor_table_get_ref(
            fd: c_int,
            entry: *mut *mut descriptor_table_entry_t,
        ) -> bool;
    }
}
