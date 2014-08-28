use std::ptr;

pub struct ByteBuf {
    ptr: *mut u8,
    cap: uint,
    pos: uint,
    lim: uint
}

impl ByteBuf {
    pub fn new(mut capacity: uint) -> ByteBuf {
        // Handle 0 capacity case
        if capacity == 0 {
            return ByteBuf {
                ptr: ptr::mut_null(),
                cap: 0,
                pos: 0,
                lim: 0
            }
        }

        unimplemented!()
    }
}
