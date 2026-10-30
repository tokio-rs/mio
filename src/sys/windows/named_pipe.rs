use std::ffi::OsStr;
use std::io::{self, IoSlice, IoSliceMut, Read, Write};
use std::os::windows::io::{AsRawHandle, FromRawHandle, RawHandle};
use std::ptr::{self, NonNull};
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::{Arc, Mutex};
use std::{cmp, fmt, iter, mem};

use windows_sys::Win32::Foundation::{
    ERROR_BROKEN_PIPE, ERROR_IO_PENDING, ERROR_MORE_DATA, ERROR_NOT_FOUND, ERROR_NO_DATA,
    ERROR_PIPE_CONNECTED, ERROR_PIPE_LISTENING, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    GetFileType, ReadFile, WriteFile, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED,
    FILE_TYPE_PIPE, PIPE_ACCESS_DUPLEX,
};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_TYPE_BYTE,
    PIPE_UNLIMITED_INSTANCES,
};
use windows_sys::Win32::System::IO::{
    CancelIoEx, GetOverlappedResult, OVERLAPPED, OVERLAPPED_ENTRY,
};

use crate::event::Source;
use crate::sys::windows::iocp::{CompletionPort, CompletionStatus};
use crate::sys::windows::{Event, Handle, Overlapped};
use crate::Registry;
use crate::{Interest, Token};

/// Non-blocking windows named pipe.
///
/// This structure internally contains a `HANDLE` which represents the named
/// pipe, and also maintains state associated with the mio event loop and active
/// I/O operations that have been scheduled to translate IOCP to a readiness
/// model.
///
/// Note, IOCP is a *completion* based model whereas mio is a *readiness* based
/// model. To bridge this, `NamedPipe` performs internal buffering. Writes are
/// written to an internal buffer and the buffer is submitted to IOCP. IOCP
/// reads are submitted using internal buffers and `NamedPipe::read` reads from
/// this internal buffer.
///
/// # Trait implementations
///
/// The `Read` and `Write` traits are implemented for `NamedPipe` and for
/// `&NamedPipe`. This represents that a named pipe can be concurrently read and
/// written to and also can be read and written to at all. Typically a named
/// pipe needs to be connected to a client before it can be read or written,
/// however.
///
/// Note that for I/O operations on a named pipe to succeed then the named pipe
/// needs to be associated with an event loop. While it's not registered, an
/// operation that has to start new I/O returns a `NotFound` error. Data, or an
/// error, from I/O started earlier is still returned.
///
/// # Managing connections
///
/// The `NamedPipe` type supports a `connect` method to connect to a client and
/// a `disconnect` method to disconnect from that client. These two methods only
/// work once a named pipe is associated with an event loop.
///
/// The `connect` method will succeed asynchronously and a completion can be
/// detected once the object receives a writable notification.
///
/// # Named pipe clients
///
/// Currently to create a client of a named pipe server then you can use the
/// `OpenOptions` type in the standard library to create a `File` that connects
/// to a named pipe. Afterwards you can use the `into_raw_handle` method coupled
/// with the `NamedPipe::from_raw_handle` method to convert that to a named pipe
/// that can operate asynchronously. Don't forget to pass the
/// `FILE_FLAG_OVERLAPPED` flag when opening the `File`.
///
/// A handle passed to `NamedPipe::from_raw_handle` must also not be associated
/// with an I/O completion port yet, nor use
/// `FILE_SKIP_COMPLETION_PORT_ON_SUCCESS`: every operation must complete
/// through the completion port the pipe is registered with.
pub struct NamedPipe {
    inner: Arc<Inner>,
}

/// # Notes
///
/// `event` must be the first field: a posted event's `OVERLAPPED` pointer is
/// the pointer to its `Inner`, see `Io::schedule_event`.
#[repr(C)]
struct Inner {
    event: Overlapped,
    handle: Handle,
    io: Mutex<Io>,
    // Largest read size, see `MAX_READ_SIZE`.
    max_read_size: usize,
}

// SAFETY: `Handles`s are, in general, not thread-safe. However, we only used `Handle`s for
// resources that are thread-safe in `Inner`.
unsafe impl Send for Inner {}

// SAFETY: `Handles`s are, in general, not thread-safe. However, we only used `Handle`s for
// resources that are thread-safe in `Inner`.
unsafe impl Sync for Inner {}

impl Inner {
    /// Issues an overlapped connect, waiting for a client.
    ///
    /// `Ok` means the connect is submitted: its completion packet delivers the
    /// outcome. `Err` means it's not submitted and no completion packet will be
    /// queued, see `is_connected` for when that means a client is connected.
    ///
    /// # Unsafety
    ///
    /// The kernel requires `overlapped` to be valid until the connect
    /// completes, and not to be used for any other I/O meanwhile.
    unsafe fn connect_overlapped(&self, overlapped: *mut OVERLAPPED) -> io::Result<()> {
        // Nonzero means it finished right away, its packet still delivers the
        // outcome.
        if ConnectNamedPipe(self.handle.raw(), overlapped) == 0 {
            let err = io::Error::last_os_error();
            if err.raw_os_error() != Some(ERROR_IO_PENDING as i32) {
                return Err(err);
            }
        }
        Ok(())
    }

