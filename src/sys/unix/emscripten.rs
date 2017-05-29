use std;
use std::collections::vec_deque::VecDeque;
use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_void};
use std::os::unix::io::RawFd;
use std::sync::{Arc, Mutex, Once, ONCE_INIT};
use std::sync::atomic::{AtomicUsize, Ordering, ATOMIC_USIZE_INIT};
use std::time::Duration;

use {io, Ready, PollOpt, Token};
use event_imp::Event;

static NEXT_ID: AtomicUsize = ATOMIC_USIZE_INIT;

#[derive(Debug)]
struct EmRegistration {
    token: Token,
    interests: Ready,
    opts: PollOpt,
    queue: Arc<Mutex<VecDeque<Event>>>,
}

#[derive(Debug)]
pub struct Selector {
    id: usize,
    events: Arc<Mutex<VecDeque<Event>>>,
}

static REG_CALLBACKS: Once = ONCE_INIT;

lazy_static! {
    static ref EM_REGS : Mutex<Vec<Option<EmRegistration>>> = Mutex::new(Vec::new());
}

impl Selector {
    pub fn new() -> io::Result<Selector> {
        // Register socket callback handlers with Emscripten runtime
        // Note: Only one set of handlers can be registered at a time
        // https://kripken.github.io/emscripten-site/docs/api_reference/emscripten.h.html#socket-event-registration
        REG_CALLBACKS.call_once(|| {
            let data_ptr = std::ptr::null();
            unsafe {
                emscripten_set_socket_error_callback(data_ptr, error_callback);
                emscripten_set_socket_open_callback(data_ptr, open_callback);
                //emscripten_set_socket_listen_callback(data_ptr, listen_callback);
                //emscripten_set_socket_connection_callback(data_ptr, connection_callback);
                emscripten_set_socket_message_callback(data_ptr, message_callback);
                //emscripten_set_socket_close_callback(data_ptr, close_callback);
            }
        });

        // offset by 1 to avoid choosing 0 as the id of a selector
        Ok(Selector {
               id: NEXT_ID.fetch_add(1, Ordering::Relaxed) + 1,
               events: Arc::new(Mutex::new(VecDeque::new())),
           })
    }

    pub fn id(&self) -> usize {
        self.id
    }

    /// Wait for events from the OS
    pub fn select(&self,
                  evts: &mut Events,
                  _awakener: Token,
                  _timeout: Option<Duration>)
                  -> io::Result<bool> {
        // Note: Emscripten is single threaded so the callbacks are envoked from the
        // main thread.  This means waiting for the timeout to expire is pointless.
        // https://kripken.github.io/emscripten-site/docs/porting/emscripten-runtime-environment.html#browser-main-loop

        unsafe {
            evts.events.set_len(0);
        }

        if let Ok(ref mut events) = self.events.lock() {
            let num_events = std::cmp::min(events.len(), evts.capacity());
            for e in events.drain(..num_events) {
                evts.push_event(e);
            }
        }

        Ok(false) // False prevents awakener use
    }

    /// Register event interests for the given IO handle with emscripten
    pub fn register(&self,
                    fd: RawFd,
                    token: Token,
                    interests: Ready,
                    opts: PollOpt)
                    -> io::Result<()> {
        // Note: Emscripten reuses file descriptors, so a vector to record registration
        // details is a reasonable choice.
        let i = fd as usize;
        let mut regs = EM_REGS.lock().unwrap();
        while regs.len() <= i {
            regs.push(None)
        }
        regs[i] = Some(EmRegistration {
                           token: token,
                           interests: interests,
                           opts: opts,
                           queue: self.events.clone(),
                       });
        Ok(())
    }

    /// Register event interests for the given IO handle with the OS
    pub fn reregister(&self,
                      fd: RawFd,
                      token: Token,
                      interests: Ready,
                      opts: PollOpt)
                      -> io::Result<()> {
        self.register(fd, token, interests, opts)
    }

