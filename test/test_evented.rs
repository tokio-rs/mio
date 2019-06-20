#[test]
fn assert_evented_implemented_for() {
    use std::rc::Rc;
    use std::sync::Arc;

    use crate::net::TcpStream;

    fn assert_evented<E: Evented>() {}

    assert_evented::<Box<dyn Evented>>();
    assert_evented::<Box<TcpStream>>();
    assert_evented::<Arc<dyn Evented>>();
    assert_evented::<Arc<TcpStream>>();
    assert_evented::<Rc<dyn Evented>>();
    assert_evented::<Rc<TcpStream>>();
}
