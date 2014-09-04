
pub trait Token : Copy {
    fn from_u64(val: u64) -> Self;

    fn to_u64(self) -> u64;
}

impl Token for int {
    fn from_u64(val: u64) -> int {
        val as int
    }

    fn to_u64(self) -> u64 {
        self as u64
    }
}

impl Token for uint {
    fn from_u64(val: u64) -> uint {
        val as uint
    }

    fn to_u64(self) -> u64 {
        self as u64
    }
}

impl Token for i64 {
    fn from_u64(val: u64) -> i64 {
        val as i64
    }

    fn to_u64(self) -> u64 {
        self as u64
    }
}

impl Token for u64 {
    fn from_u64(val: u64) -> u64 {
        val
    }

    fn to_u64(self) -> u64 {
        self
    }
}

impl Token for () {
    fn from_u64(_: u64) -> () {
        ()
    }

    fn to_u64(self) -> u64 {
        0
    }
}
