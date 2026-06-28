//! ELF exec helpers shared by the kernel process manager and POSIX facade.

use super::{get_process_manager, Pid, ProcessState};
use crate::process::elf_loader::{ElfLoader, ElfLoaderError};
use alloc::vec::Vec;

/// Replace the address space of `pid` with a freshly loaded ELF image.
pub fn exec_elf_binary(
    pid: Pid,
    program: &[u8],
    args: &[&str],
    envp: &[&str],
) -> Result<(), &'static str> {
    let process_mgr = get_process_manager();
    let mut pcb = process_mgr.get_process(pid).ok_or("Process not found")?;

    if matches!(pcb.state, ProcessState::Zombie) {
        return Err("Cannot exec zombie process");
    }

    let elf_loader = ElfLoader::new(true, true);
    let loaded_binary = elf_loader
        .load_elf_binary(program, pid)
        .map_err(map_elf_loader_error)?;

    let rsp = build_argv_stack(loaded_binary.stack_top.as_u64(), args, envp)?;

    pcb.memory.code_start = loaded_binary.base_address.as_u64();
    pcb.memory.code_size = loaded_binary
        .code_regions
        .iter()
        .map(|r| r.size as u64)
        .sum();
    pcb.memory.data_start = loaded_binary
        .data_regions
        .first()
        .map(|r| r.start.as_u64())
        .unwrap_or(0);
    pcb.memory.data_size = loaded_binary
        .data_regions
        .iter()
        .map(|r| r.size as u64)
        .sum();
    pcb.memory.heap_start = loaded_binary.heap_start.as_u64();
    pcb.memory.heap_size = 8 * 1024;
    pcb.memory.stack_start = loaded_binary
        .stack_top
        .as_u64()
        .saturating_sub(8 * 1024 * 1024);
    pcb.memory.stack_size = 8 * 1024 * 1024;

    pcb.entry_point = loaded_binary.entry_point.as_u64();
    pcb.context.rip = loaded_binary.entry_point.as_u64();
    crate::usermode::configure_user_cpu_context(
        &mut pcb.context,
        loaded_binary.entry_point.as_u64(),
        rsp,
    );

    pcb.fd_table.retain(|&fd, _| fd <= 2);
    pcb.file_descriptors.retain(|&fd, _| fd <= 2);
    pcb.state = ProcessState::Ready;
    pcb.cpu_time = 0;

    process_mgr.adopt_spawned_process(pcb)
}

fn map_elf_loader_error(err: ElfLoaderError) -> &'static str {
    match err {
        ElfLoaderError::MemoryAllocationFailed | ElfLoaderError::MappingFailed => "Out of memory",
        _ => "Invalid program format",
    }
}

/// Build a minimal argc/argv/envp stack for exec().
fn build_argv_stack(stack_top: u64, args: &[&str], envp: &[&str]) -> Result<u64, &'static str> {
    let mut sp = stack_top & !0xF;

    let mut env_addrs = Vec::with_capacity(envp.len());
    for env in envp.iter().rev() {
        let bytes = env.as_bytes();
        sp = sp.wrapping_sub((bytes.len() + 1) as u64);
        let addr = sp;
        unsafe {
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), addr as *mut u8, bytes.len());
            (addr as *mut u8).add(bytes.len()).write(0);
        }
        env_addrs.push(addr);
    }
    env_addrs.reverse();

    let mut arg_addrs = Vec::with_capacity(args.len());
    for arg in args.iter().rev() {
        let bytes = arg.as_bytes();
        sp = sp.wrapping_sub((bytes.len() + 1) as u64);
        let addr = sp;
        unsafe {
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), addr as *mut u8, bytes.len());
            (addr as *mut u8).add(bytes.len()).write(0);
        }
        arg_addrs.push(addr);
    }
    arg_addrs.reverse();

    sp &= !0xF;
    let pointer_slots = args.len() + envp.len() + 3;
    if pointer_slots % 2 == 1 {
        sp = sp.wrapping_sub(8);
    }

    sp = sp.wrapping_sub(8);
    unsafe {
        (sp as *mut u64).write(0);
    }

    for &addr in env_addrs.iter().rev() {
        sp = sp.wrapping_sub(8);
        unsafe {
            (sp as *mut u64).write(addr);
        }
    }

    sp = sp.wrapping_sub(8);
    unsafe {
        (sp as *mut u64).write(0);
    }

    for &addr in arg_addrs.iter().rev() {
        sp = sp.wrapping_sub(8);
        unsafe {
            (sp as *mut u64).write(addr);
        }
    }
    sp = sp.wrapping_sub(8);
    unsafe {
        (sp as *mut u64).write(args.len() as u64);
    }

    if sp % 16 != 0 {
        return Err("Stack alignment failed");
    }

    Ok(sp)
}
