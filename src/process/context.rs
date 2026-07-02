//! Context Switching Implementation
//!
//! This module provides low-level context switching functionality for RustOS,
//! including CPU register saving/restoring, stack switching, and FPU state management.

use super::{CpuContext, Pid};
use core::arch::x86_64::__cpuid;
use core::arch::{asm, naked_asm};

/// FPU/SSE state structure
#[derive(Debug, Clone)]
#[repr(C, align(16))]
pub struct FpuState {
    /// FPU control word
    pub fcw: u16,
    /// FPU status word
    pub fsw: u16,
    /// FPU tag word
    pub ftw: u8,
    /// Reserved
    pub reserved1: u8,
    /// FPU instruction pointer offset
    pub fop: u16,
    /// FPU instruction pointer segment
    pub fip: u32,
    /// FPU data pointer offset
    pub fdp: u32,
    /// FPU data pointer segment
    pub fds: u32,
    /// MXCSR register
    pub mxcsr: u32,
    /// MXCSR mask
    pub mxcsr_mask: u32,
    /// ST0-ST7 registers (8 * 16 bytes)
    pub st_regs: [u8; 128],
    /// XMM0-XMM15 registers (16 * 16 bytes)
    pub xmm_regs: [u8; 256],
    /// Padding to align to 512 bytes
    pub padding: [u8; 96],
}

impl Default for FpuState {
    fn default() -> Self {
        Self {
            fcw: 0x037F,
            fsw: 0,
            ftw: 0xFF,
            reserved1: 0,
            fop: 0,
            fip: 0,
            fdp: 0,
            fds: 0,
            mxcsr: 0x1F80,
            mxcsr_mask: 0xFFFF,
            st_regs: [0; 128],
            xmm_regs: [0; 256],
            padding: [0; 96],
        }
    }
}

/// Complete process context including CPU and FPU state
#[derive(Debug, Clone)]
pub struct ProcessContext {
    /// CPU register state
    pub cpu: CpuContext,
    /// FPU/SSE state
    pub fpu: FpuState,
    /// Kernel stack pointer
    pub kernel_stack: u64,
    /// User stack pointer
    pub user_stack: u64,
    /// Page table physical address
    pub page_table: u64,
}

impl Default for ProcessContext {
    fn default() -> Self {
        Self {
            cpu: CpuContext::default(),
            fpu: FpuState::default(),
            kernel_stack: 0,
            user_stack: 0,
            page_table: 0,
        }
    }
}

/// Context switcher - handles all context switching operations
pub struct ContextSwitcher {
    /// Whether FPU lazy switching is enabled
    fpu_lazy_switching: bool,
    /// Current process that owns the FPU
    fpu_owner: Option<Pid>,
    /// Context switch statistics
    switch_count: u64,
}

impl ContextSwitcher {
    /// Create a new context switcher
    pub const fn new() -> Self {
        Self {
            fpu_lazy_switching: true,
            fpu_owner: None,
            switch_count: 0,
        }
    }

    /// Initialize the context switcher
    pub fn init(&mut self) -> Result<(), &'static str> {
        // Initialize FPU
        unsafe {
            self.init_fpu()?;
        }

        // Clear FPU owner
        self.fpu_owner = None;

