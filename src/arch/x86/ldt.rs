//! x86 LDT (Local Descriptor Table) structures.
//!
//! Ported from Linux `arch/x86/include/uapi/asm/ldt.h`.

/// Maximum number of LDT entries supported.
pub const LDT_ENTRIES: usize = 8192;
/// The size of each LDT entry.
pub const LDT_ENTRY_SIZE: usize = 8;

/// `struct user_desc` — LDT entry descriptor passed to `modify_ldt`.
///
/// On 64-bit, base and limit is ignored for DS/ES/CS. This call is
/// primarily for 32-bit mode compatibility.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct UserDesc {
    pub entry_number: u32,
    pub base_addr: u32,
    pub limit: u32,
    pub seg_32bit: u32,
    pub contents: u32,
    pub read_exec_only: u32,
    pub limit_in_pages: u32,
    pub seg_not_present: u32,
    pub useable: u32,
    pub lm: u32,
}

// ── modify_ldt contents types ───────────────────────────────────────────

pub const MODIFY_LDT_CONTENTS_DATA: u32 = 0;
pub const MODIFY_LDT_CONTENTS_STACK: u32 = 1;
pub const MODIFY_LDT_CONTENTS_CODE: u32 = 2;

// ── GDT entry TLS constants (asm/segment.h) ─────────────────────────────

pub const GDT_ENTRY_TLS_MIN: u16 = 14;
pub const GDT_ENTRY_TLS_MAX: u16 = 16;
pub const GDT_ENTRY_TLS_ENTRIES: usize = 3;
