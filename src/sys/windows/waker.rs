use crate::sys::windows::Event;
use crate::sys::windows::Selector;
use crate::Token;
use crate::sys::windows::iocp::CompletionStatus;
use crate::sys::windows::tokens::TokenGenerator;
use crate::sys::windows::tokens::TokenSelector;
use crate::sys::windows::tokens::WakerTokenId;

use windows_sys::Win32::System::IO::OVERLAPPED;
use windows_sys::Win32::System::IO::OVERLAPPED_ENTRY;

use super::iocp::CompletionPort;
use std::io;
use std::sync::Arc;
use std::sync::Weak;
//use std::sync::Weak;

/// Uniq token generator for the waker.
static WAKER_TOKEN: TokenGenerator<WakerTokenId> = TokenGenerator::new();

#[repr(C)]
#[derive(Debug)]
struct OverlapWrapper
{
    pad: [u8; size_of::<OVERLAPPED>()],
    user_token: Token
}

impl OverlapWrapper
{
    fn new(user_token: Token) -> Self
    {
        return Self{ pad: [0_u8; size_of::<OVERLAPPED>()], user_token: user_token };
    }
}

#[derive(Debug)]
pub struct Waker 
{
    overlapped: Arc<OverlapWrapper>,
    internal_token: WakerTokenId,
    port: Arc<CompletionPort>,
}

impl Waker {
    pub fn new(selector: &Selector, token: Token) -> io::Result<Waker> 
    {            
        Ok(
            Waker 
            {
                overlapped: Arc::new(OverlapWrapper::new(token)),
                internal_token: WAKER_TOKEN.next(),
                port: selector.clone_port(),
            }
        )
    }

    pub fn wake(&self) -> io::Result<()> 
    {
        let mut ev = Event::new(self.internal_token.get_token());
        ev.set_readable();

        let weak_overlapped = Arc::downgrade(&self.overlapped).into_raw();
        
        return 
            self.port.post(ev.to_completion_status_with_overlapped(weak_overlapped.cast_mut() as *mut _));
    }

    pub(super)
    fn from_overlapped(status: &OVERLAPPED_ENTRY, opt_events: Option<&mut Vec<Event>>)
    {
        let cp_status = CompletionStatus::from_entry(status);

        let Some(overlap) = 
            unsafe 
            {
                Weak::<OverlapWrapper>::from_raw(cp_status.overlapped() as *const OverlapWrapper) 
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
            ev.data = overlap.user_token.0 as u64;

            events.push(ev);
        }
    }
}
