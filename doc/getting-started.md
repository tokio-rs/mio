# Getting Started

This section will serve as an introductory Mio tutorial. It assumes that
you have familiarity with the [Rust](http://www.rust-lang.org/)
programming language and the [Cargo](https://crates.io) tool. It will
start from generating a new [Rust](http://www.rust-lang.org/) project
using [Cargo](https://crates.io) up to writing a simple TCP echo server
and client.

Of course, you will need Rust installed. If you haven't already, get it
here: [rust-lang.org](https://www.rust-lang.org).

> **Note:** As of the time of writing, Mio does not support Windows
> (though support is currently in progress).

## Setting up the project

The first step is getting a new Cargo project setuup. In a new
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
mio = "0.4.1"
```

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
}

fn main() {
    let address = "0.0.0.0:6567".parse().unwrap();
    let server = TcpListener::bind(&address).unwrap();

    let mut event_loop = mio::EventLoop::new().unwrap();
    event_loop.register(&server, SERVER);

    println!("running pingpong server");
    event_loop.run(&mut Pong { server: server });
}
```

Let's break it down. The first step (at the beginning of the `main`
function), is to create a TCP listener. This will create the socket,
bind to the specified address, and start listening for inbound
connections.

The next step is to register the socket with the event loop.

### The Event Loop

The Mio event loop is able to monitor many sockets and notify the
application when the state of a socket changes. The application
registers sockets with the event loop. This is done by supplying a
`Token` with the socket, associating the two. When the event loop is
started, the application passes in a custom event handler. Whenever the
state of any socket changes, the event loop will notify the event
handler, calling the appropriate event function on the handler and
passing in the originally supplied `Token`.

In our case, the event handler is the `Pong` struct as it implements the
`mio::Handler` trait. We only define the `ready` function, but the
`mio::Handler` trait has [other
functions](http://rustdoc.s3-website-us-east-1.amazonaws.com/mio/master/mio/trait.Handler.html)
that can be defined to handle other event types.

In our `main` function, we create the `EventLoop` value and start it by
calling `event_loop.run` passing a mutable reference to our handler. The
`run` function will block until the event loop is shutdown.

However, before the event loop is started, it must be set up to do some
work. In this case, the pingpong server socket is registered with the
event loop. The constant `SERVER` token is used when registering the
socket. Whenever a connection is ready to be accepted, the event loop
will call the handler's `ready` function passing in the `SERVER` token.
This is how we are able to know, in the handler, which sockets are ready
to be operated on.

> Note:
> [`EventLoop::register_opt`](http://rustdoc.s3-website-us-east-1.amazonaws.com/mio/master/mio/struct.EventLoop.html#method.register_opt)
> allows configuring how the socket is registered with the event loop.

### Handling Events

Once the event loop notifies the handler that a socket is ready to be
operated on, the handler needs to do something. This may include reading
from, writing to, or closing a socket. The first step is to identify the
socket that is ready via the token. So far we only have a single socket
to manage: the `server` socket, so all we do is assert that the given
`Token` matches `SERVER`. However, when there are many sockets, things
get more involved. We will cover handling more than one sockets later in
the guide.

Here is the `ready` signature:

```rust
fn ready(&mut self, event_loop: &mut mio::EventLoop<Pong>, token: mio::Token, events: mio::EventSet) {
  // ...
}
```

For every call into the handler, the event loop will pass a reference to
itself. This allows us to register additional sockets. It also passes in
the `Token` that was associated with the socket during the `register`
call.

The last argument, [`hint:
EventSet`](http://rustdoc.s3-website-us-east-1.amazonaws.com/mio/master/mio/struct.EventSet.html)
sometimes provides a hint as to what will happen when the read is
performed. For example, if the socket experienced an error, a read on
the socket will fail. In this case, the `EventSet` argument will be set
to `EventSet::error()`. However, this is not a guarantee. Even if
`EventSet` is set to `EventSet::readable()`, the read may fail.

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

Even if we just received a ready notification, there is no guarantee
that a read from the socket will succeed and not return `Ok(None)`, so
we must handle that case as well.

If a connection was successfully accepted, we just print some output and
shutdown the event loop. The `event_loop.run(...)` call will return and
the program will exit.

### Handling connections

Our echo server will be pretty simple. It will receive data off of the
connection, buffer it until it sees a newline, and then return the data
to the client. We'll set a max line length to 256 characters, and if we
receive more than that, we will close the client connection.

### Tokens

Mio's strategy of using token's vs. callbacks for being notified of
events may seem surprising. The reason for this design is to allow Mio
applications to be able to operate at runtime without performing any
allocations. Using a callback for event notification would violate this
requirement.

Handlers should be able to access and modify the state surrounding a
socket using only a `Token`. Mio comes with a `Slab` utility to aid in
this process.