    /// Disconnects this named pipe from any connected client.
    fn disconnect(&self) -> io::Result<()> {
        if unsafe { DisconnectNamedPipe(self.handle.raw()) } == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    /// Issues an overlapped read of up to `len` bytes into `buf`.
    ///
    /// `Ok` means the read is submitted: its completion packet delivers the
    /// outcome, also if it finished right away. `Err` means it's not submitted
    /// and no completion packet will be queued.
    ///
    /// # Unsafety
    ///
    /// The kernel requires `buf` and `overlapped` to be valid until the read
    /// completes, and `overlapped` not to be used for any other I/O meanwhile.
    unsafe fn read_overlapped(
        &self,
        buf: *mut u8,
        len: usize,
        overlapped: *mut OVERLAPPED,
    ) -> io::Result<()> {
        let len = cmp::min(len, u32::MAX as usize) as u32;
        if ReadFile(
            self.handle.raw(),
            buf.cast(),
            len,
            ptr::null_mut(),
            overlapped,
        ) == 0
        {
            let err = io::Error::last_os_error();
            match err.raw_os_error().map(|e| e as u32) {
                // `ERROR_MORE_DATA` is only a warning (the message didn't fit),
                // the read still completes through its packet.
                Some(ERROR_IO_PENDING | ERROR_MORE_DATA) => {}
                _ => return Err(err),
            }
        }
        Ok(())
    }

    /// Issues an overlapped write of `len` bytes from `buf`.
    ///
    /// `Ok` means the write is submitted: its completion packet delivers the
    /// outcome, also if it finished right away. `Err` means it's not submitted
    /// and no completion packet will be queued.
    ///
    /// # Unsafety
    ///
    /// The kernel requires `buf` and `overlapped` to be valid until the write
    /// completes, and `overlapped` not to be used for any other I/O meanwhile.
    unsafe fn write_overlapped(
        &self,
        buf: *const u8,
        len: usize,
        overlapped: *mut OVERLAPPED,
    ) -> io::Result<()> {
        let len = cmp::min(len, u32::MAX as usize) as u32;
        if WriteFile(
            self.handle.raw(),
            buf.cast(),
            len,
            ptr::null_mut(),
            overlapped,
        ) == 0
        {
            let err = io::Error::last_os_error();
            if err.raw_os_error() != Some(ERROR_IO_PENDING as i32) {
                return Err(err);
            }
        }
        Ok(())
    }

    /// Calls the `GetOverlappedResult` function to get the result of the
    /// completed operation `op` on this pipe: the number of bytes transferred,
    /// or the error it failed with.
    ///
    /// Takes the `Op` itself, as handed back by `Op::complete`, so the
    /// `OVERLAPPED` is accessed through the `Box` that owns it.
    #[inline]
    fn result(&self, op: &mut Op) -> io::Result<usize> {
        let mut transferred = 0;
        // SAFETY: `op` is borrowed for the call, and without waiting
        // `GetOverlappedResult` only reads the `OVERLAPPED`.
        let r = unsafe {
            GetOverlappedResult(self.handle.raw(), Op::overlapped(op), &mut transferred, 0)
        };
        if r == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(transferred as usize)
        }
    }

    /// Cancels the operation `op_id`, if it's still in flight.
    ///
    /// Called with `Io` locked, for an operation it tracks as in flight, so
    /// its `Op` isn't freed or reused meanwhile.
    fn cancel(&self, op_id: OpId) {
        // `Op` and `Overlapped` are `repr(C)` and start with the `OVERLAPPED`.
        let overlapped = op_id.0.as_ptr().cast::<OVERLAPPED>();
        // SAFETY: `CancelIoEx` uses the pointer to find the operation to
        // cancel. Even so, it points to a live `OVERLAPPED`, see above.
        if unsafe { CancelIoEx(self.handle.raw(), overlapped) } == 0 {
            let err = io::Error::last_os_error();
            // `ERROR_NOT_FOUND` means it already completed, then there's
            // nothing to cancel.
            if err.raw_os_error() != Some(ERROR_NOT_FOUND as i32) {
                error!("error cancelling named pipe operation: {err}");
            }
        }
    }
}

/// Whether `ConnectNamedPipe` failed because a client is connected already.
fn is_connected(err: &io::Error) -> bool {
    matches!(
        err.raw_os_error().map(|e| e as u32),
        // Connected before we waited for it, possibly gone already.
        Some(ERROR_PIPE_CONNECTED | ERROR_NO_DATA)
    )
}

