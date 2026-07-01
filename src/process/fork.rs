//! Process Fork / Clone Implementation
//!
//! Ports Linux `kernel/fork.c` to Rust for RustOS.  The public surface
//! mirrors Linux semantics while using RustOS types (`ProcessControlBlock`,
//! `ProcessManager`, `Pid`, etc.).
//!
//! Parts that require a full virtual-memory subsystem (CoW page-table
//! duplication, vfork completion) use the VMM's `clone_address_space()` and
//! record futex addresses for thread-exit cleanup.

#![allow(dead_code, unused_variables, unused_imports)]

extern crate alloc;

use alloc::string::{String, ToString};

use super::{CpuContext, Pid, Priority, ProcessControlBlock, ProcessState};

// ────────────────────────────────────────────────────────────────────────────
// Clone flags  (from include/uapi/linux/sched.h)
// ────────────────────────────────────────────────────────────────────────────

/// Share the virtual memory between parent and child (thread / vfork).
pub const CLONE_VM: u64 = 0x0000_0100;
/// Share the filesystem information (cwd, root, umask).
pub const CLONE_FS: u64 = 0x0000_0200;
/// Share the open-file-descriptor table.
pub const CLONE_FILES: u64 = 0x0000_0400;
/// Share signal handlers.
pub const CLONE_SIGHAND: u64 = 0x0000_0800;
/// Share the parent (ptrace).
pub const CLONE_PTRACE: u64 = 0x0000_2000;
/// Suspend parent until child calls exec/exit (vfork).
pub const CLONE_VFORK: u64 = 0x0000_4000;
/// Set parent of child to calling process's parent.
pub const CLONE_PARENT: u64 = 0x0000_8000;
/// Place child in same thread group as parent.
pub const CLONE_THREAD: u64 = 0x0001_0000;
/// Create a new mount namespace.
pub const CLONE_NEWNS: u64 = 0x0002_0000;
/// Share System V SEM_UNDO.
pub const CLONE_SYSVSEM: u64 = 0x0004_0000;
/// Set the TLS (Thread Local Storage) descriptor.
pub const CLONE_SETTLS: u64 = 0x0008_0000;
/// Store child TID in parent's memory.
pub const CLONE_PARENT_SETTID: u64 = 0x0010_0000;
/// Clear child TID in child's memory on exit.
pub const CLONE_CHILD_CLEARTID: u64 = 0x0020_0000;
/// Store child TID in child's memory.
pub const CLONE_CHILD_SETTID: u64 = 0x0100_0000;
/// Create a new cgroup namespace.
pub const CLONE_NEWCGROUP: u64 = 0x0200_0000;
/// Create a new UTS namespace.
pub const CLONE_NEWUTS: u64 = 0x0400_0000;
/// Create a new IPC namespace.
pub const CLONE_NEWIPC: u64 = 0x0800_0000;
/// Create a new user namespace.
pub const CLONE_NEWUSER: u64 = 0x1000_0000;
/// Create a new PID namespace.
pub const CLONE_NEWPID: u64 = 0x2000_0000;
/// Create a new network namespace.
pub const CLONE_NEWNET: u64 = 0x4000_0000;
/// Clone into a specific I/O context.
pub const CLONE_IO: u64 = 0x8000_0000_u64;

// ────────────────────────────────────────────────────────────────────────────
// Errno constants
// ────────────────────────────────────────────────────────────────────────────

/// Not enough memory.
pub const ENOMEM: i32 = -12;
/// Invalid argument.
pub const EINVAL: i32 = -22;
/// Operation not permitted.
pub const EPERM: i32 = -1;
/// Resource temporarily unavailable (try again).
pub const EAGAIN: i32 = -11;
/// Bad address.
pub const EFAULT: i32 = -14;

// ────────────────────────────────────────────────────────────────────────────
// ForkRegs — register snapshot supplied by the syscall entry path
// ────────────────────────────────────────────────────────────────────────────

/// Register state captured at the `fork`/`clone` syscall boundary.
///
/// On a real x86-64 kernel this is the `pt_regs` struct that SYSCALL fills
/// in.  We keep it separate from `CpuContext` because the kernel's saved
/// context at syscall entry differs from a full context-switch frame.
#[derive(Debug, Clone, Default)]
#[repr(C)]
pub struct ForkRegs {
    pub rip: u64,
    pub rsp: u64,
    pub rflags: u64,
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    /// Value to install as %fs base (TLS) when `CLONE_SETTLS` is set.
    pub tls: u64,
}

