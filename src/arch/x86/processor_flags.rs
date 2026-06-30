//! x86 processor flags and control register constants.
//!
//! Ported from Linux `arch/x86/include/uapi/asm/processor-flags.h`.

// ── EFLAGS bits ─────────────────────────────────────────────────────────

pub const X86_EFLAGS_CF: u64 = 1 << 0;
pub const X86_EFLAGS_FIXED: u64 = 1 << 1;
pub const X86_EFLAGS_PF: u64 = 1 << 2;
pub const X86_EFLAGS_AF: u64 = 1 << 4;
pub const X86_EFLAGS_ZF: u64 = 1 << 6;
pub const X86_EFLAGS_SF: u64 = 1 << 7;
pub const X86_EFLAGS_TF: u64 = 1 << 8;
pub const X86_EFLAGS_IF: u64 = 1 << 9;
pub const X86_EFLAGS_DF: u64 = 1 << 10;
pub const X86_EFLAGS_OF: u64 = 1 << 11;
pub const X86_EFLAGS_IOPL: u64 = 3 << 12;
pub const X86_EFLAGS_NT: u64 = 1 << 14;
pub const X86_EFLAGS_RF: u64 = 1 << 16;
pub const X86_EFLAGS_VM: u64 = 1 << 17;
pub const X86_EFLAGS_AC: u64 = 1 << 18;
pub const X86_EFLAGS_VIF: u64 = 1 << 19;
pub const X86_EFLAGS_VIP: u64 = 1 << 20;
pub const X86_EFLAGS_ID: u64 = 1 << 21;

// ── CR0 ─────────────────────────────────────────────────────────────────

pub const X86_CR0_PE: u64 = 1 << 0;
pub const X86_CR0_MP: u64 = 1 << 1;
pub const X86_CR0_EM: u64 = 1 << 2;
pub const X86_CR0_TS: u64 = 1 << 3;
pub const X86_CR0_ET: u64 = 1 << 4;
pub const X86_CR0_NE: u64 = 1 << 5;
pub const X86_CR0_WP: u64 = 1 << 16;
pub const X86_CR0_AM: u64 = 1 << 18;
pub const X86_CR0_NW: u64 = 1 << 29;
pub const X86_CR0_CD: u64 = 1 << 30;
pub const X86_CR0_PG: u64 = 1 << 31;

pub const CR0_STATE: u64 = X86_CR0_PE | X86_CR0_MP | X86_CR0_ET | X86_CR0_NE | X86_CR0_WP | X86_CR0_AM | X86_CR0_PG;

// ── CR3 ─────────────────────────────────────────────────────────────────

pub const X86_CR3_PWT: u64 = 1 << 3;
pub const X86_CR3_PCD: u64 = 1 << 4;
pub const X86_CR3_PCID_BITS: u64 = 12;
pub const X86_CR3_PCID_MASK: u64 = (1 << X86_CR3_PCID_BITS) - 1;
pub const X86_CR3_LAM_U57: u64 = 1 << 61;
pub const X86_CR3_LAM_U48: u64 = 1 << 62;
pub const X86_CR3_PCID_NOFLUSH: u64 = 1 << 63;

// ── CR4 ─────────────────────────────────────────────────────────────────

pub const X86_CR4_VME: u64 = 1 << 0;
pub const X86_CR4_PVI: u64 = 1 << 1;
pub const X86_CR4_TSD: u64 = 1 << 2;
pub const X86_CR4_DE: u64 = 1 << 3;
pub const X86_CR4_PSE: u64 = 1 << 4;
pub const X86_CR4_PAE: u64 = 1 << 5;
pub const X86_CR4_MCE: u64 = 1 << 6;
pub const X86_CR4_PGE: u64 = 1 << 7;
pub const X86_CR4_PCE: u64 = 1 << 8;
pub const X86_CR4_OSFXSR: u64 = 1 << 9;
pub const X86_CR4_OSXMMEXCPT: u64 = 1 << 10;
pub const X86_CR4_UMIP: u64 = 1 << 11;
pub const X86_CR4_LA57: u64 = 1 << 12;
pub const X86_CR4_VMXE: u64 = 1 << 13;
pub const X86_CR4_SMXE: u64 = 1 << 14;
pub const X86_CR4_FSGSBASE: u64 = 1 << 16;
pub const X86_CR4_PCIDE: u64 = 1 << 17;
pub const X86_CR4_OSXSAVE: u64 = 1 << 18;
pub const X86_CR4_SMEP: u64 = 1 << 20;
pub const X86_CR4_SMAP: u64 = 1 << 21;
pub const X86_CR4_PKE: u64 = 1 << 22;
pub const X86_CR4_CET: u64 = 1 << 23;
pub const X86_CR4_LASS: u64 = 1 << 27;
pub const X86_CR4_LAM_SUP: u64 = 1 << 28;

// ── CR8 (Task Priority Register) ────────────────────────────────────────

pub const X86_CR8_TPR: u64 = 0x0000_000f;
