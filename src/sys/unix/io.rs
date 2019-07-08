use std::io;

pub fn set_nonblock(fd: libc::c_int) -> io::Result<()> {
    syscall!(libc::fcntl(fd, libc::F_GETFL)).and_then(|flags| {
        syscall!(libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK)).map(|_| ())
    })
}
