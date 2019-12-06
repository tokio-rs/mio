#![allow(unused_macros)]

// ===== Poll =====

macro_rules! cfg_os_poll {
    ($($item:item)*) => {
        $(
            #[cfg(feature = "os-poll")]
            $item
        )*
    }
}

macro_rules! cfg_not_os_poll {
    ($($item:item)*) => {
        $(
            #[cfg(not(feature = "os-poll"))]
            $item
        )*
    }
}

// ===== Net =====

#[cfg(unix)]
macro_rules! cfg_net {
    ($($item:item)*) => {
        $(
            #[cfg(any(feature = "tcp", feature = "udp", feature = "uds"))]
            $item
        )*
    }
}

#[cfg(windows)]
macro_rules! cfg_net {
    ($($item:item)*) => {
        $(
            #[cfg(any(feature = "tcp", feature = "udp"))]
            $item
        )*
    }
}

macro_rules! cfg_tcp {
    ($($item:item)*) => {
        $(
            #[cfg(feature = "tcp")]
            $item
        )*
    }
}

macro_rules! cfg_udp {
    ($($item:item)*) => {
        $(
            #[cfg(feature = "udp")]
            $item
        )*
    }
}

#[cfg(unix)]
macro_rules! cfg_uds {
    ($($item:item)*) => {
        $(
            #[cfg(feature = "uds")]
            $item
        )*
    }
}

// ===== Utility =====

// cfg for any feature that requires the OS's adapter for `RawFd`
#[cfg(unix)]
macro_rules! cfg_any_os_util {
    ($($item:item)*) => {
        $(
            #[cfg(any(feature = "os-util", feature = "tcp", feature = "udp", feature = "uds"))]
            $item
        )*
    }
}

// cfg for any feature that requires the OS's adapter for`RawSocket`
#[cfg(windows)]
macro_rules! cfg_any_os_util {
    ($($item:item)*) => {
        $(
            #[cfg(any(feature = "os-util", feature = "tcp", feature = "udp"))]
            $item
        )*
    }
}

// cfg for depending only on the OS's adapter for `RawFd` or `RawSocket`
macro_rules! cfg_os_util {
    ($($item:item)*) => {
        $(
            #[cfg(feature = "os-util")]
            $item
        )*
    }
}
