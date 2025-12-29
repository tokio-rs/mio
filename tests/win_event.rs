#![cfg(all(windows, feature = "os-extended"))]

#[macro_use]
mod util;

use std::io::{self};
use std::os::windows::io::{AsHandle, FromRawHandle, OwnedHandle};
use std::ptr::null;
use std::os::windows::io::AsRawHandle;
use std::time::{Duration, Instant};

use mio::windows::{SourceEventHndl, SourceHndl};
use mio::{Events, Interest, Poll, Token};
use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::System::Threading::
{
    CreateWaitableTimerExW, EVENT_ALL_ACCESS, SetWaitableTimer, WaitForSingleObject
};

#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HANDLE(pub *mut core::ffi::c_void);
impl HANDLE 
{
    pub 
    fn try_into_owned(self) -> Result<OwnedHandle, String>
    {
        if self.0 == -1 as _ || self.0 == 0 as _
        {
            return Err(format!("invalid handle!"));
        }
        else
        {
            return Ok(unsafe { OwnedHandle::from_raw_handle(self.0) });
        }
    }
}

#[derive(Debug)]
pub struct PrimitiveTimer
{
    hndl_timer: OwnedHandle
}

impl AsHandle for PrimitiveTimer
{
    fn as_handle(&self) -> std::os::windows::prelude::BorrowedHandle<'_> 
    {
        return self.hndl_timer.as_handle();
    }
}

impl PrimitiveTimer
{
    fn new(name: &str) -> PrimitiveTimer
    {
        let mut label_cstr: Vec<u16> = name.encode_utf16().collect();
        label_cstr.push(0);

        let hndl_timer = 
            unsafe
            { 
                HANDLE(
                    CreateWaitableTimerExW(
                        null(),  
                        label_cstr.as_ptr(),
                        0,
                        EVENT_ALL_ACCESS
                    )
                )
                .try_into_owned()
                .unwrap()
            };

        return Self{ hndl_timer: hndl_timer};
    }

    fn arm_relative(&self, timeout: i64)  -> io::Result<()>
    {
        let time: i64 = timeout / 100;
        let res = 
        unsafe
        {
            SetWaitableTimer(
                self.hndl_timer.as_raw_handle(), 
                &time as *const i64,
                0,
                None,
                null(),
                false.into()
            )
        };

        if res != 0
        {
            return Ok(());
        }
        else
        {
            return Err(io::Error::last_os_error());
        }
    }

    fn poll(&self) -> Result<usize, io::Error>
    {
        let res =
            unsafe { WaitForSingleObject(self.hndl_timer.as_raw_handle(), 0) };

        if res == WAIT_OBJECT_0
        {
            return Ok(0);
        }
        else if res == WAIT_TIMEOUT
        {
            // would block
            return Ok(1);
        }
        else
        {            
            return Err(io::Error::last_os_error());
        }
    }
}

#[test]
fn test_event_win_simple()
{
// timer 1 
    let mut se_hndl_timer = 
        SourceEventHndl::new(PrimitiveTimer::new("timer_6")).unwrap();

    // poll
    let mut poll = Poll::new().unwrap();

    // registering
    poll.registry().register(&mut se_hndl_timer, Token(1), Interest::READABLE).unwrap();

    // set timer to relative 100ms step
    se_hndl_timer.inner().arm_relative(-100_000).unwrap();

    // --- poll
    let mut events = Events::with_capacity(2);

    println!("poll timer 1");
    poll.poll(&mut events, None).unwrap();

    assert_eq!(events.is_empty(), false);
    
    let mut event_iter = events.iter();

    let ev0 = event_iter.next();

    assert_eq!(ev0.is_some(), true);
    assert_eq!(ev0.as_ref().unwrap().token(), Token(1));

    assert_eq!(event_iter.next().is_none(), true);

     // set timer to relative 100ms step
    se_hndl_timer.inner().arm_relative(-100_000_000).unwrap();

    // --- poll
    let mut events = Events::with_capacity(2);

    println!("poll timer 2");
    poll.poll(&mut events, Some(Duration::from_millis(200))).unwrap();

    drop(se_hndl_timer);

    println!("poll timer drop timer");
    poll.poll(&mut events, Some(Duration::from_millis(200))).unwrap();

    assert_eq!(events.is_empty(), true);

    println!("poll timer last");
    poll.poll(&mut events, Some(Duration::from_millis(200))).unwrap();

}

