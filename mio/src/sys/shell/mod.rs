macro_rules! os_required {
    () => {
        panic!("mio must be compiled with `os-poll` to run.")
    };
}

mod selector;
pub(crate) use self::selector::{event, Event, Events, Selector};

cfg_tcp! {
    mod tcp;
    pub(crate) use self::tcp::{TcpStream, TcpListener};
}

cfg_udp! {
    mod udp;
    pub(crate) use self::udp::UdpSocket;
}

cfg_uds! {
    mod uds;
    pub(crate) use self::uds::{UnixDatagram, UnixListener, UnixStream};
}

mod waker;
pub(crate) use self::waker::Waker;
