#![no_std]
#![no_main]
#![allow(static_mut_refs)]
#![feature(abi_x86_interrupt)]
#![feature(custom_test_frameworks)]
#![test_runner(crate::test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate alloc;

use bootloader::{entry_point, BootInfo};
use core::alloc::{GlobalAlloc, Layout};
use core::panic::PanicInfo;
use linked_list_allocator::LockedHeap;

// Heap allocator — registered with glib-native's delegating #[global_allocator]
// via `glib_native::set_allocator` during early boot.
pub static ALLOCATOR: LockedHeap = LockedHeap::empty();

// Allocator wrapper functions for glib-native delegation.
unsafe fn k_alloc(layout: Layout) -> *mut u8 {
    unsafe { ALLOCATOR.alloc(layout) }
}
unsafe fn k_dealloc(ptr: *mut u8, layout: Layout) {
    unsafe { ALLOCATOR.dealloc(ptr, layout) }
}
unsafe fn k_realloc(ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
    unsafe { ALLOCATOR.realloc(ptr, layout, new_size) }
}
unsafe fn k_alloc_zeroed(layout: Layout) -> *mut u8 {
    unsafe { ALLOCATOR.alloc_zeroed(layout) }
}

// Include compiler intrinsics for missing symbols (memcpy/memset/memcmp/memmove)
mod intrinsics;

// Linux rust/kernel/ ports (memory management + I/O abstractions)
mod dma; // DmaCoherent, dma_sync_*, ioremap, DmaPool
mod io; // MMIO r/w, port I/O, memory barriers, IoMem, IoRegister
mod iov; // IoVec, IovIter, import_iovec
mod kalloc; // AllocFlags, GFP_*, kmalloc/kfree/kzalloc/krealloc/vmalloc
mod linux_rust;
mod page; // page constants, Page, PageRange, BorrowedPage
mod scatterlist; // ScatterList, SgTable, DmaDirection
mod uaccess; // UserPtr, UserSlice, copy_from/to_user // Linux rust/kernel/ utility ports (sizes, bits, ioctl, bitmap, bitfield, etc.)

// Include VGA buffer module for better output
mod vga_buffer;
// Include print module for print! and println! macros
mod print;
// Include basic memory management
mod memory_basic;
// Include full memory management
mod memory;
// Include filesystem
mod fs;
// Include visual boot display
mod boot_display;
// Include enhanced boot UI with progress indicators
mod boot_ui;
// Include keyboard input handler
mod keyboard;
// Include desktop environment
mod simple_desktop;
// Include limited VGA diagnostic fallback.
mod vga_mode13h;
// Include graphics system
mod graphics;
// Include GPU support
mod gpu;
// Include data structures
mod data_structures;
// Include advanced desktop environment
mod desktop;
// Include serial port driver
mod serial;
// Include time management system
mod time;
// Include GDT (Global Descriptor Table)
mod gdt;
// Include interrupt handling
mod interrupts;
// Include ACPI support
mod acpi;
// Include APIC support
mod apic;
// Include architecture-specific code
mod arch;
// Include SMP (multiprocessor) support
mod smp;
// Include PCI bus support
mod pci;
// Include drivers
mod drivers;
// Include ALSA-style sound device registry
mod sound;
// Include network stack
pub mod net;
// Re-export network module with alternative name for compatibility
pub use net as network;
// Include security
mod security;
// Kernel crypto subsystem (Linux crypto/algapi style)
mod crypto;
// Include IPC
mod ipc;
// Include kernel core
mod kernel;
// Include event notifier chains
mod notifier;
// Include usermodehelper (kernel-spawned userspace programs)
mod usermodehelper;
// Include process management
mod process;
// Include POSIX signal subsystem
mod signal;
// Include process manager (high-level process APIs)
mod process_manager;
// Include scheduler
mod scheduler;
// Include error handling and recovery system
mod error;
// Include system health monitoring
mod health;
// Include comprehensive logging and debugging
mod debug;
mod logging;
// Include comprehensive testing framework
mod testing;
// Include testing framework core (used by testing module)
mod testing_framework;
// Include I/O optimization and scheduling system
mod io_optimized;
// Include performance monitoring
mod performance;
mod performance_monitor;
// Include experimental package management system
mod package;
// Include Linux API compatibility layer
mod linux_compat;
// Include Linux integration layer
mod linux_integration;
// Include memory manager for virtual memory management
mod memory_manager;
// Include VFS and initramfs for Linux userspace
mod initramfs;
mod kernel_cmdline;
mod sysfs;
mod vfs;
// Include ELF loader for binary execution
mod elf_loader;
// Include syscall system
mod syscall;
// Include syscall handler for INT 0x80
mod syscall_handler;
// Include fast syscall support (SYSCALL/SYSRET)
mod syscall_fast;
// Include usermode helper module
mod usermode;
// Include usermode testing module
// Include GLib compatibility layer
mod glib;
mod glib_platform;
mod glib_spawn;
mod gnome;
mod gnome_overlay;
mod installer;
mod mutter;
mod mutter_bridge;
mod mutter_port; // TEMP: build-verification only // bridges framebuffer::Rect <-> mutter_port::mtk::Rectangle
                 // Include GNOME foundation subsystems
mod dbus;
mod user_sched;
mod wayland;
// Include SoftIRQ and workqueue subsystem (deferred work, interrupt bottom halves)
mod softirq;
// Kernel thread (kthread) subsystem — mirrors Linux kernel/kthread.c
mod kthread;
// Full Linux-compatible workqueue subsystem (work_struct, delayed_work, named WQs)
mod workqueue;
// Locking primitives — mutex, rwsem, semaphore, rtmutex, completion
mod locking;
// Include futex (fast userspace mutexes)
mod futex;
// Include epoll (I/O event multiplexing)
mod epoll;
// Include OOM killer (out-of-memory handling)
mod oom;
// Include swap subsystem (paging to backing store)
mod swap;
// Include block I/O layer (generic block device abstraction)
mod block_io;
// Partition table parsing (MBR/GPT), consumed by block_io to expose
// per-partition block devices.
mod block_partition;
// Include cgroups (resource control groups)
mod cgroup;
// Include seccomp (secure computing mode - syscall filtering)
mod seccomp;
// Include namespaces (PID, mount, network, UTS, IPC, user, cgroup isolation)
mod namespace;
// Include ptrace (process tracing and debugging)
mod ptrace;
// Include inotify (filesystem event monitoring)
mod inotify;
// Include pidfd (process file descriptors)
mod pidfd;
// Include io_uring (asynchronous I/O submission/completion rings)
mod io_uring;
// Include fanotify (advanced filesystem event monitoring)
mod fanotify;
// Include mount_api (new mount API: fsopen, fsconfig, fsmount, fspick, move_mount)
mod mount_api;
// Include disk quota (per-mount block/inode limits and quotactl)
mod quota;
// Include landlock (unprivileged sandboxing via access control rulesets)
mod landlock;
// Include bpf (eBPF program and map management)
mod bpf;
mod file_handle;
mod hugetlb;
mod kexec;
mod memfd_secret;
mod perf_event;
mod power;
mod privileged_syscalls;
mod process_vm;
mod rseq;
mod userfaultfd;
// Include keyring (kernel key management subsystem)
mod keyring;
// Include sysv_ipc (System V IPC: semaphores, shared memory, message queues)
mod sysv_ipc;
// Include aio (POSIX asynchronous I/O)
mod aio;
// Include module_loader (kernel module loading with ELF relocation)
mod module_loader;
// Linux audit subsystem (syscall/path event logging)
mod audit;
// ftrace / tracepoints framework
mod md;
mod trace;
// Kernel probes (kprobes)
mod kprobes;
// RCU, cpufreq/cpuidle, NUMA, livepatch, EDAC, MFD, NVDIMM
mod cpufreq;
mod cpuidle;
mod edac;
mod efi;
mod irq_domain;
mod kasan;
mod kcsan;
mod livepatch;
mod memory_hotplug;
mod mfd;
mod numa;
mod nvdimm;
mod of;
mod rcu;
mod thp;

// VGA_WRITER is now used via macros in print module

// Print macros
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::print::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

entry_point!(kernel_main);

pub trait Testable {
    fn run(&self);
}

impl<T> Testable for T
where
    T: Fn(),
{
    fn run(&self) {
        serial_print!("{}...\t", core::any::type_name::<T>());
        self();
        serial_println!("[ok]");
    }
}

pub fn test_runner(tests: &[&dyn Testable]) {
    serial_println!("Running {} tests", tests.len());
    for test in tests {
        test.run();
    }
    exit_qemu(QemuExitCode::Success);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum QemuExitCode {
    Success = 0x10,
    Failed = 0x11,
}

pub fn exit_qemu(exit_code: QemuExitCode) {
    use x86_64::instructions::port::Port;

    unsafe {
        let mut port = Port::new(0xf4);
        port.write(exit_code as u32);
    }
}

// Early serial output functions for debugging
/// Safety: Performs raw port I/O to COM1. Caller must ensure I/O ports are valid.
/// See docs/SAFETY.md#io-port-access.
unsafe fn init_early_serial() {
    let port = 0x3f8; // COM1
                      // Disable interrupts
    outb(port + 1, 0x00);
    // Enable DLAB
    outb(port + 3, 0x80);
    // Set divisor (38400 baud)
    outb(port + 0, 0x03);
    outb(port + 1, 0x00);
    // 8 bits, no parity, one stop bit
    outb(port + 3, 0x03);
    // Enable FIFO
    outb(port + 2, 0xc7);
    // Enable interrupts
    outb(port + 4, 0x0b);
}

/// Safety: Raw port write; caller must ensure the port is valid for the platform.
/// See docs/SAFETY.md#io-port-access.
unsafe fn outb(port: u16, value: u8) {
    core::arch::asm!("out dx, al", in("dx") port, in("al") value);
}

/// Safety: Raw port read; caller must ensure the port is valid for the platform.
/// See docs/SAFETY.md#io-port-access.
unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    core::arch::asm!("in al, dx", out("al") value, in("dx") port);
    value
}

/// Safety: Requires initialized COM1 and valid I/O access.
/// See docs/SAFETY.md#io-port-access.
pub(crate) unsafe fn early_serial_write_byte(byte: u8) {
    let port = 0x3f8;
    // Wait for transmit to be ready
    while (inb(port + 5) & 0x20) == 0 {}
    outb(port, byte);
}

/// Safety: Requires initialized COM1 and valid I/O access.
/// See docs/SAFETY.md#io-port-access.
pub unsafe fn early_serial_write_str(s: &str) {
    for byte in s.bytes() {
        early_serial_write_byte(byte);
    }
}

/// Write bytes to early serial output.
fn early_serial_write_bytes(bytes: &[u8]) {
    // SAFETY: Raw I/O to COM1 for early logging. See docs/SAFETY.md#io-port-access.
    unsafe {
        for &byte in bytes {
            early_serial_write_byte(byte);
        }
    }
}

/// Write a decimal u64 to early serial output.
pub fn early_serial_write_u64(mut value: u64) {
    let mut buf = [0u8; 20];
    let mut i = buf.len();

    if value == 0 {
        early_serial_write_bytes(b"0");
        return;
    }

    while value > 0 {
        i -= 1;
        buf[i] = b'0' + (value % 10) as u8;
        value /= 10;
    }

    early_serial_write_bytes(&buf[i..]);
}

/// Write a hex u64 to early serial output.
pub fn early_serial_write_hex(value: u64) {
    early_serial_write_bytes(b"0x");
    if value == 0 {
        early_serial_write_bytes(b"0");
        return;
    }
    let mut buf = [0u8; 16];
    let mut i = buf.len();
    let mut v = value;
    while v > 0 {
        i -= 1;
        let nibble = (v & 0xF) as u8;
        buf[i] = if nibble < 10 {
            b'0' + nibble
        } else {
            b'A' + nibble - 10
        };
        v >>= 4;
    }
    early_serial_write_bytes(&buf[i..]);
}

fn boot_info_summary(boot_info: &BootInfo) -> (u64, u64, usize) {
    let mut total: u64 = 0;
    let mut usable: u64 = 0;
    let mut regions: usize = 0;

    for region in boot_info.memory_map.iter() {
        regions += 1;
        let size = region.range.end_addr() - region.range.start_addr();
        total += size;

        if region.region_type == bootloader::bootinfo::MemoryRegionType::Usable {
            usable += size;
        }
    }

    (total, usable, regions)
}