/// An overlapped operation: the `OVERLAPPED` the kernel completes, and the
/// buffer it reads into or writes from.
///
/// While the operation is in flight the kernel owns its `Op`. `Io::submit`
/// hands it over with `Box::into_raw` right before the syscall, and the
/// completion callback takes it back with `Box::from_raw` (`Op::complete`).
/// An operation that isn't submitted gets no completion, so `Io::submit` takes
/// its `Op` back right away.
///
/// Every operation has its own `Op`, so a completion always knows which
/// operation it's for. One that's no longer wanted, say after a `disconnect`,
/// is cancelled and forgotten, and its completion only frees its `Op`.
///
/// # Notes
///
/// `overlapped` must be the first field: the selector reads the callback from,
/// and the callbacks find the `Op` by, the `OVERLAPPED` pointer.
#[repr(C)]
struct Op {
    overlapped: Overlapped,
    /// Keeps the pipe alive while the kernel owns the `Op`, `None` otherwise.
    pipe: Option<Arc<Inner>>,
    buf: Vec<u8>,
}

/// Size of reads until the caller's buffer size is known. Also the minimum
/// read size, so that small reads (e.g. of a length prefix) don't turn into
/// small reads from the pipe.
const DEFAULT_READ_SIZE: usize = 4 * 1024;

/// Maximum read size, the pipe buffer size `NamedPipe::new` uses. Limits the
/// memory held by the read that is always pending. Only used for pipes: other
/// handles, such as serial ports, can wait for the whole buffer to fill before
/// a read completes, so a larger read delays the data.
const MAX_READ_SIZE: usize = 64 * 1024;

impl Op {
    fn new() -> Box<Op> {
        Box::new(Op {
            // The callback is set when the operation is submitted.
            overlapped: Overlapped::new(read_done),
            pipe: None,
            // Allocated once needed, a connect doesn't.
            buf: Vec::new(),
        })
    }

    /// The address of the `Op`, which doesn't change as an `Op` only lives in
    /// a `Box`.
    fn id(&self) -> OpId {
        OpId(NonNull::from(self))
    }

    /// Returns the `OVERLAPPED` of `op`, without dereferencing it.
    fn overlapped(op: *mut Op) -> *mut OVERLAPPED {
        // Both `Op` and `Overlapped` are `repr(C)` and start with it.
        op.cast()
    }

    /// Takes back the `Op` of a completed operation, and the pipe it kept
    /// alive.
    ///
    /// # Unsafety
    ///
    /// `overlapped` must be from the completion of an operation started by
    /// `Io::submit`. Afterwards only access the `Op` through the returned
    /// `Box`, not `overlapped`. The `overlapped` pointer is invalidated.
    unsafe fn complete(overlapped: *mut OVERLAPPED) -> (Box<Op>, Arc<Inner>) {
        let mut op = Box::from_raw(overlapped.cast::<Op>());
        let pipe = op
            .pipe
            .take()
            .expect("operation in flight without its pipe");
        (op, pipe)
    }
}

/// Identifies an operation in flight, without owning its `Op`.
#[derive(Clone, Copy, PartialEq, Eq)]
struct OpId(NonNull<Op>);

// SAFETY: an `OpId` is only compared and passed to `CancelIoEx`, never
// dereferenced.
unsafe impl Send for OpId {}

/// Number of idle `Op`s kept for reuse, so operations don't allocate.
const SPARE_OPS: usize = 2;

/// Largest buffer an idle `Op` keeps, so a pipe doesn't hold on to the memory of a
/// large write. Writes larger than this allocate their buffer every time.
const MAX_IDLE_BUF_SIZE: usize = 1024 * 1024;

/// The state of a pipe, behind `Inner::io`.
///
/// Operations are submitted, and `Io` is changed, only with it locked. A
/// completion first takes back its `Op` and the pipe reference it holds, then
/// locks `Io` before it compares the operation, changes any state or recycles
/// the `Op`. So while it's locked, an `Op` that `Io` tracks as in flight isn't
/// freed or reused, which is what lets `disconnect` and `drop` cancel and
/// forget operations.
struct Io {
    // Uniquely identifies the selector associated with this named pipe
    cp: Option<Arc<CompletionPort>>,
    // Token used to identify events
    token: Option<Token>,
    // The connect in flight, if any
    connect: Option<OpId>,
    connect_error: Option<io::Error>,
    read: ReadState,
    // Size of the next read, from the length of the caller's last read buffer
    read_size: usize,
    write: WriteState,
    spare: [Option<Box<Op>>; SPARE_OPS],
}

enum ReadState {
    /// Not reading: not connected or registered yet, or the read is cancelled.
    Idle,
    /// A read is in flight.
    InFlight(OpId),
    /// Data read into the `Op`'s buffer, of which the bytes from the position
    /// onward aren't handed out yet.
    Ready(Box<Op>, usize),
    /// The read failed, the error isn't handed out yet.
    Failed(io::Error),
}

enum WriteState {
    /// Ready for a write.
    Idle,
    /// A write is in flight, of the `Op`'s buffer from the position onward.
    InFlight(OpId, usize),
    /// The write failed, the error isn't handed out yet.
    Failed(io::Error),
}

