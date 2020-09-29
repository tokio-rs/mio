//! Windows named pipes bindings for mio.
//!
//! This crate implements bindings for named pipes for the mio crate. This
//! crate compiles on all platforms but only contains anything on Windows.
//! Currently this crate requires mio 0.6.2.
//!
//! On Windows, mio is implemented with an IOCP object at the heart of its
//! `Poll` implementation. For named pipes, this means that all I/O is done in
//! an overlapped fashion and the named pipes themselves are registered with
//! mio's internal IOCP object. Essentially, this crate is using IOCP for
//! bindings with named pipes.
//!
//! Note, though, that IOCP is a *completion* based model whereas mio expects a
//! *readiness* based model. As a result this crate, like with TCP objects in
//! mio, has internal buffering to translate the completion model to a readiness
//! model. This means that this crate is not a zero-cost binding over named
//! pipes on Windows, but rather approximates the performance of mio's TCP
//! implementation on Windows.
//!
//! # Trait implementations
//!
//! The `Read` and `Write` traits are implemented for `NamedPipe` and for
//! `&NamedPipe`. This represents that a named pipe can be concurrently read and
//! written to and also can be read and written to at all. Typically a named
//! pipe needs to be connected to a client before it can be read or written,
//! however.
//!
//! Note that for I/O operations on a named pipe to succeed then the named pipe
//! needs to be associated with an event loop. Until this happens all I/O
//! operations will return a "would block" error.
//!
//! # Managing connections
//!
//! The `NamedPipe` type supports a `connect` method to connect to a client and
//! a `disconnect` method to disconnect from that client. These two methods only
//! work once a named pipe is associated with an event loop.
//!
//! The `connect` method will succeed asynchronously and a completion can be
//! detected once the object receives a writable notification.
//!
//! # Named pipe clients
//!
//! Currently to create a client of a named pipe server then you can use the
//! `OpenOptions` type in the standard library to create a `File` that connects
//! to a named pipe. Afterwards you can use the `into_raw_handle` method coupled
//! with the `NamedPipe::from_raw_handle` method to convert that to a named pipe
//! that can operate asynchronously. Don't forget to pass the
//! `FILE_FLAG_OVERLAPPED` flag when opening the `File`.

use crate::{poll, Registry, Token};

use std::cell::UnsafeCell;
use std::ffi::OsStr;
use std::fmt;
use std::io::{self, Read};
use std::mem;
use std::os::windows::io::*;
use std::slice;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

// use mio::windows;
// use mio::{Evented, Poll, PollOpt, Ready, Registration, SetReadiness, Token};
use miow::iocp::CompletionStatus;
use miow::pipe;
use winapi::shared::winerror::*;
use winapi::um::ioapiset::*;
use winapi::um::minwinbase::*;

macro_rules! offset_of {
    ($t:ty, $($field:ident).+) => (
        &(*(0 as *const $t)).$($field).+ as *const _ as usize
    )
}

macro_rules! overlapped2arc {
    ($e:expr, $t:ty, $($field:ident).+) => ({
        let offset = offset_of!($t, $($field).+);
        debug_assert!(offset < mem::size_of::<$t>());
        Arc::from_raw(($e as usize - offset) as *mut $t)
    })
}

fn would_block() -> io::Error {
    io::ErrorKind::WouldBlock.into()
}

/// Representation of a named pipe on Windows.
///
/// This structure internally contains a `HANDLE` which represents the named
/// pipe, and also maintains state associated with the mio event loop and active
/// I/O operations that have been scheduled to translate IOCP to a readiness
/// model.
pub struct NamedPipe {
    inner: Arc<Inner>,
}

struct Inner {
    handle: pipe::NamedPipe,

    connect: Overlapped,
    connecting: AtomicBool,

    read: Overlapped,
    write: Overlapped,

    io: Mutex<Io>,

    pool: Mutex<BufferPool>,
}

struct Io {
    read: State,
    read_waker: Option<Waker>,
    write: State,
    write_waker: Option<Waker>,
    connect_error: Option<io::Error>,
}

enum State {
    None,
    Pending(Vec<u8>, usize),
    Ok(Vec<u8>, usize),
    Err(io::Error),
}

fn _assert_kinds() {
    fn _assert_send<T: Send>() {}
    fn _assert_sync<T: Sync>() {}
    _assert_send::<NamedPipe>();
    _assert_sync::<NamedPipe>();
}

