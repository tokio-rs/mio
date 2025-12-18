use windows_sys::Win32::System::IO::OVERLAPPED_ENTRY;

use crate::sys::windows::Event;
use crate::sys::windows::Selector;
use crate::Token;
//use crate::sys::windows::iocp::CompletionStatus;
use crate::sys::windows::tokens::TokenGenerator;
use crate::sys::windows::tokens::WakerTokenId;
use crate::sys::windows::tokens::TokenSelector;

use super::iocp::CompletionPort;
use std::io;
use std::sync::Arc;
use std::sync::Weak;

/// Uniq token generator for the waker.
static WAKER_TOKEN: TokenGenerator<WakerTokenId> = TokenGenerator::new();

#[derive(Debug)]
pub struct Waker 
{
    user_token: Arc<Token>,
    internal_token: WakerTokenId,
    port: Arc<CompletionPort>,
}

impl Waker {
    pub fn new(selector: &Selector, token: Token) -> io::Result<Waker> {
        Ok(
            Waker 
            {
                user_token: Arc::new(token),
                internal_token: WAKER_TOKEN.next(),
                port: selector.clone_port(),
            }
        )
    }

    pub fn wake(&self) -> io::Result<()> 
    {
        let mut ev = Event::new(self.internal_token.get_token());
        ev.set_readable();

        let weak_token = Weak::into_raw(Arc::downgrade(&self.user_token));
        self.port.post(ev.to_completion_status_with_overlapped(weak_token.cast_mut() as *mut _))
    }

    pub(super)
    fn from_overlapped(_status: &OVERLAPPED_ENTRY, _opt_events: Option<&mut Vec<Event>>)
    {
        /*let cp_status = CompletionStatus::from_entry(status);

        let Some(user_token) = 
            unsafe 
            {
                Weak::<Token>::from_raw(cp_status.overlapped() as *const Token) 
            }
            .upgrade()
            else
            {
                // the owner of the object have dropped it. Ignore
                return;
            };

        if let Some(events) = opt_events 
        {
            let mut ev = Event::from_completion_status(&cp_status); 

            // replace to internal
            ev.data = user_token.0 as u64;

            events.push(ev);
        }*/
    }
}
