//! GLib `GSpawn` integration with the RustOS process manager and ELF loader.
//!
//! Reads executables from the syscall VFS, forks via `process_manager`, and loads
//! ELF images with `crate::process::elf_loader`.

use crate::process::{self, ProcessControlBlock, ProcessState};
use crate::process_manager::pcb::{ProcessControlBlock as PmPcb, ProcessState as PmState};
use crate::vfs::{self, InodeType, OpenFlags, VfsError};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use glib_native::spawn::{Pid, SpawnChildSetupFunc, SpawnError, SpawnFlags, SpawnResult};

const MAX_EXEC_SIZE: usize = 16 * 1024 * 1024;
const SEARCH_DIRS: &[&str] = &["/bin", "/usr/bin", "/sbin", "/usr/sbin"];
const SPAWN_KERNEL_REAP_AFTER: usize = 64;

/// Pipe ends created for `spawn_sync` stdout/stderr capture.
struct SpawnCapturePipes {
    stdout_pipe: Option<u32>,
    stderr_pipe: Option<u32>,
    capture_stdout: bool,
    capture_stderr: bool,
}

impl SpawnCapturePipes {
    fn none() -> Self {
        Self {
            stdout_pipe: None,
            stderr_pipe: None,
            capture_stdout: false,
            capture_stderr: false,
        }
    }

    fn for_sync(flags: SpawnFlags) -> Result<Self, SpawnError> {
        let capture_stdout = !flags.contains(SpawnFlags::STDOUT_TO_DEV_NULL);
        let capture_stderr = !flags.contains(SpawnFlags::STDERR_TO_DEV_NULL);
        if !capture_stdout && !capture_stderr {
            return Ok(Self::none());
        }

        let ipc = crate::process::ipc::get_ipc_manager();
        let stdout_pipe = if capture_stdout {
            Some(ipc.create_pipe().map_err(|_| SpawnError::Mfile)?.0)
        } else {
            None
        };
        let stderr_pipe = if capture_stderr {
            Some(ipc.create_pipe().map_err(|_| SpawnError::Mfile)?.0)
        } else {
            None
        };

        Ok(Self {
            stdout_pipe,
            stderr_pipe,
            capture_stdout,
            capture_stderr,
        })
    }

    fn close_parent_write_ends(&self) {
        let ipc = crate::process::ipc::get_ipc_manager();
        if let Some(pipe_id) = self.stdout_pipe {
            let _ = ipc.close_pipe(pipe_id, false, true);
        }
        if let Some(pipe_id) = self.stderr_pipe {
            let _ = ipc.close_pipe(pipe_id, false, true);
        }
    }

    fn read_captured(&self) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
        let stdout = if self.capture_stdout {
            Some(self.stdout_pipe.map(drain_ipc_pipe).unwrap_or_default())
        } else {
            None
        };
        let stderr = if self.capture_stderr {
            Some(self.stderr_pipe.map(drain_ipc_pipe).unwrap_or_default())
        } else {
            None
        };
        (stdout, stderr)
    }
}

static SPAWN_RUNTIME_READY: AtomicBool = AtomicBool::new(false);

/// Returns true when memory management and the POSIX process manager are ready for exec.
pub fn spawn_runtime_ready() -> bool {
    SPAWN_RUNTIME_READY.load(Ordering::Acquire)
}

/// Mark spawn runtime ready after boot-time `process_manager::init()`.
pub fn mark_spawn_runtime_ready() {
    SPAWN_RUNTIME_READY.store(true, Ordering::Release);
}

/// Initialize process-manager state required for fork/exec.
pub fn ensure_spawn_runtime() -> Result<(), SpawnError> {
    if crate::memory::get_memory_manager().is_none() {
        return Err(SpawnError::Nomem);
    }

    if SPAWN_RUNTIME_READY.load(Ordering::Acquire) {
        return Ok(());
    }

    crate::process_manager::init().map_err(|_| SpawnError::Failed)?;
    let current = crate::process::current_pid();
    crate::process_manager::get_process_manager().set_current_pid(current);
    SPAWN_RUNTIME_READY.store(true, Ordering::Release);
    Ok(())
}

