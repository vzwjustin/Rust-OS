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

<!-- headroom:learn:start -->
## Headroom Learned Patterns
*Auto-generated by `headroom learn` on 2026-06-28 — do not edit manually*

### Large File Reading — Use grep/sed for glib-native Sources
*~1,200 tokens/session saved*
Read tool truncates mid-file on files >5KB in glib-native: `gaction.rs`, `gcancellable.rs`, `ginputstream.rs`, `goutputstream.rs`, `gfilterinputstream.rs`, `gfilteroutputstream.rs`, `lib.rs`, and others. Always use `grep -n` to locate line ranges, then `sed -n '<start>,<end>p'` to extract. Never Read these files whole.

### Build Environment — Cargo Flags Required
*~800 tokens/session saved*
Always prefix `cargo` commands with `CARGO_UNSTABLE_BUILD_STD=false CARGO_BUILD_TARGET=""` or use `cargo +stable --target aarch64-apple-darwin`. The root `.cargo/config.toml` sets a JSON build target that breaks stable cargo; these flags override it. Without them, cargo fails with 'target specs require -Zjson-target-spec'.

### Worktree Isolation — Dependency File Copies
*~600 tokens/session saved*
Agents in worktrees cannot read files from main checkout. When a new module depends on existing files (e.g., `gtlscertificate.rs`), explicitly `cp /Users/justinadams/Downloads/Rust-OS/glib-rust/glib/rust/glib-native/src/<dep>.rs <worktree>/glib-rust/glib/rust/glib-native/src/` before building or grep-searching. Otherwise module compilation fails with 'file not found for module'.

### lib.rs Volatility — Always Re-read Before Edit
*~500 tokens/session saved*
In glib-native, `lib.rs` (~27KB) is frequently edited by parallel agents. Edit fails with 'File has been modified since read' if stale. Always `Read lib.rs` immediately before Edit, even if you read it 5 minutes earlier. In worktrees, read and edit the worktree copy (`/Users/justinadams/Downloads/Rust-OS/.claude/worktrees/<agent-id>/...lib.rs`), not the main checkout.

### Shell Regex Syntax — ugrep Instead of GNU grep
*~400 tokens/session saved*
System uses `ugrep` not GNU grep. Do NOT use backslash escapes: `\|` for alternation fails with 'empty (sub)expression'. Use `-E` flag with modern syntax: `grep -E 'pattern1|pattern2'`. Never escape parentheses in character classes; use `grep -E 'pub fn|pub struct'` not `grep 'pub fn\|pub struct'`.

### Cargo / Build Commands
*~900 tokens/session saved*
- Plain `cargo test` / `cargo check` fails with `.json target specs require -Zjson-target-spec` — the root `.cargo/config.toml` sets a JSON build target that breaks stable cargo.
- Working test command for `glib-native`: `CARGO_UNSTABLE_BUILD_STD=false CARGO_BUILD_TARGET="" cargo +stable test --target aarch64-apple-darwin`
- Working check command: `CARGO_UNSTABLE_BUILD_STD=false CARGO_BUILD_TARGET="" cargo +stable check --target aarch64-apple-darwin`

### Large Files — glib-native
*~700 tokens/session saved*
- Many `glib-native` `.rs` files exceed the Read tool limit; the tool returns a `runtime_error` showing file content mid-file instead of an EOF error. Use `grep -n` or `sed -n '<start>,<end>p'` for these.
- Known large files requiring grep/sed: `gsocketaddress.rs`, `ginetsocketaddress.rs`, `gunixsocketaddress.rs`, `ginputstream.rs`, `goutputstream.rs`, `gcancellable.rs`, `gfilterinputstream.rs`, `gfilteroutputstream.rs`, `gaction.rs`, `gfileenumerator.rs`, `gvolume.rs`, `gmount.rs`, `gsettings.rs`, `gicon.rs`, `gloadableicon.rs`, `gfileicon.rs`.
- `lib.rs` (~27 KB+) sometimes hits the limit too; use `grep -n 'pub mod'` or `grep -n 'pub use'` to find module declarations and insertion points.

### Worktree Isolation
*~500 tokens/session saved*
- Agents launched with `isolation: 'worktree'` cannot write to the main checkout. All Writes/Edits must target the worktree copy: `/Users/justinadams/Downloads/Rust-OS/.claude/worktrees/<agent-id>/glib-rust/glib/rust/glib-native/src/<file>.rs`
- Worktrees do NOT auto-copy existing source files. When a new module depends on existing files (e.g., `gtlscertificate.rs` for TLS work), explicitly `cp` them before building: `cp /Users/justinadams/Downloads/Rust-OS/glib-rust/glib/rust/glib-native/src/<dep>.rs <worktree>/glib-rust/glib/rust/glib-native/src/`

### GLib-Native Module Pattern
*~350 tokens/session saved*
- Every new module needs two steps: (1) write `src/<modname>.rs`, (2) add `pub mod <modname>;` and `pub use <modname>::{...};` to `lib.rs`. Always Read `lib.rs` immediately before editing it — it changes frequently between parallel agents and Edit fails if the file is stale.
- When working in a worktree, read and edit the worktree copy of `lib.rs`, not the main checkout copy.

### Shell / grep
*~200 tokens/session saved*
- The system uses `ugrep` not GNU grep. The `\|` alternation syntax causes `ugrep: error: empty (sub)expression`. Use `-E` with `|`: `grep -E 'pattern1|pattern2'` — never `grep 'pattern1\|pattern2'`.
- `wc` and `awk` are unavailable in `eval` contexts inside shell for-loops. Use `python3 -c` for line counting across multiple files.

<!-- headroom:learn:end -->
