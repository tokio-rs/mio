use std::mem;
use std::num::FromPrimitive;
use nix::fcntl::Fd;
use error::MioResult;
use handler::Handler;
use sock::*;
use os;
use util::Slab;

#[deriving(Clone, Show)]
pub struct ReactorConfig;

pub struct Reactor {
    selector: os::Selector,
    conns: IoSlab
}

impl<T> Reactor {
    pub fn new() -> MioResult<Reactor> {
        Ok(Reactor {
            selector: try!(os::Selector::new()),
            conns: IoSlab::new(1024)
        })
    }

    /// Connects the socket to the specified address. When the operation
    /// completes, the handler will be notified with the supplied token.
    pub fn connect<S: Socket>(&mut self, io: S,
                              addr: &SockAddr, _token: T) -> MioResult<()> {

        debug!("socket connect; addr={}", addr);

        let handle = match self.conns.register(io.ident()) {
            Some(handle) => handle,
            None => fail!("too many connections")
        };

        if try!(os::connect(handle, addr)) {
            // On some OSs, connecting to localhost succeeds immediately. In
            // this case, queue the writable callback for execution during the
            // next reactor tick.
            debug!("socket connected immediately; addr={}", addr);
        }

        // Register interest
        try!(self.selector.register(handle));

        Ok(())
    }

    pub fn listen(&mut self, _io: IoHandle, _token: T) {
        unimplemented!()
    }

    pub fn shutdown(&mut self) {
        unimplemented!()
    }

    pub fn run<H: Handler<T>>(&mut self, mut handler: H) {
        // Created here for stack allocation
        let mut events = os::Events::new();
        let run = true;

        while run { // TODO: Have stop condition
            println!("Loopin'");

            self.io_poll(&mut events, &mut handler);
        }
    }

    fn io_poll<H: Handler<T>>(&mut self, events: &mut os::Events, _handler: &mut H) {
        self.selector.select(events, 1000).unwrap();

        let mut i = 0u;

        while i < events.len() {
            let evt = events.get(i);

            println!("io: {}", evt.io);

            if evt.is_readable() {
                println!(" + READABLE");
            }

            if evt.is_writable() {
                println!(" + WRITABLE");
            }

            if evt.is_error() {
                println!(" + ERROR");
            }

            let mut foo: [u8, ..1024] = unsafe { mem::uninitialized() };

            println!("{}", evt.io.read(foo.as_mut_slice()));

            i += 1;
        }
    }
}

/*
 * IoHandle is a handle to a socket registered with the reactor. It can be used
 * to retrieve the socket. It also contains the FD and can be used to read /
 * write directly. It must be at most 64bits in order to fit in the epoll registry.
 */
#[deriving(Show)]
pub struct IoHandle {
    ident: Fd,
    tag: u32
}

impl IoHandle {
    pub fn ident(&self) -> Fd {
        self.ident
    }

    pub fn tag(&self) -> uint {
        self.tag as uint
    }

    pub fn read(&self, dst: &mut [u8]) -> MioResult<uint> {
        os::read(self, dst)
    }

    pub fn write(&self, src: &[u8]) -> MioResult<uint> {
        os::write(self, src)
    }
}

struct IoSlab {
    conns: Slab<Fd>
}

impl IoSlab {
    fn new(capacity: uint) -> IoSlab {
        IoSlab { conns: Slab::new(capacity) }
    }

    fn register(&mut self, fd: Fd) -> Option<IoHandle> {
        match self.conns.put(fd) {
            Ok(handle) => {
                let handle: u32 = FromPrimitive::from_uint(handle)
                    .expect("[BUG] invalid handle");

                Some(IoHandle {
                    ident: fd,
                    tag: handle as u32
                })
            }
            Err(_) => return None
        }
    }

    fn deregister(&mut self, _handle: IoHandle) {
        unimplemented!()
    }
}

bitflags!(
    #[deriving(Show)]
    flags IoEventKind: uint {
        static IoReadable = 0x001,
        static IoWritable = 0x002,
        static IoError    = 0x004
    }
)

#[deriving(Show)]
pub struct IoEvent {
    kind: IoEventKind,
    io: IoHandle
}

impl IoEvent {
    pub fn new(kind: IoEventKind, io: IoHandle) -> IoEvent {
        IoEvent {
            kind: kind,
            io: io
        }
    }

    pub fn is_readable(&self) -> bool {
        self.kind.contains(IoReadable)
    }

    pub fn is_writable(&self) -> bool {
        self.kind.contains(IoWritable)
    }

    pub fn is_error(&self) -> bool {
        self.kind.contains(IoError)
    }
}