// Odd tokens are for named pipes
static NEXT_TOKEN: AtomicUsize = AtomicUsize::new(1);

fn would_block() -> io::Error {
    io::ErrorKind::WouldBlock.into()
}

fn not_registered() -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        "I/O source not registered with `Registry`",
    )
}

impl NamedPipe {
    /// Creates a new named pipe at the specified `addr` given a "reasonable
    /// set" of initial configuration options.
    pub fn new<A: AsRef<OsStr>>(addr: A) -> io::Result<NamedPipe> {
        use std::os::windows::ffi::OsStrExt;
        let name: Vec<_> = addr.as_ref().encode_wide().chain(Some(0)).collect();

        // Safety: syscall
        let h = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE | FILE_FLAG_OVERLAPPED,
                PIPE_TYPE_BYTE,
                PIPE_UNLIMITED_INSTANCES,
                65536,
                65536,
                0,
                std::ptr::null_mut(),
            )
        };

        if h == INVALID_HANDLE_VALUE {
            Err(io::Error::last_os_error())
        } else {
            // Safety: nothing actually unsafe about this. The trait fn includes
            // `unsafe`.
            Ok(unsafe { Self::from_raw_handle(h as RawHandle) })
        }
    }

    /// Attempts to call `ConnectNamedPipe`, if possible.
    ///
    /// This function will attempt to connect this pipe to a client in an
    /// asynchronous fashion. If a client is already connected then `Ok(())` is
    /// returned. Otherwise a connection attempt is issued and a "would block"
    /// error is returned, also if Windows finishes it right away: its
    /// completion is still handled by the event loop.
    ///
    /// When the connection is finished then this object will be flagged as
    /// being ready for a write, or otherwise in the writable state.
    ///
    /// # Errors
    ///
    /// This function will return a "would block" error if the connection
    /// operation has previously been issued but has not yet completed, or if
    /// the connect itself was issued. If the pipe isn't registered with an
    /// event loop it returns a `NotFound` error.
    ///
    /// Normal I/O errors from the call to `ConnectNamedPipe` are returned
    /// immediately.
    pub fn connect(&self) -> io::Result<()> {
        let mut io = self.inner.io.lock().unwrap();

        // Make sure we're the only connect in flight.
        if io.connect.is_some() {
            return Err(would_block());
        }

        // Not registered: nothing would receive the connect's readiness event,
        // or before the first registration even its completion, leaking it.
        if io.token.is_none() {
            return Err(not_registered());
        }

        let op = io.op();
        // SAFETY: `connect_overlapped` returns `Ok` only if it submitted the
        // connect, `connect_done` takes the `Op` back. The `Op` is only used
        // for its `OVERLAPPED` pointer, before submitting.
        let res = unsafe {
            io.submit(op, &self.inner, connect_done, |op| {
                self.inner.connect_overlapped(Op::overlapped(op))
            })
        };

        match res {
            // `connect_done` flags the pipe writable once a client connects.
            Ok(op_id) => {
                io.connect = Some(op_id);
                Err(would_block())
            }

            // The connection is already established, so let's schedule
            // reads/writes and such.
            Err(ref e) if is_connected(e) => {
                Inner::post_register(&self.inner, &mut io, None);
                Ok(())
            }

            Err(e) => Err(e),
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
    /// transitively calling the `DisconnectNamedPipe` function. I/O in flight
    /// is cancelled and data not read yet is discarded, so nothing of this
    /// client reaches the next one.
    ///
    /// After a `disconnect` is issued, then a `connect` may be called again to
    /// connect to another client.
    pub fn disconnect(&self) -> io::Result<()> {
        let mut io = self.inner.io.lock().unwrap();
        self.inner.disconnect()?;

        // Nothing of this client may reach the next one: cancel what's in
        // flight, and drop what isn't handed out yet.
        io.connect_error = None;
        if let Some(op_id) = io.connect.take() {
            self.inner.cancel(op_id);
        }
        match mem::replace(&mut io.read, ReadState::Idle) {
            ReadState::InFlight(op_id) => self.inner.cancel(op_id),
            ReadState::Ready(op, _) => io.recycle(op),
            ReadState::Idle | ReadState::Failed(_) => {}
        }
        match mem::replace(&mut io.write, WriteState::Idle) {
            WriteState::InFlight(op_id, _) => self.inner.cancel(op_id),
            WriteState::Idle | WriteState::Failed(_) => {}
        }
        Ok(())
    }
}

impl FromRawHandle for NamedPipe {
    unsafe fn from_raw_handle(handle: RawHandle) -> NamedPipe {
        let max_read_size = if GetFileType(handle as HANDLE) == FILE_TYPE_PIPE {
            MAX_READ_SIZE
        } else {
            DEFAULT_READ_SIZE
        };
        NamedPipe {
            inner: Arc::new(Inner {
                event: Overlapped::new(event_done),
                handle: Handle::new(handle as HANDLE),
                io: Mutex::new(Io {
                    cp: None,
                    token: None,
                    connect: None,
                    connect_error: None,
                    read: ReadState::Idle,
                    read_size: DEFAULT_READ_SIZE,
                    write: WriteState::Idle,
                    spare: [None, None],
                }),
                max_read_size,
            }),
        }
    }
}

impl Read for NamedPipe {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        <&NamedPipe as Read>::read(&mut &*self, buf)
    }

    fn read_vectored(&mut self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        <&NamedPipe as Read>::read_vectored(&mut &*self, bufs)
    }
}

