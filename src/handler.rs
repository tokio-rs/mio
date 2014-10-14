use std::fmt;
use event_loop::EventLoop;
use token::Token;

bitflags!(
    flags ReadHint: uint {
        const DATAHINT    = 0x001,
        const HUPHINT     = 0x002,
        const ERRORHINT   = 0x004
    }
)

impl fmt::Show for ReadHint {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let mut one = false;
        let flags = [
            (DATAHINT, "DataHint"),
            (HUPHINT, "HupHint"),
            (ERRORHINT, "ErrorHint")];

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
