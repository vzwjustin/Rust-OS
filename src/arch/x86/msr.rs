//! MSR (Model-Specific Register) index constants.
//!
//! Ported from Linux `arch/x86/include/asm/msr-index.h`.

// ── x86-64 specific MSRs ────────────────────────────────────────────────

pub const MSR_EFER: u32 = 0xc0000080;
pub const MSR_STAR: u32 = 0xc0000081;
pub const MSR_LSTAR: u32 = 0xc0000082;
pub const MSR_CSTAR: u32 = 0xc0000083;
pub const MSR_SYSCALL_MASK: u32 = 0xc0000084;
pub const MSR_FS_BASE: u32 = 0xc0000100;
pub const MSR_GS_BASE: u32 = 0xc0000101;
pub const MSR_KERNEL_GS_BASE: u32 = 0xc0000102;
pub const MSR_TSC_AUX: u32 = 0xc0000103;

// ── EFER bits ───────────────────────────────────────────────────────────

pub const EFER_SCE: u64 = 1 << 0;
pub const EFER_LME: u64 = 1 << 8;
pub const EFER_LMA: u64 = 1 << 10;
pub const EFER_NX: u64 = 1 << 11;
pub const EFER_SVME: u64 = 1 << 12;
pub const EFER_LMSLE: u64 = 1 << 13;
pub const EFER_FFXSR: u64 = 1 << 14;
pub const EFER_TCE: u64 = 1 << 15;
pub const EFER_AUTOIBRS: u64 = 1 << 21;

// ── Intel MSRs ──────────────────────────────────────────────────────────

pub const MSR_IA32_SPEC_CTRL: u32 = 0x00000048;
pub const SPEC_CTRL_IBRS: u64 = 1 << 0;
pub const SPEC_CTRL_STIBP: u64 = 1 << 1;
pub const SPEC_CTRL_SSBD: u64 = 1 << 2;
pub const SPEC_CTRL_RRSBA_DIS_S: u64 = 1 << 6;
pub const SPEC_CTRL_BHI_DIS_S: u64 = 1 << 10;

pub const MSR_IA32_PRED_CMD: u32 = 0x00000049;
pub const PRED_CMD_IBPB: u64 = 1 << 0;
pub const PRED_CMD_SBPB: u64 = 1 << 7;

pub const MSR_IA32_PERFCTR0: u32 = 0x000000c1;
pub const MSR_IA32_PERFCTR1: u32 = 0x000000c2;
pub const MSR_FSB_FREQ: u32 = 0x000000cd;
pub const MSR_PLATFORM_INFO: u32 = 0x000000ce;

pub const MSR_IA32_UCODE_REV: u32 = 0x0000008b;

pub const MSR_IA32_SYSENTER_CS: u32 = 0x00000174;
pub const MSR_IA32_SYSENTER_ESP: u32 = 0x00000175;
pub const MSR_IA32_SYSENTER_EIP: u32 = 0x00000176;

pub const MSR_IA32_CR_PAT: u32 = 0x00000277;

pub const MSR_IA32_APICBASE: u32 = 0x0000001b;
pub const MSR_IA32_APICBASE_ENABLE: u64 = 1 << 11;
pub const MSR_IA32_APICBASE_BASE: u64 = 0xfffff000;

pub const MSR_IA32_TSC: u32 = 0x00000010;
pub const MSR_IA32_TSC_DEADLINE: u32 = 0x000006e0;

pub const MSR_IA32_MISC_ENABLE: u32 = 0x000001a0;

pub const MSR_IA32_BBL_CR_CTL: u32 = 0x00000119;

pub const MSR_IA32_MC0_CTL: u32 = 0x00000400;
pub const MSR_IA32_MC0_STATUS: u32 = 0x00000401;

pub const MSR_IA32_UMWAIT_CONTROL: u32 = 0xe1;

// ── AMD MSRs ────────────────────────────────────────────────────────────

pub const MSR_K6_EFER: u32 = 0xc0000080;
pub const MSR_K6_STAR: u32 = 0xc0000081;
pub const MSR_K6_WHCR: u32 = 0xc0000082;
pub const MSR_K6_UWCCR: u32 = 0xc0000085;
pub const MSR_K6_PSOR: u32 = 0xc0000086;
pub const MSR_K6_PFIR: u32 = 0xc0000087;

pub const MSR_K7_EVNTSEL0: u32 = 0xc0010000;
pub const MSR_K7_PERFCTR0: u32 = 0xc0010004;

pub const MSR_AMD64_TBAR_MASK: u32 = 0xc000102f;

// ── x2APIC MSRs ─────────────────────────────────────────────────────────

pub const MSR_X2APIC_EOI: u32 = 0x80b;
pub const MSR_X2APIC_SELF_IPI: u32 = 0x83f;

// ── TSC-related MSRs ────────────────────────────────────────────────────

pub const MSR_IA32_TSC_ADJUST: u32 = 0x0000003b;
pub const MSR_TSC_ADJUST: u32 = MSR_IA32_TSC_ADJUST;

// ── MTRR MSRs ───────────────────────────────────────────────────────────

pub const MSR_MTRRcap: u32 = 0x000000fe;
pub const MSR_MTRRdefType: u32 = 0x000002ff;

// ── Architectural memory types ──────────────────────────────────────────

pub const X86_MEMTYPE_UC: u64 = 0;
pub const X86_MEMTYPE_WC: u64 = 1;
pub const X86_MEMTYPE_WT: u64 = 4;
pub const X86_MEMTYPE_WP: u64 = 5;
pub const X86_MEMTYPE_WB: u64 = 6;
pub const X86_MEMTYPE_UC_MINUS: u64 = 7;
