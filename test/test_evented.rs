use std::rc::Rc;
use std::sync::Arc;

use mio::event::Evented;
use mio::net::TcpStream;

#[test]
fn assert_evented_implemented_for() {
    fn assert_evented<E: Evented>() {}

    assert_evented::<Box<dyn Evented>>();
    assert_evented::<Box<TcpStream>>();
    assert_evented::<Arc<dyn Evented>>();
    assert_evented::<Arc<TcpStream>>();
    assert_evented::<Rc<dyn Evented>>();
    assert_evented::<Rc<TcpStream>>();
}