impl Write for NamedPipe {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        <&NamedPipe as Write>::write(&mut &*self, buf)
    }

    fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        <&NamedPipe as Write>::write_vectored(&mut &*self, bufs)
    }

    fn flush(&mut self) -> io::Result<()> {
        <&NamedPipe as Write>::flush(&mut &*self)
    }
}

impl NamedPipe {
    /// Reads from the internal read buffer using `f`, shared by `read` and
    /// `read_vectored`. `len` is the size of the caller's buffer(s).
    fn read_with(
        &self,
        len: usize,
        f: impl FnOnce(&mut &[u8]) -> io::Result<usize>,
    ) -> io::Result<usize> {
        let mut state = self.inner.io.lock().unwrap();

        // Size the next read from the pipe like the caller's buffer, so it can
        // be filled in one call.
        state.read_size = len.clamp(DEFAULT_READ_SIZE, self.inner.max_read_size);

        match mem::replace(&mut state.read, ReadState::Idle) {
            // Not registered: the event of a new read would never come.
            ReadState::Idle if state.token.is_none() => Err(not_registered()),

            // Not connected, or a read is in flight, still waiting for it to
            // finish.
            read @ (ReadState::Idle | ReadState::InFlight(_)) => {
                state.read = read;
                Err(would_block())
            }

            // We previously read something into `op`, try to copy out some
            // data. If we copy out all the data schedule a new read and
            // otherwise store the data to get read later.
            ReadState::Ready(op, cur) => {
                let n = {
                    let mut remaining = &op.buf[cur..];
                    f(&mut remaining)?
                };
                let next = cur + n;
                if next != op.buf.len() {
                    state.read = ReadState::Ready(op, next);
                } else {
                    state.recycle(op);
                    Inner::schedule_read(&self.inner, &mut state, None);
                }
                Ok(n)
            }

            // Looks like an in-flight read hit an error, return that here while
            // we schedule a new one.
            ReadState::Failed(e) => {
                Inner::schedule_read(&self.inner, &mut state, None);
                if e.raw_os_error() == Some(ERROR_BROKEN_PIPE as i32) {
                    Ok(0)
                } else {
                    Err(e)
                }
            }
        }
    }

    /// Writes all of `bufs`, shared by `write` and `write_vectored`.
    ///
    /// Takes byte slices, not `IoSlice`s: `IoSlice::new` panics on Windows for
    /// buffers over 4 GiB, and `write` must not panic.
    fn write_bufs<'b>(&self, bufs: impl Iterator<Item = &'b [u8]> + Clone) -> io::Result<usize> {
        // Make sure there's no writes pending
        let mut io = self.inner.io.lock().unwrap();

        match mem::replace(&mut io.write, WriteState::Idle) {
            // Not registered: the write's event would never come.
            WriteState::Idle if io.token.is_none() => return Err(not_registered()),
            WriteState::Idle => {}
            WriteState::Failed(e) => return Err(e),
            // `write_done` flags the pipe writable once the write finished.
            write @ WriteState::InFlight(..) => {
                io.write = write;
                return Err(would_block());
            }
        }

        // Move `bufs` onto the heap and fire off the write, which writes all of
        // it. If that fails the next write returns the error.
        let mut op = io.op();
        op.buf
            .reserve(bufs.clone().map(<[u8]>::len).fold(0, usize::saturating_add));
        for buf in bufs {
            op.buf.extend_from_slice(buf);
        }
        let len = op.buf.len();
        Inner::schedule_write(&self.inner, &mut io, op, 0)?;
        Ok(len)
    }
}

impl<'a> Read for &'a NamedPipe {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.read_with(buf.len(), |data| data.read(buf))
    }

    fn read_vectored(&mut self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        let len = bufs.iter().map(|buf| buf.len()).sum();
        self.read_with(len, |data| data.read_vectored(bufs))
    }
}

