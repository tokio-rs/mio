use std::fmt;
use event_loop::EventLoop;
use token::Token;

bitflags!(
    flags ReadHint: uint {
        static DataHint    = 0x001,
        static HupHint     = 0x002,
        static ErrorHint   = 0x004
    }
)

impl fmt::Show for ReadHint {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let mut one = false;
        let flags = [
            (DataHint, "DataHint"),
            (HupHint, "HupHint"),
            (ErrorHint, "ErrorHint")];

        for &(flag, msg) in flags.iter() {
            if self.contains(flag) {
                if one { try!(write!(fmt, " | ")) }
                try!(write!(fmt, "{}", msg));

                one = true
            }
        }

        Ok(())
    }
}

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
