use mio::*;
use super::localhost;
use std::mem;

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
            Err(MioError::eof())
        } else {
            Ok(())
        }
    }
}

struct EchoClient {
    msgs: Vec<&'static str>,
    rx:   RWIobuf<'static>,
}

impl EchoClient {
    fn new(msgs: Vec<&'static str>) -> EchoClient {
        let curr = msgs[0];

        let curr = ROIobuf::from_str(curr);

        EchoClient {
            msgs: msgs,
            rx: curr.deep_clone(),
        }
    }

    fn next_msg(&mut self, reactor: &mut Reactor, c: &mut ConnectionState<()>) -> MioResult<RWIobuf<'static>> {
        debug!("EchoClient::next_msg");
        let curr =
            match self.msgs.remove(0) {
                Some(msg) => msg,
                // All done!
                None => {
                    reactor.shutdown();
                    return Err(MioError::eof());
                },
            };

        let old_rx = mem::replace(&mut self.rx, c.make_iobuf(curr.len()));
        c.return_iobuf(old_rx);
        self.rx.fill(curr.as_slice().as_bytes()).unwrap();
        self.rx.flip_lo();

        debug!("rx = {}", self.rx);

        let mut tx = c.make_iobuf(curr.len());
        tx.fill(curr.as_slice().as_bytes()).unwrap();
        tx.flip_lo();
        debug!("tx = {}", tx);
        Ok(tx)
    }
}

impl PerClient<()> for EchoClient {
    fn on_start(&mut self, reactor: &mut Reactor, c: &mut ConnectionState<()>) -> MioResult<()> {
        debug!("EchoClient::on_start");
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
}

static test_vec: [&'static str, ..2] = [ "foo", "bar" ];

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

    gen_tcp_client(
        &mut reactor,
        &addr,
        |_| {},
        EchoClient::new(test_vector)).unwrap();

    // Start the reactor
    reactor.run().ok().expect("failed to execute reactor");
}
