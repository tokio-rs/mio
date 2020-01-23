use std::fmt;
use std::os::raw::c_void;
use std::os::unix::io::{AsRawFd, RawFd};
#[cfg(debug_assertions)]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::{cmp, io};

use crate::interest::Interest;
use crate::sys::unix::waker::pipe::Waker;
use crate::token::Token;

use libc::{c_int, c_short, nfds_t};
use libc::{POLLIN, POLLNVAL, POLLOUT, POLLPRI, POLLRDBAND, POLLRDNORM, POLLWRBAND, POLLWRNORM};

#[cfg(debug_assertions)]
static NEXT_ID: AtomicUsize = AtomicUsize::new(1);

#[derive(Debug)]
enum Task {
    Add {
        fd: RawFd,
        token: Token,
        interests: Interest,
        is_waker: bool,
    },
    Update {
        fd: RawFd,
        token: Token,
        interests: Interest,
    },
    #[allow(dead_code)]
    Rearm {
        fd: RawFd,
        interests: Interest,
    },
    Del {
        fd: RawFd,
    },
}

impl Task {
    fn apply(&self, poll: &mut PollImpl) {
        match self {
            Task::Add {
                fd,
                token,
                interests,
                is_waker,
            } => {
                poll.fdarr.push(libc::pollfd {
                    fd: *fd,
                    events: interests_to_poll(*interests),
                    revents: 0,
                });
                poll.fdmeta.push(FdMetadata {
                    token: usize::from(*token),
                    is_waker: *is_waker,
                });
                // Store index for quick lookup.
                if (poll.fdhash.len() as i32) <= *fd {
                    poll.fdhash.resize((*fd as usize) + 1, None);
                }
                poll.fdhash[*fd as usize] = Some((poll.fdarr.len() as usize) - 1);
            }
            Task::Del { fd } => {
                let pos = poll.fdhash[*fd as usize].unwrap();
                poll.fdarr.remove(pos);
                poll.fdmeta.remove(pos);
                // Re-calculate hashed indexes after arrays shrunk down by 1.
                poll.fdhash.iter_mut().for_each(|opt| match opt.as_mut() {
                    Some(ref mut x) if **x > pos => **x -= 1,
                    Some(ref mut x) if **x == pos => *opt = None,
                    _ => (),
                });
            }
            Task::Update {
                fd,
                token,
                interests,
            } => {
                let pos = poll.fdhash[*fd as usize].unwrap();
                poll.fdarr[pos].events = interests_to_poll(*interests);
                poll.fdmeta[pos].token = usize::from(*token);
            }
            #[allow(dead_code)]
            Task::Rearm { fd, interests } => {
                let pos = poll.fdhash[*fd as usize].unwrap();
                poll.fdarr[pos].events = interests_to_poll(*interests);
            }
        }
    }
}

struct FdMetadata {
    token: usize,
    is_waker: bool,
}

struct PollImpl {
    fdarr: Box<Vec<libc::pollfd>>,
    fdmeta: Box<Vec<FdMetadata>>,
    fdhash: Box<Vec<Option<usize>>>,
}

impl fmt::Debug for PollImpl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PollImpl {{\n")?;
        for (fd, meta) in self.fdarr.iter().zip(self.fdmeta.iter()) {
            write!(
                f,
                "fd={:?} token={:?} events={:?} revents={:?}\n",
                fd.fd, meta.token, fd.events, fd.revents
            )?;
        }
        write!(f, "fdhash {{\n")?;
        for (i, h) in self.fdhash.iter().enumerate() {
            if let Some(x) = h {
                write!(f, "{} => {}\n", i, x)?
            }
        }
        write!(f, "}}\n")?;
        write!(f, "}}\n")?;
        Ok(())
    }
}

#[derive(Debug)]
struct SelectorImpl {
    tasks: Mutex<Vec<Task>>,
    poll: Mutex<PollImpl>,
    waker: Mutex<Option<Box<Waker>>>,
    as_fd: Mutex<RawFd>,
}

