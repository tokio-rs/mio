use std::rc::Rc;
use std::sync::Arc;

use mio::event;
use mio::net::TcpStream;

#[test]
fn assert_event_source_implemented_for() {
    fn assert_event_source<E: event::Source>() {}

    assert_event_source::<Box<dyn event::Source>>();
    assert_event_source::<Box<TcpStream>>();
    assert_event_source::<Arc<dyn event::Source>>();
    assert_event_source::<Arc<TcpStream>>();
    assert_event_source::<Rc<dyn event::Source>>();
    assert_event_source::<Rc<TcpStream>>();
}
