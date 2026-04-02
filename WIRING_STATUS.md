# WIRING_STATUS.md

Evidence-backed verification ledger for RustOS. This file stores verification status, not hopes or guesses.

**Last updated:** 2026-04-02 (audit pass 2)
**Verified by:** cargo build + cargo clippy + init sequence trace

---

## 1. Executive Verdict

**The kernel compiles with 0 errors and 3166 warnings.** No runtime validation has been performed. Many subsystems are structurally present (code exists, compiles) but functionally unproven (never executed against real or emulated hardware in a verified test).

**Clippy found 18 deny-level errors** (logic bugs, not just style). These include loops that never loop, comparisons that are always true/false, operations that always return zero, and `set_len()` on uninitialized buffers. These are real bugs in the code.

**Confidence level:** LOW. Compilation success proves syntax and type correctness but clippy reveals logic bugs. Additionally, the init sequence trace shows most subsystems are declared but never initialized from the boot path.

---

## 2. Source Inputs

| Input | Location | Last Checked |
|-------|----------|--------------|
| Cargo.toml | `/Cargo.toml` | 2026-04-02 |
| Main entry | `src/main.rs` | 2026-04-02 |
| Target spec | `x86_64-rustos.json` | 2026-04-02 |
| Build output | `cargo +nightly build` | 2026-04-02 |

---

## 3. Verification Coverage Map

| Check | Available | Last Run | Result |
|-------|-----------|----------|--------|
| `cargo build` | Yes | 2026-04-02 | PASS (0 errors, 3166 warnings) |
| `cargo clippy` | Yes (via `make lint`) | 2026-04-02 | FAIL (18 errors, 3631 warnings) |
| `rustfmt` | Yes (via `make format`) | Not run this session | Unknown |
| `cargo test` | Partial (bare-metal target) | Not run | N/A — requires QEMU |
| QEMU boot test | Yes (via `make run`) | Not run this session | Unknown |
| Boot smoke test | Yes (via `make boot-smoke`) | Not run this session | Unknown |
| Integration tests | Exist (`src/integration_tests.rs`) | Not run | Unknown |

---

## 4. Subsystem Inventory

### Proven: Compiles
### Unproven: Runtime behavior
### Init Trace: Which subsystems are actually called from kernel_main

| Subsystem | Compiles | Init Called | Clippy Clean | Runtime Validated |
|-----------|:--------:|:----------:|:------------:|:-----------------:|
| GDT | Yes | **Yes** (`gdt::init()`) | Unknown | No |
| Interrupts / IDT | Yes | **Yes** (`interrupts::init()`) | Unknown | No |
| Memory (heap) | Yes | **Yes** (`memory_basic::init_heap_from_memory_map()`) | Unknown | No |
| VGA text output | Yes | **Yes** (`vga_buffer::init()`) | Unknown | No |
| Serial output | Yes | **Yes** (`init_early_serial()`) | **No** (loops never loop) | No |
| Error handling | Yes | **Yes** (`error::init_error_handling()`) | Unknown | No |
| Health monitoring | Yes | **Yes** (`health::init_health_monitoring()`) | Unknown | No |
| Logging | Yes | **Yes** (`logging::init_logging_and_debugging()`) | Unknown | No |
| Syscall (fast) | Yes | **Yes** (conditional `syscall_fast::init()`) | Unknown | No |
| Time / RTC | Yes | **Yes** (`time::init_system_time_from_rtc()`) | Unknown | No |
| Linux integration | Yes | **Yes** (`linux_integration::init()`) | Unknown | No |
| Desktop (pixel/modern) | Yes | **Yes** (desktop main loop) | Unknown | No |
| VGA Mode 13h | Yes | **Yes** (`set_phys_mem_offset()`) | Unknown | No |
| ACPI parsing | Yes | **No** (wrapped in boot_ui, not directly called) | Unknown | No |
| APIC (Local + IO) | Yes | **No** | Unknown | No |
| PCI enumeration | Yes | **No** | **No** (always-true comparison) | No |
| Memory manager (VM) | Yes | **No** | Unknown | No |
| Process management | Yes | **No** | Unknown | No |
| Process manager | Yes | **No** | **No** (loops never loop) | No |
| Scheduler | Yes | **No** | Unknown | No |
| Syscall (INT 0x80) | Yes | **No** | Unknown | No |
| Filesystem (VFS) | Yes | **No** | Unknown | No |
| Network stack (TCP/IP) | Yes | **No** | **No** (always-true comparisons) | No |
| Network drivers | Yes | **No** | **No** (uninitialized buffer UB) | No |
| Storage drivers | Yes | **No** | **No** (always-zero ops, always-true cmp) | No |
| GPU abstraction | Yes | **No** | **No** (equal operands to `/`) | No |
| Graphics | Yes | **No** | **No** (framebuffer error) | No |
| Linux compat layer | Yes | **No** | Unknown | No |
| ELF loader | Yes | **No** | Unknown | No |
| IPC | Yes | **No** | Unknown | No |
| Package management | Yes | **No** | Unknown | No |
| Security (ring levels) | Yes | **No** | Unknown | No |
| SMP support | Yes | **No** | Unknown | No |
| Arch (CPUID) | Yes | **No** | Unknown | No |
| Testing framework | Yes | **No** | **No** (always-true comparison) | No |

