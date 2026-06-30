//! RustOS native PID 1 (Milestone 1: prove Ring-3 execution + the ELF loader).
//!
//! This is a static, freestanding ELF executable — our own Rust code, no libc,
//! no dynamic linker. It runs as a real Linux-style userspace process on the
//! kernel's syscall ABI (INT 0x80). For now it just writes a line to stdout and
//! exits, validating that:
//!   1. the ELF loader maps a static ET_EXEC at its link address, and
//!   2. a Ring-3 process can issue syscalls and exit cleanly.
//!
//! It will grow into the real service-manager (Stage 1) once Ring-3 preemption
//! of concurrent processes is proven (see docs/boot-to-desktop-in-rust.md).

#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;

const SYS_WRITE: u64 = 1;
const SYS_EXIT: u64 = 60;

/// Linux/RustOS INT 0x80 syscall, 3 args. Software interrupts preserve all GP
/// registers except the return value in rax, so we only clobber rax.
#[inline(always)]
unsafe fn syscall3(n: u64, a1: u64, a2: u64, a3: u64) -> i64 {
    let ret: i64;
    asm!(
        "int 0x80",
        inlateout("rax") n => ret,
        in("rdi") a1,
        in("rsi") a2,
        in("rdx") a3,
        options(nostack, preserves_flags),
    );
    ret
}

#[inline(always)]
unsafe fn write(fd: u64, buf: &[u8]) -> i64 {
    syscall3(SYS_WRITE, fd, buf.as_ptr() as u64, buf.len() as u64)
}

#[inline(always)]
unsafe fn exit(code: u64) -> ! {
    asm!("int 0x80", in("rax") SYS_EXIT, in("rdi") code, options(noreturn, nostack));
}

#[no_mangle]
#[link_section = ".text._start"]
pub extern "C" fn _start() -> ! {
    unsafe {
        write(1, b"[rustos-init] hello from Ring-3 PID 1 (native Rust, static ELF)\n");
        exit(0);
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    unsafe { exit(101) }
}