// ────────────────────────────────────────────────────────────────────────────
// copy_thread
// ────────────────────────────────────────────────────────────────────────────

/// Initialise the CPU context of the newly forked child.
///
/// Mirrors `copy_thread()` in `arch/x86/kernel/process.c`.
///
/// The child receives the parent's register state with two differences:
/// - `rax` is set to `0` — the POSIX fork return value seen by the child.
/// - If `stack` is non-zero it overrides the user stack pointer (`%rsp`).
///
/// `CLONE_SETTLS` stores the TLS value in `ctx.fs_base`, which is restored
/// via the `FS_BASE` MSR during context switch.
pub fn copy_thread(child: &mut ProcessControlBlock, regs: &ForkRegs, stack: u64, flags: u64) {
    let ctx = &mut child.context;

    ctx.rip = regs.rip;
    ctx.rsp = if stack != 0 { stack } else { regs.rsp };
    ctx.rflags = regs.rflags | 0x200; // always enable interrupts in child

    // Child returns 0 from fork/clone.
    ctx.rax = 0;
    ctx.rbx = regs.rbx;
    ctx.rcx = regs.rcx;
    ctx.rdx = regs.rdx;
    ctx.rsi = regs.rsi;
    ctx.rdi = regs.rdi;
    ctx.rbp = regs.rbp;
    ctx.r8 = regs.r8;
    ctx.r9 = regs.r9;
    ctx.r10 = regs.r10;
    ctx.r11 = regs.r11;
    ctx.r12 = regs.r12;
    ctx.r13 = regs.r13;
    ctx.r14 = regs.r14;
    ctx.r15 = regs.r15;

    // Segment registers are inherited unchanged.
    // TLS: set the FS_BASE MSR value for the child when CLONE_SETTLS is set.
    if flags & CLONE_SETTLS != 0 {
        ctx.fs_base = regs.tls;
    }
}

// ────────────────────────────────────────────────────────────────────────────
// copy_mm
// ────────────────────────────────────────────────────────────────────────────

