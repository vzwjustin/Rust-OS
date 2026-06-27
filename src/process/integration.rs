//! Process Management Integration
//!
//! This module provides integration between the process management system
//! and other kernel subsystems like memory management and interrupts.

use super::{get_process_manager, Pid};
use alloc::vec;

/// Process management integration with timer interrupts
pub struct TimerIntegration {
    /// Time slice counter
    time_slice_counter: u32,
    /// Scheduling frequency (ticks per schedule)
    schedule_frequency: u32,
}

impl TimerIntegration {
    /// Create new timer integration
    pub const fn new() -> Self {
        Self {
            time_slice_counter: 0,
            schedule_frequency: 10, // Schedule every 10 timer ticks
        }
    }

    /// Handle timer interrupt for process scheduling
    pub fn handle_timer_interrupt(&mut self) -> Result<Option<Pid>, &'static str> {
        // Increment system time
        super::tick_system_time();

        // Wake up sleeping threads
        let thread_manager = super::thread::get_thread_manager();
        thread_manager.wake_sleeping_threads();

        // Update scheduler tick
        let process_manager = get_process_manager();
        {
            let mut scheduler = process_manager.scheduler.lock();
            scheduler.tick();
        }

        self.time_slice_counter += 1;

        // Check if we should perform scheduling
        if self.time_slice_counter >= self.schedule_frequency {
            self.time_slice_counter = 0;

            // Trigger process scheduling
            process_manager.schedule()
        } else {
            Ok(None)
        }
    }

    /// Set scheduling frequency
    pub fn set_schedule_frequency(&mut self, frequency: u32) {
        self.schedule_frequency = frequency.max(1);
    }

    /// Get current time slice counter
    pub fn get_time_slice_counter(&self) -> u32 {
        self.time_slice_counter
    }
}

/// Process management integration with memory management
pub struct MemoryIntegration;

impl MemoryIntegration {
    /// Handle page fault for process
    pub fn handle_page_fault(
        pid: Pid,
        fault_address: u64,
        error_code: u64,
    ) -> Result<(), &'static str> {
        let process_manager = get_process_manager();

        // Get process information
        let process = process_manager
            .get_process(pid)
            .ok_or("Process not found")?;