        Ok(())
    }

    /// Switch context from current process to target process
    pub unsafe fn switch_context(
        &mut self,
        current_context: &mut ProcessContext,
        target_context: &ProcessContext,
        target_pid: Pid,
    ) -> Result<(), &'static str> {
        self.switch_count += 1;

        let mut tmp_cr0: u64;

        // Handle FPU context switching. This must run BEFORE the register switch
        // below, because `context_switch_asm` does not return here until this task
        // is scheduled back in.
        if self.fpu_lazy_switching {
            // Lazy FPU switching: set the Task Switched flag so the next FPU
            // instruction triggers a #NM exception.  The #NM handler
            // (`handle_fpu_exception`) saves the previous owner's state and
            // restores the new owner's state on first use.
            if self.fpu_owner.is_some() {
                asm!("mov {0:r}, cr0", out(reg) tmp_cr0, options(nomem, preserves_flags));
                tmp_cr0 |= 0x8; // TS bit
                asm!("mov cr0, {0:r}", in(reg) tmp_cr0, options(nomem, preserves_flags));
            }
        } else {
            // Always save/restore FPU state
            self.save_fpu_context(&mut current_context.fpu)?;
            self.restore_fpu_context(&target_context.fpu)?;
            self.fpu_owner = Some(target_pid);
        }

        // Switch page tables if necessary
        if current_context.page_table != target_context.page_table {
            self.switch_page_table(target_context.page_table);
        }

        // Switch to kernel stack if necessary
        if current_context.kernel_stack != target_context.kernel_stack {
            self.switch_kernel_stack(target_context.kernel_stack);
        }
        crate::privileged_syscalls::apply_io_privileges_for_process(target_pid);

        // ponytail: route the actual register/stack switch through the working
        // naked `context_switch_asm` instead of the broken `save_cpu_context` /
        // `restore_cpu_context` pair (whose out operands aliased the GP regs being
        // read and which saved/restored the switch routine's own RSP). The naked
        // routine saves the current GP regs + RSP + the return RIP into
        // `current_context.cpu`, loads `target_context.cpu`, and `ret`s onto the
        // target stack. It does not return to this point until `current` is
        // switched back in later.
        context_switch_asm(&mut current_context.cpu, &target_context.cpu);

        Ok(())
    }

    /// Save FPU/SSE context
    unsafe fn save_fpu_context(&self, fpu_state: &mut FpuState) -> Result<(), &'static str> {
        // Check if we have SSE support
        if self.has_sse() {
            // Use FXSAVE to save FPU and SSE state
            asm!(
                "fxsave [{}]",
                in(reg) fpu_state as *mut FpuState,
            );
        } else {
            // Fall back to FSAVE for older processors
            asm!(
                "fsave [{}]",
                in(reg) fpu_state as *mut FpuState,
            );
        }

        Ok(())
    }

    /// Restore FPU/SSE context
    unsafe fn restore_fpu_context(&self, fpu_state: &FpuState) -> Result<(), &'static str> {
        // Check if we have SSE support
        if self.has_sse() {
            // Use FXRSTOR to restore FPU and SSE state
            asm!(
                "fxrstor [{}]",
                in(reg) fpu_state as *const FpuState,
            );
        } else {
            // Fall back to FRSTOR for older processors
            asm!(
                "frstor [{}]",
                in(reg) fpu_state as *const FpuState,
            );
        }

        Ok(())
    }

    /// Initialize FPU
    unsafe fn init_fpu(&self) -> Result<(), &'static str> {
        // Initialize FPU
        asm!("finit");

        // Enable FPU and SSE if available
        if self.has_sse() {
            // Enable SSE and FXSAVE/FXRSTOR
            let mut cr4: u64;
            asm!("mov {0:r}, cr4", out(reg) cr4);
            cr4 |= (1 << 9) | (1 << 10); // OSFXSR and OSXMMEXCPT
            asm!("mov cr4, {0:r}", in(reg) cr4);
        }

        // Clear task switched flag
        self.clear_task_switched_flag();

        Ok(())
    }

    /// Check if processor has SSE support
    fn has_sse(&self) -> bool {
        // Check CPUID for SSE support using the intrinsic to avoid clobbering RBX
        (__cpuid(1).edx & (1 << 25)) != 0
    }

    /// Check if processor has XSAVE support
    fn has_xsave(&self) -> bool {
        (__cpuid(1).ecx & (1 << 26)) != 0
    }

    /// Check if processor has AVX support
    fn has_avx(&self) -> bool {
        let cpuid = __cpuid(1);
        (cpuid.ecx & (1 << 28)) != 0 && (cpuid.ecx & (1 << 26)) != 0 // AVX + XSAVE
    }

    /// Get XSAVE area size
    fn get_xsave_area_size(&self) -> usize {
        if self.has_xsave() {
            let cpuid = __cpuid(13); // XSAVE features
            cpuid.ecx as usize
        } else {
            512 // Standard FXSAVE area size
        }
    }

    /// Clear task switched flag in CR0
    unsafe fn clear_task_switched_flag(&self) {
        asm!("clts");
    }

    /// Switch page table
    unsafe fn switch_page_table(&self, page_table_phys: u64) {
        if page_table_phys != 0 {
            asm!(
                "mov cr3, {}",
                in(reg) page_table_phys,
            );
        }
    }

    /// Switch kernel stack
    ///
    /// Updates the TSS RSP0 entry so that the next ring-0 entry (syscall or
    /// interrupt) uses the target process's kernel stack.
    unsafe fn switch_kernel_stack(&self, kernel_stack: u64) {
        if kernel_stack != 0 {
            crate::gdt::set_kernel_stack(x86_64::VirtAddr::new(kernel_stack));
        }
    }

    /// Handle FPU exception (for lazy switching)
    ///
    /// Saves the previous owner's FPU state to its persistent PCB, then restores
    /// the current process's FPU state and marks it as the new owner.
    pub unsafe fn handle_fpu_exception(
        &mut self,
        current_pid: Pid,
        context: &ProcessContext,
    ) -> Result<(), &'static str> {
        if self.fpu_lazy_switching {
            // Clear task switched flag to allow FPU access
            self.clear_task_switched_flag();

            // If a different process owned the FPU, save its state persistently.
            if let Some(owner_pid) = self.fpu_owner {
                if owner_pid != current_pid {
                    let process_manager = crate::process::get_process_manager();
                    if let Some(owner) = process_manager.get_process(owner_pid) {
                        let mut owner_fpu = owner.fpu;
                        self.save_fpu_context(&mut owner_fpu)?;
                        process_manager.with_process_mut(owner_pid, |p| {
                            p.fpu = owner_fpu;
                        });
                    }
                }
            }

            // Restore FPU state for current process
            self.restore_fpu_context(&context.fpu)?;
            self.fpu_owner = Some(current_pid);
        }

        Ok(())
    }

    /// Get context switch statistics
    pub fn get_switch_count(&self) -> u64 {
        self.switch_count
    }

    /// Enable or disable FPU lazy switching
    pub fn set_fpu_lazy_switching(&mut self, enable: bool) {
        self.fpu_lazy_switching = enable;
    }
}