fn kernel_main(boot_info: &'static BootInfo) -> ! {
    // Enable SSE before ANY other kernel code. rustc emits SSE (`xorps`/`movaps`)
    // to zero stack buffers even in plain integer functions, so the first such
    // function (early_serial_write_u64) executes an SSE op. The bootloader hands
    // off with SSE disabled, so without this the first SSE instruction raises #UD
    // and — with no IDT yet — escalates to a triple fault. Raw asm only (no SSE).
    // SAFETY: writing CR0/CR4 to enable SSE per the x86_64 spec.
    unsafe {
        core::arch::asm!(
            "mov rax, cr0",
            "and ax, 0xFFFB",       // clear CR0.EM (no FPU emulation)
            "or ax, 0x2",           // set CR0.MP (monitor coprocessor)
            "mov cr0, rax",
            "mov rax, cr4",
            "or ax, 0x600",         // set CR4.OSFXSR | CR4.OSXMMEXCPT
            "mov cr4, rax",
            out("rax") _,
            options(nostack, preserves_flags),
        );
    }

    // Initialize early serial output for debugging
    // SAFETY: Raw I/O to COM1 for early logging. See docs/SAFETY.md#io-port-access.
    unsafe {
        init_early_serial();
        early_serial_write_str("RustOS: Kernel entry point reached!\r\n");
    }
    // SAFETY: BootInfo provided by bootloader entry. See docs/SAFETY.md#bootinfo-use.
    let (total_bytes, usable_bytes, regions) = boot_info_summary(boot_info);
    early_serial_write_bytes(b"RustOS: BootInfo memory map regions=");
    early_serial_write_u64(regions as u64);
    early_serial_write_bytes(b", total=");
    early_serial_write_u64(total_bytes / (1024 * 1024));
    early_serial_write_bytes(b"MiB, usable=");
    early_serial_write_u64(usable_bytes / (1024 * 1024));
    early_serial_write_bytes(b"MiB\r\n");

    // Write directly to VGA buffer without any initialization to test if kernel is running
    // SAFETY: Direct write to VGA text buffer at 0xB8000 within bounds.
    // See docs/SAFETY.md#vga-text-buffer.
    unsafe {
        let vga_buffer = 0xb8000 as *mut u8;
        let message = b"KERNEL STARTED!";
        for (i, &byte) in message.iter().enumerate() {
            *vga_buffer.offset(i as isize * 2) = byte;
            *vga_buffer.offset(i as isize * 2 + 1) = 0x0f; // White on black
        }
        early_serial_write_str("RustOS: VGA buffer initialized\r\n");
    }

    // Initialize VGA buffer for text mode display
    vga_buffer::init();
    // SAFETY: Raw I/O to COM1 for early logging. See docs/SAFETY.md#io-port-access.
    unsafe {
        early_serial_write_str("RustOS: VGA buffer system initialized\r\n");
    }

    // Display boot logo and welcome message in text mode
    boot_display::show_boot_logo();
    boot_display::show_welcome_message();
    boot_display::show_kernel_version();

    // ========================================================================
    // CRITICAL: Register allocator and panic handler with glib-native, then
    // initialize heap allocator BEFORE any alloc usage
    // ========================================================================
    // SAFETY: Raw I/O to COM1 for early logging. See docs/SAFETY.md#io-port-access.
    unsafe {
        early_serial_write_str("RustOS: Registering allocator with glib-native...\r\n");
    }

    // Register the kernel's allocator and panic handler with glib-native's
    // delegating #[global_allocator] and #[panic_handler].
    glib_native::set_allocator(k_alloc, k_dealloc, k_realloc, k_alloc_zeroed);
    #[cfg(not(test))]
    glib_native::set_panic_handler(kernel_panic);
    #[cfg(test)]
    glib_native::set_panic_handler(test_panic);

    // SAFETY: Raw I/O to COM1 for early logging. See docs/SAFETY.md#io-port-access.
    unsafe {
        early_serial_write_str("RustOS: Initializing heap allocator from memory map...\r\n");
    }

    // Get physical memory offset from bootloader (requires map_physical_memory feature)
    let phys_mem_offset = boot_info.physical_memory_offset;

    // Initialize the kernel heap using bootloader's memory map - MUST happen before any String/Vec/Box usage
    if let Err(_e) = memory_basic::init_heap_from_memory_map(
        &ALLOCATOR,
        boot_info.memory_map.iter().as_slice(),
        phys_mem_offset,
    ) {
        unsafe {
            early_serial_write_str("RustOS: FATAL - Heap initialization failed!\r\n");
        }
        loop {
            unsafe {
                core::arch::asm!("hlt");
            }
        }
    }

    // SAFETY: Raw I/O to COM1 for early logging. See docs/SAFETY.md#io-port-access.
    unsafe {
        early_serial_write_str("RustOS: Heap allocator ready\r\n");
    }

    // Register kernel subsystems for init-order tracking and dependency checks.
    match kernel::init() {
        Ok(()) => unsafe {
            // notifier::init() is called from kernel::init(); mark it ready here.
            kernel::mark_subsystem_ready("notifier");
            early_serial_write_str("RustOS: Kernel subsystem registry initialized\r\n");
        },
        Err(e) => unsafe {
            kernel::mark_subsystem_failed("notifier");
            early_serial_write_str("RustOS: Kernel subsystem registry FAILED: ");
            early_serial_write_str(e);
            early_serial_write_str("\r\n");
        },
    }

    // Early security init — seed the RNG so arch setup (ASLR, stack canaries)
    // has access to random numbers.  Mirrors Linux's early_security_init().
    security::early_init();

    // Initialize CPU architecture detection (CPUID vendor, brand, features).
    // Needs heap for String allocation. performance::init() depends on this.
    match arch::init() {
        Ok(()) => {
            kernel::mark_subsystem_ready("arch");
            unsafe {
                early_serial_write_str("RustOS: CPU architecture detected\r\n");
            }
        }
        Err(e) => {
            kernel::mark_subsystem_failed("arch");
            unsafe {
                early_serial_write_str("RustOS: CPU arch detection FAILED: ");
                early_serial_write_str(e);
                early_serial_write_str("\r\n");
            }
        }
    }

    // Initialize performance optimizations (caches CPU features for fast paths).
    match performance::init() {
        Ok(()) => unsafe {
            early_serial_write_str("RustOS: Performance optimizations initialized\r\n");
        },
        Err(e) => unsafe {
            early_serial_write_str("RustOS: Performance init FAILED: ");
            early_serial_write_str(e);
            early_serial_write_str("\r\n");
        },
    }

    // Initialize syscall VFS then GLib (platform hooks need VFS)
    match crate::vfs::init() {
        Ok(()) => kernel::mark_subsystem_ready("vfs"),
        Err(_) => kernel::mark_subsystem_failed("vfs"),
    }
    glib::init_glib_logging();
    glib::init_glib_platform();
    unsafe {
        early_serial_write_str("RustOS: GLib logging initialized\r\n");
    }
    unsafe {
        early_serial_write_str("RustOS: GLib native smoke check deferred to userspace\r\n");
    }

    #[cfg(test)]
    {
        let _ = security::init_rng();
        test_main();
        loop {
            unsafe {
                core::arch::asm!("hlt");
            }
        }
    }

    #[cfg(not(test))]
    {
        // Set physical memory offset for VGA Mode 13h graphics
        // This allows the VGA driver to access the framebuffer at 0xA0000
        vga_mode13h::set_phys_mem_offset(phys_mem_offset);
        unsafe {
            early_serial_write_str("RustOS: VGA physical memory offset configured\r\n");
        }

        // ========================================================================
        // EARLY GRAPHICS INIT: Initialize VBE framebuffer before boot splash
        // so the entire boot sequence uses a graphical splash screen instead of
        // text-mode VGA. Falls back to text mode if display init fails.
        // ========================================================================
        let mut early_graphics_result = boot_ui::GraphicsInitResult::new();
        let mut early_display_ready = false;

        {
            let bc = boot_ui::boot_config();
            if !bc.force_text_mode && !bc.safe_mode {
                match drivers::display::init(phys_mem_offset) {
                    Ok(mode) => {
                        crate::serial_println!(
                            "display: {}x{}x{} initialized (early)",
                            mode.width,
                            mode.height,
                            mode.bpp
                        );
                        early_graphics_result.framebuffer_ready = true;
                        early_graphics_result.width = mode.width;
                        early_graphics_result.height = mode.height;
                        early_graphics_result.bpp = mode.bpp as u16;
                        early_graphics_result.output_verified = true;
                        gnome::mark_boot_graphics_ready();
                        early_display_ready = true;
                        kernel::mark_subsystem_ready("graphics");
                    }
                    Err(e) => {
                        crate::serial_println!("display: early init failed: {}", e);
                        kernel::mark_subsystem_failed("graphics");
                    }
                }
            }
        }

        // Record boot start time (after basic init)
        let _boot_start_time = 0u64; // Will use time::uptime_ms() after time init

        // SAFETY: Debug output
        unsafe {
            early_serial_write_str("RustOS: About to show boot splash...\r\n");
        }

        // ========================================================================
        // PHASE 1: Boot Splash (graphical if framebuffer is ready, text fallback)
        // ========================================================================
        if early_display_ready {
            boot_ui::show_graphical_splash();
        } else {
            boot_ui::show_boot_splash();
        }

        // SAFETY: Debug output
        unsafe {
            early_serial_write_str("RustOS: Boot splash complete, doing delay...\r\n");
        }

        boot_ui::boot_delay_medium();

        // SAFETY: Debug output
        unsafe {
            early_serial_write_str("RustOS: Delay complete\r\n");
        }

        // ========================================================================
        // PHASE 2: Hardware Detection
        // ========================================================================
        // SAFETY: Debug output
        unsafe {
            early_serial_write_str("RustOS: Starting hardware detection...\r\n");
        }
        let hardware_result = boot_ui::hardware_detection_progress();
        unsafe {
            early_serial_write_str("RustOS: Hardware detection done\r\n");
        }

        // ========================================================================
        // PHASE 3: ACPI Initialization
        // ========================================================================
        // SAFETY: Debug output
        unsafe {
            early_serial_write_str("RustOS: Starting ACPI phase...\r\n");
        }

        // Note: bootloader v0.9.33 doesn't provide rsdp_addr or physical_memory_offset
        // We'll use manual ACPI detection and a default physical offset
        let physical_memory_offset = x86_64::VirtAddr::new(phys_mem_offset);
        let _acpi_result = {
            unsafe {
                early_serial_write_str("RustOS: ACPI begin_stage...\r\n");
            }
            boot_ui::begin_stage(boot_ui::BootStage::AcpiInit, 1);
            unsafe {
                early_serial_write_str("RustOS: ACPI report_warning...\r\n");
            }
            boot_ui::report_warning("ACPI", "Using manual ACPI detection");
            unsafe {
                early_serial_write_str("RustOS: ACPI complete_stage...\r\n");
            }
            boot_ui::complete_stage(boot_ui::BootStage::AcpiInit);
            // Try ACPI initialization with manual detection
            unsafe {
                early_serial_write_str("RustOS: ACPI init_progress...\r\n");
            }
            boot_ui::acpi_init_progress(None, physical_memory_offset.as_u64())
        };
        if _acpi_result.tables_parsed {
            kernel::mark_subsystem_ready("acpi");
        } else {
            kernel::mark_subsystem_failed("acpi");
        }
        // SAFETY: Debug output
        unsafe {
            early_serial_write_str("RustOS: ACPI phase complete\r\n");
        }

        // ========================================================================
        // PHASE 4: PCI Bus Enumeration
        // ========================================================================
        // SAFETY: Debug output
        unsafe {
            early_serial_write_str("RustOS: Starting PCI enumeration...\r\n");
        }
        let _pci_result = boot_ui::pci_enum_progress();
        // SAFETY: Debug output
        unsafe {
            early_serial_write_str("RustOS: PCI enumeration done\r\n");
        }

        // ========================================================================
        // PHASE 5: Memory Management Initialization
        // ========================================================================
        // SAFETY: Debug output
        unsafe {
            early_serial_write_str("RustOS: Starting memory management init...\r\n");
        }
        let memory_result =
            boot_ui::memory_init_progress(&boot_info.memory_map, physical_memory_offset);
        // SAFETY: Debug output
        unsafe {
            early_serial_write_str("RustOS: Memory management done\r\n");
        }

        // Initialize the full paging-based memory manager (frame allocator + page table manager).
        // boot_ui::memory_init_progress only analyzes the memory map; this sets up the actual
        // paging infrastructure needed by map_user_page, protect_user_page, mmap, brk, etc.
        unsafe {
            early_serial_write_str("RustOS: Initializing paging memory manager...\r\n");
        }
        // Parse UEFI runtime services if firmware left a discoverable system table.
        efi::init_from_boot_info(boot_info);
        efi::init();
        kernel::mark_subsystem_ready("efi");

        match memory::init_memory_management(
            boot_info.memory_map.iter().as_slice(),
            Some(phys_mem_offset),
        ) {
            Ok(()) => {
                kernel::mark_subsystem_ready("memory");
                unsafe {
                    early_serial_write_str("RustOS: Paging memory manager initialized\r\n");
                }
            }
            Err(e) => {
                kernel::mark_subsystem_failed("memory");
                unsafe {
                    let msg = match e {
                        memory::MemoryError::OutOfMemory => "Out of physical memory",
                        memory::MemoryError::MappingFailed => "Failed to map virtual memory",
                        memory::MemoryError::HeapInitFailed => "Heap initialization failed",
                        memory::MemoryError::InvalidAddress => "Invalid address",
                        _ => "Other memory error",
                    };
                    early_serial_write_str("RustOS: Paging memory manager init FAILED: ");
                    early_serial_write_str(msg);
                    early_serial_write_str("\r\n");
                }
            }
        }

        // Initialize the virtual memory manager (mmap/brk/mprotect support).
        unsafe {
            early_serial_write_str("RustOS: Initializing virtual memory manager...\r\n");
        }
        match memory_manager::init_virtual_memory(physical_memory_offset) {
            Ok(()) => unsafe {
                early_serial_write_str("RustOS: Virtual memory manager initialized\r\n");
            },
            Err(_) => unsafe {
                early_serial_write_str("RustOS: Virtual memory manager init FAILED\r\n");
            },
        }

        // Wire the physical memory offset into the SMP subsystem so that
        // APIC MMIO accesses use the kernel's direct physical mapping
        // instead of identity mapping (which is not present in higher-half
        // kernels and would cause a page fault).
        let phys_offset = memory::get_physical_memory_offset();
        if phys_offset != 0 {
            smp::set_physical_memory_offset(phys_offset);
            unsafe {
                early_serial_write_str("RustOS: SMP physical memory offset set\r\n");
            }
        }

        // Runtime proof that user-page mapping actually backs frames (brk/mmap path).
        match memory::selftest_user_paging() {
            Ok(()) => unsafe {
                early_serial_write_str("RustOS: user-paging self-test PASSED\r\n");
            },
            Err(e) => unsafe {
                early_serial_write_str("RustOS: user-paging self-test FAILED: ");
                early_serial_write_str(e);
                early_serial_write_str("\r\n");
            },
        }

        // ========================================================================
        // PHASE 6: Interrupt and System Setup
        // ========================================================================
        boot_ui::begin_stage(boot_ui::BootStage::InterruptInit, 5);

        // Initialize error handling system early
        boot_ui::update_substage(1, "Initializing error handling...");
        error::init_error_handling();
        boot_ui::report_success("Error handling system initialized");

        // Initialize health monitoring system
        boot_ui::update_substage(2, "Starting health monitoring...");
        health::init_health_monitoring();
        boot_ui::report_success("System health monitoring active");

        // Initialize comprehensive logging and debugging
        boot_ui::update_substage(3, "Setting up logging subsystem...");
        logging::init_logging_and_debugging();
        boot_ui::report_success("Logging and debugging ready");

        // Initialize GDT and interrupts
        boot_ui::update_substage(4, "Configuring GDT and IDT...");
        gdt::init();
        gdt::init_interrupt_stacks();
        kernel::mark_subsystem_ready("gdt");
        interrupts::init();
        kernel::mark_subsystem_ready("interrupts");
        // APIC is initialized inside interrupts::init() — mark it here.
        kernel::mark_subsystem_ready("apic");
        boot_ui::report_success("GDT and interrupts configured");

        // Initialize SMP subsystem (APIC base, BSP CPU data).
        // Needs GDT and interrupts to be ready.
        match smp::init() {
            Ok(()) => {
                kernel::mark_subsystem_ready("smp");
                boot_ui::report_success("SMP subsystem initialized");
                unsafe {
                    early_serial_write_str("RustOS: SMP initialized\r\n");
                }
            }
            Err(e) => {
                kernel::mark_subsystem_failed("smp");
                boot_ui::report_warning("SMP", "SMP init failed (single-CPU mode)");
                unsafe {
                    early_serial_write_str("RustOS: SMP init FAILED: ");
                    early_serial_write_str(e);
                    early_serial_write_str("\r\n");
                }
            }
        }

        // Initialize fast syscall support
        boot_ui::update_substage(5, "Setting up syscall interface...");
        if syscall_fast::is_supported() {
            syscall_fast::init();
            kernel::mark_subsystem_ready("syscall_fast");
            boot_ui::report_success("Fast syscall (SYSCALL/SYSRET) enabled");
        } else {
            kernel::mark_subsystem_failed("syscall_fast");
            boot_ui::report_warning("Syscall", "Using INT 0x80 fallback");
        }

        // Initialize the INT 0x80 syscall interface (complements syscall_fast).
        match syscall::init() {
            Ok(()) => unsafe {
                kernel::mark_subsystem_ready("syscall");
                early_serial_write_str("RustOS: Syscall (INT 0x80) interface initialized\r\n");
            },
            Err(e) => unsafe {
                kernel::mark_subsystem_failed("syscall");
                early_serial_write_str("RustOS: Syscall init FAILED: ");
                early_serial_write_str(e);
                early_serial_write_str("\r\n");
            },
        }
        unsafe {
            early_serial_write_str("RustOS: Syscall state=");
            early_serial_write_str(syscall::init_state().as_str());
            early_serial_write_str(" int80=");
            early_serial_write_str(if syscall::int80_entry_ready() {
                "ready"
            } else {
                "not-ready"
            });
            early_serial_write_str("\r\n");
        }

        // Initialize IRQ domain framework (hierarchical interrupt controller mapping).
        match irq_domain::init() {
            Ok(()) => unsafe {
                kernel::mark_subsystem_ready("irq_domain");
                early_serial_write_str("RustOS: IRQ domain framework initialized\r\n");
            },
            Err(e) => unsafe {
                kernel::mark_subsystem_failed("irq_domain");
                early_serial_write_str("RustOS: IRQ domain init FAILED: ");
                early_serial_write_str(e);
                early_serial_write_str("\r\n");
            },
        }

        // SAFETY: Debug output
        unsafe {
            early_serial_write_str("RustOS: Syscall init done, completing stage...\r\n");
        }

        boot_ui::complete_stage(boot_ui::BootStage::InterruptInit);

        // SAFETY: Debug output
        unsafe {
            early_serial_write_str("RustOS: Stage complete, doing short delay...\r\n");
        }

        // All PIC interrupts are masked in interrupts::init() for safe boot
        boot_ui::boot_delay_short();

        // SAFETY: Debug output
        unsafe {
            early_serial_write_str("RustOS: Short delay done\r\n");
        }

        // SAFETY: Debug output
        unsafe {
            early_serial_write_str("RustOS: Phase 6 complete, starting Phase 7...\r\n");
        }

        // ========================================================================
        // Boot Menu - Skip in fast boot mode (auto-boot like native OS)
        // ========================================================================
        let boot_selection = if boot_ui::boot_config().fast_boot {
            unsafe {
                early_serial_write_str("RustOS: Fast boot — skipping boot menu\r\n");
            }
            boot_ui::BootMenuSelection::NormalBoot
        } else if early_display_ready {
            unsafe {
                early_serial_write_str("RustOS: Showing boot menu (graphical)...\r\n");
            }
            boot_ui::show_graphical_boot_menu()
        } else {
            unsafe {
                early_serial_write_str("RustOS: Showing boot menu (text mode)...\r\n");
            }
            boot_ui::show_boot_menu()
        };

        if boot_selection == boot_ui::BootMenuSelection::InstallRustOS {
            installer::set_install_mode(true);
        }
        unsafe {
            early_serial_write_str("RustOS: Boot menu selection made\r\n");
        }

        // Clear screen for normal boot progress display
        if boot_selection == boot_ui::BootMenuSelection::NormalBoot {
            if early_display_ready {
                boot_ui::show_graphical_splash();
            } else {
                boot_ui::show_boot_splash();
            }
        }

        // ========================================================================
        // PHASE 7: Driver Loading
        // ========================================================================
        // SAFETY: Debug output
        unsafe {
            early_serial_write_str("RustOS: Starting driver loading...\r\n");
        }
        let driver_result = boot_ui::driver_loading_progress();

        // sound::init() is called inside driver_loading_progress(); mark it
        // here based on whether PCM devices were registered.
        if sound::pcm_count() > 0 {
            kernel::mark_subsystem_ready("sound");
        } else {
            kernel::mark_subsystem_failed("sound");
        }

        // Initialize network sub-modules (net::init() is called inside
        // driver_loading_progress, but these sub-module inits are not called
        // from net::init() itself).
        match net::device::init() {
            Ok(()) => {
                kernel::mark_subsystem_ready("network");
                unsafe {
                    early_serial_write_str("RustOS: Network device subsystem initialized\r\n");
                }
            }
            Err(_e) => {
                kernel::mark_subsystem_failed("network");
                unsafe {
                    early_serial_write_str("RustOS: Network device init FAILED\r\n");
                }
            }
        }
        net::buffer::init_buffer_manager();
        unsafe {
            early_serial_write_str("RustOS: Network buffer manager initialized\r\n");
        }
        let _ = net::arp::init();
        let _ = net::icmp::init();
        unsafe {
            early_serial_write_str("RustOS: Network ARP/ICMP subsystems initialized\r\n");
        }

        if boot_ui::boot_config().install_mode {
            installer::init();
        }

        // Initialize the comprehensive GPU system (PCI scan, memory manager,
        // acceleration engine, opensource drivers). Needs PCI bus from driver loading.
        match gpu::initialize() {
            Ok(()) => {
                kernel::mark_subsystem_ready("gpu");
                unsafe {
                    early_serial_write_str("RustOS: GPU system initialized\r\n");
                }
            }
            Err(e) => {
                kernel::mark_subsystem_failed("gpu");
                unsafe {
                    early_serial_write_str("RustOS: GPU system init skipped: ");
                    early_serial_write_str(e);
                    early_serial_write_str("\r\n");
                }
            }
        }

        // Initialize Mesa (OpenGL) compatibility layer for GPU acceleration.
        // Needs the GPU system to be initialized first.
        match gpu::opensource::mesa_compat::init_mesa_compat() {
            Ok(()) => unsafe {
                early_serial_write_str("RustOS: Mesa compatibility layer initialized\r\n");
            },
            Err(e) => unsafe {
                early_serial_write_str("RustOS: Mesa compat init skipped: ");
                early_serial_write_str(e);
                early_serial_write_str("\r\n");
            },
        }

        // Initialize I/O optimization system (lazy-initialized, but explicit init
        // ensures it's ready before filesystem/network operations need it).
        match io_optimized::init_io_system() {
            Ok(()) => unsafe {
                early_serial_write_str("RustOS: I/O optimization system initialized\r\n");
            },
            Err(e) => unsafe {
                early_serial_write_str("RustOS: I/O optimization init FAILED: ");
                early_serial_write_str(e);
                early_serial_write_str("\r\n");
            },
        }

        match drivers::init_drivers() {
            Ok(()) => {
                kernel::mark_subsystem_ready("drivers");
                unsafe {
                    early_serial_write_str("RustOS: Linux driver subsystems initialized\r\n");
                }
            }
            Err(e) => {
                kernel::mark_subsystem_failed("drivers");
                unsafe {
                    early_serial_write_str("RustOS: Linux driver subsystem init FAILED: ");
                    early_serial_write_str(e);
                    early_serial_write_str("\r\n");
                }
            }
        }

        // SAFETY: Debug output
        unsafe {
            early_serial_write_str("RustOS: Driver loading done\r\n");
        }

        // Initialize hot-plug subsystem (needs PCI bus from driver loading).
        // Registers default handler and common driver match patterns.
        match crate::drivers::hotplug::init() {
            Ok(()) => unsafe {
                early_serial_write_str("RustOS: Hot-plug subsystem initialized\r\n");
            },
            Err(_) => unsafe {
                early_serial_write_str("RustOS: Hot-plug init skipped\r\n");
            },
        }
        let _ = crate::drivers::hotplug::scan_devices();
        let _ = crate::drivers::hotplug::process_events();

        // Initialize the global testing framework for in-kernel test execution.
        testing_framework::init_testing_framework();

        // Initialize the kernel VFS manager (crate::fs::vfs()).
        // This mounts the root filesystem (ext4/fat32 from storage, or ramfs
        // fallback), devfs at /dev, sysfs at /sys, and hugetlbfs at
        // /dev/hugepages. The buffer cache for block I/O is also initialized.
        // Must run after driver loading (which discovers storage devices) and
        // before process/scheduler init (which need file loading via VFS_MANAGER).
        match fs::init() {
            Ok(()) => {
                kernel::mark_subsystem_ready("filesystem");
                unsafe {
                    early_serial_write_str("RustOS: Kernel VFS manager initialized\r\n");
                }
            }
            Err(e) => {
                kernel::mark_subsystem_failed("filesystem");
                unsafe {
                    early_serial_write_str("RustOS: Kernel VFS manager init FAILED\r\n");
                    let _ = e;
                }
            }
        }

        // Initialize performance monitoring for benchmarks.
        match testing::benchmarking::init_performance_monitoring() {
            Ok(()) => unsafe {
                early_serial_write_str("RustOS: Performance monitoring initialized\r\n");
            },
            Err(e) => unsafe {
                early_serial_write_str("RustOS: Performance monitoring init FAILED: ");
                early_serial_write_str(e);
                early_serial_write_str("\r\n");
            },
        }

        // Time system was already initialized in driver_loading_progress()
        // Check if it succeeded and enable timer interrupt
        unsafe {
            early_serial_write_str("RustOS: Checking time init result...\r\n");
        }
        let time_initialized = driver_result.timer_loaded;
        unsafe {
            early_serial_write_str("RustOS: time_initialized check done\r\n");
        }
        if time_initialized {
            kernel::mark_subsystem_ready("time");
            unsafe {
                early_serial_write_str("RustOS: About to get timer stats...\r\n");
            }
            let stats = time::get_timer_stats();
            unsafe {
                early_serial_write_str("RustOS: Got timer stats\r\n");
            }
            log_info!(
                "kernel",
                "Time system initialized with {:?} timer",
                stats.active_timer
            );

            // Initialize system time from RTC
            if let Ok(()) = time::init_system_time_from_rtc() {
                log_info!(
                    "kernel",
                    "System time initialized from RTC: {}",
                    time::system_time()
                );
            }
        } else {
            kernel::mark_subsystem_failed("time");
            log_error!(
                "kernel",
                "Time system initialization failed in driver loading phase"
            );
        }

        // Enable keyboard and mouse interrupts for user input
        unsafe {
            early_serial_write_str("RustOS: Enabling keyboard interrupt...\r\n");
        }
        interrupts::enable_keyboard_interrupt();
        unsafe {
            early_serial_write_str("RustOS: Keyboard interrupt enabled\r\n");
        }
        unsafe {
            early_serial_write_str("RustOS: Enabling mouse interrupt...\r\n");
        }
        interrupts::enable_mouse_interrupt();
        unsafe {
            early_serial_write_str("RustOS: Mouse interrupt enabled\r\n");
        }

        // ========================================================================
        // PHASE 8: File System Mount
        // ========================================================================
        // Initialize context switcher (FPU, context management) before process init.
        unsafe {
            early_serial_write_str("RustOS: Initializing context switcher...\r\n");
        }
        match process::context::init() {
            Ok(()) => unsafe {
                early_serial_write_str("RustOS: Context switcher initialized\r\n");
            },
            Err(e) => unsafe {
                early_serial_write_str("RustOS: Context switcher init FAILED: ");
                early_serial_write_str(e);
                early_serial_write_str("\r\n");
            },
        }
        // Initialize process management and scheduler before filesystem/Linux init.
        // process_manager::init() calls process::init() internally, so we do not need
        // a separate process::init() call here.
        unsafe {
            early_serial_write_str("RustOS: Initializing process manager...\r\n");
        }
        match process::init() {
            Ok(()) => {
                kernel::mark_subsystem_ready("process");
                unsafe {
                    early_serial_write_str("RustOS: Process manager initialized\r\n");
                }
            }
            Err(e) => {
                kernel::mark_subsystem_failed("process");
                unsafe {
                    early_serial_write_str("RustOS: Process manager init FAILED: ");
                    early_serial_write_str(e);
                    early_serial_write_str("\r\n");
                }
            }
        }
        // Initialize the dynamic linker (needed for ELF loading with shared libraries).
        process::dynamic_linker::init_dynamic_linker();
        unsafe {
            early_serial_write_str("RustOS: Dynamic linker initialized\r\n");
        }
        match process_manager::init() {
            Ok(()) => {
                kernel::mark_subsystem_ready("process_manager");
                crate::glib_spawn::mark_spawn_runtime_ready();
                unsafe {
                    early_serial_write_str("RustOS: POSIX process manager initialized\r\n");
                }
            }
            Err(e) => {
                kernel::mark_subsystem_failed("process_manager");
                unsafe {
                    early_serial_write_str("RustOS: POSIX process manager init FAILED: ");
                    early_serial_write_str(e);
                    early_serial_write_str("\r\n");
                }
            }
        }
        unsafe {
            early_serial_write_str(
                "RustOS: GLib/GNOME smoke checks deferred to userspace PID 1\r\n",
            );
        }
        // Early cgroup init — root cgroup must exist before the scheduler
        // creates PID 1 so processes can be assigned to a cgroup.
        // Mirrors Linux's cgroup_init_early() in start_kernel().
        cgroup::init_early();
        unsafe {
            early_serial_write_str("RustOS: Initializing scheduler...\r\n");
        }
        match scheduler::init() {
            Ok(()) => {
                scheduler::load_balance::init_run_queues(smp::cpu_count());
                kernel::mark_subsystem_ready("scheduler");
                unsafe {
                    early_serial_write_str("RustOS: Scheduler initialized\r\n");
                }
            }
            Err(e) => {
                kernel::mark_subsystem_failed("scheduler");
                unsafe {
                    early_serial_write_str("RustOS: Scheduler init FAILED: ");
                    early_serial_write_str(e);
                    early_serial_write_str("\r\n");
                }
            }
        }
        // Transition to SCHEDULING — scheduler is now running.
        // Mirrors Linux's rest_init() setting system_state = SYSTEM_SCHEDULING.
        kernel::set_system_state(kernel::SystemState::Scheduling);
        unsafe {
            early_serial_write_str("RustOS: Initializing security subsystem...\r\n");
        }
        match security::init() {
            Ok(()) => {
                kernel::mark_subsystem_ready("security");
                unsafe {
                    early_serial_write_str("RustOS: Security subsystem initialized\r\n");
                }
            }
            Err(e) => {
                kernel::mark_subsystem_failed("security");
                unsafe {
                    early_serial_write_str("RustOS: Security init FAILED: ");
                    early_serial_write_str(e);
                    early_serial_write_str("\r\n");
                }
            }
        }
        // Initialize the secure key store (for cryptographic key storage).
        // Needs the security subsystem to be initialized first.
        match security::init_key_store() {
            Ok(()) => unsafe {
                early_serial_write_str("RustOS: Secure key store initialized\r\n");
            },
            Err(e) => unsafe {
                early_serial_write_str("RustOS: Secure key store init FAILED: ");
                early_serial_write_str(e);
                early_serial_write_str("\r\n");
            },
        }

        // Initialize the kernel crypto subsystem
        crypto::init();
        kernel::mark_subsystem_ready("crypto");
        unsafe {
            early_serial_write_str("RustOS: Crypto subsystem initialized\r\n");
        }

        unsafe {
            early_serial_write_str("RustOS: Starting Phase 8 - Filesystem mount...\r\n");
        }
        let _fs_result = boot_ui::filesystem_mount_progress();
        if _fs_result.initramfs_loaded {
            kernel::mark_subsystem_ready("initramfs");
        } else {
            kernel::mark_subsystem_failed("initramfs");
        }
        unsafe {
            early_serial_write_str("RustOS: Phase 8 complete\r\n");
        }

        // Enable timer interrupt now that filesystem init is done
        unsafe {
            early_serial_write_str("RustOS: Enabling timer interrupt...\r\n");
        }
        interrupts::enable_timer_interrupt();
        unsafe {
            early_serial_write_str("RustOS: Timer interrupt enabled\r\n");
        }

        // Initialize Linux compatibility layer (file_ops, process_ops, etc.)
        unsafe {
            early_serial_write_str("RustOS: Initializing Linux compatibility layer...\r\n");
        }
        linux_compat::init_linux_compat();
        kernel::mark_subsystem_ready("linux_compat");
        unsafe {
            early_serial_write_str("RustOS: Linux compatibility layer initialized\r\n");
        }

        // Set default hostname so GNOME processes and procfs see a proper name.
        let _ = linux_compat::sysinfo_ops::set_kernel_hostname("rustos");
        unsafe {
            early_serial_write_str("RustOS: Default hostname set to 'rustos'\r\n");
        }

        // Initialize SoftIRQ and workqueue subsystem
        softirq::init();
        kernel::mark_subsystem_ready("softirq");
        workqueue::init();
        kernel::mark_subsystem_ready("workqueue");

        // Start the kthreadd daemon (Linux PID 2 equivalent).
        // Must run after scheduler::init() and process::init() so that
        // create_kernel_thread is available.  kthreadd processes the
        // kthread_create_queue for deferred kernel-thread creation.
        match kthread::kthreadd_init() {
            Ok(tid) => unsafe {
                early_serial_write_str("RustOS: kthreadd daemon started\r\n");
                let _ = tid;
            },
            Err(e) => unsafe {
                early_serial_write_str("RustOS: kthreadd init FAILED\r\n");
                let _ = e;
            },
        }

        // NUMA policy backend and RCU (RCU uses RCU softirq)
        numa::init();
        kernel::mark_subsystem_ready("numa");
        rcu::init();
        kernel::mark_subsystem_ready("rcu");

        // Initialize futex subsystem
        futex::init();
        kernel::mark_subsystem_ready("futex");

        // Initialize epoll subsystem
        epoll::init();
        kernel::mark_subsystem_ready("epoll");

        // Initialize OOM killer
        oom::init();
        kernel::mark_subsystem_ready("oom");

        // Initialize swap subsystem
        swap::init();
        kernel::mark_subsystem_ready("swap");

        // Initialize block I/O layer (registers virtio-blk if available)
        block_io::init();
        kernel::mark_subsystem_ready("block_io");

        // Register block devices in /dev (devfs must be mounted first in Phase 8)
        crate::fs::devfs::register_block_devices();

        // Scan for Linux md (RAID) arrays on storage devices and register
        // them as block devices (major 9).  Runs after storage detection
        // (Phase 7) and block_io init so members are available.
        {
            let md_result = md::init();
            if md_result.errors.is_empty() {
                kernel::mark_subsystem_ready("md");
            } else {
                kernel::mark_subsystem_failed("md");
            }
            if md_result.arrays_registered > 0 {
                crate::serial_println!(
                    "[md] registered {} RAID arrays",
                    md_result.arrays_registered
                );
                // Re-register block devices to pick up new md devices
                crate::fs::devfs::register_block_devices();
            }
            for err in &md_result.errors {
                crate::serial_println!("[md] {}", err);
            }
        }

        // Initialize cgroups
        cgroup::init();
        kernel::mark_subsystem_ready("cgroup");

        // Initialize usermodehelper (kernel-spawned userspace programs).
        // Mirrors Linux's usermodehelper_init() in do_basic_setup().
        usermodehelper::init();
        kernel::mark_subsystem_ready("usermodehelper");

        // Initialize seccomp
        seccomp::init();
        kernel::mark_subsystem_ready("seccomp");

        // Initialize namespaces
        namespace::init();
        kernel::mark_subsystem_ready("namespace");

        // Initialize ptrace
        ptrace::init();
        kernel::mark_subsystem_ready("ptrace");

        // Initialize inotify
        inotify::init();
        kernel::mark_subsystem_ready("inotify");

        // Initialize pidfd
        pidfd::init();
        kernel::mark_subsystem_ready("pidfd");

        // Initialize io_uring
        io_uring::init();
        kernel::mark_subsystem_ready("io_uring");

        // Initialize fanotify
        fanotify::init();
        kernel::mark_subsystem_ready("fanotify");

        // Initialize new mount API
        mount_api::init();
        kernel::mark_subsystem_ready("mount_api");

        // Initialize disk quota subsystem
        quota::init();
        kernel::mark_subsystem_ready("quota");

        // Initialize Landlock
        landlock::init();
        kernel::mark_subsystem_ready("landlock");

        // Initialize BPF
        bpf::init();
        kernel::mark_subsystem_ready("bpf");

        // Initialize keyring
        keyring::init();
        kernel::mark_subsystem_ready("keyring");

        // Load integrity keys from the root filesystem.
        // Mirrors Linux's integrity_load_keys() in kernel_init_freeable().
        // Runs after fs::init() mounted the root fs and after keyring::init().
        keyring::load_keys_from_rootfs();

        // Initialize SysV IPC
        sysv_ipc::init();
        kernel::mark_subsystem_ready("sysv_ipc");

        // Initialize AIO
        aio::init();
        kernel::mark_subsystem_ready("aio");

        // Initialize perf events
        perf_event::init();
        kernel::mark_subsystem_ready("perf_event");

        // Initialize userfaultfd and secret memory fd state
        userfaultfd::init();
        kernel::mark_subsystem_ready("userfaultfd");
        memfd_secret::init();
        kernel::mark_subsystem_ready("memfd_secret");
        hugetlb::init();
        kernel::mark_subsystem_ready("hugetlb");
        thp::init();
        kernel::mark_subsystem_ready("thp");
        memory_hotplug::init();
        kernel::mark_subsystem_ready("memory_hotplug");
        kasan::init();
        kernel::mark_subsystem_ready("kasan");
        kcsan::init();
        kernel::mark_subsystem_ready("kcsan");
        // of::init() is called from drivers::init_drivers().
        kernel::mark_subsystem_ready("of");

        // notifier::init() is called from kernel::init().
        // Notifier chains must be ready before power::init() which uses PM notifier chains.

        power::init();
        kernel::mark_subsystem_ready("power");
        cpufreq::init();
        kernel::mark_subsystem_ready("cpufreq");
        cpuidle::init();
        kernel::mark_subsystem_ready("cpuidle");

        // Initialize runtime file handles and privileged low-level syscall state
        file_handle::init();
        kernel::mark_subsystem_ready("file_handle");
        privileged_syscalls::init();
        kernel::mark_subsystem_ready("privileged_syscalls");
        // Initialize restartable sequence registrations
        rseq::init();
        kernel::mark_subsystem_ready("rseq");

        // Initialize module loader
        module_loader::init();
        kernel::mark_subsystem_ready("module_loader");
        livepatch::init();
        kernel::mark_subsystem_ready("livepatch");
        // edac::init() and nvdimm::init() are called from drivers::init_drivers().
        kernel::mark_subsystem_ready("edac");
        mfd::init();
        kernel::mark_subsystem_ready("mfd");
        kernel::mark_subsystem_ready("nvdimm");

        // Initialize audit, trace, and kprobes
        audit::init();
        kernel::mark_subsystem_ready("audit");
        trace::init();
        kernel::mark_subsystem_ready("trace");
        kprobes::init();
        kernel::mark_subsystem_ready("kprobes");

        // Initialize kexec
        kexec::init();
        kernel::mark_subsystem_ready("kexec");

        // Initialize network filesystems
        fs::nfs_client::init();
        fs::nfsd::init();
        fs::cifs::init();

        // Verify C compression libraries (zstd, bzip2, xz) are linked and
        // the kernel allocator FFI callbacks work.
        match package::compression::ffi::zstd_decompress_safe(&[
            0x28, 0xB5, 0x2F, 0xFD, 0x00, 0x58, 0x01, 0x00, 0x00,
        ]) {
            Ok(_) => unsafe {
                early_serial_write_str("RustOS: Zstd decompressor linked OK\r\n");
            },
            Err(_) => unsafe {
                early_serial_write_str("RustOS: Zstd decompressor link check (expected: format error on test input)\r\n");
            },
        }
        match package::compression::ffi::bzip2_decompress_safe(&[0x42, 0x5A, 0x68, 0x39, 0x00]) {
            Ok(_) => unsafe {
                early_serial_write_str("RustOS: Bzip2 decompressor linked OK\r\n");
            },
            Err(_) => unsafe {
                early_serial_write_str("RustOS: Bzip2 decompressor link check (expected: format error on test input)\r\n");
            },
        }
        match package::compression::ffi::xz_decompress_safe(&[
            0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ]) {
            Ok(_) => unsafe {
                early_serial_write_str("RustOS: XZ decompressor linked OK\r\n");
            },
            Err(_) => unsafe {
                early_serial_write_str(
                    "RustOS: XZ decompressor link check (expected: format error on test input)\r\n",
                );
            },
        }

        // Initialize Linux integration layer
        if !early_display_ready {
            boot_display::show_subsystem_init(
                "Linux Integration Layer",
                boot_display::SubsystemStatus::Initializing,
            );
        }
        match linux_integration::init() {
            Ok(_) => {
                kernel::mark_subsystem_ready("linux_integration");
                unsafe {
                    early_serial_write_str("RustOS: Linux init OK, showing status...\r\n");
                }
                if !early_display_ready {
                    boot_display::show_subsystem_init(
                        "Linux Integration Layer",
                        boot_display::SubsystemStatus::Ready,
                    );
                }
                unsafe {
                    early_serial_write_str("RustOS: Linux status shown, skip state updates\r\n");
                }
            }
            Err(_e) => {
                kernel::mark_subsystem_failed("linux_integration");
                unsafe {
                    early_serial_write_str("RustOS: Linux init error\r\n");
                }
                if !early_display_ready {
                    boot_display::show_subsystem_init(
                        "Linux Integration Layer",
                        boot_display::SubsystemStatus::Warning,
                    );
                }
            }
        }
        unsafe {
            early_serial_write_str("RustOS: Linux integration done\r\n");
        }

        // Initialize D-Bus message bus
        unsafe {
            early_serial_write_str("RustOS: Initializing D-Bus message bus...\r\n");
        }
        match dbus::init() {
            Ok(()) => {
                kernel::mark_subsystem_ready("dbus");
                unsafe {
                    early_serial_write_str("RustOS: D-Bus message bus ready\r\n");
                }
            }
            Err(e) => {
                kernel::mark_subsystem_failed("dbus");
                unsafe {
                    early_serial_write_str("RustOS: D-Bus init FAILED: ");
                    early_serial_write_str(e);
                    early_serial_write_str("\r\n");
                }
            }
        }

        // Initialize Wayland compositor
        unsafe {
            early_serial_write_str("RustOS: Initializing Wayland compositor...\r\n");
        }
        match wayland::init() {
            Ok(()) => {
                kernel::mark_subsystem_ready("wayland");
                unsafe {
                    early_serial_write_str("RustOS: Wayland compositor ready\r\n");
                    early_serial_write_str(
                        "RustOS: Wayland smoke check deferred to userspace PID 1\r\n",
                    );
                }
            }
            Err(e) => {
                kernel::mark_subsystem_failed("wayland");
                unsafe {
                    early_serial_write_str("RustOS: Wayland init FAILED: ");
                    early_serial_write_str(e);
                    early_serial_write_str("\r\n");
                }
            }
        }

        // Initialize Mutter foundation (Wayland handshake verification)
        match mutter::init() {
            Ok(()) => unsafe {
                kernel::mark_subsystem_ready("mutter");
                early_serial_write_str("RustOS: Mutter foundation ready\r\n");
            },
            Err(e) => unsafe {
                kernel::mark_subsystem_failed("mutter");
                early_serial_write_str("RustOS: Mutter init FAILED: ");
                early_serial_write_str(e);
                early_serial_write_str("\r\n");
            },
        }

        // Validate full desktop session stack (linux compat + overlay + wayland + dbus).
        unsafe {
            early_serial_write_str(
                "RustOS: desktop readiness checks deferred to userspace PID 1\r\n",
            );
        }

        // ========================================================================
        // PHASE 9: Graphics Initialization (already done early — mark complete)
        // ========================================================================
        let mut graphics_result = early_graphics_result;
        let display_driver_ready = early_display_ready;

        if display_driver_ready {
            boot_ui::begin_stage(boot_ui::BootStage::GraphicsInit, 1);
            boot_ui::update_substage(1, "Display ready (early init)");
            boot_ui::report_success("Display output verified");
            boot_ui::complete_stage(boot_ui::BootStage::GraphicsInit);
        } else if !boot_ui::boot_config().force_text_mode && !boot_ui::boot_config().safe_mode {
            // Fall back to existing graphics init (checks bootloader framebuffer, then text mode)
            graphics_result = boot_ui::graphics_init_progress();
        }

        if display_driver_ready {
            crate::serial_println!(
                "display: driver ready {}x{}x{}",
                graphics_result.width,
                graphics_result.height,
                graphics_result.bpp
            );
        }
        unsafe {
            early_serial_write_str("RustOS: Phase 9 complete\r\n");
        }

        // Render graphical boot progress if framebuffer is ready
        if graphics_result.framebuffer_ready {
            boot_ui::render_graphical_boot_progress();
        }

        // Decide boot mode based on graphics initialization
        let use_graphics_desktop = graphics_result.framebuffer_ready
            && graphics_result.bpp == 32
            && !graphics_result.fallback_to_text;

        // ========================================================================
        // PHASE 10: Desktop Environment Initialization
        // ========================================================================
        let desktop_result = if use_graphics_desktop {
            if display_driver_ready {
                let mut result = boot_ui::DesktopInitResult::new();
                match desktop::init_default_desktop() {
                    Ok(()) => {
                        kernel::mark_subsystem_ready("desktop");
                        result.window_manager_ready = true;
                        result.input_ready = true;
                        result.taskbar_ready = true;
                        result.windows_created = true;
                    }
                    Err(e) => {
                        kernel::mark_subsystem_failed("desktop");
                        crate::serial_println!("Desktop setup error: {}", e);
                    }
                }
                result
            } else {
                boot_ui::desktop_init_progress()
            }
        } else {
            // Skip desktop init when the 32-bit framebuffer desktop is unavailable.
            boot_ui::begin_stage(boot_ui::BootStage::DesktopInit, 1);
            boot_ui::update_substage(1, "32-bit framebuffer desktop unavailable...");
            boot_ui::report_warning("Desktop", "32-bit graphical UI unavailable");
            boot_ui::complete_stage(boot_ui::BootStage::DesktopInit);
            boot_ui::DesktopInitResult::new()
        };

        // ========================================================================
        // Boot Complete Summary
        // ========================================================================
        let boot_time = if time_initialized {
            time::uptime_ms()
        } else {
            0
        };
        unsafe {
            early_serial_write_str("RustOS: Boot complete in ");
            early_serial_write_u64(boot_time);
            early_serial_write_str("ms\r\n");
        }
        kernel::mark_boot_ready();
        // Transition to RUNNING — all subsystems initialized, userspace
        // init is about to be launched.  Mirrors Linux's kernel_init()
        // setting system_state = SYSTEM_RUNNING.
        kernel::set_system_state(kernel::SystemState::Running);
        if !display_driver_ready {
            boot_ui::boot_complete_summary();
            boot_display::show_boot_complete(boot_time);
            boot_display::show_system_info();
            boot_display::show_services_status();
        }

        // Show first boot information (text mode only)
        if !graphics_result.framebuffer_ready {
            boot_ui::show_first_boot_info(&hardware_result, &memory_result);
            boot_display::show_memory_info(
                memory_result.total_memory_mb as usize,
                memory_result.usable_memory_mb as usize,
                memory_result.memory_regions,
            );
            boot_display::show_desktop_startup();
        }

        // Brief pause before transitioning to desktop
        if !display_driver_ready {
            boot_ui::boot_delay_medium();
        }

        // Render graphical boot complete screen
        if graphics_result.framebuffer_ready {
            boot_ui::render_graphical_boot_complete();
        }

        // ========================================================================
        // Transition to Desktop Environment
        // ========================================================================
        if !display_driver_ready {
            boot_ui::transition_to_desktop();
        } else if graphics_result.framebuffer_ready {
            // GNOME-style smooth fade before desktop appears
            boot_ui::transition_to_desktop();
        }

        // ========================================================================
        // Userspace init — spawn GNOME session bootstrap alongside kernel compositor
        // ========================================================================
        let boot_config = boot_ui::boot_config();
        let session_boot = if installer::is_install_mode() {
            crate::linux_compat::desktop::SessionBoot::Install
        } else if boot_config.live_mode {
            crate::linux_compat::desktop::SessionBoot::Live
        } else {
            crate::linux_compat::desktop::SessionBoot::Desktop
        };
        let drm_mode_configured = use_graphics_desktop
            && crate::gpu::opensource::drm_compat::configure_primary_mode(
                graphics_result.width as u32,
                graphics_result.height as u32,
                graphics_result.bpp as u32,
            )
            .is_ok();
        if drm_mode_configured {
            crate::serial_println!(
                "drm/kms: primary mode configured {}x{}x{}",
                graphics_result.width,
                graphics_result.height,
                graphics_result.bpp
            );
        }
        let drm_kms_ready =
            use_graphics_desktop && drm_mode_configured && crate::vfs::drmfs::smoke_check().is_ok();
        crate::linux_compat::desktop::mark_graphical_boot(
            session_boot,
            graphics_result.framebuffer_ready,
            drm_kms_ready,
            graphics_result.gpu_accelerated,
            graphics_result.width,
            graphics_result.height,
            graphics_result.bpp,
        );

        let mut userspace_spawned = false;
        if boot_config.prefer_userspace_init
            && !boot_config.safe_mode
            && initramfs::userspace_init_available()
        {
            unsafe {
                early_serial_write_str(
                    "RustOS: userspace init found, spawning session bootstrap\r\n",
                );
            }
            match initramfs::spawn_userspace_init(session_boot) {
                Ok(pid) => {
                    userspace_spawned = true;
                    crate::serial_println!(
                        "Boot: userspace init PID {} queued (boot={:?})",
                        pid,
                        session_boot
                    );
                }
                Err(e) => {
                    crate::serial_println!("Boot: userspace init spawn failed: {:?}", e);
                }
            }
        } else if boot_config.verbose {
            unsafe {
                early_serial_write_str(
                    "RustOS: no userspace init (or disabled), using kernel desktop fallback\r\n",
                );
            }
        }

        // Kernel installer wizard is fallback when userspace GTK installer did not spawn
        if installer::is_install_mode() && !userspace_spawned {
            crate::serial_println!("Boot: entering kernel installer wizard (no userspace init)");
            match installer::run_wizard() {
                Ok(plan) => match installer::apply_plan(&plan) {
                    Ok(()) => installer::finish_install_and_reboot(),
                    Err(e) => {
                        crate::serial_println!("Boot: install apply failed: {}", e);
                        installer::finish_install_and_reboot();
                    }
                },
                Err(e) => {
                    crate::serial_println!("Boot: installer failed: {}", e);
                    installer::finish_install_and_reboot();
                }
            }
        } else if installer::is_install_mode() {
            crate::serial_println!(
                "Boot: install mode — userspace GTK installer + kernel compositor"
            );
        }

        // Launch appropriate desktop environment
        if userspace_spawned {
            // Userspace init (GNOME) is running - don't launch the kernel
            // desktop.  Enter a minimal compositor/idle loop that services
            // the userspace process, forwards input to Wayland clients,
            // and renders their surfaces without drawing a kernel desktop.
            crate::serial_println!("Boot: entering userspace session loop");
            userspace_session_loop()
        } else if use_graphics_desktop && desktop_result.window_manager_ready {
            crate::serial_println!(
                "desktop: {}x{}x{} gpu={}",
                graphics_result.width,
                graphics_result.height,
                graphics_result.bpp,
                graphics_result.gpu_accelerated
            );

            // Enter modern desktop main loop
            modern_desktop_main_loop()
        } else {
            // Fall back to limited VGA graphics only when the 32-bit desktop is unavailable.
            handle_graphics_fallback();

            println!();
            println!("Launching LIMITED GRAPHICS FALLBACK");
            println!("   Mode: VGA Mode 13h (320x200x8)");
            println!("   Interface: diagnostic fallback");
            println!();

            // Brief delay to show message before mode switch
            boot_ui::boot_delay_short();

            // Initialize pixel-based desktop with VGA Mode 13h
            unsafe {
                early_serial_write_str("RustOS: Starting simple_desktop::init_pixel_desktop()\r\n");
            }

            simple_desktop::init_pixel_desktop();
            unsafe {
                early_serial_write_str("RustOS: Starting pixel_desktop_main_loop()\r\n");
            }
            pixel_desktop_main_loop()
        }
    }
}