impl<'a> Write for &'a NamedPipe {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.write_bufs(iter::once(buf))
    }

    fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        self.write_bufs(bufs.iter().map(|buf| &**buf))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Source for NamedPipe {
    fn register(&mut self, registry: &Registry, token: Token, _: Interest) -> io::Result<()> {
        let mut io = self.inner.io.lock().unwrap();

        io.check_association(registry, false)?;

        if io.token.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "I/O source already registered with a `Registry`",
            ));
        }

        if io.cp.is_none() {
            let selector = registry.selector();

            let inner_token = NEXT_TOKEN.fetch_add(2, Relaxed) + 2;
            selector.inner.cp.add_handle(inner_token, self)?;

            // Only once associated, so a failed registration can be retried.
            io.cp = Some(selector.clone_port());
        }

        io.token = Some(token);
        Inner::post_register(&self.inner, &mut io, None);

        Ok(())
    }

    fn reregister(&mut self, registry: &Registry, token: Token, _: Interest) -> io::Result<()> {
        let mut io = self.inner.io.lock().unwrap();

        io.check_association(registry, true)?;

        io.token = Some(token);
        Inner::post_register(&self.inner, &mut io, None);

        Ok(())
    }

    fn deregister(&mut self, registry: &Registry) -> io::Result<()> {
        let mut io = self.inner.io.lock().unwrap();

        io.check_association(registry, true)?;

        if io.token.is_none() {
            return Err(not_registered());
        }

        io.token = None;
        Ok(())
    }
}

impl AsRawHandle for NamedPipe {
    fn as_raw_handle(&self) -> RawHandle {
        self.inner.handle.raw() as RawHandle
    }
}

impl fmt::Debug for NamedPipe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.handle.fmt(f)
    }
}

impl Drop for NamedPipe {
    fn drop(&mut self) {
        let mut io = self.inner.io.lock().unwrap();

        // Nothing receives this pipe's events anymore.
        io.token = None;

        // Cancel pending reads/connects, but don't cancel writes to ensure that
        // everything is flushed out. That only happens while the event loop
        // runs, see #1944.
        if let Some(op_id) = io.connect.take() {
            self.inner.cancel(op_id);
        }
        match mem::replace(&mut io.read, ReadState::Idle) {
            ReadState::InFlight(op_id) => self.inner.cancel(op_id),
            // Nothing reads it anymore.
            ReadState::Ready(..) | ReadState::Idle | ReadState::Failed(_) => {}
        }
    }
}

impl Inner {
    /// Schedules a read to happen in the background, executing an overlapped
    /// operation, unless a read is in flight, done or failed already.
    ///
    /// Returns `false` only if the pipe isn't connected (`ERROR_PIPE_LISTENING`),
    /// then no read is scheduled. A read that fails otherwise makes the pipe
    /// readable, to hand out the error.
    fn schedule_read(me: &Arc<Inner>, io: &mut Io, events: Option<&mut Vec<Event>>) -> bool {
        // Check to see if a read is already scheduled/completed
        if !matches!(io.read, ReadState::Idle) {
            return true;
        }

        let len = io.read_size;
        let mut op = io.op();
        op.buf.reserve_exact(len);
        // SAFETY: `read_overlapped` returns `Ok` only if it submitted the read,
        // `read_done` takes the `Op` back. The `Op` is only used for its
        // pointers, before submitting. The kernel owns the `Op`, and so its
        // buffer of at least `len` bytes, until the read completes.
        let res = unsafe {
            io.submit(op, me, read_done, |op| {
                me.read_overlapped((*op).buf.as_mut_ptr(), len, Op::overlapped(op))
            })
        };

        match res {
            Ok(op_id) => {
                io.read = ReadState::InFlight(op_id);
                true
            }

            // If ERROR_PIPE_LISTENING happens then it's not a real read error,
            // we just need to wait for a connect.
            Err(ref e) if e.raw_os_error() == Some(ERROR_PIPE_LISTENING as i32) => false,

            // If some other error happened, though, we're now readable to give
            // out the error.
            Err(e) => {
                io.read = ReadState::Failed(e);
                io.notify_readable(me, events);
                true
            }
        }
    }

    /// Schedules a write of the `op`'s buffer from `pos` onward to happen in
    /// the background, executing an overlapped operation.
    fn schedule_write(me: &Arc<Inner>, io: &mut Io, op: Box<Op>, pos: usize) -> io::Result<()> {
        // SAFETY: `write_overlapped` returns `Ok` only if it submitted the
        // write, `write_done` takes the `Op` back. The `Op` is only used for its
        // pointers, before submitting. The kernel owns the `Op`, and so its
        // buffer, until the write completes.
        let op_id = unsafe {
            io.submit(op, me, write_done, |op| {
                let buf = &(*op).buf;
                me.write_overlapped(buf.as_ptr().add(pos), buf.len() - pos, Op::overlapped(op))
            })
        }?;
        io.write = WriteState::InFlight(op_id, pos);
        Ok(())
    }

    /// Starts reading, and announces what's ready, once the pipe is registered
    /// or connected.
    ///
    /// A read is always kept in flight, as its completion is what makes the
    /// pipe readable. So this starts one, unless there is one already.
    fn post_register(me: &Arc<Inner>, io: &mut Io, mut events: Option<&mut Vec<Event>>) {
        // Data or an error read while not registered, its event went nowhere.
        let readable = matches!(io.read, ReadState::Ready(..) | ReadState::Failed(_));

        // Not connected yet: nothing is readable or writable until a client
        // connects, then `connect_done` calls this again.
        #[allow(clippy::needless_option_as_deref)]
        if !Inner::schedule_read(me, io, events.as_deref_mut()) {
            return;
        }

        if readable {
            io.notify_readable(me, events.as_deref_mut());
        }
        // Also when a write failed, so its error gets handed out. A write in
        // flight makes the pipe writable once it's done.
        if !matches!(io.write, WriteState::InFlight(..)) {
            io.notify_writable(me, events);
        }
    }
}

