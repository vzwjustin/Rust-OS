# RustOS Missing Codebase Audit - 2026-06-30

Branch: `audit/missing-linux-boot-wide`
Linux source of truth: `/home/justin/Downloads/linux-master(1)/linux-master`

This is a live audit index for missing implementation work. It intentionally
separates real implementation gaps from current compile blockers and from
intentional compatibility stubs.

## Command Evidence

- `LINUX_MASTER_DIR='/home/justin/Downloads/linux-master(1)/linux-master' rtk bash scripts/check_linux_mirror.sh`
  - Linux driver dirs: 144
  - RustOS effective driver modules: 171
  - Pending driver mirrors: 13
- `rtk rg -n "Err\(LinuxError::ENOSYS\)|LinuxError::ENOSYS|=> -38|ENOSYS" src --glob '!target/**'`
  - Active `ENOSYS`/`-38` sites remain in `linux_integration`, `io_uring`, `futex`, `keyring`, `seccomp`, `ptrace`, `linux_compat/*`, and `acpi`.
- `CC=clang RUSTC_WRAPPER= CARGO_BUILD_RUSTC_WRAPPER= make check`
  - Current build stops on unrelated compile blockers before boot/runtime validation.

## Current Compile Blockers

These are not necessarily Linux-parity gaps, but they block reliable audit
verification until fixed.

1. Missing module file: `error[E0583]`.
2. Duplicate/import-name errors:
   - `src/drivers/base/mod.rs:37`
   - `src/drivers/base/mod.rs:463`
   - `src/drivers/platform/mod.rs:53`
3. Bad `crate::EINVAL` references:
   - `src/page.rs:112`
   - `src/page.rs:124`
4. Unsafe pointer operations needing explicit unsafe blocks:
   - `src/linux_rust/bitmap.rs:64`
   - `src/linux_rust/bitmap.rs:89`

## Highest-Priority Missing Areas

### 1. Linux kexec file boot handoff

Status: partially wired.

Evidence:
- `src/linux_compat/sysinfo_ops.rs:661-663` routes `LINUX_REBOOT_CMD_KEXEC` to `crate::kexec::execute_loaded_image()`.
- `src/linux_integration.rs:1554-1555` exposes `KexecFileLoad`.
- `src/kexec.rs` now parses ELF64 and x86 bzImage payloads into staged segments.

Remaining gap:
- Linux `arch/x86/kernel/kexec-bzimage64.c` builds boot params, command line,
  initrd placement, EFI/kexec flags, and setup data. RustOS currently jumps to
  the parsed entry with copied segments, but does not yet build a Linux
  zero-page/`boot_params` handoff.

### 2. io_uring operation coverage

Status: many operations are structurally present, but `ENOSYS` remains.

Evidence:
- `src/io_uring.rs` declares operations through at least `IORING_OP_SYMLINKAT`.
- `src/io_uring.rs:1273` still returns `-(LinuxError::ENOSYS as i32)` for unsupported operations.
- Prior plan: `docs/superpowers/plans/2026-06-30-missing-infra.md` tracks the concrete missing operations.

Primary missing operations to verify next:
- fixed-buffer read/write
- sync-file-range
- openat2
- sendmsg/recvmsg
- fadvise/madvise
- splice/tee
- poll remove
- async cancel
- link timeout / timeout remove
- files update

### 3. Futex PI/requeue behavior

Status: non-PI behavior exists; PI/requeue remains incomplete.

Evidence:
- `src/futex.rs:895-899` and `src/linux_compat/thread_ops.rs:554-598` identify `FUTEX_WAIT_REQUEUE_PI` and `FUTEX_CMP_REQUEUE_PI`.
- `src/futex.rs` still has an explicit `ENOSYS` path for these operations.

Remaining gap:
- Linux-compatible PI futex state, requeue validation, and wake ownership
  semantics.

### 4. Security and LSM syscalls

Status: mostly shape-only.

Evidence:
- `src/linux_integration.rs:1207` returns `ENOSYS` for `Security`.
- `src/linux_integration.rs:1432-1437` handles LSM get/list minimally but leaves `LsmSetSelfAttr` as `ENOSYS`.
- `src/seccomp.rs:260` retains `-38`.

Remaining gap:
- Real LSM attribute mutation, seccomp filter enforcement parity, and security
  syscall routing policy.

### 5. Keyring unsupported commands

Status: key permission work is in progress, but crypto/watch/restrict gaps remain.

Evidence:
- `src/keyring.rs:742-756` leaves `KEYCTL_RESTRICT_KEYRING`,
  `KEYCTL_WATCH_KEY`, and public-key/DH operations unsupported.

Remaining gap:
- Watch notifications, keyring restrictions, and asymmetric key operations.

### 6. Filesystem and memory compatibility ENOSYS paths

Status: multiple VFS compatibility conversions still map unsupported behavior to
`ENOSYS`.

Evidence:
- `src/linux_compat/fs_ops.rs:728`, `:762`, `:796`, `:818`, `:831`, `:921`, `:944`
- `src/linux_compat/memory_ops.rs:1398`
- `src/linux_compat/file_ops.rs:188`
- `src/linux_compat/advanced_io.rs:30`

Remaining gap:
- Decide per call whether Linux expects `ENOSYS`, `EINVAL`, `EOPNOTSUPP`,
  `ENOENT`, or a real implementation.

### 7. ACPI power-management hooks

Status: ACPICA/AML support is wired separately, but PM hooks remain missing.

Evidence:
- `src/acpi/mod.rs:1565-1597` documents PM `ENOSYS` paths.

Remaining gap:
- ACPI-backed suspend/power transition hooks and notifier integration.

### 8. Driver mirror directory gaps

Status: 13 Linux driver directories have no Rust-owned mirror module.

Pending modules from `scripts/check_linux_mirror.sh`:
- `drivers/accessibility`
- `drivers/dibs`
- `drivers/dio`
- `drivers/macintosh`
- `drivers/nubus`
- `drivers/parisc`
- `drivers/ps3`
- `drivers/s390`
- `drivers/sbus`
- `drivers/sh`
- `drivers/staging`
- `drivers/tc`
- `drivers/zorro`

These are low boot priority unless a target platform or dependency pulls them in.

## Next Audit Pass

1. Fix current compile blockers so `make check` becomes a usable verification gate.
2. Trace kexec bzImage handoff against Linux `arch/x86/kernel/kexec-bzimage64.c`
   and add a boot-params/zero-page work item or implementation.
3. Convert `ENOSYS` inventory into a table with syscall number, Linux source file,
   current RustOS behavior, expected errno/behavior, and test command.
4. Re-run `TMPDIR=/tmp CC=clang RUSTC_WRAPPER= CARGO_BUILD_RUSTC_WRAPPER= make boot-smoke`
   only after compile blockers are gone.
