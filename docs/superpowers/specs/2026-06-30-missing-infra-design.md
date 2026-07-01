# Missing Infrastructure Design — RustOS

**Date:** 2026-06-30  
**Scope:** Four confirmed unwired/incomplete gaps found by audit

---

## Gap 1: Page Fault Handler → Memory Manager Wiring

### Problem

`src/interrupts.rs` `page_fault_handler` calls a custom `attempt_page_fault_recovery()` → `attempt_swap_in_page()` chain that bypasses `MemoryManager::handle_page_fault()` entirely. Consequences:

- Copy-on-write (CoW) faults are never handled correctly from hardware
- Guard page violations reach the error manager instead of killing the process
- Protection violations in userspace processes don't send SIGSEGV
- `try_fast_page_fault_handler()` exists in `memory.rs` but is never called

### Design

Replace the custom recovery chain in `page_fault_handler` with a three-tier dispatch:

**Tier 1 — fast path (no lock, O(1)):**  
Call `memory::try_fast_page_fault_handler(fault_address)`.  
Returns `true` if the fault was an anonymous demand page (stack growth, heap, data). Returns `false` otherwise.

**Tier 2 — full MM path:**  
Call `memory::handle_page_fault(fault_address, error_code.bits())`.  
Handles: CoW remapping, swap-in, write protection checks, guard page detection, user-accessible checks.  
Returns `Ok(())` on success, `Err(_)` if the fault is unrecoverable.

**Tier 3 — process termination or kernel panic:**  
- `PROTECTION_VIOLATION` in kernel mode → kernel panic (unreachable if kernel is correct)
- `PROTECTION_VIOLATION` in user mode → send SIGSEGV to current process, yield scheduler
- Unrecoverable fault from Tier 2 → same SIGSEGV path for user; panic for kernel

**Userfaultfd** check stays at the top of the handler (before Tier 1), unchanged.

**Remove from main flow:**  
`attempt_page_fault_recovery` and `attempt_swap_in_page` are no longer called from the interrupt handler. They may remain as dead code or be deleted.

### Files Changed

- `src/interrupts.rs`: rewrite `page_fault_handler` body

---

## Gap 2: io_uring Missing Operations

### Problem

15 io_uring operations return `ENOSYS`. `IoUring` struct has no registered-buffer or registered-file tables. `register()` accepts opcode 0 but stores nothing.

### Design

#### Struct additions

Add to `IoUring`:
```
registered_buffers: Vec<(u64, u64)>   // (addr, len) per registered iovec
registered_files:   Vec<Option<i32>>  // fixed file descriptor table
timeout_deadlines:  Vec<(u64, u64)>   // (sqe_user_data, deadline_ns)
```

Add `RegisteredIoVec { addr: u64, len: u64 }` (repr C) for parsing from userspace.

#### `register()` opcodes

| Opcode | Name | Action |
|--------|------|--------|
| 0 | `IORING_REGISTER_BUFFERS` | Read `nr_args` iovecs from userspace into `registered_buffers` |
| 1 | `IORING_UNREGISTER_BUFFERS` | Clear `registered_buffers` |
| 2 | `IORING_REGISTER_FILES` | Read `nr_args` fds from userspace into `registered_files` |
| 3 | `IORING_UNREGISTER_FILES` | Clear `registered_files` |
| 7 | `IORING_REGISTER_FILES_UPDATE` | Update individual slots in `registered_files` from a `FilesUpdate` struct |

#### Missing ops in `process_sqe()`

| Op | Implementation |
|----|---------------|
| `READV`/`WRITEV` with offset | When `sqe.off != u64::MAX`, call `preadv2`/`pwritev2` with the offset; else `readv`/`writev` |
| `READ_FIXED` / `WRITE_FIXED` | Validate `sqe.buf_index < registered_buffers.len()`; get `(addr, len)`; call `read`/`write` on that buffer slice |
| `SYNC_FILE_RANGE` | Call `fsync(sqe.fd)` — full fsync is a safe superset of partial range sync |
| `OPENAT2` | Read path from `sqe.addr`, call `openat2(sqe.fd, path, sqe.open_flags())` |
| `SENDMSG` | Read `msghdr` from `sqe.addr`, call `sendmsg(sqe.fd, &msg, sqe.msg_flags)` |
| `RECVMSG` | Read `msghdr` from `sqe.addr`, call `recvmsg(sqe.fd, &mut msg, sqe.msg_flags)`, write back |
| `FADVISE` | Call `posix_fadvise(sqe.fd, sqe.off, sqe.len, sqe.fadvise_advice)` — advisory, returns `Ok(0)` |
| `MADVISE` | Call `madvise(sqe.addr, sqe.len, sqe.fadvise_advice)` — advisory, returns `Ok(0)` |
| `SPLICE` | Read `splice_fd_in` + `splice_off_in` from SQE, splice into `sqe.fd` at `sqe.off`, up to `sqe.len` bytes |
| `TEE` | Duplicate from `splice_fd_in` to `sqe.fd`, `sqe.len` bytes |
| `POLL_REMOVE` | Look up pending poll op by `sqe.addr` (user_data) in ring's pending list, cancel it |
| `ASYNC_CANCEL` | Cancel any pending SQE whose `user_data == sqe.addr`; post `-ECANCELED` CQE |
| `TIMEOUT_REMOVE` | Remove entry from `timeout_deadlines` where `deadline.0 == sqe.addr` |
| `LINK_TIMEOUT` | Push `(next_sqe_user_data, deadline_from_sqe_timespec)` onto `timeout_deadlines` |
| `FILES_UPDATE` | Read update array from userspace, update slots in `registered_files` |

