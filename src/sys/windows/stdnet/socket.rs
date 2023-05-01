use std::cmp::min;
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
            Err(ref err) if err.raw_os_error() == Some(WinSock::WSAESHUTDOWN) => Ok(0),
            Err(err) => Err(err),
        }
    }

    pub fn write(&self, buf: &[u8]) -> io::Result<usize> {
        let response = unsafe {
            windows_sys::Win32::Networking::WinSock::send(
                self.0,
                buf.as_ptr().cast(),
                min(buf.len(), MAX_BUF_LEN) as c_int,
                0,
            )
        };
        if response == SOCKET_ERROR {
            return match unsafe { windows_sys::Win32::Networking::WinSock::WSAGetLastError() } {
                windows_sys::Win32::Networking::WinSock::WSAESHUTDOWN => {
                    Err(io::Error::new(io::ErrorKind::BrokenPipe, "brokenpipe"))
                }
                e => Err(std::io::Error::from_raw_os_error(e)),
            };
        }
        Ok(response as usize)
    }

    pub fn write_vectored(&self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        let mut total = 0;
        wsa_syscall!(
            WSASend(
                self.0,
                bufs.as_ptr() as *mut WSABUF,
                bufs.len().min(u32::MAX as usize) as u32,
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
        wsa_syscall!(shutdown(self.0, how), SOCKET_ERROR)?;
        Ok(())
    }

    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        let mut val: mem::MaybeUninit<c_int> = mem::MaybeUninit::uninit();
        let mut len = mem::size_of::<c_int>() as i32;
        wsa_syscall!(
            getsockopt(
                self.0 as _,
                WinSock::SOL_SOCKET,
                WinSock::SO_ERROR,
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
            Ok(Some(io::Error::from_raw_os_error(val)))
        }
    }
}

cfg_os_poll! {
    use windows_sys::Win32::Foundation::{HANDLE, HANDLE_FLAG_INHERIT, SetHandleInformation};
    use windows_sys::Win32::Networking::WinSock::{INVALID_SOCKET, SOCKADDR};
    use super::init;

    impl Socket {
        pub fn new() -> io::Result<Socket> {
            init();
            match wsa_syscall!(WSASocketW(
                WinSock::AF_UNIX.into(),
                WinSock::SOCK_STREAM,
                0,
                ptr::null_mut(),
                0,
                WinSock::WSA_FLAG_OVERLAPPED,
            ), INVALID_SOCKET) {
                Ok(res) => {
                    let socket = Socket(res);
                    socket.set_no_inherit()?;
                    Ok(socket)
                },
                Err(e) => Err(e),
            }
        }

        pub fn accept(&self, storage: *mut SOCKADDR, len: *mut c_int) -> io::Result<Socket> {
            // WinSock's accept returns a socket with the same properties as the listener.  it is
            // called on. In particular, the WSA_FLAG_NO_HANDLE_INHERIT will be inherited from the
            // listener.
            match wsa_syscall!(accept(self.0, storage, len), INVALID_SOCKET) {
                Ok(res) => {
                    let socket = Socket(res);
                    socket.set_no_inherit()?;
                    Ok(socket)
                },
                Err(e) => Err(e),
            }
        }

        pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
            let mut nonblocking = if nonblocking { 1 } else { 0 };
            wsa_syscall!(
                ioctlsocket(self.0, WinSock::FIONBIO, &mut nonblocking),
                SOCKET_ERROR
            )?;
            Ok(())
        }

        pub fn set_no_inherit(&self) -> io::Result<()> {
            syscall!(SetHandleInformation(self.0 as HANDLE, HANDLE_FLAG_INHERIT, 0), PartialEq::eq, -1)?;
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
