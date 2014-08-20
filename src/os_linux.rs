use std::mem;
use nix::errno::EINPROGRESS;
use nix::fcntl::Fd;
use error::MioResult;
use sock::{Socket, SockAddr, InetAddr, IpV4Addr};
use reactor::{IoHandle, IoEventKind, IoEvent, IoReadable, IoWritable, IoError};

mod nix {
    pub use nix::sys::epoll::*;
    pub use nix::sys::socket::*;
    pub use nix::unistd::*;
}

pub fn connect(io: IoHandle, addr: SockAddr) -> MioResult<bool> {
    match nix::connect(io.ident(), &from_sockaddr(addr)) {
        Ok(_) => Ok(true),
        Err(e) => {
            match e.kind {
                EINPROGRESS => Ok(false),
                _ => Err(e)
            }
        }
    }
}

pub fn read(io: &IoHandle, dst: &mut [u8]) -> MioResult<uint> {
    nix::read(io.ident(), dst)
}

pub struct Selector {
    epfd: Fd
}

impl Selector {
    pub fn new() -> MioResult<Selector> {
        Ok(Selector {
            epfd: try!(nix::epoll_create())
        })
    }

    /// Wait for events from the OS
    pub fn select(&mut self, evts: &mut Events, timeout_ms: uint) -> MioResult<()> {
        let cnt = try!(nix::epoll_wait(
                self.epfd,
                evts.events.as_mut_slice(),
                timeout_ms));

        println!("select: {}", cnt);

        evts.len = cnt;
        Ok(())
    }

    /// Register event interests for the given IO handle with the OS
    pub fn register(&mut self, handle: IoHandle) -> MioResult<()> {
        let interests = nix::EPOLLIN | nix::EPOLLOUT | nix::EPOLLERR;

        let info = nix::EpollEvent {
            events: interests | nix::EPOLLET,
            data: unsafe { mem::transmute(handle) }
        };

        nix::epoll_ctl(self.epfd, nix::EpollCtlAdd, handle.ident(), &info)
    }
}

pub struct Events {
    len: uint,
    events: [nix::EpollEvent, ..256]
}

impl Events {
    pub fn new() -> Events {
        Events {
            len: 0,
            events: unsafe { mem::uninitialized() }
        }
    }

    #[inline]
    pub fn len(&self) -> uint {
        self.len
    }

    #[inline]
    pub fn get(&self, idx: uint) -> IoEvent {
        if idx >= self.len {
            fail!("invalid index");
        }

        let epoll = self.events[idx].events;
        let mut kind = IoEventKind::empty();

        if epoll.contains(nix::EPOLLIN) {
            kind = kind | IoReadable;
        }

        if epoll.contains(nix::EPOLLOUT) {
            kind = kind | IoWritable;
        }

        if epoll.contains(nix::EPOLLERR) {
            kind = kind | IoError;
        }

        let handle = unsafe { mem::transmute(self.events[idx].data) };

        IoEvent::new(kind, handle)
    }
}

fn from_sockaddr(addr: SockAddr) -> nix::SockAddr {
    use std::mem;

    println!("addr: {}", addr);

    match addr {
        InetAddr(ip, port) => {
            match ip {
                IpV4Addr(a, b, c, d) => {
                    let addr = nix::sockaddr_in {
                        sin_family: nix::AF_INET as nix::sa_family_t,
                        sin_port: port.to_be(),
                        sin_addr: ip4_to_inaddr(a, b, c, d),
                        sin_zero: unsafe { mem::zeroed() }

                    };

                    nix::SockIpV4(addr)
                }
                _ => unimplemented!()
            }
        }
        _ => unimplemented!()
    }
}

fn ip4_to_inaddr(a: u8, b: u8, c: u8, d: u8) -> nix::in_addr {
    let ip = (a as u32 << 24) |
             (b as u32 << 16) |
             (c as u32 <<  8) |
             (d as u32 <<  0);

    nix::in_addr {
        s_addr: Int::from_be(ip)
    }
}