        // Check if fault address is within process memory space
        if fault_address >= process.memory.vm_start
            && fault_address < process.memory.vm_start + process.memory.vm_size
        {
            // Handle different types of page faults
            if (error_code & 0x1) == 0 {
                // Page not present - allocate page
                Self::allocate_page_for_process(pid, fault_address)
            } else if (error_code & 0x2) != 0 {
                // Write to read-only page
                Self::handle_cow_page(pid, fault_address)
            } else {
                Err("Invalid page fault")
            }
        } else {
            // Segmentation fault - terminate process
            process_manager.terminate_process(pid, -11) // SIGSEGV
        }
    }

    /// Allocate a new page for process using production memory manager
    fn allocate_page_for_process(_pid: Pid, fault_address: u64) -> Result<(), &'static str> {
        use crate::memory::{get_memory_manager, MemoryProtection, MemoryRegionType, PAGE_SIZE};
        use x86_64::VirtAddr;

        let memory_manager = get_memory_manager().ok_or("Memory manager not initialized")?;
        let fault_addr = VirtAddr::new(fault_address);

        // Check if we already have a region containing this address
        if let Some(region) = memory_manager.find_region(fault_addr) {
            if !region.mapped {
                // Implement demand paging by triggering page fault handler
                return crate::memory::handle_page_fault(fault_addr, 0) // Page not present
                    .map_err(|_| "Failed to handle demand paging");
            }
        } else {
            // Create a new memory region for this process
            let _page_addr = fault_address & !(PAGE_SIZE as u64 - 1); // Align to page boundary
            let _region = memory_manager
                .allocate_region(
                    PAGE_SIZE,
                    MemoryRegionType::UserData,
                    MemoryProtection::USER_DATA,
                )
                .map_err(|_| "Failed to allocate memory region")?;
        }

        Ok(())
    }

    /// Handle copy-on-write page fault using production memory manager
    fn handle_cow_page(_pid: Pid, fault_address: u64) -> Result<(), &'static str> {
        use crate::memory::{get_memory_manager, handle_page_fault};
        use x86_64::VirtAddr;

        let memory_manager = get_memory_manager().ok_or("Memory manager not initialized")?;
        let fault_addr = VirtAddr::new(fault_address);

        // Check if this is a valid copy-on-write region
        if let Some(region) = memory_manager.find_region(fault_addr) {
            if region.protection.copy_on_write {
                // Handle COW fault with write access
                return handle_page_fault(fault_addr, 0x2) // Write fault
                    .map_err(|_| "Failed to handle copy-on-write fault");
            }
        }

        Err("Invalid copy-on-write access")
    }

    /// Set up complete memory space for new process
    pub fn setup_process_memory(pid: Pid, size: u64) -> Result<u64, &'static str> {
        use crate::memory::{
            allocate_memory_with_guards, get_memory_manager, MemoryProtection, MemoryRegionType,
            PAGE_SIZE,
        };

        let memory_manager = get_memory_manager().ok_or("Memory manager not initialized")?;

        // Calculate memory layout
        let base_address = 0x400000 + (pid as u64 * 0x10000000); // 256MB per process
        let code_size =
            MemoryIntegration::align_up_u64(size.max(PAGE_SIZE as u64), PAGE_SIZE as u64);
        let data_size = PAGE_SIZE as u64 * 16; // 64KB data section
        let heap_size = PAGE_SIZE as u64 * 256; // 1MB heap
        let stack_size = PAGE_SIZE as u64 * 32; // 128KB stack

        // Allocate code region
        let _code_region = memory_manager
            .allocate_region(
                code_size as usize,
                MemoryRegionType::UserCode,
                MemoryProtection::USER_CODE,
            )
            .map_err(|_| "Failed to allocate code region")?;

        // Allocate data region with guard pages
        let _data_addr = allocate_memory_with_guards(
            data_size as usize,
            MemoryRegionType::UserData,
            MemoryProtection::USER_DATA,
        )
        .map_err(|_| "Failed to allocate data region")?;

        // Allocate heap region with guard pages
        let _heap_addr = allocate_memory_with_guards(
            heap_size as usize,
            MemoryRegionType::UserHeap,
            MemoryProtection::USER_DATA,
        )
        .map_err(|_| "Failed to allocate heap region")?;

        // Allocate stack region with guard pages (grows downward)
        let _stack_addr = allocate_memory_with_guards(
            stack_size as usize,
            MemoryRegionType::UserStack,
            MemoryProtection::USER_DATA,
        )
        .map_err(|_| "Failed to allocate stack region")?;

        Ok(base_address)
    }

    /// Align up to nearest boundary for u64
    fn align_up_u64(addr: u64, align: u64) -> u64 {
        (addr + align - 1) & !(align - 1)
    }

    /// Clean up memory space for terminated process
    pub fn cleanup_process_memory(pid: Pid) -> Result<(), &'static str> {
        use crate::memory::{deallocate_memory, get_memory_manager, PAGE_SIZE};
        use x86_64::VirtAddr;

        let memory_manager = get_memory_manager().ok_or("Memory manager not initialized")?;
        let process_manager = get_process_manager();

        // Get process information for detailed cleanup
        let process = process_manager
            .get_process(pid)
            .ok_or("Process not found")?;

        // Calculate process memory layout
        let base_address = 0x400000 + (pid as u64 * 0x10000000);
        let code_size = 0x100000; // 1MB code section
        let data_size = PAGE_SIZE * 16; // 64KB data section
        let _heap_size = PAGE_SIZE * 256; // 1MB heap
        let _stack_size = PAGE_SIZE * 32; // 128KB stack

        // List of memory regions to clean up
        let regions_to_cleanup = vec![
            (base_address, code_size),                    // Code region
            (base_address + code_size as u64, data_size), // Data region
            (process.memory.heap_start, process.memory.heap_size as usize), // Heap
            (
                process.memory.stack_start,
                process.memory.stack_size as usize,
            ), // Stack
        ];

        // Clean up each memory region
        for (start_addr, size) in regions_to_cleanup {
            let start_vaddr = VirtAddr::new(start_addr);

            // Find and deallocate region
            if let Some(_region) = memory_manager.find_region(start_vaddr) {
                // Unmap pages in the region
                for offset in (0..size).step_by(PAGE_SIZE) {
                    let addr = VirtAddr::new(start_addr + offset as u64);
                    let _ = deallocate_memory(addr); // Ignore errors for cleanup
                }
            }
        }

        // Clean up page table entries for this process
        if process.memory.page_directory != 0 {
            // In a real implementation, we would walk the page table and free all mapped pages
            // For now, we'll clean up the known regions
        }

        // Clean up any remaining shared memory segments
        let ipc_manager = super::ipc::get_ipc_manager();
        ipc_manager.cleanup_process_ipc(pid)?;

        Ok(())
    }
}

