//! Fast Syscall Support (SYSCALL/SYSRET instructions)
//!
//! This module provides support for the modern SYSCALL/SYSRET instructions
//! which offer faster privilege level switching than INT/IRET on x86_64.
//!
//! The SYSCALL instruction:
//! - Loads CS from IA32_STAR MSR
//! - Loads SS from IA32_STAR MSR + 8
//! - Loads RIP from IA32_LSTAR MSR
//! - Saves RFLAGS to R11
//! - Masks RFLAGS using IA32_FMASK MSR
//! - Saves return address to RCX
//!
//! The SYSRET instruction reverses this process.

use core::arch::asm;
use x86_64::registers::model_specific::{LStar, SFMask, Star};
use x86_64::VirtAddr;

/// MSR numbers for SYSCALL/SYSRET support
const IA32_STAR: u32 = 0xC000_0081;
const IA32_LSTAR: u32 = 0xC000_0082;
const IA32_FMASK: u32 = 0xC000_0084;
const IA32_EFER: u32 = 0xC000_0080;

/// EFER bits
const EFER_SCE: u64 = 1 << 0; // System Call Extensions

/// Initialize SYSCALL/SYSRET support
///
/// This configures the MSRs required for fast syscalls:
/// - STAR: Segment selectors for kernel/user code and data
/// - LSTAR: Syscall entry point address
/// - FMASK: RFLAGS mask (bits to clear on syscall entry)
/// - EFER.SCE: Enable syscall/sysret instructions
pub fn init() {
    // Get segment selectors from GDT
    let kernel_code = crate::gdt::get_kernel_code_selector().0 as u64;
    let user_code = crate::gdt::get_user_code_selector().0 as u64;

    // Configure STAR MSR
    // STAR format:
    // [63:48] - User CS and SS base selector (user_code - 16)
    // [47:32] - Kernel CS and SS base selector
    // [31:0]  - Reserved
    //
    // SYSCALL loads:
    // - CS = STAR[47:32]
    // - SS = STAR[47:32] + 8
    //
    // SYSRET loads:
    // - CS = STAR[63:48] + 16
    // - SS = STAR[63:48] + 8
    let _star_value = (user_code - 16) << 48 | kernel_code << 32;

    Star::write(
        crate::gdt::get_user_code_selector(),
        crate::gdt::get_user_data_selector(),
        crate::gdt::get_kernel_code_selector(),
        crate::gdt::get_kernel_data_selector(),
    )
    .expect("Failed to write STAR MSR");

    // Configure LSTAR MSR - points to syscall entry point
    LStar::write(VirtAddr::new(syscall_entry as *const () as u64));

    // Configure FMASK MSR - RFLAGS bits to clear on syscall
    // Clear:
    // - IF (bit 9): Disable interrupts during syscall
    // - DF (bit 10): Clear direction flag
    // - TF (bit 8): Clear trap flag
    // - AC (bit 18): Clear alignment check
    let fmask: u64 = (1 << 9) | (1 << 10) | (1 << 8) | (1 << 18);

    SFMask::write(x86_64::registers::rflags::RFlags::from_bits_truncate(fmask));

    // Enable SYSCALL/SYSRET in EFER
    unsafe {
        let mut efer = read_msr(IA32_EFER);
        efer |= EFER_SCE;
        write_msr(IA32_EFER, efer);
    }

    crate::serial_println!("Fast syscall support initialized");
}

/// Read from a Model-Specific Register
unsafe fn read_msr(msr: u32) -> u64 {
    let low: u32;
    let high: u32;

    asm!(
        "rdmsr",
        in("ecx") msr,
        out("eax") low,
        out("edx") high,
        options(nostack, preserves_flags)
    );

    ((high as u64) << 32) | (low as u64)
}

/// Write to a Model-Specific Register
unsafe fn write_msr(msr: u32, value: u64) {
    let low = value as u32;
    let high = (value >> 32) as u32;

    asm!(
        "wrmsr",
        in("ecx") msr,
        in("eax") low,
        in("edx") high,
        options(nostack, preserves_flags)
    );
}