/// Context switcher performance statistics
#[derive(Debug, Clone)]
pub struct ContextSwitcherStats {
    pub total_switches: u64,
    pub average_switch_time: u64,
    pub fpu_owner: Option<super::Pid>,
    pub lazy_fpu_enabled: bool,
}

/// Assembly function for low-level context switch
/// This would typically be implemented in assembly for maximum efficiency
#[unsafe(naked)]
pub unsafe extern "C" fn context_switch_asm(
    _old_context: *mut CpuContext,
    _new_context: *const CpuContext,
) {
    naked_asm!(
        r#"
        mov [rdi + 0x00], rax
        mov [rdi + 0x08], rbx
        mov [rdi + 0x10], rcx
        mov [rdi + 0x18], rdx
        mov [rdi + 0x20], rsi
        mov [rdi + 0x28], rdi
        mov [rdi + 0x30], rbp
        mov [rdi + 0x38], rsp
        mov [rdi + 0x40], r8
        mov [rdi + 0x48], r9
        mov [rdi + 0x50], r10
        mov [rdi + 0x58], r11
        mov [rdi + 0x60], r12
        mov [rdi + 0x68], r13
        mov [rdi + 0x70], r14
        mov [rdi + 0x78], r15

        mov rax, [rsp]
        mov [rdi + 0x80], rax

        pushf
        pop rax
        mov [rdi + 0x88], rax

        // Save current FS_BASE (TLS pointer) via RDMSR.
        // MSR_FS_BASE = 0xC0000100
        mov ecx, 0xC0000100
        rdmsr
        shl rdx, 32
        or rax, rdx
        mov [rdi + 0xA0], rax

        mov rax, [rsi + 0x00]
        mov rbx, [rsi + 0x08]
        mov rcx, [rsi + 0x10]
        mov rdx, [rsi + 0x18]
        mov rbp, [rsi + 0x30]
        mov rsp, [rsi + 0x38]
        mov r8,  [rsi + 0x40]
        mov r9,  [rsi + 0x48]
        mov r10, [rsi + 0x50]
        mov r11, [rsi + 0x58]
        mov r12, [rsi + 0x60]
        mov r13, [rsi + 0x68]
        mov r14, [rsi + 0x70]
        mov r15, [rsi + 0x78]

        push qword ptr [rsi + 0x88]
        popf

        // Restore FS_BASE (TLS pointer) via WRMSR if non-zero.
        // MSR_FS_BASE = 0xC0000100
        mov rax, [rsi + 0xA0]
        test rax, rax
        jz 2f
        mov rdx, rax
        shr rdx, 32
        mov ecx, 0xC0000100
        wrmsr
    2:

        push qword ptr [rsi + 0x80]

        mov rdi, [rsi + 0x28]
        mov rsi, [rsi + 0x20]

        ret
        "#
    );
}

/// Create a new process context for a given entry point
pub fn create_process_context(
    entry_point: u64,
    stack_pointer: u64,
    kernel_stack: u64,
    page_table: u64,
) -> ProcessContext {
    let mut context = ProcessContext::default();

    // Set up CPU context
    context.cpu.rip = entry_point;
    context.cpu.rsp = stack_pointer;
    context.cpu.rflags = 0x202; // Enable interrupts
    context.cpu.cs = 0x08; // Kernel code segment
    context.cpu.ds = 0x10; // Kernel data segment
    context.cpu.es = 0x10;
    context.cpu.fs = 0x10;
    context.cpu.gs = 0x10;
    context.cpu.ss = 0x10; // Kernel stack segment

    // Set up memory management
    context.kernel_stack = kernel_stack;
    context.user_stack = stack_pointer;
    context.page_table = page_table;

    // Initialize FPU state to default
    context.fpu = FpuState::default();

    context
}

/// Global context switcher instance.
///
/// Deliberately kept as `static mut` rather than a `spin::Mutex` wrapper.
/// `switch_context` does not return until the outgoing task is scheduled back
/// in (it jumps to another task's stack mid-call), so a lock taken here would
/// be held across the switch and never released before the next task tries to
/// switch — an unconditional scheduler deadlock. Context switching is already
/// serialized per core (only one switch can be in flight on a CPU at a time),
/// so the `static mut` access is sound. Do not "fix" this into a Mutex.
static mut CONTEXT_SWITCHER: ContextSwitcher = ContextSwitcher::new();

/// Get the global context switcher.
///
/// See [`CONTEXT_SWITCHER`] for why this returns a raw `&'static mut` instead
/// of a lock guard.
pub fn get_context_switcher() -> &'static mut ContextSwitcher {
    unsafe { &mut *core::ptr::addr_of_mut!(CONTEXT_SWITCHER) }
}

/// Initialize the context switching system
pub fn init() -> Result<(), &'static str> {
    unsafe { (&mut *core::ptr::addr_of_mut!(CONTEXT_SWITCHER)).init() }
}
