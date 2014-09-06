use std::mem;
use nix::fcntl::Fd;
pub use nix::sys::epoll::*;
use nix::unistd::close;
use error::{MioResult, MioError};
use super::posix::{IoDesc};
use event::*;

pub type IoPollEvent = EpollEvent;

pub struct Selector {
    epfd: Fd
}

impl Selector {
    pub fn new() -> MioResult<Selector> {
        let epfd = try!(epoll_create().map_err(MioError::from_sys_error));

        Ok(Selector { epfd: epfd })
    }

    /// Wait for events from the OS
    pub fn select(&mut self, evts: &mut [EpollEvent], timeout_ms: uint) -> MioResult<uint> {
        // Wait for epoll events for at most timeout_ms milliseconds
        let cnt = try!(epoll_wait(self.epfd, evts.events.as_mut_slice(), timeout_ms)
                           .map_err(MioError::from_sys_error));

        Ok(cnt)
    }

    /// Register event interests for the given IO handle with the OS
    pub fn register(&mut self, io: IoDesc, token: u64, events: IoEventKind) -> MioResult<()> {
        let interests = from_ioevent(events);

        let info = EpollEvent {
            events: interests | EPOLLET,
            data: token
        };

        epoll_ctl(self.epfd, EpollCtlAdd, io.fd, &info)
            .map_err(MioError::from_sys_error)
    }

    /// Register event interests for the given IO handle with the OS
    pub fn unregister(&mut self, io: IoDesc, token: u64, events: IoEventKind) -> MioResult<()> {
        let interests = 0;

        let info = EpollEvent {
            events: interests | EPOLLET,
            data: token
        };

        epoll_ctl(self.epfd, EpollCtlDel, io.fd, &info)
            .map_err(MioError::from_sys_error)
    }

}

impl Drop for Selector {
    fn drop(&mut self) {
        close(self.epfd);
    }
}


impl IoEvent for IoPollEvent {

    fn is_readable(&self) -> bool {
        self.kind.contains(IoReadable)
    }

    fn is_writable(&self) -> bool {
        self.kind.contains(IoWritable)
    }

    fn is_hangup(&self) -> bool {
        self.kind.contains(IoHangup)
    }

    fn is_error(&self) -> bool {
        self.kind.contains(IoError)
    }


    fn to_ioevent(&self) -> IoEventKind {

        let mut kind = IoEventKind::empty();

        if self.events.contains(EPOLLIN) {
            kind = kind | IoReadable;
        }

        if self.events.contains(EPOLLOUT) {
            kind = kind | IoWritable;
        }

        // EPOLLHUP - Usually means a socket error happened
        if self.events.contains(EPOLLERR) {
            kind = kind | IoError;
        }

        if self.events.contains(EPOLLHUP) || self.events.contains(EPOLLRDHUP) {
            kind = kind | IoHangup;
        }

        kind
    }
}

fn from_ioevent(ioevents: IoEventKind) -> EpollEventKind {
    let mut mask : EpollEventKind = 0;

    if ioevents.contains(IoReadable) {
        mask = mask | EPOLLIN;
    }
    if ioevents.contains(IoWritable) {
        mask = mask | EPOLLOUT;
    }
    if ioevents.contains(IoReadable) {
        mask = mask | EPOLLHUP | EPOLLRDHUP;
    }
    // this one probably isnt' necessary, but for completeness...
    if ioevents.contains(IoError) {
        mask = mask | EPOLLERR;
    }

    mask
}