---

## 5. Reachability Chains

The kernel entry point (`kernel_main` in `src/main.rs`) is the sole entry. All modules are registered via `mod` declarations in `main.rs`. Init sequence has been traced.

**Reachable from kernel_main (13 subsystems):**
`kernel_main` → `init_early_serial()` → `vga_buffer::init()` → `memory_basic::init_heap_from_memory_map()` → `vga_mode13h::set_phys_mem_offset()` → boot_ui phases → `error::init_error_handling()` → `health::init_health_monitoring()` → `logging::init_logging_and_debugging()` → `gdt::init()` → `interrupts::init()` → `syscall_fast::init()` (conditional) → `time::init_system_time_from_rtc()` → `interrupts::enable_timer_interrupt()` → `interrupts::enable_keyboard_interrupt()` → `linux_integration::init()` → desktop main loop

**NOT reachable from kernel_main (20+ subsystems):**
ACPI, APIC, PCI, memory_manager, process, process_manager, scheduler, syscall (INT 0x80), fs, vfs, net, drivers (network/storage), GPU, graphics, ELF loader, IPC, package, security, SMP, arch, testing, performance.

**Critical finding:** `kernel::init()` and `kernel::init_all_subsystems()` exist in `src/kernel.rs` as a sophisticated subsystem registry and dispatcher, but they are **never called from main.rs**. They are only used in the alternative `main_integrated.rs` entry point.

**Dead code assessment:** The majority of the codebase (~60-70% of modules) is structurally present but not wired into the active boot path. The 1169 dead-code function warnings confirm this.

---

## 6. Warning and Error Analysis

### Cargo Build Warnings: 3166 (363 auto-fixable)

| Category | Count | Risk |
|----------|------:|------|
| Dead functions | 1169 | Low — but indicates unwired subsystems |
| Dead constants | 538 | Low |
| Dead structs | 355 | Low |
| Unused variables | 227 | Low |
| Dead enums | 108 | Low |
| Unused imports | 130 | Low (auto-fixable) |
| Dead statics | 64 | Low |
| Dead struct fields | 62+ | Low |
| Mutable static refs | 66 | **HIGH — undefined behavior** |
| Other | ~347 | Mixed |

### Clippy Errors: 18 (deny-level logic bugs)

| Error | File | Line | Severity |
|-------|------|------|----------|
| Equal operands to `/` | `src/graphics/framebuffer.rs` | 1398 | Medium — `0x4 / 4` is suspicious |
| Loop never loops | `src/serial.rs` | 32, 49 | **High — serial receive is broken** |
| Loop never loops | `src/process_manager/operations.rs` | 90, 140 | Medium — dead code path |
| Always-true comparison | `src/pci/mod.rs` | 604 | Low — bounds check on u8 vs MAX |
| Always-zero operation | `src/drivers/storage/nvme.rs` | 682, 719 | **High — Queue 0 doorbell calc wrong** |
| Always-true comparison | `src/drivers/storage/nvme.rs` | 490 | Low — u16 >= 65535 |
| `set_len()` on uninit buffer | `src/drivers/network/realtek.rs` | 679, 730 | **HIGH — reads uninitialized memory** |
| Always-true comparison | `src/net/tcp.rs` | 470 | Low — u16 >= 65535 |
| Always-true comparison | `src/net/udp.rs` | 436, 440, 446 | Low — u16 >= 65535 |
| Always-true comparison | `src/net/icmp.rs` | 236 | Low — u16 >= 65535 |
| Always-true comparison | `src/testing/integration_tests.rs` | 632 | Low — unsigned >= 0 |

### Mutable Static UB: 66 instances across 13 files

| File | Count | Risk |
|------|------:|------|
| `src/graphics/framebuffer.rs` | 3 | High |
| `src/arch.rs` | 3 | High |
| `src/usermode_test.rs` | 2 | High |
| `src/testing_framework.rs` | 2 | Medium |
| `src/testing/benchmarking.rs` | 2 | Medium |
| `src/gpu/opensource/mesa_compat.rs` | 2 | Medium |
| `src/gpu/opensource/drm_compat.rs` | 2 | Medium |
| `src/package/syscalls.rs` | 1 | Medium |
| `src/memory_basic.rs` | 1 | High (memory allocator) |
| `src/gdt.rs` | 1 | High (core kernel) |
| `src/drivers/storage/filesystem_interface.rs` | 1 | Medium |
| `src/drivers/network/mod.rs` | 1 | Medium |
| `src/boot_ui.rs` | 1 | Medium |

