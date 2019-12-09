#![cfg(not(all(
    feature = "os-poll",
    feature = "os-util",
    feature = "tcp",
    feature = "udp",
    feature = "uds"
)))]
compile_error!("run main Mio tests with `--all-features`");
