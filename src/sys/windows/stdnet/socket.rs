use std::cmp::min;
use std::convert::TryInto;
use std::io::{self, IoSlice, IoSliceMut};
use std::mem;
use std::net::Shutdown;
use std::os::raw::{c_int, c_ulong};
use std::os::windows::io::{AsRawSocket, FromRawSocket, IntoRawSocket, RawSocket};
use std::ptr;
use std::time::Duration;

use super::init;
use windows_sys::Win32::Foundation::{SetHandleInformation, HANDLE, HANDLE_FLAG_INHERIT};
use windows_sys::Win32::Networking::WinSock::{
    self, closesocket, INVALID_SOCKET, SOCKADDR, SOCKET, SOCKET_ERROR, SOL_SOCKET, SO_ERROR,
    WSABUF, WSAESHUTDOWN,
};
use windows_sys::Win32::System::Threading::GetCurrentProcessId;
use windows_sys::Win32::System::WindowsProgramming::INFINITE;

/// Maximum size of a buffer passed to system call like `recv` and `send`.
const MAX_BUF_LEN: usize = c_int::MAX as usize;

#[derive(Debug)]
pub struct Socket(SOCKET);

impl Socket {
    pub fn new() -> io::Result<Socket> {
        init();
        let socket = wsa_syscall!(
            WSASocketW(
                WinSock::AF_UNIX.into(),
                WinSock::SOCK_STREAM.into(),
                0,
                ptr::null_mut(),
                0,
                WinSock::WSA_FLAG_OVERLAPPED | WinSock::WSA_FLAG_NO_HANDLE_INHERIT,
            ),
            PartialEq::eq,
            INVALID_SOCKET
        )?;
        Ok(Socket(socket))
    }

    pub fn accept(&self, storage: *mut SOCKADDR, len: *mut c_int) -> io::Result<Socket> {
        let socket = wsa_syscall!(accept(self.0, storage, len), PartialEq::eq, INVALID_SOCKET)?;
        let socket = Socket(socket);
        socket.set_no_inherit()?;
        Ok(socket)
    }

    pub fn duplicate(&self) -> io::Result<Socket> {
        let mut info: WinSock::WSAPROTOCOL_INFOW = unsafe { mem::zeroed() };
        wsa_syscall!(
            WSADuplicateSocketW(self.0, GetCurrentProcessId(), &mut info,),
            PartialEq::eq,
            SOCKET_ERROR
        )?;
        let socket = wsa_syscall!(
            WSASocketW(
                info.iAddressFamily,
                info.iSocketType,
                info.iProtocol,
                &info,
                0,
                WinSock::WSA_FLAG_OVERLAPPED | WinSock::WSA_FLAG_NO_HANDLE_INHERIT,
            ),
            PartialEq::eq,
            INVALID_SOCKET
        )?;
        Ok(Socket(socket))
    }

    pub fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        let ret = wsa_syscall!(
            recv(self.0, buf.as_mut_ptr() as *mut _, buf.len() as c_int, 0,),
            PartialEq::eq,
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
            PartialEq::eq,
            SOCKET_ERROR
        );
        match res {
            Ok(_) => Ok(total as usize),
            Err(ref err) if err.raw_os_error() == Some(WSAESHUTDOWN as i32) => Ok(0),
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
            PartialEq::eq,
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
            PartialEq::eq,
            SOCKET_ERROR
        )
        .map(|_| total as usize)
    }

    fn set_no_inherit(&self) -> io::Result<()> {
        syscall!(
            SetHandleInformation(self.0 as HANDLE, HANDLE_FLAG_INHERIT, 0),
            PartialEq::eq,
            0
        )?;
        Ok(())
    }

    pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        let mut nonblocking: c_ulong = if nonblocking { 1 } else { 0 };
        wsa_syscall!(
            ioctlsocket(self.0, WinSock::FIONBIO, &mut nonblocking),
            PartialEq::eq,
            SOCKET_ERROR
        )?;
        Ok(())
    }

    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        let how = match how {
            Shutdown::Write => WinSock::SD_SEND,
            Shutdown::Read => WinSock::SD_RECEIVE,
            Shutdown::Both => WinSock::SD_BOTH,
        };
        wsa_syscall!(
            shutdown(self.0, how.try_into().unwrap()),
            PartialEq::eq,
            SOCKET_ERROR
        )?;
        Ok(())
    }

    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        let raw = getsockopt::<c_int>(
            self,
            SOL_SOCKET.try_into().unwrap(),
            SO_ERROR.try_into().unwrap(),
        )?;
        if raw == 0 {
            Ok(None)
        } else {
            Ok(Some(io::Error::from_raw_os_error(raw as i32)))
        }
    }

    pub fn set_timeout(&self, dur: Option<Duration>, kind: c_int) -> io::Result<()> {
        let timeout = match dur {
            Some(dur) => {
                let timeout = dur2timeout(dur);
                if timeout == 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "cannot set a 0 duration timeout",
                    ));
                }
                timeout
            }
            None => 0,
        };
        setsockopt(self, SOL_SOCKET.try_into().unwrap(), kind, timeout)
    }

    pub fn timeout(&self, kind: c_int) -> io::Result<Option<Duration>> {
        let raw: u32 = getsockopt(self, SOL_SOCKET.try_into().unwrap(), kind)?;
        if raw == 0 {
            Ok(None)
        } else {
            let secs = raw / 1000;
            let nsec = (raw % 1000) * 1000000;
            Ok(Some(Duration::new(secs as u64, nsec as u32)))
        }
    }
}

fn setsockopt<T>(sock: &Socket, opt: c_int, val: c_int, payload: T) -> io::Result<()> {
    wsa_syscall!(
        setsockopt(
            sock.as_raw_socket() as usize,
            opt,
            val,
            &payload as *const T as *const _,
            mem::size_of::<T>() as i32,
        ),
        PartialEq::eq,
        SOCKET_ERROR
    )?;
    Ok(())
}

fn getsockopt<T: Copy>(sock: &Socket, opt: c_int, val: c_int) -> io::Result<T> {
    let mut slot: T = unsafe { mem::zeroed() };
    let mut len = mem::size_of::<T>() as i32;
    wsa_syscall!(
        getsockopt(
            sock.as_raw_socket() as _,
            opt,
            val,
            &mut slot as *mut _ as *mut _,
            &mut len,
        ),
        PartialEq::eq,
        SOCKET_ERROR
    )?;
    assert_eq!(len as usize, mem::size_of::<T>());
    Ok(slot)
}

fn dur2timeout(dur: Duration) -> u32 {
    // Note that a duration is a (u64, u32) (seconds, nanoseconds) pair, and the
    // timeouts in windows APIs are typically u32 milliseconds. To translate, we
    // have two pieces to take care of:
    //
    // * Nanosecond precision is rounded up
    // * Greater than u32::MAX milliseconds (50 days) is rounded up to INFINITE
    //   (never time out).
    dur.as_secs()
        .checked_mul(1000)
        .and_then(|ms| ms.checked_add((dur.subsec_nanos() as u64) / 1_000_000))
        .and_then(|ms| {
            ms.checked_add(if dur.subsec_nanos() % 1_000_000 > 0 {
                1
            } else {
                0
            })
        })
        .map(|ms| {
            if ms > <u32>::max_value() as u64 {
                INFINITE
            } else {
                ms as u32
            }
        })
        .unwrap_or(INFINITE)
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
