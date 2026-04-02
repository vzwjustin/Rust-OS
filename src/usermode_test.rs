//! User Mode Testing
//!
//! This module provides test functions to validate user/kernel mode switching.

use crate::usermode::{UserContext, is_valid_user_address};
use crate::serial_println;
use core::ptr::{addr_of, addr_of_mut};

/// Allocate a simple user stack for testing
const USER_STACK_SIZE: usize = 4096 * 4; // 16KB stack
static mut USER_STACK: [u8; USER_STACK_SIZE] = [0; USER_STACK_SIZE];

/// Simple test program that executes in user mode
///
/// This program will:
/// 1. Execute a few instructions in user mode
/// 2. Make a syscall (INT 0x80) to return to kernel
///
/// Machine code for:
/// ```asm
/// mov rax, 1          ; syscall number (write)
/// mov rdi, 1          ; fd (stdout)
/// lea rsi, [rip+msg]  ; buffer
/// mov rdx, 13         ; count
/// int 0x80            ; syscall
///
/// mov rax, 60         ; syscall number (exit)
/// xor rdi, rdi        ; status = 0
/// int 0x80            ; syscall
///
/// msg: db "Hello User!\n"
/// ```
#[repr(C, align(4096))]
struct UserTestProgram {
    code: [u8; 4096],
}

static mut USER_TEST_PROGRAM: UserTestProgram = UserTestProgram {
    code: [0; 4096],
};

/// Initialize the test user program
fn init_test_program() {
    let code: &[u8] = &[
        // mov rax, 1 (sys_write)
        0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00,

        // mov rdi, 1 (stdout)
        0x48, 0xc7, 0xc7, 0x01, 0x00, 0x00, 0x00,

        // lea rsi, [rip+25] (message address - relative to next instruction)
        0x48, 0x8d, 0x35, 0x19, 0x00, 0x00, 0x00,

        // mov rdx, 13 (message length)
        0x48, 0xc7, 0xc2, 0x0d, 0x00, 0x00, 0x00,

        // int 0x80 (syscall)
        0xcd, 0x80,

        // mov rax, 60 (sys_exit)
        0x48, 0xc7, 0xc0, 0x3c, 0x00, 0x00, 0x00,

        // xor rdi, rdi (exit code 0)
        0x48, 0x31, 0xff,

        // int 0x80 (syscall)
        0xcd, 0x80,

        // Infinite loop (should never reach here)
        0xeb, 0xfe,

        // Message: "Hello User!\n"
        b'H', b'e', b'l', b'l', b'o', b' ',
        b'U', b's', b'e', b'r', b'!', b'\n', 0x00,
    ];

    unsafe {
        // Copy test program to the user program buffer
        let program = &mut *addr_of_mut!(USER_TEST_PROGRAM);
        program.code[..code.len()].copy_from_slice(code);
    }
}

/// Test basic user mode switching
///
/// This test:
/// 1. Validates privilege level checking
/// 2. Sets up a user context
/// 3. Attempts to switch to user mode (would execute test program)
pub fn test_privilege_levels() {
    serial_println!("\n=== Testing User Mode Support ===");

    // Test 1: Verify we start in kernel mode
    let initial_cpl = crate::gdt::get_current_privilege_level();
    serial_println!("Current Privilege Level: {}", initial_cpl);

    if initial_cpl != 0 {
        serial_println!("[FAIL] Expected Ring 0 (kernel mode)");
        return;
    }
    serial_println!("[OK] Currently in Ring 0 (kernel mode)");

    // Test 2: Verify privilege level detection functions
    if !crate::gdt::is_kernel_mode() {
        serial_println!("[FAIL] is_kernel_mode() returned false in kernel mode");
        return;
    }
    serial_println!("[OK] is_kernel_mode() works correctly");

    if crate::gdt::is_user_mode() {
        serial_println!("[FAIL] is_user_mode() returned true in kernel mode");
        return;
    }
    serial_println!("[OK] is_user_mode() works correctly");

    // Test 3: Verify segment selectors
    let kernel_cs = crate::gdt::get_kernel_code_selector();
    let kernel_ds = crate::gdt::get_kernel_data_selector();
    let user_cs = crate::gdt::get_user_code_selector();
    let user_ds = crate::gdt::get_user_data_selector();

    serial_println!("Kernel CS: 0x{:x}, DS: 0x{:x}", kernel_cs.0, kernel_ds.0);
    serial_println!("User CS: 0x{:x}, DS: 0x{:x}", user_cs.0, user_ds.0);

    // Verify user segments have RPL=3
    if (user_cs.0 & 3) != 3 {
        serial_println!("[FAIL] User code segment doesn't have RPL=3");
        return;
    }
    serial_println!("[OK] User code segment has correct RPL");

    if (user_ds.0 & 3) != 3 {
        serial_println!("[FAIL] User data segment doesn't have RPL=3");
        return;
    }
    serial_println!("[OK] User data segment has correct RPL");

    // Test 4: Test address validation
    test_address_validation();

    // Test 5: Test user context creation
    test_user_context();

    serial_println!("\n=== User Mode Support Tests Complete ===");
}

