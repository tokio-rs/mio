#![cfg(all(feature = "os-poll", feature = "net", feature = "process"))]

use std::process::{Command, Stdio};

use mio::{Interest, Process, Token};

mod util;

use util::{expect_events, init_with_poll_with_capacity, ExpectEvent};

// Test basic process polling functionality by spawning two child processes.
#[test]
fn child_process() {
    let (mut poll, mut events) = init_with_poll_with_capacity(2);
    let mut child1 = new_command().spawn().unwrap();
    let mut child2 = new_command().spawn().unwrap();
    let mut p1 = Process::new(&child1).unwrap();
    let mut p2 = Process::new(&child2).unwrap();

    poll.registry()
        .register(&mut p1, ID1, Interest::READABLE)
        .unwrap();
    poll.registry()
        .register(&mut p2, ID2, Interest::READABLE)
        .unwrap();

    expect_events(
        &mut poll,
        &mut events,
        vec![
            ExpectEvent::new(ID1, Interest::READABLE),
            ExpectEvent::new(ID2, Interest::READABLE),
        ],
    );

    child1.wait().unwrap();
    child2.wait().unwrap();
}

// Test for potential race conditions in process polling by spawning many child processes at once.
#[test]
fn stress_test() {
    let num_processes = 1000;
    let (mut poll, mut events) = init_with_poll_with_capacity(num_processes);
    let mut children = Vec::with_capacity(num_processes);
    let mut procs = Vec::with_capacity(num_processes);
    let mut expected_events = Vec::with_capacity(num_processes);

    for i in 1..=num_processes {
        let child = new_command().spawn().unwrap();
        let mut proc = Process::new(&child).unwrap();
        let token = Token(i);
        poll.registry()
            .register(&mut proc, token, Interest::READABLE)
            .unwrap();
        children.push(child);
        procs.push(proc);
        expected_events.push(ExpectEvent::new(token, Interest::READABLE));
    }

    expect_events(&mut poll, &mut events, expected_events);

    for mut child in children.into_iter() {
        child.wait().unwrap();
    }
}

// Neutral command to test process spawning.
fn new_command() -> Command {
    let mut cmd = Command::new("cargo");
    cmd.arg("--version");
    cmd.stdout(Stdio::null());
    cmd
}

const ID1: Token = Token(1);
const ID2: Token = Token(2);