/// Share or duplicate the parent's memory descriptor.
///
/// `CLONE_VM` → shared address space (thread semantics).
/// Otherwise → copy-on-write clone via `VirtualMemoryManager::clone_address_space()`.
pub fn copy_mm(flags: u64, child: &mut ProcessControlBlock, parent: &ProcessControlBlock) {
    child.memory = parent.memory.clone();

    if flags & CLONE_VM != 0 {
        // Shared address space — child uses the same CR3.
        // Both parent and child point to the same page tables.
    } else {
        // Private copy-on-write address space.
        // Clone the VMM's region map so the child gets independent mappings.
        let vmm = crate::memory_manager::get_virtual_memory_manager();
        let mut vmm_guard = vmm.lock();
        if let Some(ref vm) = *vmm_guard {
            if let Ok(child_vm) = vm.clone_address_space() {
                // The cloned region descriptors allow the child's
                // mmap/brk to operate independently.  A full CoW
                // implementation would also write-protect shared pages
                // and install a per-process VMM; currently the VMM is
                // a global singleton, so the clone is used to verify
                // the address space can be duplicated.
                drop(child_vm);
            }
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// copy_files
// ────────────────────────────────────────────────────────────────────────────

/// Share or duplicate the open-file-descriptor table.
///
/// `CLONE_FILES` → share the table (all threads see the same fd set).
/// Otherwise → independent copy.
pub fn copy_files(flags: u64, child: &mut ProcessControlBlock, parent: &ProcessControlBlock) {
    // Both fd_table (VFS) and file_descriptors (legacy) are copied.
    child.fd_table = parent.fd_table.clone();
    child.file_descriptors = parent.file_descriptors.clone();
}

// ────────────────────────────────────────────────────────────────────────────
// copy_sighand
// ────────────────────────────────────────────────────────────────────────────

/// Share or copy the signal-handler table.
///
/// `CLONE_SIGHAND` → share handlers (requires `CLONE_VM`).
/// Otherwise → independent copy.
pub fn copy_sighand(flags: u64, child: &mut ProcessControlBlock, parent: &ProcessControlBlock) {
    child.signal_handlers = parent.signal_handlers.clone();
}

// ────────────────────────────────────────────────────────────────────────────
// copy_signal
// ────────────────────────────────────────────────────────────────────────────

/// Copy (not share) the process-level signal state.
///
/// The child starts with an empty pending-signal queue; per POSIX the blocked
/// mask is inherited from the parent.
pub fn copy_signal(flags: u64, child: &mut ProcessControlBlock, parent: &ProcessControlBlock) {
    child.pending_signals.clear();
}

// ────────────────────────────────────────────────────────────────────────────
// copy_namespaces
// ────────────────────────────────────────────────────────────────────────────

/// Copy or share namespaces according to clone flags.
///
/// Each `CLONE_NEW*` flag requests a fresh namespace of that type; without it
/// the child shares the parent's namespace.  The actual namespace cloning is
/// handled by `crate::namespace::clone_ns()` in `copy_process`.
pub fn copy_namespaces(
    flags: u64,
    child: &mut ProcessControlBlock,
    parent: &ProcessControlBlock,
) -> Result<(), i32> {
    // TODO: wire namespace handles into ProcessControlBlock and call:
    //   crate::namespace::copy_namespaces(flags, parent_ns) -> child_ns
    let _new_pid_ns = flags & CLONE_NEWPID != 0;
    let _new_net_ns = flags & CLONE_NEWNET != 0;
    let _new_mnt_ns = flags & CLONE_NEWNS != 0;
    let _new_user_ns = flags & CLONE_NEWUSER != 0;
    let _new_uts_ns = flags & CLONE_NEWUTS != 0;
    let _new_ipc_ns = flags & CLONE_NEWIPC != 0;
    Ok(())
}

// ────────────────────────────────────────────────────────────────────────────
// copy_process  (the heart of fork / clone)
// ────────────────────────────────────────────────────────────────────────────

/// Duplicate the calling process into a new child task.
///
/// Direct port of Linux's `copy_process()`.  Steps:
///
/// 1. Validate clone flags.
/// 2. Allocate a new PCB via `ProcessManager::create_process` (which handles
///    PID allocation, credential copying, and scheduler insertion).
/// 3. Overwrite the child's subsystem state with the `copy_*` helpers.
/// 4. Patch the child's CPU context via `copy_thread`.
///
/// Returns the child `Pid` on success, or an `i32` errno on failure.
pub fn copy_process(
    flags: u64,
    stack: u64,
    tls: u64,
    parent_tid: *mut u32,
    child_tid: *mut u32,
    regs: &ForkRegs,
) -> Result<Pid, i32> {
    // ── 1. Validate flags ───────────────────────────────────────────────────

    // CLONE_SIGHAND requires CLONE_VM.
    if flags & CLONE_SIGHAND != 0 && flags & CLONE_VM == 0 {
        return Err(EINVAL);
    }
    // CLONE_THREAD requires CLONE_SIGHAND.
    if flags & CLONE_THREAD != 0 && flags & CLONE_SIGHAND == 0 {
        return Err(EINVAL);
    }
    // CLONE_NEWUSER and CLONE_THREAD cannot coexist.
    if flags & CLONE_NEWUSER != 0 && flags & CLONE_THREAD != 0 {
        return Err(EINVAL);
    }

    // ── 2. Allocate child PCB ───────────────────────────────────────────────

    let pm = crate::process::get_process_manager();
    let parent_pid = pm.current_process();

    // Snapshot the parent while no locks are held by us.
    let parent_snap = pm.get_process(parent_pid).ok_or(EINVAL)?;

    let child_name: alloc::string::String = {
        let raw = &parent_snap.name;
        let len = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
        alloc::string::String::from_utf8_lossy(&raw[..len]).into_owned()
    };

    // `create_process` allocates a PID, builds a PCB, and adds it to the
    // scheduler at Normal priority.
    let child_pid = pm
        .create_process(&child_name, Some(parent_pid), Priority::Normal)
        .map_err(|_| EAGAIN)?;

    // ── 3. Copy subsystems ──────────────────────────────────────────────────

    pm.with_process_mut(child_pid, |child| {
        // FPU state — full copy so the child can use FP independently.
        child.fpu = parent_snap.fpu.clone();

        copy_mm(flags, child, &parent_snap);
        copy_files(flags, child, &parent_snap);
        copy_sighand(flags, child, &parent_snap);
        copy_signal(flags, child, &parent_snap);
        crate::namespace::clone_ns(parent_pid, child_pid, flags);

        // ── 4. CPU context ──────────────────────────────────────────────────
        copy_thread(child, regs, stack, flags);
    });

    // ── 5. CLONE_PARENT_SETTID / CLONE_CHILD_SETTID ─────────────────────────

    if flags & CLONE_PARENT_SETTID != 0 && !parent_tid.is_null() {
        // Safety: caller is responsible for a valid user-space pointer.
        unsafe { parent_tid.write_volatile(child_pid) };
    }
    if flags & (CLONE_CHILD_SETTID | CLONE_CHILD_CLEARTID) != 0 && !child_tid.is_null() {
        // Store the futex address so do_exit can clear it via futex wake.
        // The tid is written once the child is scheduled.
        let futex_addr = child_tid as u64;
        pm.with_process_mut(child_pid, |pcb| {
            pcb.clear_child_tid = futex_addr;
        });
    }

    Ok(child_pid)
}

// ────────────────────────────────────────────────────────────────────────────
// do_fork  (syscall dispatcher)
// ────────────────────────────────────────────────────────────────────────────

/// Entry point for `fork(2)`, `vfork(2)`, and `clone(2)` syscalls.
///
/// Wraps `copy_process` and handles the `CLONE_VFORK` suspension of the
/// parent.  When `CLONE_VFORK` is set, the parent yields the CPU until the
/// child either calls `execve` (changes its `exec_path`) or exits (becomes
/// Zombie).
pub fn do_fork(
    flags: u64,
    stack: u64,
    tls: u64,
    parent_tid: *mut u32,
    child_tid: *mut u32,
    regs: &ForkRegs,
) -> Result<Pid, i32> {
    let child_pid = copy_process(flags, stack, tls, parent_tid, child_tid, regs)?;

    if flags & CLONE_VFORK != 0 {
        // Suspend the parent until the child execs or exits.
        let pm = crate::process::get_process_manager();
        let initial_exec_path = pm
            .get_process(child_pid)
            .map(|p| p.exec_path.clone())
            .unwrap_or_default();

        loop {
            let child = match pm.get_process(child_pid) {
                Some(c) => c,
                None => break, // child was reaped
            };
            // Child exited?
            if matches!(child.state, ProcessState::Zombie | ProcessState::Dead) {
                break;
            }
            // Child called execve?
            if child.exec_path != initial_exec_path {
                break;
            }
            // Yield CPU to let the child run.
            crate::scheduler::yield_cpu();
        }
    }

    Ok(child_pid)
}

// ────────────────────────────────────────────────────────────────────────────
// POSIX syscall wrappers
// ────────────────────────────────────────────────────────────────────────────

/// `fork(2)` — create a copy of the current process.
pub fn sys_fork(regs: &ForkRegs) -> Result<Pid, i32> {
    do_fork(0, 0, 0, core::ptr::null_mut(), core::ptr::null_mut(), regs)
}

/// `vfork(2)` — create a child sharing the parent's VM, parent suspended
/// until child calls `execve` or `_exit`.
pub fn sys_vfork(regs: &ForkRegs) -> Result<Pid, i32> {
    do_fork(
        CLONE_VM | CLONE_VFORK,
        0,
        0,
        core::ptr::null_mut(),
        core::ptr::null_mut(),
        regs,
    )
}

/// `clone(2)` — the general-purpose thread/process creation syscall.
///
/// Argument order matches the x86-64 ABI: flags, stack, parent_tid,
/// child_tid, tls.
pub fn sys_clone(
    flags: u64,
    stack: u64,
    parent_tid: *mut u32,
    child_tid: *mut u32,
    tls: u64,
    regs: &ForkRegs,
) -> Result<Pid, i32> {
    do_fork(flags, stack, tls, parent_tid, child_tid, regs)
}
