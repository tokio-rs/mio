use {EventLoop, ReadHint, Token};

#[allow(unused_variables)]
pub trait Handler {
    type Timeout;
    type Message: Send;

    fn readable(&mut self, event_loop: &mut EventLoop<Self>, token: Token, hint: ReadHint) {
    }

    fn writable(&mut self, event_loop: &mut EventLoop<Self>, token: Token) {
    }

    fn notify(&mut self, event_loop: &mut EventLoop<Self>, msg: Self::Message) {
    }

    fn timeout(&mut self, event_loop: &mut EventLoop<Self>, timeout: Self::Timeout) {
    }

    fn interrupted(&mut self, event_loop: &mut EventLoop<Self>) {
    }
}
