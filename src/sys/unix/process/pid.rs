use std::io::Error;
use std::process::Child;

use libc::pid_t;

use crate::event::Source;
use crate::{Interest, Registry, Token};

#[derive(Debug)]
pub struct Process {
    pid: pid_t,
}

impl Process {
    pub fn new(child: &Child) -> Result<Self, Error> {
        Self::from_pid(child.id() as pid_t)
    }

    pub fn from_pid(pid: pid_t) -> Result<Self, Error> {
        Ok(Self { pid })
    }
}

impl Source for Process {
    fn register(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> Result<(), Error> {
        registry.selector().register_pid(self.pid, token, interests)
    }

    fn reregister(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> Result<(), Error> {
        registry
            .selector()
            .reregister_pid(self.pid, token, interests)
    }

    fn deregister(&mut self, registry: &Registry) -> Result<(), Error> {
        registry.selector().deregister_pid(self.pid)
    }
}
