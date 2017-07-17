use {io, poll, Evented, Ready, Poll, PollOpt, Token};
use magenta::HandleBase;
use std::sync::Mutex;

/// Wrapper for registering a `HandleBase` type with mio.
#[derive(Debug)]
pub struct EventedHandle<T> where T: HandleBase {
    /// The handle to be registered.
    handle: T,

    /// The current `Token` with which the handle is registered with mio.
    token: Mutex<Option<Token>>,
}

impl<T> EventedHandle<T> where T: HandleBase {
    /// Create a new `EventedHandle` which can be registered with mio
    /// in order to receive event notifications.
    pub fn new(handle: T) -> Self {
        EventedHandle {
            handle: handle,
            token: Mutex::new(None),
        }
    }

    /// Get a reference to the underlying `HandleBase`.
    pub fn get_ref(&self) -> &T {
        &self.handle
    }

    /// Get a mutable reference to the underlying `HandleBase`.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.handle
    }

    /// Convert back into the inner `HandleBase`.
    pub fn into_inner(self) -> T {
        self.handle
    }
}

impl<T> Evented for EventedHandle<T> where T: HandleBase {
    fn register(&self,
                poll: &Poll,
                token: Token,
                interest: Ready,
                opts: PollOpt) -> io::Result<()>
    {
        let mut this_token = self.token.lock().unwrap();
        {
            poll::selector(poll).register_handle(&self.handle, token, interest, opts)?;
            *this_token = Some(token);
        }
        Ok(())
    }

    fn reregister(&self,
        poll: &Poll,
        token: Token,
        interest: Ready,
        opts: PollOpt) -> io::Result<()>
    {
        let mut this_token = self.token.lock().unwrap();
        {
            poll::selector(poll).deregister_handle(&self.handle, token)?;
            *this_token = None;
            poll::selector(poll).register_handle(&self.handle, token, interest, opts)?;
            *this_token = Some(token);
        }
        Ok(())
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        let mut this_token = self.token.lock().unwrap();
        let token = if let Some(token) = *this_token { token } else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Attempted to deregister an unregistered handle."))
        };
        {
            poll::selector(poll).deregister_handle(&self.handle, token)?;
            *this_token = None;
        }
        Ok(())
    }
}
