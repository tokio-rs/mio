// You can run this example from the root of the mio repo:
// cargo run --example udp_server --features="os-poll net"

use std::io;

#[cfg(target_os = "windows")]
pub mod os_spec
{
    
    use std::{io, os::windows::io::{AsHandle, AsRawHandle, FromRawHandle, OwnedHandle}, ptr::null};
    use mio::{Events, Interest, Poll, Token, net::UdpSocket, windows::SourceEventHndl};
    use log::warn;
    use windows_sys::Win32::System::Threading::{CreateWaitableTimerExW, EVENT_ALL_ACCESS, SetWaitableTimer};
    

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

    /// Our instace that we want to poll
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

    // A token to allow us to identify which event is for the `UdpSocket`.
    const UDP_SOCKET: Token = Token(0);
    const TIMER_EVENT: Token = Token(1);

    pub 
    fn main1() -> io::Result<()> 
    {
        env_logger::init();

        // Create a poll instance.
        let mut poll = Poll::new()?;
        // Create storage for events. Since we will only register a single socket, a
        // capacity of 1 will do.
        let mut events = Events::with_capacity(1);

        // Setup the UDP socket.
        let addr = "127.0.0.1:9000".parse().unwrap();

        let socket = UdpSocket::bind(addr)?;

        // Setup timer
        let mut se_hndl_timer = 
            SourceEventHndl::new(PrimitiveTimer::new("timer_1")).unwrap();

        // Register our socket with the token defined above and an interest in being
        // `READABLE`.
        poll
            .registry()
            .register(&mut se_hndl_timer, TIMER_EVENT, Interest::READABLE)?;

        println!("You can connect to the server using `nc`:");
        println!(" $ nc -u 127.0.0.1 9000");
        println!("Anything you type will be echoed back to you.");

        // Initialize a buffer for the UDP packet. We use the maximum size of a UDP
        // packet, which is the maximum value of a 16-bit integer (65536).
        let mut buf = [0; 1 << 16];

        // set connection timeout
        se_hndl_timer.inner().arm_relative(-5_000_000_000); // 5 sec relative

        // Our event loop.
        loop {
            // Poll to check if we have events waiting for us.
            if let Err(err) = poll.poll(&mut events, None) {
                if err.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(err);
            }

            // Process each event.
            for event in events.iter() {
                // Validate the token we registered our socket with,
                // in this example it will only ever be one but we
                // make sure it's valid none the less.
                match event.token() 
                {
                    TIMER_EVENT =>
                    {
                        eprintln!("timeout!");
                        return Ok(());
                    },
                    UDP_SOCKET => loop 
                    {
                        // In this loop we receive all packets queued for the socket.
                        match socket.recv_from(&mut buf) {
                            Ok((packet_size, source_address)) => {
                                // Echo the data.
                                socket.send_to(&buf[..packet_size], source_address)?;
                            }
                            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                                // If we get a `WouldBlock` error we know our socket
                                // has no more packets queued, so we can return to
                                // polling and wait for some more.
                                break;
                            }
                            Err(e) => {
                                // If it was any other kind of error, something went
                                // wrong and we terminate with an error.
                                return Err(e);
                            }
                        }
                    },
                    _ => {
                        // This should never happen as we only registered our
                        // `UdpSocket` using the `UDP_SOCKET` token, but if it ever
                        // does we'll log it.
                        warn!("Got event for unexpected token: {event:?}");
                    }
                }
            }
        }
    }

    
}

#[cfg(target_os = "windows")]
fn main() -> io::Result<()>  
{
    return self::os_spec::main1();
}

#[cfg(not(target_os = "windows"))]
fn main() -> io::Result<()>  
{
    panic!("can't monitor event not on windows")
}