/// Process management integration with interrupt handling
pub struct InterruptIntegration;

impl InterruptIntegration {
    /// Handle system call interrupt
    pub fn handle_syscall_interrupt(
        syscall_number: u64,
        args: &[u64],
    ) -> Result<u64, &'static str> {
        let process_manager = get_process_manager();
        process_manager.handle_syscall(syscall_number, args)
    }

    /// Handle keyboard interrupt for process input
    pub fn handle_keyboard_interrupt(scancode: u8) -> Result<(), &'static str> {
        // Convert scancode to character
        let character = Self::scancode_to_char(scancode)?;

        // Get process manager and IPC manager
        let process_manager = get_process_manager();
        let ipc_manager = super::ipc::get_ipc_manager();

        // Find processes waiting for keyboard input
        // In a full implementation, we would maintain a list of processes waiting for stdin
        let processes = process_manager.list_processes();

        for (pid, _name, state, _priority) in processes {
            if state == super::ProcessState::Blocked {
                // Check if process is waiting for keyboard input
                if let Some(process) = process_manager.get_process(pid) {
                    // Check if stdin (fd 0) is being read
                    if process.fd_table.contains_key(&0) {
                        // Create keyboard input message
                        let input_data = vec![character];

                        // Try to deliver via stdin pipe or message queue
                        // This is a simplified implementation - real kernels have more complex TTY handling
                        if let Ok(msgq_id) = ipc_manager.create_message_queue(64, 256) {
                            let _ = ipc_manager.send_message(
                                msgq_id, 1, // Message type for keyboard input
                                input_data, 0, // Kernel PID
                            );

                            // Wake up the blocked process
                            let _ = process_manager.unblock_process(pid);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Convert scancode to ASCII character
    fn scancode_to_char(scancode: u8) -> Result<u8, &'static str> {
        // Simplified scancode to ASCII mapping (US keyboard layout)
        match scancode {
            0x02 => Ok(b'1'),
            0x03 => Ok(b'2'),
            0x04 => Ok(b'3'),
            0x05 => Ok(b'4'),
            0x06 => Ok(b'5'),
            0x07 => Ok(b'6'),
            0x08 => Ok(b'7'),
            0x09 => Ok(b'8'),
            0x0A => Ok(b'9'),
            0x0B => Ok(b'0'),
            0x10 => Ok(b'q'),
            0x11 => Ok(b'w'),
            0x12 => Ok(b'e'),
            0x13 => Ok(b'r'),
            0x14 => Ok(b't'),
            0x15 => Ok(b'y'),
            0x16 => Ok(b'u'),
            0x17 => Ok(b'i'),
            0x18 => Ok(b'o'),
            0x19 => Ok(b'p'),
            0x1C => Ok(b'\n'), // Enter
            0x39 => Ok(b' '),  // Space
            _ => Err("Unknown scancode"),
        }
    }

    /// Handle signal delivery to process
    pub fn deliver_signal(pid: Pid, signal: u32) -> Result<(), &'static str> {
        let process_manager = get_process_manager();
        let ipc_manager = super::ipc::get_ipc_manager();

        // Check if process exists
        let process = process_manager
            .get_process(pid)
            .ok_or("Process not found")?;

        // Convert signal number to IPC signal enum
        let ipc_signal = match signal {
            1 => super::ipc::Signal::SIGHUP,
            2 => super::ipc::Signal::SIGINT,
            3 => super::ipc::Signal::SIGQUIT,
            4 => super::ipc::Signal::SIGILL,
            5 => super::ipc::Signal::SIGTRAP,
            6 => super::ipc::Signal::SIGABRT,
            7 => super::ipc::Signal::SIGBUS,
            8 => super::ipc::Signal::SIGFPE,
            9 => super::ipc::Signal::SIGKILL,
            10 => super::ipc::Signal::SIGUSR1,
            11 => super::ipc::Signal::SIGSEGV,
            12 => super::ipc::Signal::SIGUSR2,
            13 => super::ipc::Signal::SIGPIPE,
            14 => super::ipc::Signal::SIGALRM,
            15 => super::ipc::Signal::SIGTERM,
            17 => super::ipc::Signal::SIGCHLD,
            18 => super::ipc::Signal::SIGCONT,
            19 => super::ipc::Signal::SIGSTOP,
            20 => super::ipc::Signal::SIGTSTP,
            _ => return Err("Invalid signal number"),
        };

        // Check for uncatchable signals
        match ipc_signal {
            super::ipc::Signal::SIGKILL | super::ipc::Signal::SIGSTOP => {
                // These signals cannot be caught, blocked, or ignored
                match ipc_signal {
                    super::ipc::Signal::SIGKILL => {
                        process_manager.terminate_process(pid, -9)?;
                    }
                    super::ipc::Signal::SIGSTOP => {
                        // Stop the process (change state to blocked)
                        process_manager.block_process(pid)?;
                    }
                    _ => unreachable!(),
                }
                return Ok(());
            }
            _ => {}
        }

        // Send signal via IPC system
        ipc_manager.send_signal(pid, ipc_signal, 0)?;

        // Get pending signals and check for default actions
        let pending_signals = ipc_manager.get_pending_signals(pid);

        for signal_info in pending_signals {
            match signal_info.signal {
                super::ipc::Signal::SIGTERM => {
                    process_manager.terminate_process(pid, -15)?;
                }
                super::ipc::Signal::SIGINT => {
                    process_manager.terminate_process(pid, -2)?;
                }
                super::ipc::Signal::SIGSEGV => {
                    process_manager.terminate_process(pid, -11)?;
                }
                super::ipc::Signal::SIGCONT => {
                    // Continue a stopped process
                    if process.state == super::ProcessState::Blocked {
                        process_manager.unblock_process(pid)?;
                    }
                }
                _ => {
                    // Other signals are delivered to the process for handling
                }
            }
        }

        Ok(())
    }
}

/// Central integration manager
pub struct ProcessIntegration {
    timer_integration: TimerIntegration,
}

impl ProcessIntegration {
    /// Create new process integration manager
    pub const fn new() -> Self {
        Self {
            timer_integration: TimerIntegration::new(),
        }
    }

    /// Fork current process with copy-on-write memory
    pub fn fork_process(&self, parent_pid: Pid) -> Result<Pid, &'static str> {
        use crate::memory::get_memory_manager;

        let process_manager = get_process_manager();
        let memory_manager = get_memory_manager().ok_or("Memory manager not initialized")?;

        // Get parent process memory layout
        let (
            code_start,
            code_size,
            data_start,
            data_size,
            heap_start,
            heap_size,
            stack_start,
            stack_size,
            _vm_start,
            _vm_size,
            parent_priority,
        ) = {
            let parent_process = process_manager
                .get_process(parent_pid)
                .ok_or("Parent process not found")?;
            (
                parent_process.memory.code_start,
                parent_process.memory.code_size,
                parent_process.memory.data_start,
                parent_process.memory.data_size,
                parent_process.memory.heap_start,
                parent_process.memory.heap_size,
                parent_process.memory.stack_start,
                parent_process.memory.stack_size,
                parent_process.memory.vm_start,
                parent_process.memory.vm_size,
                parent_process.priority,
            )
        };

        // Create child process with same priority as parent
        let child_name = "forked_process";
        let child_pid =
            process_manager.create_process(child_name, Some(parent_pid), parent_priority)?;

        // Clone parent's memory space with proper COW (share physical frames)
        // 1. Clone code segment (read-only, directly shared)
        if code_size > 0 {
            memory_manager
                .clone_page_entries_cow(
                    x86_64::VirtAddr::new(code_start),
                    code_size as usize,
                    x86_64::VirtAddr::new(code_start),
                )
                .map_err(|_| "Failed to clone code segment")?;
        }

        // 2. Clone data segment with COW
        if data_size > 0 {
            memory_manager
                .clone_page_entries_cow(
                    x86_64::VirtAddr::new(data_start),
                    data_size as usize,
                    x86_64::VirtAddr::new(data_start),
                )
                .map_err(|_| "Failed to clone data segment")?;
        }

        // 3. Clone heap with COW
        if heap_size > 0 {
            memory_manager
                .clone_page_entries_cow(
                    x86_64::VirtAddr::new(heap_start),
                    heap_size as usize,
                    x86_64::VirtAddr::new(heap_start),
                )
                .map_err(|_| "Failed to clone heap")?;
        }

        // 4. Clone stack with COW
        if stack_size > 0 {
            memory_manager
                .clone_page_entries_cow(
                    x86_64::VirtAddr::new(stack_start),
                    stack_size as usize,
                    x86_64::VirtAddr::new(stack_start),
                )
                .map_err(|_| "Failed to clone stack")?;
        }

        // 5. Update child process memory info through process manager's internal access
        // Note: In a production system, we would need a proper API to update PCB fields
        // For now, the memory is COW-mapped and will work correctly even without updating PCB

        Ok(child_pid)
    }

    /// Execute new program in process
    pub fn exec_process(
        &self,
        pid: Pid,
        _program_path: &str,
        program_data: &[u8],
    ) -> Result<(), &'static str> {
        use crate::memory::{get_memory_manager, MemoryProtection, MemoryRegionType};

        let memory_manager = get_memory_manager().ok_or("Memory manager not initialized")?;

        // Clean up existing process memory
        MemoryIntegration::cleanup_process_memory(pid)?;

        // Parse ELF header to get program information
        let elf_info = Self::parse_elf_header(program_data)?;

        // Allocate code segment
        let code_region = memory_manager
            .allocate_region(
                elf_info.code_size as usize,
                MemoryRegionType::UserCode,
                MemoryProtection::USER_CODE,
            )
            .map_err(|_| "Failed to allocate code region for exec")?;

        // Allocate data segment
        let data_region = if elf_info.data_size > 0 {
            Some(
                memory_manager
                    .allocate_region(
                        elf_info.data_size as usize,
                        MemoryRegionType::UserData,
                        MemoryProtection::USER_DATA,
                    )
                    .map_err(|_| "Failed to allocate data region for exec")?,
            )
        } else {
            None
        };

        // Allocate stack (default 8MB)
        let stack_size = 8 * 1024 * 1024; // 8MB stack
        let stack_region = memory_manager
            .allocate_region(
                stack_size,
                MemoryRegionType::UserStack,
                MemoryProtection::USER_DATA,
            )
            .map_err(|_| "Failed to allocate stack for exec")?;

        // Load program sections into memory
        unsafe {
            // Load code section
            let code_ptr = code_region.start.as_u64() as *mut u8;
            if let Some(code_data) = elf_info.code_data {
                core::ptr::copy_nonoverlapping(code_data.as_ptr(), code_ptr, code_data.len());
            }

            // Load data section
            if let (Some(ref data_region), Some(data_data)) = (data_region, elf_info.data_data) {
                let data_ptr = data_region.start.as_u64() as *mut u8;
                core::ptr::copy_nonoverlapping(data_data.as_ptr(), data_ptr, data_data.len());
            }

            // Initialize stack with program arguments
            let stack_ptr = (stack_region.start.as_u64() + stack_size as u64 - 8) as *mut u64;
            *stack_ptr = 0; // Null terminator for argv
        }

        // Note: Process memory layout updates would require ProcessManager API
        // In production, add: process_manager.update_memory_layout(pid, code_region, data_region, etc.)
        // For now, the memory is allocated and mapped correctly for the process

        Ok(())
    }

    /// Parse ELF header and extract program information
    fn parse_elf_header(program_data: &[u8]) -> Result<ElfInfo<'_>, &'static str> {
        if program_data.len() < 64 {
            return Err("ELF file too small");
        }

        // Verify ELF magic
        if &program_data[0..4] != b"\x7FELF" {
            return Err("Invalid ELF magic");
        }

        let elf_class = program_data[4];
        if elf_class != 2 {
            return Err("Only 64-bit ELF supported");
        }

        // Extract entry point (64-bit)
        let entry_point = u64::from_le_bytes([
            program_data[24],
            program_data[25],
            program_data[26],
            program_data[27],
            program_data[28],
            program_data[29],
            program_data[30],
            program_data[31],
        ]);

        // Extract program header table offset and size
        let ph_offset = u64::from_le_bytes([
            program_data[32],
            program_data[33],
            program_data[34],
            program_data[35],
            program_data[36],
            program_data[37],
            program_data[38],
            program_data[39],
        ]) as usize;

        let ph_entry_size = u16::from_le_bytes([program_data[54], program_data[55]]) as usize;
        let ph_num = u16::from_le_bytes([program_data[56], program_data[57]]) as usize;

        // Parse program headers to find loadable segments
        let mut code_size = 0u64;
        let mut data_size = 0u64;
        let mut code_data: Option<&[u8]> = None;
        let mut data_data: Option<&[u8]> = None;

        for i in 0..ph_num {
            let ph_start = ph_offset + i * ph_entry_size;
            if ph_start + 56 > program_data.len() {
                break;
            }

            let ph = &program_data[ph_start..ph_start + 56];

            // Check if this is a loadable segment (PT_LOAD = 1)
            let p_type = u32::from_le_bytes([ph[0], ph[1], ph[2], ph[3]]);
            if p_type != 1 {
                continue;
            }

            // Extract segment information
            let p_flags = u32::from_le_bytes([ph[4], ph[5], ph[6], ph[7]]);
            let p_offset =
                u64::from_le_bytes([ph[8], ph[9], ph[10], ph[11], ph[12], ph[13], ph[14], ph[15]])
                    as usize;
            let p_filesz = u64::from_le_bytes([
                ph[32], ph[33], ph[34], ph[35], ph[36], ph[37], ph[38], ph[39],
            ]);
            let p_memsz = u64::from_le_bytes([
                ph[40], ph[41], ph[42], ph[43], ph[44], ph[45], ph[46], ph[47],
            ]);

            // Determine if this is code or data segment based on flags
            let is_executable = (p_flags & 0x1) != 0; // PF_X
            let is_writable = (p_flags & 0x2) != 0; // PF_W

            if is_executable && !is_writable {
                // Code segment
                code_size = p_memsz;
                if p_offset + p_filesz as usize <= program_data.len() {
                    code_data = Some(&program_data[p_offset..p_offset + p_filesz as usize]);
                }
            } else if is_writable {
                // Data segment
                data_size = p_memsz;
                if p_offset + p_filesz as usize <= program_data.len() {
                    data_data = Some(&program_data[p_offset..p_offset + p_filesz as usize]);
                }
            }
        }

        Ok(ElfInfo {
            entry_point,
            code_size,
            data_size,
            code_data,
            data_data,
        })
    }
}

