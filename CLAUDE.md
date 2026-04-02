# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Startup Sequence

Upon reading this file:
1. This document becomes active policy for the session.
2. Read `context.md`, `WIRING_STATUS.md`, and `learnings.md` if present — they form the repo memory system.
3. Compare current code reality against these docs. Code and evidence win over stale documentation.
4. Do not claim success until the strongest applicable repo-native checks have been run.

## Verification Discipline

- Distinguish between: edited, built, linted, tested, and runtime-validated states.
- A file that compiles is not verified. A test that passes is not runtime-validated.
- When fixing bugs, apply root-cause reasoning (5 Whys) before patching symptoms.
- Newly surfaced issues must be classified: direct cause, indirect exposure, pre-existing-but-blocking, or truly unrelated (with evidence).

## Project Overview

RustOS is a production-ready operating system kernel written in Rust, featuring hardware abstraction, network stack, process management, GPU acceleration, and AI integration. This is a bare-metal x86_64 kernel that boots via multiboot.

## Essential Build Commands

### Primary Build Methods
```bash
# Using Makefile (recommended)
make build              # Build debug kernel
make build-release      # Build release kernel
make run               # Build and run in QEMU
make run-release       # Build and run release in QEMU
make test              # Run kernel tests
make clean             # Clean build artifacts
make check             # Check compilation without building
make format            # Format code with rustfmt
make lint              # Run clippy linter

# Using build script
./build_rustos.sh                # Build debug kernel
./build_rustos.sh --release      # Build release kernel
./build_rustos.sh --check-only   # Check compilation

# Direct cargo commands (requires nightly)
cargo +nightly build --bin rustos -Zbuild-std=core,compiler_builtins --target x86_64-rustos.json
```

### Testing a Single Component
```bash
# Test specific module (example)
cargo test -p rustos --lib memory
```

### Creating Bootable Images
```bash
./create_bootimage.sh      # Create bootable image
./create_final_multiboot.sh # Create multiboot kernel
make bootimage             # Create bootable debug image
make bootimage-release     # Create bootable release image
```

## High-Level Architecture

### Core Kernel Design
The kernel follows a modular architecture with clear separation between subsystems:

1. **Hardware Abstraction Layer (HAL)**
   - ACPI subsystem (`src/acpi/`) - Parses RSDP, RSDT/XSDT, MADT, FADT, MCFG tables for hardware discovery
   - APIC system (`src/apic/`) - Manages Local APIC and IO APIC for modern interrupt handling
   - PCI subsystem (`src/pci/`) - Enumerates PCI/PCIe devices, supports hot-plug detection

2. **Core Kernel Services**
   - Process Management (`src/process/`) - Implements process lifecycle, context switching, synchronization primitives
   - Memory Management (`src/memory.rs`) - Zone-based allocation with bootloader integration
   - Scheduler (`src/scheduler/`) - Preemptive scheduling with SMP load balancing
   - System Calls (`src/syscall/`) - POSIX-compatible syscall interface

3. **Network Stack**
   - Full TCP/IP implementation (`src/net/`) - Ethernet, IPv4, TCP, UDP protocols
   - Socket interface with connection management
   - Zero-copy I/O for performance
   - Network device drivers (`src/drivers/network/`) - Intel, Realtek, Broadcom NICs

4. **Graphics and Desktop**
   - GPU acceleration (`src/gpu/`) - Multi-vendor support (Intel, NVIDIA, AMD)
   - Open source drivers integration (`src/gpu/opensource/`) - Nouveau, AMDGPU, i915
   - Desktop environment (`src/desktop/`) - Hardware-accelerated windowing system

5. **AI Integration**
   - Predictive health monitoring and autonomous recovery
   - System optimization through machine learning
   - Located in AI-related code sections within main kernel

### Entry Points
- **Main kernel**: `src/main.rs` - Full-featured kernel with all subsystems
- **Simplified kernel**: `src/main_simple.rs` - Minimal bootable kernel
- **Library interface**: `src/lib.rs.bak` - Exposes kernel functionality as a library

