use core::{str, str::Utf8Error, slice};
use alloc::{vec::Vec, format, collections::BTreeMap, string::String};
use eh::eh_artiq::{Exception, StackPointerBacktrace, StringBuffer};
use cslice::CSlice;
use cslice::AsCSlice;

use io::{Read, ProtoRead, Write, ProtoWrite, Error as IoError, ReadStringError};

pub type DeviceMap = BTreeMap<u32, String>;

static mut RTIO_DEVICE_MAP: Option<DeviceMap> = None;

#[derive(Fail, Debug)]
pub enum Error<T> {
    #[fail(display = "incorrect magic")]
    WrongMagic,
    #[fail(display = "unknown packet {:#02x}", _0)]
    UnknownPacket(u8),
    #[fail(display = "invalid UTF-8: {}", _0)]
    Utf8(Utf8Error),
    #[fail(display = "{}", _0)]
    Io(#[cause] IoError<T>)
}

impl<T> From<IoError<T>> for Error<T> {
    fn from(value: IoError<T>) -> Error<T> {
        Error::Io(value)
    }
}

impl<T> From<ReadStringError<IoError<T>>> for Error<T> {
    fn from(value: ReadStringError<IoError<T>>) -> Error<T> {
        match value {
            ReadStringError::Utf8(err) => Error::Utf8(err),
            ReadStringError::Other(err) => Error::Io(err)
        }
    }
}

pub fn read_magic<R>(reader: &mut R) -> Result<(), Error<R::ReadError>>
    where R: Read + ?Sized
{
    const MAGIC: &'static [u8] = b"ARTIQ coredev\n";

    let mut magic: [u8; 14] = [0; 14];
    reader.read_exact(&mut magic)?;
    if magic != MAGIC {
        Err(Error::WrongMagic)
    } else {
        Ok(())
    }
}

fn read_sync<R>(reader: &mut R) -> Result<(), IoError<R::ReadError>>
    where R: Read + ?Sized
{
    let mut sync = [0; 4];
    for i in 0.. {
        sync[i % 4] = reader.read_u8()?;
        if sync == [0x5a; 4] { break }
    }
    Ok(())
}

fn write_sync<W>(writer: &mut W) -> Result<(), IoError<W::WriteError>>
    where W: Write + ?Sized
{
    writer.write_all(&[0x5a; 4])
}

#[derive(Debug)]
pub enum Request {
    SystemInfo,

    LoadKernel(Vec<u8>),
    RunKernel,

    RpcReply { tag: Vec<u8> },
    RpcException {
        id:       u32,
        message:  u32,
        file:     u32,
        line:     u32,
        column:   u32,
        function: u32,
    },

    UploadSubkernel { id: u32, destination: u8, kernel: Vec<u8> },
}

#[derive(Debug)]
pub enum Reply<'a> {
    SystemInfo {
        ident: &'a str,
        finished_cleanly: bool
    },

    LoadCompleted,
    LoadFailed(&'a str),

    KernelFinished {
        async_errors: u8
    },
    KernelStartupFailed,
    KernelException {
        exceptions: &'a [Option<Exception<'a>>],
        stack_pointers: &'a [StackPointerBacktrace],
        backtrace: &'a [(usize, usize)],
        async_errors: u8
    },

    RpcRequest { async: bool },

    ClockFailure,
}

impl Request {
    pub fn read_from<R>(reader: &mut R) -> Result<Self, Error<R::ReadError>>
        where R: Read + ?Sized
    {
        read_sync(reader)?;
        Ok(match reader.read_u8()? {
            3  => Request::SystemInfo,

            5  => Request::LoadKernel(reader.read_bytes()?),
            6  => Request::RunKernel,

            7  => Request::RpcReply {
                tag: reader.read_bytes()?
            },
            8  => Request::RpcException {
                id:       reader.read_u32()?,
                message:  reader.read_u32()?,
                file:     reader.read_u32()?,
                line:     reader.read_u32()?,
                column:   reader.read_u32()?,
                function: reader.read_u32()?
            },
            9 => Request::UploadSubkernel {
                id: reader.read_u32()?,
                destination: reader.read_u8()?,
                kernel: reader.read_bytes()?
            },

            ty  => return Err(Error::UnknownPacket(ty))
        })
    }
}

