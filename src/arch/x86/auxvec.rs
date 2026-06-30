//! ELF auxiliary vector constants.
//!
//! Ported from Linux:
//! - `include/uapi/linux/auxvec.h` (generic AT_* values)
//! - `arch/x86/include/uapi/asm/auxvec.h` (x86-specific AT_* values)

// ── Generic AT_* values (include/uapi/linux/auxvec.h) ───────────────────

pub const AT_NULL: u64 = 0;
pub const AT_IGNORE: u64 = 1;
pub const AT_EXECFD: u64 = 2;
pub const AT_PHDR: u64 = 3;
pub const AT_PHENT: u64 = 4;
pub const AT_PHNUM: u64 = 5;
pub const AT_PAGESZ: u64 = 6;
pub const AT_BASE: u64 = 7;
pub const AT_FLAGS: u64 = 8;
pub const AT_ENTRY: u64 = 9;
pub const AT_NOTELF: u64 = 10;
pub const AT_UID: u64 = 11;
pub const AT_EUID: u64 = 12;
pub const AT_GID: u64 = 13;
pub const AT_EGID: u64 = 14;
pub const AT_PLATFORM: u64 = 15;
pub const AT_HWCAP: u64 = 16;
pub const AT_CLKTCK: u64 = 17;
pub const AT_SECURE: u64 = 23;
pub const AT_BASE_PLATFORM: u64 = 24;
pub const AT_RANDOM: u64 = 25;
pub const AT_HWCAP2: u64 = 26;
pub const AT_EXECFN: u64 = 31;
pub const AT_HWCAP3: u64 = 29;
pub const AT_HWCAP4: u64 = 30;

// ── x86-specific AT_* values (arch/x86/include/uapi/asm/auxvec.h) ───────

pub const AT_SYSINFO: u64 = 32;
pub const AT_SYSINFO_EHDR: u64 = 33;

// ── AT_VECTOR_SIZE_ARCH ─────────────────────────────────────────────────
//
// Number of arch-specific entries. For non-compat x86-64 this is 2
// (AT_SYSINFO_EHDR and one spare).

pub const AT_VECTOR_SIZE_ARCH: usize = 2;

// ── AT_HWCAP bit flags for x86_64 ───────────────────────────────────────
//
// These match the CPUID feature bits that glibc expects in AT_HWCAP.
// See `arch/x86/include/asm/cpufeature.h` and `arch/x86/kernel/cpu/capflags.c`.

pub const HWCAP_X86_FPU: u64 = 1 << 0;
pub const HWCAP_X86_VME: u64 = 1 << 1;
pub const HWCAP_X86_DE: u64 = 1 << 2;
pub const HWCAP_X86_PSE: u64 = 1 << 3;
pub const HWCAP_X86_TSC: u64 = 1 << 4;
pub const HWCAP_X86_MSR: u64 = 1 << 5;
pub const HWCAP_X86_PAE: u64 = 1 << 6;
pub const HWCAP_X86_MCE: u64 = 1 << 7;
pub const HWCAP_X86_CX8: u64 = 1 << 8;
pub const HWCAP_X86_APIC: u64 = 1 << 9;
pub const HWCAP_X86_SEP: u64 = 1 << 10;
pub const HWCAP_X86_MTRR: u64 = 1 << 11;
pub const HWCAP_X86_PGE: u64 = 1 << 12;
pub const HWCAP_X86_MCA: u64 = 1 << 13;
pub const HWCAP_X86_CMOV: u64 = 1 << 14;
pub const HWCAP_X86_PAT: u64 = 1 << 15;
pub const HWCAP_X86_PSE36: u64 = 1 << 16;
pub const HWCAP_X86_PN: u64 = 1 << 17;
pub const HWCAP_X86_CLFLUSH: u64 = 1 << 18;
pub const HWCAP_X86_DTS: u64 = 1 << 19;
pub const HWCAP_X86_ACPI: u64 = 1 << 20;
pub const HWCAP_X86_MMX: u64 = 1 << 21;
pub const HWCAP_X86_FXSR: u64 = 1 << 22;
pub const HWCAP_X86_SSE: u64 = 1 << 23;
pub const HWCAP_X86_SSE2: u64 = 1 << 24;
pub const HWCAP_X86_SS: u64 = 1 << 25;
pub const HWCAP_X86_HTT: u64 = 1 << 26;
pub const HWCAP_X86_TM: u64 = 1 << 27;
pub const HWCAP_X86_IA64: u64 = 1 << 28;
pub const HWCAP_X86_PBE: u64 = 1 << 29;