### Critical Dependencies
- Rust nightly toolchain (required for no_std and kernel features)
- Target specification: `x86_64-rustos.json`
- Key crates: `bootloader`, `x86_64`, `linked_list_allocator`, `spin`

## Module Organization

### Core Systems
- `gdt.rs` - Global Descriptor Table setup
- `interrupts.rs` - Interrupt handling and IDT
- `memory.rs` - Memory management and allocation
- `process/` - Process management subsystem
  - `mod.rs` - Process lifecycle
  - `scheduler.rs` - Scheduling algorithms
  - `context.rs` - Context switching
  - `sync.rs` - Synchronization primitives

### Hardware Support
- `acpi/` - ACPI table parsing and hardware discovery
- `apic/` - Advanced Programmable Interrupt Controller
- `pci/` - PCI bus management
  - `config.rs` - Configuration space access
  - `database.rs` - Device ID database (500+ devices)
  - `detection.rs` - Hardware detection and classification
- `drivers/` - Device driver framework
  - `network/` - Network drivers
  - `storage/` - Storage drivers (AHCI, NVMe, IDE)
  - `vbe.rs` - VESA BIOS Extensions

### Network Stack
- `net/` - Core networking
  - `ethernet.rs` - Ethernet frame processing
  - `ip.rs` - IPv4 implementation
  - `tcp.rs` - TCP protocol
  - `udp.rs` - UDP protocol
  - `socket.rs` - Socket interface

### Performance Optimization
- `*_optimized.rs` files - Optimized implementations of core systems
- `performance_monitor.rs` - System metrics and analytics
- `benchmarking.rs` - Performance benchmarking utilities

## Development Workflow

### Setting Up Development Environment
1. Install Rust nightly: `rustup toolchain install nightly`
2. Add required components: `rustup component add rust-src llvm-tools-preview`
3. Install QEMU for testing: Platform-specific installation
4. Optional: Install bootimage tool for creating bootable images

### Code Style and Conventions
- The codebase uses standard Rust formatting (rustfmt)
- Follow existing patterns in neighboring files
- Use existing libraries and utilities rather than adding new dependencies
- Security: Never commit secrets or keys

### Testing Strategy
- Unit tests within modules
- Integration tests in `src/integration_tests.rs`
- Stress tests in `src/stress_tests.rs`
- Run with `make test` or specific module tests

## Important Notes

### Current Build Configuration
- Main binary path set to `src/main.rs` in Cargo.toml (full-featured kernel)
- Library functionality commented out (no `lib.rs`, using `lib.rs.bak`)
- Multiboot support through assembly boot code (`src/boot.s`)
- Build requires `-Zjson-target-spec` flag with nightly toolchain

### Key Constants and Configuration
- Kernel heap: Starts at `memory::KERNEL_HEAP_START`, size `memory::KERNEL_HEAP_SIZE`
- VGA buffer: Located at `0xb8000`
- Target architecture: x86_64 with custom target JSON

### Active Development Areas
- Inter-Process Communication (IPC) - In progress
- Security framework - Next priority
- ELF loader and user processes - Planned
- Advanced memory management (virtual memory, demand paging) - Planned

The kernel is approximately 35% complete with core foundation 100% ready for advanced feature development.

## Governance Files

This repository uses four governance files that form an interdependent repo memory system:

| File | Purpose |
|------|---------|
| `CLAUDE.md` | Operating policy — non-negotiable agent behaviors and build commands |
| `context.md` | Current project truth — architecture, boundaries, assumptions |
| `WIRING_STATUS.md` | Evidence-backed verification ledger — what is proven vs. unproven |
| `learnings.md` | Reusable lessons — recurring failure patterns and debugging heuristics |

These files must be updated when repository reality changes. Do not leave optimistic status text when current evidence disproves earlier claims.