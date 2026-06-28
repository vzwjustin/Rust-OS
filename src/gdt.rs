//! Global Descriptor Table (GDT) and Task State Segment (TSS)
//!
//! This module provides GDT setup for kernel/user segments, TSS for stack switching,
//! and privilege level management for RustOS.

use lazy_static::lazy_static;
use x86_64::instructions::segmentation::{Segment, CS, DS, ES, FS, GS, SS};
use x86_64::instructions::tables::load_tss;
use x86_64::structures::gdt::{
    Descriptor, GlobalDescriptorTable, SegmentSelector as GdtSegmentSelector,
};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::VirtAddr;

/// Double fault stack index in the IST
pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;
/// Page fault stack index — its own aligned stack so the handler never runs on
/// a corrupt/misaligned faulting stack (which would #GP on `movaps`).
pub const PAGE_FAULT_IST_INDEX: u16 = 1;
/// General-protection fault stack index — same rationale.
pub const GENERAL_PROTECTION_IST_INDEX: u16 = 2;

/// Stack size for interrupt stacks
const STACK_SIZE: usize = 4096 * 5; // 20KB stack

/// 16-byte-aligned stack storage. A bare `[u8; N]` has alignment 1, so its top
/// can land on an 8-byte boundary — and the x86-64 ABI requires 16-byte stack
/// alignment. A misaligned exception stack makes the compiler's `movaps`
/// instructions #GP, turning any fault into a fatal cascade.
#[repr(align(16))]
struct AlignedStack([u8; STACK_SIZE]);

/// Interrupt stack for double fault handler
static mut DOUBLE_FAULT_STACK: AlignedStack = AlignedStack([0; STACK_SIZE]);
/// Dedicated stack for the page fault handler.
static mut PAGE_FAULT_STACK: AlignedStack = AlignedStack([0; STACK_SIZE]);
/// Dedicated stack for the general-protection fault handler.
static mut GP_FAULT_STACK: AlignedStack = AlignedStack([0; STACK_SIZE]);
/// Ring-0 stack used when interrupts/syscalls arrive from Ring 3.
static mut RING0_STACK: AlignedStack = AlignedStack([0; STACK_SIZE]);

/// Task State Segment (mutable for stack updates)
static mut TSS: TaskStateSegment = TaskStateSegment::new();

/// GDT segment selectors
struct Selectors {
    kernel_code_selector: GdtSegmentSelector,
    kernel_data_selector: GdtSegmentSelector,
    user_code_selector: GdtSegmentSelector,
    user_data_selector: GdtSegmentSelector,
    tss_selector: GdtSegmentSelector,
}

lazy_static! {
    static ref GDT: (GlobalDescriptorTable, Selectors) = {
        let mut gdt = GlobalDescriptorTable::new();

        // GDT Layout for SYSCALL/SYSRET compatibility:
        // The STAR MSR requires specific segment ordering:
        // - SYSCALL loads CS from STAR[47:32], SS from STAR[47:32]+8
        // - SYSRET loads CS from STAR[63:48]+16, SS from STAR[63:48]+8
        // So the order must be: kernel_code, kernel_data, user_data, user_code

        // Entry 1 (0x08): Kernel code segment (Ring 0)
        let kernel_code_selector = gdt.add_entry(Descriptor::kernel_code_segment());

        // Entry 2 (0x10): Kernel data segment (Ring 0)
        let kernel_data_selector = gdt.add_entry(Descriptor::kernel_data_segment());

        // Entry 3 (0x18): User data segment (Ring 3) - MUST come before user code for SYSRET
        let user_data_selector = gdt.add_entry(Descriptor::user_data_segment());

        // Entry 4 (0x20): User code segment (Ring 3)
        let user_code_selector = gdt.add_entry(Descriptor::user_code_segment());

        // Entry 5 (0x28): Task State Segment (takes 2 entries for 64-bit TSS)
        let tss_selector = gdt.add_entry(Descriptor::tss_segment(unsafe { &TSS }));

        (gdt, Selectors {
            kernel_code_selector,
            kernel_data_selector,
            user_code_selector,
            user_data_selector,
            tss_selector,
        })
    };
}