/// SYSCALL entry point
///
/// This is the kernel entry point when userspace executes SYSCALL.
/// At entry:
/// - RCX contains the return address (user RIP)
/// - R11 contains the saved RFLAGS
/// - CS/SS are set to kernel segments
/// - Interrupts are disabled (by FMASK)
///
/// Syscall arguments are in registers:
/// - RAX: syscall number
/// - RDI: arg1
/// - RSI: arg2
/// - RDX: arg3
/// - R10: arg4
/// - R8: arg5
/// - R9: arg6
///
/// We must preserve RCX and R11 to return with SYSRET.
#[unsafe(naked)]
pub unsafe extern "C" fn syscall_entry() {
    use core::arch::naked_asm;

    // ponytail: push the full user GP register set to form a `SyscallFrame` the
    // Rust handler reads through a pointer. The old code `call`ed a plain
    // `extern "C"` wrapper that then re-read rax/rdi/rsi/... via `asm!` — but the
    // wrapper's own C prologue had already clobbered those arg registers, so the
    // handler saw garbage. Passing a pointer to a frame we control fixes the
    // calling convention without needing 7-argument stack-alignment juggling.
    //
    // SYSCALL entry state: RAX=num, RDI/RSI/RDX/R10/R8/R9=args,
    // RCX=user RIP, R11=user RFLAGS (RCX/R11 must survive for SYSRETQ).
    naked_asm!(
        // Push order (high->low addr). Read from the frame pointer (rsp after the
        // pushes) the layout is the reverse: r15 at offset 0 ... rax at offset 112.
        "push rax",           // syscall number
        "push rbx",
        "push rcx",           // user RIP
        "push rdx",           // arg3
        "push rsi",           // arg2
        "push rdi",           // arg1
        "push rbp",
        "push r8",            // arg5
        "push r9",            // arg6
        "push r10",           // arg4
        "push r11",           // user RFLAGS
        "push r12",
        "push r13",
        "push r14",
        "push r15",

        // Pass &frame (current rsp) as the single SysV argument.
        "mov rdi, rsp",

        // Align the stack to 16 bytes for the call without losing the frame base.
        "mov rbp, rsp",
        "and rsp, -16",

        "call {syscall_handler}",

        // Restore exact stack and overwrite the saved RAX slot (offset 14*8 = 112)
        // with the i64 return value so the user sees it after SYSRETQ.
        "mov rsp, rbp",
        "mov [rsp + 112], rax",

        // Pop the frame back into registers (reverse of the push order).
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop r11",            // user RFLAGS
        "pop r10",
        "pop r9",
        "pop r8",
        "pop rbp",
        "pop rdi",
        "pop rsi",
        "pop rdx",
        "pop rcx",            // user RIP
        "pop rbx",
        "pop rax",            // return value

        // Return to user mode with SYSRET (RIP<-RCX, RFLAGS<-R11, CPL<-3).
        "sysretq",

        syscall_handler = sym syscall_handler_wrapper
    );
}

/// Saved user register frame built by `syscall_entry` before the dispatch call.
///
/// Field order matches the on-stack layout (lowest address first), i.e. the
/// reverse of the push order in `syscall_entry`.
#[repr(C)]
struct SyscallFrame {
    r15: u64,
    r14: u64,
    r13: u64,
    r12: u64,
    r11: u64,
    r10: u64,
    r9: u64,
    r8: u64,
    rbp: u64,
    rdi: u64,
    rsi: u64,
    rdx: u64,
    rcx: u64,
    rbx: u64,
    rax: u64,
}

/// Wrapper function that handles syscall dispatch.
///
/// Called from `syscall_entry` with a pointer to the saved user register frame.
/// Reading the args from the frame (rather than re-reading the live registers)
/// is what makes this correct: by the time a normal `extern "C"` body runs, its
/// prologue may already have reused the SysV arg registers.
#[no_mangle]
extern "C" fn syscall_handler_wrapper(frame: *const SyscallFrame) -> i64 {
    // ponytail: arg mapping follows the Linux x86_64 syscall convention —
    // num=rax, arg1=rdi, arg2=rsi, arg3=rdx, arg4=r10, arg5=r8, arg6=r9.
    let f = unsafe { &*frame };

    crate::syscall_handler::dispatch_syscall(f.rax, f.rdi, f.rsi, f.rdx, f.r10, f.r8, f.r9)
}

/// Check if SYSCALL/SYSRET instructions are supported
pub fn is_supported() -> bool {
    // Check CPUID for SYSCALL support
    // CPUID.80000001h:EDX[11] = SYSCALL/SYSRET support
    let mut _eax: u32;
    let mut _ebx: u32;
    let mut _ecx: u32;
    let mut edx: u32;

    unsafe {
        asm!(
            "mov eax, 0x80000001",
            "mov {tmp:e}, ebx",
            "cpuid",
            "mov ebx, {tmp:e}",
            tmp = out(reg) _ebx,
            out("eax") _eax,
            out("ecx") _ecx,
            out("edx") edx,
            options(nostack, preserves_flags)
        );
    }

    // Check bit 11 of EDX
    (edx & (1 << 11)) != 0
}

/// Execute a syscall from kernel mode (for testing)
///
/// This is primarily for testing the syscall mechanism.
/// Normal userspace programs would execute SYSCALL directly.
///
/// # Safety
///
/// This should only be called for testing purposes.
pub unsafe fn test_syscall(syscall_num: u64, arg1: u64, arg2: u64, arg3: u64) -> i64 {
    let result: i64;

    asm!(
        "mov rax, {syscall_num}",
        "mov rdi, {arg1}",
        "mov rsi, {arg2}",
        "mov rdx, {arg3}",
        "syscall",
        "mov {result}, rax",
        syscall_num = in(reg) syscall_num,
        arg1 = in(reg) arg1,
        arg2 = in(reg) arg2,
        arg3 = in(reg) arg3,
        result = out(reg) result,
        out("rcx") _,  // clobbered by syscall
        out("r11") _,  // clobbered by syscall
        options(nostack)
    );

    result
}
