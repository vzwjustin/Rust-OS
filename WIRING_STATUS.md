# WIRING_STATUS.md

Evidence-backed verification ledger for RustOS. This file stores verification status, not hopes or guesses.

**Last updated:** 2026-04-02
**Verified by:** Automated build check

---

## 1. Executive Verdict

**The kernel compiles with 0 errors and 3166 warnings.** No runtime validation has been performed. Many subsystems are structurally present (code exists, compiles) but functionally unproven (never executed against real or emulated hardware in a verified test).

**Confidence level:** LOW-MEDIUM. Compilation success proves syntax and type correctness. It does not prove functional correctness, initialization ordering, or runtime behavior.

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
| `cargo clippy` | Yes (via `make lint`) | Not run this session | Unknown |
| `rustfmt` | Yes (via `make format`) | Not run this session | Unknown |
| `cargo test` | Partial (bare-metal target) | Not run | N/A — requires QEMU |
| QEMU boot test | Yes (via `make run`) | Not run this session | Unknown |
| Boot smoke test | Yes (via `make boot-smoke`) | Not run this session | Unknown |
| Integration tests | Exist (`src/integration_tests.rs`) | Not run | Unknown |

---

## 4. Subsystem Inventory

### Proven: Compiles
### Unproven: Runtime behavior

| Subsystem | Files Exist | Compiles | Tested | Runtime Validated |
|-----------|:-----------:|:--------:|:------:|:-----------------:|
| GDT / IDT | Yes | Yes | No | No |
| Memory management | Yes | Yes | No | No |
| VGA text output | Yes | Yes | No | No |
| Serial output | Yes | Yes | No | No |
| ACPI parsing | Yes | Yes | No | No |
| APIC (Local + IO) | Yes | Yes | No | No |
| PCI enumeration | Yes | Yes | No | No |
| Process management | Yes | Yes | No | No |
| Scheduler | Yes | Yes | No | No |
| Syscall interface | Yes | Yes | No | No |
| Filesystem (VFS) | Yes | Yes | No | No |
| Network stack (TCP/IP) | Yes | Yes | No | No |
| Network drivers | Yes | Yes | No | No |
| Storage drivers | Yes | Yes | No | No |
| GPU abstraction | Yes | Yes | No | No |
| Desktop environment | Yes | Yes | No | No |
| Linux compat layer | Yes | Yes | No | No |
| ELF loader | Yes | Yes | No | No |
| IPC | Yes | Yes | No | No |
| Package management | Yes | Yes | No | No |
| Security (ring levels) | Yes | Yes | No | No |
| SMP support | Yes | Yes | No | No |

---

## 5. Reachability Chains

The kernel entry point (`kernel_main` in `src/main.rs`) is the sole entry. All modules are registered via `mod` declarations in `main.rs`. Whether each module's `init()` or equivalent is actually *called* from the boot sequence has not been traced in this session.

**Known reachable from entry:** `kernel_main` → (needs trace of init calls)
**Potentially dead code:** 3166 warnings include many `dead_code` warnings — significant portions of declared modules may not be reachable from the boot path.

---

## 6. Warning Analysis

Total warnings: **3166** (363 auto-fixable suggestions)

Key warning categories (not exhaustively counted):
- `dead_code` — Functions/structs declared but never called
- `unused_imports` — Imported items not referenced
- `unsafe` usage — Shared references to mutable statics (UB risk in `usermode_test.rs`)
- `unused_variables` — Variables assigned but not read

**Risk assessment:** The `dead_code` warnings suggest many subsystems are structurally present but not wired into the boot/init sequence. The mutable static warnings in `usermode_test.rs` indicate potential undefined behavior.

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

1. **Run `make run` or QEMU boot test** — Determine if the kernel actually boots. This is the single highest-value verification step.
2. **Run `make lint`** — Get clippy diagnostics to identify logic bugs beyond type errors.
3. **Trace init sequence** — Map which subsystem `init()` calls are reachable from `kernel_main` to identify dead subsystems.
4. **Fix `-Zjson-target-spec`** — Update Makefile/build scripts to include this flag so builds work out of the box.
5. **Address mutable static UB** — Fix `usermode_test.rs` shared references to mutable statics.
6. **Reduce warning count** — The 363 auto-fixable suggestions are low-hanging fruit.

---

## 10. Evidence Appendix

### Build Evidence (2026-04-02)
```
$ cargo +nightly build --bin rustos -Zbuild-std=core,compiler_builtins,alloc -Zjson-target-spec --target x86_64-rustos.json
warning: `rustos` (bin "rustos") generated 3166 warnings (run `cargo fix --bin "rustos" -p rustos` to apply 363 suggestions)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.14s
```

### Cargo.toml Binary Target
```toml
[[bin]]
name = "rustos"
path = "src/main.rs"
```