/// Handle graphics initialization failure with user options
fn handle_graphics_fallback() {
    let progress = boot_ui::boot_progress();

    if progress.is_safe_mode() {
        boot_display::show_safe_mode_banner();
        return;
    }

    // Show error information
    boot_ui::show_graphics_error("Graphics initialization failed or unsupported hardware");

    println!();
    println!("  Automatically continuing in text mode...");
    boot_ui::boot_delay_medium();
}

/// Demonstrate the new error handling and logging system
#[allow(dead_code)]
fn demonstrate_error_handling_and_logging() {
    unsafe {
        early_serial_write_str("demo: error_handling start\r\n");
    }
    println!("Demonstrating Error Handling and Logging System:");

    // Test different log levels
    log_info!("demo", "Testing structured logging system");
    log_debug!("demo", "Debug message with timestamp and location");
    log_warn!("demo", "Warning message example");

    unsafe {
        early_serial_write_str("demo: profiling start\r\n");
    }
    // Test performance profiling
    {
        let _timer = logging::profiling::start_measurement("demo_function");
        // Simulate some work using while loop (for loop ranges can crash in nightly)
        let mut i: u32 = 0;
        while i < 1000 {
            core::hint::spin_loop();
            i = i.wrapping_add(1);
        }
    } // Timer automatically records when dropped

    unsafe {
        early_serial_write_str("demo: dump_kernel_state start\r\n");
    }
    // Display system diagnostics
    logging::kernel_debug::dump_kernel_state();

    unsafe {
        early_serial_write_str("demo: get_health_status start\r\n");
    }
    // Show health status
    let health_status = health::get_health_status();
    println!("   System Health: {:?}", health_status);

    unsafe {
        early_serial_write_str("demo: validate_kernel_subsystems start\r\n");
    }
    // Validate kernel subsystems
    let validation_result = logging::kernel_debug::validate_kernel_subsystems();
    println!(
        "   Kernel Validation: {}",
        if validation_result {
            "PASSED"
        } else {
            "FAILED"
        }
    );

    unsafe {
        early_serial_write_str("demo: get_recent_logs start\r\n");
    }
    // Show recent logs
    let recent_logs = logging::get_recent_logs();
    println!(
        "   Recent Log Entries: {} stored in memory",
        recent_logs.len()
    );

    println!("Error handling and logging demonstration complete");
    unsafe {
        early_serial_write_str("demo: error_handling done\r\n");
    }
    println!();
}

