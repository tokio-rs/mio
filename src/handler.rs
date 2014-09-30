use reactor::Reactor;
use token::Token;

#[allow(unused_variable)]
pub trait Handler<T, M: Send> {
    fn readable(&mut self, reactor: &mut Reactor<T, M>, token: Token) {
    }

    fn writable(&mut self, reactor: &mut Reactor<T, M>, token: Token) {
    }

    fn notify(&mut self, reactor: &mut Reactor<T, M>, msg: M) {
    }

    fn timeout(&mut self, reactor: &mut Reactor<T, M>, timeout: T) {
    }
}
