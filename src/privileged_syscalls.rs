//! Privileged low-level Linux syscalls.

extern crate alloc;

use alloc::collections::BTreeMap;
use core::arch::asm;
use spin::RwLock;

use crate::linux_compat::{LinuxError, LinuxResult};
use crate::process;

const IO_BITMAP_PORTS: u64 = 65_536;
const MAX_IOPL: u32 = 3;
const IOPL_SHIFT: u64 = 12;
const IOPL_MASK: u64 = 0b11 << IOPL_SHIFT;

#[derive(Clone)]
struct IoPrivilegeState {
    bitmap: [u8; crate::gdt::IO_BITMAP_BYTES],
    iopl: u8,
}

impl IoPrivilegeState {
    fn denied() -> Self {
        Self {
            bitmap: [0xff; crate::gdt::IO_BITMAP_BYTES],
            iopl: 0,
        }
    }

    fn hardware_bitmap(&self) -> [u8; crate::gdt::IO_BITMAP_BYTES] {
        if self.iopl == 3 {
            [0x00; crate::gdt::IO_BITMAP_BYTES]
        } else {
            self.bitmap
        }
    }
}

static IO_PRIVILEGES: RwLock<BTreeMap<u32, IoPrivilegeState>> = RwLock::new(BTreeMap::new());

fn current_euid() -> Option<u32> {
    let pid = process::current_pid();
    process::get_process_manager()
        .get_process(pid)
        .map(|pcb| pcb.euid)
}

fn current_is_privileged() -> bool {
    current_euid().map(|euid| euid == 0).unwrap_or(true)
}

fn state_for_pid(pid: u32) -> IoPrivilegeState {
    IO_PRIVILEGES
        .read()
        .get(&pid)
        .cloned()
        .unwrap_or_else(IoPrivilegeState::denied)
}

fn update_port_range(
    bitmap: &mut [u8; crate::gdt::IO_BITMAP_BYTES],
    from: u64,
    num: u64,
    allow: bool,
) {
    for port in from..from + num {
        let byte = (port / 8) as usize;
        let bit = (port & 7) as u8;
        let mask = 1u8 << bit;
        if allow {
            bitmap[byte] &= !mask;
        } else {
            bitmap[byte] |= mask;
        }
    }
}

unsafe fn set_current_rflags_iopl(level: u8) {
    let mut flags: u64;
    asm!("pushfq; pop {}", out(reg) flags, options(nomem));
    flags = (flags & !IOPL_MASK) | (((level as u64) & 0b11) << IOPL_SHIFT);
    asm!("push {}; popfq", in(reg) flags, options(nomem));
}

fn set_saved_rflags_iopl(pid: u32, level: u8) {
    let _ = process::get_process_manager().with_process_mut(pid, |pcb| {
        pcb.context.rflags =
            (pcb.context.rflags & !IOPL_MASK) | (((level as u64) & 0b11) << IOPL_SHIFT);
    });
}

pub fn apply_io_privileges_for_process(pid: u32) {
    let state = state_for_pid(pid);
    let bitmap = state.hardware_bitmap();
    crate::gdt::set_io_permission_bitmap(&bitmap);
    unsafe {
        set_current_rflags_iopl(state.iopl);
    }
}

pub fn ioperm(from: u64, num: u64, turn_on: i32) -> LinuxResult<i32> {
    let end = from.checked_add(num).ok_or(LinuxError::EINVAL)?;
    if end > IO_BITMAP_PORTS {
        return Err(LinuxError::EINVAL);
    }
    if num == 0 {
        return Ok(0);
    }
    if turn_on != 0 && !current_is_privileged() {
        return Err(LinuxError::EPERM);
    }

    let pid = process::current_pid();
    {
        let mut states = IO_PRIVILEGES.write();
        let state = states.entry(pid).or_insert_with(IoPrivilegeState::denied);
        update_port_range(&mut state.bitmap, from, num, turn_on != 0);
    }
    apply_io_privileges_for_process(pid);
    Ok(0)
}

pub fn iopl(level: u32) -> LinuxResult<i32> {
    if level > MAX_IOPL {
        return Err(LinuxError::EINVAL);
    }
    if level > 0 && !current_is_privileged() {
        return Err(LinuxError::EPERM);
    }

    let pid = process::current_pid();
    {
        let mut states = IO_PRIVILEGES.write();
        let state = states.entry(pid).or_insert_with(IoPrivilegeState::denied);
        state.iopl = level as u8;
    }
    set_saved_rflags_iopl(pid, level as u8);
    apply_io_privileges_for_process(pid);
    Ok(0)
}

pub fn vhangup() -> LinuxResult<i32> {
    if !current_is_privileged() {
        return Err(LinuxError::EPERM);
    }
    Err(LinuxError::ENOTTY)
}

pub fn clear_for_pid(pid: u32) {
    IO_PRIVILEGES.write().remove(&pid);
}

pub fn init() {
    IO_PRIVILEGES.write().clear();
    crate::gdt::deny_all_io_ports();
    unsafe {
        set_current_rflags_iopl(0);
    }
}
