# Ping Pong

A simple line based echo server. The server accepts new client
connections and reads data until it sees a new line. The server will
then write back the contents of the line to the client. It will keep
doing this as long as the client remains connected.

## Usage

There are to executables.

[Server Source](src/server.rs)

[Client Source](src/client.rs)

You must specify which you want to run using the cargo --bin option.

Run the server with the following command:

```
cargo run --bin server
```

The server is currently hardcoded to listen on port **6567**

Run the client with the following command.

```
cargo run --bin client
```
