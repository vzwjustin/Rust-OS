//! BPF — eBPF program and map management
//!
//! Ported from Linux kernel/bpf/ (syscall.c, map.c, verifier.c).
//! Provides basic BPF map creation/lookup/update/delete and program load.
//! The eBPF verifier and JIT are stubbed — programs are stored but not executed.

use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::{Mutex, RwLock};

// ── BPF commands ────────────────────────────────────────────────────────

pub const BPF_MAP_CREATE: u32 = 0;
pub const BPF_MAP_LOOKUP_ELEM: u32 = 1;
pub const BPF_MAP_UPDATE_ELEM: u32 = 2;
pub const BPF_MAP_DELETE_ELEM: u32 = 3;
pub const BPF_MAP_GET_NEXT_KEY: u32 = 4;
pub const BPF_PROG_LOAD: u32 = 5;
pub const BPF_OBJ_PIN: u32 = 6;
pub const BPF_OBJ_GET: u32 = 7;
pub const BPF_PROG_ATTACH: u32 = 8;
pub const BPF_PROG_DETACH: u32 = 9;
pub const BPF_PROG_TEST_RUN: u32 = 10;
pub const BPF_PROG_GET_NEXT_ID: u32 = 11;
pub const BPF_MAP_GET_NEXT_ID: u32 = 12;
pub const BPF_PROG_GET_FD_BY_ID: u32 = 13;
pub const BPF_MAP_GET_FD_BY_ID: u32 = 14;
pub const BPF_OBJ_GET_INFO_BY_FD: u32 = 15;
pub const BPF_PROG_QUERY: u32 = 16;
pub const BPF_RAW_TRACEPOINT_OPEN: u32 = 17;
pub const BPF_BTF_LOAD: u32 = 18;
pub const BPF_BTF_GET_FD_BY_ID: u32 = 19;
pub const BPF_TASK_FD_QUERY: u32 = 20;
pub const BPF_MAP_LOOKUP_AND_DELETE_ELEM: u32 = 21;
pub const BPF_MAP_FREEZE: u32 = 22;
pub const BPF_BTF_GET_NEXT_ID: u32 = 23;
pub const BPF_MAP_LOOKUP_BATCH: u32 = 24;
pub const BPF_MAP_LOOKUP_AND_DELETE_BATCH: u32 = 25;
pub const BPF_MAP_UPDATE_BATCH: u32 = 26;
pub const BPF_MAP_DELETE_BATCH: u32 = 27;
pub const BPF_LINK_CREATE: u32 = 28;
pub const BPF_LINK_UPDATE: u32 = 29;
pub const BPF_LINK_GET_FD_BY_ID: u32 = 30;
pub const BPF_LINK_GET_NEXT_ID: u32 = 31;
pub const BPF_ENABLE_STATS: u32 = 32;
pub const BPF_ITER_CREATE: u32 = 33;
pub const BPF_LINK_DETACH: u32 = 34;
pub const BPF_PROG_BIND_MAP: u32 = 35;
pub const BPF_TOKEN_CREATE: u32 = 36;

// ── Map types ───────────────────────────────────────────────────────────

pub const BPF_MAP_TYPE_UNSPEC: u32 = 0;
pub const BPF_MAP_TYPE_HASH: u32 = 1;
pub const BPF_MAP_TYPE_ARRAY: u32 = 2;
pub const BPF_MAP_TYPE_PROG_ARRAY: u32 = 3;
pub const BPF_MAP_TYPE_PERF_EVENT_ARRAY: u32 = 4;
pub const BPF_MAP_TYPE_PERCPU_HASH: u32 = 5;
pub const BPF_MAP_TYPE_PERCPU_ARRAY: u32 = 6;
pub const BPF_MAP_TYPE_STACK_TRACE: u32 = 7;
pub const BPF_MAP_TYPE_CGROUP_ARRAY: u32 = 8;
pub const BPF_MAP_TYPE_LRU_HASH: u32 = 9;
pub const BPF_MAP_TYPE_LRU_PERCPU_HASH: u32 = 10;
pub const BPF_MAP_TYPE_LPM_TRIE: u32 = 11;

