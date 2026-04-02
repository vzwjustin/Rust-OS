# context.md

Current project truth for RustOS. Read after `CLAUDE.md`. Code reality wins over this file.

## Project Overview

RustOS is a bare-metal x86_64 operating system kernel written in Rust. It targets `x86_64-rustos.json` (a custom target spec) and boots via the `bootloader` crate (v0.9.23 with `map_physical_memory`). The kernel is `#![no_std]` and `#![no_main]`, using the `entry_point!` macro from `bootloader`.

**Maturity:** ~35% complete. Core foundation compiles. Many subsystems are structurally present but not runtime-validated. No CI/CD pipeline exists.

## Repo Shape

- **Language:** Rust (nightly required)
- **Build system:** Makefile wrapping `cargo +nightly build` with `-Zbuild-std=core,compiler_builtins,alloc -Zjson-target-spec`
- **Target:** `x86_64-rustos.json` (custom JSON target spec)
- **Entry point:** `src/main.rs` (full-featured kernel with 40+ modules)
- **Alternative entry:** `src/main_simple.rs` (minimal bootable kernel, not currently the default)
- **Library crate:** Disabled (`src/lib.rs.bak` exists but `[lib]` is commented out in `Cargo.toml`)
- **Tests:** `src/integration_tests.rs`, `src/stress_tests.rs`, `src/testing/`, `tests/` — but cannot run until runtime environment is functional
- **Docs:** 92+ markdown files across `docs/`, `claudedocs/`, `codex/`, and module READMEs
- **Containerization:** Dockerfile + docker-compose for Linux and macOS
- **No GitHub Actions or CI/CD configuration**

## Architecture Boundaries

### Kernel Core (always loaded)
- `gdt.rs` — Global Descriptor Table
- `interrupts.rs` — IDT and interrupt handlers
- `memory.rs` / `memory_basic.rs` — Physical/virtual memory management
- `memory_manager/` — Higher-level virtual memory manager
- `vga_buffer.rs` / `print.rs` — Console output via VGA text mode at `0xB8000`
- `serial.rs` — COM1 serial output (used for early debug and QEMU output)

### Hardware Abstraction
- `acpi/` — RSDP, RSDT/XSDT, MADT, FADT, MCFG table parsing
- `apic/` — Local APIC + IO APIC for interrupt routing
- `pci/` — PCI/PCIe enumeration with 500+ device database
- `arch.rs` — CPUID-based CPU feature detection
- `smp.rs` — Symmetric multiprocessing via APIC IPI
- `time.rs` — PIT and TSC timers

### Subsystems
- `process/` + `process_manager/` — Process lifecycle, context switching
- `scheduler/` — Preemptive scheduling with SMP load balancing
- `syscall/` + `syscall_handler.rs` + `syscall_fast.rs` — POSIX-compatible syscall interface
- `fs/` + `vfs/` — Virtual filesystem with RamFS, DevFS
- `net/` — TCP/IP stack (Ethernet, IPv4, TCP, UDP, sockets)
- `drivers/network/` — Intel E1000, Realtek, Broadcom, Atheros WiFi
- `drivers/storage/` — AHCI, NVMe, IDE
- `ipc.rs` — Pipes, queues, semaphores, shared memory
- `security.rs` — Ring 0-3 access control

### Graphics / Desktop
- `gpu/` — Multi-vendor GPU abstraction (Intel, NVIDIA, AMD)
- `gpu/opensource/` — Nouveau, AMDGPU, i915 driver stubs
- `graphics/` — Framebuffer management
- `desktop/` — Window manager and desktop environment
- `vga_mode13h.rs` — VGA Mode 13h (320x200, 256 colors)

### Linux Compatibility
- `linux_compat/` — Linux API compatibility layer
- `linux_integration.rs` — Integration glue
- `elf_loader/` — ELF binary loading
- `usermode.rs` / `usermode_test.rs` — User-mode execution support
- `initramfs.rs` — Initial ramdisk support

