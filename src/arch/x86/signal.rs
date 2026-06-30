//! x86-64 signal-frame structures ported from Linux arch/x86 headers:
//!   - `uapi/asm/signal.h`  — SA_* flags, stack_t, old_sigaction
//!   - `asm/sigcontext.h`   — struct sigcontext (register save area)
//!   - `asm/ucontext.h`     — struct ucontext
//!   - `asm/sigframe.h`     — rt_sigframe layout pushed on the user stack

#![allow(dead_code)]

// ---------------------------------------------------------------------------
// x86-64 SA_* flag bits (arch/x86 additions on top of the POSIX set)
// ---------------------------------------------------------------------------

pub const SA_RESTORER: u64 = 0x0400_0000;
pub const SA_NOCLDSTOP: u64 = 1;
pub const SA_NOCLDWAIT: u64 = 2;
pub const SA_SIGINFO: u64 = 4;
pub const SA_ONSTACK: u64 = 0x0800_0000;
pub const SA_RESTART: u64 = 0x1000_0000;
pub const SA_NODEFER: u64 = 0x4000_0000;
pub const SA_RESETHAND: u64 = 0x8000_0000;
pub const SA_NOMASK: u64 = SA_NODEFER;
pub const SA_ONESHOT: u64 = SA_RESETHAND;

pub const MINSIGSTKSZ: usize = 2048;
pub const SIGSTKSZ: usize = 8192;

pub const NSIG: i32 = 32;

// ── FP xstate magic (uapi/asm/sigcontext.h) ─────────────────────────────

pub const FP_XSTATE_MAGIC1: u32 = 0x46505853;
pub const FP_XSTATE_MAGIC2: u32 = 0x46505845;
pub const FP_XSTATE_MAGIC2_SIZE: usize = 4;

// ── ucontext flags (uapi/asm/ucontext.h) ────────────────────────────────

pub const UC_FP_XSTATE: u64 = 0x1;
pub const UC_SIGCONTEXT_SS: u64 = 0x2;
pub const UC_STRICT_RESTORE_SS: u64 = 0x4;

// ---------------------------------------------------------------------------
// stack_t — alternate signal stack descriptor (sigaltstack)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct StackT {
    /// Base address of the alternate signal stack.
    pub ss_sp:    u64,
    /// Flags: SS_ONSTACK | SS_DISABLE | SS_AUTODISARM.
    pub ss_flags: i32,
    /// Size of the alternate signal stack.
    pub ss_size:  u64,
}

pub const SS_ONSTACK:   i32 = 1;
pub const SS_DISABLE:   i32 = 2;
pub const SS_AUTODISARM: i32 = 1 << 31;

// ---------------------------------------------------------------------------
// FpState / XmmReg — FPU/SSE save area embedded in sigcontext
// ---------------------------------------------------------------------------

/// 128-bit XMM register save slot.
#[derive(Clone, Copy, Debug, Default)]
#[repr(C, align(16))]
pub struct XmmReg(pub [u64; 2]);

/// Legacy 512-byte FXSAVE area referenced by sigcontext.fpstate.
#[derive(Clone, Copy, Debug)]
#[repr(C, align(64))]
pub struct FpState {
    pub cwd:       u16,
    pub swd:       u16,
    pub twd:       u16,
    pub fop:       u16,
    pub rip:       u64,
    pub rdp:       u64,
    pub mxcsr:     u32,
    pub mxcr_mask: u32,
    pub st_space:  [u32; 32],    // 8 FP regs × 4 u32
    pub xmm_space: [u32; 64],    // 16 XMM regs × 4 u32
    _padding:       [u32; 12],
}

// ---------------------------------------------------------------------------
// SigContext — register state saved on signal entry (arch/x86/sigcontext.h)
// ---------------------------------------------------------------------------

/// Full CPU register state saved when a signal is delivered.
/// This is what `ucontext.uc_mcontext` contains on x86-64.
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct SigContext {
    pub r8:      u64,
    pub r9:      u64,
    pub r10:     u64,
    pub r11:     u64,
    pub r12:     u64,
    pub r13:     u64,
    pub r14:     u64,
    pub r15:     u64,
    pub rdi:     u64,
    pub rsi:     u64,
    pub rbp:     u64,
    pub rbx:     u64,
    pub rdx:     u64,
    pub rax:     u64,
    pub rcx:     u64,
    pub rsp:     u64,
    pub rip:     u64,
    pub eflags:  u64,
    pub cs:      u16,
    pub gs:      u16,
    pub fs:      u16,
    pub ss:      u16,
    pub err:     u64,
    pub trapno:  u64,
    pub oldmask: u64,   // compat: low 32 bits of blocked signal mask
    pub cr2:     u64,   // faulting address for SIGSEGV
    pub fpstate: u64,   // pointer to FpState (nullable)
    _reserved:   [u64; 8],
}

// ---------------------------------------------------------------------------
// UContext — POSIX user context (asm/ucontext.h)
// ---------------------------------------------------------------------------

/// POSIX `ucontext_t` as passed to SA_SIGINFO handlers.
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct UContext {
    pub uc_flags:    u64,
    pub uc_link:     u64,          // pointer to next context (nullable)
    pub uc_stack:    StackT,
    pub uc_mcontext: SigContext,
    pub uc_sigmask:  u64,          // low 64-bit signal mask (matches SigSet)
}

// ---------------------------------------------------------------------------
// RtSigFrame — the frame pushed on the user stack during signal delivery
// (arch/x86/include/asm/sigframe.h)
// ---------------------------------------------------------------------------

/// Signal frame layout placed on the user stack by the kernel.
/// On return from the handler, the `sa_restorer` trampoline executes
/// `sys_rt_sigreturn` to restore this frame.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct RtSigFrame {
    /// Pointer to `pretcode` field (used as return address by sa_restorer).
    pub pretcode:   u64,
    pub uc:         UContext,
    // SigInfo follows in the ABI but we store a pointer here for clarity
    pub siginfo_ptr: u64,
}
