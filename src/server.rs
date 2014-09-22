//! A generic MIO server.
use error::{MioResult, MioError};
use handler::Handler;
use io::{Ready, IoHandle, IoReader, IoWriter, IoAcceptor};
use iobuf::{Iobuf, RWIobuf};
use reactor::Reactor;
use socket::{TcpSocket, TcpAcceptor, SockAddr};
use std::collections::{Deque, RingBuf};
use std::rc::Rc;
use std::sync::mpmc_bounded_queue;

// TODO(cgaebel): Use a lock-free stack. None of this bounded queue business.
static READBUF_POOL_SIZE: uint = 32;
static READBUF_SIZE:      uint = 4096;

// The number of sends that we queue up before pushing back on the client.
static MAX_OUTSTANDING_SENDS: uint = 1;

pub trait PerClient<St> {
    fn on_read(&mut self, reactor: &mut Reactor, c: &mut ConnectionState<St>, buf: RWIobuf<'static>);
}

/// Global state for a server.
pub struct Global<St> {
    // This should really be a lock-free stack. Unfortunately, only a bounded
    // queue is implemented in the standard library. It'll do for now. =(
    readbuf_pool: mpmc_bounded_queue::Queue<Vec<u8>>,

    custom_state: St,
}

impl<St> Global<St> {
    /// Creates a new global state for a server.
    fn new(custom_state: St) -> Global<St> {
        Global {
            readbuf_pool: mpmc_bounded_queue::Queue::with_capacity(READBUF_POOL_SIZE),

            custom_state: custom_state,
        }
    }

    /// Mints a new iobuf with the given capacity. If the requested length is
    /// less than or equal to 4kb, a pool of iobufs will be used. Recieved data
    /// will automatically use iobufs from this pool, and buffers `sent` will be
    /// returned to it when empty.
    fn make_iobuf(&self, capacity: uint) -> RWIobuf<'static> {
        unsafe {
            if capacity > READBUF_SIZE {
                return RWIobuf::new(capacity);
            }

            let mut ret =
                match self.readbuf_pool.pop() {
                    None    => RWIobuf::new(READBUF_SIZE),
                    Some(v) => RWIobuf::from_vec(v),
                };

            debug_assert!(ret.len() == READBUF_SIZE);
            ret.unsafe_resize(capacity);
            ret
        }
    }

    /// Returns an iobuf to the pool, if possible. It's safe to send any iobuf
    /// back to the pool, but only iobufs constructed with `make_iobuf` (or
    /// luckily compatible other ones) will actually end up in the pool.
    fn return_iobuf(&self, buf: RWIobuf<'static>) {
        match buf.into_vec() {
            Some(v) => {
                if v.len() == READBUF_SIZE {
                    self.readbuf_pool.push(v);
                }
            },
            _ => {},
        }
    }

    #[inline(always)]
    pub fn state(&self) -> &St { &self.custom_state }
}

bitflags! {
    flags Flags: u8 {
        static Readable = 0x01,
        static Writable = 0x02,
    }
}

pub struct ConnectionState<St> {
    global:     Rc<Global<St>>,
    fd:         TcpSocket,
    send_queue: RingBuf<RWIobuf<'static>>,
    flags:      Flags,
}

impl<St> ConnectionState<St> {
    pub fn new(fd: TcpSocket, global: Rc<Global<St>>) -> ConnectionState<St> {
        ConnectionState {
            global:     global,
            fd:         fd,
            send_queue: RingBuf::new(),
            flags:      Flags::empty(),
        }
    }

    pub fn fd(&self) -> &TcpSocket { &self.fd }

    pub fn global(&self) -> &Rc<Global<St>> { &self.global }

    pub fn make_iobuf(&self, capacity: uint) -> RWIobuf<'static> { self.global.make_iobuf(capacity) }

