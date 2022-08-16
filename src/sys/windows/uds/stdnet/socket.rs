use std::io::{self, IoSlice, IoSliceMut};
use std::convert::TryInto;
use std::mem;
use std::net::Shutdown;
use std::os::raw::{c_int, c_ulong};
use std::os::windows::io::{AsRawSocket, FromRawSocket, IntoRawSocket, RawSocket};
use std::ptr;
use std::time::Duration;

use windows_sys::Win32::Foundation::{
    HANDLE,
    SetHandleInformation,
    HANDLE_FLAG_INHERIT
};
use windows_sys::Win32::System::Threading::GetCurrentProcessId;
use windows_sys::Win32::System::WindowsProgramming::INFINITE;
use windows_sys::Win32::Networking::WinSock::{INVALID_SOCKET, SOCKADDR, SOCKET, SOCKET_ERROR, SOCK_STREAM, SOL_SOCKET, SO_ERROR, WSADuplicateSocketW, WSAPROTOCOL_INFOW, WSASocketW, accept, closesocket, getsockopt as c_getsockopt, ioctlsocket, recv, send, setsockopt as c_setsockopt, shutdown};

#[derive(Debug)]
pub struct Socket(SOCKET);

impl Socket {
    pub fn new() -> io::Result<Socket> {
        let socket = wsa_syscall!(
            WSASocketW(
                WinSock::AF_UNIX.into(),
                WinSock::SOCK_STREAM.into(),
                0,
                ptr::null_mut(),
                0,
                WinSock::WSA_FLAG_OVERLAPPED,
            ),
            PartialEq::eq,
            INVALID_SOCKET
        )?;
        let socket = Socket(socket);
        socket.set_no_inherit()?;
        Ok(socket)
    }

    pub fn accept(&self, storage: *mut SOCKADDR, len: *mut c_int) -> io::Result<Socket> {
        let socket = wsa_syscall!(
            accept(self.0, storage, len),
            PartialEq::eq,
            INVALID_SOCKET
        )?;
        let socket = Socket(socket);
        socket.set_no_inherit()?;
        Ok(socket)
    }

    pub fn duplicate(&self) -> io::Result<Socket> {
        let mut info: WSAPROTOCOL_INFOW = unsafe { mem::zeroed() };
        wsa_syscall!(
            WSADuplicateSocketW(
                self.0,
                GetCurrentProcessId(),
                &mut info,
            ),
            PartialEq::eq,
            SOCKET_ERROR
        )?;
        let n = wsa_syscall!(
            WSASocketW(
                info.iAddressFamily,
                info.iSocketType,
                info.iProtocol,
                &mut info,
                0,
                WSA_FLAG_OVERLAPPED,
            ),
            PartialEq::eq,
            INVALID_SOCKET
        )?;
        let socket = Socket(n);
        socket.set_no_inherit()?;
        Ok(socket)
    }

    fn recv_with_flags(&self, buf: &mut [u8], flags: c_int) -> io::Result<usize> {
        let ret = wsa_syscall!(
            recv(
                self.0,
                buf.as_mut_ptr() as *mut _,
                buf.len() as c_int,
                flags,
            ),
            PartialEq::eq,
            SOCKET_ERROR
        )?;
        Ok(ret as usize)
    }

    pub fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        self.recv_with_flags(buf, 0)
    }

    pub fn read_vectored(&self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        let mut total = 0;
        for slice in &mut *bufs {
            let wsa_buf = unsafe { *(slice as *const _ as *const WinSock::WSABUF) };
            let len = wsa_buf.len;
            let buf = unsafe { std::slice::from_raw_parts_mut(wsa_buf.buf, len.try_into().unwrap()) };
            total += self.recv_with_flags(buf, 0)?;
        }
        println!("Wrote vectored: {total:?}, {bufs:?}");
        Ok(total as usize)
    }

    pub fn write(&self, buf: &[u8]) -> io::Result<usize> {
        let ret = wsa_syscall!(
            send(self.0, buf as *const _ as *const _, buf.len() as c_int, 0),
            PartialEq::eq,
            SOCKET_ERROR
        )?;
        Ok(ret as usize)
    }

    pub fn write_vectored(&self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        let mut total = 0;
        for slice in bufs {
            let wsa_buf = unsafe { *(slice as *const _ as *const WinSock::WSABUF) };
            let len = wsa_buf.len;
            let buf = unsafe { std::slice::from_raw_parts(wsa_buf.buf, len.try_into().unwrap()) };
            dbg!(buf);
            let ret = wsa_syscall!(
                send(self.0, buf as *const _ as *const _, len as c_int, 0),
                PartialEq::eq,
                SOCKET_ERROR
            )?;
            total += ret;
        }
        println!("Wrote vectored: {total:?}, {bufs:?}");
        Ok(total as usize)
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
            SO_ERROR.try_into().unwrap()
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

pub fn setsockopt<T>(sock: &Socket, opt: c_int, val: c_int, payload: T) -> io::Result<()> {
    let payload = &payload as *const T as *const _;
    wsa_syscall!(
        c_setsockopt(
            sock.as_raw_socket() as usize,
            opt,
            val,
            payload,
            mem::size_of::<T>() as i32,
        ),
        PartialEq::eq,
        SOCKET_ERROR
    )?;
    Ok(())
}

pub fn getsockopt<T: Copy>(sock: &Socket, opt: c_int, val: c_int) -> io::Result<T> {
    let mut slot: T = unsafe { mem::zeroed() };
    let mut len = mem::size_of::<T>() as i32;
    wsa_syscall!(
        c_getsockopt(
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