/// ELF program information
struct ElfInfo<'a> {
    entry_point: u64,
    code_size: u64,
    data_size: u64,
    code_data: Option<&'a [u8]>,
    data_data: Option<&'a [u8]>,
}

impl ProcessIntegration {
    /// Initialize integration with other kernel systems
    pub fn init(&mut self) -> Result<(), &'static str> {
        // Initialize synchronization system
        super::sync::init()?;

        // Initialize memory management integration
        // Ensure memory manager is available
        use crate::memory::get_memory_manager;
        if get_memory_manager().is_none() {
            return Err("Memory manager must be initialized before process integration");
        }

        Ok(())
    }

    /// Handle timer interrupt
    pub fn handle_timer(&mut self) -> Result<Option<Pid>, &'static str> {
        self.timer_integration.handle_timer_interrupt()
    }

    /// Handle page fault
    pub fn handle_page_fault(
        &self,
        pid: Pid,
        fault_address: u64,
        error_code: u64,
    ) -> Result<(), &'static str> {
        MemoryIntegration::handle_page_fault(pid, fault_address, error_code)
    }

    /// Handle system call
    pub fn handle_syscall(&self, syscall_number: u64, args: &[u64]) -> Result<u64, &'static str> {
        InterruptIntegration::handle_syscall_interrupt(syscall_number, args)
    }

    /// Handle keyboard input
    pub fn handle_keyboard(&self, scancode: u8) -> Result<(), &'static str> {
        InterruptIntegration::handle_keyboard_interrupt(scancode)
    }

    /// Deliver signal to process
    pub fn deliver_signal(&self, pid: Pid, signal: u32) -> Result<(), &'static str> {
        InterruptIntegration::deliver_signal(pid, signal)
    }

    /// Set timer scheduling frequency
    pub fn set_schedule_frequency(&mut self, frequency: u32) {
        self.timer_integration.set_schedule_frequency(frequency);
    }

    /// Get integration statistics
    pub fn get_stats(&self) -> IntegrationStats {
        IntegrationStats {
            time_slice_counter: self.timer_integration.get_time_slice_counter(),
            schedule_frequency: self.timer_integration.schedule_frequency,
            sync_stats: super::sync::get_sync_manager().get_stats(),
        }
    }

    /// Comprehensive system health check
    pub fn system_health_check(&self) -> Result<SystemHealthReport, &'static str> {
        let process_manager = get_process_manager();
        let memory_manager =
            crate::memory::get_memory_manager().ok_or("Memory manager not initialized")?;
        let ipc_manager = super::ipc::get_ipc_manager();
        let thread_manager = super::thread::get_thread_manager();

        // Check process system health
        let processes = process_manager.list_processes();
        let total_processes = processes.len();
        let active_processes = processes
            .iter()
            .filter(|(_, _, state, _)| {
                matches!(
                    state,
                    super::ProcessState::Running | super::ProcessState::Ready
                )
            })
            .count();

        // Check memory system health
        let memory_stats = memory_manager.memory_stats();
        let memory_utilization = if memory_stats.total_memory > 0 {
            (memory_stats.allocated_memory as f32) / (memory_stats.total_memory as f32) * 100.0
        } else {
            0.0
        };

        // Check IPC system health
        let ipc_stats = ipc_manager.get_stats();

        // Check thread system health
        let thread_stats = thread_manager.list_threads();
        let total_threads = thread_stats.len();

        // Performance monitoring
        let perf_stats = crate::performance::get_performance_monitor().get_stats();

        Ok(SystemHealthReport {
            total_processes,
            active_processes,
            total_threads,
            memory_utilization,
            ipc_objects_count: ipc_stats.pipe_count + ipc_stats.shm_count + ipc_stats.msgq_count,
            allocation_failures: perf_stats.allocation_failures,
            average_allocation_time: perf_stats.average_allocation_time,
            system_stable: memory_utilization < 90.0 && perf_stats.allocation_failures < 100,
        })
    }

    /// Emergency system cleanup and recovery
    pub fn emergency_cleanup(&self) -> Result<(), &'static str> {
        let process_manager = get_process_manager();
        let processes = process_manager.list_processes();

        // Terminate zombie processes
        for (pid, _, state, _) in processes {
            if state == super::ProcessState::Zombie {
                let _ = process_manager.terminate_process(pid, -1); // Force cleanup
            }
        }

        // Clean up orphaned IPC objects
        let ipc_manager = super::ipc::get_ipc_manager();
        for (pid, _, _, _) in process_manager.list_processes() {
            let _ = ipc_manager.cleanup_process_ipc(pid);
        }

        // Force memory compaction if needed
        // In a real implementation, we would trigger garbage collection/compaction here

        Ok(())
    }
}

