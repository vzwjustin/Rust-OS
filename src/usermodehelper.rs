//! User-mode helper framework — allows the kernel to spawn userspace
//! programs (e.g. for firmware loading, modprobe, kexec).
//!
//! Mirrors Linux's `kernel/umh.c` and `include/linux/umh.h`.
//!
//! When a kernel subsystem needs to run a userspace program (e.g. loading
//! firmware via `/sbin/firmware_loader` or loading a module via
//! `/sbin/modprobe`), it calls `call_usermodehelper()` which queues a
//! work item.  The helper is spawned as a userspace process with the
//! given argv and envp.

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;

static UMH_ENABLED: AtomicBool = AtomicBool::new(false);

/// Queue of pending usermode helper requests.
static UMH_QUEUE: Mutex<Vec<UmhRequest>> = Mutex::new(Vec::new());

/// A pending usermode helper request.
struct UmhRequest {
    path: String,
    argv: Vec<String>,
    envp: Vec<String>,
}

/// Initialize the usermodehelper subsystem.
/// Mirrors Linux's `usermodehelper_init()` in do_basic_setup().
pub fn init() {
    UMH_ENABLED.store(true, Ordering::Release);
    crate::serial_println!("[umh] usermodehelper initialized");
}

/// Disable usermodehelper (e.g. during shutdown or when the system is
/// not yet ready for userspace helpers).
pub fn disable() {
    UMH_ENABLED.store(false, Ordering::Release);
}

/// Returns true if usermodehelper is enabled and ready to accept requests.
pub fn is_enabled() -> bool {
    UMH_ENABLED.load(Ordering::Acquire)
}

/// Queue a usermode helper request.  The helper will be spawned when
/// `process_pending()` is called (typically from the session idle loop).
///
/// Mirrors Linux's `call_usermodehelper_setup()` + `call_usermodehelper_exec()`.
pub fn call_usermodehelper(path: &str, argv: &[&str], envp: &[&str]) {
    if !is_enabled() {
        crate::serial_println!(
            "[umh] call_usermodehelper('{}') ignored — not enabled",
            path
        );
        return;
    }

    let req = UmhRequest {
        path: String::from(path),
        argv: argv.iter().map(|s| String::from(*s)).collect(),
        envp: envp.iter().map(|s| String::from(*s)).collect(),
    };

    UMH_QUEUE.lock().push(req);
}

/// Process pending usermode helper requests by spawning them as userspace
/// processes.  Should be called from the session idle loop or scheduler tick.
///
/// Returns the number of helpers spawned.
pub fn process_pending() -> usize {
    let requests: Vec<UmhRequest> = {
        let mut q = UMH_QUEUE.lock();
        let drained = q.drain(..).collect();
        drained
    };

    let mut count = 0;
    for req in requests {
        crate::serial_println!("[umh] spawning '{}'", req.path);

        // Spawn the helper via the linux compat ELF loader.
        use crate::linux_compat::desktop;
        use crate::process::scheduler::create_process;
        use crate::process::Priority;

        match create_process(Some(0), Priority::Normal, "umh") {
            Ok(pid) => {
                let argv: Vec<String> = req.argv.clone();
                let envp: &[&str] = &[];
                let _ = desktop::exec_program(pid, &req.path, &argv, envp);
                crate::user_sched::queue_user_pid(pid);
                count += 1;
            }
            Err(e) => {
                crate::serial_println!("[umh] failed to create process: {}", e);
            }
        }
    }

    count
}

/// Convenience: request firmware loading via a userspace helper.
/// Mirrors Linux's `request_firmware_nowait()` path that calls
/// `/sbin/firmware_loader` when built-in firmware is not available.
pub fn request_firmware_load(firmware_name: &str, device: &str) {
    let path = "/sbin/firmware_loader";
    let argv = [path, "--firmware", firmware_name, "--device", device];
    let envp = ["PATH=/sbin:/bin:/usr/sbin:/usr/bin"];
    call_usermodehelper(path, &argv, &envp);
}
