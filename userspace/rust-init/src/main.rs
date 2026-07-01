#![no_main]
#![no_std]

use core::arch::asm;
use core::panic::PanicInfo;

const SYS_WRITE: usize = 1;
const SYS_EXIT: usize = 60;
const SYS_SCHED_YIELD: usize = 24;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    let msg = b"rustos-init: hi from static Rust PID1\n";
    write(1, msg);
    exit(0)
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    let msg = b"rustos-init: panic\n";
    write(2, msg);
    exit(101)
}

/// Write `buf` to file descriptor `fd`. Returns the number of bytes written
/// or a negative errno on failure.
#[inline(always)]
fn write(fd: usize, buf: &[u8]) -> isize {
    // SAFETY: syscall3 only touches registers; the buffer pointer/len are
    // derived from a valid Rust slice so the kernel reads in-bounds memory.
    unsafe { syscall3(SYS_WRITE, fd, buf.as_ptr() as usize, buf.len()) }
}

/// Exit the process with status `code`. Never returns.
fn exit(code: usize) -> ! {
    loop {
        // SAFETY: the exit syscall never returns; if it somehow does, we
        // yield and retry rather than falling through.
        unsafe {
            let _ = syscall1(SYS_EXIT, code);
            let _ = syscall0(SYS_SCHED_YIELD);
            asm!("pause", options(nomem, nostack, preserves_flags));
        }
    }
}

unsafe fn syscall0(number: usize) -> isize {
    let ret: isize;
    asm!(
        "syscall",
        inlateout("rax") number as isize => ret,
        lateout("rcx") _,
        lateout("r11") _,
        options(nostack),
    );
    ret
}

unsafe fn syscall1(number: usize, arg0: usize) -> isize {
    let ret: isize;
    asm!(
        "syscall",
        inlateout("rax") number as isize => ret,
        in("rdi") arg0,
        lateout("rcx") _,
        lateout("r11") _,
        options(nostack),
    );
    ret
}

unsafe fn syscall3(number: usize, arg0: usize, arg1: usize, arg2: usize) -> isize {
    let ret: isize;
    asm!(
        "syscall",
        inlateout("rax") number as isize => ret,
        in("rdi") arg0,
        in("rsi") arg1,
        in("rdx") arg2,
        lateout("rcx") _,
        lateout("r11") _,
        options(nostack),
    );
    ret
}
