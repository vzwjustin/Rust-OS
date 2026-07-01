# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

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
- Main binary path set to `src/main_simple.rs` in Cargo.toml
- Library functionality commented out (no `lib.rs`, using `lib.rs.bak`)
- Multiboot support through assembly boot code (`src/boot.s`)

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

<!-- rtk-instructions v2 -->
# RTK (Rust Token Killer) - Token-Optimized Commands

## Golden Rule

**Always prefix commands with `rtk`**. If RTK has a dedicated filter, it uses it. If not, it passes through unchanged. This means RTK is always safe to use.

**Important**: Even in command chains with `&&`, use `rtk`:
```bash
# ❌ Wrong
git add . && git commit -m "msg" && git push

# ✅ Correct
rtk git add . && rtk git commit -m "msg" && rtk git push
```

## RTK Commands by Workflow

### Build & Compile (80-90% savings)
```bash
rtk cargo build         # Cargo build output
rtk cargo check         # Cargo check output
rtk cargo clippy        # Clippy warnings grouped by file (80%)
rtk tsc                 # TypeScript errors grouped by file/code (83%)
rtk lint                # ESLint/Biome violations grouped (84%)
rtk prettier --check    # Files needing format only (70%)
rtk next build          # Next.js build with route metrics (87%)
```

### Test (60-99% savings)
```bash
rtk cargo test          # Cargo test failures only (90%)
rtk go test             # Go test failures only (90%)
rtk jest                # Jest failures only (99.5%)
rtk vitest              # Vitest failures only (99.5%)
rtk playwright test     # Playwright failures only (94%)
rtk pytest              # Python test failures only (90%)
rtk rake test           # Ruby test failures only (90%)
rtk rspec               # RSpec test failures only (60%)
rtk test <cmd>          # Generic test wrapper - failures only
```

### Git (59-80% savings)
```bash
rtk git status          # Compact status
rtk git log             # Compact log (works with all git flags)
rtk git diff            # Compact diff (80%)
rtk git show            # Compact show (80%)
rtk git add             # Ultra-compact confirmations (59%)
rtk git commit          # Ultra-compact confirmations (59%)
rtk git push            # Ultra-compact confirmations
rtk git pull            # Ultra-compact confirmations
rtk git branch          # Compact branch list
rtk git fetch           # Compact fetch
rtk git stash           # Compact stash
rtk git worktree        # Compact worktree
```

Note: Git passthrough works for ALL subcommands, even those not explicitly listed.

### GitHub (26-87% savings)
```bash
rtk gh pr view <num>    # Compact PR view (87%)
rtk gh pr checks        # Compact PR checks (79%)
rtk gh run list         # Compact workflow runs (82%)
rtk gh issue list       # Compact issue list (80%)
rtk gh api              # Compact API responses (26%)
```

### JavaScript/TypeScript Tooling (70-90% savings)
```bash
rtk pnpm list           # Compact dependency tree (70%)
rtk pnpm outdated       # Compact outdated packages (80%)
rtk pnpm install        # Compact install output (90%)
rtk npm run <script>    # Compact npm script output
rtk npx <cmd>           # Compact npx command output
rtk prisma              # Prisma without ASCII art (88%)
```

### Files & Search (60-75% savings)
```bash
rtk ls <path>           # Tree format, compact (65%)
rtk read <file>         # Code reading with filtering (60%)
rtk grep <pattern>      # Search grouped by file (75%). Format flags (-c, -l, -L, -o, -Z) run raw.
rtk find <pattern>      # Find grouped by directory (70%)
```

### Analysis & Debug (70-90% savings)
```bash
rtk err <cmd>           # Filter errors only from any command
rtk log <file>          # Deduplicated logs with counts
rtk json <file>         # JSON structure without values
rtk deps                # Dependency overview
rtk env                 # Environment variables compact
rtk summary <cmd>       # Smart summary of command output
rtk diff                # Ultra-compact diffs
```

### Infrastructure (85% savings)
```bash
rtk docker ps           # Compact container list
rtk docker images       # Compact image list
rtk docker logs <c>     # Deduplicated logs
rtk kubectl get         # Compact resource list
rtk kubectl logs        # Deduplicated pod logs
```

### Network (65-70% savings)
```bash
rtk curl <url>          # Compact HTTP responses (70%)
rtk wget <url>          # Compact download output (65%)
```

### Meta Commands
```bash
rtk gain                # View token savings statistics
rtk gain --history      # View command history with savings
rtk discover            # Analyze Claude Code sessions for missed RTK usage
rtk proxy <cmd>         # Run command without filtering (for debugging)
rtk init                # Add RTK instructions to CLAUDE.md
rtk init --global       # Add RTK to ~/.claude/CLAUDE.md
```

## Token Savings Overview

| Category | Commands | Typical Savings |
|----------|----------|-----------------|
| Tests | vitest, playwright, cargo test | 90-99% |
| Build | next, tsc, lint, prettier | 70-87% |
| Git | status, log, diff, add, commit | 59-80% |
| GitHub | gh pr, gh run, gh issue | 26-87% |
| Package Managers | pnpm, npm, npx | 70-90% |
| Files | ls, read, grep, find | 60-75% |
| Infrastructure | docker, kubectl | 85% |
| Network | curl, wget | 65-70% |

Overall average: **60-90% token reduction** on common development operations.
<!-- /rtk-instructions -->

## Local Reading Preferences
- When using `sed` for targeted code reads, include about 30 lines before and after the target so surrounding context is visible.
