use mio::*;
use super::localhost;
use std::cell::Cell;
use std::mem;
use std::rc::Rc;

struct EchoServer {
    num_msgs: uint
}

impl EchoServer {
    fn new(num_msgs: uint) -> EchoServer {
        EchoServer {
            num_msgs: num_msgs
        }
    }
}

impl PerClient<()> for EchoServer {
    fn on_read(&mut self, _reactor: &mut Reactor, c: &mut ConnectionState<()>, buf: RWIobuf<'static>) -> MioResult<()> {
        assert!(self.num_msgs != 0);

        debug!("EchoServer::on_read");
        debug!("Recv'd on server (and echoing): {}", buf);
        c.send(buf);

        self.num_msgs -= 1;
        if self.num_msgs == 0 {
            debug!("a server just died!");
            Err(MioError::eof())
        } else {
            Ok(())
        }
    }
}

struct EchoClient {
    msgs: Vec<&'static str>,
    rx:   RWIobuf<'static>,

    addr:           Rc<SockAddr>,
    left_to_start:  Rc<Cell<uint>>,
    left_to_finish: Rc<Cell<uint>>,
}

impl EchoClient {
    fn new(msgs: Vec<&'static str>, addr: Rc<SockAddr>, left_to_start: Rc<Cell<uint>>, left_to_finish: Rc<Cell<uint>>) -> EchoClient {
        let curr = msgs[0];

        let curr = ROIobuf::from_str(curr);

        EchoClient {
            msgs: msgs,
            rx: curr.deep_clone(),
            addr:           addr,
            left_to_start:  left_to_start,
            left_to_finish: left_to_finish,
        }
    }

    fn next_msg(&mut self, _reactor: &mut Reactor, c: &mut ConnectionState<()>) -> MioResult<RWIobuf<'static>> {
        debug!("EchoClient::next_msg");
        let curr =
            match self.msgs.remove(0) {
                Some(msg) => msg,
                // All done!
                None => {
                    return Err(MioError::eof());
                },
            };

        let old_rx = mem::replace(&mut self.rx, c.make_iobuf(curr.len()));
        c.return_iobuf(old_rx);
        self.rx.fill(curr.as_slice().as_bytes()).unwrap();
        self.rx.flip_lo();

        let mut tx = c.make_iobuf(curr.len());
        tx.fill(curr.as_slice().as_bytes()).unwrap();
        tx.flip_lo();
        Ok(tx)
    }
}

impl PerClient<()> for EchoClient {
    fn on_start(&mut self, reactor: &mut Reactor, c: &mut ConnectionState<()>) -> MioResult<()> {
        debug!("EchoClient::on_start");

        let left_to_start = self.left_to_start.get() - 1;
        self.left_to_start.set(left_to_start);

        // spawn the next client!
        if left_to_start != 0 {
            gen_tcp_client(
                reactor,
                &*self.addr,
                |_| {},
                EchoClient::new(
                    self.msgs.clone(),
                    self.addr.clone(),
                    self.left_to_start.clone(),
                    self.left_to_finish.clone())).unwrap();
        }

        let next_msg = try!(self.next_msg(reactor, c));
        debug!("Sending from the client: {}", next_msg);
        c.send(next_msg);
        Ok(())
    }

    fn on_read(&mut self, reactor: &mut Reactor, c: &mut ConnectionState<()>, mut buf: RWIobuf<'static>) -> MioResult<()> {
        debug!("EchoClient::on_read");
        debug!("Recv'd on client: {}", buf);
        while !buf.is_empty() {
            let actual: u8 = buf.consume_be().unwrap();
            let expect: u8 = self.rx.consume_be().unwrap();

            assert_eq!(actual, expect);
        }

        c.return_iobuf(buf);

        if self.rx.is_empty() {
            let msg = try!(self.next_msg(reactor, c));
            println!("sending from the client: {}", msg);
            c.send(msg);
        }

        Ok(())
    }

    fn on_close(&mut self, reactor: &mut Reactor, _c: &mut ConnectionState<()>) -> MioResult<()> {
        let left_to_finish = self.left_to_finish.get() - 1;
        self.left_to_finish.set(left_to_finish);
        if left_to_finish == 0 {
            reactor.shutdown();
        }
        Ok(())
    }
}

static test_vec: [&'static str, ..8] =
    [ "foo", "bar", "hello", "world", "what", "a", "nice", "day" ];

static NUM_CLIENTS: uint = 1;

#[test]
pub fn test_echo_server() {
    let test_vector = Vec::from_slice(test_vec.as_slice());

    let mut reactor = Reactor::new().unwrap();

    let addr = SockAddr::parse(localhost().as_slice())
        .expect("could not parse InetAddr");

    fn new_echo_server(_reactor: &mut Reactor) -> EchoServer {
        EchoServer::new(test_vec.len())
    }

    gen_tcp_server(
        &mut reactor,
        &addr,
        |s| s.set_reuseaddr(true).unwrap(),
        256u,
        (),
        new_echo_server).unwrap();

    let left_to_start  = Rc::new(Cell::new(NUM_CLIENTS));
    let left_to_finish = Rc::new(Cell::new(NUM_CLIENTS));

    let addr = Rc::new(addr);

    gen_tcp_client(
        &mut reactor,
        &*addr,
        |_| {},
        EchoClient::new(
            test_vector.clone(),
            addr.clone(),
            left_to_start.clone(),
            left_to_finish.clone())).unwrap();

    // Start the reactor
    reactor.run().ok().expect("failed to execute reactor");

    assert_eq!(left_to_start.get(), 0u);
    assert_eq!(left_to_finish.get(), 0u);
}