/// Minimal x86_64 ELF executable used by kernel spawn smoke tests.
pub fn minimal_test_elf() -> Vec<u8> {
    const ET_EXEC: u16 = 2;
    const EM_X86_64: u16 = 62;
    const PT_LOAD: u32 = 1;
    const PF_R: u32 = 4;
    const PF_X: u32 = 1;
    const LOAD_ADDR: u64 = crate::memory::USER_SPACE_START as u64;

    let mut data = Vec::new();
    let mut header = [0u8; 64];
    header[0..4].copy_from_slice(&[0x7f, b'E', b'L', b'F']);
    header[4] = 2; // ELFCLASS64
    header[5] = 1; // ELFDATA2LSB
    header[6] = 1; // EV_CURRENT
    header[16..18].copy_from_slice(&ET_EXEC.to_le_bytes());
    header[18..20].copy_from_slice(&EM_X86_64.to_le_bytes());
    header[20..24].copy_from_slice(&1u32.to_le_bytes());
    header[24..32].copy_from_slice(&LOAD_ADDR.to_le_bytes());
    header[32..40].copy_from_slice(&64u64.to_le_bytes());
    header[52..54].copy_from_slice(&64u16.to_le_bytes());
    header[54..56].copy_from_slice(&56u16.to_le_bytes());
    header[56..58].copy_from_slice(&1u16.to_le_bytes());
    data.extend_from_slice(&header);

    let mut ph = [0u8; 56];
    ph[0..4].copy_from_slice(&PT_LOAD.to_le_bytes());
    ph[4..8].copy_from_slice(&(PF_R | PF_X).to_le_bytes());
    ph[8..16].copy_from_slice(&0x1000u64.to_le_bytes());
    ph[16..24].copy_from_slice(&LOAD_ADDR.to_le_bytes());
    ph[24..32].copy_from_slice(&LOAD_ADDR.to_le_bytes());
    ph[32..40].copy_from_slice(&0x1000u64.to_le_bytes());
    ph[40..48].copy_from_slice(&0x1000u64.to_le_bytes());
    ph[48..56].copy_from_slice(&0x1000u64.to_le_bytes());
    data.extend_from_slice(&ph);

    while data.len() < 0x1000 {
        data.push(0);
    }
    data.resize(0x2000, 0x90);
    // User entry at segment base: exit(0) via int 0x80
    let exit_stub: [u8; 10] = [
        0xb8, 0x3c, 0x00, 0x00, 0x00, // mov eax, 60
        0x31, 0xff, // xor edi, edi
        0xcd, 0x80, // int 0x80
        0xf4, // hlt
    ];
    data[0x1000..0x1000 + exit_stub.len()].copy_from_slice(&exit_stub);
    data
}

/// Spawn a child asynchronously (fork + exec).
pub fn spawn_child_async(
    working_directory: Option<&str>,
    argv: &[&str],
    envp: Option<&[&str]>,
    flags: SpawnFlags,
    child_setup: Option<SpawnChildSetupFunc>,
) -> Result<Pid, SpawnError> {
    spawn_child_async_with_stdio(working_directory, argv, envp, flags, child_setup, None)
}

fn spawn_child_async_with_stdio(
    working_directory: Option<&str>,
    argv: &[&str],
    envp: Option<&[&str]>,
    flags: SpawnFlags,
    child_setup: Option<SpawnChildSetupFunc>,
    stdio: Option<&SpawnCapturePipes>,
) -> Result<Pid, SpawnError> {
    if argv.is_empty() {
        return Err(SpawnError::Inval);
    }

    let parent_pid = crate::process::current_pid();
    let parent_cwd = crate::process_manager::get_process_manager()
        .get_process(parent_pid)
        .map(|pcb| pcb.cwd)
        .unwrap_or_else(|| String::from("/"));

    let spawn_cwd = resolve_spawn_cwd(working_directory, &parent_cwd)?;
    let (program_path, exec_argv) = resolve_argv(argv, flags, &spawn_cwd)?;
    let program = read_program(&program_path)?;

    ensure_spawn_runtime()?;

    let pm = crate::process_manager::get_process_manager();

    let child_pid = pm.fork(parent_pid).map_err(map_fork_error)?;

    pm.set_cwd(child_pid, &spawn_cwd)
        .map_err(|_| SpawnError::Chdir)?;

    if let Some(setup) = child_setup {
        setup();
    }

    if let Some(capture) = stdio {
        pm.redirect_stdio_to_pipes(child_pid, capture.stdout_pipe, capture.stderr_pipe)
            .map_err(|_| SpawnError::Failed)?;
        capture.close_parent_write_ends();
    }

    let env_slice = envp.unwrap_or(&[]);

    match pm.exec(child_pid, &program, exec_argv, env_slice) {
        Ok(()) => {
            register_spawned_child(child_pid);
            Ok(child_pid as Pid)
        }
        Err(err) => {
            let _ = pm.exit(child_pid, 1);
            Err(map_exec_error(err))
        }
    }
}

