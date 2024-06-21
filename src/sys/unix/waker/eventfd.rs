use std::fs::File;
use std::io::{self, Read, Write};
#[cfg(not(target_os = "hermit"))]
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
// TODO: once <https://github.com/rust-lang/rust/issues/126198> is fixed this
// can use `std::os::fd` and be merged with the above.
#[cfg(target_os = "hermit")]
use std::os::hermit::io::{AsRawFd, FromRawFd, RawFd};

/// Waker backed by `eventfd`.
///
/// `eventfd` is effectively an 64 bit counter. All writes must be of 8
/// bytes (64 bits) and are converted (native endian) into an 64 bit
/// unsigned integer and added to the count. Reads must also be 8 bytes and
/// reset the count to 0, returning the count.
#[derive(Debug)]
pub(crate) struct WakerInternal {
    fd: File,
}

impl WakerInternal {
    pub(crate) fn new() -> io::Result<WakerInternal> {
        #[cfg(not(target_os = "espidf"))]
        let flags = libc::EFD_CLOEXEC | libc::EFD_NONBLOCK;
        // ESP-IDF is EFD_NONBLOCK by default and errors if you try to pass this flag.
        #[cfg(target_os = "espidf")]
        let flags = 0;
        let fd = syscall!(eventfd(0, flags))?;

        let file = unsafe { File::from_raw_fd(fd) };
        Ok(WakerInternal { fd: file })
    }

    #[allow(clippy::unused_io_amount)] // Don't care about partial writes.
    pub(crate) fn wake(&self) -> io::Result<()> {
        let buf: [u8; 8] = 1u64.to_ne_bytes();
        match (&self.fd).write(&buf) {
            Ok(_) => Ok(()),
            Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => {
                // Writing only blocks if the counter is going to overflow.
                // So we'll reset the counter to 0 and wake it again.
                self.reset()?;
                self.wake()
            }
            Err(err) => Err(err),
        }
    }

    #[cfg(any(
        mio_unsupported_force_poll_poll,
        target_os = "espidf",
        target_os = "fuchsia",
        target_os = "hermit",
    ))]
    pub(crate) fn ack_and_reset(&self) {
        let _ = self.reset();
    }

    /// Reset the eventfd object, only need to call this if `wake` fails.
    #[allow(clippy::unused_io_amount)] // Don't care about partial reads.
    fn reset(&self) -> io::Result<()> {
        let mut buf: [u8; 8] = 0u64.to_ne_bytes();
        match (&self.fd).read(&mut buf) {
            Ok(_) => Ok(()),
            // If the `Waker` hasn't been awoken yet this will return a
            // `WouldBlock` error which we can safely ignore.
            Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => Ok(()),
            Err(err) => Err(err),
        }
    }
}

impl AsRawFd for WakerInternal {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}