/// Test user space address validation
fn test_address_validation() {
    serial_println!("\n--- Testing Address Validation ---");

    // Test valid user addresses
    if !is_valid_user_address(0x1000, 4096) {
        serial_println!("[FAIL] Valid user address rejected: 0x1000");
        return;
    }
    serial_println!("[OK] Valid user address accepted: 0x1000");

    if !is_valid_user_address(0x400000, 4096) {
        serial_println!("[FAIL] Valid user address rejected: 0x400000");
        return;
    }
    serial_println!("[OK] Valid user address accepted: 0x400000");

    // Test invalid user addresses (null page)
    if is_valid_user_address(0, 4096) {
        serial_println!("[FAIL] Null page address accepted");
        return;
    }
    serial_println!("[OK] Null page address rejected");

    if is_valid_user_address(0x500, 4096) {
        serial_println!("[FAIL] Low memory address accepted");
        return;
    }
    serial_println!("[OK] Low memory address rejected");

    // Test kernel space addresses
    if is_valid_user_address(0xFFFF_8000_0000_0000, 4096) {
        serial_println!("[FAIL] Kernel space address accepted");
        return;
    }
    serial_println!("[OK] Kernel space address rejected");

    // Test overflow
    if is_valid_user_address(0x7FFF_FFFF_FFFF, 4096) {
        serial_println!("[FAIL] Overflowing address accepted");
        return;
    }
    serial_println!("[OK] Overflowing address rejected");
}

/// Test user context creation and setup
fn test_user_context() {
    serial_println!("\n--- Testing User Context ---");

    // Create a new user context
    let mut context = UserContext::new();

    // Verify initial state
    if context.rflags != 0x202 {
        serial_println!("[FAIL] Initial RFLAGS incorrect: 0x{:x}", context.rflags);
        return;
    }
    serial_println!("[OK] Initial RFLAGS correct (IF=1, IOPL=0)");

    // Verify segment selectors are user mode
    let user_cs = crate::gdt::get_user_code_selector().0;
    let user_ds = crate::gdt::get_user_data_selector().0;

    if context.cs != user_cs {
        serial_println!("[FAIL] User context CS incorrect");
        return;
    }
    serial_println!("[OK] User context CS set correctly");

    if context.ss != user_ds || context.ds != user_ds {
        serial_println!("[FAIL] User context data segments incorrect");
        return;
    }
    serial_println!("[OK] User context data segments set correctly");

    // Set entry point and stack
    let entry_point = 0x400000u64;
    let stack_top = 0x500000u64;

    context.set_entry_point(entry_point);
    context.set_stack_pointer(stack_top);

    if context.rip != entry_point {
        serial_println!("[FAIL] Entry point not set correctly");
        return;
    }
    serial_println!("[OK] Entry point set correctly: 0x{:x}", context.rip);

    if context.rsp != stack_top {
        serial_println!("[FAIL] Stack pointer not set correctly");
        return;
    }
    serial_println!("[OK] Stack pointer set correctly: 0x{:x}", context.rsp);
}

/// Demonstrate user mode switch preparation (without actually switching)
///
/// This shows how to set up a user mode execution environment.
/// Actual switching would require proper memory mapping and executable user code.
pub fn demonstrate_user_mode_setup() {
    serial_println!("\n=== Demonstrating User Mode Setup ===");

    // Initialize test program
    init_test_program();
    serial_println!("[OK] Test user program initialized");

    // Get addresses
    let code_addr = unsafe { (*addr_of!(USER_TEST_PROGRAM)).code.as_ptr() as u64 };
    let stack_addr = unsafe { addr_of!(USER_STACK) as *const u8 as u64 + USER_STACK_SIZE as u64 };

    serial_println!("User code would be at: 0x{:x}", code_addr);
    serial_println!("User stack would be at: 0x{:x}", stack_addr);

    // Create user context
    let mut context = UserContext::new();
    context.set_entry_point(code_addr);
    context.set_stack_pointer(stack_addr);

    serial_println!("[OK] User context configured");
    serial_println!("    Entry point: 0x{:x}", context.rip);
    serial_println!("    Stack: 0x{:x}", context.rsp);
    serial_println!("    CS: 0x{:x} (RPL={})", context.cs, context.cs & 3);
    serial_println!("    SS: 0x{:x} (RPL={})", context.ss, context.ss & 3);

    serial_println!("\nNote: Actual switch requires:");
    serial_println!("  1. User pages must be mapped in page tables");
    serial_println!("  2. Code must be at a valid user space address");
    serial_println!("  3. Kernel stack must be set in TSS.RSP0");
    serial_println!("  4. User code should make syscalls to return to kernel");

    // In a real system, we would now do:
    // unsafe { context.restore_and_switch(); }
    // But this would only work if memory is properly set up
}

/// Run all user mode tests
pub fn run_all_tests() {
    test_privilege_levels();
    demonstrate_user_mode_setup();
}
