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

    unsafe {
        Star::write(
            crate::gdt::get_user_code_selector(),
            crate::gdt::get_user_data_selector(),
            crate::gdt::get_kernel_code_selector(),
            crate::gdt::get_kernel_data_selector(),
        ).expect("Failed to write STAR MSR");
    }

    // Configure LSTAR MSR - points to syscall entry point
    unsafe {
        LStar::write(VirtAddr::new(syscall_entry as *const () as u64));
    }

    // Configure FMASK MSR - RFLAGS bits to clear on syscall
    // Clear:
    // - IF (bit 9): Disable interrupts during syscall
    // - DF (bit 10): Clear direction flag
    // - TF (bit 8): Clear trap flag
    // - AC (bit 18): Clear alignment check
    let fmask: u64 = (1 << 9) | (1 << 10) | (1 << 8) | (1 << 18);

    unsafe {
        SFMask::write(x86_64::registers::rflags::RFlags::from_bits_truncate(fmask));
    }

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

    naked_asm!(
        // Save user space registers
        // We're now on the kernel stack (from TSS RSP0)

        // Save the return address (RCX) and RFLAGS (R11)
        "push rcx",           // User RIP
        "push r11",           // User RFLAGS

        // Save user stack pointer (will be in a per-CPU or per-task structure)
        // For now, we just preserve it on the stack
        "push rbp",
        "mov rbp, rsp",

        // Save registers that might be clobbered
        "push rbx",
        "push r12",
        "push r13",
        "push r14",
        "push r15",

        // Arguments are already in the right registers for the handler:
        // RAX = syscall number
        // RDI = arg1, RSI = arg2, RDX = arg3
        // R10 = arg4, R8 = arg5, R9 = arg6

        // Call the syscall dispatcher
        // It will read arguments from registers
        "call {syscall_handler}",

        // Result is now in RAX

        // Restore saved registers
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbx",

        "pop rbp",

        // Restore return address and RFLAGS
        "pop r11",           // User RFLAGS
        "pop rcx",           // User RIP

        // Return to user mode with SYSRET
        // SYSRET will:
        // - Load RIP from RCX
        // - Load RFLAGS from R11
        // - Load CS from STAR[63:48]+16
        // - Load SS from STAR[63:48]+8
        // - Set CPL to 3
        "sysretq",

        syscall_handler = sym syscall_handler_wrapper
    );
}

/// Wrapper function that handles syscall dispatch
///
/// This is called from the syscall_entry assembly code.
/// It reads arguments from registers and dispatches to the syscall handler.
#[no_mangle]
extern "C" fn syscall_handler_wrapper() -> i64 {
    let syscall_num: u64;
    let arg1: u64;
    let arg2: u64;
    let arg3: u64;
    let arg4: u64;
    let arg5: u64;
    let arg6: u64;

    unsafe {
        asm!(
            // Read syscall arguments from registers
            "mov {syscall_num}, rax",
            "mov {arg1}, rdi",
            "mov {arg2}, rsi",
            "mov {arg3}, rdx",
            "mov {arg4}, r10",
            "mov {arg5}, r8",
            "mov {arg6}, r9",
            syscall_num = out(reg) syscall_num,
            arg1 = out(reg) arg1,
            arg2 = out(reg) arg2,
            arg3 = out(reg) arg3,
            arg4 = out(reg) arg4,
            arg5 = out(reg) arg5,
            arg6 = out(reg) arg6,
            options(nostack, preserves_flags)
        );
    }

    // Dispatch to the syscall handler
    crate::syscall_handler::dispatch_syscall(
        syscall_num,
        arg1,
        arg2,
        arg3,
        arg4,
        arg5,
        arg6,
    )
}

/// Check if SYSCALL/SYSRET instructions are supported
pub fn is_supported() -> bool {
    // Check CPUID for SYSCALL support
    // CPUID.80000001h:EDX[11] = SYSCALL/SYSRET support
    let mut eax: u32;
    let mut ebx: u32;
    let mut ecx: u32;
    let mut edx: u32;

    unsafe {
        asm!(
            "mov eax, 0x80000001",
            "mov {tmp:e}, ebx",
            "cpuid",
            "mov ebx, {tmp:e}",
            tmp = out(reg) ebx,
            out("eax") eax,
            out("ecx") ecx,
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
