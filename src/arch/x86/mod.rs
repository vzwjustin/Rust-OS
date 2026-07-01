//! x86 architecture-specific definitions ported from Linux `arch/x86/`.
//!
//! Submodules:
//! - `msr`: MSR index constants from `asm/msr-index.h`
//! - `processor_flags`: EFLAGS/CR0/CR3/CR4 constants from `asm/processor-flags.h`
//! - `signal`: Signal structures and flags from `uapi/asm/signal.h`, `sigcontext.h`, `ucontext.h`, `sigframe.h`
//! - `ptrace`: PtRegs and ptrace ABI constants from `uapi/asm/ptrace.h`, `ptrace-abi.h`
//! - `auxvec`: ELF aux vector constants from `uapi/asm/auxvec.h` and `linux/auxvec.h`
//! - `ldt`: LDT/user_desc structures from `uapi/asm/ldt.h`
//! - `prctl`: arch_prctl codes from `uapi/asm/prctl.h`
//! - `mman`: x86-specific mmap flags from `uapi/asm/mman.h`
//! - `hwcap`: HWCAP2 constants from `uapi/asm/hwcap2.h`

pub mod auxvec;
pub mod hwcap;
pub mod ldt;
pub mod mman;
pub mod msr;
pub mod prctl;
pub mod processor_flags;
pub mod ptrace;
pub mod signal;
