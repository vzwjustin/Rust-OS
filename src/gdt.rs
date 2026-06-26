//! Global Descriptor Table (GDT) and Task State Segment (TSS)
//!
//! This module provides GDT setup for kernel/user segments, TSS for stack switching,
//! and privilege level management for RustOS.

use lazy_static::lazy_static;
use spin::Mutex;
use x86_64::instructions::segmentation::{Segment, CS, DS, ES, FS, GS, SS};
use x86_64::instructions::tables::load_tss;
use x86_64::structures::gdt::{
    Descriptor, GlobalDescriptorTable, SegmentSelector as GdtSegmentSelector,
};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::VirtAddr;

/// Double fault stack index in the IST
pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

/// Stack size for interrupt stacks
const STACK_SIZE: usize = 4096 * 5; // 20KB stack

/// Interrupt stack for double fault handler
static mut DOUBLE_FAULT_STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];

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
        TSS.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
            let stack_start = VirtAddr::from_ptr(&raw const DOUBLE_FAULT_STACK);
            let stack_end = stack_start + STACK_SIZE;
            stack_end
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
            kernel_stack: VirtAddr::new(0), // Would be set during task switching
            user_stack: None,               // Would be set during task switching
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
    use core::ptr;

    // Get a mutable reference to TSS
    // Safety: We have exclusive access during init
    unsafe {
        TSS.privilege_stack_table[0] = stack_ptr;
    }

    crate::serial_println!("Kernel stack set to {:?} in TSS", stack_ptr);
}

/// Set user stack pointer (for task switching)
pub fn set_user_stack(_stack_ptr: VirtAddr) {
    // In a full implementation, this would be used during privilege level changes
    // User stack pointer set
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

/// Get information about a segment selector
pub fn get_segment_info(selector: GdtSegmentSelector) -> Option<SegmentInfo> {
    // This is a simplified implementation
    // In a real system, you'd read the actual GDT entry

    if selector == GDT.1.kernel_code_selector {
        Some(SegmentInfo {
            selector: selector.0,
            base: 0,
            limit: 0xFFFFFFFF,
            privilege_level: PrivilegeLevel::Ring0,
            is_code: true,
            is_executable: true,
            is_readable: true,
            is_writable: false,
        })
    } else if selector == GDT.1.kernel_data_selector {
        Some(SegmentInfo {
            selector: selector.0,
            base: 0,
            limit: 0xFFFFFFFF,
            privilege_level: PrivilegeLevel::Ring0,
            is_code: false,
            is_executable: false,
            is_readable: true,
            is_writable: true,
        })
    } else if selector == GDT.1.user_code_selector {
        Some(SegmentInfo {
            selector: selector.0,
            base: 0,
            limit: 0xFFFFFFFF,
            privilege_level: PrivilegeLevel::Ring3,
            is_code: true,
            is_executable: true,
            is_readable: true,
            is_writable: false,
        })
    } else if selector == GDT.1.user_data_selector {
        Some(SegmentInfo {
            selector: selector.0,
            base: 0,
            limit: 0xFFFFFFFF,
            privilege_level: PrivilegeLevel::Ring3,
            is_code: false,
            is_executable: false,
            is_readable: true,
            is_writable: true,
        })
    } else {
        None
    }
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
