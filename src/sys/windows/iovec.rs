use std::cmp;
use std::mem;
use std::slice;

use winapi::{WSABUF, DWORD};

pub struct IoVec {
    inner: [u8],
}

impl IoVec {
    pub fn as_bytes(&self) -> &[u8] {
        let vec = self.wsabuf();
        unsafe {
            slice::from_raw_parts(vec.buf as *const u8, vec.len as usize)
        }
    }

    pub fn as_mut_bytes(&mut self) -> &mut [u8] {
        let vec = self.wsabuf();
        unsafe {
            slice::from_raw_parts_mut(vec.buf as *mut u8, vec.len as usize)
        }
    }

    pub fn wsabuf(&self) -> WSABUF {
        unsafe { mem::transmute(&self.inner) }
    }
}

impl<'a> From<&'a [u8]> for &'a IoVec {
    fn from(bytes: &'a [u8]) -> &'a IoVec {
        let len = cmp::min(<DWORD>::max_value() as usize, bytes.len());
        unsafe {
            mem::transmute(WSABUF {
                buf: bytes.as_ptr() as *mut _,
                len: len as DWORD,
            })
        }
    }
}

impl<'a> From<&'a mut [u8]> for &'a mut IoVec {
    fn from(bytes: &'a mut [u8]) -> &'a mut IoVec {
        let len = cmp::min(<DWORD>::max_value() as usize, bytes.len());
        unsafe {
            mem::transmute(WSABUF {
                buf: bytes.as_ptr() as *mut _,
                len: len as DWORD,
            })
        }
    }
}
