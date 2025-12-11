#![cfg(all(windows, feature = "os-poll", feature = "os-extended"))]

#[macro_use]
mod util;

use std::collections::HashSet;
use std::io::ErrorKind;
use std::mem;
use std::os::windows::io::{AsHandle, FromRawHandle, OwnedHandle};
use std::ptr::null;
use std::os::windows::io::AsRawHandle;
use std::time::{Duration, Instant};
use mio::net::UdpSocket;
use mio::windows::{SourceEventHndl};
use mio::{Events, Interest, Poll, Token};
use windows_sys::Win32::System::Threading::
{
    CreateEventA, CreateWaitableTimerExW, EVENT_ALL_ACCESS, SetEvent, SetWaitableTimer
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

    fn arm_relative(&self, timeout: i64) 
    {
        let time: i64 = timeout / 100;
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
    }
}

#[test]
fn test_event_win()
{
    // timer 1
    let mut se_hndl_timer = 
        SourceEventHndl::new(PrimitiveTimer::new("timer_1")).unwrap();

    // timer 2
    let mut se_hndl_timer2 = 
        SourceEventHndl::new(PrimitiveTimer::new("timer_2")).unwrap();

    // a control UDP socket
    let mut udp_addr = util::any_local_address();
    udp_addr.set_port(40000);
    
    let mut udp_s1 = UdpSocket::bind(udp_addr).unwrap();
    println!("{:?}", udp_addr);

    // udp sender (sends to control UDP socket)
    let mut udp_addr2 = util::any_local_address();
    udp_addr2.set_port(40001);
    let udp_sender = std::net::UdpSocket::bind(udp_addr2).unwrap();
   
    // simple event
    let hndl_event = 
        unsafe
        {
            HANDLE(
                CreateEventA(
                    null(), 
                    false.into(), 
                    false.into(),  
                    mem::transmute("test_event\0".as_ptr())
                )
            )
            .try_into_owned()
            .unwrap()
        };

    
    let mut se_hndl_event = SourceEventHndl::new(hndl_event).unwrap();


    // poll
    let mut poll = Poll::new().unwrap();

    // registering
    poll.registry().register(&mut se_hndl_timer, Token(1), Interest::READABLE).unwrap();

    // check double registration
    let fail = poll.registry().register(&mut se_hndl_timer, Token(8), Interest::READABLE);
    assert_eq!(fail.is_err(), true);
    assert_eq!(fail.err().as_ref().unwrap().kind(), ErrorKind::ResourceBusy);

    // rest
    poll.registry().register(&mut se_hndl_timer2, Token(2), Interest::READABLE).unwrap();
    poll.registry().register(&mut se_hndl_event, Token(3), Interest::READABLE).unwrap();
    poll.registry().register(&mut udp_s1, Token(10), Interest::READABLE | Interest::WRITABLE).unwrap();

    let mut events = Events::with_capacity(2);
    poll.poll(&mut events, Some(Duration::from_millis(200))).unwrap();

    // set timer to relative 100ms step
    se_hndl_timer.inner().arm_relative(-100_000_000);

    // --- poll
    let mut events = Events::with_capacity(2);

    println!("poll timer 1");
    poll.poll(&mut events, Some(Duration::from_millis(200))).unwrap();

    assert_eq!(events.is_empty(), false);
    
    let mut event_iter = events.iter();

    let ev0 = event_iter.next();

    assert_eq!(ev0.is_some(), true);
    assert_eq!(ev0.as_ref().unwrap().token(), Token(1));

    assert_eq!(event_iter.next().is_none(), true);

    // ---
    se_hndl_timer.inner().arm_relative(-200_000_000);
    se_hndl_timer2.inner().arm_relative(-250_000_000);
  
    // --- poll
    let mut events = Events::with_capacity(2);
    let mut exp_ev: HashSet<usize> = [1, 2].into_iter().collect();

    for i in 0..2
    {
        println!("poll timer 2, round: {}", i);
        poll.poll(&mut events, Some(Duration::from_millis(300))).unwrap();

        assert_eq!(events.is_empty(), false);

        let mut event_iter = events.iter();

        let ev0 = event_iter.next();

        assert_eq!(ev0.is_some(), true);

        let exp_val = exp_ev.remove(&ev0.as_ref().unwrap().token().0);

        assert_eq!(exp_val, true);

        assert_eq!(event_iter.next().is_none(), true);
    }


    unsafe 
    {
        SetEvent(se_hndl_event.as_raw_handle())
    };

    udp_sender.send_to(&[1, 2, 3], udp_addr).unwrap();

    let mut exp_ev: HashSet<usize> = [3, 10].into_iter().collect();

    println!("poll event/udp");
    poll.poll(&mut events, Some(Duration::from_millis(300))).unwrap();

    assert_eq!(events.is_empty(), false);

    for ev in events.iter()
    {
        println!("poll event/udp event: {:?}", ev);

        assert_eq!(exp_ev.remove(&ev.token().0), true);
    }

    assert_eq!(exp_ev.is_empty(), true);
    

    // empty poll

    println!("poll empty");
    poll.poll(&mut events, Some(Duration::from_millis(300))).unwrap();

    assert_eq!(events.is_empty(), true);


    // unregister

    println!("unregister test poll");
    poll.registry().deregister(&mut se_hndl_timer2).unwrap();

    // set unreg timer
    se_hndl_timer2.inner().arm_relative(-200_000_000);

    // poll res must be empty
    poll.poll(&mut events, Some(Duration::from_millis(300))).unwrap();

    assert_eq!(events.is_empty(), true);

    // reregister with different token
    println!("reregister test poll");

    poll.registry().reregister(&mut se_hndl_timer, Token(7), Interest::READABLE).unwrap();

    // set unreg timer
    se_hndl_timer.inner().arm_relative(-1_000_000_000); // 1 sec

    let s = Instant::now();

    // poll res must be empty
    poll.poll(&mut events, Some(Duration::from_millis(1100))).unwrap();

    let e = s.elapsed();

    assert_eq!(events.is_empty(), false);
    
    let mut event_iter = events.iter();

    let ev0 = event_iter.next();

    assert_eq!(ev0.is_some(), true);
    assert_eq!(ev0.as_ref().unwrap().token(), Token(7));

    assert_eq!(event_iter.next().is_none(), true);

    println!("time took: {:?}", e);
    assert!(e.as_millis() >= 900 && e.as_millis() < 1200);

    return;
}