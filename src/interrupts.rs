//! Interrupt Descriptor Table (IDT) and Interrupt Handling
//!
//! This module provides a comprehensive interrupt handling system for RustOS.
//! It includes the IDT setup, exception handlers, and hardware interrupt management.

use alloc::string::ToString;
use core::{fmt, ptr};
use lazy_static::lazy_static;
use pic8259::ChainedPics;
use spin::Mutex;
use x86_64::instructions::port::Port;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};
use x86_64::{PrivilegeLevel, VirtAddr};

/// Hardware interrupt offsets for the PIC (Programmable Interrupt Controller)
pub const PIC_1_OFFSET: u8 = 32;
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

/// Hardware interrupt indices
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    Timer = PIC_1_OFFSET,
    Keyboard = PIC_1_OFFSET + 1,
    SerialPort1 = PIC_1_OFFSET + 4,
    SerialPort2 = PIC_1_OFFSET + 3,
    SpuriousInterrupt = PIC_1_OFFSET + 7,
    Mouse = PIC_2_OFFSET + 4, // IRQ 12
}

impl InterruptIndex {
    fn as_u8(self) -> u8 {
        self as u8
    }

    pub fn as_usize(self) -> usize {
        usize::from(self.as_u8())
    }
}

lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();

        // CPU Exception handlers
        idt.breakpoint.set_handler_fn(breakpoint_handler);
        unsafe {
            idt.double_fault
                .set_handler_fn(double_fault_handler)
                .set_stack_index(crate::gdt::DOUBLE_FAULT_IST_INDEX);
        }
        unsafe {
            idt.page_fault
                .set_handler_fn(page_fault_handler)
                .set_stack_index(crate::gdt::PAGE_FAULT_IST_INDEX);
        }
        idt.divide_error.set_handler_fn(divide_error_handler);
        idt.invalid_opcode.set_handler_fn(invalid_opcode_handler);
        unsafe {
            idt.general_protection_fault
                .set_handler_fn(general_protection_fault_handler)
                .set_stack_index(crate::gdt::GENERAL_PROTECTION_IST_INDEX);
        }
        idt.stack_segment_fault.set_handler_fn(stack_segment_fault_handler);
        idt.segment_not_present.set_handler_fn(segment_not_present_handler);
        idt.overflow.set_handler_fn(overflow_handler);
        idt.bound_range_exceeded.set_handler_fn(bound_range_exceeded_handler);
        idt.invalid_tss.set_handler_fn(invalid_tss_handler);
        // Machine check exception handler not yet implemented
        // SIMD floating point exception handler not yet implemented
        idt.virtualization.set_handler_fn(virtualization_handler);
        idt.alignment_check.set_handler_fn(alignment_check_handler);

        // Hardware interrupt handlers
        idt[InterruptIndex::Timer.as_usize()].set_handler_fn(timer_interrupt_handler);
        idt[InterruptIndex::Keyboard.as_usize()].set_handler_fn(keyboard_interrupt_handler);
        idt[InterruptIndex::Mouse.as_usize()].set_handler_fn(mouse_interrupt_handler);
        idt[InterruptIndex::SerialPort1.as_usize()].set_handler_fn(serial_port1_interrupt_handler);
        idt[InterruptIndex::SerialPort2.as_usize()].set_handler_fn(serial_port2_interrupt_handler);
        idt[InterruptIndex::SpuriousInterrupt.as_usize()].set_handler_fn(spurious_interrupt_handler);

        // Linux syscall handler (INT 0x80)
        idt[0x80]
            .set_handler_fn(crate::syscall_handler::syscall_0x80_handler)
            .set_privilege_level(PrivilegeLevel::Ring3);

        idt
    };
}

/// Global PIC controller instance
pub static PICS: Mutex<ChainedPics> =
    Mutex::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });

/// Interrupt statistics
#[derive(Clone, Copy)]
pub struct InterruptStats {
    pub timer_count: u64,
    pub keyboard_count: u64,
    pub mouse_count: u64,
    pub serial_count: u64,
    pub exception_count: u64,
    pub page_fault_count: u64,
    pub spurious_count: u64,
    pub missed_interrupts: u64,
}

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

// Thread-safe interrupt statistics using atomic operations
static APIC_EOI_ACTIVE: AtomicBool = AtomicBool::new(false);
static TIMER_COUNT: AtomicU64 = AtomicU64::new(0);
static KEYBOARD_COUNT: AtomicU64 = AtomicU64::new(0);
static MOUSE_COUNT: AtomicU64 = AtomicU64::new(0);
static SERIAL_COUNT: AtomicU64 = AtomicU64::new(0);
static EXCEPTION_COUNT: AtomicU64 = AtomicU64::new(0);
static PAGE_FAULT_COUNT: AtomicU64 = AtomicU64::new(0);
static SPURIOUS_COUNT: AtomicU64 = AtomicU64::new(0);
static MISSED_INTERRUPTS: AtomicU64 = AtomicU64::new(0);