/// Demonstrate the package management system
#[allow(dead_code)]
fn demonstrate_package_manager() {
    println!("📦 Demonstrating Package Management System:");

    // Initialize package manager with Native RustOS package manager
    package::init_package_manager(package::PackageManagerType::Native);
    println!("   ✅ Package manager initialized (Native RustOS mode)");

    // Show supported package formats
    println!("   📋 Supported Package Formats:");
    println!("      • .deb  - Debian/Ubuntu packages (full support)");
    println!("      • .rpm  - Fedora/RHEL packages (validation only)");
    println!("      • .apk  - Alpine Linux packages (validation only)");
    println!("      • .rustos - Native RustOS packages (planned)");

    println!("   🔧 Available Operations:");
    println!("      • Install: syscall(200, name_ptr, name_len)");
    println!("      • Remove: syscall(201, name_ptr, name_len)");
    println!("      • Search: syscall(202, query_ptr, query_len, result_ptr, result_len)");
    println!("      • Info: syscall(203, name_ptr, name_len, result_ptr, result_len)");
    println!("      • List: syscall(204, result_ptr, result_len)");
    println!("      • Update: syscall(205)");
    println!("      • Upgrade: syscall(206, name_ptr, name_len)");

    println!("   📚 Features:");
    println!("      • AR archive parsing (for .deb)");
    println!("      • TAR archive extraction");
    println!("      • GZIP/DEFLATE decompression (miniz_oxide)");
    println!("      • Zstd decompression (C library port)");
    println!("      • Bzip2 decompression (C library port)");
    println!("      • XZ/LZMA2 decompression (C library port)");
    println!("      • Package metadata parsing");
    println!("      • Dependency tracking");
    println!("      • Package database management");

    println!("   ⚠️  Note: Full installation requires:");
    println!("      • Network stack (for downloads)");
    println!("      • Filesystem support (for file installation)");
    println!("      • Script execution (for postinst/prerm)");

    println!("✅ Package management system demonstration complete");
    println!();
}