impl NamedPipe {
    /// Creates a new named pipe at the specified `addr` given a "reasonable
    /// set" of initial configuration options.
    pub fn new<A: AsRef<OsStr>>(addr: A, registry: &Registry, token: Token) -> io::Result<NamedPipe> {
        let pipe = pipe::NamedPipe::new(addr)?;
        NamedPipe::from_raw_handle(pipe.into_raw_handle(), registry, token)
    }

    /// TODO: Dox
    pub fn from_raw_handle(handle: RawHandle, registry: &Registry, token: Token) -> io::Result<NamedPipe> {
        // Create the pipe
        let pipe = NamedPipe {
            inner: Arc::new(Inner {
                // Safety: not really unsafe
                handle: unsafe { pipe::NamedPipe::from_raw_handle(handle) },
                // transmutes to straddle winapi versions (mio 0.6 is on an
                // older winapi)
                connect: Overlapped::new(connect_done),
                connecting: AtomicBool::new(false),
                read: Overlapped::new(read_done),
                write: Overlapped::new(write_done),
                io: Mutex::new(Io {
                    read: State::None,
                    read_waker: None,
                    write: State::None,
                    write_waker: None,
                    connect_error: None,
                }),
                pool: Mutex::new(BufferPool::with_capacity(2)),
            }),
        };

        // Register the handle w/ the IOCP handle
        poll::selector(registry).inner.cp.add_handle(usize::from(token), &pipe.inner.handle)?;

        // Queue the initial read
        pipe.inner.post_register();

        Ok(pipe)
    }

    /// Attempts to call `ConnectNamedPipe`, if possible.
    ///
    /// This function will attempt to connect this pipe to a client in an
    /// asynchronous fashion. If the function immediately establishes a
    /// connection to a client then `Ok(())` is returned. Otherwise if a
    /// connection attempt was issued and is now in progress then a "would
    /// block" error is returned.
    ///
    /// When the connection is finished then this object will be flagged as
    /// being ready for a write, or otherwise in the writable state.
    ///
    /// # Errors
    ///
    /// This function will return a "would block" error if the pipe has not yet
    /// been registered with an event loop, if the connection operation has
    /// previously been issued but has not yet completed, or if the connect
    /// itself was issued and didn't finish immediately.
    ///
    /// Normal I/O errors from the call to `ConnectNamedPipe` are returned
    /// immediately.
    pub fn connect(&self) -> io::Result<()> {
        // "Acquire the connecting lock" or otherwise just make sure we're the
        // only operation that's using the `connect` overlapped instance.
        if self.inner.connecting.swap(true, SeqCst) {
            return Err(would_block());
        }

        // Now that we've flagged ourselves in the connecting state, issue the
        // connection attempt. Afterwards interpret the return value and set
        // internal state accordingly.
        let res = unsafe {
            let overlapped = self.inner.connect.as_mut_ptr() as *mut _;
            self.inner.handle.connect_overlapped(overlapped)
        };

        match res {
            // The connection operation finished immediately, so let's schedule
            // reads/writes and such.
            Ok(true) => {
                self.inner.connecting.store(false, SeqCst);
                Inner::post_register(&self.inner);
                Ok(())
            }

            // If the overlapped operation was successful and didn't finish
            // immediately then we forget a copy of the arc we hold
            // internally. This ensures that when the completion status comes
            // in for the I/O operation finishing it'll have a reference
            // associated with it and our data will still be valid. The
            // `connect_done` function will "reify" this forgotten pointer to
            // drop the refcount on the other side.
            Ok(false) => {
                mem::forget(self.inner.clone());
                Err(would_block())
            }

            // TODO: are we sure no IOCP notification comes in here?
            Err(e) => {
                self.inner.connecting.store(false, SeqCst);
                Err(e)
            }
        }
    }

