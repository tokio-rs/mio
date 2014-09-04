
bitflags!(
    #[deriving(Show)]
    flags IoEventKind: uint {
        static IoReadable = 0x001,
        static IoWritable = 0x002,
        static IoError    = 0x004,
        static IOHangup   = 0x008
    }
)

#[deriving(Show)]
pub struct IoEvent {
    kind: IoEventKind,
    token: u64
}

impl IoEvent {
    pub fn new(kind: IoEventKind, token: u64) -> IoEvent {
        IoEvent {
            kind: kind,
            token: token
        }
    }

    pub fn is_readable(&self) -> bool {
        self.kind.contains(IoReadable)
    }

    pub fn is_writable(&self) -> bool {
        self.kind.contains(IoWritable)
    }

    buf fn is_hangup(&self) -> bool {
        self.kind.contains(IoHangup)
    }

    pub fn is_error(&self) -> bool {
        self.kind.contains(IoError)
    }
}

trait IoEventMask {

}