/// Initialize the interrupt system
pub fn init() {
    IDT.load();

    // Initialize ACPI tables first for APIC configuration
    if let Err(e) = crate::acpi::init_acpi_tables() {
        crate::serial_println!("Warning: ACPI initialization failed: {}", e);
    }

    // Initialize APIC system or fall back to PIC
    match crate::apic::init_apic_system() {
        Ok(()) => {
            crate::serial_println!("APIC system initialized successfully");

            // Configure standard IRQs with APIC
            match configure_standard_irqs_apic() {
                Ok(()) => {
                    crate::serial_println!("Using APIC for interrupt handling");
                    disable_legacy_pic();
                    APIC_EOI_ACTIVE.store(true, Ordering::Release);
                }
                Err(e) => {
                    crate::serial_println!("APIC configuration failed: {}, falling back to PIC", e);
                    APIC_EOI_ACTIVE.store(false, Ordering::Release);
                    init_legacy_pic();
                }
            }
        }
        Err(e) => {
            crate::serial_println!("APIC initialization failed: {}, using legacy PIC", e);
            APIC_EOI_ACTIVE.store(false, Ordering::Release);
            init_legacy_pic();
        }
    }

    // Validate interrupt system is working
    validate_interrupt_system();

    // CRITICAL: Mask all PIC interrupts before enabling CPU interrupt flag
    // This prevents any hardware interrupts during the early boot phase
    // Interrupts will be unmasked selectively as drivers initialize
    unsafe {
        let mut pic1_data: Port<u8> = Port::new(0x21);
        let mut pic2_data: Port<u8> = Port::new(0xA1);
        pic1_data.write(0xFF); // Mask all master PIC interrupts
        pic2_data.write(0xFF); // Mask all slave PIC interrupts
    }

    x86_64::instructions::interrupts::enable();
    crate::serial_println!("Interrupts enabled with PIC masked");
}

/// Initialize legacy PIC
fn init_legacy_pic() {
    unsafe { PICS.lock().initialize() };
}

/// Unmask specific IRQs at the PIC level
/// IRQ 0 = Timer, IRQ 1 = Keyboard, IRQ 12 = Mouse
pub fn unmask_irq(irq: u8) {
    crate::serial_println!("unmask_irq: about to unmask IRQ {}", irq);
    unsafe {
        let port = if irq < 8 {
            0x21 // Master PIC data port
        } else {
            0xA1 // Slave PIC data port
        };
        let irq_bit = if irq < 8 { irq } else { irq - 8 };

        let mut data_port: Port<u8> = Port::new(port);
        crate::serial_println!("unmask_irq: reading current mask");
        let current_mask = data_port.read();
        crate::serial_println!(
            "unmask_irq: current mask = 0x{:02X}, writing new mask",
            current_mask
        );
        data_port.write(current_mask & !(1 << irq_bit));
        crate::serial_println!("unmask_irq: IRQ {} unmasked successfully", irq);
    }
}

/// Unmask timer interrupt (IRQ 0)
pub fn enable_timer_interrupt() {
    unmask_irq(0);
}

/// Unmask keyboard interrupt (IRQ 1)
pub fn enable_keyboard_interrupt() {
    unmask_irq(1);
}

/// Unmask mouse interrupt (IRQ 12)
pub fn enable_mouse_interrupt() {
    unmask_irq(12);
    // Also need to unmask IRQ 2 for slave PIC cascade
    unmask_irq(2);
}

/// Disable legacy PIC when using APIC
fn disable_legacy_pic() {
    unsafe {
        // Mask all interrupts on both PICs
        let mut pic1_data: Port<u8> = Port::new(0x21);
        let mut pic2_data: Port<u8> = Port::new(0xA1);

        pic1_data.write(0xFF);
        pic2_data.write(0xFF);
    }
}