#[test]
fn test_event_win_simple2()
{
// timer 1 
    let mut se_hndl_timer = 
        SourceEventHndl::new(PrimitiveTimer::new("timer_8888")).unwrap();

    // poll
    let mut poll = Poll::new().unwrap();

    // registering
    poll.registry().register(&mut se_hndl_timer, Token(1), Interest::READABLE).unwrap();

    // set timer to relative 100ns step = 500ms
    se_hndl_timer.inner().arm_relative(-500_000_000).unwrap();

    // --- poll
    let mut events = Events::with_capacity(2);

    println!("poll timer 1");
    
    let s = Instant::now();
    
    poll.poll(&mut events, Some(Duration::from_millis(900))).unwrap();

    let e = s.elapsed();
    println!("{}", e.as_millis());
    assert!(e.as_millis() > 480 && e.as_millis() < 550, "{}", e.as_millis());
    

    assert_eq!(events.is_empty(), false);
    
    let mut event_iter = events.iter();

    let ev0 = event_iter.next();

    assert_eq!(ev0.is_some(), true);
    assert_eq!(ev0.as_ref().unwrap().token(), Token(1));

    assert_eq!(event_iter.next().is_none(), true);

     // set timer to relative 100ms step
    se_hndl_timer.inner().arm_relative(-100_000_000).unwrap();

    // --- poll
    let mut events = Events::with_capacity(2);

    println!("poll timer 2");
    poll.poll(&mut events, Some(Duration::from_millis(200))).unwrap();

    poll.registry().deregister(&mut se_hndl_timer).unwrap();

    println!("poll timer dereg");
    poll.poll(&mut events, Some(Duration::from_millis(200))).unwrap();

    assert_eq!(events.is_empty(), true);

    println!("poll timer last");
    poll.poll(&mut events, Some(Duration::from_millis(200))).unwrap();
}

#[test]
fn test_event_win_try_check_packet_cancel_ordeing_racing()
{
    // timer 1 
    let mut se_hndl_timer = 
        SourceEventHndl::new(PrimitiveTimer::new("timer_3")).unwrap();

    // poll
    let mut poll = Poll::new().unwrap();

    // registering
    poll.registry().register(&mut se_hndl_timer, Token(1), Interest::READABLE).unwrap();

    // set timer to relative 100ns (minimum)
    se_hndl_timer.inner().arm_relative(-100).unwrap();
    
    std::thread::sleep(Duration::from_nanos(1000));

    // now deregister the instance to generate the cancel before timer
    poll.registry().deregister(&mut se_hndl_timer).unwrap();

    assert_eq!(se_hndl_timer.get_token(), None);

    // --- poll
    let mut events = Events::with_capacity(2);

    println!("poll timer after timer drop");
    poll.poll(&mut events, Some(Duration::from_millis(200))).unwrap();

    assert_eq!(events.is_empty(), true);
    assert_eq!(se_hndl_timer.inner().poll().ok(), Some(1));

    println!("end of test");
    drop(se_hndl_timer);

}

#[test]
fn test_event_win_try_check_packet_cancel_ordeing_racing_ev()
{
    // timer 1 
    let se_hndl_timer = PrimitiveTimer::new("timer_1");

    // poll
    let mut poll = Poll::new().unwrap();

    // registering
    poll.registry().register(&mut SourceHndl::new(&se_hndl_timer).unwrap(), Token(1), Interest::READABLE).unwrap();

    // set timer to relative 100ns (minimum)
    se_hndl_timer.arm_relative(-100).unwrap();
    
    std::thread::sleep(Duration::from_nanos(100));
    
    // now deregister the instance to generate the cancel before timer
    poll.registry().deregister(&mut SourceHndl::new(&se_hndl_timer).unwrap()).unwrap();

    // --- poll
    let mut events = Events::with_capacity(2);

    println!("poll timer after timer drop");
    poll.poll(&mut events, Some(Duration::from_millis(200))).unwrap();

    assert_eq!(events.is_empty(), true);

    assert_eq!(se_hndl_timer.poll().ok(), Some(1));

    println!("end of test");
    drop(se_hndl_timer);

}

#[test]
fn test_event_win_different_thread()
{
    // timer 1 
    let mut se_hndl_timer = 
        SourceEventHndl::new(PrimitiveTimer::new("timer_11")).unwrap();

    let mut events = Events::with_capacity(2);

    // poll
    let mut poll = Poll::new().unwrap();

    // registering
    poll.registry().register(&mut se_hndl_timer, Token(1), Interest::READABLE).unwrap();

    let (tx, rx) = std::sync::mpsc::channel::<()>();

    std::thread::spawn(move ||
        {
            // set timer to relative 100ns (minimum)
            se_hndl_timer.inner().arm_relative(-1_000_000_000).unwrap(); // 1 sec

            std::thread::sleep(Duration::from_millis(1600));

            se_hndl_timer.inner().arm_relative(-1_000).unwrap(); // 10 us

            std::thread::sleep(Duration::from_millis(1));

            drop(se_hndl_timer);

            tx.send(()).unwrap();
            return;
        }
    );

    let s = Instant::now();

    poll.poll(&mut events, Some(Duration::from_millis(1200))).unwrap();

    let e = s.elapsed();

    let mut event_iter = events.iter();
    
    let ev0 = event_iter.next();
    assert_eq!(ev0.is_some(), true);
    assert_eq!(ev0.as_ref().unwrap().token(), Token(1));

    assert_eq!(event_iter.next().is_none(), true);

    println!("time took: {:?}", e);
    assert!(e.as_millis() >= 900 && e.as_millis() < 1200, "{}", e.as_millis());

    rx.recv_timeout(Duration::from_secs(1)).unwrap();

    poll.poll(&mut events, Some(Duration::from_millis(100))).unwrap();

    assert_eq!(events.is_empty(), true);

    return;

}

