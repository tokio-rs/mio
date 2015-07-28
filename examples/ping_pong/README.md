# Ping Pong

A simple line based echo server. The server accepts new client
connections and reads data until it sees a new line. The server will
then write back the contents of the line to the client. It will keep
doing this as long as the client remains connected.

[Source](src/main.rs)

## Usage

Run the server with the following:

```
cargo run
```

The server is currently hardcoded to listen on port **6567**
