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
unsafe fn on_error<H: Handler>(when: &'static str, reactor: &mut Reactor, error: &MioError, mut h: Box<BoxedHandler<H>>) {
    info!("IO handler died during {}: {}", when, error);
    match reactor.unregister_fd(h.fd) {
        Ok(()) => drop(h),
        Err(_) => { (*h).poison = true; mem::forget(h); }
    }
}

unsafe fn handle<H: Handler>(reactor: &mut Reactor, event: &IoEvent, boxed_h: *mut ()) {
    let mut h: Box<BoxedHandler<H>> = mem::transmute(boxed_h);

    if h.poison {
        on_error("poisoned handler", reactor, &MioError::unknown_sys_error(), h);
        return;
    }

    debug!("evt = {}", event);

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
    handle: unsafe fn(reactor: &mut Reactor, event: &IoEvent, h: *mut ()),
    fd:     fcntl::Fd,
    poison: bool, // is the current handler even valid?
    data:   H,
}

impl<H: Handler> BoxedHandler<H> {
    pub fn new(fd: fcntl::Fd, handler: H) -> BoxedHandler<H> {
        BoxedHandler {
            handle: handle::<H>,
            fd:     fd,
            poison: false,
            data:   handler,
        }
    }
}

#[inline(always)]
#[must_use]
pub unsafe fn call_handler(reactor: &mut Reactor, event: &IoEvent, boxed_handler: *mut ()) {
    debug!("in call_handler");
    let untyped_boxed_handler: *mut BoxedHandler<()> = mem::transmute(boxed_handler);
    ((*untyped_boxed_handler).handle)(reactor, event, boxed_handler)
}