    pub fn return_iobuf(&self, buf: RWIobuf<'static>) { self.global.return_iobuf(buf) }

    pub fn send(&mut self, buf: RWIobuf<'static>) { self.send_queue.push(buf); }
}

struct Connection<St, C> {
    state:      ConnectionState<St>,
    per_client: C,
}

impl<St, C: PerClient<St>> Connection<St, C> {
    fn new(fd: TcpSocket, global: Rc<Global<St>>, per_client: C) -> Connection<St, C> {
        Connection {
            state:      ConnectionState::new(fd, global),
            per_client: per_client,
        }
    }

    fn can_continue(&self) -> bool {
        let send_queue_len = self.state.send_queue.len();

        // readable, and still room on the send queue.
        (self.state.flags.contains(Readable) && send_queue_len <= MAX_OUTSTANDING_SENDS)
        // writable, and there's still stuff to send.
     || (self.state.flags.contains(Writable) && send_queue_len != 0)
    }

    fn tick(&mut self, reactor: &mut Reactor) -> MioResult<()> {
        while self.can_continue() {
            try!(self.fill_buf(reactor));
            try!(self.flush_buf());
        }

        Ok(())
    }

    fn fill_buf(&mut self, reactor: &mut Reactor) -> MioResult<()> {
        if !self.state.flags.contains(Readable) {
            return Ok(());
        }

        let mut in_buf = self.state.make_iobuf(READBUF_SIZE);

        let res = try!(self.state.fd.read(&mut in_buf));

        if res.would_block() {
            self.state.flags.remove(Readable);
        }

        in_buf.flip_lo();

        if !in_buf.is_empty() {
            self.per_client.on_read(reactor, &mut self.state, in_buf);
        } else {
            return Err(MioError::eof());
        }

        Ok(())
    }

    fn flush_buf(&mut self) -> MioResult<()> {
        if !self.state.flags.contains(Writable) {
            return Ok(());
        }

        let mut drop_head = false;

        match self.state.send_queue.front_mut() {
            Some(buf) => {
                let res = try!(self.state.fd.write(buf));

                if res.would_block() {
                    self.state.flags.remove(Writable);
                }

                if buf.is_empty() { drop_head = true; }
            },
            None => {}
        }

        if drop_head {
            let first_elem = self.state.send_queue.pop_front().unwrap();
            self.state.return_iobuf(first_elem);
        }

        Ok(())
    }
}

impl<St, C: PerClient<St>> Handler for Connection<St, C> {
    fn readable(&mut self, reactor: &mut Reactor) -> MioResult<()> {
        self.state.flags.insert(Readable);
        self.tick(reactor)
    }

    fn writable(&mut self, reactor: &mut Reactor) -> MioResult<()> {
        self.state.flags.insert(Writable);
        self.tick(reactor)
    }
}

struct AcceptHandler<St, F> {
    accept_socket: TcpAcceptor,
    global:        Rc<Global<St>>,
    on_accept:     Rc<F>,
}

impl<St, C: PerClient<St>, F: Fn<(), C>> AcceptHandler<St, F> {
    fn new(
        accept_socket: TcpAcceptor,
        global:        Rc<Global<St>>,
        on_accept:     Rc<F>)
          -> AcceptHandler<St, F> {
        AcceptHandler {
            accept_socket: accept_socket,
            global:        global,
            on_accept:     on_accept,
        }
    }
}

impl<St, C: PerClient<St>, F: Fn<(), C>> Handler for AcceptHandler<St, F> {
    fn readable(&mut self, reactor: &mut Reactor) -> MioResult<()> {
        let socket: TcpSocket =
            match self.accept_socket.accept() {
                Ok(Ready(socket)) => socket,
                // It's fine if this didn't work out. We can still accept other
                // connections.
                _                 => return Ok(()),
            };

        let fd = socket.desc().fd;
        let per_client = self.on_accept.deref().call(());
        let handler = Connection::new(socket, self.global.clone(), per_client);
        reactor.register(fd, handler)
    }

    fn writable(&mut self, _reactor: &mut Reactor) -> MioResult<()> {
        warn!("Accepting socket got a `writable` notification. How odd. Ignoring.");
        Ok(())
    }
}

// TODO(cgaebel): The connection factory `F` should take the reactor, but
// doesn't because I have no idea how to pass a &mut to an unboxed closure.

pub fn gen_tcp_server<St, C: PerClient<St>, F: Fn<(), C>>(
    reactor:      &mut Reactor,
    listen_on:    &SockAddr,
    backlog:      uint,
    shared_state: St,
    on_accept:    F)
      -> MioResult<()> {
    // TODO(cgaebel): ipv6? udp?
    let accept_socket: TcpSocket = try!(TcpSocket::v4());
    let acceptor: TcpAcceptor = try!(accept_socket.bind(listen_on));
    let global    = Rc::new(Global::new(shared_state));
    let on_accept = Rc::new(on_accept);
    reactor.listen(acceptor, backlog, |socket|
      AcceptHandler::new(socket, global.clone(), on_accept.clone()))
}
