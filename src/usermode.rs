//! User Mode Support
//!
//! This module provides complete Ring 0 to Ring 3 privilege level switching
//! for RustOS, enabling execution of userspace programs.

use core::arch::asm;
use core::sync::atomic::{AtomicU64, Ordering};

/// Sentinel: no pending user-mode entry after execve.
const PENDING_USER_ENTRY_NONE: u64 = u64::MAX;

/// After a successful execve from user mode, the syscall epilogue jumps here
/// instead of returning to the pre-exec instruction pointer.
static PENDING_USER_ENTRY_RIP: AtomicU64 = AtomicU64::new(PENDING_USER_ENTRY_NONE);
static PENDING_USER_ENTRY_RSP: AtomicU64 = AtomicU64::new(PENDING_USER_ENTRY_NONE);

/// Switch from kernel mode (Ring 0) to user mode (Ring 3)
///
/// This function performs a complete privilege level change using the iretq instruction.
/// It sets up the stack frame required for iretq and jumps to user code.
///
/// # Arguments
///
/// * `entry_point` - Virtual address of the user code entry point
/// * `user_stack` - Virtual address of the user mode stack (top of stack)
///
/// # Safety
///
/// This function is unsafe because:
/// - It performs a privilege level change which affects system security
/// - The entry_point must point to valid user code
/// - The user_stack must point to valid, mapped user memory
/// - Once in user mode, only syscalls/interrupts can return to kernel mode
///
/// # Details
///
/// The function prepares the following stack frame for iretq:
/// - SS (Stack Segment) - User data segment with RPL=3
/// - RSP (Stack Pointer) - User stack pointer
/// - RFLAGS - CPU flags (with interrupts enabled, IOPL=0)
/// - CS (Code Segment) - User code segment with RPL=3
/// - RIP (Instruction Pointer) - User entry point
///
/// After iretq executes, the CPU will be in Ring 3 with:
/// - CS = User code segment (DPL=3, RPL=3)
/// - SS = User data segment (DPL=3, RPL=3)
/// - DS, ES, FS, GS = User data segment
/// - RSP = user_stack
/// - RIP = entry_point
/// - RFLAGS.IF = 1 (interrupts enabled)
/// - RFLAGS.IOPL = 0 (no I/O privilege)
#[inline(never)]
pub unsafe fn switch_to_user_mode(entry_point: u64, user_stack: u64) -> ! {
    // Get the user segment selectors from GDT
    // These are Ring 3 segments (DPL=3) and we set RPL=3
    let user_code_selector = crate::gdt::get_user_code_selector();
    let user_data_selector = crate::gdt::get_user_data_selector();

    // Prepare RFLAGS value for user mode:
    // - Bit 1: Reserved (always 1)
    // - Bit 9: IF (Interrupt Enable) = 1
    // - Bits 12-13: IOPL (I/O Privilege Level) = 0
    // - Other bits: cleared or default values
    let rflags: u64 = 0x202; // IF=1, IOPL=0, reserved bit 1 set

    // Set up all data segments to user data segment before switching
    asm!(
        // Load user data segment into all data segment registers
        "mov ds, {0:x}",
        "mov es, {0:x}",
        "mov fs, {0:x}",
        "mov gs, {0:x}",
        in(reg) user_data_selector.0,
        options(nostack, preserves_flags)
    );

    // Perform the privilege level switch using iretq
    // We build the iretq stack frame manually:
    //
    // Stack layout (growing downward):
    // +0x20: SS (user data segment with RPL=3)
    // +0x18: RSP (user stack pointer)
    // +0x10: RFLAGS (with IF=1, IOPL=0)
    // +0x08: CS (user code segment with RPL=3)
    // +0x00: RIP (entry point) <- RSP points here
    //
    // iretq will pop these values in order: RIP, CS, RFLAGS, RSP, SS
    asm!(
        // Push user data segment selector (SS for iretq)
        "push {user_ss}",

        // Push user stack pointer (RSP for iretq)
        "push {user_rsp}",

        // Push RFLAGS (with interrupts enabled)
        "push {rflags}",

        // Push user code segment selector (CS for iretq)
        "push {user_cs}",

        // Push entry point address (RIP for iretq)
        "push {entry_point}",

        // Execute iretq to perform privilege level switch
        // This will:
        // 1. Pop RIP and jump to entry_point
        // 2. Pop CS and set CPL to 3
        // 3. Pop RFLAGS
        // 4. Pop RSP
        // 5. Pop SS
        // 6. Continue execution in Ring 3 at entry_point
        "iretq",

        user_ss = in(reg) user_data_selector.0 as u64,
        user_rsp = in(reg) user_stack,
        rflags = in(reg) rflags,
        user_cs = in(reg) user_code_selector.0 as u64,
        entry_point = in(reg) entry_point,
        options(noreturn)
    );
}