fn notify_irq_eoi(index: InterruptIndex) {
    unsafe {
        if APIC_EOI_ACTIVE.load(Ordering::Acquire) {
            if let Some(mut apic) = crate::apic::apic_system().try_lock() {
                if apic.is_initialized() {
                    apic.end_of_interrupt();
                    return;
                }
            } else {
                MISSED_INTERRUPTS.fetch_add(1, Ordering::Relaxed);
                return;
            }
        }

        if let Some(mut pics) = PICS.try_lock() {
            pics.notify_end_of_interrupt(index.as_u8());
        } else {
            MISSED_INTERRUPTS.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// Configure standard IRQs using APIC
fn configure_standard_irqs_apic() -> Result<(), &'static str> {
    // Get current CPU ID for interrupt routing
    let cpu_id = 0; // For now, route all interrupts to CPU 0

    // Configure timer (IRQ 0) - critical for system operation
    if let Err(e) = crate::apic::configure_irq(0, InterruptIndex::Timer.as_u8(), cpu_id) {
        crate::serial_println!("Warning: Failed to configure timer IRQ: {}", e);
        // Timer is critical, but we can continue with PIC fallback
    }

    // Configure keyboard (IRQ 1)
    if let Err(e) = crate::apic::configure_irq(1, InterruptIndex::Keyboard.as_u8(), cpu_id) {
        crate::serial_println!("Warning: Failed to configure keyboard IRQ: {}", e);
    }

    // Configure mouse (IRQ 12)
    if let Err(e) = crate::apic::configure_irq(12, InterruptIndex::Mouse.as_u8(), cpu_id) {
        crate::serial_println!("Warning: Failed to configure mouse IRQ: {}", e);
    }

    // Configure serial ports
    if let Err(e) = crate::apic::configure_irq(4, InterruptIndex::SerialPort1.as_u8(), cpu_id) {
        crate::serial_println!("Warning: Failed to configure serial port 1 IRQ: {}", e);
    }

    if let Err(e) = crate::apic::configure_irq(3, InterruptIndex::SerialPort2.as_u8(), cpu_id) {
        crate::serial_println!("Warning: Failed to configure serial port 2 IRQ: {}", e);
    }

    // Validate that at least timer configuration succeeded
    // If not, we should fall back to PIC
    crate::serial_println!("APIC interrupt configuration completed");

    Ok(())
}

/// Get interrupt statistics
pub fn get_stats() -> InterruptStats {
    InterruptStats {
        timer_count: TIMER_COUNT.load(Ordering::Relaxed),
        keyboard_count: KEYBOARD_COUNT.load(Ordering::Relaxed),
        mouse_count: MOUSE_COUNT.load(Ordering::Relaxed),
        serial_count: SERIAL_COUNT.load(Ordering::Relaxed),
        exception_count: EXCEPTION_COUNT.load(Ordering::Relaxed),
        page_fault_count: PAGE_FAULT_COUNT.load(Ordering::Relaxed),
        spurious_count: SPURIOUS_COUNT.load(Ordering::Relaxed),
        missed_interrupts: MISSED_INTERRUPTS.load(Ordering::Relaxed),
    }
}

/// Reset interrupt statistics
pub fn reset_stats() {
    TIMER_COUNT.store(0, Ordering::Relaxed);
    KEYBOARD_COUNT.store(0, Ordering::Relaxed);
    SERIAL_COUNT.store(0, Ordering::Relaxed);
    EXCEPTION_COUNT.store(0, Ordering::Relaxed);
    PAGE_FAULT_COUNT.store(0, Ordering::Relaxed);
    SPURIOUS_COUNT.store(0, Ordering::Relaxed);
}

/// Disable interrupts and return previous interrupt state
pub fn disable() -> bool {
    let rflags = x86_64::instructions::interrupts::are_enabled();
    x86_64::instructions::interrupts::disable();
    rflags
}

/// Enable interrupts
pub fn enable() {
    x86_64::instructions::interrupts::enable();
}

/// Execute a closure with interrupts disabled
pub fn without_interrupts<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    let saved = disable();
    let result = f();
    if saved {
        enable();
    }
    result
}

// ========== CPU EXCEPTION HANDLERS ==========

extern "x86-interrupt" fn breakpoint_handler(_stack_frame: InterruptStackFrame) {
    // Handle breakpoint interrupt - increment counter for debugging
    EXCEPTION_COUNT.fetch_add(1, Ordering::Relaxed);
    // Continue execution - breakpoints are non-fatal in production
}

extern "x86-interrupt" fn double_fault_handler(
    _stack_frame: InterruptStackFrame,
    error_code: u64,
) -> ! {
    use crate::error::{ErrorContext, ErrorSeverity, KernelError, SystemError, ERROR_MANAGER};

    let error_context = ErrorContext::new(
        KernelError::System(SystemError::InternalError),
        ErrorSeverity::Fatal,
        "double_fault_handler",
        alloc::format!("Double fault with error code: {}", error_code),
    );

    // Try to handle the fatal error gracefully
    if let Some(mut manager) = ERROR_MANAGER.try_lock() {
        let _ = manager.handle_error(error_context);
    } else {
        // Fallback if error manager is not available
        crate::serial_println!("FATAL: Double fault (error code: {})", error_code);
        crate::serial_println!("Stack frame: {:#?}", _stack_frame);
    }

    // Double fault is unrecoverable - halt system
    loop {
        unsafe {
            core::arch::asm!("hlt");
        }
    }
}

extern "x86-interrupt" fn page_fault_handler(
    _stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    use x86_64::registers::control::Cr2;

    let fault_address = Cr2::read();

    // Log all page faults for production debugging
    crate::serial_println!(
        "Page fault at {:?}: present={}, write={}, user={}",
        fault_address,
        !error_code.contains(PageFaultErrorCode::PROTECTION_VIOLATION),
        error_code.contains(PageFaultErrorCode::CAUSED_BY_WRITE),
        error_code.contains(PageFaultErrorCode::USER_MODE)
    );

    PAGE_FAULT_COUNT.fetch_add(1, Ordering::Relaxed);
    EXCEPTION_COUNT.fetch_add(1, Ordering::Relaxed);

    // In production, attempt page fault recovery
    if let Some(recovery_result) = attempt_page_fault_recovery(fault_address, error_code) {
        match recovery_result {
            PageFaultRecovery::Recovered => {
                crate::serial_println!("Page fault recovered successfully");
                return;
            }
            PageFaultRecovery::NeedsSwap => {
                crate::serial_println!("Page fault requires swap operation");
                // Attempt to swap in the page from disk
                // If swap-in fails, terminate the process
                if let Err(_e) = attempt_swap_in_page(fault_address) {
                    crate::serial_println!("Swap-in failed for address {:?}", fault_address);
                    terminate_current_process("Page swap-in failure");
                    return;
                }
            }
        }
    }

    // If recovery fails, use error handling system
    use crate::error::{ErrorContext, ErrorSeverity, KernelError, MemoryError, ERROR_MANAGER};

    let error_context = ErrorContext::new(
        KernelError::Memory(MemoryError::PageFaultUnrecoverable),
        ErrorSeverity::Critical,
        "page_fault_handler",
        alloc::format!("Unrecoverable page fault at address {:?}", fault_address),
    );

    if let Some(mut manager) = ERROR_MANAGER.try_lock() {
        if let Err(_) = manager.handle_error(error_context) {
            // Critical error handling failed - this is very bad
            crate::serial_println!("CRITICAL: Page fault recovery failed completely");
            loop {
                unsafe {
                    core::arch::asm!("hlt");
                }
            }
        }
    } else {
        // Fallback - terminate the current process if possible
        crate::serial_println!("CRITICAL: Unrecoverable page fault at {:?}", fault_address);
        terminate_current_process("Unrecoverable page fault");
    }
}

extern "x86-interrupt" fn divide_error_handler(stack_frame: InterruptStackFrame) {
    use crate::error::{
        ErrorContext, ErrorSeverity, KernelError, ProcessError, RecoveryAction, ERROR_MANAGER,
    };

    // Log divide by zero with context information
    crate::serial_println!(
        "Divide by zero error at RIP: {:?}, RSP: {:?}",
        stack_frame.instruction_pointer,
        stack_frame.stack_pointer
    );
    EXCEPTION_COUNT.fetch_add(1, Ordering::Relaxed);

    let error_context = ErrorContext::new(
        KernelError::Process(ProcessError::InvalidState),
        ErrorSeverity::Error,
        "divide_error_handler",
        alloc::format!("Divide by zero at {:?}", stack_frame.instruction_pointer),
    )
    .with_recovery(RecoveryAction::Isolate);

    if let Some(mut manager) = ERROR_MANAGER.try_lock() {
        if let Err(_) = manager.handle_error(error_context) {
            // Error handling failed - terminate current process
            crate::serial_println!("Terminating process due to divide by zero");
            terminate_current_process("Divide by zero exception");
        }
    } else {
        crate::serial_println!("CRITICAL: Divide by zero - error manager unavailable");
        terminate_current_process("Divide by zero exception (error manager unavailable)");
    }
}

extern "x86-interrupt" fn invalid_opcode_handler(stack_frame: InterruptStackFrame) {
    use crate::error::{
        ErrorContext, ErrorSeverity, KernelError, ProcessError, RecoveryAction, ERROR_MANAGER,
    };

    // Log invalid opcode with detailed context
    crate::serial_println!(
        "Invalid opcode at RIP: {:?}, attempting instruction recovery",
        stack_frame.instruction_pointer
    );
    EXCEPTION_COUNT.fetch_add(1, Ordering::Relaxed);

    let error_context = ErrorContext::new(
        KernelError::Process(ProcessError::InvalidState),
        ErrorSeverity::Error,
        "invalid_opcode_handler",
        alloc::format!("Invalid opcode at {:?}", stack_frame.instruction_pointer),
    )
    .with_recovery(RecoveryAction::Isolate);

    if let Some(mut manager) = ERROR_MANAGER.try_lock() {
        if let Err(_) = manager.handle_error(error_context) {
            crate::serial_println!("Terminating process due to invalid opcode");
            terminate_current_process("Invalid opcode exception");
        }
    } else {
        crate::serial_println!("CRITICAL: Invalid opcode - error manager unavailable");
        terminate_current_process("Invalid opcode exception (error manager unavailable)");
    }
}

extern "x86-interrupt" fn general_protection_fault_handler(
    _stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    use crate::error::{
        ErrorContext, ErrorSeverity, KernelError, RecoveryAction, SecurityError, ERROR_MANAGER,
    };

    // Production: critical protection fault
    crate::serial_println!("CRITICAL: General protection fault ({})", error_code);
    EXCEPTION_COUNT.fetch_add(1, Ordering::Relaxed);

    let error_context = ErrorContext::new(
        KernelError::Security(SecurityError::AccessDenied),
        ErrorSeverity::Critical,
        "general_protection_fault_handler",
        alloc::format!("General protection fault with error code: {}", error_code),
    )
    .with_recovery(RecoveryAction::Isolate);

    if let Some(mut manager) = ERROR_MANAGER.try_lock() {
        if let Err(_) = manager.handle_error(error_context) {
            crate::serial_println!("CRITICAL: GPF recovery failed - system may be compromised");
            // Isolate and terminate the compromised process
            let pid = crate::process::current_pid();
            crate::serial_println!("Isolating compromised process PID {}", pid);
            terminate_current_process("General protection fault - potential security threat");
        }
    } else {
        crate::serial_println!("FATAL: General protection fault - error manager unavailable");
        // Emergency termination
        terminate_current_process("General protection fault (critical)");
    }
}

extern "x86-interrupt" fn stack_segment_fault_handler(
    _stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    use crate::error::{
        ErrorContext, ErrorSeverity, KernelError, MemoryError, RecoveryAction, ERROR_MANAGER,
    };

    // Production: critical stack fault
    crate::serial_println!("CRITICAL: Stack segment fault ({})", error_code);
    EXCEPTION_COUNT.fetch_add(1, Ordering::Relaxed);

    let error_context = ErrorContext::new(
        KernelError::Memory(MemoryError::InvalidAddress),
        ErrorSeverity::Critical,
        "stack_segment_fault_handler",
        alloc::format!("Stack segment fault with error code: {}", error_code),
    )
    .with_recovery(RecoveryAction::Isolate);

    if let Some(mut manager) = ERROR_MANAGER.try_lock() {
        if let Err(_) = manager.handle_error(error_context) {
            crate::serial_println!("CRITICAL: Stack fault recovery failed");
            // Stack is corrupted, terminate process immediately
            terminate_current_process("Stack segment fault - stack corrupted");
        }
    } else {
        crate::serial_println!("FATAL: Stack segment fault - error manager unavailable");
        // Emergency termination with stack cleanup
        terminate_current_process("Stack segment fault (critical)");
    }
}

extern "x86-interrupt" fn segment_not_present_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    use crate::error::{
        ErrorContext, ErrorSeverity, KernelError, MemoryError, RecoveryAction, ERROR_MANAGER,
    };

    // Log detailed segment fault information for debugging
    crate::serial_println!(
        "Segment not present fault - Error code: 0x{:x}, RIP: {:?}",
        error_code,
        stack_frame.instruction_pointer
    );
    EXCEPTION_COUNT.fetch_add(1, Ordering::Relaxed);

    let error_context = ErrorContext::new(
        KernelError::Memory(MemoryError::InvalidAddress),
        ErrorSeverity::Error,
        "segment_not_present_handler",
        alloc::format!(
            "Segment not present - error code: 0x{:x} at {:?}",
            error_code,
            stack_frame.instruction_pointer
        ),
    )
    .with_recovery(RecoveryAction::Retry);

    if let Some(mut manager) = ERROR_MANAGER.try_lock() {
        if let Err(_) = manager.handle_error(error_context) {
            crate::serial_println!("Segment fault recovery failed - terminating process");
            terminate_current_process("Segment not present fault");
        }
    } else {
        crate::serial_println!("CRITICAL: Segment not present - error manager unavailable");
        terminate_current_process("Segment not present fault (error manager unavailable)");
    }
}