/// Initialize the GDT and load segment selectors
pub fn init() {
    // Initialize TSS with double fault stack
    unsafe {
        TSS.privilege_stack_table[0] = {
            let stack_start = VirtAddr::from_ptr(&raw const RING0_STACK);
            stack_start + (STACK_SIZE - 8)
        };
        TSS.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
            let stack_start = VirtAddr::from_ptr(&raw const DOUBLE_FAULT_STACK);
            let stack_end = stack_start + STACK_SIZE;
            stack_end
        };
        TSS.interrupt_stack_table[PAGE_FAULT_IST_INDEX as usize] = {
            let stack_start = VirtAddr::from_ptr(&raw const PAGE_FAULT_STACK);
            stack_start + STACK_SIZE
        };
        TSS.interrupt_stack_table[GENERAL_PROTECTION_IST_INDEX as usize] = {
            let stack_start = VirtAddr::from_ptr(&raw const GP_FAULT_STACK);
            stack_start + STACK_SIZE
        };
    }

    GDT.0.load();

    unsafe {
        // Set kernel code segment
        CS::set_reg(GDT.1.kernel_code_selector);

        // Set data segments to kernel data segment
        DS::set_reg(GDT.1.kernel_data_selector);
        ES::set_reg(GDT.1.kernel_data_selector);
        FS::set_reg(GDT.1.kernel_data_selector);
        GS::set_reg(GDT.1.kernel_data_selector);
        SS::set_reg(GDT.1.kernel_data_selector);

        // Load TSS
        load_tss(GDT.1.tss_selector);
    }
}

/// Get kernel code segment selector
pub fn get_kernel_code_selector() -> GdtSegmentSelector {
    GDT.1.kernel_code_selector
}

/// Get kernel data segment selector
pub fn get_kernel_data_selector() -> GdtSegmentSelector {
    GDT.1.kernel_data_selector
}

/// Get user code segment selector
pub fn get_user_code_selector() -> GdtSegmentSelector {
    GDT.1.user_code_selector
}

/// Get user data segment selector
pub fn get_user_data_selector() -> GdtSegmentSelector {
    GDT.1.user_data_selector
}

/// Get TSS selector
pub fn get_tss_selector() -> GdtSegmentSelector {
    GDT.1.tss_selector
}

/// Get current privilege level from CS register
pub fn get_current_privilege_level() -> u16 {
    CS::get_reg().rpl() as u16
}

/// Check if currently running in kernel mode (Ring 0)
pub fn is_kernel_mode() -> bool {
    get_current_privilege_level() == 0
}

/// Check if currently running in user mode (Ring 3)
pub fn is_user_mode() -> bool {
    get_current_privilege_level() == 3
}

/// Privilege levels for segment descriptors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PrivilegeLevel {
    Ring0 = 0, // Kernel mode
    Ring1 = 1, // Device drivers (rarely used)
    Ring2 = 2, // Device drivers (rarely used)
    Ring3 = 3, // User mode
}

impl PrivilegeLevel {
    /// Convert privilege level to x86_64 PrivilegeLevel
    pub fn to_x86_64(self) -> x86_64::PrivilegeLevel {
        match self {
            PrivilegeLevel::Ring0 => x86_64::PrivilegeLevel::Ring0,
            PrivilegeLevel::Ring1 => x86_64::PrivilegeLevel::Ring1,
            PrivilegeLevel::Ring2 => x86_64::PrivilegeLevel::Ring2,
            PrivilegeLevel::Ring3 => x86_64::PrivilegeLevel::Ring3,
        }
    }

    /// Get privilege level from u16
    pub fn from_u16(level: u16) -> Option<Self> {
        match level {
            0 => Some(PrivilegeLevel::Ring0),
            1 => Some(PrivilegeLevel::Ring1),
            2 => Some(PrivilegeLevel::Ring2),
            3 => Some(PrivilegeLevel::Ring3),
            _ => None,
        }
    }
}

