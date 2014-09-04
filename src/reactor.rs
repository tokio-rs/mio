use error::MioResult;
use handler::{Handler, Token};
use io::*;
use event::*;
use os;

/// A lightweight IO reactor.
///
/// An internal lookup structure is used to associate tokens with io
/// descriptors as well as track whether a socket is a listener or not.

#[deriving(Clone, Show)]
pub struct ReactorConfig;

pub struct Reactor<T> {
    selector: os::Selector,
}

impl<T: Token> Reactor<T> {
    /// Initializes a new reactor. The reactor will not be running yet.
    pub fn new() -> MioResult<Reactor<T>> {
        Ok(Reactor {
            selector: try!(os::Selector::new()),
            run: true
        })
    }

    /// Registers an IO descriptor with the reactor.
    pub fn register<S: Socket>(&mut self, io: S, token: T, events: IoEventKind) -> MioResult<()> {
        debug!("registering IO with reactor");

        // Register interets for this socket
        try!(self.selector.register(io.desc(), token.to_u64()));

        Ok(())
    }


    pub fn run(&mut self, fn (token: T, event: IoEventKind) -> bool) {
        self.run = true;

        // Created here for stack allocation
        let mut events = [EpollEvent ..1024];

        while self.io_poll(&mut events, &mut handler) {
            debug!("reactor tick");
        }
    }

    fn io_poll(&mut self, events: &mut [EpollEvent], handler: fn (token: T, event: IoEventKind) -> bool) -> bool {

        let len = self.selector.select(events, 1000).unwrap();

        while i < len && i < events.len() {
            let evt = events[i].from_mask();
            let tok = Token::from_u64(evt.token);

            debug!("event={}", evt);

            if ( ! handler(tok, evt)) {
               return false;
            }

            i += 1;
        }

        true
    }
}

