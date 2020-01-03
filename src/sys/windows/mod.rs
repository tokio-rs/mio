use std::pin::Pin;
use std::sync::{Arc, Mutex};

mod afd;
mod io_status_block;

pub mod event;
pub use event::{Event, Events};

mod selector;
pub use selector::{Selector, SelectorInner, SockState};

// Macros must be defined before the modules that use them
cfg_net! {
    /// Helper macro to execute a system call that returns an `io::Result`.
    //
    // Macro must be defined before any modules that uses them.
    macro_rules! syscall {
        ($fn: ident ( $($arg: expr),* $(,)* ), $err_test: path, $err_value: expr) => {{
            let res = unsafe { $fn($($arg, )*) };
            if $err_test(&res, &$err_value) {
                Err(io::Error::last_os_error())
            } else {
                Ok(res)
            }
        }};
    }
}

cfg_tcp! {
    pub(crate) mod tcp;
}

cfg_udp! {
    pub(crate) mod udp;
}

mod waker;
pub(crate) use waker::Waker;

pub trait SocketState {
    // The `SockState` struct needs to be pinned in memory because it contains
    // `OVERLAPPED` and `AFD_POLL_INFO` fields which are modified in the
    // background by the windows kernel, therefore we need to ensure they are
    // never moved to a different memory address.
    fn get_sock_state(&self) -> Option<Pin<Arc<Mutex<SockState>>>>;
    fn set_sock_state(&self, sock_state: Option<Pin<Arc<Mutex<SockState>>>>);
}

cfg_net! {
    use crate::{poll, Interest, Registry, Token};
    use std::io;
    use std::mem::size_of_val;
    use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
    use std::os::windows::io::RawSocket;
    use std::sync::Once;
    use winapi::ctypes::c_int;
    use winapi::shared::ws2def::SOCKADDR;
    use winapi::um::winsock2::{
        ioctlsocket, socket, FIONBIO, INVALID_SOCKET, PF_INET, PF_INET6, SOCKET,
    };

    struct InternalState {
        selector: Arc<SelectorInner>,
        token: Token,
        interests: Interest,
        sock_state: Option<Pin<Arc<Mutex<SockState>>>>,
    }

    impl Drop for InternalState {
        fn drop(&mut self) {
            if let Some(sock_state) = self.sock_state.as_ref() {
                let mut sock_state = sock_state.lock().unwrap();
                sock_state.mark_delete();
            }
        }
    }

    pub struct IoSourceState {
        // This is `None` if the socket has not yet been registered.
        //
        // We box the internal state to not increase the size on the stack as the
        // type might move around a lot.
        inner: Option<Box<InternalState>>,
    }

    impl IoSourceState {
        pub fn new() -> IoSourceState {
            IoSourceState { inner: None }
        }

        pub fn do_io<T, F, R>(&self, f: F, io: &T) -> io::Result<R>
        where
            F: FnOnce(&T) -> io::Result<R>,
        {
            let result = f(io);
            if let Err(ref e) = result {
                if e.kind() == io::ErrorKind::WouldBlock {
                    self.inner.as_ref().map_or(Ok(()), |state| {
                        // TODO: remove this unwrap once `InternalState.sock_state`
                        // no longer uses `Option`.
                        let sock_state = state.sock_state.as_ref().unwrap();
                        state
                            .selector
                            .reregister(sock_state, state.token, state.interests)
                    })?;
                }
            }
            result
        }

        pub fn register(
            &mut self,
            registry: &Registry,
            token: Token,
            interests: Interest,
            socket: RawSocket,
        ) -> io::Result<()> {
            if self.inner.is_some() {
                Err(io::Error::from(io::ErrorKind::AlreadyExists))
            } else {
                poll::selector(registry)
                    .register(socket, token, interests)
                    .map(|state| {
                        self.inner = Some(Box::new(state));
                    })
            }
        }

        pub fn reregister(
            &mut self,
            registry: &Registry,
            token: Token,
            interests: Interest,
        ) -> io::Result<()> {
            match self.inner.as_mut() {
                Some(state) => {
                    let sock_state = state.sock_state.as_ref().unwrap();
                    poll::selector(registry)
                        .reregister(sock_state, token, interests)
                        .map(|()| {
                            state.token = token;
                            state.interests = interests;
                        })
                }
                None => Err(io::Error::from(io::ErrorKind::NotFound)),
            }
        }

        pub fn deregister(&mut self) -> io::Result<()> {
            match self.inner.as_mut() {
                Some(state) => {
                    {
                        let sock_state = state.sock_state.as_ref().unwrap();
                        let mut sock_state = sock_state.lock().unwrap();
                        sock_state.mark_delete();
                    }
                    self.inner = None;
                    Ok(())
                }
                None => Err(io::Error::from(io::ErrorKind::NotFound)),
            }
        }
    }

    /// Initialise the network stack for Windows.
    fn init() {
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            // Let standard library call `WSAStartup` for us, we can't do it
            // ourselves because otherwise using any type in `std::net` would panic
            // when it tries to call `WSAStartup` a second time.
            drop(std::net::UdpSocket::bind("127.0.0.1:0"));
        });
    }

    /// Create a new non-blocking socket.
    fn new_socket(addr: SocketAddr, socket_type: c_int) -> io::Result<SOCKET> {
        let domain = match addr {
            SocketAddr::V4(..) => PF_INET,
            SocketAddr::V6(..) => PF_INET6,
        };

        syscall!(
            socket(domain, socket_type, 0),
            PartialEq::eq,
            INVALID_SOCKET
        )
        .and_then(|socket| {
            syscall!(ioctlsocket(socket, FIONBIO, &mut 1), PartialEq::ne, 0).map(|_| socket as SOCKET)
        })
    }

    fn socket_addr(addr: &SocketAddr) -> (*const SOCKADDR, c_int) {
        match addr {
            SocketAddr::V4(ref addr) => (
                addr as *const _ as *const SOCKADDR,
                size_of_val(addr) as c_int,
            ),
            SocketAddr::V6(ref addr) => (
                addr as *const _ as *const SOCKADDR,
                size_of_val(addr) as c_int,
            ),
        }
    }

    fn inaddr_any(other: SocketAddr) -> SocketAddr {
        match other {
            SocketAddr::V4(..) => {
                let any = Ipv4Addr::new(0, 0, 0, 0);
                let addr = SocketAddrV4::new(any, 0);
                SocketAddr::V4(addr)
            }
            SocketAddr::V6(..) => {
                let any = Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0);
                let addr = SocketAddrV6::new(any, 0, 0, 0);
                SocketAddr::V6(addr)
            }
        }
    }
}
