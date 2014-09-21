use error::MioResult;
use handler::{Handler, BoxedHandler};
use io::IoHandle;
use nix::fcntl;
use os;
use std::mem;

pub struct Poll {
    selector: os::Selector,
    events: os::Events
}

impl Poll {
    pub fn new() -> MioResult<Poll> {
        Ok(Poll {
            selector: try!(os::Selector::new()),
            events: os::Events::new()
        })
    }

    pub fn register<Fd: IoHandle, H: Handler>(&mut self, io: &Fd, handler: H) -> MioResult<()> {
        debug!("registering IO with poller");

        unsafe {
            let boxed: Box<BoxedHandler<H>> = box BoxedHandler::new(io.desc().fd, handler);
            let the_box_as_uint: uint = mem::transmute(boxed);
            debug!("box as uint={:x}", the_box_as_uint);
            // The box is now conveniently forgotten into an int.

            // Register interests for this socket
            match self.selector.register(io.desc(), the_box_as_uint) {
                Ok(()) => {},
                Err(e) => {
                    let boxed: Box<BoxedHandler<H>> = mem::transmute(the_box_as_uint);
                    drop(boxed);
                    debug!("error during registration: {}", e);
                    return Err(e);
                }
            }
        }

        Ok(())
    }

    pub fn unregister_fd(&mut self, fd: fcntl::Fd) -> MioResult<()> {
        self.selector.unregister_fd(fd)
    }

    pub fn poll(&mut self, timeout_ms: uint) -> MioResult<uint> {
        try!(self.selector.select(&mut self.events, timeout_ms));
        Ok(self.events.len())
    }

    pub fn event(&self, idx: uint) -> IoEvent {
        self.events.get(idx)
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
    kind:  IoEventKind,
    token: uint,
}

/// IoEvent represents the raw event that the OS-specific selector
/// returned. An event can represent more than one kind (such as
/// readable or writable) at a time.
///
/// These IoEvent objects are created by the OS-specific concrete
/// Selector when they have events to report.
impl IoEvent {
    /// Create a new IoEvent.
    pub fn new(kind: IoEventKind, token: uint) -> IoEvent {
        IoEvent {
            kind: kind,
            token: token
        }
    }

    #[inline(always)]
    pub fn token(&self) -> uint {
        self.token
    }

    /// This event indicated that the IO handle is now readable
    #[inline(always)]
    pub fn is_readable(&self) -> bool {
        self.kind.contains(IoReadable)
    }

    #[inline(always)]
    /// This event indicated that the IO handle is now writable
    pub fn is_writable(&self) -> bool {
        self.kind.contains(IoWritable)
    }

    #[inline(always)]
    /// This event indicated that the IO handle had an error
    pub fn is_error(&self) -> bool {
        self.kind.contains(IoError)
    }
}
