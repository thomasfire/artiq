// ARTIQ Exception struct declaration
use cslice::CSlice;
use cslice::{AsCSlice};
use heapless;
use core::{mem, str, slice};

#[repr(C)]
#[derive(Copy, Clone)]
pub struct StringBuffer {
    pub pos: usize,
    pub buf: [u8; 128],
}

impl StringBuffer {
    /// Copies the given string into the buffer safely.
    pub fn copy_str(&mut self, s: &str) {
        let bytes = s.as_bytes();
        let len = bytes.len().min(self.buf.len().saturating_sub(self.pos));

        self.buf[self.pos..self.pos + len].copy_from_slice(&bytes[..len]);
        self.pos += len;
    }

    /// Returns the buffer as a raw byte slice.
    pub fn as_bytes(&self) -> &[u8] {
        &self.buf[..self.pos]
    }

    pub fn as_str(&self) -> &str {
        if self.pos >= self.buf.len() {
            "<host string>"
        } else {
            str::from_utf8(&self.buf[..self.pos]).unwrap_or("<invalid UTF-8>")
        }
    }

    pub fn new() -> Self {
        StringBuffer {
            buf: [0; 128],
            pos: 0,
        }
    }

    pub fn from_str(s: &str) -> Self {
        let mut result: StringBuffer = StringBuffer::new();
        result.copy_str(s);
        result
    }

    pub fn clear(&mut self) {
        self.pos = 0;
        self.buf.fill(0);
    }

    pub fn from_host(message_id: u32) -> Self {
        let mut result = StringBuffer {
            buf: [0; 128],
            pos: usize::MAX,
        };
        result.buf[..4].copy_from_slice(unsafe { &mem::transmute::<u32, [u8; 4]>(message_id) });
        result
    }

    pub fn is_host(&self) -> bool {
        self.pos >= 128
    }
}

impl core::fmt::Write for StringBuffer {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.copy_str(s);
        Ok(())
    }
}

impl<'a> AsCSlice<'a, u8> for StringBuffer {
    fn as_c_slice(&'a self) -> CSlice<'a, u8> {
        unsafe{CSlice::new((self.buf.as_ptr()), self.pos)}
    }
}


// Note: CSlice within an exception may not be actual cslice, they may be strings that exist only
// in the host. If the length == usize:MAX, the pointer is actually a string key in the host.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct Exception<'a> {
    pub id:       u32,
    pub file:     CSlice<'a, u8>,
    pub line:     u32,
    pub column:   u32,
    pub function: CSlice<'a, u8>,
    pub message:  StringBuffer
}

fn str_err(_: core::str::Utf8Error) -> core::fmt::Error {
    core::fmt::Error
}

fn exception_str<'a>(s: &'a CSlice<'a, u8>) -> Result<&'a str, core::str::Utf8Error> {
    if s.len() == usize::MAX {
        Ok("<host string>")
    } else {
        core::str::from_utf8(s.as_ref())
    }
}

impl<'a> core::fmt::Debug for Exception<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.message.is_host() {
            write!(f, "Exception {} from {} in {}:{}:{}, message: {:?}",
                   self.id,
                   exception_str(&self.function).map_err(str_err)?,
                   exception_str(&self.file).map_err(str_err)?,
                   self.line, self.column,
                   &self.message.buf[..4])
        } else {
            write!(f, "Exception {} from {} in {}:{}:{}, message: {}",
                   self.id,
                   exception_str(&self.function).map_err(str_err)?,
                   exception_str(&self.file).map_err(str_err)?,
                   self.line, self.column,
                   exception_str(&self.message.as_str().as_c_slice()).map_err(str_err)?)
        }
    }
}

#[derive(Copy, Clone, Debug, Default)]
pub struct StackPointerBacktrace {
    pub stack_pointer: usize,
    pub initial_backtrace_size: usize,
    pub current_backtrace_size: usize,
}

