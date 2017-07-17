use mio::*;
use mio::fuchsia::EventedHandle;
use magenta;
use std::time::Duration;

const MS: u64 = 1_000;

#[test]
pub fn test_fuchsia_channel() {
    let poll = Poll::new().unwrap();
    let mut event_buffer = Events::with_capacity(1);
    let event_buffer = &mut event_buffer;

    let (channel0, channel1) = magenta::Channel::create(magenta::ChannelOpts::Normal).unwrap();
    let channel1_evented = EventedHandle::new(channel1);

    poll.register(&channel1_evented, Token(1), Ready::readable(), PollOpt::edge()).unwrap();

    poll.poll(event_buffer, Some(Duration::from_millis(MS))).unwrap();
    assert_eq!(event_buffer.len(), 0);

    channel0.write(&[1, 2, 3], &mut vec![], 0).unwrap();

    poll.poll(event_buffer, Some(Duration::from_millis(MS))).unwrap();
    let event = event_buffer.get(0).unwrap();
    assert_eq!(event.token(), Token(1));
    assert!(event.readiness().is_readable());

    poll.deregister(&channel1_evented).unwrap();
}