    /// Deregister event interests for the given IO handle with the OS
    pub fn deregister(&self, fd: RawFd) -> io::Result<()> {
        let i = fd as usize;
        let mut regs = EM_REGS.lock().unwrap();
        if i < regs.len() {
            regs[i] = None;
        }
        Ok(())
    }
}

impl Drop for Selector {
    fn drop(&mut self) {
        // Remove any remaining registrations made on this selector.
        let mut regs = EM_REGS.lock().unwrap();
        for reg in regs.iter_mut() {
            match *reg {
                None => continue,
                Some(ref r) => {
                    if !Arc::ptr_eq(&r.queue, &self.events) {
                        continue;
                    }
                }
            }
            *reg = None;
        }
    }
}

pub struct Events {
    events: Vec<Event>,
}

impl Events {
    pub fn with_capacity(u: usize) -> Events {
        Events { events: Vec::with_capacity(u) }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.events.capacity()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    #[inline]
    pub fn get(&self, idx: usize) -> Option<Event> {
        self.events.get(idx).cloned()
    }

    pub fn push_event(&mut self, event: Event) {
        self.events.push(event);
    }
}

// Emscripten runtime socket support
// https://kripken.github.io/emscripten-site/docs/api_reference/emscripten.h.html#socket-event-registration

#[allow(non_camel_case_types)]
type userData = c_void;
#[allow(non_camel_case_types)]
type em_socket_callback = extern "C" fn(c_int, *mut userData);
#[allow(non_camel_case_types)]
type em_socket_error_callback = extern "C" fn(c_int, c_int, *const c_char, *mut userData);

// Emscripten callback registration definitions
extern "C" {
    fn emscripten_set_socket_error_callback(data: *const userData, cb: em_socket_error_callback);
    fn emscripten_set_socket_open_callback(data: *const userData, cb: em_socket_callback);
    //fn emscripten_set_socket_listen_callback(data: *const userData, cb: em_socket_callback);
    //fn emscripten_set_socket_connection_callback(data: *const userData, cb: em_socket_callback);
    fn emscripten_set_socket_message_callback(data: *const userData, cb: em_socket_callback);
    //fn emscripten_set_socket_close_callback(data: *const userData, cb: em_socket_callback);
}

// Triggered by a WebSocket error
extern "C" fn error_callback(fd: c_int, err: c_int, msg: *const c_char, _data: *mut userData) {
    let err_msg;
    unsafe {
        err_msg = CStr::from_ptr(msg);
    }
    println!("DEBUG: error callback ({} {}): {:?}", fd, err, err_msg);
}

// Triggered when the WebSocket has opened
extern "C" fn open_callback(fd: c_int, _data: *mut userData) {
    let regs = EM_REGS.lock().unwrap();
    if let Some(ref reg) = regs[fd as usize] {
        if reg.interests.is_writable() {
            if let Ok(mut queue) = reg.queue.lock() {
                queue.push_back(Event::new(Ready::writable(), reg.token));
            }
        }
    }
}

// Triggered when listen has been called (synthetic event)
//extern "C" fn listen_callback(fd: c_int, _data: *mut userData) {
//    println!("DEBUG: listen callback ({})", fd);
//}

// Triggered when the connection has been established
//extern "C" fn connection_callback(fd: c_int, _data: *mut userData) {
//    println!("DEBUG: connection callback ({})", fd);
//}

// Triggered when data is available to be read from the socket
extern "C" fn message_callback(fd: c_int, _data: *mut userData) {
    let regs = EM_REGS.lock().unwrap();
    if let Some(ref reg) = regs[fd as usize] {
        if reg.interests.is_readable() {
            if let Ok(mut queue) = reg.queue.lock() {
                queue.push_back(Event::new(Ready::readable(), reg.token));
            }
        }
    }
}

// Triggered when the WebSocket has closed
//extern "C" fn close_callback(fd: c_int, _data: *mut userData) {
//    println!("DEBUG: close callback ({})", fd);
//}