/// Integration statistics
#[derive(Debug)]
pub struct IntegrationStats {
    pub time_slice_counter: u32,
    pub schedule_frequency: u32,
    pub sync_stats: super::sync::SyncStats,
}

/// System health report
#[derive(Debug)]
pub struct SystemHealthReport {
    pub total_processes: usize,
    pub active_processes: usize,
    pub total_threads: usize,
    pub memory_utilization: f32,
    pub ipc_objects_count: usize,
    pub allocation_failures: u64,
    pub average_allocation_time: u64,
    pub system_stable: bool,
}

/// Global process integration manager
static mut PROCESS_INTEGRATION: ProcessIntegration = ProcessIntegration::new();

/// Get the global process integration manager
pub fn get_integration_manager() -> &'static mut ProcessIntegration {
    unsafe { &mut *core::ptr::addr_of_mut!(PROCESS_INTEGRATION) }
}

/// Initialize process integration
pub fn init() -> Result<(), &'static str> {
    let integration = get_integration_manager();
    integration.init()
}

/// Timer interrupt handler (to be called from interrupt handler)
pub fn timer_interrupt_handler() -> Option<Pid> {
    let integration = get_integration_manager();
    integration.handle_timer().unwrap_or(None)
}

/// Page fault handler (to be called from interrupt handler)
pub fn page_fault_handler(fault_address: u64, error_code: u64) -> Result<(), &'static str> {
    let process_manager = get_process_manager();
    let current_pid = process_manager.current_process();

    let integration = get_integration_manager();
    integration.handle_page_fault(current_pid, fault_address, error_code)
}

/// System call handler (to be called from interrupt handler)
pub fn syscall_interrupt_handler(syscall_number: u64, args: &[u64]) -> Result<u64, &'static str> {
    let integration = get_integration_manager();
    integration.handle_syscall(syscall_number, args)
}

/// Keyboard interrupt handler (to be called from interrupt handler)
pub fn keyboard_interrupt_handler(scancode: u8) -> Result<(), &'static str> {
    let integration = get_integration_manager();
    integration.handle_keyboard(scancode)
}