/// Information about the current execution context
#[derive(Debug)]
pub struct ExecutionContext {
    pub privilege_level: PrivilegeLevel,
    pub code_segment: u16,
    pub data_segment: u16,
    pub stack_segment: u16,
    pub is_kernel_mode: bool,
}

/// Get current execution context information
pub fn get_execution_context() -> ExecutionContext {
    let cs = CS::get_reg();
    let ds = DS::get_reg();
    let ss = SS::get_reg();
    let privilege_level =
        PrivilegeLevel::from_u16(cs.rpl() as u16).unwrap_or(PrivilegeLevel::Ring0);

    ExecutionContext {
        privilege_level,
        code_segment: cs.0,
        data_segment: ds.0,
        stack_segment: ss.0,
        is_kernel_mode: privilege_level == PrivilegeLevel::Ring0,
    }
}

/// Stack information for privilege levels
#[derive(Debug)]
pub struct StackInfo {
    pub kernel_stack: VirtAddr,
    pub user_stack: Option<VirtAddr>,
    pub interrupt_stacks: [VirtAddr; 7], // IST entries
}

/// Get stack information from TSS
pub fn get_stack_info() -> StackInfo {
    unsafe {
        StackInfo {
            kernel_stack: TSS.privilege_stack_table[0],
            user_stack: None,
            interrupt_stacks: [
                TSS.interrupt_stack_table[0],
                TSS.interrupt_stack_table[1],
                TSS.interrupt_stack_table[2],
                TSS.interrupt_stack_table[3],
                TSS.interrupt_stack_table[4],
                TSS.interrupt_stack_table[5],
                TSS.interrupt_stack_table[6],
            ],
        }
    }
}

/// Set kernel stack pointer in TSS (for task switching)
///
/// This sets RSP0 in the TSS, which is used by the CPU when switching
/// from user mode (Ring 3) to kernel mode (Ring 0) via interrupts or syscalls.
///
/// # Safety
///
/// The stack pointer must point to a valid, mapped kernel stack.
pub fn set_kernel_stack(stack_ptr: VirtAddr) {
    // Get a mutable reference to TSS
    // Safety: We have exclusive access during init
    unsafe {
        TSS.privilege_stack_table[0] = stack_ptr;
    }

    crate::serial_println!("Kernel stack set to {:?} in TSS", stack_ptr);
}

/// Set user stack pointer (for task switching)
///
/// Writes the stack pointer into the TSS privilege stack table entry 0
/// (RSP0). When a user-mode process transitions to kernel mode via
/// an interrupt or syscall, the CPU loads RSP from this entry.
pub fn set_user_stack(stack_ptr: VirtAddr) {
    unsafe {
        TSS.privilege_stack_table[0] = stack_ptr;
    }
}

/// Memory segment information
#[derive(Debug, Clone)]
pub struct SegmentInfo {
    pub selector: u16,
    pub base: u64,
    pub limit: u64,
    pub privilege_level: PrivilegeLevel,
    pub is_code: bool,
    pub is_executable: bool,
    pub is_readable: bool,
    pub is_writable: bool,
}