fn connect_done(status: &OVERLAPPED_ENTRY, events: Option<&mut Vec<Event>>) {
    let status = CompletionStatus::from_entry(status);

    // SAFETY: connects are started by `Io::submit`.
    let (mut op, me) = unsafe { Op::complete(status.overlapped()) };

    let mut io = me.io.lock().unwrap();
    if io.connect != Some(op.id()) {
        // Cancelled by `disconnect`, or by dropping the pipe.
        io.recycle(op);
        return;
    }
    io.connect = None;

    // Stash away our connect error if one happened
    debug_assert_eq!(status.bytes_transferred(), 0);
    if let Err(e) = me.result(&mut op) {
        io.connect_error = Some(e);
    }
    io.recycle(op);

    // We essentially just finished a registration, so kick off a
    // read and register write readiness.
    Inner::post_register(&me, &mut io, events);
}

fn read_done(status: &OVERLAPPED_ENTRY, events: Option<&mut Vec<Event>>) {
    let status = CompletionStatus::from_entry(status);

    // SAFETY: reads are started by `Io::submit`.
    let (mut op, me) = unsafe { Op::complete(status.overlapped()) };

    let mut io = me.io.lock().unwrap();
    if !matches!(io.read, ReadState::InFlight(id) if id == op.id()) {
        // Cancelled by `disconnect`, or by dropping the pipe.
        io.recycle(op);
        return;
    }

    io.read = match me.result(&mut op) {
        Ok(n) => {
            debug_assert_eq!(status.bytes_transferred() as usize, n);
            // SAFETY: the kernel read `n` bytes into the buffer, at most its
            // capacity.
            unsafe { op.buf.set_len(n) };
            ReadState::Ready(op, 0)
        }
        // This is non-fatal. The buffer was simply too small for the entire message.
        // Deliver the bytes we got, and if the caller wants to read the rest of the
        // message, they can initiate another read.
        Err(e) if e.raw_os_error() == Some(ERROR_MORE_DATA as i32) => {
            // SAFETY: as above.
            unsafe { op.buf.set_len(status.bytes_transferred() as usize) };
            ReadState::Ready(op, 0)
        }
        Err(e) => {
            debug_assert_eq!(status.bytes_transferred(), 0);
            io.recycle(op);
            ReadState::Failed(e)
        }
    };

    // Flag our readiness that we've got data.
    io.notify_readable(&me, events);
}

fn write_done(status: &OVERLAPPED_ENTRY, events: Option<&mut Vec<Event>>) {
    let status = CompletionStatus::from_entry(status);

    // SAFETY: writes are started by `Io::submit`.
    let (mut op, me) = unsafe { Op::complete(status.overlapped()) };

    let mut io = me.io.lock().unwrap();
    let pos = match io.write {
        WriteState::InFlight(id, pos) if id == op.id() => pos,
        // Cancelled by `disconnect`.
        _ => {
            io.recycle(op);
            return;
        }
    };
    io.write = WriteState::Idle;

    // If we wrote the entire buffer then we're writable again and otherwise we
    // schedule another write.
    match me.result(&mut op) {
        Ok(n) => {
            debug_assert_eq!(status.bytes_transferred() as usize, n);
            let pos = pos + n;
            if pos == op.buf.len() {
                // All written.
                io.recycle(op);
                io.notify_writable(&me, events);
            } else if n == 0 {
                // Nothing written, say to a full `PIPE_NOWAIT` pipe. Writing the
                // rest again would do the same, forever.
                io.recycle(op);
                io.write = WriteState::Failed(io::ErrorKind::WriteZero.into());
                io.notify_writable(&me, events);
            } else {
                // A short write, write the rest.
                let res = Inner::schedule_write(&me, &mut io, op, pos);
                if let Err(e) = res {
                    io.write = WriteState::Failed(e);
                    io.notify_writable(&me, events);
                }
            }
        }
        Err(e) => {
            debug_assert_eq!(status.bytes_transferred(), 0);
            io.recycle(op);
            io.write = WriteState::Failed(e);
            io.notify_writable(&me, events);
        }
    }
}

