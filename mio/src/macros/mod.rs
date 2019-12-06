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

macro_rules! cfg_todo {
    ($($item:item)*) => {
        $(
            #[cfg(any(feature = "os-ext", feature = "tcp", feature = "udp", feature = "uds"))]
            $item
        )*
    }
}

macro_rules! cfg_net {
    ($($item:item)*) => {
        $(
            #[cfg(any(feature = "tcp", feature = "udp", feature = "uds"))]
            $item
        )*
    }
}

macro_rules! cfg_tcp {
    ($($item:item)*) => {
        $(
            #[cfg(any(feature = "tcp"))]
            $item
        )*
    }
}

macro_rules! cfg_udp {
    ($($item:item)*) => {
        $(
            #[cfg(any(feature = "udp"))]
            $item
        )*
    }
}

macro_rules! cfg_uds {
    ($($item:item)*) => {
        $(
            #[cfg(any(feature = "uds"))]
            $item
        )*
    }
}

// ===== Utility =====

macro_rules! cfg_os_ext {
    ($($item:item)*) => {
        $(
            #[cfg(all(feature = "os-ext", feature = "unix"))]
            $item
        )*
    }
}

macro_rules! cfg_unix_only {
    ($($item:item)*) => {
        $(
            #[cfg(any(all(unix, feature = "os-ext"), all(unix, feature = "uds")))]
            $item
        )*
    }
}
