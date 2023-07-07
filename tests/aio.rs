#![cfg(all(
    not(mio_unsupported_force_poll_poll),
    any(target_os = "freebsd", target_os = "dragonfly"),
))]
#![cfg(all(feature = "os-poll", feature = "net"))]

use mio::{event::Source, Events, Interest, Poll, Registry, Token};
use std::{
    fs::File,
    io, mem,
    os::unix::io::{AsRawFd, RawFd},
    pin::Pin,
    ptr,
};

mod util;
use util::{expect_events, expect_no_events, init, temp_file, ExpectEvent};

const UDATA: Token = Token(0xdead_beef);

/// A highly feature-incomplete POSIX AIO event source, suitable for testing
/// mio's handling of kqueue's EVFILT_AIO.
struct Aiocb(Pin<Box<libc::aiocb>>);

impl Aiocb {
    /// Constructs a new `Aiocb` with no associated data.
    ///
    /// The resulting `Aiocb` structure is suitable for use with `aio_fsync`
    pub fn from_fd(fd: RawFd) -> Aiocb {
        // Use mem::zeroed instead of explicitly zeroing each field, because the
        // number and name of reserved fields is OS-dependent.  On some OSes,
        // some reserved fields are used the kernel for state, and must be
        // explicitly zeroed when allocated.
        let mut inner = unsafe { mem::zeroed::<libc::aiocb>() };
        inner.aio_fildes = fd;
        inner.aio_sigevent.sigev_notify = libc::SIGEV_NONE;
        Aiocb(Box::pin(inner))
    }

    /// Constructs a new `Aiocb` suitable for writing to offset 0 of a file.
    #[cfg(target_os = "freebsd")]
    pub fn from_slice(fd: RawFd, buf: &[u8]) -> Aiocb {
        let mut aiocb = Aiocb::from_fd(fd);
        aiocb.0.aio_nbytes = buf.len();
        aiocb.0.aio_buf = buf.as_ptr() as *mut libc::c_void;
        aiocb
    }

    pub fn fsync(&mut self) -> io::Result<()> {
        unsafe {
            // Safe because we don't move the libc::aiocb
            let selfp = self.0.as_mut().get_unchecked_mut();
            match libc::aio_fsync(libc::O_SYNC, selfp) {
                0 => Ok(()),
                _ => Err(io::Error::last_os_error()),
            }
        }
    }
}

impl Source for Aiocb {
    fn register(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> io::Result<()> {
        assert!(interests.is_aio());
        let udata = usize::from(token);
        let kq = registry.as_raw_fd();
        self.0.aio_sigevent.sigev_notify = libc::SIGEV_KEVENT;
        self.0.aio_sigevent.sigev_signo = kq;
        self.0.aio_sigevent.sigev_value.sival_ptr = udata as *mut libc::c_void;
        self.0.aio_sigevent.sigev_notify_thread_id = 0;
        Ok(())
    }

    fn reregister(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> io::Result<()> {
        self.register(registry, token, interests)
    }

    fn deregister(&mut self, _registry: &Registry) -> io::Result<()> {
        self.0.aio_sigevent.sigev_notify = libc::SIGEV_NONE;
        self.0.aio_sigevent.sigev_value.sival_ptr = ptr::null_mut();
        Ok(())
    }
}

#[cfg(target_os = "freebsd")]
struct Liocb {
    _aiocbs: Box<[Aiocb]>,
    /// The actual list passed to `libc::lio_listio`.
    ///
    /// It must live for as long as any of the operations are still being
    /// processesed, because the aio subsystem uses its address as a unique
    /// identifier.
    list: Box<[*mut libc::aiocb]>,
    sev: libc::sigevent,
}

#[cfg(target_os = "freebsd")]
impl Liocb {
    fn listio(&mut self) -> io::Result<()> {
        unsafe {
            let r = libc::lio_listio(
                libc::LIO_NOWAIT,
                self.list.as_ptr(),
                self.list.len() as i32,
                &mut self.sev as *mut libc::sigevent,
            );
            match r {
                0 => Ok(()),
                _ => Err(io::Error::last_os_error()),
            }
        }
    }

    fn new(inputs: Vec<Aiocb>) -> Liocb {
        let mut aiocbs = inputs.into_boxed_slice();
        for aiocb in aiocbs.iter_mut() {
            aiocb.0.aio_lio_opcode = libc::LIO_WRITE;
        }
        let list = aiocbs
            .iter_mut()
            .map(|aiocb| &mut *aiocb.0 as *mut libc::aiocb)
            .collect::<Vec<_>>()
            .into_boxed_slice();
        let sev = unsafe { mem::zeroed::<libc::sigevent>() };
        Liocb {
            _aiocbs: aiocbs,
            list,
            sev,
        }
    }
}

#[cfg(target_os = "freebsd")]
impl Source for Liocb {
    fn register(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> io::Result<()> {
        assert!(interests.is_lio());
        let udata = usize::from(token);
        let kq = registry.as_raw_fd();
        self.sev.sigev_notify = libc::SIGEV_KEVENT;
        self.sev.sigev_signo = kq;
        self.sev.sigev_value.sival_ptr = udata as *mut libc::c_void;
        self.sev.sigev_notify_thread_id = 0;
        Ok(())
    }

    fn reregister(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> io::Result<()> {
        self.register(registry, token, interests)
    }

    fn deregister(&mut self, _registry: &Registry) -> io::Result<()> {
        self.sev.sigev_notify = libc::SIGEV_NONE;
        self.sev.sigev_value.sival_ptr = ptr::null_mut();
        Ok(())
    }
}

mod aio {
    use super::*;

    #[test]
    fn smoke() {
        init();
        let mut poll = Poll::new().unwrap();
        let mut events = Events::with_capacity(8);

        let f = File::create(temp_file("aio::smoke")).unwrap();
        let mut aiocb = Aiocb::from_fd(f.as_raw_fd());
        poll.registry()
            .register(&mut aiocb, UDATA, Interest::AIO)
            .unwrap();

        expect_no_events(&mut poll, &mut events);
        aiocb.fsync().unwrap();
        expect_events(
            &mut poll,
            &mut events,
            vec![ExpectEvent::new(UDATA, Interest::AIO)],
        );
    }
}

#[cfg(target_os = "freebsd")]
mod lio {
    use super::*;

    #[test]
    fn smoke() {
        init();
        let data = b"hello, world!\n";
        let mut poll = Poll::new().unwrap();
        let mut events = Events::with_capacity(8);

        let f0 = File::create(temp_file("lio::smoke0")).unwrap();
        let f1 = File::create(temp_file("lio::smoke1")).unwrap();
        let aiocb0 = Aiocb::from_slice(f0.as_raw_fd(), data);
        let aiocb1 = Aiocb::from_slice(f1.as_raw_fd(), data);
        let mut liocb = Liocb::new(vec![aiocb0, aiocb1]);
        poll.registry()
            .register(&mut liocb, UDATA, Interest::LIO)
            .unwrap();

        expect_no_events(&mut poll, &mut events);
        liocb.listio().unwrap();
        expect_events(
            &mut poll,
            &mut events,
            vec![ExpectEvent::new(UDATA, Interest::LIO)],
        );
    }
}
