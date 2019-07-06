use std::rc::Rc;
use std::sync::Arc;

use mio::event;
use mio::net::TcpStream;

#[test]
fn assert_evented_implemented_for() {
    fn assert_evented<E: event::Source>() {}

    assert_evented::<Box<dyn event::Source>>();
    assert_evented::<Box<TcpStream>>();
    assert_evented::<Arc<dyn event::Source>>();
    assert_evented::<Arc<TcpStream>>();
    assert_evented::<Rc<dyn event::Source>>();
    assert_evented::<Rc<TcpStream>>();
}