/// Demonstrate the Linux compatibility layer
#[allow(dead_code)]
fn demonstrate_linux_compat() {
    println!("🐧 Demonstrating Linux API Compatibility Layer:");

    // Initialize Linux compatibility layer
    linux_compat::init_linux_compat();
    println!("   ✅ Linux compatibility layer initialized");

    // Show supported API categories
    println!("   📋 Supported POSIX/Linux APIs (200+ functions):");
    println!("      • File Operations: fstat, lstat, access, dup, link, chmod, chown, truncate");
    println!("      • Process Control: getuid, setuid, getpgid, setsid, getrusage, prctl");
    println!("      • Time APIs: clock_gettime, nanosleep, timer_create, gettimeofday");
    println!("      • Signal Handling: sigaction, sigprocmask, sigpending, rt_sig*, pause");
    println!("      • Socket Operations: send, recv, setsockopt, poll, epoll, select");
    println!("      • IPC: message queues, semaphores, shared memory, eventfd, timerfd");
    println!("      • Device Control: ioctl, fcntl, flock");
    println!("      • Advanced I/O: pread/pwrite, readv/writev, sendfile, splice, tee");
    println!("      • Extended Attrs: getxattr, setxattr, listxattr, removexattr");
    println!("      • Directory Ops: mkdir, rmdir, getdents64");
    println!("      • Terminal/TTY: tcgetattr, tcsetattr, openpty, isatty, cfsetspeed");
    println!("      • Memory Mgmt: mmap, munmap, mprotect, madvise, mlock, brk, sbrk");
    println!("      • Threading: clone, futex, set_tid_address, robust_list, arch_prctl");
    println!("      • Filesystem: mount, umount, statfs, pivot_root, sync, quotactl");
    println!("      • Resources: getrlimit, setrlimit, prlimit, getpriority, sched_*");
    println!("      • System Info: sysinfo, uname, gethostname, getrandom, syslog");

    // Show statistics
    let stats = linux_compat::get_compat_stats();
    println!("   📊 API Call Statistics:");
    println!("      • File operations: {}", stats.file_ops_count);
    println!("      • Process operations: {}", stats.process_ops_count);
    println!("      • Time operations: {}", stats.time_ops_count);
    println!("      • Signal operations: {}", stats.signal_ops_count);
    println!("      • Socket operations: {}", stats.socket_ops_count);
    println!("      • IPC operations: {}", stats.ipc_ops_count);
    println!("      • Ioctl operations: {}", stats.ioctl_ops_count);
    println!("      • Advanced I/O: {}", stats.advanced_io_count);
    println!("      • TTY operations: {}", stats.tty_ops_count);
    println!("      • Memory operations: {}", stats.memory_ops_count);
    println!("      • Thread operations: {}", stats.thread_ops_count);
    println!("      • Filesystem operations: {}", stats.fs_ops_count);
    println!("      • Resource operations: {}", stats.resource_ops_count);
    println!("      • Sysinfo operations: {}", stats.sysinfo_ops_count);

    println!("   ✨ Linux Compatibility Features:");
    println!("      • POSIX-compliant error codes (errno)");
    println!("      • Linux syscall number compatibility");
    println!("      • struct stat, timespec, sigaction compatibility");
    println!("      • Binary-compatible with Linux applications");

    println!("✅ Linux compatibility layer demonstration complete");
    println!();
}

