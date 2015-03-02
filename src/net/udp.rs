use {Io, NonBlock, MioResult};
use buf::{Buf, MutBuf};
use io::{self, FromFd, IoHandle};
use net::{self, TrySend, TryRecv, Socket};
use std::mem;
use std::net::SocketAddr;
use std::os::unix::{Fd, AsRawFd};

pub use std::net::UdpSocket;

impl IoHandle for UdpSocket {
    fn fd(&self) -> Fd {
        self.as_raw_fd()
    }
}

impl FromFd for UdpSocket {
    fn from_fd(fd: Fd) -> UdpSocket {
        unsafe { mem::transmute(Io::new(fd)) }
    }
}

impl Socket for UdpSocket {
}

impl TrySend for UdpSocket {
    fn send_to<B: Buf>(&self, buf: &mut B, target: &SocketAddr) -> MioResult<NonBlock<()>> {
        net::sendto(as_io(self), buf.bytes(), &net::to_nix_addr(target))
            .map(|cnt| {
                buf.advance(cnt);
                NonBlock::Ready(())
            })
            .or_else(io::to_non_block)
    }
}

impl TryRecv for UdpSocket {
    fn recv_from<B: MutBuf>(&self, buf: &mut B) -> MioResult<NonBlock<SocketAddr>> {
        net::recvfrom(as_io(self), buf.mut_bytes())
            .map(|(cnt, addr)| {
                buf.advance(cnt);
                NonBlock::Ready(net::to_std_addr(addr))
            })
            .or_else(io::to_non_block)
    }
}

fn as_io<'a, T>(udp: &'a T) -> &'a Io {
    unsafe { mem::transmute(udp) }
}
