/* Event Traits for converting, building and analyzing events in the selector.
 * To be implemented by the selector for each os/platform */

bitflags!(
    #[deriving(Show)]
    flags IoEventKind: uint {
        static IoReadable = 0x001,
        static IoWritable = 0x002,
        static IoError    = 0x004,
        static IOHangup   = 0x008
    }
)


pub impl IoEvent {
    type MaskType;

    fn is_readable(&self) -> bool;

    fn is_writable(&self) -> bool;

    fn is_hangup(&self) -> bool;

    fn is_error(&self) -> bool;

    fn to_mask(ioevents: IoEventKind) -> MaskType;

    fn from_mask(events: MaskType) -> IoEventKind;
}