// ── Program types ───────────────────────────────────────────────────────

pub const BPF_PROG_TYPE_UNSPEC: u32 = 0;
pub const BPF_PROG_TYPE_SOCKET_FILTER: u32 = 1;
pub const BPF_PROG_TYPE_KPROBE: u32 = 2;
pub const BPF_PROG_TYPE_SCHED_CLS: u32 = 3;
pub const BPF_PROG_TYPE_SCHED_ACT: u32 = 4;
pub const BPF_PROG_TYPE_TRACEPOINT: u32 = 5;
pub const BPF_PROG_TYPE_XDP: u32 = 6;
pub const BPF_PROG_TYPE_PERF_EVENT: u32 = 7;
pub const BPF_PROG_TYPE_CGROUP_SKB: u32 = 8;
pub const BPF_PROG_TYPE_CGROUP_SOCK: u32 = 9;
pub const BPF_PROG_TYPE_LWT_IN: u32 = 10;
pub const BPF_PROG_TYPE_LWT_OUT: u32 = 11;
pub const BPF_PROG_TYPE_LWT_XMIT: u32 = 12;

// ── BPF attr structures (simplified) ────────────────────────────────────

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct BpfMapCreateAttr {
    pub map_type: u32,
    pub key_size: u32,
    pub value_size: u32,
    pub max_entries: u32,
    pub map_flags: u32,
    pub inner_map_fd: u32,
    pub numa_node: u32,
    pub map_name: [u8; 16],
    pub map_ifindex: u32,
    pub btf_fd: u32,
    pub btf_key_type_id: u32,
    pub btf_value_type_id: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct BpfMapElemAttr {
    pub map_fd: u32,
    pub _pad: u32,
    pub key: u64,
    pub value_or_next_key: u64,
    pub flags: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct BpfProgLoadAttr {
    pub prog_type: u32,
    pub insn_cnt: u32,
    pub insns: u64,
    pub license: u64,
    pub log_level: u32,
    pub log_size: u32,
    pub log_buf: u64,
    pub kern_version: u32,
    pub prog_flags: u32,
    pub prog_name: [u8; 16],
    pub prog_ifindex: u32,
    pub expected_attach_type: u32,
    pub btf_fd: u32,
    pub btf_key_type_id: u32,
}

// ── BPF map state ───────────────────────────────────────────────────────

pub struct BpfMap {
    pub id: u32,
    pub map_type: u32,
    pub key_size: u32,
    pub value_size: u32,
    pub max_entries: u32,
    pub flags: u32,
    pub frozen: bool,
    pub entries: BTreeMap<Vec<u8>, Vec<u8>>,
}

pub struct BpfProg {
    pub id: u32,
    pub prog_type: u32,
    pub insns: Vec<u64>,
    pub license: Vec<u8>,
}

// ── Global state ────────────────────────────────────────────────────────

static MAPS: RwLock<BTreeMap<u32, Mutex<BpfMap>>> = RwLock::new(BTreeMap::new());
static PROGS: RwLock<BTreeMap<u32, Mutex<BpfProg>>> = RwLock::new(BTreeMap::new());
static NEXT_MAP_ID: AtomicU32 = AtomicU32::new(1);
static NEXT_PROG_ID: AtomicU32 = AtomicU32::new(1);

// ── Syscall implementation ──────────────────────────────────────────────

/// bpf syscall — dispatch on cmd
pub fn bpf(cmd: u32, attr: u64, size: u32) -> i32 {
    if attr == 0 && cmd != BPF_PROG_GET_NEXT_ID && cmd != BPF_MAP_GET_NEXT_ID {
        return -14; // EFAULT
    }

    match cmd {
        BPF_MAP_CREATE => bpf_map_create(attr, size),
        BPF_MAP_LOOKUP_ELEM => bpf_map_lookup_elem(attr, size),
        BPF_MAP_UPDATE_ELEM => bpf_map_update_elem(attr, size),
        BPF_MAP_DELETE_ELEM => bpf_map_delete_elem(attr, size),
        BPF_MAP_GET_NEXT_KEY => bpf_map_get_next_key(attr, size),
        BPF_PROG_LOAD => bpf_prog_load(attr, size),
        BPF_MAP_FREEZE => bpf_map_freeze(attr, size),
        BPF_OBJ_GET_INFO_BY_FD => bpf_obj_get_info_by_fd(attr, size),
        BPF_PROG_GET_NEXT_ID => {
            let start = unsafe { *(attr as *const u32) };
            let progs = PROGS.read();
            let next = progs.keys().find(|&&k| k > start).copied();
            match next {
                Some(id) => {
                    unsafe {
                        *(attr as *mut u32) = id;
                    }
                    0
                }
                None => -2, // ENOENT
            }
        }
        BPF_MAP_GET_NEXT_ID => {
            let start = unsafe { *(attr as *const u32) };
            let maps = MAPS.read();
            let next = maps.keys().find(|&&k| k > start).copied();
            match next {
                Some(id) => {
                    unsafe {
                        *(attr as *mut u32) = id;
                    }
                    0
                }
                None => -2,
            }
        }
        BPF_PROG_GET_FD_BY_ID => {
            let id = unsafe { *(attr as *const u32) };
            let progs = PROGS.read();
            if progs.contains_key(&id) {
                crate::linux_compat::special_fd::register_bpf_prog(id, crate::vfs::OpenFlags::RDWR)
            } else {
                -2 // ENOENT
            }
        }
        BPF_MAP_GET_FD_BY_ID => {
            let id = unsafe { *(attr as *const u32) };
            let maps = MAPS.read();
            if maps.contains_key(&id) {
                crate::linux_compat::special_fd::register_bpf_map(id, crate::vfs::OpenFlags::RDWR)
            } else {
                -2
            }
        }
        BPF_PROG_ATTACH
        | BPF_PROG_DETACH
        | BPF_PROG_TEST_RUN
        | BPF_PROG_QUERY
        | BPF_RAW_TRACEPOINT_OPEN
        | BPF_BTF_LOAD
        | BPF_BTF_GET_FD_BY_ID
        | BPF_TASK_FD_QUERY
        | BPF_MAP_LOOKUP_AND_DELETE_ELEM
        | BPF_BTF_GET_NEXT_ID
        | BPF_MAP_LOOKUP_BATCH
        | BPF_MAP_LOOKUP_AND_DELETE_BATCH
        | BPF_MAP_UPDATE_BATCH
        | BPF_MAP_DELETE_BATCH
        | BPF_LINK_CREATE
        | BPF_LINK_UPDATE
        | BPF_LINK_GET_FD_BY_ID
        | BPF_LINK_GET_NEXT_ID
        | BPF_ENABLE_STATS
        | BPF_ITER_CREATE
        | BPF_LINK_DETACH
        | BPF_PROG_BIND_MAP
        | BPF_TOKEN_CREATE
        | BPF_OBJ_PIN
        | BPF_OBJ_GET => {
            // These require more infrastructure — return EPERM for now
            -1 // EPERM
        }
        _ => -22, // EINVAL
    }
}

fn bpf_map_create(attr: u64, size: u32) -> i32 {
    if size < core::mem::size_of::<BpfMapCreateAttr>() as u32 {
        return -22;
    }
    let a = unsafe { *(attr as *const BpfMapCreateAttr) };

    // Validate map type
    let valid_types = [
        BPF_MAP_TYPE_HASH,
        BPF_MAP_TYPE_ARRAY,
        BPF_MAP_TYPE_PERCPU_HASH,
        BPF_MAP_TYPE_PERCPU_ARRAY,
        BPF_MAP_TYPE_LRU_HASH,
        BPF_MAP_TYPE_LRU_PERCPU_HASH,
    ];
    if !valid_types.contains(&a.map_type) {
        return -22;
    }
    if a.key_size == 0 || a.value_size == 0 || a.max_entries == 0 {
        return -22;
    }
    if a.key_size > 256 || a.value_size > 4096 || a.max_entries > 100_000 {
        return -7; // E2BIG
    }

    let id = NEXT_MAP_ID.fetch_add(1, Ordering::SeqCst);
    let map = BpfMap {
        id,
        map_type: a.map_type,
        key_size: a.key_size,
        value_size: a.value_size,
        max_entries: a.max_entries,
        flags: a.map_flags,
        frozen: false,
        entries: BTreeMap::new(),
    };
    MAPS.write().insert(id, Mutex::new(map));

    let fd = crate::linux_compat::special_fd::register_bpf_map(id, crate::vfs::OpenFlags::RDWR);
    if fd < 0 {
        MAPS.write().remove(&id);
        return -23;
    }
    crate::serial_println!(
        "[bpf] map_create: type={} key_sz={} val_sz={} fd={}",
        a.map_type,
        a.key_size,
        a.value_size,
        fd
    );
    fd
}

fn bpf_map_lookup_elem(attr: u64, size: u32) -> i32 {
    if size < core::mem::size_of::<BpfMapElemAttr>() as u32 {
        return -22;
    }
    let a = unsafe { *(attr as *const BpfMapElemAttr) };
    let map_id = match crate::linux_compat::special_fd::get_bpf_map_id(a.map_fd as i32) {
        Some(id) => id,
        None => return -9,
    };
    let maps = MAPS.read();
    let map_mutex = match maps.get(&map_id) {
        Some(m) => m,
        None => return -9,
    };
    let map = map_mutex.lock();

    let key = unsafe { core::slice::from_raw_parts(a.key as *const u8, map.key_size as usize) };
    match map.entries.get(key) {
        Some(val) => {
            let dst = unsafe {
                core::slice::from_raw_parts_mut(
                    a.value_or_next_key as *mut u8,
                    map.value_size as usize,
                )
            };
            dst.copy_from_slice(val);
            0
        }
        None => -2, // ENOENT
    }
}

fn bpf_map_update_elem(attr: u64, size: u32) -> i32 {
    if size < core::mem::size_of::<BpfMapElemAttr>() as u32 {
        return -22;
    }
    let a = unsafe { *(attr as *const BpfMapElemAttr) };
    let map_id = match crate::linux_compat::special_fd::get_bpf_map_id(a.map_fd as i32) {
        Some(id) => id,
        None => return -9,
    };
    let maps = MAPS.read();
    let map_mutex = match maps.get(&map_id) {
        Some(m) => m,
        None => return -9,
    };
    let mut map = map_mutex.lock();
    if map.frozen {
        return -1; // EPERM
    }
    if map.entries.len() as u32 >= map.max_entries {
        // Check if key already exists
        let key = unsafe { core::slice::from_raw_parts(a.key as *const u8, map.key_size as usize) };
        if !map.entries.contains_key(key) {
            return -7; // E2BIG
        }
    }
    let key = unsafe { core::slice::from_raw_parts(a.key as *const u8, map.key_size as usize) };
    let val = unsafe {
        core::slice::from_raw_parts(a.value_or_next_key as *const u8, map.value_size as usize)
    };
    map.entries.insert(key.to_vec(), val.to_vec());
    0
}

fn bpf_map_delete_elem(attr: u64, size: u32) -> i32 {
    if size < core::mem::size_of::<BpfMapElemAttr>() as u32 {
        return -22;
    }
    let a = unsafe { *(attr as *const BpfMapElemAttr) };
    let map_id = match crate::linux_compat::special_fd::get_bpf_map_id(a.map_fd as i32) {
        Some(id) => id,
        None => return -9,
    };
    let maps = MAPS.read();
    let map_mutex = match maps.get(&map_id) {
        Some(m) => m,
        None => return -9,
    };
    let mut map = map_mutex.lock();
    if map.frozen {
        return -1;
    }
    let key = unsafe { core::slice::from_raw_parts(a.key as *const u8, map.key_size as usize) };
    match map.entries.remove(key) {
        Some(_) => 0,
        None => -2,
    }
}

fn bpf_map_get_next_key(attr: u64, size: u32) -> i32 {
    if size < core::mem::size_of::<BpfMapElemAttr>() as u32 {
        return -22;
    }
    let a = unsafe { *(attr as *const BpfMapElemAttr) };
    let map_id = match crate::linux_compat::special_fd::get_bpf_map_id(a.map_fd as i32) {
        Some(id) => id,
        None => return -9,
    };
    let maps = MAPS.read();
    let map_mutex = match maps.get(&map_id) {
        Some(m) => m,
        None => return -9,
    };
    let map = map_mutex.lock();

    if a.key == 0 {
        // Return first key
        if let Some(first_key) = map.entries.keys().next() {
            let dst = unsafe {
                core::slice::from_raw_parts_mut(
                    a.value_or_next_key as *mut u8,
                    map.key_size as usize,
                )
            };
            dst.copy_from_slice(first_key);
            return 0;
        }
        return -2;
    }

    let key = unsafe { core::slice::from_raw_parts(a.key as *const u8, map.key_size as usize) };
    let mut found = false;
    for k in map.entries.keys() {
        if found {
            let dst = unsafe {
                core::slice::from_raw_parts_mut(
                    a.value_or_next_key as *mut u8,
                    map.key_size as usize,
                )
            };
            dst.copy_from_slice(k);
            return 0;
        }
        if k == key {
            found = true;
        }
    }
    -2 // ENOENT
}

fn bpf_prog_load(attr: u64, size: u32) -> i32 {
    if size < 48 {
        // Minimum size for the fields we use
        return -22;
    }
    let a = unsafe { *(attr as *const BpfProgLoadAttr) };

    // Validate program type
    let valid_types = [
        BPF_PROG_TYPE_SOCKET_FILTER,
        BPF_PROG_TYPE_KPROBE,
        BPF_PROG_TYPE_SCHED_CLS,
        BPF_PROG_TYPE_SCHED_ACT,
        BPF_PROG_TYPE_TRACEPOINT,
        BPF_PROG_TYPE_XDP,
        BPF_PROG_TYPE_PERF_EVENT,
        BPF_PROG_TYPE_CGROUP_SKB,
        BPF_PROG_TYPE_CGROUP_SOCK,
    ];
    if !valid_types.contains(&a.prog_type) && a.prog_type != BPF_PROG_TYPE_UNSPEC {
        return -22;
    }
    if a.insn_cnt == 0 || a.insn_cnt > 4096 {
        return -22;
    }
    if a.insns == 0 || a.license == 0 {
        return -14;
    }

    // Copy instructions (each BPF insn is 8 bytes: u8 code, u8 regs, s16 off, s32 imm)
    let insn_bytes = a.insn_cnt as usize * 8;
    let insn_ptr = a.insns as *const u64;
    let mut insns = Vec::with_capacity(a.insn_cnt as usize);
    for i in 0..a.insn_cnt as usize {
        insns.push(unsafe { core::ptr::read_volatile(insn_ptr.add(i)) });
    }

    // Copy license string
    let mut license = Vec::new();
    let lic_ptr = a.license as *const u8;
    let mut i = 0;
    while i < 128 {
        let b = unsafe { *lic_ptr.add(i) };
        if b == 0 {
            break;
        }
        license.push(b);
        i += 1;
    }

    // Basic validation: check that insns are within bounds
    // (Full verifier would check control flow, register usage, etc.)
    let _ = insn_bytes;

    let id = NEXT_PROG_ID.fetch_add(1, Ordering::SeqCst);
    let prog = BpfProg {
        id,
        prog_type: a.prog_type,
        insns,
        license,
    };
    PROGS.write().insert(id, Mutex::new(prog));

    let fd = crate::linux_compat::special_fd::register_bpf_prog(id, crate::vfs::OpenFlags::RDWR);
    if fd < 0 {
        PROGS.write().remove(&id);
        return -23;
    }
    crate::serial_println!(
        "[bpf] prog_load: type={} cnt={} fd={}",
        a.prog_type,
        a.insn_cnt,
        fd
    );
    fd
}

fn bpf_map_freeze(attr: u64, _size: u32) -> i32 {
    let map_fd = unsafe { *(attr as *const u32) };
    let map_id = match crate::linux_compat::special_fd::get_bpf_map_id(map_fd as i32) {
        Some(id) => id,
        None => return -9,
    };
    let maps = MAPS.read();
    if let Some(map_mutex) = maps.get(&map_id) {
        let mut map = map_mutex.lock();
        map.frozen = true;
        0
    } else {
        -9
    }
}

fn bpf_obj_get_info_by_fd(attr: u64, _size: u32) -> i32 {
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct InfoAttr {
        bpf_fd: u32,
        info_len: u32,
        info: u64,
    }
    let a = unsafe { *(attr as *const InfoAttr) };

    // Try map first, then prog
    if let Some(map_id) = crate::linux_compat::special_fd::get_bpf_map_id(a.bpf_fd as i32) {
        let maps = MAPS.read();
        if let Some(map_mutex) = maps.get(&map_id) {
            let map = map_mutex.lock();
            // Write minimal map info
            #[repr(C)]
            struct BpfMapInfo {
                map_type: u32,
                id: u32,
                key_size: u32,
                value_size: u32,
                max_entries: u32,
                map_flags: u32,
                name: [u8; 16],
                ifindex: u32,
                btf_vmlinux_value_type_id: u32,
            }
            let info = BpfMapInfo {
                map_type: map.map_type,
                id: map.id,
                key_size: map.key_size,
                value_size: map.value_size,
                max_entries: map.max_entries,
                map_flags: map.flags,
                name: [0u8; 16],
                ifindex: 0,
                btf_vmlinux_value_type_id: 0,
            };
            let info_bytes = core::mem::size_of::<BpfMapInfo>() as u32;
            let copy_len = core::cmp::min(a.info_len, info_bytes) as usize;
            unsafe {
                core::ptr::copy_nonoverlapping(
                    &info as *const BpfMapInfo as *const u8,
                    a.info as *mut u8,
                    copy_len,
                );
            }
            return 0;
        }
    }
    if let Some(prog_id) = crate::linux_compat::special_fd::get_bpf_prog_id(a.bpf_fd as i32) {
        let progs = PROGS.read();
        if let Some(prog_mutex) = progs.get(&prog_id) {
            let prog = prog_mutex.lock();
            #[repr(C)]
            struct BpfProgInfo {
                prog_type: u32,
                id: u32,
                tag: [u8; 8],
                jited_prog_len: u32,
                xlated_prog_len: u32,
                jited_prog_insns: u64,
                xlated_prog_insns: u64,
                load_time: u64,
                created_by_uid: u32,
                nr_map_ids: u32,
                map_ids: u64,
                name: [u8; 16],
            }
            let info = BpfProgInfo {
                prog_type: prog.prog_type,
                id: prog.id,
                tag: [0u8; 8],
                jited_prog_len: 0,
                xlated_prog_len: prog.insns.len() as u32 * 8,
                jited_prog_insns: 0,
                xlated_prog_insns: 0,
                load_time: 0,
                created_by_uid: 0,
                nr_map_ids: 0,
                map_ids: 0,
                name: [0u8; 16],
            };
            let info_bytes = core::mem::size_of::<BpfProgInfo>() as u32;
            let copy_len = core::cmp::min(a.info_len, info_bytes) as usize;
            unsafe {
                core::ptr::copy_nonoverlapping(
                    &info as *const BpfProgInfo as *const u8,
                    a.info as *mut u8,
                    copy_len,
                );
            }
            return 0;
        }
    }
    -9 // EBADF
}

/// Close a BPF map (called when fd is closed).
pub fn close_map(id: u32) {
    MAPS.write().remove(&id);
}

/// Close a BPF prog (called when fd is closed).
pub fn close_prog(id: u32) {
    PROGS.write().remove(&id);
}

/// Initialize the BPF subsystem.
pub fn init() {
    crate::serial_println!("[bpf] BPF subsystem initialized");
}