#[derive(Debug)]
pub struct Selector {
    #[cfg(debug_assertions)]
    id: usize,
    state: Arc<SelectorImpl>,
}

impl Selector {
    pub fn new() -> io::Result<Selector> {
        let state = Arc::new(SelectorImpl {
            tasks: Mutex::new(vec![]),
            poll: Mutex::new(PollImpl {
                fdarr: Box::new(vec![]),
                fdmeta: Box::new(vec![]),
                fdhash: Box::new(vec![]),
            }),
            waker: Mutex::new(None),
            as_fd: Mutex::new(-1),
        });
        #[cfg(debug_assertions)]
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let sel = Selector {
            #[cfg(debug_assertions)]
            id,
            state,
        };
        sel.add_waker()?;
        Ok(sel)
    }

    fn add_waker(&self) -> io::Result<()> {
        let poll = self.get_impl();
        let mut guard = poll.waker.lock().unwrap();
        *guard = Some(Box::new(Waker::new(self, Token(0))?));
        Ok(())
    }

    fn get_impl(&self) -> &SelectorImpl {
        &*self.state
    }

    pub fn try_clone(&self) -> io::Result<Selector> {
        let state = Arc::clone(&self.state);
        Ok(Selector {
            #[cfg(debug_assertions)]
            id: self.id,
            state,
        })
    }

    pub fn select(&self, events: &mut Events, timeout: Option<Duration>) -> io::Result<()> {
        let me = self.get_impl();
        let mut poll = me.poll.lock().unwrap();
        loop {
            {
                // process pending tasks to (re|de)-register IO sources
                let mut tasks = me.tasks.lock().unwrap();
                for task in tasks.iter() {
                    task.apply(&mut *poll);
                }
                tasks.clear();
            }

            events.clear();
            let timeout = timeout
                .map(|to| cmp::min(to.as_millis(), c_int::max_value() as u128) as c_int)
                .unwrap_or(-1);
            let poll_rv = unsafe {
                let rv = libc::poll(poll.fdarr.as_mut_ptr(), poll.fdarr.len() as nfds_t, timeout);
                if rv < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                rv
            };

            for i in 0..poll.fdarr.len() {
                let revents = poll.fdarr[i].revents;
                let token = poll.fdmeta[i].token;

                // Skip over internal waker at index 0
                if i > 0 && revents != 0 && revents & !POLLNVAL != 0 {
                    events.push(Event {
                        data: token,
                        events: revents,
                    });
                }
                if revents & POLLNVAL != 0 {
                    // This FD died. Someone closed this FD before
                    // deregistering first or there was a race condition
                    // between poll and deregister. Set it to -1 to let
                    // poll know it should be ignored.
                    poll.fdarr[i].fd = -1;
                }
                // Emulate edge triggered events
                if !poll.fdmeta[i].is_waker {
                    poll.fdarr[i].events &= !(revents | POLLNVAL);
                }
                // Empty waker's queue
                if revents != 0 && poll.fdmeta[i].is_waker {
                    let mut buf: [u8; 8] = [0; 8];
                    unsafe {
                        libc::read(poll.fdarr[i].fd, buf.as_mut_ptr() as *mut c_void, 8);
                    }
                }
            }
            if events.len() > 0 || poll_rv == 0 {
                // Something is ready or poll(2) timed out
                break;
            }
        }
        return Ok(());
    }

    fn add_task(&self, task: Task) {
        let me = self.get_impl();
        if let Ok(mut poll) = me.poll.try_lock() {
            // poll not running, apply task directly if nothing sits in queue
            let mut tasks = self.get_impl().tasks.lock().unwrap();
            match tasks.len() {
                0 => task.apply(&mut poll),
                _ => tasks.push(task),
            }
        } else {
            // poll is running, queue task and wake up poll thread
            if let Some(ref waker) = me.waker.lock().unwrap().as_ref() {
                self.get_impl().tasks.lock().unwrap().push(task);
                waker.wake().unwrap();
            }
        }
    }

