use core::sync::atomic::{AtomicUsize, AtomicBool, Ordering};
//use alloc::vec::Vec;
use core::ptr;

/// Safer implementation of RPC Queue

pub const FIFO_BUFFER_SIZE: usize = 4096;
const FIFO_QUEUE_SIZE: usize = 128;

const FIFO_BASE: usize = 0x44000000;
//static mut FIFO: *mut [[u8; FIFO_BUFFER_SIZE]; FIFO_QUEUE_SIZE] = &mut [[0; FIFO_BUFFER_SIZE]; FIFO_QUEUE_SIZE];
const FIFO: *mut [[u8; FIFO_BUFFER_SIZE]; FIFO_QUEUE_SIZE] = FIFO_BASE as *mut [[u8; FIFO_BUFFER_SIZE]; FIFO_QUEUE_SIZE];
const FIFO_LENS: *mut [usize; FIFO_QUEUE_SIZE] = (FIFO_BASE + FIFO_BUFFER_SIZE * (FIFO_QUEUE_SIZE + 1)) as *mut [usize; FIFO_QUEUE_SIZE];
static FIFO_LOCK: AtomicBool = AtomicBool::new(false);
static FIFO_READ: AtomicUsize = AtomicUsize::new(0);
static FIFO_WRITE: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug)]
pub enum RpcFifoError {
    Unknown = 0,
    FifoFull,
    EmptyRead,
    DataOverflow
}

pub fn init() {
    while FIFO_LOCK.load(Ordering::Relaxed) {}
    FIFO_LOCK.store(true, Ordering::Relaxed);
    unsafe {
        (*FIFO).iter_mut().for_each(|buffer| {
            buffer.iter_mut().for_each(|byte| *byte = 0);
        });
        (*FIFO_LENS).iter_mut().for_each(|val| *val =0);
    }
    FIFO_LOCK.store(false, Ordering::Relaxed);
}

#[inline]
fn next(index: usize) -> usize {
    (index + 1) % FIFO_QUEUE_SIZE
}

pub fn empty() -> bool {
    let (fifo_w, fifo_r) = (FIFO_WRITE.load(Ordering::Relaxed), FIFO_READ.load(Ordering::Relaxed));
    if next(fifo_r) == fifo_w && unsafe {(*FIFO_LENS)[next(fifo_r)]} == 0 {
        true
    } else {
        false
    }
}

pub fn full() -> bool {
    let (fifo_w, fifo_r) = (FIFO_WRITE.load(Ordering::Relaxed), FIFO_READ.load(Ordering::Relaxed));
    if next(fifo_w) == fifo_r && unsafe {(*FIFO_LENS)[next(fifo_w)]} != 0 {
        true
    } else {
        false
    }
}

pub fn push(data: &[u8]) -> Result<usize, RpcFifoError> {
    if data.len() > FIFO_BUFFER_SIZE {
        return Err(RpcFifoError::DataOverflow);
    }

    let (fifo_w, fifo_r) = (FIFO_WRITE.load(Ordering::Relaxed), FIFO_READ.load(Ordering::Relaxed));
    let fifo_n = next(fifo_w);
    if full() {
        return Err(RpcFifoError::FifoFull);
    }
    while FIFO_LOCK.load(Ordering::Relaxed) {}
    FIFO_LOCK.store(true, Ordering::Relaxed);

    unsafe {
        (*FIFO_LENS)[fifo_n] = data.len();
        (*FIFO)[fifo_n].copy_from_slice(data);
        FIFO_WRITE.store(fifo_n, Ordering::Relaxed);
    }
    FIFO_LOCK.store(false, Ordering::Relaxed);
    Ok(data.len())
}

pub fn pull(target: &mut [u8]) -> Result<usize, RpcFifoError> {
    if target.len() < FIFO_BUFFER_SIZE {
        return Err(RpcFifoError::DataOverflow);
    }

    let (fifo_w, fifo_r) = (FIFO_WRITE.load(Ordering::Relaxed), FIFO_READ.load(Ordering::Relaxed));
    let fifo_n = next(fifo_r);
    if empty() {
        return Err(RpcFifoError::EmptyRead);
    }

    while FIFO_LOCK.load(Ordering::Relaxed) {}
    FIFO_LOCK.store(true, Ordering::Relaxed);

    let mut len: usize = 0;
    unsafe {
        len = (*FIFO_LENS)[fifo_n];
       // target.resize(len, 0);
        target[..].copy_from_slice(&(*FIFO)[fifo_n]);
        (*FIFO)[fifo_n].iter_mut().for_each(|byte| { *byte = 0; });
        (*FIFO_LENS)[fifo_n] = 0;

        FIFO_READ.store(fifo_n, Ordering::Relaxed);
    }
    FIFO_LOCK.store(false, Ordering::Relaxed);
    Ok(len)
}