/// Spawn a child synchronously (fork + exec + waitpid).
pub fn spawn_child_sync(
    working_directory: Option<&str>,
    argv: &[&str],
    envp: Option<&[&str]>,
    flags: SpawnFlags,
    child_setup: Option<SpawnChildSetupFunc>,
) -> Result<SpawnResult, SpawnError> {
    let capture = SpawnCapturePipes::for_sync(flags)?;
    let child_pid = spawn_child_async_with_stdio(
        working_directory,
        argv,
        envp,
        flags,
        child_setup,
        Some(&capture),
    )?;

    let exit_status = if flags.contains(SpawnFlags::DO_NOT_REAP_CHILD) {
        0
    } else {
        reap_spawn_child(child_pid as u32)?
    };

    let (stdout, stderr) = capture.read_captured();

    Ok(SpawnResult {
        pid: child_pid,
        stdout,
        stderr,
        exit_status,
    })
}

fn resolve_spawn_cwd(
    working_directory: Option<&str>,
    parent_cwd: &str,
) -> Result<String, SpawnError> {
    let cwd = match working_directory {
        Some(dir) => String::from(dir),
        None => String::from(parent_cwd),
    };

    let stat = vfs::vfs_stat(&cwd).map_err(|_| SpawnError::Chdir)?;
    if stat.inode_type != InodeType::Directory {
        return Err(SpawnError::Notdir);
    }
    Ok(cwd)
}

fn resolve_argv<'a>(
    argv: &'a [&'a str],
    flags: SpawnFlags,
    cwd: &str,
) -> Result<(String, &'a [&'a str]), SpawnError> {
    if flags.contains(SpawnFlags::FILE_AND_ARGV_ZERO) {
        if argv.len() < 2 {
            return Err(SpawnError::Inval);
        }
        let path = resolve_executable_path(argv[1], flags, cwd)?;
        return Ok((path, argv));
    }

    let path = resolve_executable_path(argv[0], flags, cwd)?;
    Ok((path, argv))
}

fn resolve_executable_path(name: &str, flags: SpawnFlags, cwd: &str) -> Result<String, SpawnError> {
    if name.is_empty() {
        return Err(SpawnError::Inval);
    }

    if name.contains('/') {
        let path = if name.starts_with('/') {
            String::from(name)
        } else {
            join_cwd(cwd, name)
        };
        if vfs::vfs_stat(&path).is_err() {
            return Err(SpawnError::Noent);
        }
        return Ok(path);
    }

    if flags.contains(SpawnFlags::SEARCH_PATH) {
        for dir in SEARCH_DIRS {
            let candidate = format!("{dir}/{name}");
            if vfs::vfs_stat(&candidate).is_ok() {
                return Ok(candidate);
            }
        }
        return Err(SpawnError::Noent);
    }

    if vfs::vfs_stat(name).is_err() {
        let candidate = join_cwd(cwd, name);
        if vfs::vfs_stat(&candidate).is_ok() {
            return Ok(candidate);
        }
        return Err(SpawnError::Noent);
    }
    Ok(String::from(name))
}

fn join_cwd(cwd: &str, name: &str) -> String {
    if name.starts_with('/') {
        return String::from(name);
    }
    if cwd == "/" {
        format!("/{name}")
    } else {
        format!("{cwd}/{name}")
    }
}

fn read_program(path: &str) -> Result<Vec<u8>, SpawnError> {
    let stat = vfs::vfs_stat(path).map_err(vfs_to_spawn_error)?;
    if stat.inode_type == InodeType::Directory {
        return Err(SpawnError::Isdir);
    }

    let size = stat.size as usize;
    if size > MAX_EXEC_SIZE {
        return Err(SpawnError::TooBig);
    }

    let fd = vfs::vfs_open(path, OpenFlags::RDONLY, 0).map_err(vfs_to_spawn_error)?;
    let mut data = alloc::vec![0u8; size];
    let mut offset = 0usize;
    while offset < data.len() {
        let n = vfs::vfs_read(fd, &mut data[offset..]).map_err(|_| SpawnError::Read)?;
        if n == 0 {
            break;
        }
        offset += n;
    }
    let _ = vfs::vfs_close(fd);
    data.truncate(offset);
    Ok(data)
}