### Support
- `io_optimized.rs` — I/O scheduler with priority queuing
- `performance.rs` / `performance_monitor.rs` — Perf counters (RDPMC)
- `health.rs` — System health monitoring
- `logging.rs` — Kernel logging
- `error.rs` — Error handling and recovery
- `package/` — Experimental package management
- `data_structures.rs` — Kernel data structures
- `intrinsics.rs` — Compiler intrinsics for missing symbols

## Boundary Catalog

| Boundary | Inside | Outside | Contract |
|----------|--------|---------|----------|
| Kernel ↔ Hardware | All `src/` code | Physical hardware / QEMU | Port I/O, MMIO, interrupts |
| Kernel ↔ Userspace | Ring 0 kernel | Ring 3 user processes | Syscall interface (INT 0x80 + SYSCALL/SYSRET) |
| Bootloader ↔ Kernel | `kernel_main(boot_info)` | `bootloader` crate | `BootInfo` struct with memory map |
| VGA ↔ Console | `vga_buffer.rs`, `print.rs` | Display hardware | Direct memory write to `0xB8000` |
| Serial ↔ Host | `serial.rs` | QEMU stdio | COM1 port `0x3F8` |

## Source-of-Truth Ownership

| Artifact | Owner |
|----------|-------|
| Build commands | `Makefile` (canonical), `build_rustos.sh` (scripted) |
| Dependencies | `Cargo.toml` |
| Target spec | `x86_64-rustos.json` |
| Toolchain | `rust-toolchain.toml` |
| Boot config | `boot_config.txt`, linker scripts (`link.ld`, `linker.ld`) |
| Module registration | `src/main.rs` (all `mod` declarations) |

## Known Invariants

1. The kernel runs in Ring 0 on x86_64 with no standard library.
2. All heap allocation goes through `ALLOCATOR` (a `LockedHeap`), initialized in `kernel_main`.
3. The `bootloader` crate provides the entry point and physical memory map.
4. VGA text buffer is at physical address `0xB8000`.
5. COM1 serial port is at I/O port `0x3F8`.
6. Panic handler halts the CPU in a loop (`hlt`).
7. Both `dev` and `release` profiles use `panic = "abort"`.

## Risks and Assumptions

- **No CI/CD:** Build status is only known by running locally. Regressions can go undetected.
- **2683 warnings, 0 clippy errors:** All 18 clippy deny-level bugs fixed. Remaining warnings are mostly `dead_code`.
- **Mutable static UB:** All 66 instances fixed via `addr_of!`/`addr_of_mut!` patterns.
- **~8 subsystems still unwired:** 20 of 28 subsystems now initialized from `kernel_main`. GPU, memory_manager, ELF loader, IPC, and testing framework remain unwired.
- **No runtime testing in CI:** QEMU-based tests require manual execution.
- **Stale documentation:** Many docs reference planned features as complete. Cross-check against code.
- **Build scripts:** Fixed — `build_rustos.sh` now includes `-Zjson-target-spec` at all cargo invocation points.
- **Linux compat layer:** Structurally present but many method signatures are misaligned; not functionally tested.

## Current Known State (as of 2026-04-02)

- **Build:** Compiles successfully with 0 errors, 2683 warnings (nightly toolchain)
- **Clippy:** PASSES with 0 errors, 3148 warnings
- **Build command:** `cargo +nightly build --bin rustos -Zbuild-std=core,compiler_builtins,alloc -Zjson-target-spec --target x86_64-rustos.json`
- **Tests:** Cannot be meaningfully run (kernel requires bare-metal / QEMU environment)
- **Runtime validation:** Not performed in this session
- **Binary target:** `src/main.rs` (Cargo.toml `[[bin]]` section)
- **Init coverage:** 20 subsystems wired into boot sequence (up from 13); 8 still unwired
- **Mutable static UB:** 0 (all 66 instances fixed)
- **Build scripts:** Fixed — `build_rustos.sh` now includes `-Zjson-target-spec`