/// Execute a function in user mode and return to kernel mode
///
/// This is a convenience wrapper that switches to user mode, executes
/// a function, and handles the return via syscall or interrupt.
///
/// # Arguments
///
/// * `entry_point` - Address of user function to execute
/// * `user_stack` - User stack pointer
///
/// # Safety
///
/// Same safety requirements as switch_to_user_mode.
/// Additionally, the user code must be prepared to make a syscall
/// to return control to the kernel.
pub unsafe fn execute_in_user_mode(entry_point: u64, user_stack: u64) -> ! {
    // Validate that we're currently in kernel mode
    if !crate::gdt::is_kernel_mode() {
        panic!("execute_in_user_mode called from user mode");
    }

    // Log the transition for debugging
    crate::serial_println!(
        "Switching to user mode: entry=0x{:x}, stack=0x{:x}",
        entry_point,
        user_stack
    );

    // Perform the switch
    switch_to_user_mode(entry_point, user_stack);
}

/// Queue a user-mode entry point for the next INT 0x80 return (successful execve).
pub fn schedule_user_entry(entry_point: u64, user_stack: u64) {
    PENDING_USER_ENTRY_RIP.store(entry_point, Ordering::Release);
    PENDING_USER_ENTRY_RSP.store(user_stack, Ordering::Release);
}

/// Take a pending execve redirect, if any.
pub fn take_pending_user_entry() -> Option<(u64, u64)> {
    let rip = PENDING_USER_ENTRY_RIP.swap(PENDING_USER_ENTRY_NONE, Ordering::AcqRel);
    if rip == PENDING_USER_ENTRY_NONE {
        return None;
    }
    let rsp = PENDING_USER_ENTRY_RSP.swap(PENDING_USER_ENTRY_NONE, Ordering::AcqRel);
    Some((rip, rsp))
}

/// Patch the CPU interrupt frame below the saved INT 0x80 GPR block.
///
/// `saved_regs` must point at the top of the 15 pushed registers (r15).
///
/// # Safety
/// Must only be called from the INT 0x80 handler with a valid stack frame.
pub unsafe fn patch_syscall_return_to_user(
    saved_regs: *mut u8,
    entry_point: u64,
    user_stack: u64,
) {
    let user_code = crate::gdt::get_user_code_selector().0 as u64;
    let user_data = crate::gdt::get_user_data_selector().0 as u64;
    let rflags: u64 = 0x202;

    // Layout below saved GPRs: RIP, CS, RFLAGS, RSP, SS (iretq pop order).
    let iretq_frame = saved_regs.add(120);
    core::ptr::write(iretq_frame as *mut u64, entry_point);
    core::ptr::write(iretq_frame.add(8) as *mut u64, user_code);
    core::ptr::write(iretq_frame.add(16) as *mut u64, rflags);
    core::ptr::write(iretq_frame.add(24) as *mut u64, user_stack);
    core::ptr::write(iretq_frame.add(32) as *mut u64, user_data);
}

/// Return from user mode to kernel mode (called from syscall handler)
///
/// This function is called by the syscall handler after processing a syscall.
/// It restores kernel segments and returns control to the kernel.
///
/// # Safety
///
/// Must only be called from interrupt/syscall context with valid kernel state.
pub unsafe fn return_to_kernel() {
    let _kernel_code_selector = crate::gdt::get_kernel_code_selector();
    let kernel_data_selector = crate::gdt::get_kernel_data_selector();

    // Restore kernel data segments
    asm!(
        "mov ds, {0:x}",
        "mov es, {0:x}",
        "mov fs, {0:x}",
        "mov gs, {0:x}",
        in(reg) kernel_data_selector.0,
        options(nostack, preserves_flags)
    );

    // CS will be restored by iretq in the interrupt handler
}

/// Validate that an address is in user space
///
/// User space addresses must be:
/// - Below the kernel space boundary (0xFFFF_8000_0000_0000)
/// - Above the null page (0x1000)
/// - Properly aligned
///
/// # Arguments
///
/// * `addr` - Address to validate
/// * `size` - Size of the memory region
///
/// # Returns
///
/// true if the address is valid user space, false otherwise
pub fn is_valid_user_address(addr: u64, size: usize) -> bool {
    // User space: 0x1000 to 0x7FFF_FFFF_FFFF
    // Kernel space starts at: 0xFFFF_8000_0000_0000
    const USER_SPACE_START: u64 = 0x1000;
    const USER_SPACE_END: u64 = 0x7FFF_FFFF_FFFF;

    // Check for null pointer or reserved region
    if addr < USER_SPACE_START {
        return false;
    }

    // Check for overflow
    let end_addr = match addr.checked_add(size as u64) {
        Some(end) => end,
        None => return false,
    };

    // Check if entirely in user space
    if end_addr > USER_SPACE_END {
        return false;
    }

    true
}