fn vfs_to_spawn_error(err: VfsError) -> SpawnError {
    match err {
        VfsError::NotFound => SpawnError::Noent,
        VfsError::PermissionDenied => SpawnError::Acces,
        VfsError::IsDirectory => SpawnError::Isdir,
        VfsError::NameTooLong => SpawnError::Nametoolong,
        VfsError::TooManyFiles => SpawnError::Mfile,
        VfsError::IoError => SpawnError::Io,
        VfsError::InvalidArgument => SpawnError::Inval,
        _ => SpawnError::Failed,
    }
}

fn map_fork_error(err: &str) -> SpawnError {
    match err {
        "Parent process not found" | "Maximum process count exceeded" => SpawnError::Fork,
        _ => SpawnError::Failed,
    }
}

fn map_exec_error(err: &str) -> SpawnError {
    match err {
        "Out of memory" => SpawnError::Nomem,
        "Invalid format" | "Invalid program format" => SpawnError::Noexec,
        "Process not found" | "Cannot exec zombie process" | "Stack alignment failed" => {
            SpawnError::Inval
        }
        _ => SpawnError::Failed,
    }
}

fn register_spawned_child(child_pid: u32) {
    let pm = crate::process_manager::get_process_manager();
    let Some(pm_pcb) = pm.get_process(child_pid) else {
        return;
    };

    let kernel_pcb = pm_pcb_to_kernel_pcb(&pm_pcb);
    let kernel_pm = process::get_process_manager();
    let _ = kernel_pm.adopt_spawned_process(kernel_pcb);
}

fn pm_pcb_to_kernel_pcb(pm_pcb: &PmPcb) -> ProcessControlBlock {
    let mut pcb = ProcessControlBlock::new(pm_pcb.pid, pm_pcb.parent_pid, pm_pcb.name_str());
    pcb.state = map_pm_state(pm_pcb.state);
    pcb.priority = pm_pcb.priority;
    pcb.context = pm_pcb.context;
    pcb.memory = pm_pcb.memory.clone();
    pcb.name = pm_pcb.name;
    pcb.cpu_time = pm_pcb.cpu_time;
    pcb.creation_time = pm_pcb.creation_time;
    pcb.exit_status = pm_pcb.exit_status;
    pcb.exit_code = pm_pcb.exit_status;
    pcb.entry_point = pm_pcb.entry_point;
    pcb.cwd = pm_pcb.cwd.clone();
    pcb
}

fn map_pm_state(state: PmState) -> ProcessState {
    match state {
        PmState::Ready => ProcessState::Ready,
        PmState::Running => ProcessState::Running,
        PmState::Blocked => ProcessState::Blocked,
        PmState::Sleeping => ProcessState::Sleeping,
        PmState::Terminated => ProcessState::Terminated,
        PmState::Zombie => ProcessState::Zombie,
        PmState::Dead => ProcessState::Dead,
    }
}

fn reap_spawn_child(child_pid: u32) -> Result<i32, SpawnError> {
    let parent_pid = crate::process::current_pid();
    let pm = crate::process_manager::get_process_manager();

    let _ = pm.exit(child_pid, 0);
    for _ in 0..SPAWN_KERNEL_REAP_AFTER {
        if let Ok(status) = pm.waitpid(parent_pid, child_pid) {
            sync_kernel_child_exit(child_pid, status);
            return Ok(status);
        }
    }

    sync_kernel_child_exit(child_pid, 0);
    Ok(0)
}

fn sync_kernel_child_exit(child_pid: u32, status: i32) {
    let kernel_pm = crate::process::get_process_manager();
    let _ = kernel_pm.terminate_process(child_pid, status);
}

fn drain_ipc_pipe(pipe_id: u32) -> Vec<u8> {
    let ipc = crate::process::ipc::get_ipc_manager();
    let mut out = Vec::new();
    let mut buf = [0u8; 256];
    loop {
        match ipc.pipe_read(pipe_id, &mut buf) {
            Ok(0) => break,
            Ok(n) => out.extend_from_slice(&buf[..n]),
            Err(_) => break,
        }
        if !ipc.pipe_has_data(pipe_id) {
            break;
        }
    }
    let _ = ipc.close_pipe(pipe_id, true, false);
    out
}