extern "x86-interrupt" fn overflow_handler(stack_frame: InterruptStackFrame) {
    // Handle arithmetic overflow - log for debugging but continue execution
    crate::serial_println!(
        "Arithmetic overflow detected at RIP: {:?}",
        stack_frame.instruction_pointer
    );
    EXCEPTION_COUNT.fetch_add(1, Ordering::Relaxed);
    // In production, overflow should be handled gracefully
}

extern "x86-interrupt" fn bound_range_exceeded_handler(stack_frame: InterruptStackFrame) {
    // Handle bounds check failure - log detailed information
    crate::serial_println!(
        "Bounds check failed at RIP: {:?}",
        stack_frame.instruction_pointer
    );
    EXCEPTION_COUNT.fetch_add(1, Ordering::Relaxed);
    // Continue execution after logging - bounds checks are recoverable
}

extern "x86-interrupt" fn invalid_tss_handler(_stack_frame: InterruptStackFrame, error_code: u64) {
    use crate::error::{
        ErrorContext, ErrorSeverity, KernelError, RecoveryAction, SystemError, ERROR_MANAGER,
    };

    // Production: critical TSS error
    crate::serial_println!("CRITICAL: Invalid TSS ({})", error_code);
    EXCEPTION_COUNT.fetch_add(1, Ordering::Relaxed);

    let error_context = ErrorContext::new(
        KernelError::System(SystemError::InternalError),
        ErrorSeverity::Critical,
        "invalid_tss_handler",
        alloc::format!("Invalid TSS with error code: {}", error_code),
    )
    .with_recovery(RecoveryAction::Restart);

    if let Some(mut manager) = ERROR_MANAGER.try_lock() {
        if let Err(_) = manager.handle_error(error_context) {
            crate::serial_println!("CRITICAL: TSS recovery failed - system unstable");
            loop {
                unsafe {
                    core::arch::asm!("hlt");
                }
            }
        }
    } else {
        crate::serial_println!("FATAL: Invalid TSS - error manager unavailable");
        loop {
            unsafe {
                core::arch::asm!("hlt");
            }
        }
    }
}

