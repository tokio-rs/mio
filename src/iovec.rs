use std::mem;

use sys;

pub struct IoVec {
    data: sys::IoVec,
}

impl<'a> From<&'a [u8]> for &'a IoVec {
    fn from(bytes: &'a [u8]) -> &'a IoVec {
        unsafe {
            mem::transmute(<&sys::IoVec>::from(bytes))
        }
    }
}

impl<'a> From<&'a mut [u8]> for &'a mut IoVec {
    fn from(bytes: &'a mut [u8]) -> &'a mut IoVec {
        unsafe {
            mem::transmute(<&mut sys::IoVec>::from(bytes))
        }
    }
}

impl IoVec {
    pub fn as_bytes(&self) -> &[u8] {
        self.data.as_bytes()
    }

    pub fn as_mut_bytes(&mut self) -> &mut [u8] {
        self.data.as_mut_bytes()
    }
}
