/* Event Traits for converting, building and analyzing events in the selector.
 * To be implemented by the selector for each os/platform */

bitflags!(
    #[deriving(Show)]
    flags IoEventKind: uint {
        static IoReadable = 0x001,
        static IoWritable = 0x002,
        static IoError    = 0x004,
        static IoHangup   = 0x008
    }
)


pub trait IoEvent {

    fn is_readable(&self) -> bool;

    fn is_writable(&self) -> bool;

    fn is_hangup(&self) -> bool;

    fn is_error(&self) -> bool;

    fn to_ioevent(&self) -> IoEventKind;
}

// this should also be part of the trait but until
// we get associated types, I can't think of a good way
//fn from_ioevent(ioevents: IoEventKind) -> OSMaskType;