### Files Changed

- `src/io_uring.rs`: extend `IoUring` struct, update `register()`, fill in all 15 ops in `process_sqe()`

---

## Gap 3: Futex REQUEUE_PI / CMP_REQUEUE_PI

### Problem

`FUTEX_WAIT_REQUEUE_PI` and `FUTEX_CMP_REQUEUE_PI` return `ENOSYS`. These are used by glibc for `pthread_cond_wait` with priority-inheritance mutexes.

### Design

#### New state

```rust
struct PiState {
    owner_pid:      u32,
    saved_priority: u8,           // owner's original priority before boost
    waiters:        VecDeque<(u32, u8)>,  // (waiter_pid, waiter_priority)
}

static PI_STATES: Mutex<BTreeMap<usize, PiState>> = ...;
```

The key is the virtual address of the PI futex (uaddr2 as `usize`).

#### `FUTEX_WAIT_REQUEUE_PI`

1. Atomically check `*uaddr == val`; if not equal return `-EAGAIN`
2. Add current process to `FUTEX_BUCKETS[hash(uaddr)]` with a `pi_requeue_key = Some(uaddr2 as usize)`
3. Block current process
4. On wakeup (from `futex_requeue_pi_wake`): try to acquire `uaddr2`:
   - CAS `*uaddr2` from `0` to `current_tid` succeeds → done
   - CAS fails (another owner) → register in `PI_STATES[uaddr2]`, boost owner priority, block again

#### `FUTEX_CMP_REQUEUE_PI`

1. Check `*uaddr == cmpval`, return `-EAGAIN` if not
2. Wake up to `val` waiters directly from `FUTEX_BUCKETS[hash(uaddr)]`
3. Requeue up to `val2` waiters: move from `FUTEX_BUCKETS[hash(uaddr)]` to `PI_STATES[uaddr2].waiters`
   - For each requeued waiter: if current owner exists, boost owner priority to `max(owner_prio, waiter_prio)`
4. Return `(woken + requeued) as i32`

#### `futex_pi_unlock(uaddr2)`

Called by the lock owner when releasing the PI mutex:
1. Remove `PI_STATES[uaddr2]`
2. Restore owner's saved priority
3. If `waiters` is non-empty: pop highest-priority waiter, set `*uaddr2 = waiter_tid`, unblock waiter

Add dispatch in `futex_syscall()`:
```
FUTEX_WAIT_REQUEUE_PI => futex_wait_requeue_pi(uaddr, val, uaddr2, timeout)
FUTEX_CMP_REQUEUE_PI  => futex_cmp_requeue_pi(uaddr, uaddr2, val, val2, cmpval)
```

### Files Changed

- `src/futex.rs`: add `PI_STATES`, `PiState`, `futex_wait_requeue_pi`, `futex_cmp_requeue_pi`, `futex_pi_unlock`, wire dispatch

---

## Gap 4: sysctl Fallthrough → ENOENT

### Problem

The `sysctl()` handler in `src/linux_compat/sysinfo_ops.rs` returns `ENOSYS` for unknown MIBs. `ENOSYS` means "syscall not implemented" — the correct error for an unknown sysctl key is `ENOENT`.

Several common MIBs are also missing, causing programs to see `ENOSYS` when they query standard kernel tunables.

### Design

Add missing MIB entries:
- `[1, 65]` → `net.core.somaxconn = 128`
- `[4, 2]` → `kernel.printk = 4` (log level)
- `[1, 7]` → `net.ipv4.ip_forward = 0`
- `[2, 11]` → `vm.dirty_background_ratio = 10`
- `[2, 17]` → `vm.nr_hugepages = 0`

Change fallthrough `Err(LinuxError::ENOSYS)` → `Err(LinuxError::ENOENT)`.

### Files Changed

- `src/linux_compat/sysinfo_ops.rs`: add MIB entries, fix fallthrough error code

---

## Implementation Order

1. Gap 4 (sysctl) — 5-minute fix, no risk
2. Gap 1 (page fault) — foundational; do before anything that loads user processes
3. Gap 2 (io_uring) — self-contained, no ordering dependency
4. Gap 3 (futex PI) — self-contained, no ordering dependency

## Testing

Each gap has a natural smoke test:
1. **Page fault:** boot to desktop, open a terminal → stack/heap demand paging works; mmap a private file and write to it → CoW works
2. **io_uring:** run any program that uses `liburing` with fixed buffers (e.g., a simple file copy test)
3. **Futex PI:** `pthread_cond_wait` on a `PTHREAD_MUTEX_ERRORCHECK`-type condvar
4. **sysctl:** `sysctl net.core.somaxconn` returns `128` instead of an error