**Risk assessment:** The Realtek driver `set_len()` bug creates uninitialized memory reads, which is **undefined behavior** and a potential security vulnerability. The NVMe doorbell calculations always returning zero would make the storage driver non-functional. The serial port loops that never loop mean serial receive is broken.

---

## 7. Build Configuration Status

| Config Item | Expected | Actual | Match |
|-------------|----------|--------|:-----:|
| Binary target | `src/main.rs` | `src/main.rs` | Yes |
| `[lib]` section | Disabled | Commented out | Yes |
| Panic strategy | `abort` | `abort` (both profiles) | Yes |
| LTO (release) | Enabled | `lto = true` | Yes |
| Nightly required | Yes | Yes (features: `abi_x86_interrupt`) | Yes |
| `-Zjson-target-spec` | Required | Not in Makefile/scripts | **MISMATCH** |

**MISMATCH:** The build requires `-Zjson-target-spec` on current nightly, but this flag is not present in `Makefile` or `build_rustos.sh`. The build scripts may fail unless they already handle this internally.

---

## 8. Documentation vs. Reality Contradictions

| Claim (in docs) | Reality | Source |
|------------------|---------|--------|
| CLAUDE.md: binary is `main_simple.rs` | Binary is `main.rs` per Cargo.toml | Fixed in this session |
| BUILD_STATUS.md: 117 errors remaining | 0 errors now | BUILD_STATUS.md is stale |
| ROADMAP.md: Build & Test 100% Complete | No CI/CD exists, no automated tests run | ROADMAP.md overstates |
| Various docs claim features "production-ready" | No runtime validation performed | Aspirational language |

---

## 9. Highest-Value Next Actions

1. **Fix Realtek driver UB** (`src/drivers/network/realtek.rs:679,730`) — `set_len()` on uninitialized buffer is undefined behavior and a security risk. Use `resize()` or zero-fill instead.
2. **Fix NVMe doorbell calculations** (`src/drivers/storage/nvme.rs:682,719`) — Always-zero operations make Queue 0 doorbell non-functional. The `0 * 2` should likely be a queue index variable.
3. **Fix serial receive loops** (`src/serial.rs:32,49`) — Loops that never loop mean serial input is broken.
4. **Fix `-Zjson-target-spec` in build scripts** — `build_rustos.sh` lines 215, 221, 261, 291 all need this flag. Makefile delegates to this script, so fixing the script fixes all Make targets.
5. **Wire subsystems into boot path** — 20+ subsystems compile but are never initialized. Either call `kernel::init_all_subsystems()` from `kernel_main` or add individual init calls.
6. **Address 66 mutable static UB instances** — Convert to `spin::Once`, `spin::Mutex`, or `core::cell::SyncUnsafeCell` patterns. Priority: `memory_basic.rs`, `gdt.rs`, `arch.rs` (core boot path).
7. **Run `make run` or QEMU boot test** — Determine if the kernel actually boots with the 13 wired subsystems.
8. **Reduce warnings** — Run `cargo fix --bin rustos` for 363 auto-fixable suggestions, then address dead code.

---

## 10. Evidence Appendix

### Build Evidence (2026-04-02)
```
$ cargo +nightly build --bin rustos -Zbuild-std=core,compiler_builtins,alloc -Zjson-target-spec --target x86_64-rustos.json
warning: `rustos` (bin "rustos") generated 3166 warnings (run `cargo fix --bin "rustos" -p rustos` to apply 363 suggestions)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.14s
```

### Clippy Evidence (2026-04-02)
```
$ cargo +nightly clippy --bin rustos -Zbuild-std=core,compiler_builtins,alloc -Zjson-target-spec --target x86_64-rustos.json
error: could not compile `rustos` (bin "rustos") due to 18 previous errors; 3631 warnings emitted
```

### Warning Breakdown (top categories)
```
1169 dead functions
 538 dead constants
 355 dead structs
 227 unused variables
 108 dead enums
 130 unused imports
  66 mutable static references (UB)
  64 dead statics
```

### Cargo.toml Binary Target
```toml
[[bin]]
name = "rustos"
path = "src/main.rs"
```

### Build Script Flag Gap
`build_rustos.sh` cargo commands at lines 215, 221, 261, 291 all lack `-Zjson-target-spec`.