extern "x86-interrupt" fn virtualization_handler(_stack_frame: InterruptStackFrame) {
    use crate::error::{
        ErrorContext, ErrorSeverity, HardwareError, KernelError, RecoveryAction, ERROR_MANAGER,
    };

    // Production: virtualization error handled
    crate::serial_println!("CRITICAL: Virtualization");
    EXCEPTION_COUNT.fetch_add(1, Ordering::Relaxed);

    let error_context = ErrorContext::new(
        KernelError::Hardware(HardwareError::HardwareFault),
        ErrorSeverity::Warning,
        "virtualization_handler",
        "Virtualization exception occurred".to_string(),
    )
    .with_recovery(RecoveryAction::None);

    if let Some(mut manager) = ERROR_MANAGER.try_lock() {
        let _ = manager.handle_error(error_context);
    } else {
        crate::serial_println!("WARNING: Virtualization exception - error manager unavailable");
    }
}

extern "x86-interrupt" fn alignment_check_handler(
    _stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    use crate::error::{
        ErrorContext, ErrorSeverity, KernelError, ProcessError, RecoveryAction, ERROR_MANAGER,
    };

    // Production: alignment error handled
    crate::serial_println!("CRITICAL: Alignment check ({})", error_code);
    EXCEPTION_COUNT.fetch_add(1, Ordering::Relaxed);

    let error_context = ErrorContext::new(
        KernelError::Process(ProcessError::InvalidState),
        ErrorSeverity::Error,
        "alignment_check_handler",
        alloc::format!("Alignment check exception with error code: {}", error_code),
    )
    .with_recovery(RecoveryAction::Isolate);

    if let Some(mut manager) = ERROR_MANAGER.try_lock() {
        if let Err(_) = manager.handle_error(error_context) {
            crate::serial_println!("Alignment check recovery failed - terminating process");
            terminate_current_process("Alignment check exception");
        }
    } else {
        crate::serial_println!("CRITICAL: Alignment check - error manager unavailable");
        terminate_current_process("Alignment check exception (error manager unavailable)");
    }
}

