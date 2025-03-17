cfg_os_proc_pidfd! {
    mod pidfd;
    pub use self::pidfd::*;
}

cfg_os_proc_kqueue! {
    mod pid;
    pub use self::pid::*;
}