    pub fn register(&self, fd: RawFd, token: Token, interests: Interest) -> io::Result<()> {
        self.add_task(Task::Add {
            fd,
            token,
            interests,
            is_waker: false,
        });
        Ok(())
    }

    pub(crate) fn register_pipe_waker(
        &self,
        fd: RawFd,
        token: Token,
        interests: Interest,
    ) -> io::Result<()> {
        self.add_task(Task::Add {
            fd,
            token,
            interests,
            is_waker: true,
        });
        let mut as_fd = self.get_impl().as_fd.lock().unwrap();
        if *as_fd == -1 {
            *as_fd = fd
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub fn rearm(&self, fd: RawFd, interests: Interest) -> io::Result<()> {
        self.add_task(Task::Rearm { fd, interests });
        Ok(())
    }

    pub fn reregister(&self, fd: RawFd, token: Token, interests: Interest) -> io::Result<()> {
        self.add_task(Task::Update {
            fd,
            token,
            interests,
        });
        Ok(())
    }

    pub fn deregister(&self, fd: RawFd) -> io::Result<()> {
        self.add_task(Task::Del { fd });
        Ok(())
    }
}

cfg_net! {
    impl Selector {
        #[cfg(debug_assertions)]
        pub fn id(&self) -> usize {
            self.id
        }
    }
}

impl AsRawFd for Selector {
    fn as_raw_fd(&self) -> RawFd {
        *self.get_impl().as_fd.lock().unwrap()
    }
}

fn interests_to_poll(interests: Interest) -> c_short {
    let mut kind = 0;

    if interests.is_readable() {
        kind |= POLLIN | POLLPRI | POLLRDBAND | POLLRDNORM;
    }
    if interests.is_writable() {
        kind |= POLLOUT | POLLWRNORM | POLLWRBAND;
    }
    kind
}

pub struct Event {
    data: usize,
    events: c_short,
}

pub type Events = Vec<Event>;

pub mod event {
    use std::fmt;

    use crate::sys::Event;
    use crate::Token;
    use libc::c_short;
    use libc::{POLLERR, POLLHUP, POLLIN, POLLOUT, POLLPRI};

    pub fn token(event: &Event) -> Token {
        Token(event.data)
    }

    pub fn is_readable(event: &Event) -> bool {
        event.events & (POLLIN | POLLPRI) != 0
    }

    pub fn is_writable(event: &Event) -> bool {
        event.events & POLLOUT != 0
    }

    pub fn is_error(event: &Event) -> bool {
        event.events & POLLERR != 0
    }

    pub fn is_read_closed(_event: &Event) -> bool {
        // Not supported. Use read(2) to detect EOF.
        false
    }

    pub fn is_write_closed(event: &Event) -> bool {
        event.events & POLLHUP != 0
    }

    pub fn is_priority(event: &Event) -> bool {
        event.events & POLLPRI != 0
    }

    pub fn is_aio(_: &Event) -> bool {
        // Not supported.
        false
    }

    pub fn is_lio(_: &Event) -> bool {
        // Not supported.
        false
    }

    pub fn debug_details(f: &mut fmt::Formatter<'_>, _event: &Event) -> fmt::Result {
        #[allow(clippy::trivially_copy_pass_by_ref)]
        fn check_flag(got: &c_short, want: &c_short) -> bool {
            (got & want) != 0
        }

        debug_detail!(
            FlagsDetails(c_short),
            check_flag,
            libc::POLLIN,
            libc::POLLOUT,
            libc::POLLPRI,
            libc::POLLRDBAND,
            libc::POLLRDNORM,
            libc::POLLWRBAND,
            libc::POLLWRNORM,
            libc::POLLNVAL,
            libc::POLLERR
        );

        f.debug_struct("event")
            .field("flags", &FlagsDetails(_event.events))
            .field("data", &_event.data)
            .finish()
    }
}