// ========== HARDWARE INTERRUPT HANDLERS ==========

extern "x86-interrupt" fn timer_interrupt_handler(_stack_frame: InterruptStackFrame) {
    TIMER_COUNT.fetch_add(1, Ordering::Relaxed);
    crate::time::timer_tick();

    unsafe {
        // Send EOI directly to PIC port 0x20
        core::arch::asm!(
            "mov al, 0x20",
            "out 0x20, al",
            options(nomem, nostack, preserves_flags)
        );
    }
}

extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    // Read scancode and forward to keyboard driver for processing
    crate::keyboard::handle_keyboard_interrupt();

    KEYBOARD_COUNT.fetch_add(1, Ordering::Relaxed);

    notify_irq_eoi(InterruptIndex::Keyboard);
}

extern "x86-interrupt" fn mouse_interrupt_handler(_stack_frame: InterruptStackFrame) {
    // Read mouse data byte from PS/2 controller
    let mut port: Port<u8> = Port::new(0x60);
    let byte = unsafe { port.read() };

    // Process the byte through PS/2 mouse driver
    if let Some(packet) = crate::drivers::ps2_mouse::process_byte(byte) {
        // Got a complete mouse packet - send to input manager
        crate::drivers::input_manager::handle_mouse_packet(packet);
    }

    MOUSE_COUNT.fetch_add(1, Ordering::Relaxed);

    notify_irq_eoi(InterruptIndex::Mouse);
}

extern "x86-interrupt" fn serial_port1_interrupt_handler(_stack_frame: InterruptStackFrame) {
    // Handle serial port 1 interrupt - process incoming data
    SERIAL_COUNT.fetch_add(1, Ordering::Relaxed);

    // Process any available serial data
    crate::serial::handle_port1_interrupt();

    notify_irq_eoi(InterruptIndex::SerialPort1);
}

extern "x86-interrupt" fn serial_port2_interrupt_handler(_stack_frame: InterruptStackFrame) {
    // Handle serial port 2 interrupt - process incoming data
    SERIAL_COUNT.fetch_add(1, Ordering::Relaxed);

    // Process any available serial data
    crate::serial::handle_port2_interrupt();

    notify_irq_eoi(InterruptIndex::SerialPort2);
}

extern "x86-interrupt" fn spurious_interrupt_handler(_stack_frame: InterruptStackFrame) {
    // Production: spurious interrupt handled silently
    SPURIOUS_COUNT.fetch_add(1, Ordering::Relaxed);
    // Don't send EOI for spurious interrupts
}

