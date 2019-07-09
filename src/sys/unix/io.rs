use std::io;

pub fn set_nonblock(fd: libc::c_int) -> io::Result<()> {
    syscall!(fcntl(fd, libc::F_GETFL))
        .and_then(|flags| syscall!(fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK)).map(|_| ()))
}