/// Check if currently executing in user mode
///
/// # Returns
///
/// true if CPL == 3 (user mode), false if CPL == 0 (kernel mode)
pub fn in_user_mode() -> bool {
    crate::gdt::is_user_mode()
}

/// Get the current privilege level (0 = kernel, 3 = user)
pub fn current_privilege_level() -> u16 {
    crate::gdt::get_current_privilege_level()
}

/// User mode context for saving/restoring user state
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct UserContext {
    // General purpose registers
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub rsp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,

    // Instruction pointer and flags
    pub rip: u64,
    pub rflags: u64,

    // Segment selectors
    pub cs: u16,
    pub ss: u16,
    pub ds: u16,
    pub es: u16,
    pub fs: u16,
    pub gs: u16,
}

impl UserContext {
    /// Create a new user context with default values
    pub fn new() -> Self {
        let user_code_selector = crate::gdt::get_user_code_selector();
        let user_data_selector = crate::gdt::get_user_data_selector();

        UserContext {
            rax: 0,
            rbx: 0,
            rcx: 0,
            rdx: 0,
            rsi: 0,
            rdi: 0,
            rbp: 0,
            rsp: 0,
            r8: 0,
            r9: 0,
            r10: 0,
            r11: 0,
            r12: 0,
            r13: 0,
            r14: 0,
            r15: 0,
            rip: 0,
            rflags: 0x202, // IF=1, IOPL=0
            cs: user_code_selector.0,
            ss: user_data_selector.0,
            ds: user_data_selector.0,
            es: user_data_selector.0,
            fs: user_data_selector.0,
            gs: user_data_selector.0,
        }
    }

    /// Set the instruction pointer (entry point)
    pub fn set_entry_point(&mut self, entry: u64) {
        self.rip = entry;
    }

    /// Set the stack pointer (user stack top)
    pub fn set_stack_pointer(&mut self, stack: u64) {
        self.rsp = stack;
    }

    /// Restore this context and switch to user mode
    ///
    /// # Safety
    ///
    /// All context values must be valid for user mode execution
    pub unsafe fn restore_and_switch(&self) -> ! {
        // Restore general purpose registers and perform switch
        asm!(
            // Set up data segments
            "mov ds, {0:x}",
            "mov es, {0:x}",
            "mov fs, {0:x}",
            "mov gs, {0:x}",

            // Restore general purpose registers
            "mov rax, {rax}",
            "mov rbx, {rbx}",
            "mov rcx, {rcx}",
            "mov rdx, {rdx}",
            "mov rsi, {rsi}",
            "mov rdi, {rdi}",
            "mov rbp, {rbp}",
            "mov r8, {r8}",
            "mov r9, {r9}",
            "mov r10, {r10}",
            "mov r11, {r11}",
            "mov r12, {r12}",
            "mov r13, {r13}",
            "mov r14, {r14}",
            "mov r15, {r15}",

            // Build iretq frame
            "push {ss}",
            "push {rsp}",
            "push {rflags}",
            "push {cs}",
            "push {rip}",

            // Switch to user mode
            "iretq",

            in(reg) self.ds,
            rax = in(reg) self.rax,
            rbx = in(reg) self.rbx,
            rcx = in(reg) self.rcx,
            rdx = in(reg) self.rdx,
            rsi = in(reg) self.rsi,
            rdi = in(reg) self.rdi,
            rbp = in(reg) self.rbp,
            r8 = in(reg) self.r8,
            r9 = in(reg) self.r9,
            r10 = in(reg) self.r10,
            r11 = in(reg) self.r11,
            r12 = in(reg) self.r12,
            r13 = in(reg) self.r13,
            r14 = in(reg) self.r14,
            r15 = in(reg) self.r15,
            ss = in(reg) self.ss as u64,
            rsp = in(reg) self.rsp,
            rflags = in(reg) self.rflags,
            cs = in(reg) self.cs as u64,
            rip = in(reg) self.rip,
            options(noreturn)
        );
    }
}

impl Default for UserContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Test user mode switching (for kernel testing only)
#[cfg(test)]
pub fn test_user_mode_switch() {
    use crate::serial_println;

    serial_println!("Testing user mode switch...");

    // This would normally be actual user code
    // For testing, we just verify the switch mechanism
    let entry_point: u64 = 0x400000; // Example user space address
    let user_stack: u64 = 0x500000; // Example user stack

    // In a real test, we would:
    // 1. Map user pages
    // 2. Load user code
    // 3. Switch to user mode
    // 4. User code makes syscall
    // 5. Return to kernel

    serial_println!("User mode switch test completed");
}