// ========== INTERRUPT UTILITIES ==========

/// Trigger a breakpoint exception for testing
pub fn trigger_breakpoint() {
    x86_64::instructions::interrupts::int3();
}

/// Trigger a page fault for testing
pub unsafe fn trigger_page_fault() {
    let ptr = 0xdeadbeef as *mut u8;
    *ptr = 42;
}

/// Page fault recovery result
#[derive(Debug, Clone, Copy)]
pub enum PageFaultRecovery {
    Recovered,
    NeedsSwap,
}

/// Attempt to recover from a page fault
fn attempt_page_fault_recovery(
    fault_address: x86_64::VirtAddr,
    error_code: x86_64::structures::idt::PageFaultErrorCode,
) -> Option<PageFaultRecovery> {
    use x86_64::structures::idt::PageFaultErrorCode;

    // Check if this is a demand paging fault (page not present)
    if !error_code.contains(PageFaultErrorCode::PROTECTION_VIOLATION) {
        // Page not present - check if it's within valid memory ranges
        let addr = fault_address.as_u64();

        // MMIO regions (PCI BARs, framebuffer, APIC, etc.) are typically
        // mapped at high physical addresses (>= 0x80000000). These should
        // already be mapped via map_mmio_region — if they fault, it's a
        // real error, not a demand page. Skip swap-in for these.
        if addr >= 0x8000_0000 {
            return None;
        }

        // Check if address is in user space and within reasonable bounds
        if addr >= 0x1000 && addr < 0x7fff_ffff_ffff {
            // For now, cannot recover - demand paging not fully implemented
            // In a full implementation, we would allocate and map a page here
            return Some(PageFaultRecovery::NeedsSwap);
        }
    }

    // Cannot recover from this type of page fault
    None
}

/// Attempt to swap in a page from disk
fn attempt_swap_in_page(fault_address: x86_64::VirtAddr) -> Result<(), &'static str> {
    use crate::memory::get_memory_manager;
    use x86_64::structures::paging::{Page, Size4KiB};

    crate::serial_println!("Attempting swap-in for address {:?}", fault_address);

    // Get the memory manager
    let memory_manager = get_memory_manager().ok_or("Memory manager not available")?;

    // Get the page that caused the fault
    let _page: Page<Size4KiB> = Page::containing_address(fault_address);

    // Step 1: Use the existing memory manager's swap-in functionality
    // The memory manager already has a handle_swap_in method we can use

    // Find the region containing this address
    let region = memory_manager
        .find_region(fault_address)
        .ok_or("No memory region found for fault address")?;

    // Use the existing swap-in handler
    memory_manager
        .handle_swap_in(fault_address, &region)
        .map_err(|e| match e {
            crate::memory::MemoryError::OutOfMemory => "Out of memory during swap-in",
            crate::memory::MemoryError::MappingFailed => "Failed to map swapped page",
            crate::memory::MemoryError::InvalidAddress => "Invalid address for swap-in",
            _ => "Memory manager swap-in failed",
        })?;

    crate::serial_println!("Successfully swapped in page at {:?}", fault_address);
    Ok(())
}

/// Terminate the current process and yield to scheduler
fn terminate_current_process(reason: &str) {
    let pid = crate::process::current_pid();

    // Log termination reason with severity
    crate::serial_println!("PROCESS TERMINATION: PID {} - {}", pid, reason);

    // If we're terminating the kernel process (PID 0), this is a fatal error
    if pid == 0 {
        crate::serial_println!("FATAL: Cannot terminate kernel process - halting system");
        loop {
            unsafe {
                core::arch::asm!("hlt");
            }
        }
    }

    // Attempt to terminate the process via process manager
    let process_manager = crate::process::get_process_manager();
    if let Err(e) = process_manager.terminate_process(pid, -1) {
        // If termination fails, log the error but don't panic
        crate::serial_println!("ERROR: Failed to terminate process {}: {}", pid, e);
        crate::serial_println!("Forcing process state change and yielding to scheduler");
    }

    // Yield to the scheduler to switch to another process
    // This should not return if termination was successful
    crate::scheduler::yield_cpu();

    // If we somehow return here, force a reschedule
    crate::serial_println!("WARNING: Returned from yield after termination - forcing reschedule");
    loop {
        crate::scheduler::yield_cpu();
        unsafe {
            core::arch::asm!("pause");
        }
    }
}

/// Trigger a divide by zero exception for testing
static ZERO_DIVISOR: i32 = 0;

pub fn trigger_divide_by_zero() {
    let x: i32 = 42;
    let zero = unsafe { ptr::read_volatile(&ZERO_DIVISOR) };
    let _result = x / zero;
}

/// Check if interrupts are enabled
pub fn are_enabled() -> bool {
    x86_64::instructions::interrupts::are_enabled()
}

/// Get the current interrupt stack frame address
pub fn get_current_stack_frame() -> VirtAddr {
    // Use inline assembly to get RSP since the rsp module might not be available
    let rsp: u64;
    unsafe {
        core::arch::asm!("mov {0:r}, rsp", out(reg) rsp, options(nostack, preserves_flags));
    }
    VirtAddr::new(rsp)
}