/// Demonstrate the comprehensive testing system
#[allow(dead_code)]
fn demonstrate_comprehensive_testing() {
    println!("🧪 Demonstrating Comprehensive Testing System:");

    // Initialize testing system
    match testing::init_testing_system() {
        Ok(()) => {
            println!("   ✅ Testing framework initialized successfully");

            // Run a quick subset of tests for demonstration
            println!("   🔬 Running sample unit tests...");
            let unit_stats = testing::run_test_category("unit");
            println!(
                "      Unit Tests: {}/{} passed",
                unit_stats.passed, unit_stats.total_tests
            );

            println!("   🔗 Running sample integration tests...");
            let integration_stats = testing::run_test_category("integration");
            println!(
                "      Integration Tests: {}/{} passed",
                integration_stats.passed, integration_stats.total_tests
            );

            println!("   ⚡ Running sample performance tests...");
            let perf_stats = testing::run_test_category("performance");
            println!(
                "      Performance Tests: {}/{} passed",
                perf_stats.passed, perf_stats.total_tests
            );

            // Show testing capabilities
            println!("   📊 Available test categories:");
            println!("      • Unit Tests - Core functionality validation");
            println!("      • Integration Tests - System interaction validation");
            println!("      • Stress Tests - High-load system testing");
            println!("      • Performance Tests - Benchmarking and regression detection");
            println!("      • Security Tests - Security vulnerability testing");
            println!("      • Hardware Tests - Real hardware validation");

            println!("   🎯 Comprehensive testing ready for production validation");

            // Demonstrate production validation capabilities
            println!("   🏭 Production validation features:");
            println!("      • Real hardware configuration testing");
            println!("      • Memory safety validation");
            println!("      • Security audit and vulnerability assessment");
            println!("      • Performance regression detection");
            println!("      • Backward compatibility verification");
            println!("      • System stability under load");
            println!("      • Production readiness scoring");

            // Note: Full production validation would be run separately due to time requirements
            println!("   📋 Full production validation available via testing::production_validation::run_production_validation()");
        }
        Err(e) => {
            println!("   ❌ Testing framework initialization failed: {}", e);
        }
    }

    println!("✅ Comprehensive testing demonstration complete");
    println!();
}

