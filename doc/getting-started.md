# Getting Started

This section will serve as an introductory Mio tutorial. It assumes
you have familiarity with the [Rust](http://www.rust-lang.org/)
programming language and the [Cargo](https://crates.io) tool. It will
start from generating a new [Rust](http://www.rust-lang.org/) project
using [Cargo](https://crates.io) and guide you through writing a simple TCP echo server
and client.

Of course, Rust needs to be installed. If you haven't already installed it, get it
here: [rust-lang.org](https://www.rust-lang.org).

The complete echo server can be found
[here](../examples/ping_pong/src/server.rs). An example of an echo client can be found
[here](../examples/ping_pong/src/client.rs).

Before you get started, set up an empty text file. As you go through this guide
capture any thoughts, confusions, or questions that come to mind.
You only have one first impression and I would like to capture
your first impression in order to improve this document.

Post your notes as a comment [here](https://github.com/carllerche/mio/pull/222).

## Setting Up the Project

The first step is to set up a new Rust project using Cargo. In a new
directory, run the following:

```not_rust
cargo new pingpong --bin
cd pingpong
```

Now, open the directory in your favorite text editor. You should see the
following files:

* src/main.rs
* Cargo.toml

If you are not already familiar with Cargo, you can learn more about it
[here](http://doc.crates.io/).

Open `Cargo.toml` and add a dependency on Mio by putting the following a
the bottom of the file:

```toml
[dependencies]
mio = "0.5.0"
bytes = "0.3.0"
```
> Note:
> If you're attempting to follow this tutorial using the master branch you will
> also need to add `slab = "0.1.0"` to the list of dependencies.

Save the file, then compile and run the project using the following
command:

```not_rust
cargo run
```

You will see some Cargo related output followed by `Hello, world!`. We
haven't written any code yet and this is the default behavior of a
freshly generated Cargo project.

## The Echo Server

Let's start by writing a very simple server that accepts connections and
does nothing with them. The connections will be accepted and shutdown
immediately after.

Here is the entire code, we'll step through it in a bit.

```rust
extern crate mio;

use mio::tcp::*;
use mio::{Token, EventSet, PollOpt};

const SERVER: mio::Token = mio::Token(0);

struct Pong {
    server: TcpListener,
}

impl mio::Handler for Pong {
    type Timeout = ();
    type Message = ();

    fn ready(&mut self, event_loop: &mut mio::EventLoop<Pong>, token: mio::Token, events: mio::EventSet) {
        match token {
            SERVER => {
                // Only receive readable events
                assert!(events.is_readable());

                println!("the server socket is ready to accept a connection");
                match self.server.accept() {
                    Ok(Some(socket)) => {
                        println!("accepted a socket, exiting program");
                        event_loop.shutdown();
                    }
                    Ok(None) => {
                        println!("the server socket wasn't actually ready");
                    }
                    Err(e) => {
                        println!("listener.accept() errored: {}", e);
                        event_loop.shutdown();
                    }
                }
            }
            _ => panic!("Received unknown token"),
        }
    }
}

fn main() {
    let address = "0.0.0.0:6567".parse().unwrap();
    let server = TcpListener::bind(&address).unwrap();

    let mut event_loop = mio::EventLoop::new().unwrap();
    event_loop.register(&server, SERVER, EventSet::readable(), PollOpt::level());

    println!("running pingpong server");
    event_loop.run(&mut Pong { server: server });
}
```

Let's break it down. The first step (at the beginning of the `main`
function), is to create a TCP listener. This will create the socket,
bind to the specified address, and start listening for inbound
connections.

The next step is to create the event loop and register the socket with it.

### The [Event Loop](http://rustdoc.s3-website-us-east-1.amazonaws.com/mio/v0.5.x/mio/struct.EventLoop.html)

The Mio event loop is able to monitor many sockets and notify the
application when the state of a socket changes. The application
registers sockets with the event loop. This is done by supplying a
[Token](http://rustdoc.s3-website-us-east-1.amazonaws.com/mio/v0.5.x/mio/struct.Token.html)
along with the socket, associating the two.  When the event loop notifies the application
about changes in a socket's state it is the `Token` the event loop will use to
identify which socket has changed to the application.  The event loop passes the
identifying `Token` to the custom event handler specified when the event loop is
started, `event_loop.run(&mut Pong { server: server });`.  

***When the state of a socket changes, the event loop will call the appropriate
event function on the custom handler passing the function the `Token`
associated with the socket which has changed.***

In our case, the event handler is the `Pong` struct as it implements the
[Handler](http://rustdoc.s3-website-us-east-1.amazonaws.com/mio/v0.5.x/mio/trait.Handler.html)
trait. We only define the
[ready](http://rustdoc.s3-website-us-east-1.amazonaws.com/mio/v0.5.x/mio/trait.Handler.html#method.ready)
function, but the `mio::Handler` trait has other functions that can be defined
to handle other types of events.

In our `main` function, we create the `EventLoop` binding and start the event loop
by calling `event_loop.run(..)`.  We pass `run(..)` a mutable reference to our
handler. The `run(..)` function will block the application from exiting until
the event loop is shut down.

The event loop isn't much good if it has no input upon which to act.

Before the event loop is started we must provide the event loop a link to the
outside world.  In our case, we register the pingpong server socket with the
event loop
`event_loop.register(&server, SERVER, EventSet::readable(), PollOpt::level())`
and associate the socket with a token named `SERVER`, a constant binding in our
application.  Whenever a connection is ready to be accepted the event loop
will call our custom handler's `ready` function passing it the `SERVER` token.
This is how the handler identifies on which sockets it should operate.

There are two additional pieces of configuration conveyed with the registration
of the server socket; the event set and the polling option.

#### Polling Options: Level vs. Edge

Mio (like Epoll, Kqueue, etc..) supports both level-triggered and
edge-triggered notifications.

A level-triggered socket having pending data will result in a
call to a handler's `ready()` function each time the event loop iterates through
its registered sockets even if the socket is buffering the exact same data
during consecutive iterations. In other words, a handler's `ready()` function
will be called until the data has been read from the socket.  
The same is true for writable events. As long as a socket can accept data
written to it, the handler's `ready()` function will be called.

An edge-triggered socket, on the other hand, will only call a handler's
`ready()` function once per state change. That is, when a socket receives new
data, `ready()` will be called. But it will only be called once regardless of
whether or not the data is read from the socket.

Whether a socket is ready to be read or ready to be written is conveyed to a
handler via an EventSet.

### Handling Events

As discussed previously, the event loop notifies a handler that a socket is ready
to be operated on.  The handler needs to do something. The handler may read
from, write to, or close a socket.

The event loop notifies the handler via a call to one of its handler
functions.  In the case of our pingpong server the `ready` handler function
is called.

The following is the `ready` function's signature.

```rust
fn ready(&mut self, event_loop: &mut EventLoop<Self>, token: Token, events: EventSet) { ... }
```

The socket which caused the notification is identified by the token passed to the
handler function.  So far we've only created a single socket: the
`server` socket.  So we simply ensure that the only token passed to our handler
is the `server` socket by matching against the `SERVER` `Token` which was
associated with the `server` socket by the register call.  If we receive
another token we panic because that would be very, very weird since we haven't
registered any other tokens.  When there are many sockets, handlers get more
involved.  We will cover handling more than one sockets later in the guide.

The event loop will also pass a reference to itself allowing us to register
additional sockets.  This becomes important when we're accepting multiple
connections.

The last argument, the [`events:
EventSet`](http://rustdoc.s3-website-us-east-1.amazonaws.com/mio/master/mio/struct.EventSet.html)
argument, may provide a hint with regard to what will happen when an attempt to
read from the socket is performed.  For example, if the socket experienced an
error, a read from the socket will fail. The `EventSet` argument will be set
to `EventSet::error()`.  But it's important to understand the `events` argument
provides no guarantees.  Even if `events` is set to `EventSet::readable()`,
a read from the socket may fail.

With that information let's study the body of our implementation of the ready
function.

```rust
fn ready(&mut self, event_loop: &mut mio::EventLoop<Pong>, token: mio::Token, events: mio::EventSet) {
    match token {
        SERVER => {
            assert!(events.is_readable());

            println!("the server socket is ready to accept a connection");
            match self.server.accept() {
                Ok(Some(socket)) => {
                    println!("accepted a socket, exiting program");
                    event_loop.shutdown();
                }
                Ok(None) => {
                    println!("the server socket wasn't actually ready");
                }
                Err(e) => {
                    println!("listener.accept() errored: {}", e);
                    event_loop.shutdown();
                }
            }
        }
        _ => panic!("Received unknown token"),
    }
}
```

Once our `ready` handler has determined the identity of the socket the handler
ensures `events` is set to `EventSet::readable()`.  We're only interested in
data coming to our server at the moment.

With that out of the way we accept the incoming connection.

The signature for the [`TcpListener::accept()`]
(http://rustdoc.s3-website-us-east-1.amazonaws.com/mio/v0.5.x/mio/tcp/struct.TcpListener.html#method.accept)
function follows.

```no_run
fn accept(&self) -> Result<Option<(TcpStream, SocketAddr)>>
```

All non-blocking socket types follow a similar pattern of returning
`io::Result<Option<(T, SocketAddr)>>`.

An operation on a non-blocking socket can flat out fail and return an error
while the socket is still in a good state.  It may be, however, that the socket
is not be ready to be operated on.  If it were a blocking socket, the operation
would block.  Instead, the non-blocking socket returns immediately with Ok(None)
when the socket isn't ready to be operated on.  When the socket is in that state
we must wait for the event loop to notify the handler that the socket is
readable again.

> **Important:** Even if we just received a ready notification, there is
> no guarantee a read from the socket will succeed and not return
> `Ok(None)` so we must handle that case as well.

If a connection was successfully accepted the handler prints a short message and
shuts down the event loop.  At that point the `event_loop.run(...)` call will
return and the program will exit.

So far our server doesn't really do much.  Let's change that by managing the
connections made to our pingpong server.

### Handling Client Connections

Our echo server will be pretty simple when complete. It will receive data from
a socket, the connection, buffer the data until a newline is received, and send
the data back to the client (hence the echo).

We need a way to store the connection state on the `Pong` server.
To do this, we create a `Connection` struct that will
store the state of a single client connection.  The `Pong` struct will
use a [`Slab`](http://rustdoc.s3-website-us-east-1.amazonaws.com/slab/master/slab/struct.Slab.html)
to store all of the `Connection` instances.  We'll come back to the `Slab`.

The `Connection` struct follows.

```rust
struct Connection {
    socket: TcpStream,
    token: mio::Token,
    state: State,
}
```

The `Connection` struct provides a way to store the association between a
`Token` and socket in a convenient package.  This makes it easy to access when
we want to read from the socket or write to the socket.  Including the `Token`
also makes it convenient reregister the socket via the `EventLoop::reregister`
method.

The last field represents the state of a `Connection` at any point while the
`Pong` server is running.  We represent the states as an enum.

The echo server's goal is to read from a client connection until a new
line has been received, then write the line back to the client (and
repeat). That implies two clear states.  While in the first state the connection
is reading from a socket, looking for a new line.  If a new line is never
received, the `Connection` stays in the reading state.
If a new line is found, the `Connection` transitions to a writing state
until the line has been written back to the client.  Once the write is complete
the `Connection` transitions back to the reading state.

We'll use and enum to represent the states of a `Connection` and the data
required by the `Connection` in a particular state.

```rust
enum State {
    Reading(Vec<u8>),
    Writing(Take<Cursor<Vec<u8>>>),
    Closed,
}
```

Of course, a `Connection` could be closed so we also include that as a valid
`State`.

Now that we have a convenient representation of connections our `Pong` server
can store all of the connections it is handling.  

A `Slab` is a fixed capacity map of `Token` to `Connection` similar to a HashMap
but lighter weight and optimized for use with mio.

Our `Pong` struct looks like this now.

```rust
struct Pong {
    server: TcpListener,
    connections: Slab<Connection, Token>
}
```

We have a pretty good server now.  We understand how a Mio based application
should be structured at the lowest layers.

There are a few implementation details we need to discus before we finish
writing our server.  The final sections will bring some of the architectural
decisions to light and help you understand how to use Mio in a finer grained
detail.

#### Tokens

Mio's decision to use tokens that identify sockets on which events take place
may seem surprising.  We could have used some form of callback strategy as a
lot of async frameworks do. The reason Mio chose a token based strategy
is that it allows Mio applications to operate at runtime without performing
allocations. Using a callback strategy for handling event notifications would
violate this requirement.

A `Token` is simply a wrapper around `usize`. In our example we have
`SERVER` hardcoded to `Token(0)`. We need to ensure that a client
socket does not get registered with the same token otherwise there will be no
way to differentiate between sockets.  The `Slab` we are using to store the
`Connection` instances will be responsible for generating the value of the `Token`
associated with each `Connection` instance.

We're going to add a factory function to our `Pong` server that configures the
`Slab` so that it can't reuse `Token(0)`.

```rust
impl Pong {
    fn new(server: TcpListener) -> Pong {
        let slab = Slab::new_starting_at(mio::Token(1), 1024);

        Pong {
            server: server,
            connections: slab,
        }
    }
}
```

Now when a new client socket comes knocking and we accept,
we initialize a new `Connection`instance and insert it into the `Slab`.
We take the token that the `Slab` associates with the socket and use it to
register the client socket with the EventLoop. Once registered, our handler will
receive notifications about events on that socket.

```rust
match self.server.accept() {
    Ok(Some((socket, _))) => {
        let token = self.connections
            .insert_with(|token| Connection::new(socket, token))
            .unwrap();

        event_loop.register(
            &self.connections[token].socket,
            token,
            mio::EventSet::readable(),
            mio::PollOpt::edge() | mio::PollOpt::oneshot()).unwrap();
    }
    ...
}
```

There's one detail in this change that should be pointed out.

In this case, we register new sockets with
edge + oneshot `mio::PollOpt::edge() | mio::PollOpt::oneshot()).unwrap();`.
This means that when a ready notification is fired the socket will be
"unarmed". In other words, once a notification for the socket is received,
no further notifications will be fired. When we want to receive another notification,
we [`reregister`](http://rustdoc.s3-website-us-east-1.amazonaws.com/mio/master/mio/struct.EventLoop.html#method.reregister)
the socket with the event loop.

Using this pattern of edge + oneshot makes state management a bit
easier. We only receive notifications when we are ready to handle them.
The alternative is to simply use edge triggered notifications without
oneshot, in which case we would have to handle receiving a notification
while not being ready to handle it. For example, if we
receive a writable notification for a socket, but we don't have
anything to write yet.

#### Handling Client Events

Our `Pong` server can now accept and track client connections. Each time an
event is generated by a client socket the `ready` function of our handler
will be called. Currently, the `ready` function only handles the server
socket. To start handling client socket events, we need to update the
function as follows.

```rust
fn ready(&mut self, event_loop: &mut mio::EventLoop<Pong>, token: mio::Token, events: mio::EventSet) {
    match token {
        SERVER => {
            // ... handle incoming connections
        }
        _ => {
            self.connections[token].ready(event_loop, events);

            if self.connections[token].is_closed() {
                let _ = self.connections.remove(token);
            }
        }
    }
}
```

We assume any token that is not the server token is a client token.
We look up the connection in the `Slab` and then forward the
notification to the specific client connection struct via its `ready` function,
another notification handler. Once the handler completes its work, we check if
the handler closed the connection and, if so, remove it from the slab so it will
no longer be tracked.

### Buffers

Mio uses buffers extensively.  A **buffer** is an abstraction representing byte
storage and a cursor that tracks the current position in that byte storage.  
The byte storage may or may not be contiguous memory. Buffers can be readable,
represented by the [`Buf`](http://carllerche.github.io/bytes/bytes/buf/trait.Buf.html)
trait, and they can be writable, represented by the
[`MutBuf`](http://carllerche.github.io/bytes/bytes/buf/trait.MutBuf.html) trait.

For example, `Vec<u8>` implements `MutBuf` such that, when new bytes are
written to `Vec<u8>` they are appended to the end of the vector. This
allows us to use `Vec<u8>` as an argument when reading from an Mio socket.

```rust
let mut buf = vec![];
try!(sock.try_read_buf(&mut buf));
```

Similarly, to write to a socket we use `std::io::Cursor<Vec<u8>>` as follows.

```rust
let mut buf = Cursor::new(vec);
try!(sock.try_write_buf(&mut buf));
```

The reason it's advisable to use a `Buf` instead of just calling
`try_write(...)` with a slice is that buffers maintain a cursor that tracks the
current position in the buffer, the next accessible byte. That means calling
`try_write_buf` multiple times using the same buffer isn't a problem because
each time `try_write_buf` is called, writing starts where the previous write
finished.

Having a buffer that maintains the cursor and keep track of its current position
is especially useful when working with non blocking sockets since, at any
point, the socket may not be ready to be operated on the operation will
have to be attempted at a later time. For example:

```rust
let mut buf = Cursor::new(vec);

while buf.has_remaining() {
  try!(sock.try_read_buf(&mut buf));
}
```

Also, because the memory backing the buffer is abstracted, it's possible to
have different kinds of buffers suitable for different use cases.
The [bytes](https://github.com/carllerche/bytes) crate contains basic byte buffers,
ring buffer, ropes, etc...

## Reading and Writing Data

Now that we've covered reading and writing to sockets we can finish our server.

We modify the `Connection::ready` function so that it now looks like the
following.

```rust
fn ready(&mut self, event_loop: &mut mio::EventLoop<Pong>, events: mio::EventSet) {
    println!("    connection-state={:?}", self.state);

    match self.state {
        State::Reading(..) => {
            assert!(events.is_readable(), "unexpected events; events={:?}", events);
            self.read(event_loop)
        }
        State::Writing(..) => {
            assert!(events.is_writable(), "unexpected events; events={:?}", events);
            self.write(event_loop)
        }
        _ => unimplemented!(),
    }
}
```

And the read function which is called from the ready function `self.read(event_loop)`
follows.

Our `Connection::ready` function handles the two connection states we care
about for our `Pong` server; `State::Reading` and `State::Writing`.

Lets talk about `State::Reading` first.

The way our connection handler is structured, when the state is set to
`State::Reading` it can only receive notifications indicating the socket is
readable.  We do that by asserting the event is readable `events.is_readable()`.
Once the handler do receives a notification that the connection is readable it
attempts to read from the socket via the `read` function
`self.read(event_loop)`. This is done by calling `try_read_buf` and passing in
our buffer.

```rust
fn read(&mut self, event_loop: &mut mio::EventLoop<Pong>) {
      match self.socket.try_read_buf(self.state.mut_read_buf()) {
          Ok(Some(0)) => {
              println!("    read 0 bytes from client; buffered={}", self.state.read_buf().len());

              match self.state.read_buf().len() {
                  n if n > 0 => {
                      self.state.transition_to_writing(n);
                      self.reregister(event_loop);
                  }

                  _ => self.state = State::Closed,
              }
          }
          Ok(Some(n)) => {
              println!("read {} bytes", n);
              self.state.try_transition_to_writing();
              self.reregister(event_loop);
          }
          Ok(None) => {
              self.reregister(event_loop);
          }
          Err(e) => {
              panic!("got an error trying to read; err={:?}", e);
          }
      }
  }
```

The `read` function is fully annotated in the example
[here](../examples/ping_pong/src/server.rs).

But lets talk about some the nuanced parts.

The only time a read can succeed while pushing 0 bytes into the buffer
`Ok(Some(0))` is when the socket is closed or the client end of the connection
shut down its half of the connection using
[`shutdown()`](http://rustdoc.s3-website-us-east-1.amazonaws.com/mio/master/mio/tcp/enum.Shutdown.html).

We'll come back to how we handle this later.

If the read returns an `Ok(None)`, the event notification is
a spurious so we reregister the socket with the event loop and wait for another
ready notification.

The most interesting result of a read operation is when the read completes
successfully and some number of bytes have been pushed into our buffer.

The buffer's internal cursor is moved forward, so if the call to `try_read_buf`
is repeated, additional data will be appended to any data that was just pushed
into the buffer.

Once the read is complete and data has been pushed into the buffer, the first
thing to do is check to see if a new line is part of the data in the buffer. If
a new lined character was sent by the client and pushed into the buffer
we want to write all of the data up to the new line back to the client.

The handler transitions the connection into its writing state.

```rust
fn try_transition_to_writing(&mut self) {
    if let Some(pos) = self.read_buf().iter().position(|b| *b == b'\n') {
        self.transition_to_writing(pos + 1);
    }
}

fn transition_to_writing(&mut self, pos: usize) {
    // First, remove the current read buffer, replacing it with an
    // empty Vec<u8>.
    let buf = mem::replace(self, State::Closed)
        .unwrap_read_buf();

    // Wrap in `Cursor`, this allows Vec<u8> to act as a readable
    // buffer
    let buf = Cursor::new(buf);

    // Transition the state to `Writing`, limiting the buffer to the
    // new line (inclusive).
    *self = State::Writing(Take::new(buf, pos));
}
```

Transitioning to the writing state is done by getting the buffer, which
for us is a `Vec<u8>`, wrapping it in `Cursor`, which makes the byte vec
a `Buf`, then transitioning our state field to `State::Writing`.

The algorithm for writing to a socket is very similar to the algorithm for
reading from a socket.  The major difference is that when an event is passed to
a `Connection` in the writing state, the handler checks that the event means
the socket is writeable and if so, attempts the write to the socket.

## Some Final Notes.

We've finished building a working echo server.  

The guide omits a number of utility functions, most of them on
[`State`](../examples/ping_pong/src/server.rs#L249).

The full source of the echo server in its final form is
[here](../examples/ping_pong/src/server.rs).

A working client in its final form can be found
[here](../examples/ping_pong/src/client.rs).