    /// Takes any internal error that has happened after the last I/O operation
    /// which hasn't been retrieved yet.
    ///
    /// This is particularly useful when detecting failed attempts to `connect`.
    /// After a completed `connect` flags this pipe as writable then callers
    /// must invoke this method to determine whether the connection actually
    /// succeeded. If this function returns `None` then a client is connected,
    /// otherwise it returns an error of what happened and a client shouldn't be
    /// connected.
    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        Ok(self.inner.io.lock().unwrap().connect_error.take())
    }

    /// Disconnects this named pipe from a connected client.
    ///
    /// This function will disconnect the pipe from a connected client, if any,
    /// transitively calling the `DisconnectNamedPipe` function. If the
    /// disconnection is successful then this object will no longer be readable
    /// or writable.
    ///
    /// After a `disconnect` is issued, then a `connect` may be called again to
    /// connect to another client.
    pub fn disconnect(&self) -> io::Result<()> {
        self.inner.handle.disconnect()
    }

    /// TODO: dox
    pub fn read(&mut self, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<io::Result<usize>> {
        let mut state = self.inner.io.lock().unwrap();
        match mem::replace(&mut state.read, State::None) {
            // In theory not possible with `ready_registration` checked above,
            // but return would block for now.
            State::None => {
                state.read_waker = Some(cx.waker().clone());
                Poll::Pending
            }

            // A read is in flight, still waiting for it to finish
            State::Pending(buf, amt) => {
                state.read = State::Pending(buf, amt);
                state.read_waker = Some(cx.waker().clone());
                Poll::Pending
            }

            // We previously read something into `data`, try to copy out some
            // data. If we copy out all the data schedule a new read and
            // otherwise store the buffer to get read later.
            State::Ok(data, cur) => {
                let n = {
                    let mut remaining = &data[cur..];
                    remaining.read(buf)?
                };
                let next = cur + n;
                if next != data.len() {
                    state.read = State::Ok(data, next);
                } else {
                    self.inner.put_buffer(data);
                    Inner::schedule_read(&self.inner, &mut state);
                }
                Poll::Ready(Ok(n))
            }

            // Looks like an in-flight read hit an error, return that here while
            // we schedule a new one.
            State::Err(e) => {
                Inner::schedule_read(&self.inner, &mut state);
                if e.raw_os_error() == Some(ERROR_BROKEN_PIPE as i32) {
                    Poll::Ready(Ok(0))
                } else {
                    Poll::Ready(Err(e))
                }
            }
        }
    }

    /// TODO: dox
    pub fn write(&mut self, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        // Make sure there's no writes pending
        let mut io = self.inner.io.lock().unwrap();
        match io.write {
            State::None => {}
            _ => {
                io.write_waker = Some(cx.waker().clone());
                return Poll::Pending;
            }
        }

        // Move `buf` onto the heap and fire off the write
        let mut owned_buf = self.inner.get_buffer();
        owned_buf.extend(buf);
        Inner::schedule_write(&self.inner, owned_buf, 0, &mut io);
        Poll::Ready(Ok(buf.len()))
    }
}

impl AsRawHandle for NamedPipe {
    fn as_raw_handle(&self) -> RawHandle {
        self.inner.handle.as_raw_handle()
    }
}

impl fmt::Debug for NamedPipe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.handle.fmt(f)
    }
}

impl Drop for NamedPipe {
    fn drop(&mut self) {
        // Cancel pending reads/connects, but don't cancel writes to ensure that
        // everything is flushed out.
        unsafe {
            if self.inner.connecting.load(SeqCst) {
                drop(cancel(&self.inner.handle, &self.inner.connect));
            }

            let io = self.inner.io.lock().unwrap();

            match io.read {
                State::Pending(..) => {
                    drop(cancel(&self.inner.handle, &self.inner.read));
                }
                _ => {}
            }
        }
    }
}

impl Inner {
    /// Schedules a read to happen in the background, executing an overlapped
    /// operation.
    ///
    /// This function returns `true` if a normal error happens or if the read
    /// is scheduled in the background. If the pipe is no longer connected
    /// (ERROR_PIPE_LISTENING) then `false` is returned and no read is
    /// scheduled.
    fn schedule_read(me: &Arc<Inner>, io: &mut Io) -> bool {
        // Check to see if a read is already scheduled/completed
        match io.read {
            State::None => {}
            _ => return true,
        }

        // Allocate a buffer and schedule the read.
        let mut buf = me.get_buffer();
        let e = unsafe {
            let overlapped = me.read.as_mut_ptr() as *mut _;
            let slice = slice::from_raw_parts_mut(buf.as_mut_ptr(), buf.capacity());
            me.handle.read_overlapped(slice, overlapped)
        };

        match e {
            // See `connect` above for the rationale behind `forget`
            Ok(_) => {
                io.read = State::Pending(buf, 0); // 0 is ignored on read side
                mem::forget(me.clone());
                true
            }

            // If ERROR_PIPE_LISTENING happens then it's not a real read error,
            // we just need to wait for a connect.
            Err(ref e) if e.raw_os_error() == Some(ERROR_PIPE_LISTENING as i32) => false,

            // If some other error happened, though, we're now readable to give
            // out the error.
            Err(e) => {
                io.read = State::Err(e);
                if let Some(waker) = io.read_waker.take() {
                    waker.wake();
                }
                true
            }
        }
    }

