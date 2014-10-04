use event_loop::EventLoop;
use token::Token;

bitflags!(
    #[deriving(Show)]
    flags ReadHint: uint {
        static DataHint    = 0x001,
        static HupHint     = 0x002,
        static ErrorHint   = 0x004
    }
)

#[allow(unused_variable)]
pub trait Handler<T, M: Send> {
    fn readable(&mut self, event_loop: &mut EventLoop<T, M>, token: Token, hint: ReadHint) {
    }

    fn writable(&mut self, event_loop: &mut EventLoop<T, M>, token: Token) {
    }

    fn notify(&mut self, event_loop: &mut EventLoop<T, M>, msg: M) {
    }

    fn timeout(&mut self, event_loop: &mut EventLoop<T, M>, timeout: T) {
    }
}