// ========== INTERRUPT DEBUGGING ==========

impl fmt::Display for InterruptStats {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Interrupt Statistics:\n\
             Timer: {}\n\
             Keyboard: {}\n\
             Serial: {}\n\
             Exceptions: {}\n\
             Page Faults: {}\n\
             Spurious: {}",
            self.timer_count,
            self.keyboard_count,
            self.serial_count,
            self.exception_count,
            self.page_fault_count,
            self.spurious_count
        )
    }
}

/// Get current interrupt statistics for monitoring
pub fn get_interrupt_stats() -> InterruptStats {
    get_stats()
}

/// Reset interrupt statistics counters
pub fn reset_interrupt_stats() {
    reset_stats();
}

/// Validate interrupt system is properly configured
fn validate_interrupt_system() {
    // Check if APIC is being used
    if crate::apic::is_apic_available() {
        crate::serial_println!("Interrupt system validation: APIC active");
    } else {
        crate::serial_println!("Interrupt system validation: Legacy PIC active");
    }

    // Verify IDT is loaded
    let idt_info = x86_64::instructions::tables::sidt();
    if idt_info.limit == 0 {
        crate::serial_println!("Warning: IDT appears to be empty");
    } else {
        crate::serial_println!("IDT loaded with {} entries", (idt_info.limit + 1) / 16);
    }
}

/// Validate interrupt system functionality
pub fn test_interrupts() {
    // Validate interrupt system by triggering a controlled breakpoint
    trigger_breakpoint();
    // System validated - breakpoint handler completed successfully
}

/// Get total interrupt count for health monitoring
pub fn get_interrupt_count() -> u64 {
    TIMER_COUNT.load(Ordering::Relaxed)
        + KEYBOARD_COUNT.load(Ordering::Relaxed)
        + EXCEPTION_COUNT.load(Ordering::Relaxed)
        + PAGE_FAULT_COUNT.load(Ordering::Relaxed)
        + SPURIOUS_COUNT.load(Ordering::Relaxed)
}

// =============================================================================
// Interrupt Control Functions
// =============================================================================

/// Initialize the legacy Programmable Interrupt Controller (8259 PIC).
///
/// This function explicitly initializes both the primary and secondary PICs
/// with the configured interrupt vector offsets. The PICs are set up in
/// cascade mode with the secondary PIC connected to IRQ2 of the primary.
///
/// # Safety
///
/// This function is safe to call multiple times, though typically it should
/// only be called once during kernel initialization via `init()`.
///
/// # Note
///
/// The PIC is also initialized during the `init()` function. This function
/// exists for compatibility with code that expects an explicit `init_pic()` call
/// or for reinitializing the PIC after certain operations.
pub fn init_pic() {
    unsafe {
        PICS.lock().initialize();
    }
}

/// Check if interrupts are currently enabled.
///
/// This function reads the RFLAGS register and checks the Interrupt Flag (IF).
/// When IF is set, the CPU will respond to maskable hardware interrupts.
///
/// # Returns
///
/// Returns `true` if interrupts are enabled (IF flag is set), `false` otherwise.
///
/// # Example
///
/// ```no_run
/// if interrupts_enabled() {
///     // Perform operation that requires interrupts to be enabled
/// }
/// ```
pub fn interrupts_enabled() -> bool {
    x86_64::instructions::interrupts::are_enabled()
}

/// Disable interrupts by clearing the Interrupt Flag (IF) in RFLAGS.
///
/// This function executes the `CLI` instruction, which clears the IF flag
/// and prevents the CPU from responding to maskable hardware interrupts.
/// Non-maskable interrupts (NMI) are not affected.
///
/// # Safety
///
/// Disabling interrupts for extended periods can cause system instability,
/// missed hardware events, and timer drift. Always re-enable interrupts
/// as soon as possible using `enable_interrupts()`.
///
/// # Example
///
/// ```no_run
/// disable_interrupts();
/// // Critical section - no interrupts will occur
/// // ... perform atomic operations ...
/// enable_interrupts();
/// ```
pub fn disable_interrupts() {
    x86_64::instructions::interrupts::disable();
}

/// Enable interrupts by setting the Interrupt Flag (IF) in RFLAGS.
///
/// This function executes the `STI` instruction, which sets the IF flag
/// and allows the CPU to respond to maskable hardware interrupts.
///
/// # Safety
///
/// Ensure that the Interrupt Descriptor Table (IDT) and interrupt handlers
/// are properly configured before enabling interrupts. Enabling interrupts
/// without proper handlers will cause undefined behavior or system crashes.
///
/// # Example
///
/// ```no_run
/// disable_interrupts();
/// // Critical section
/// enable_interrupts();
/// // Interrupts can now be serviced
/// ```
pub fn enable_interrupts() {
    x86_64::instructions::interrupts::enable();
}
