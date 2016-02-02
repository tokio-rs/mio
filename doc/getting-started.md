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

## Setting up the project

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

## Writing the Echo Server

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
functions.  In the case of our pingpong server the ready handler function
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
additional sockets.

The last argument, the [`events:
EventSet`](http://rustdoc.s3-website-us-east-1.amazonaws.com/mio/master/mio/struct.EventSet.html)
argument, may provide a hint with regard to what will happen when an attempt to
read from the socket is performed.  For example, if the socket experienced an
error, a read from the socket will fail. The `EventSet` argument will be set
to `EventSet::error()`.  But it's important to understand the `events` argument
provides no guarantees. Even if `events` is set to `EventSet::readable()`,
a read from the socket may fail.

Now let's look at the body:

```rust
fn ready(&mut self, event_loop: &mut mio::EventLoop<Pong>, token: mio::Token, events: mio::EventSet) {
    match token {
        SERVER => {
            // Only receive readable events
            assert!(events.is_readable());

            println!("the server socket is ready to accept a connection");
            match self.server.accept() {
                Ok(Some(connection)) => {
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

Since we only ever registered one socket with the event loop, the
`ready` handler will only ever be called with the `SERVER` token.
Then, we try to accept a connection. It's important to note that the
event loop will never operate on any sockets. It only watches sockets
for changes in state.

The signature for the `TcpListener::accept()` function is as follows:

```no_run
fn accept(&self) -> io::Result<Option<TcpStream>>;
```

All non-blocking socket types follow a similar pattern of returning
`io::Result<Option<T>>`. An operation on a non-blocking socket can
obviously flat out fail and return an error. However, the socket can
also be in a good state, but not be ready to operate on. If it were a
blocking socket, the operation would block. Instead, it returns
immediately with Ok(None), in which case we must wait for the event loop
to notify us again that the socket is readable.

> **Important:** Even if we just received a ready notification, there is
> no guarantee that a read from the socket will succeed and not return
> `Ok(None)`, so we must handle that case as well.

If a connection was successfully accepted, we just print some output and
shutdown the event loop. The `event_loop.run(...)` call will return and
the program will exit.

### Handling connections

Our echo server will be pretty simple. It will receive data off of the
connection, buffer it until it sees a newline, and then return the data
to the client.

First, we need a way to store the connection state on the `Pong` server.
To do this, we are going to create a `Connection` struct that will
contain the state for a single client connection. The `Pong` struct will
use a `Slab` to store all the `Connection` instances.

A `Slab` is a fixed capacity map of `Token`, the type that mio uses to
identify sockets (discussed below) to T, defined by the user. In this
case, it will be a `Slab<Connection>`. The advantage of using a slab
over a HashMap is that it is much lighter weight and optimized for use
with mio.

Our `Pong` struct looks like this now:

```rust
struct Pong {
    server: TcpListener,
    connections: Slab<Connection>,
}
```

#### Tokens

Mio's strategy of using token's vs. callbacks for being notified of
events may seem surprising. The reason for this design is to allow Mio
applications to be able to operate at runtime without performing any
allocations. Using a callback for event notification would violate this
requirement.

A `Token` is simply a wrapper around `usize`. In our example, we have
`SERVER` hardcoded to `Token(0)`. We need to ensure that no client
socket gets registered with the same token, otherwise there will be no
way to tell the difference. The `Slab` that we are using to store the
`Connection` instances will be responsible for generating the `Token`
values that are associated with each `Connection` instance. So, we need
to tell it to skip `Token(0)`.

We do it like this:

```rust
impl Pong {
    // Initialize a new `Pong` server from the given TCP listener socket
    fn new(server: TcpListener) -> Pong {
        // Create a Slab with capacity 1024. Skip Token(0).
        let slab = Slab::new_starting_at(mio::Token(1), 1024);

        Pong {
            server: server,
            connections: slab,
        }
    }
}
```

Now when we accept a new client socket, we initialize a new `Connection`
instance and insert it into the `Slab`. We take the token that `Slab`
gives us and use it to register the client socket with the EventLoop. We
will then receive event notifications for that socket.

```rust
match self.server.accept() {
    Ok(Some(socket)) => {
        let token = self.connections
            .insert_with(|token| Connection::new(socket, token))
            .unwrap();

        event_loop.register_opt(
            &self.connections[token].socket,
            token,
            mio::EventSet::readable(),
            mio::PollOpt::edge() | mio::PollOpt::oneshot()).unwrap();
    }
    ...
}
```

In this case, we register the socket with edge + oneshot. When a ready
notification is fired, the socket will be "unarmed". In otherwords, once
a notification for the socket is received, no further notifications will
be fired. When we want to receive another event, we [`reregister`](http://rustdoc.s3-website-us-east-1.amazonaws.com/mio/master/mio/struct.EventLoop.html#method.reregister) the socket with the event loop.

Using this pattern of edge + oneshot makes state management a bit
easier. We only receive notifications when we are ready to handle them.
The alternative is to simply use edge triggered notifications without
oneshot, in which case we would need to handle receiving a notification
but not actually be ready to do anything with it. For example, if we
receive a writable notification for our socket, but we don't have
anything to write yet.

#### Handling client socket events

The client connections are now being accepted and tracked. When events
are pending for these connections, the `ready` function on our handler
will be called. Currently, the `ready` function only handles the server
socket. To start handling client connections, we need to update the
function as such:

```rust
fn ready(&mut self, event_loop: &mut mio::EventLoop<Pong>, token: mio::Token, events: mio::EventSet) {
    match token {
        SERVER => {
            // ... handle incoming connections
        }
        _ => {
            self.connections[token].ready(event_loop, events);
        }
    }
}
```

Here, we assume that any token that is not the server token is a client
one. We lookup the connection state in the `Slab` and then forward the
notifcation to the specific client connection state struct.

## Client connections

The echo server's goal is to read from the client connection until a new
line has been reached, then write back that line to the client (and
repeat). So, there are two clear states. First, the connection is in a
"reading" state where it focuses on reading from the socket then looking
for a new line. If no new line is found, it stays in the reading state.
Otherwise, if a new line is found, it transitions to a writing state
until the line is written back to the client, at which point it
transitions back to the reading state.

So, let's setup the Connection struct to represent that:

```rust
struct Connection {
    socket: TcpStream,
    token: mio::Token,
    state: State,
}

enum State {
    Reading(Vec<u8>),
    Writing(Take<Cursor<Vec<u8>>>),
}
```

In the `Connection` struct, we track the socket so that we can read &
write. We also track token so that we can make calls to
`EventLoop::reregister`. The last field is an enum representing the
current state of the socket as well as any fields required for that
state.

In our case, we only need a buffer to hold data read from the socket.

### Buffers

A **buffer** is an abstraction around byte storage and a cursor and is
used extensively with Mio. The byte storage may or may not be contiguous
memory. Buffers can be readable, represented by the `Buf` trait, they
can also be writable, represented by the `MutBuf` trait.

For example, `Vec<u8>` implements `MutBuf` such that, when new bytes are
written to `Vec<u8>`, they are appended to the end of the array. This
allows using `Vec<u8>` as an argument when reading from an mio socket:

```rust
let mut buf = vec![];
try!(sock.try_read_buf(&mut buf));
```

To write to a socket, `std::io::Cursor<Vec<u8>>` is needed, as such:

```rust
let mut buf = Cursor::new(vec);
try!(sock.try_write_buf(&mut buf));
```

The reason to use a `Buf` vs. just calling `try_write(...)` with a slice
is that, buffers contain an cursor that tracks the current position. So,
it allows calling `try_write_buf` multiple times with the same buffer
and each time, the call will resume from where it left off.

This is an especially useful helper when working with non blocking
sockets, since at any point, the socket may not be ready and we have to
wait and try again later. For example:

```rust
let mut buf = Cursor::new(vec);

while buf.has_remaining() {
  try!(sock.try_read_buf(&mut buf));
}
```

Also, because the actual byte backend is abstracted, it's possible to
have a whole set of different kinds of buffers suitable for different
use cases. The [bytes](github.com/carllerche/bytes) crate contains basic
byte buffers, ring buffer, ropes, etc...

## Reading data

The `Connection::ready` function now looks like this:

> The guide omits a number of utility functions, most of them on
> [`State`](../examples/ping_pong/src/server.rs#L249). The full source of
> the echo server in its final form is
> [here](../examples/ping_pong/src/server.rs).

```rust
fn ready(&mut self, event_loop: &mut mio::EventLoop<Pong>, events: mio::EventSet) {
    match self.state {
        State::Reading(ref mut buf) => {
            assert!(events.is_readable(), "unexpected events; events={:?}", events);
            self.read(event_loop);
        }
        _ => unimplemented!(),
    }
}

fn read(&mut self, event_loop: &mut mio::EventLoop<Pong>) {
    match self.socket.try_read_buf(self.state.mut_read_buf()) {
        Ok(Some(0)) => {
            unimplemented!();
        }
        Ok(Some(n)) => {
            // Look for a new line. If a new line is received, then the
            // state is transitioned from `Reading` to `Writing`.
            self.state.try_transition_to_writing();

            // Re-register the socket with the event loop. The current
            // state is used to determine whether we are currently reading
            // or writing.
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

fn reregister(&self, event_loop: &mut mio::EventLoop<Pong>) {
    let event_set = match self {
        State::Reading(..) => mio::EventSet::readable(),
        State::Writing(..) => mio::EventSet::writable(),
        _ => mio::EventSet::none(),
    };

    event_loop.reregister(&self.socket, self.token, event_set, mio::PollOpt::oneshot())
        .unwrap();
}
```

Our `Connection::ready` function first checks the current state. The way
our connection handler is structured, when the state is set to
`State::Reading`, we can only receive readable notifications. Once we do
receive the notification, we attempt a read. This is done by calling
`try_read_buf` passing in our buffer.

The only time a read can succeed with 0 bytes read is if the socket is
closed or the other end shutdown half the socket using
[`shutdown()`](http://rustdoc.s3-website-us-east-1.amazonaws.com/mio/master/mio/tcp/enum.Shutdown.html).
We'll come back around to handling this later.

In the case of the read returning `Ok(None)`, the ready notification is
a spurious event, so we reregister the socket with the event loop and
wait for another ready notification.

When the read completes successfully, some number of bytes have been
loaded into our buffer. The buffer's internal cursor is moved forward,
so if the call to `try_read_buf` is repeated, additional data will be
appended to any data that was just read.

The first thing to do is check to see if a new line has been read. In
which case, we want to write back the data up to the new line to the
client.

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

Now that the state has been transitioned to writing, when
`self.reregister` is called, we will ask for writable notifications
instead of readable. So, our `Connection::ready` function will be called
once the socket is ready to accept writes. Once this happens, we will be
ready to write back the data that we just read.
