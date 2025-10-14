use mio::{Events, event::Event};

// test that Event implements Send and Sync across all platforms
// this ensures that whenever we add a new platform, we verify Send/Sync
#[test]
fn is_event_send_sync() {
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}
    
    assert_send::<Event>();
    assert_sync::<Event>();
    
    assert_send::<Events>();
    assert_sync::<Events>();
}