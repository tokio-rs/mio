use error::{MioResult, MioError};
use nix::fcntl;
use poll::IoEvent;
use reactor::{Reactor};
use std::mem;

/// `drop` is called when the connection dies. Return an error to kill the
/// connection yourself.
pub trait Handler {
    fn readable(&mut self, reactor: &mut Reactor) -> MioResult<()>;
    fn writable(&mut self, reactor: &mut Reactor) -> MioResult<()>;
}

#[inline(never)]
fn on_error<H: Handler>(when: &'static str, reactor: &mut Reactor, error: &MioError, h: Box<BoxedHandler<H>>) {
    info!("IO handler died during {}: {}", when, error);
    unsafe { reactor.unregister_fd(h.fd).unwrap(); }
    drop(h);
}

unsafe fn handle<H: Handler>(reactor: &mut Reactor, event: Option<&IoEvent>, boxed_h: *mut ()) {
    let mut h: Box<BoxedHandler<H>> = mem::transmute(boxed_h);

    let event = match event {
        Some(event) => event,
        None => {
            let r = reactor.unregister_fd(h.fd);
            drop(h);
            r.unwrap();
            return;
        },
    };

    if event.is_error() {
        on_error("[generic epoll error]", reactor, &MioError::unknown_sys_error(), h);
        return;
    }

    if event.is_readable() {
        let ret = h.data.readable(reactor);
        match ret {
            Ok(()) => {},
            Err(x) => { on_error("read", reactor, &x, h); return; },
        }
    }

    if event.is_writable() {
        let ret = h.data.writable(reactor);
        match ret {
            Ok(()) => {},
            Err(x) => { on_error("write", reactor, &x, h); return; },
        }
    }

    // If we get down here, epoll still owns the handler. Don't drop it.
    mem::forget(h);
}

pub struct BoxedHandler<H> {
    handle: unsafe fn(reactor: &mut Reactor, event: Option<&IoEvent>, h: *mut ()),
    fd:     fcntl::Fd,
    data:   H, // MUST BE THE LAST MEMBER!
}

impl<H: Handler> BoxedHandler<H> {
    pub fn new(fd: fcntl::Fd, handler: H) -> BoxedHandler<H> {
        BoxedHandler {
            handle: handle::<H>,
            fd:     fd,
            data:   handler,
        }
    }
}

#[inline(always)]
pub unsafe fn call_handler(reactor: &mut Reactor, event: &IoEvent, boxed_handler: *mut ()) {
    let untyped_boxed_handler: *mut BoxedHandler<()> = mem::transmute(boxed_handler);
    ((*untyped_boxed_handler).handle)(reactor, Some(event), boxed_handler)
}

#[inline(always)]
pub unsafe fn drop_handler(reactor: &mut Reactor, boxed_handler: *mut ()) {
    let untyped_boxed_handler: *mut BoxedHandler<()> = mem::transmute(boxed_handler);
    ((*untyped_boxed_handler).handle)(reactor, None, boxed_handler);
}
