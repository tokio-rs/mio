pub(crate) use super::stdnet::SocketAddr;

cfg_os_poll! {
    use std::convert::TryInto;
    use windows_sys::Win32::Networking::WinSock::SOCKET_ERROR;
    use std::os::windows::io::RawSocket;
    use std::io;

    pub(crate) mod listener;
    pub(crate) mod stream;

    pub(crate) fn local_addr(socket: RawSocket) -> io::Result<SocketAddr> {
        SocketAddr::new(|sockaddr, socklen| {
            wsa_syscall!(
                getsockname(socket.try_into().unwrap(), sockaddr, socklen),
                SOCKET_ERROR
            )
        })
    }

    pub(crate) fn peer_addr(socket: RawSocket) -> io::Result<SocketAddr> {
        SocketAddr::new(|sockaddr, socklen| {
            wsa_syscall!(
                getpeername(socket.try_into().unwrap(), sockaddr, socklen),
                SOCKET_ERROR
            )
        })
    }
}
