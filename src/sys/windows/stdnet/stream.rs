use std::io::{self, IoSlice, IoSliceMut};
use std::net::Shutdown;
use std::os::windows::io::{AsRawSocket, FromRawSocket, IntoRawSocket, RawSocket};
use std::{fmt, mem};

use windows_sys::Win32::Networking::WinSock::SOCKET_ERROR;

use super::{socket::Socket, SocketAddr};

pub(crate) struct UnixStream(pub(super) Socket);

impl fmt::Debug for UnixStream {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut builder = fmt.debug_struct("UnixStream");
        builder.field("socket", &self.0.as_raw_socket());
        if let Ok(addr) = self.local_addr() {
            builder.field("local", &addr);
        }
        if let Ok(addr) = self.peer_addr() {
            builder.field("peer", &addr);
        }
        builder.finish()
    }
}

impl UnixStream {
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        SocketAddr::new(|addr, len| {
            wsa_syscall!(
                getsockname(self.0.as_raw_socket() as _, addr, len),
                SOCKET_ERROR
            )
        })
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        SocketAddr::new(|addr, len| {
            wsa_syscall!(
                getpeername(self.0.as_raw_socket() as _, addr, len),
                SOCKET_ERROR
            )
        })
    }

    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.0.take_error()
    }

    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        self.0.shutdown(how)
    }
}

impl io::Read for UnixStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        io::Read::read(&mut &*self, buf)
    }

    fn read_vectored(&mut self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        io::Read::read_vectored(&mut &*self, bufs)
    }
}

impl<'a> io::Read for &'a UnixStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.recv(buf)
    }

    fn read_vectored(&mut self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        self.0.recv_vectored(bufs)
    }
}

impl io::Write for UnixStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        io::Write::write(&mut &*self, buf)
    }

    fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        io::Write::write_vectored(&mut &*self, bufs)
    }

    fn flush(&mut self) -> io::Result<()> {
        io::Write::flush(&mut &*self)
    }
}

impl<'a> io::Write for &'a UnixStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.send(buf)
    }

    fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        self.0.send_vectored(bufs)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl AsRawSocket for UnixStream {
    fn as_raw_socket(&self) -> RawSocket {
        self.0.as_raw_socket()
    }
}

impl FromRawSocket for UnixStream {
    unsafe fn from_raw_socket(sock: RawSocket) -> Self {
        UnixStream(Socket::from_raw_socket(sock))
    }
}

impl IntoRawSocket for UnixStream {
    fn into_raw_socket(self) -> RawSocket {
        let ret = self.0.as_raw_socket();
        mem::forget(self);
        ret
    }
}

cfg_os_poll! {
    use std::path::{Path, PathBuf};
    use windows_sys::Win32::Foundation::STATUS_SUCCESS;
    use windows_sys::Win32::Networking::WinSock::WSAEINPROGRESS;
    use windows_sys::Win32::Security::Cryptography::{
        BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG,
    };

    use super::{socket_addr, UnixListener};

    impl UnixStream {
        pub fn connect<P: AsRef<Path>>(path: P) -> io::Result<UnixStream> {
            let inner = Socket::new()?;
            let (addr, len) = socket_addr(path.as_ref())?;

            match wsa_syscall!(
                connect(
                    inner.as_raw_socket() as _,
                    &addr as *const _ as *const _,
                    len as i32,
                ),
                SOCKET_ERROR
            ) {
                Ok(_) => {}
                Err(ref err) if err.raw_os_error() == Some(WSAEINPROGRESS) => {}
                Err(e) => return Err(e),
            }
            Ok(UnixStream(inner))
        }

        pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
            self.0.set_nonblocking(nonblocking)
        }

        pub fn pair() -> io::Result<(Self, Self)> {
            use std::sync::{Arc, RwLock};
            use std::thread::spawn;

            let file_path = temp_path(10)?;
            let a: Arc<RwLock<Option<io::Result<UnixStream>>>> = Arc::new(RwLock::new(None));
            let ul = UnixListener::bind(&file_path).unwrap();
            let server = {
                let a = a.clone();
                spawn(move || {
                    let mut store = a.write().unwrap();
                    let stream0 = ul.accept().map(|s| s.0);
                    *store = Some(stream0);
                })
            };
            let stream1 = UnixStream::connect(&file_path)?;
            server
                .join()
                .map_err(|_| io::Error::from(io::ErrorKind::ConnectionRefused))?;
            let stream0 = (*(a.write().unwrap())).take().unwrap()?;
            let _ = std::fs::remove_file(&file_path);
            Ok((stream0, stream1))
        }
    }

    fn sample_ascii_string(len: usize) -> io::Result<String> {
        const GEN_ASCII_STR_CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ\
                abcdefghijklmnopqrstuvwxyz\
                0123456789-_";
        let mut buf: Vec<u8> = vec![0; len];
        for chunk in buf.chunks_mut(u32::max_value() as usize) {
            syscall!(
                BCryptGenRandom(
                    0,
                    chunk.as_mut_ptr(),
                    chunk.len() as u32,
                    BCRYPT_USE_SYSTEM_PREFERRED_RNG,
                ),
                PartialEq::ne,
                STATUS_SUCCESS
            )?;
        }
        let result: String = buf
            .into_iter()
            .map(|r| {
                // We pick from 64=2^6 characters so we can use a simple bitshift.
                let idx = r >> (8 - 6);
                char::from(GEN_ASCII_STR_CHARSET[idx as usize])
            })
            .collect();
        Ok(result)
    }

    fn temp_path(len: usize) -> io::Result<PathBuf> {
        let dir = std::env::temp_dir();
        // Retry a few times in case of collisions
        for _ in 0..10 {
            let rand_str = sample_ascii_string(len)?;
            let filename = format!(".tmp-{rand_str}.socket");
            let path = dir.join(filename);
            if !path.exists() {
                return Ok(path);
            }
        }

        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "too many temporary files exist",
        ))
    }
}
