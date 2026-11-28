use super::*;
use crate::{Events, Poll};
use std::fs::OpenOptions;
use std::time::Duration;

enum ReadRequest {
    ConnectBeforeClient,
    ConnectAfterClient,
    Reregister,
}

#[test]
fn listening_read_error_before_connect() {
    listening_read_error(false, ReadRequest::ConnectBeforeClient);
    listening_read_error(false, ReadRequest::ConnectAfterClient);
}

#[test]
fn listening_read_error_after_connect() {
    listening_read_error(true, ReadRequest::ConnectBeforeClient);
    listening_read_error(true, ReadRequest::ConnectAfterClient);
}

#[test]
fn listening_read_error_after_reregister() {
    listening_read_error(true, ReadRequest::Reregister);
}

fn listening_read_error(request_first: bool, request: ReadRequest) {
    let name = format!(r"\\.\pipe\mio-listening-{}", rand::random::<u64>());
    // Keep the test-owned Op alive until its completion is modeled or
    // the pipe is dropped.
    let mut pending = Some(Op::new());
    let mut pipe = NamedPipe::new(&name).unwrap();
    let mut poll = Poll::new().unwrap();
    let token = Token(0);
    let interests = Interest::READABLE | Interest::WRITABLE;

    // Model just the pending read, since Windows can instead return
    // ERROR_PIPE_LISTENING inline.
    let initial_read = || ReadState::InFlight {
        id: pending.as_ref().unwrap().id(),
        read_requested: false,
    };
    pipe.inner.io.lock().unwrap().read = initial_read();
    poll.registry()
        .register(&mut pipe, token, interests)
        .unwrap();
    // The modeled read stands for the one requested by this registration.
    pipe.inner.io.lock().unwrap().read = initial_read();
    let mut events = Events::with_capacity(8);
    let timeout = Some(Duration::from_secs(5));
    poll.poll(&mut events, Some(Duration::ZERO)).unwrap();

    let finish_read = |pipe: &NamedPipe, op| {
        let mut events = Vec::new();
        let err = io::Error::from_raw_os_error(ERROR_PIPE_LISTENING as i32);
        Inner::read_error(
            &pipe.inner,
            &mut pipe.inner.io.lock().unwrap(),
            op,
            err,
            Some(&mut events),
        );
        assert!(events.is_empty());
    };
    if !request_first {
        finish_read(&pipe, pending.take().unwrap());
        let idle = matches!(pipe.inner.io.lock().unwrap().read, ReadState::Idle);
        assert!(idle);
    }

    let open_client = || {
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(&name)
            .unwrap()
    };
    let client = match request {
        ReadRequest::ConnectBeforeClient => {
            assert_eq!(
                pipe.connect().unwrap_err().kind(),
                io::ErrorKind::WouldBlock
            );
            open_client()
        }
        ReadRequest::ConnectAfterClient => {
            let client = open_client();
            pipe.connect().unwrap();
            client
        }
        ReadRequest::Reregister => {
            let client = open_client();
            poll.registry()
                .reregister(&mut pipe, token, interests)
                .unwrap();
            client
        }
    };

    poll.poll(&mut events, timeout).unwrap();
    let (read_deferred, connecting) = {
        let io = pipe.inner.io.lock().unwrap();
        let deferred = matches!(
            io.read,
            ReadState::InFlight {
                read_requested: true,
                ..
            }
        );
        (deferred, io.connect.is_some())
    };
    assert!(!connecting);
    assert_eq!(read_deferred, request_first);

    if let Some(pending) = pending.take() {
        finish_read(&pipe, pending);
    }

    let read_started = matches!(
        pipe.inner.io.lock().unwrap().read,
        ReadState::InFlight {
            read_requested: false,
            ..
        }
    );
    assert!(read_started);

    poll.registry().deregister(&mut pipe).unwrap();
    drop(client);
    poll.poll(&mut events, Some(Duration::from_millis(100)))
        .unwrap();
}
