use event_loop::EventLoop;
use token::Token;

#[allow(unused_variable)]
pub trait Handler<T, M: Send> {
    fn readable(&mut self, event_loop: &mut EventLoop<T, M>, token: Token) {
    }

    fn writable(&mut self, event_loop: &mut EventLoop<T, M>, token: Token) {
    }

    fn notify(&mut self, event_loop: &mut EventLoop<T, M>, msg: M) {
    }

    fn timeout(&mut self, event_loop: &mut EventLoop<T, M>, timeout: T) {
    }
}