    fn schedule_write(me: &Arc<Inner>, buf: Vec<u8>, pos: usize, io: &mut Io) {
        // Very similar to `schedule_read` above, just done for the write half.
        let e = unsafe {
            let overlapped = me.write.as_mut_ptr() as *mut _;
            me.handle.write_overlapped(&buf[pos..], overlapped)
        };

        match e {
            // See `connect` above for the rationale behind `forget`
            Ok(_) => {
                io.write = State::Pending(buf, pos);
                mem::forget(me.clone())
            }
            Err(e) => {
                io.write = State::Err(e);
                if let Some(waker) = io.write_waker.take() {
                    waker.wake();
                }
            }
        }
    }

    fn post_register(self: &Arc<Inner>) {
        let mut io = self.io.lock().unwrap();
        if Inner::schedule_read(&self, &mut io) {
            if let State::None = io.write {
                if let Some(waker) = io.write_waker.take() {
                    waker.wake();
                }
            }
        }
    }

    fn get_buffer(&self) -> Vec<u8> {
        self.pool.lock().unwrap().get(8 * 1024)
    }

    fn put_buffer(&self, buf: Vec<u8>) {
        self.pool.lock().unwrap().put(buf)
    }
}

unsafe fn cancel<T: AsRawHandle>(handle: &T, overlapped: &Overlapped) -> io::Result<()> {
    let ret = CancelIoEx(handle.as_raw_handle(), overlapped.as_mut_ptr() as *mut _);
    if ret == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn connect_done(status: &OVERLAPPED_ENTRY) {
    let status = CompletionStatus::from_entry(status);

    // Acquire the `Arc<Inner>`. Note that we should be guaranteed that
    // the refcount is available to us due to the `mem::forget` in
    // `connect` above.
    let me = unsafe { overlapped2arc!(status.overlapped(), Inner, connect) };

    // Flag ourselves as no longer using the `connect` overlapped instances.
    let prev = me.connecting.swap(false, SeqCst);
    assert!(prev, "wasn't previously connecting");

    // Stash away our connect error if one happened
    debug_assert_eq!(status.bytes_transferred(), 0);
    unsafe {
        match me.handle.result(status.overlapped()) {
            Ok(n) => debug_assert_eq!(n, 0),
            Err(e) => me.io.lock().unwrap().connect_error = Some(e),
        }
    }

    // We essentially just finished a registration, so kick off a
    // read and register write readiness.
    Inner::post_register(&me);
}

fn read_done(status: &OVERLAPPED_ENTRY) {
    let status = CompletionStatus::from_entry(status);

    // Acquire the `FromRawArc<Inner>`. Note that we should be guaranteed that
    // the refcount is available to us due to the `mem::forget` in
    // `schedule_read` above.
    let me = unsafe { overlapped2arc!(status.overlapped(), Inner, read) };

    // Move from the `Pending` to `Ok` state.
    let mut io = me.io.lock().unwrap();
    let mut buf = match mem::replace(&mut io.read, State::None) {
        State::Pending(buf, _) => buf,
        _ => unreachable!(),
    };
    unsafe {
        match me.handle.result(status.overlapped()) {
            Ok(n) => {
                debug_assert_eq!(status.bytes_transferred() as usize, n);
                buf.set_len(status.bytes_transferred() as usize);
                io.read = State::Ok(buf, 0);
            }
            Err(e) => {
                debug_assert_eq!(status.bytes_transferred(), 0);
                io.read = State::Err(e);
            }
        }
    }

    // Flag our readiness that we've got data.
    if let Some(waker) = io.read_waker.take() {
        waker.wake();
    }
}

fn write_done(status: &OVERLAPPED_ENTRY) {
    let status = CompletionStatus::from_entry(status);

    // Acquire the `Arc<Inner>`. Note that we should be guaranteed that
    // the refcount is available to us due to the `mem::forget` in
    // `schedule_write` above.
    let me = unsafe { overlapped2arc!(status.overlapped(), Inner, write) };

    // Make the state change out of `Pending`. If we wrote the entire buffer
    // then we're writable again and otherwise we schedule another write.
    let mut io = me.io.lock().unwrap();
    let (buf, pos) = match mem::replace(&mut io.write, State::None) {
        State::Pending(buf, pos) => (buf, pos),
        _ => unreachable!(),
    };

    unsafe {
        match me.handle.result(status.overlapped()) {
            Ok(n) => {
                debug_assert_eq!(status.bytes_transferred() as usize, n);
                let new_pos = pos + (status.bytes_transferred() as usize);
                if new_pos == buf.len() {
                    me.put_buffer(buf);
                    if let Some(waker) = io.write_waker.take() {
                        waker.wake();
                    }
                } else {
                    Inner::schedule_write(&me, buf, new_pos, &mut io);
                }
            }
            Err(e) => {
                debug_assert_eq!(status.bytes_transferred(), 0);
                io.write = State::Err(e);
                if let Some(waker) = io.write_waker.take() {
                    waker.wake();
                }
            }
        }
    }
}

// Based on https://github.com/tokio-rs/mio/blob/13d5fc9/src/sys/windows/buffer_pool.rs
struct BufferPool {
    pool: Vec<Vec<u8>>,
}

impl BufferPool {
    fn with_capacity(cap: usize) -> BufferPool {
        BufferPool {
            pool: Vec::with_capacity(cap),
        }
    }

    fn get(&mut self, default_cap: usize) -> Vec<u8> {
        self.pool
            .pop()
            .unwrap_or_else(|| Vec::with_capacity(default_cap))
    }

    fn put(&mut self, mut buf: Vec<u8>) {
        if self.pool.len() < self.pool.capacity() {
            unsafe {
                buf.set_len(0);
            }
            self.pool.push(buf);
        }
    }
}

// See sys::windows module docs for why this exists.
//
// The gist of it is that `Selector` assumes that all `OVERLAPPED` pointers are
// actually inside one of these structures so it can use the `Callback` stored
// right after it.
//
// We use repr(C) here to ensure that we can assume the overlapped pointer is
// at the start of the structure so we can just do a cast.
/// A wrapper around an internal instance over `miow::Overlapped` which is in
/// turn a wrapper around the Windows type `OVERLAPPED`.
///
/// This type is required to be used for all IOCP operations on handles that are
/// registered with an event loop. The event loop will receive notifications
/// over `OVERLAPPED` pointers that have completed, and it will cast that
/// pointer to a pointer to this structure and invoke the associated callback.
#[repr(C)]
pub struct Overlapped {
    inner: UnsafeCell<miow::Overlapped>,
    callback: fn(&OVERLAPPED_ENTRY),
}

impl Overlapped {
    /// Creates a new `Overlapped` which will invoke the provided `cb` callback
    /// whenever it's triggered.
    ///
    /// The returned `Overlapped` must be used as the `OVERLAPPED` passed to all
    /// I/O operations that are registered with mio's event loop. When the I/O
    /// operation associated with an `OVERLAPPED` pointer completes the event
    /// loop will invoke the function pointer provided by `cb`.
    pub fn new(cb: fn(&OVERLAPPED_ENTRY)) -> Overlapped {
        Overlapped {
            inner: UnsafeCell::new(miow::Overlapped::zero()),
            callback: cb,
        }
    }

    /// Get the underlying `Overlapped` instance as a raw pointer.
    ///
    /// This can be useful when only a shared borrow is held and the overlapped
    /// pointer needs to be passed down to winapi.
    pub fn as_mut_ptr(&self) -> *mut OVERLAPPED {
        unsafe {
            (*self.inner.get()).raw()
        }
    }
}

impl fmt::Debug for Overlapped {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Overlapped")
            .finish()
    }
}

// Overlapped's APIs are marked as unsafe Overlapped's APIs are marked as
// unsafe as they must be used with caution to ensure thread safety. The
// structure itself is safe to send across threads.
unsafe impl Send for Overlapped {}
unsafe impl Sync for Overlapped {}