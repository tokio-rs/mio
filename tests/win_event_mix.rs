#![cfg(all(windows, feature = "os-extended", feature = "net"))]

#[macro_use]
mod util;

use std::collections::HashSet;
use std::io::{self, ErrorKind};
use std::mem;
use std::os::windows::io::{AsHandle, FromRawHandle, OwnedHandle};
use std::ptr::null;
use std::os::windows::io::AsRawHandle;
use std::time::{Duration, Instant};

use mio::windows::{SourceEventHndl, SourceHndl};
use mio::{Events, Interest, Poll, Token};
use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::System::Threading::
{
    CreateEventA, CreateWaitableTimerExW, EVENT_ALL_ACCESS, SetEvent, SetWaitableTimer, WaitForSingleObject
};

use mio::net::UdpSocket;

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
fn test_event_win_mixed()
{


    // timer 1 
    let mut se_hndl_timer = 
        SourceEventHndl::new(PrimitiveTimer::new("timer_2001")).unwrap();

    // timer 2
    let mut se_hndl_timer2 = 
        SourceEventHndl::new(PrimitiveTimer::new("timer_2002")).unwrap();

    // a control UDP socket
    let mut udp_addr = util::any_local_address();
    udp_addr.set_port(40002);
    
    let mut udp_s1 = UdpSocket::bind(udp_addr).unwrap();
    println!("{:?}", udp_addr);

    // udp sender (sends to control UDP socket)
    let mut udp_addr2 = util::any_local_address();
    udp_addr2.set_port(40003);
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
    assert_eq!(fail.err().as_ref().unwrap().kind(), ErrorKind::AlreadyExists);

    // rest
    poll.registry().register(&mut se_hndl_timer2, Token(2), Interest::READABLE).unwrap();
    poll.registry().register(&mut se_hndl_event, Token(3), Interest::READABLE).unwrap();
    
    #[cfg(feature = "net")]
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


    // --------------------- unregister

    println!("unregister test poll");
    poll.registry().deregister(&mut se_hndl_timer2).unwrap();

    // set unreg timer
    se_hndl_timer2.inner().arm_relative(-200_000_000);

    // poll res must be empty
    poll.poll(&mut events, Some(Duration::from_millis(300))).unwrap();

    assert_eq!(events.is_empty(), true);

    // ------------------ reregister with different token
    println!("reregister test poll");

    poll.registry().reregister(&mut se_hndl_timer, Token(7), Interest::READABLE).unwrap();

    // set unreg timer
    se_hndl_timer.inner().arm_relative(-1_000_000_000); // 1 sec

    let s = Instant::now();

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

    // ------------------ into inner test
    println!("into inner test");

    se_hndl_timer.inner().arm_relative(-1_000_000_000); // 1.5 sec

    let hndl_timer = se_hndl_timer.try_into_inner().unwrap();

    // poll res must be empty
    poll.poll(&mut events, Some(Duration::from_millis(1100))).unwrap();

    assert_eq!(events.is_empty(), true);

    drop(hndl_timer);

    return;
}

#[test]
fn test_event_win2()
{
    // timer 1 
    let se_hndl_timer = PrimitiveTimer::new("timer_128");

    // timer 2
    let se_hndl_timer2 = PrimitiveTimer::new("timer_298");

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
    let se_hndl_event = 
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

    // poll
    let mut poll = Poll::new().unwrap();

    // registering
    poll.registry().register(&mut SourceHndl::new(&se_hndl_timer).unwrap(), Token(1), Interest::READABLE).unwrap();

    // check double registration
    let fail = poll.registry().register(&mut SourceHndl::new(&se_hndl_timer).unwrap(), Token(8), Interest::READABLE);
    assert_eq!(fail.is_err(), true);
    assert_eq!(fail.err().as_ref().unwrap().kind(), ErrorKind::AlreadyExists);

    // rest
    poll.registry().register(&mut SourceHndl::new(&se_hndl_timer2).unwrap(), Token(2), Interest::READABLE).unwrap();
    poll.registry().register(&mut SourceHndl::new(&se_hndl_event).unwrap(), Token(3), Interest::READABLE).unwrap();
    
    #[cfg(feature = "net")]
    poll.registry().register(&mut udp_s1, Token(10), Interest::READABLE | Interest::WRITABLE).unwrap();


    let mut events = Events::with_capacity(2);
    poll.poll(&mut events, Some(Duration::from_millis(200))).unwrap();

    // set timer to relative 100ms step
    se_hndl_timer.arm_relative(-100_000_000);

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

    // ------------------

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

    assert_eq!(exp_ev.is_empty(), true, "{:?}", exp_ev);
    
    // --------------------- unregister

    println!("unregister test poll");
    poll.registry().deregister(&mut SourceHndl::new(&se_hndl_timer2).unwrap()).unwrap();

    // set unreg timer
    se_hndl_timer2.arm_relative(-200_000_000);

    // poll res must be empty
    poll.poll(&mut events, Some(Duration::from_millis(300))).unwrap();

    assert_eq!(events.is_empty(), true);

    // ------------------ reregister with different token
    println!("reregister test poll");

    poll.registry().reregister(&mut SourceHndl::new(&se_hndl_timer).unwrap(), Token(7), Interest::READABLE).unwrap();

    // set unreg timer
    se_hndl_timer.arm_relative(-1_000_000_000); // 1 sec

    let s = Instant::now();

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

}

#[test]
fn test_event_mix()
{
    // timer 1 
    let se_hndl_timer = PrimitiveTimer::new("timer_1001");

    // timer 2
    let mut se_hndl_timer2 = 
        SourceEventHndl::new(PrimitiveTimer::new("timer_1002")).unwrap();

    // poll
    let mut poll = Poll::new().unwrap();

    // registering
    poll.registry().register(&mut SourceHndl::new(&se_hndl_timer).unwrap(), Token(1), Interest::READABLE).unwrap();
    poll.registry().register(&mut se_hndl_timer2, Token(2), Interest::READABLE).unwrap();

    se_hndl_timer.arm_relative(-400_000_000);
    se_hndl_timer2.inner().arm_relative(-500_000_000);

    let mut exp_times = vec![2, 1];

    let mut events = Events::with_capacity(2);

    while exp_times.is_empty() == false
    {
        println!("poll timer 1");
        poll.poll(&mut events, Some(Duration::from_millis(500))).unwrap();

        let mut eve_iter = events.iter();

        let ev = eve_iter.next().unwrap();

        assert_eq!(ev.token().0, exp_times.pop().unwrap());

        assert_eq!(eve_iter.next().is_none(), true);
    }
}