/// Main desktop loop that handles keyboard input and desktop updates
#[allow(dead_code)]
fn desktop_main_loop() -> ! {
    let mut update_counter: u64 = 0;
    let mut last_time_display = 0u64;

    // Test timer system functionality
    println!("Testing timer system...");
    match time::test_timer_accuracy() {
        Ok(()) => println!("✅ Timer system test completed successfully"),
        Err(e) => println!("❌ Timer system test failed: {}", e),
    }

    // Display timer system information
    time::display_time_info();

    // Schedule a test timer to demonstrate functionality
    let _timer_id = time::schedule_periodic_timer(5_000_000, || {
        // This callback runs every 5 seconds
        // Note: We can't use println! from interrupt context, but this demonstrates the timer system
    });

    loop {
        // Process keyboard events and forward to desktop
        while let Some(key_event) = keyboard::get_key_event() {
            match key_event {
                keyboard::KeyEvent::CharacterPress(c) => {
                    simple_desktop::with_desktop(|desktop| {
                        desktop.handle_key(c as u8);
                    });
                }
                keyboard::KeyEvent::SpecialPress(special_key) => {
                    // Map special keys to desktop key codes
                    let key_code = match special_key {
                        keyboard::SpecialKey::Escape => 27,   // ESC
                        keyboard::SpecialKey::Enter => 13,    // Enter
                        keyboard::SpecialKey::Backspace => 8, // Backspace
                        keyboard::SpecialKey::Tab => 9,       // Tab
                        keyboard::SpecialKey::F1 => 112,      // F1
                        keyboard::SpecialKey::F2 => 113,      // F2
                        keyboard::SpecialKey::F3 => 114,      // F3
                        keyboard::SpecialKey::F4 => 115,      // F4
                        keyboard::SpecialKey::F5 => 116,      // F5
                        _ => continue,                        // Ignore other special keys for now
                    };

                    simple_desktop::with_desktop(|desktop| {
                        desktop.handle_key(key_code);
                    });
                }
                _ => {
                    // Ignore key releases for now
                }
            }
        }

        // Update desktop periodically (for clock and animations)
        if update_counter.is_multiple_of(1_000_000) {
            simple_desktop::with_desktop(|desktop| {
                desktop.update();
            });

            // Display time information every few seconds
            let current_time = time::uptime_ms();
            if current_time > last_time_display + 5000 {
                last_time_display = current_time;
                // Update desktop with current time info
                simple_desktop::with_desktop(|_desktop| {
                    // The desktop will show uptime in its status
                });
            }
        }

        // Poll network devices for incoming packets
        if update_counter.is_multiple_of(1000) {
            crate::net::poll_network();
        }

        update_counter += 1;

        // Halt CPU until next interrupt to save power
        // SAFETY: Idle loop halts CPU until next interrupt. See docs/SAFETY.md#halt-loop.
        unsafe {
            core::arch::asm!("hlt");
        }
    }
}

