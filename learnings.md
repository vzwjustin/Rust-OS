# learnings.md

Reusable lessons, failure patterns, and debugging heuristics for RustOS. Updated as new patterns are discovered.

---

## Build System

### The `-Zjson-target-spec` flag is required on current nightly
- **Pattern:** Build fails with `error: .json target specs require -Zjson-target-spec`
- **Root cause:** Newer nightly Rust requires an explicit unstable flag to use custom JSON target specifications.
- **Fix:** Add `-Zjson-target-spec` to all cargo invocations that use `--target x86_64-rustos.json`.
- **Watch out:** `Makefile` and `build_rustos.sh` may not include this flag yet. Always test with the direct cargo command first.

### `rust-src` component must be installed for `-Zbuild-std`
- **Pattern:** Build fails with `Cargo.lock does not exist, unable to build with the standard library`
- **Fix:** `rustup component add rust-src --toolchain nightly-x86_64-unknown-linux-gnu`
- **Note:** This is not always pre-installed in fresh environments or containers.

### Full build command (known working as of 2026-04-02)
```bash
cargo +nightly build --bin rustos \
    -Zbuild-std=core,compiler_builtins,alloc \
    -Zjson-target-spec \
    --target x86_64-rustos.json
```

### Clippy requires separate installation on nightly
- **Pattern:** `cargo clippy` fails with `'cargo-clippy' is not installed for the toolchain`
- **Fix:** `rustup component add clippy --toolchain nightly-x86_64-unknown-linux-gnu`

---

## Code Patterns

### Duplicate definitions cause cascading errors
- **Pattern:** A trait or struct defined in two places (e.g., `NetworkDriver` was in both `drivers/network/mod.rs` and another file) causes 50+ downstream errors.
- **Heuristic:** When you see a large cluster of "method not a member of trait" errors, search for duplicate trait definitions first before fixing individual call sites.
- **Evidence:** BUILD_STATUS.md documents this as a historical issue that generated 500+ errors.

### Module registration is in `main.rs`, not `lib.rs`
- **Pattern:** New modules must be declared with `mod module_name;` in `src/main.rs`.
- **Why:** The `[lib]` section in Cargo.toml is commented out. There is no `lib.rs`. The binary crate in `main.rs` is the root.
- **Gotcha:** If you create a new module file and forget to add the `mod` declaration in `main.rs`, it will simply not compile and may produce confusing "unresolved import" errors in other files that try to use it.

### `#![no_std]` means no `println!` from std
- **Pattern:** The kernel defines its own `print!` / `println!` macros that write to VGA buffer.
- **Gotcha:** Do not use `std::println!`. The macros in `src/print.rs` route through `vga_buffer.rs`.
- **Serial output:** For QEMU debugging, use the serial port functions, not VGA.

### `kernel::init_all_subsystems()` exists but is never called from main.rs
- **Pattern:** `src/kernel.rs` contains a sophisticated subsystem registry that initializes ACPI, PCI, memory, network, etc. But `kernel_main` in `main.rs` never calls it.
- **Why:** It's only used in the alternative `main_integrated.rs` entry point.
- **Impact:** 20+ subsystems are declared as modules but never initialized in the active boot path.
- **Heuristic:** If you're adding a new subsystem, check whether you need to add an init call to `kernel_main` directly or wire through `kernel::init_all_subsystems()`.

### `Vec::with_capacity()` + `set_len()` is UB
- **Pattern:** `src/drivers/network/realtek.rs` does `Vec::with_capacity(n)` followed by `set_len(n)`, which reads uninitialized memory.
- **Fix:** Use `vec![0u8; n]` or `Vec::resize(n, 0)` instead.
- **Evidence:** Clippy deny-level error at lines 679 and 730.

### Port number wrapping comparisons are always true for u16
- **Pattern:** `if port >= 65535` where `port` is `u16` — this is always true when port is 65535 (the max value), making the comparison tautological.
- **Files affected:** `src/net/tcp.rs:470`, `src/net/udp.rs:436,440,446`, `src/net/icmp.rs:236`, `src/drivers/storage/nvme.rs:490`
- **Fix:** Use `u16::MAX` comparison or wrapping arithmetic.

---

## Verification Pitfalls

### "It compiles" does not mean "it works"
- **Pattern:** This kernel has 40+ modules that all compile, but many may be dead code not reachable from the boot sequence.
- **Evidence:** 3166 warnings, many of which are `dead_code`.
- **Heuristic:** After adding a new subsystem, trace whether `kernel_main` actually calls its initialization. Compilation is necessary but nowhere near sufficient for a kernel.

### Stale documentation is the norm, not the exception
- **Pattern:** BUILD_STATUS.md says 117 errors when there are actually 0. ROADMAP.md claims 100% build completion but there's no CI.
- **Heuristic:** Always run the build yourself. Never trust markdown status files without re-verification.
- **Rule:** When you fix something, update the governance files. When you read a claim, verify it.

### Warning count is a health metric
- **Pattern:** 3166 warnings (build) / 3631 warnings (clippy) is very high. Among them are `dead_code`, `unused_imports`, and critically, 66 shared references to mutable statics (undefined behavior).
- **Heuristic:** The warning count should decrease over time. A PR that increases warnings significantly should be scrutinized.

### Clippy reveals logic bugs that `cargo build` misses
- **Pattern:** `cargo build` passes with 0 errors, but `cargo clippy` finds 18 deny-level errors including loops that never loop, operations that always return zero, and reads of uninitialized memory.
- **Heuristic:** Always run clippy, not just build. Build success is necessary but not sufficient.
- **Key bugs found:** Serial receive broken (loops exit immediately), NVMe doorbells always zero, Realtek driver reads uninitialized buffers.

---

## Architecture Gotchas

### Two memory modules exist
- `memory_basic.rs` — Simpler memory management
- `memory.rs` + `memory_manager/` — Full virtual memory management
- **Danger:** Changes to memory management must consider which module is actually being used in the active boot path.

### Two process management modules exist
- `process/` — Core process lifecycle
- `process_manager/` — Higher-level process APIs
- **Heuristic:** Check both before assuming how process management works.

### Network module aliasing
- `src/net/` is declared as `pub mod net` and then re-exported as `pub use net as network`.
- **Gotcha:** Both `net::` and `network::` paths work. Be consistent — prefer `net::` as the canonical path.

---

## Debugging Heuristics

### For "cannot find type/function" errors
1. Check if the module is declared in `main.rs`
2. Check if the item is `pub`
3. Check if there's a `use` import at the call site
4. Check for typos in method names (e.g., `exec` vs `execve`)

### For "method not a member of trait" errors
1. Check the trait definition file (usually `mod.rs`)
2. Compare method signature exactly (name, params, return type)
3. Search for duplicate trait definitions across files

### For bootloader compatibility issues
- The kernel uses `bootloader` v0.9.23, not the newer `bootloader_api`.
- Types like `MemoryMap` and `MemoryRegionType` come from the `bootloader` crate's specific version.
- Do not upgrade to `bootloader_api` without a full migration plan.

---

## Session Discipline

### Re-read before editing
- Context decay is real. If you read a file earlier in the session and are now editing it, re-read it first. The file may have been modified by a previous edit in the same session.

### Large file awareness
- `src/main.rs` has 40+ module declarations and kernel_main implementation — read in chunks.
- `src/pci/database.rs` has 500+ device entries — do not read the whole thing unless searching for a specific device.
- Network driver files can be large — read the trait definition in `mod.rs` before reading individual driver implementations.
