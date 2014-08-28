#![feature(globs)]
#![feature(phase)]

extern crate mio;

#[phase(plugin, link)]
extern crate log;

mod test_echo_server;