fn event_done(status: &OVERLAPPED_ENTRY, events: Option<&mut Vec<Event>>) {
    let status = CompletionStatus::from_entry(status);

    // SAFETY: the reference `Io::schedule_event` posted along with the event.
    let me = unsafe { Arc::from_raw(status.overlapped() as *const Inner) };

    let io = me.io.lock().unwrap();

    // Make sure the I/O handle is still registered with the selector
    if io.token.is_some() {
        // This method is also called during `Selector::drop` to perform
        // cleanup. In this case, `events` is `None` and we don't need to track
        // the event.
        if let Some(events) = events {
            let mut ev = Event::from_completion_status(status);
            // Reverse the `.data` alteration done in `schedule_event`. This
            // alteration was done so the selector recognized the event as one from
            // a named pipe.
            ev.data >>= 1;
            events.push(ev);
        }
    }
}

impl Io {
    /// Returns an idle `Op`, reusing a spare one if possible.
    fn op(&mut self) -> Box<Op> {
        // The last one kept, which is the most likely to have a buffer.
        self.spare
            .iter_mut()
            .rev()
            .find_map(Option::take)
            .unwrap_or_else(Op::new)
    }

    /// Keeps the idle `op` for reuse, if there's room.
    fn recycle(&mut self, mut op: Box<Op>) {
        if let Some(spare) = self.spare.iter_mut().find(|spare| spare.is_none()) {
            if op.buf.capacity() > MAX_IDLE_BUF_SIZE {
                op.buf = Vec::new();
            } else {
                op.buf.clear();
            }
            *spare = Some(op);
        }
    }

    /// Hands `op` to the kernel and submits an operation with `start`.
    ///
    /// If `start` returns `Ok` the operation is in flight, until its completion
    /// calls `done`, which takes `op` back. Otherwise `op` is recycled.
    ///
    /// # Unsafety
    ///
    /// `start` must return `Ok` if, and only if, it submitted an operation on
    /// this pipe using the `OVERLAPPED` of the `Op` it's given, so that exactly
    /// one completion for it is queued to the pipe's completion port. On `Err`
    /// nothing may still access the `Op`. `start` must not access the `Op` once
    /// the operation is submitted, as its completion may already be handled on
    /// another thread.
    ///
    /// `done` must take the `Op` back with `Op::complete`, exactly once.
    unsafe fn submit(
        &mut self,
        mut op: Box<Op>,
        pipe: &Arc<Inner>,
        done: fn(&OVERLAPPED_ENTRY, Option<&mut Vec<Event>>),
        start: impl FnOnce(*mut Op) -> io::Result<()>,
    ) -> io::Result<OpId> {
        // Owning the `Box` means the `Op` is idle: in flight it has no `Box`.
        debug_assert!(op.pipe.is_none(), "submitting an `Op` in flight");
        op.overlapped.callback = done;
        op.pipe = Some(pipe.clone());
        // Taken before submitting, as afterwards the completion may already
        // own the `Op`. The pointer is also the `OpId`.
        let op = NonNull::from(Box::leak(op));
        match start(op.as_ptr()) {
            Ok(()) => Ok(OpId(op)),
            Err(e) => {
                // Not submitted, so nothing else takes it back.
                let mut op = Box::from_raw(op.as_ptr());
                op.pipe = None;
                self.recycle(op);
                Err(e)
            }
        }
    }

    fn check_association(&self, registry: &Registry, required: bool) -> io::Result<()> {
        match self.cp {
            Some(ref cp) if !registry.selector().same_port(cp) => Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "I/O source already registered with a different `Registry`",
            )),
            None if required => Err(not_registered()),
            _ => Ok(()),
        }
    }

    fn notify_readable(&self, me: &Arc<Inner>, events: Option<&mut Vec<Event>>) {
        if let Some(token) = self.token {
            let mut ev = Event::new(token);
            ev.set_readable();

            if let Some(events) = events {
                events.push(ev);
            } else {
                self.schedule_event(me, ev);
            }
        }
    }

    fn notify_writable(&self, me: &Arc<Inner>, events: Option<&mut Vec<Event>>) {
        if let Some(token) = self.token {
            let mut ev = Event::new(token);
            ev.set_writable();

            if let Some(events) = events {
                events.push(ev);
            } else {
                self.schedule_event(me, ev);
            }
        }
    }

    fn schedule_event(&self, me: &Arc<Inner>, mut event: Event) {
        // Alter the token so that the selector will identify the IOCP event as
        // one for a named pipe. This will be reversed in `event_done`
        //
        // `data` for named pipes is an auto-incrementing counter. Because
        // `data` is `u64` we do not risk losing the most-significant bit
        // (unless a user creates 2^62 named pipes during the lifetime of the
        // process).
        event.data <<= 1;
        event.data += 1;

        // The event holds a reference to `Inner` until `event_done` takes it
        // back. `event` is the first field of `Inner`, so the pointer to
        // `Inner` points to its `OVERLAPPED` as well.
        let overlapped = Arc::into_raw(me.clone()) as *mut Overlapped;
        let completion_status = event.to_completion_status_with_overlapped(overlapped);

        if self.cp.as_ref().unwrap().post(completion_status).is_err() {
            // SAFETY: not posted, so `event_done` doesn't take it back.
            drop(unsafe { Arc::from_raw(overlapped as *const Inner) });
        }
    }
}