/// Pixel-based desktop main loop for VGA Mode 13h graphics
///
/// Main loop for the limited VGA diagnostic fallback.
fn pixel_desktop_main_loop() -> ! {
    let mut update_counter: u64 = 0;

    // SAFETY: Raw I/O to COM1 for logging. See docs/SAFETY.md#io-port-access.
    unsafe {
        early_serial_write_str("RustOS: pixel_desktop_main_loop entered\r\n");
    }

    loop {
        // Process keyboard events
        while let Some(key_event) = keyboard::get_key_event() {
            match key_event {
                keyboard::KeyEvent::CharacterPress(c) => {
                    // Log key press for debugging
                    unsafe {
                        early_serial_write_str("Key: ");
                        early_serial_write_byte(c as u8);
                        early_serial_write_str("\r\n");
                    }

                    // Handle specific keys
                    match c {
                        'r' | 'R' => {
                            // Refresh/redraw desktop
                            simple_desktop::init_pixel_desktop();
                        }
                        'q' | 'Q' => {
                            // Show quit message (in pixel mode)
                            use vga_mode13h::{colors, draw_3d_rect, draw_string, fill_rect};
                            fill_rect(100, 80, 120, 40, colors::BUTTON_FACE);
                            draw_3d_rect(100, 80, 120, 40, true);
                            draw_string(
                                110,
                                95,
                                "Press ESC to continue",
                                colors::BLACK,
                                colors::BUTTON_FACE,
                            );
                        }
                        _ => {}
                    }
                }
                keyboard::KeyEvent::SpecialPress(special_key) => {
                    match special_key {
                        keyboard::SpecialKey::Escape => {
                            // Redraw desktop on ESC
                            simple_desktop::init_pixel_desktop();
                        }
                        keyboard::SpecialKey::F1 => {
                            // Help: draw a help dialog
                            use vga_mode13h::{colors, draw_3d_rect, draw_string, fill_rect};
                            fill_rect(60, 50, 200, 100, colors::BUTTON_FACE);
                            draw_3d_rect(60, 50, 200, 100, true);
                            // Title bar
                            fill_rect(63, 53, 194, 16, colors::TITLE_BAR_BLUE);
                            draw_string(70, 57, "Help", colors::WHITE, colors::TITLE_BAR_BLUE);
                            // Content
                            draw_string(
                                70,
                                75,
                                "RustOS Pixel Desktop",
                                colors::BLACK,
                                colors::BUTTON_FACE,
                            );
                            draw_string(
                                70,
                                90,
                                "R - Refresh desktop",
                                colors::BLACK,
                                colors::BUTTON_FACE,
                            );
                            draw_string(
                                70,
                                105,
                                "F1 - This help",
                                colors::BLACK,
                                colors::BUTTON_FACE,
                            );
                            draw_string(
                                70,
                                120,
                                "ESC - Close dialog",
                                colors::BLACK,
                                colors::BUTTON_FACE,
                            );
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        // Periodic updates
        if update_counter.is_multiple_of(1_000_000) {
            // Could update clock display here
        }

        // Poll network devices for incoming packets
        if update_counter.is_multiple_of(1000) {
            crate::net::poll_network();
        }

        update_counter = update_counter.wrapping_add(1);

        // Halt CPU until next interrupt to save power
        // SAFETY: Idle loop halts CPU until next interrupt. See docs/SAFETY.md#halt-loop.
        unsafe {
            core::arch::asm!("hlt");
        }
    }
}

/// Modern desktop loop that handles graphics-based desktop
///
/// This is the main event loop for the graphical desktop environment.
/// It handles:
/// - Keyboard input routing to windows
/// - Mouse cursor rendering and movement
/// - Window focus, dragging, and interaction
/// - Periodic desktop updates and rendering
extern "C" fn userspace_idle_resume() -> ! {
    loop {
        x86_64::instructions::hlt();
    }
}

fn userspace_init_idle_loop() -> ! {
    crate::serial_println!("userspace_init_idle_loop: entered");
    loop {
        crate::user_sched::service_pending(userspace_idle_resume as *const () as u64);
        // SAFETY: the boot CPU is idle here; timer interrupts wake it to run the scheduler.
        unsafe {
            core::arch::asm!("hlt");
        }
    }
}

/// Minimal compositor/idle loop for userspace sessions (GNOME).
///
/// Unlike `modern_desktop_main_loop`, this does NOT render a kernel desktop.
/// It only:
/// - Services pending user processes via `user_sched::service_pending`
/// - Forwards input events to Wayland clients
/// - Renders Wayland client surfaces
/// - Polls network devices
/// - Launches the in-kernel Mutter client when ready
fn userspace_session_loop() -> ! {
    let mut update_counter: u64 = 0;

    crate::serial_println!("userspace_session_loop: entered");

    loop {
        // Forward kernel input events to connected Wayland clients
        wayland::poll_input();

        // Launch the in-kernel Mutter client once the compositor is ready
        if mutter::should_launch() {
            crate::serial_println!("userspace_session_loop: launching Mutter client");
            if let Err(e) = mutter::launch_client() {
                crate::serial_println!("userspace_session_loop: Mutter launch failed: {}", e);
            }
        }

        // Render Wayland client surfaces (no kernel desktop underneath)
        if update_counter % 2 == 0 {
            wayland::render_clients();
            graphics::framebuffer::present();
        }

        // Refresh Mutter top bar periodically (clock updates)
        if update_counter.is_multiple_of(500) {
            mutter::update_client();
        }

        // Service pending user processes (GNOME init, etc.)
        crate::user_sched::service_pending(userspace_idle_resume as *const () as u64);

        // Process pending usermodehelper requests (firmware loading, etc.)
        if update_counter % 1000 == 0 {
            usermodehelper::process_pending();
        }

        // Poll network devices for incoming packets
        if update_counter.is_multiple_of(1000) {
            crate::net::poll_network();
        }

        // Periodic system maintenance
        if update_counter.is_multiple_of(1_000_000) {
            let _ = crate::vfs::procfs::update_gnome_status();
            dbus::emit_readiness_changed_if_needed();
        }

        update_counter = update_counter.wrapping_add(1);

        // Halt CPU until next interrupt
        // SAFETY: Idle loop halts CPU until next interrupt. See docs/SAFETY.md#halt-loop.
        unsafe {
            core::arch::asm!("hlt");
        }
    }
}

extern "C" fn modern_desktop_idle_resume() -> ! {
    // Resume point after a bootstrap user task exits during the desktop idle
    // phase. The bootstrap code jumps here directly, so this function must
    // never return. Halt the CPU until the next interrupt wakes the kernel.
    loop {
        x86_64::instructions::hlt();
    }
}

fn modern_desktop_main_loop() -> ! {
    // Desktop state
    let mut update_counter: u64 = 0;
    let mut frame_counter: usize = 0;
    let mut last_render_time: u64 = 0;
    let target_frame_time_ms: u64 = 16; // ~60 FPS target

    // Window interaction state
    let mut _dragging_window: Option<desktop::WindowId> = None;
    let mut _drag_start_x: usize = 0;
    let mut _drag_start_y: usize = 0;
    let mut _window_start_x: usize = 0;
    let mut _window_start_y: usize = 0;

    // Set cursor bounds for input manager based on actual screen dimensions
    use drivers::{get_cursor_position, set_cursor_bounds};
    let (cursor_max_x, cursor_max_y) = if let Some((w, h)) = graphics::get_screen_dimensions() {
        (w.saturating_sub(1), h.saturating_sub(1))
    } else {
        (799, 599) // Fallback to 800x600
    };
    set_cursor_bounds(cursor_max_x, cursor_max_y);

    // Initial render
    desktop::invalidate_desktop();
    desktop::render_desktop();

    // Track last cursor position to detect movement and force redraws
    let (init_x, init_y) = get_cursor_position();
    let mut last_cursor_x = init_x;
    let mut last_cursor_y = init_y;

    // Main event loop
    loop {
        let current_time = time::uptime_ms();

        // ====================================================================
        // Input Processing Phase
        // ====================================================================

        // Process all pending input events from the unified input manager
        while let Some(input_event) = drivers::get_input_event() {
            match input_event {
                drivers::InputEvent::KeyPress(key_event) => {
                    // Handle keyboard press events
                    handle_keyboard_input(key_event);
                }
                drivers::InputEvent::KeyRelease(_key_event) => {
                    // Key release events - could be used for modifier tracking
                }
                drivers::InputEvent::MouseMove { x, y } => {
                    // Real hardware mouse movement
                    desktop::handle_mouse_move(x, y);
                }
                drivers::InputEvent::MouseButtonDown { button, x, y } => {
                    // Convert input manager button to desktop button
                    let desktop_button = match button {
                        drivers::MouseButton::Left => desktop::MouseButton::Left,
                        drivers::MouseButton::Right => desktop::MouseButton::Right,
                        drivers::MouseButton::Middle => desktop::MouseButton::Middle,
                        _ => continue, // Ignore extra buttons for now
                    };
                    desktop::handle_mouse_down(x, y, desktop_button);
                }
                drivers::InputEvent::MouseButtonUp { button, x, y } => {
                    // Convert input manager button to desktop button
                    let desktop_button = match button {
                        drivers::MouseButton::Left => desktop::MouseButton::Left,
                        drivers::MouseButton::Right => desktop::MouseButton::Right,
                        drivers::MouseButton::Middle => desktop::MouseButton::Middle,
                        _ => continue, // Ignore extra buttons for now
                    };
                    desktop::handle_mouse_up(x, y, desktop_button);
                }
                drivers::InputEvent::MouseScroll { delta, x, y } => {
                    // Handle scroll wheel
                    desktop::handle_scroll(x as i32, y as i32, delta as i32);
                }
            }
        }

        // Forward kernel input events to connected Wayland clients
        wayland::poll_input();

        // Launch the in-kernel Mutter client once the compositor is ready
        if update_counter == 1 {
            log_debug!(
                "mutter",
                "readiness: overlay={} wayland={} handshake={} should_launch={}",
                crate::gnome_overlay::is_ready(),
                wayland::is_ready(),
                wayland::server::is_handshake_ready(),
                mutter::should_launch()
            );
        }
        if mutter::should_launch() {
            log_debug!("mutter", "launching in-kernel Mutter client");
            if let Err(e) = mutter::launch_client() {
                log_debug!("mutter", "client launch failed: {}", e);
            }
        }

        // ====================================================================
        // Desktop Update Phase
        // ====================================================================

        // Process pending desktop events every iteration for responsive input
        desktop::process_desktop_events();

        // Update desktop state periodically (clock, stats, file listings)
        if update_counter.is_multiple_of(500) {
            desktop::update_desktop();
            // Refresh the in-kernel Mutter client's top bar (clock updates)
            mutter::update_client();
        }

        // ====================================================================
        // Rendering Phase
        // ====================================================================

        // Check if cursor moved since last frame — if so, force a full
        // desktop redraw to overwrite the old cursor pixels (no trail).
        let (cur_x, cur_y) = get_cursor_position();
        if cur_x != last_cursor_x || cur_y != last_cursor_y {
            desktop::invalidate_desktop();
            last_cursor_x = cur_x;
            last_cursor_y = cur_y;
        }

        // Render at target frame rate or when needed
        let should_render = desktop::desktop_needs_redraw()
            || (current_time >= last_render_time + target_frame_time_ms);

        if should_render {
            // Render the desktop (windows, taskbar, dock) — this repaints
            // the area under the old cursor, erasing the trail.
            desktop::render_desktop();

            // Composite Wayland client surfaces on top of the desktop
            wayland::render_clients();

            // Get current mouse position from input manager
            let (mouse_x, mouse_y) = get_cursor_position();
            let button_state = drivers::input_manager::get_button_states();

            // Render mouse cursor overlay
            render_mouse_cursor(mouse_x, mouse_y, button_state.left);

            // Present the frame
            graphics::framebuffer::present();

            frame_counter += 1;
            last_render_time = current_time;

            // Log frame rate periodically (every 60 frames)
            if frame_counter % 60 == 0 {
                log_debug!(
                    "desktop",
                    "Frame {}, uptime {}ms",
                    frame_counter,
                    current_time
                );
            }
        }

        // ====================================================================
        // System Tasks Phase
        // ====================================================================

        // Periodic system maintenance
        if update_counter.is_multiple_of(1_000_000) {
            let _ = crate::vfs::procfs::update_gnome_status();
            dbus::emit_readiness_changed_if_needed();
        }

        // Poll network devices for incoming packets
        if update_counter.is_multiple_of(1000) {
            crate::net::poll_network();
        }

        update_counter = update_counter.wrapping_add(1);

        crate::user_sched::service_pending(modern_desktop_idle_resume as *const () as u64);

        // Halt CPU until next interrupt to save power
        // SAFETY: Idle loop halts CPU until next interrupt. See docs/SAFETY.md#halt-loop.
        unsafe {
            core::arch::asm!("hlt");
        }
    }
}

/// Handle keyboard input events (unified keyboard handler for modern desktop)
fn handle_keyboard_input(key_event: keyboard::KeyEvent) {
    match key_event {
        keyboard::KeyEvent::CharacterPress(c) => {
            let key_code = c as u8;

            // Forward character input to desktop/window manager
            desktop::handle_key_down(key_code);

            // Log significant keypresses for debugging
            if c == '\x1b' {
                // ESC
                log_debug!("input", "ESC pressed - could trigger menu");
            }
        }
        keyboard::KeyEvent::SpecialPress(special_key) => {
            // Map special keys to key codes for desktop
            let key_code = match special_key {
                keyboard::SpecialKey::Escape => 27,
                keyboard::SpecialKey::Enter => 13,
                keyboard::SpecialKey::Backspace => 8,
                keyboard::SpecialKey::Tab => 9,
                keyboard::SpecialKey::F1 => 112, // Help
                keyboard::SpecialKey::F2 => 113, // Rename
                keyboard::SpecialKey::F3 => 114, // Search
                keyboard::SpecialKey::F4 => 115, // Close (Alt+F4)
                keyboard::SpecialKey::F5 => 116, // Refresh
                keyboard::SpecialKey::F6 => 117,
                keyboard::SpecialKey::F7 => 118,
                keyboard::SpecialKey::F8 => 119,
                keyboard::SpecialKey::F9 => 120,
                keyboard::SpecialKey::F10 => 121,
                keyboard::SpecialKey::F11 => 122, // Fullscreen
                keyboard::SpecialKey::F12 => 123, // Debug console
                keyboard::SpecialKey::Insert => 45,
                keyboard::SpecialKey::Delete => 46,
                keyboard::SpecialKey::Home => 36,
                keyboard::SpecialKey::End => 35,
                keyboard::SpecialKey::PageUp => 33,
                keyboard::SpecialKey::PageDown => 34,
                keyboard::SpecialKey::ArrowUp => 38,
                keyboard::SpecialKey::ArrowDown => 40,
                keyboard::SpecialKey::ArrowLeft => 37,
                keyboard::SpecialKey::ArrowRight => 39,
                _ => return, // Ignore other special keys
            };

            desktop::handle_key_down(key_code);
        }
        _ => {}
    }
}

/// Handle keyboard character input with mouse simulation (legacy - kept for text mode desktop)
fn handle_keyboard_character(
    c: char,
    mouse_x: &mut usize,
    mouse_y: &mut usize,
    button_left: &mut bool,
    _button_right: &mut bool,
) {
    let key_code = c as u8;

    // Mouse simulation keys (WASD or similar)
    match c {
        // WASD for mouse movement
        'w' | 'W' => *mouse_y = mouse_y.saturating_sub(5),
        'a' | 'A' => *mouse_x = mouse_x.saturating_sub(5),
        's' | 'S' => *mouse_y = (*mouse_y + 5).min(479),
        'd' | 'D' => *mouse_x = (*mouse_x + 5).min(639),
        // Space for left click
        ' ' => *button_left = true,
        _ => {
            // Forward to desktop/window manager
            desktop::handle_key_down(key_code);
        }
    }

    // Log significant keypresses for debugging
    if key_code == 27 {
        // ESC
        log_debug!("input", "ESC pressed - could trigger menu");
    }
}

/// Handle special key presses (function keys, arrows, etc.)
fn handle_special_key(special_key: keyboard::SpecialKey, mouse_x: &mut usize, mouse_y: &mut usize) {
    // Arrow keys for cursor movement
    let move_amount = 10;
    match special_key {
        keyboard::SpecialKey::ArrowUp => {
            *mouse_y = mouse_y.saturating_sub(move_amount);
            desktop::handle_mouse_move(*mouse_x, *mouse_y);
            return;
        }
        keyboard::SpecialKey::ArrowDown => {
            *mouse_y = (*mouse_y + move_amount).min(479);
            desktop::handle_mouse_move(*mouse_x, *mouse_y);
            return;
        }
        keyboard::SpecialKey::ArrowLeft => {
            *mouse_x = mouse_x.saturating_sub(move_amount);
            desktop::handle_mouse_move(*mouse_x, *mouse_y);
            return;
        }
        keyboard::SpecialKey::ArrowRight => {
            *mouse_x = (*mouse_x + move_amount).min(639);
            desktop::handle_mouse_move(*mouse_x, *mouse_y);
            return;
        }
        _ => {}
    }

    let key_code = match special_key {
        keyboard::SpecialKey::Escape => 27,
        keyboard::SpecialKey::Enter => 13,
        keyboard::SpecialKey::Backspace => 8,
        keyboard::SpecialKey::Tab => 9,
        keyboard::SpecialKey::F1 => 112, // Help
        keyboard::SpecialKey::F2 => 113, // Rename
        keyboard::SpecialKey::F3 => 114, // Search
        keyboard::SpecialKey::F4 => 115, // Close (Alt+F4)
        keyboard::SpecialKey::F5 => 116, // Refresh
        keyboard::SpecialKey::F6 => 117,
        keyboard::SpecialKey::F7 => 118,
        keyboard::SpecialKey::F8 => 119,
        keyboard::SpecialKey::F9 => 120,
        keyboard::SpecialKey::F10 => 121,
        keyboard::SpecialKey::F11 => 122, // Fullscreen
        keyboard::SpecialKey::F12 => 123, // Debug console
        keyboard::SpecialKey::Insert => 45,
        keyboard::SpecialKey::Delete => 46,
        keyboard::SpecialKey::Home => 36,
        keyboard::SpecialKey::End => 35,
        keyboard::SpecialKey::PageUp => 33,
        keyboard::SpecialKey::PageDown => 34,
        _ => return, // Already handled or ignore
    };

    desktop::handle_key_down(key_code);

    // Handle special window operations
    match special_key {
        keyboard::SpecialKey::F4 => {
            // Close focused window (would need Alt modifier check)
            log_debug!("input", "F4 pressed - close window shortcut");
        }
        keyboard::SpecialKey::F11 => {
            // Toggle fullscreen
            log_debug!("input", "F11 pressed - fullscreen toggle");
        }
        keyboard::SpecialKey::F12 => {
            // Debug console toggle
            log_debug!("input", "F12 pressed - debug console");
        }
        _ => {}
    }
}

/// Render the mouse cursor at the specified position
fn render_mouse_cursor(x: usize, y: usize, pressed: bool) {
    // Get screen dimensions for bounds checking
    let (max_x, max_y) = if let Some((w, h)) = graphics::get_screen_dimensions() {
        (w, h)
    } else {
        return; // No framebuffer available
    };

    // Cursor color based on state
    let cursor_color = if pressed {
        graphics::Color::rgb(255, 200, 0) // Yellow when pressed
    } else {
        graphics::Color::WHITE
    };

    // Cursor shadow for visibility
    let shadow_color = graphics::Color::rgb(0, 0, 0);

    // Simple arrow cursor pattern (12 pixels tall)
    let cursor_pattern: [(usize, usize); 21] = [
        (0, 0),
        (0, 1),
        (1, 1),
        (0, 2),
        (1, 2),
        (2, 2),
        (0, 3),
        (1, 3),
        (2, 3),
        (3, 3),
        (0, 4),
        (1, 4),
        (2, 4),
        (3, 4),
        (4, 4),
        (0, 5),
        (1, 5),
        (2, 5),
        (0, 6),
        (1, 6),
        (3, 6),
    ];

    // Draw shadow first (offset by 1 pixel)
    for &(dx, dy) in cursor_pattern.iter() {
        let px = x + dx + 1;
        let py = y + dy + 1;
        if px < max_x && py < max_y {
            graphics::framebuffer::set_pixel(px, py, shadow_color);
        }
    }

    // Draw cursor
    for &(dx, dy) in cursor_pattern.iter() {
        let px = x + dx;
        let py = y + dy;
        if px < max_x && py < max_y {
            graphics::framebuffer::set_pixel(px, py, cursor_color);
        }
    }
}

#[no_mangle]
pub extern "C" fn rust_main() -> ! {
    // Multiboot entry is not supported in the default build path.
    // SAFETY: Raw I/O to COM1 for early logging. See docs/SAFETY.md#io-port-access.
    unsafe {
        init_early_serial();
        early_serial_write_str(
            "RustOS: multiboot entry unsupported; use bootloader/bootimage.\r\n",
        );
    }

    loop {
        // SAFETY: Halt loop in a fatal path. See docs/SAFETY.md#halt-loop.
        unsafe {
            core::arch::asm!("hlt");
        }
    }
}

#[cfg(not(test))]
fn kernel_panic(info: &PanicInfo) -> ! {
    // Write to serial first — this works without heap or any initialization.
    // SAFETY: Raw I/O to COM1 for panic diagnostics. See docs/SAFETY.md#io-port-access.
    unsafe {
        init_early_serial();
        early_serial_write_str("\r\nRustOS: KERNEL PANIC!\r\n");
        if let Some(loc) = info.location() {
            early_serial_write_str("  at ");
            early_serial_write_str(loc.file());
            early_serial_write_bytes(b":");
            early_serial_write_u64(loc.line() as u64);
            early_serial_write_bytes(b":");
            early_serial_write_u64(loc.column() as u64);
            early_serial_write_bytes(b"\r\n");
        }
        early_serial_write_str("  msg: ");
        use core::fmt::Write as _;
        struct SerialWriter;
        impl core::fmt::Write for SerialWriter {
            fn write_str(&mut self, s: &str) -> core::fmt::Result {
                for byte in s.bytes() {
                    unsafe {
                        early_serial_write_byte(byte);
                    }
                }
                Ok(())
            }
        }
        let _ = write!(SerialWriter, "{}", info.message());
        early_serial_write_bytes(b"\r\n");
        early_serial_write_str("RustOS: System halted.\r\n");
    }

    // If heap is available, try the full error handler path.
    if let Some(mut manager) = crate::error::ERROR_MANAGER.try_lock() {
        use crate::error::{ErrorContext, ErrorSeverity, KernelError, SystemError};
        let location = if let Some(loc) = info.location() {
            alloc::format!("{}:{}:{}", loc.file(), loc.line(), loc.column())
        } else {
            "unknown location".into()
        };
        let message = alloc::format!("{}", info.message());
        let error_context = ErrorContext::new(
            KernelError::System(SystemError::InternalError),
            ErrorSeverity::Fatal,
            "panic_handler",
            alloc::format!("KERNEL PANIC: {} at {}", message, location),
        );
        let _ = manager.handle_error(error_context);
    }

    loop {
        // SAFETY: Halt loop in a fatal path. See docs/SAFETY.md#halt-loop.
        unsafe {
            core::arch::asm!("hlt");
        }
    }
}

#[cfg(test)]
fn test_panic(info: &PanicInfo) -> ! {
    serial_println!("[failed]\nError: {}", info);
    exit_qemu(QemuExitCode::Failed);
    loop {
        unsafe {
            core::arch::asm!("hlt");
        }
    }
}