fn write_exception_string<'a, W>(writer: &mut W, s: &CSlice<'a, u8>) -> Result<(), IoError<W::WriteError>>
    where W: Write + ?Sized
{
    if s.len() == usize::MAX {
        writer.write_u32(u32::MAX)?;
        writer.write_u32(s.as_ptr() as u32)?;
    } else {
        writer.write_string(core::str::from_utf8(s.as_ref()).unwrap())?;
    }
    Ok(())
}

fn write_exception_stringbuffer<'a, W>(writer: &mut W, s: &StringBuffer) -> Result<(), IoError<W::WriteError>>
    where W: Write + ?Sized
{
    use core::convert::TryInto;
    if s.is_host() {
        writer.write_u32(u32::MAX)?;

        let bytes: &[u8] = &s.buf[0..4];
        let byte_array: [u8; 4] = bytes.try_into().expect("Slice must have exactly 4 bytes");
        let value = unsafe { core::mem::transmute::<[u8; 4], u32>(byte_array) };
        debug!("value: {}", value);
        writer.write_u32(value)?;
        //writer.write_u32(unsafe { core::mem::transmute::<[u8; 4], u32>(core::slice::from_raw_parts(s.as_ptr(), 4)) })?;
    } else {
        debug!("write_string_buffer {}", s.pos);
        writer.write_string(s.as_str())?;
    }
    Ok(())
}

impl<'a> Reply<'a> {
    pub fn write_to<W>(&self, writer: &mut W) -> Result<(), IoError<W::WriteError>>
        where W: Write + ?Sized
    {
        write_sync(writer)?;
        match *self {
            Reply::SystemInfo { ident, finished_cleanly } => {
                writer.write_u8(2)?;
                writer.write(b"AROR")?;
                writer.write_string(ident)?;
                writer.write_u8(finished_cleanly as u8)?;
            },

            Reply::LoadCompleted => {
                writer.write_u8(5)?;
            },
            Reply::LoadFailed(reason) => {
                writer.write_u8(6)?;
                writer.write_string(reason)?;
            },

            Reply::KernelFinished { async_errors } => {
                writer.write_u8(7)?;
                writer.write_u8(async_errors)?;
            },
            Reply::KernelStartupFailed => {
                writer.write_u8(8)?;
            },
            Reply::KernelException {
                exceptions,
                stack_pointers,
                backtrace,
                async_errors
            } => {
                writer.write_u8(9)?;
                writer.write_u32(exceptions.len() as u32)?;
                for exception in exceptions.iter() {
                    let exception = exception.as_ref().unwrap();
                    writer.write_u32(exception.id as u32)?;
                    write_exception_stringbuffer(writer, &exception.message)?;
                    write_exception_string(writer, &exception.file)?;
                    writer.write_u32(exception.line)?;
                    writer.write_u32(exception.column)?;
                    write_exception_string(writer, &exception.function)?;
                }

                for sp in stack_pointers.iter() {
                    writer.write_u32(sp.stack_pointer as u32)?;
                    writer.write_u32(sp.initial_backtrace_size as u32)?;
                    writer.write_u32(sp.current_backtrace_size as u32)?;
                }

                writer.write_u32(backtrace.len() as u32)?;
                for &(addr, sp) in backtrace {
                    writer.write_u32(addr as u32)?;
                    writer.write_u32(sp as u32)?;
                }
                writer.write_u8(async_errors)?;
            },

            Reply::RpcRequest { async } => {
                writer.write_u8(10)?;
                writer.write_u8(async as u8)?;
            },

            Reply::ClockFailure => {
                writer.write_u8(15)?;
            },
        }
        Ok(())
    }
}

pub fn set_device_map(device_map: DeviceMap) {
    unsafe { RTIO_DEVICE_MAP = Some(device_map); }
}

fn _resolve_channel_name(channel: u32, device_map: &Option<DeviceMap>) -> String {
    if let Some(dev_map) = device_map {
        match dev_map.get(&channel) {
            Some(val) => val.clone(),
            None => String::from("unknown")
        }
    } else { String::from("unknown") }
}

pub fn resolve_channel_name(channel: u32) -> String {
    _resolve_channel_name(channel, unsafe{&RTIO_DEVICE_MAP})
}