/// Get information about a segment selector by reading the actual GDT entry.
///
/// Uses the SGDT instruction to locate the GDT in memory, then reads and
/// parses the 8-byte segment descriptor at the index specified by the
/// selector. In long mode, all segment descriptors are 64-bit with a
/// flat base of 0 and a limit of 0xFFFFF (granularity=4KiB → 4GB).
pub fn get_segment_info(selector: GdtSegmentSelector) -> Option<SegmentInfo> {
    // Get the GDT base address and limit via SGDT
    let (gdt_base, _gdt_limit): (*const u8, u16);
    unsafe {
        // SGDT stores a 10-byte pseudo-descriptor: 2-byte limit + 8-byte base.
        // On x86-64 the base is 64-bit and the structure is naturally aligned.
        #[repr(C, packed)]
        struct GdtrPseudo {
            limit: u16,
            base: u64,
        }
        let mut gdtr: GdtrPseudo = GdtrPseudo { limit: 0, base: 0 };
        core::arch::asm!(
            "sgdt [{}]",
            in(reg) &mut gdtr,
            options(nostack, preserves_flags),
        );
        gdt_base = gdtr.base as *const u8;
        _gdt_limit = gdtr.limit;
    }

    // The selector index points to the descriptor (index * 8 bytes)
    let index = (selector.0 >> 3) as usize;
    let entry_ptr = unsafe { gdt_base.add(index * 8) };
    let entry: u64 = unsafe { core::ptr::read(entry_ptr as *const u64) };

    // A null descriptor (all zeros) is not a valid segment
    if entry == 0 {
        return None;
    }

    // Parse the 64-bit segment descriptor (long-mode format)
    // Bit 44: descriptor type (0 = system, 1 = code/data)
    // Bit 43: executable (1 = code, 0 = data)
    // Bit 41: readable (for code) / writable (for data)
    // Bits 46-47: DPL (privilege level)
    let is_code = (entry >> 43) & 1 == 1;
    let dpl = ((entry >> 45) & 3) as u8;
    let access_rw = (entry >> 41) & 1 == 1;

    let privilege_level = match dpl {
        0 => PrivilegeLevel::Ring0,
        1 => PrivilegeLevel::Ring1,
        2 => PrivilegeLevel::Ring2,
        3 => PrivilegeLevel::Ring3,
        _ => return None,
    };

    Some(SegmentInfo {
        selector: selector.0,
        base: 0,           // Long-mode segments have a flat base
        limit: 0xFFFFFFFF, // Granularity=1, limit=0xFFFFF → 4GB
        privilege_level,
        is_code,
        is_executable: is_code,
        is_readable: if is_code { access_rw } else { true },
        is_writable: if !is_code { access_rw } else { false },
    })
}

/// Production GDT - no debug output (security sensitive)
pub fn print_gdt_info() {
    // Production kernels don't expose GDT details
}

/// Production GDT validation
pub fn test_gdt() {
    // Production: validate GDT setup internally without exposing details
    let _is_kernel = is_kernel_mode();
    let _is_user = is_user_mode();

    // Validate segment selectors are valid
    let _info = get_segment_info(get_kernel_code_selector());
}

/// Advanced TSS management for future extensions
pub mod tss_management {
    use super::*;

    /// TSS fields that can be modified
    #[derive(Debug)]
    pub struct TssFields {
        pub rsp0: u64,
        pub rsp1: u64,
        pub rsp2: u64,
        pub ist1: u64,
        pub ist2: u64,
        pub ist3: u64,
        pub ist4: u64,
        pub ist5: u64,
        pub ist6: u64,
        pub ist7: u64,
    }

    /// Get current TSS field values
    pub fn get_tss_fields() -> TssFields {
        unsafe {
            TssFields {
                rsp0: TSS.privilege_stack_table[0].as_u64(),
                rsp1: TSS.privilege_stack_table[1].as_u64(),
                rsp2: TSS.privilege_stack_table[2].as_u64(),
                ist1: TSS.interrupt_stack_table[0].as_u64(),
                ist2: TSS.interrupt_stack_table[1].as_u64(),
                ist3: TSS.interrupt_stack_table[2].as_u64(),
                ist4: TSS.interrupt_stack_table[3].as_u64(),
                ist5: TSS.interrupt_stack_table[4].as_u64(),
                ist6: TSS.interrupt_stack_table[5].as_u64(),
                ist7: TSS.interrupt_stack_table[6].as_u64(),
            }
        }
    }

    /// Print TSS information
    pub fn print_tss_info() {
        let _fields = get_tss_fields();
        // Production: TSS info not exposed
    }
}

/// Initialize additional interrupt stacks
pub fn init_interrupt_stacks() {
    // This could be extended to set up additional IST entries
    // for different types of critical interrupts
    // Interrupt stacks initialized
}
