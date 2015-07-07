use {EventLoop, Interest, Token};

#[allow(unused_variables)]
pub trait Handler {
    type Timeout;
    type Message: Send;

    fn ready(&mut self, event_loop: &mut EventLoop<Self>, token: Token, events: Interest) {
    }

    fn notify(&mut self, event_loop: &mut EventLoop<Self>, msg: Self::Message) {
    }

    fn timeout(&mut self, event_loop: &mut EventLoop<Self>, timeout: Self::Timeout) {
    }

    fn interrupted(&mut self, event_loop: &mut EventLoop<Self>) {
    }
}
