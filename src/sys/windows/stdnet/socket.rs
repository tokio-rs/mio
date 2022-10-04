use std::cmp::min;
use std::convert::TryInto;
use std::io::{self, IoSlice, IoSliceMut};
use std::mem;
use std::net::Shutdown;
use std::os::raw::c_int;
use std::os::windows::io::{AsRawSocket, FromRawSocket, IntoRawSocket, RawSocket};
use std::ptr;

use windows_sys::Win32::Networking::WinSock::{self, closesocket, SOCKET, SOCKET_ERROR, WSABUF};

/// Maximum size of a buffer passed to system call like `recv` and `send`.
const MAX_BUF_LEN: usize = c_int::MAX as usize;

#[derive(Debug)]
pub(crate) struct Socket(SOCKET);

impl Socket {
    pub fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        let ret = wsa_syscall!(
            recv(self.0, buf.as_mut_ptr() as *mut _, buf.len() as c_int, 0,),
            SOCKET_ERROR
        )?;
        Ok(ret as usize)
    }

    pub fn recv_vectored(&self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        let mut total = 0;
        let mut flags: u32 = 0;
        let bufs = unsafe { &mut *(bufs as *mut [IoSliceMut<'_>] as *mut [WSABUF]) };
        let res = wsa_syscall!(
            WSARecv(
                self.0,
                bufs.as_mut_ptr().cast(),
                min(bufs.len(), u32::MAX as usize) as u32,
                &mut total,
                &mut flags,
                ptr::null_mut(),
                None,
            ),
            SOCKET_ERROR
        );
        match res {
            Ok(_) => Ok(total as usize),
            Err(ref err) if err.raw_os_error() == Some(WinSock::WSAESHUTDOWN as i32) => Ok(0),
            Err(err) => Err(err),
        }
    }

    pub fn send(&self, buf: &[u8]) -> io::Result<usize> {
        wsa_syscall!(
            send(
                self.0,
                buf.as_ptr().cast(),
                min(buf.len(), MAX_BUF_LEN) as c_int,
                0,
            ),
            SOCKET_ERROR
        )
        .map(|n| n as usize)
    }

    pub fn send_vectored(&self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        let mut total = 0;
        wsa_syscall!(
            WSASend(
                self.0,
                // FIXME: From the `WSASend` docs [1]:
                // > For a Winsock application, once the WSASend function is called,
                // > the system owns these buffers and the application may not
                // > access them.
                //
                // So what we're doing is actually UB as `bufs` needs to be `&mut
                // [IoSlice<'_>]`.
                //
                // See: https://github.com/rust-lang/socket2-rs/issues/129.
                //
                // [1] https://docs.microsoft.com/en-us/windows/win32/api/winsock2/nf-winsock2-wsasend
                bufs.as_ptr() as *mut _,
                min(bufs.len(), u32::MAX as usize) as u32,
                &mut total,
                0,
                std::ptr::null_mut(),
                None,
            ),
            SOCKET_ERROR
        )
        .map(|_| total as usize)
    }

    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        let how = match how {
            Shutdown::Write => WinSock::SD_SEND,
            Shutdown::Read => WinSock::SD_RECEIVE,
            Shutdown::Both => WinSock::SD_BOTH,
        };
        wsa_syscall!(shutdown(self.0, how.try_into().unwrap()), SOCKET_ERROR)?;
        Ok(())
    }

    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        let mut val: mem::MaybeUninit<c_int> = mem::MaybeUninit::uninit();
        let mut len = mem::size_of::<c_int>() as i32;
        wsa_syscall!(
            getsockopt(
                self.0 as _,
                WinSock::SOL_SOCKET.try_into().unwrap(),
                WinSock::SO_ERROR.try_into().unwrap(),
                &mut val as *mut _ as *mut _,
                &mut len,
            ),
            SOCKET_ERROR
        )?;
        assert_eq!(len as usize, mem::size_of::<c_int>());
        let val = unsafe { val.assume_init() };
        if val == 0 {
            Ok(None)
        } else {
            Ok(Some(io::Error::from_raw_os_error(val as i32)))
        }
    }
}

cfg_os_poll! {
    use windows_sys::Win32::Networking::WinSock::{INVALID_SOCKET, SOCKADDR};
    use super::init;

    impl Socket {
        pub fn new() -> io::Result<Socket> {
            init();
            wsa_syscall!(
                WSASocketW(
                    WinSock::AF_UNIX.into(),
                    WinSock::SOCK_STREAM.into(),
                    0,
                    ptr::null_mut(),
                    0,
                    WinSock::WSA_FLAG_OVERLAPPED | WinSock::WSA_FLAG_NO_HANDLE_INHERIT,
                ),
                INVALID_SOCKET
            ).map(Socket)
        }

        pub fn accept(&self, storage: *mut SOCKADDR, len: *mut c_int) -> io::Result<Socket> {
            // WinSock's accept returns a socket with the same properties as the listener.  it is
            // called on. In particular, the WSA_FLAG_NO_HANDLE_INHERIT will be inherited from the
            // listener.
            wsa_syscall!(accept(self.0, storage, len), INVALID_SOCKET).map(Socket)
        }

        pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
            let mut nonblocking = if nonblocking { 1 } else { 0 };
            wsa_syscall!(
                ioctlsocket(self.0, WinSock::FIONBIO, &mut nonblocking),
                SOCKET_ERROR
            )?;
            Ok(())
        }
    }
}

impl Drop for Socket {
    fn drop(&mut self) {
        let _ = unsafe { closesocket(self.0) };
    }
}

impl AsRawSocket for Socket {
    fn as_raw_socket(&self) -> RawSocket {
        self.0 as RawSocket
    }
}

impl FromRawSocket for Socket {
    unsafe fn from_raw_socket(sock: RawSocket) -> Self {
        Socket(sock as SOCKET)
    }
}

impl IntoRawSocket for Socket {
    fn into_raw_socket(self) -> RawSocket {
        let ret = self.0 as RawSocket;
        mem::forget(self);
        ret
    }
}
