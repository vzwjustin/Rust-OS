//! x86_64 ptrace structures and ABI constants.
//!
//! Ported from Linux:
//! - `arch/x86/include/uapi/asm/ptrace.h`
//! - `arch/x86/include/uapi/asm/ptrace-abi.h`

// ── pt_regs register layout for x86_64 ──────────────────────────────────
//
// C ABI says these regs are callee-preserved. They aren't saved on kernel entry
// unless syscall needs a complete, fully filled "struct pt_regs".
//
// Offsets match the kernel `arch/x86/include/uapi/asm/ptrace-abi.h` definitions
// for use by assembly code and debuggers.

pub const R15: usize = 0;
pub const R14: usize = 8;
pub const R13: usize = 16;
pub const R12: usize = 24;
pub const RBP: usize = 32;
pub const RBX: usize = 40;
// These regs are callee-clobbered. Always saved on kernel entry.
pub const R11: usize = 48;
pub const R10: usize = 56;
pub const R9: usize = 64;
pub const R8: usize = 72;
pub const RAX: usize = 80;
pub const RCX: usize = 88;
pub const RDX: usize = 96;
pub const RSI: usize = 104;
pub const RDI: usize = 112;
// On syscall entry, this is syscall#. On CPU exception, this is error code.
// On hw interrupt, it's IRQ number.
pub const ORIG_RAX: usize = 120;
// Return frame for iretq
pub const RIP: usize = 128;
pub const CS: usize = 136;
pub const EFLAGS: usize = 144;
pub const RSP: usize = 152;
pub const SS: usize = 160;

pub const FRAME_SIZE: usize = 168;

/// `struct pt_regs` for x86_64 — the register state saved on the kernel
/// stack on syscall entry, exception, or hardware interrupt.
///
/// Matches the layout in `arch/x86/include/uapi/asm/ptrace.h`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct PtRegs {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub rbp: u64,
    pub rbx: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rax: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub orig_rax: u64,
    pub rip: u64,
    pub cs: u64,
    pub eflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

// ── Ptrace register access constants (ptrace-abi.h) ─────────────────────

pub const PTRACE_GETREGS: u32 = 12;
pub const PTRACE_SETREGS: u32 = 13;
pub const PTRACE_GETFPREGS: u32 = 14;
pub const PTRACE_SETFPREGS: u32 = 15;
pub const PTRACE_GETFPXREGS: u32 = 18;
pub const PTRACE_SETFPXREGS: u32 = 19;
pub const PTRACE_OLDSETOPTIONS: u32 = 21;
pub const PTRACE_GET_THREAD_AREA: u32 = 25;
pub const PTRACE_SET_THREAD_AREA: u32 = 26;
pub const PTRACE_ARCH_PRCTL: u32 = 30;
pub const PTRACE_SYSEMU: u32 = 31;
pub const PTRACE_SYSEMU_SINGLESTEP: u32 = 32;
pub const PTRACE_SINGLEBLOCK: u32 = 33;

// ── UserRegsStruct for PTRACE_GETREGS/SETREGS (x86_64) ──────────────────
//
// This is the `struct user_regs_struct` from `arch/x86/include/asm/user.h`.
// It has the same layout as `pt_regs` but is a separate type for the
// ptrace ABI.

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct UserRegsStruct {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub rbp: u64,
    pub rbx: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rax: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub orig_rax: u64,
    pub rip: u64,
    pub cs: u64,
    pub eflags: u64,
    pub rsp: u64,
    pub ss: u64,
    pub fs_base: u64,
    pub gs_base: u64,
    pub ds: u64,
    pub es: u64,
    pub fs: u64,
    pub gs: u64,
}
