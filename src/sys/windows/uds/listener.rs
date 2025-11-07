use super::{socketaddr_un, startup, wsa_error, Socket, SocketAddr, UnixStream};
use std::{
    io, ops::{Deref, DerefMut}, os::windows::io::{AsRawSocket, RawSocket}, path::Path
};
use windows_sys::Win32::Networking::WinSock::{self, SOCKADDR_UN, SOCKET_ERROR};
#[derive(Debug)]
pub struct UnixListener(Socket);

impl UnixListener {
    pub fn bind<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        unsafe {
            startup()?;
            let s = Socket::new()?;
            let (addr, len) = socketaddr_un(path.as_ref())?;
            if WinSock::bind(s.0, &addr as *const _ as *const _, len) == SOCKET_ERROR {
                Err(wsa_error())
            } else {
                match WinSock::listen(s.0, 5) {
                    SOCKET_ERROR => Err(wsa_error()),
                    _ => Ok(Self(s)),
                }
            }
        }
    }
    pub fn bind_addr(socket_addr: &SocketAddr) -> io::Result<Self> {
        unsafe {
            let s = Socket::new()?;
            if WinSock::bind(
                s.0,
                &socket_addr.addr as *const _ as *const _,
                socket_addr.addrlen,
            ) == SOCKET_ERROR
            {
                Err(wsa_error())
            } else {
                match WinSock::listen(s.0, 5) {
                    SOCKET_ERROR => Err(wsa_error()),
                    _ => Ok(Self(s)),
                }
            }
        }
    }
    pub fn accept(&self) -> io::Result<(UnixStream, SocketAddr)> {
        let mut addr = SOCKADDR_UN::default();
        let mut addrlen = size_of::<SOCKADDR_UN>() as _;
        let s = self
            .0
            .accept(&mut addr as *mut _ as *mut _, &mut addrlen as *mut _)?;
        Ok((UnixStream(s), SocketAddr { addr, addrlen }))
    }
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.0.local_addr()
    }
    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.0.take_error()
    }
}
impl Deref for UnixListener {
    type Target = Socket;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for UnixListener {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
pub fn bind_addr(socket_addr: &SocketAddr) -> io::Result<UnixListener> {
    UnixListener::bind_addr(socket_addr)
}
pub fn accept(s: &UnixListener) -> io::Result<(crate::net::UnixStream, SocketAddr)> {
    let (inner, addr) = s.accept()?;
    Ok((crate::net::UnixStream::from_std(inner), addr))
}

impl AsRawSocket for UnixListener {
    fn as_raw_socket(&self) -> RawSocket {
        self.0.0 as _
    }
}