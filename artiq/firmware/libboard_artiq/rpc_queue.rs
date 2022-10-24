use core::ptr::{read_volatile, write_volatile};
use core::slice;
use board_misoc::{mem, cache, csr::CONFIG_DATA_WIDTH_BYTES};

const SEND_MAILBOX: *mut usize = (mem::MAILBOX_BASE + CONFIG_DATA_WIDTH_BYTES as usize) as *mut usize;
const RECV_MAILBOX: *mut usize = (mem::MAILBOX_BASE + (CONFIG_DATA_WIDTH_BYTES * 2) as usize) as *mut usize;
const QUEUE_LOCK: *mut usize = (mem::MAILBOX_BASE + (CONFIG_DATA_WIDTH_BYTES * 4) as usize) as *mut usize;

const QUEUE_BEGIN: usize = 0x44000000;
const QUEUE_END:   usize = 0x44ffff80;
const QUEUE_CHUNK: usize = 0x1000;
//static QUEUE_LOCK: AtomicBool = AtomicBool::new(false);

pub unsafe fn init() {
    unsafe {info!("init mbase={:#x}, config_data_w={:#x}", mem::MAILBOX_BASE, CONFIG_DATA_WIDTH_BYTES);}

    write_volatile(SEND_MAILBOX, QUEUE_BEGIN);
    write_volatile(RECV_MAILBOX, QUEUE_BEGIN);
    write_volatile(QUEUE_LOCK, 0);
}

fn next(mut addr: usize) -> usize {
    debug_assert!(addr % QUEUE_CHUNK == 0);
    debug_assert!(addr >= QUEUE_BEGIN && addr < QUEUE_END);

    addr += QUEUE_CHUNK;
    if addr >= QUEUE_END { addr = QUEUE_BEGIN }
    addr
}

pub fn empty() -> bool {
    //unsafe {info!("empty s={:#x}, r={:#x}", read_volatile(SEND_MAILBOX), read_volatile(RECV_MAILBOX));}
   // while unsafe { read_volatile(QUEUE_LOCK) } {}
    unsafe { read_volatile(SEND_MAILBOX) == read_volatile(RECV_MAILBOX) }
}

pub fn full() -> bool {
    unsafe {info!("full s={:#x}, r={:#x}", read_volatile(SEND_MAILBOX), read_volatile(RECV_MAILBOX));}
    //while unsafe { read_volatile(QUEUE_LOCK) } {}
    unsafe { next(read_volatile(SEND_MAILBOX)) == read_volatile(RECV_MAILBOX) }
}

pub fn enqueue<T, E, F>(f: F) -> Result<T, E>
        where F: FnOnce(&mut [u8]) -> Result<T, E> {
    unsafe {info!("enqueue s={:#x}, r={:#x}", read_volatile(SEND_MAILBOX), read_volatile(RECV_MAILBOX));}
    debug_assert!(!full());
    //while unsafe { *QUEUE_LOCK != 0 } {}
    unsafe {
        //write_volatile(QUEUE_LOCK, 0xfffafeff);
      //  *QUEUE_LOCK = 0xfffafeff;

        let slice = slice::from_raw_parts_mut(read_volatile(SEND_MAILBOX) as *mut u8, QUEUE_CHUNK);
        let res = f(slice).and_then(|x| {
            write_volatile(SEND_MAILBOX, next(read_volatile(SEND_MAILBOX)));
            Ok(x)
        });
        //write_volatile(QUEUE_LOCK, 0);
       // *QUEUE_LOCK = 0x0;
        res
    }
}

pub fn dequeue<T, E, F>(f: F) -> Result<T, E>
        where F: FnOnce(&mut [u8]) -> Result<T, E> {
    unsafe{info!("dequeue s={:#x}, r={:#x}", read_volatile(SEND_MAILBOX), read_volatile(RECV_MAILBOX));}
    debug_assert!(!empty());
   // while unsafe { *QUEUE_LOCK != 0 } {}
    unsafe {
        cache::flush_cpu_dcache();
       // write_volatile(QUEUE_LOCK, 0xfffafeff);
       // *QUEUE_LOCK = 0xfffafeff;

        let slice = slice::from_raw_parts_mut(read_volatile(RECV_MAILBOX) as *mut u8, QUEUE_CHUNK);
        let res = f(slice).and_then(|x| {
            write_volatile(RECV_MAILBOX, next(read_volatile(RECV_MAILBOX)));
            Ok(x)
        });
       // *QUEUE_LOCK = 0x0;
        //write_volatile(QUEUE_LOCK, 0);
        res
    }
}
