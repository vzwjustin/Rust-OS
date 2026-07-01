//! Linux IPC operation APIs
//!
//! This module implements Linux-compatible IPC operations including
//! message queues, semaphores, shared memory, and event file descriptors.

use alloc::collections::{BTreeMap, VecDeque};
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use spin::RwLock;

use super::types::*;
use super::{LinuxError, LinuxResult};
use crate::process::current_pid;
use crate::process::ipc::{get_ipc_manager, IpcId, SharedMemoryPermissions};

/// Operation counter for statistics
static IPC_OPS_COUNT: AtomicU64 = AtomicU64::new(0);
static NEXT_MQ_DESC: AtomicU32 = AtomicU32::new(10_000);

const O_CREAT: i32 = 0o100;
const O_EXCL: i32 = 0o200;
const O_NONBLOCK: i32 = 0o4000;
const DEFAULT_MQ_MAXMSG: i64 = 10;
const DEFAULT_MQ_MSGSIZE: i64 = 8192;
const MAX_MQ_MAXMSG: i64 = 1024;
const MAX_MQ_MSGSIZE: i64 = 64 * 1024;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct MqAttr {
    pub mq_flags: i64,
    pub mq_maxmsg: i64,
    pub mq_msgsize: i64,
    pub mq_curmsgs: i64,
}

#[derive(Clone)]
struct MqMessage {
    priority: u32,
    data: Vec<u8>,
    sequence: u64,
}

struct PosixMessageQueue {
    name: String,
    attr: MqAttr,
    messages: VecDeque<MqMessage>,
    unlinked: bool,
    notify_pid: Option<i32>,
}

#[derive(Clone)]
struct MqDescriptor {
    queue_id: u32,
    flags: i32,
}

static NEXT_MQ_ID: AtomicU32 = AtomicU32::new(1);
static NEXT_MQ_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static MQ_BY_NAME: RwLock<BTreeMap<String, u32>> = RwLock::new(BTreeMap::new());
static MQ_QUEUES: RwLock<BTreeMap<u32, PosixMessageQueue>> = RwLock::new(BTreeMap::new());
static MQ_DESCRIPTORS: RwLock<BTreeMap<i32, MqDescriptor>> = RwLock::new(BTreeMap::new());

/// Initialize IPC operations subsystem
pub fn init_ipc_operations() {
    IPC_OPS_COUNT.store(0, Ordering::Relaxed);
}

/// Get number of IPC operations performed
pub fn get_operation_count() -> u64 {
    IPC_OPS_COUNT.load(Ordering::Relaxed)
}

fn read_c_string(ptr: *const u8, max_len: usize) -> LinuxResult<String> {
    if ptr.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let mut bytes = Vec::new();
    for offset in 0..max_len {
        let byte = unsafe { *ptr.add(offset) };
        if byte == 0 {
            return String::from_utf8(bytes).map_err(|_| LinuxError::EINVAL);
        }
        bytes.push(byte);
    }
    Err(LinuxError::EINVAL)
}

fn normalize_mq_name(name: *const u8) -> LinuxResult<String> {
    let raw = read_c_string(name, 255)?;
    if raw.is_empty() || !raw.starts_with('/') || raw.len() == 1 || raw[1..].contains('/') {
        return Err(LinuxError::EINVAL);
    }
    Ok(raw)
}

fn mq_attr_from_user(attr: *const MqAttr, oflag: i32) -> LinuxResult<MqAttr> {
    let mut out = if attr.is_null() {
        MqAttr {
            mq_flags: (oflag & O_NONBLOCK) as i64,
            mq_maxmsg: DEFAULT_MQ_MAXMSG,
            mq_msgsize: DEFAULT_MQ_MSGSIZE,
            mq_curmsgs: 0,
        }
    } else {
        unsafe { *attr }
    };

    if out.mq_maxmsg <= 0
        || out.mq_maxmsg > MAX_MQ_MAXMSG
        || out.mq_msgsize <= 0
        || out.mq_msgsize > MAX_MQ_MSGSIZE
    {
        return Err(LinuxError::EINVAL);
    }
    out.mq_flags = (oflag & O_NONBLOCK) as i64;
    out.mq_curmsgs = 0;
    Ok(out)
}

fn mq_descriptor(mqd: i32) -> LinuxResult<MqDescriptor> {
    MQ_DESCRIPTORS
        .read()
        .get(&mqd)
        .cloned()
        .ok_or(LinuxError::EBADF)
}

fn copy_mq_attr_to_user(attr: *mut MqAttr, value: MqAttr) -> LinuxResult<()> {
    if attr.is_null() {
        return Ok(());
    }
    unsafe {
        *attr = value;
    }
    Ok(())
}

/// Increment operation counter
fn inc_ops() {
    IPC_OPS_COUNT.fetch_add(1, Ordering::Relaxed);
}

/// IPC key to ID mapping for System V IPC
static IPC_KEY_TABLE: RwLock<BTreeMap<Key, (IpcResourceType, IpcId)>> =
    RwLock::new(BTreeMap::new());
static NEXT_IPC_KEY: AtomicU32 = AtomicU32::new(1000);

/// IPC resource types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IpcResourceType {
    MessageQueue,
    Semaphore,
    SharedMemory,
}

/// A pending semaphore operation that a blocked process is waiting on.
#[derive(Debug, Clone, Copy)]
struct SemWaitEntry {
    pid: u32,
    sem_num: u16,
    /// Target value: for P ops (sem_op < 0), we need semval + sem_op >= 0.
    /// For zero-wait (sem_op == 0), we need semval == 0.
    sem_op: i16,
}

/// Semaphore structure for System V semaphores
#[derive(Debug)]
struct SemaphoreSet {
    id: IpcId,
    semaphores: Vec<i32>,
    owner_pid: u32,
    /// PIDs waiting for semaphore values to change
    waiters: Vec<SemWaitEntry>,
}

/// Global semaphore table
static SEMAPHORE_TABLE: RwLock<BTreeMap<IpcId, SemaphoreSet>> = RwLock::new(BTreeMap::new());

/// Event file descriptor data
#[derive(Debug)]
struct EventFd {
    value: AtomicU64,
    flags: i32,
}

/// Global event file descriptor table
static EVENTFD_TABLE: RwLock<BTreeMap<Fd, EventFd>> = RwLock::new(BTreeMap::new());
static NEXT_EVENTFD: AtomicU32 = AtomicU32::new(200);

/// Timer file descriptor data
#[derive(Debug)]
struct TimerFd {
    clockid: i32,
    interval_sec: u64,
    interval_nsec: u64,
    value_sec: u64,
    value_nsec: u64,
    flags: i32,
}

/// Global timer file descriptor table
static TIMERFD_TABLE: RwLock<BTreeMap<Fd, TimerFd>> = RwLock::new(BTreeMap::new());
static NEXT_TIMERFD: AtomicU32 = AtomicU32::new(300);

/// Signal file descriptor data
#[derive(Debug)]
struct SignalFd {
    mask: u64,
    flags: i32,
}

/// Global signal file descriptor table
static SIGNALFD_TABLE: RwLock<BTreeMap<Fd, SignalFd>> = RwLock::new(BTreeMap::new());
static NEXT_SIGNALFD: AtomicU32 = AtomicU32::new(400);

/// Convert IPC key to IPC ID, creating if necessary
fn key_to_id(key: Key, resource_type: IpcResourceType, create: bool) -> LinuxResult<IpcId> {
    let mut table = IPC_KEY_TABLE.write();

    if let Some((existing_type, id)) = table.get(&key) {
        if *existing_type != resource_type {
            return Err(LinuxError::EINVAL);
        }
        return Ok(*id);
    }

    if !create {
        return Err(LinuxError::ENOENT);
    }

    // Generate new IPC ID
    let id = NEXT_IPC_KEY.fetch_add(1, Ordering::SeqCst);
    table.insert(key, (resource_type, id));
    Ok(id)
}

/// IPC key type
pub type Key = i32;

/// Simplified `struct msqid_ds` for `IPC_STAT`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct MsqidDs {
    msg_perm_key: i32,
    msg_perm_uid: u32,
    msg_perm_gid: u32,
    msg_perm_cuid: u32,
    msg_perm_cgid: u32,
    msg_perm_mode: u16,
    msg_stime: u64,
    msg_rtime: u64,
    msg_ctime: u64,
    msg_cbytes: u64,
    msg_qnum: u64,
    msg_qbytes: u64,
    msg_lspid: u32,
    msg_lrpid: u32,
}

/// Simplified `struct semid_ds` for `IPC_STAT`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct SemidDs {
    sem_perm_key: i32,
    sem_perm_uid: u32,
    sem_perm_gid: u32,
    sem_perm_cuid: u32,
    sem_perm_cgid: u32,
    sem_perm_mode: u16,
    sem_otime: u64,
    sem_ctime: u64,
    sem_nsems: u64,
}

/// Simplified `struct shmid_ds` for `IPC_STAT`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct ShmidDs {
    shm_perm_key: i32,
    shm_perm_uid: u32,
    shm_perm_gid: u32,
    shm_perm_cuid: u32,
    shm_perm_cgid: u32,
    shm_perm_mode: u16,
    shm_segsz: u64,
    shm_atime: u64,
    shm_dtime: u64,
    shm_ctime: u64,
    shm_cpid: u32,
    shm_lpid: u32,
    shm_nattch: u64,
}

/// Message queue ID type
pub type MsqId = i32;

/// Semaphore ID type
pub type SemId = i32;

/// Shared memory ID type
pub type ShmId = i32;

// IPC flags
const IPC_CREAT: i32 = 0o1000;
const IPC_EXCL: i32 = 0o2000;

// Message queue constants
const MSG_MAX_SIZE: usize = 8192;
const MSG_MAX_QUEUE: usize = 256;

/// msgget - get message queue identifier
pub fn msgget(key: Key, msgflg: i32) -> LinuxResult<MsqId> {
    inc_ops();

    let create = (msgflg & IPC_CREAT) != 0;
    let exclusive = (msgflg & IPC_EXCL) != 0;

    // Try to get existing or create new
    match key_to_id(key, IpcResourceType::MessageQueue, create) {
        Ok(ipc_id) => {
            if exclusive && create {
                return Err(LinuxError::EEXIST);
            }

            // Check if message queue exists in IPC manager
            let ipc_manager = get_ipc_manager();

            // If not created yet, create it now
            if create {
                match ipc_manager.create_message_queue(MSG_MAX_QUEUE, MSG_MAX_SIZE) {
                    Ok(new_id) => {
                        // Update mapping to use actual IPC manager ID
                        let mut table = IPC_KEY_TABLE.write();
                        table.insert(key, (IpcResourceType::MessageQueue, new_id));
                        Ok(new_id as MsqId)
                    }
                    Err(_) => Err(LinuxError::ENOSPC),
                }
            } else {
                Ok(ipc_id as MsqId)
            }
        }
        Err(e) => Err(e),
    }
}

/// msgsnd - send message to message queue
pub fn msgsnd(msqid: MsqId, msgp: *const u8, msgsz: usize, _msgflg: i32) -> LinuxResult<i32> {
    inc_ops();

    if msgp.is_null() {
        return Err(LinuxError::EFAULT);
    }

    if msgsz > MSG_MAX_SIZE {
        return Err(LinuxError::EINVAL);
    }

    // Read message type (first 4 bytes) and data
    let msg_type = unsafe { *(msgp as *const u32) };
    let data_ptr = unsafe { msgp.add(4) };

    // Copy message data
    let mut data = Vec::with_capacity(msgsz);
    for i in 0..msgsz {
        data.push(unsafe { *data_ptr.add(i) });
    }

    let ipc_manager = get_ipc_manager();
    let sender_pid = current_pid();

    match ipc_manager.send_message(msqid as IpcId, msg_type, data, sender_pid) {
        Ok(_) => Ok(0),
        Err(_) => Err(LinuxError::EAGAIN),
    }
}

/// msgrcv - receive message from message queue
pub fn msgrcv(
    msqid: MsqId,
    msgp: *mut u8,
    msgsz: usize,
    msgtyp: i64,
    _msgflg: i32,
) -> LinuxResult<isize> {
    inc_ops();

    if msgp.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let ipc_manager = get_ipc_manager();
    let msg_type = msgtyp as u32;

    match ipc_manager.receive_message(msqid as IpcId, msg_type) {
        Ok(Some(message)) => {
            let copy_size = core::cmp::min(message.data.len(), msgsz);

            // Write message type
            unsafe {
                *(msgp as *mut u32) = message.msg_type;
            }

            // Write message data
            let data_ptr = unsafe { msgp.add(4) };
            for i in 0..copy_size {
                unsafe {
                    *data_ptr.add(i) = message.data[i];
                }
            }

            Ok(copy_size as isize)
        }
        Ok(None) => Err(LinuxError::ENOMSG),
        Err(_) => Err(LinuxError::EINVAL),
    }
}

/// msgctl - message queue control operations
pub fn msgctl(msqid: MsqId, cmd: i32, buf: *mut u8) -> LinuxResult<i32> {
    inc_ops();

    // Command constants
    const IPC_STAT: i32 = 2;
    const IPC_SET: i32 = 1;
    const IPC_RMID: i32 = 0;

    match cmd {
        IPC_STAT => {
            if buf.is_null() {
                return Err(LinuxError::EFAULT);
            }
            // Look up the key for this msqid.
            let key_table = IPC_KEY_TABLE.read();
            let key = key_table
                .iter()
                .find(|(_, (rtype, id))| {
                    *rtype == IpcResourceType::MessageQueue && *id == msqid as IpcId
                })
                .map(|(k, _)| *k)
                .unwrap_or(0);

            let ds = MsqidDs {
                msg_perm_key: key,
                msg_perm_uid: current_pid(),
                msg_perm_gid: 0,
                msg_perm_cuid: current_pid(),
                msg_perm_cgid: 0,
                msg_perm_mode: 0o666,
                msg_qbytes: MSG_MAX_SIZE as u64,
                ..MsqidDs::default()
            };
            unsafe {
                *(buf as *mut MsqidDs) = ds;
            }
            Ok(0)
        }
        IPC_SET => {
            // IPC_SET updates queue parameters from buf.  Our message
            // queues are managed by the IPC manager with fixed limits,
            // so we accept but ignore the update.
            Ok(0)
        }
        IPC_RMID => {
            // Remove message queue
            let mut table = IPC_KEY_TABLE.write();

            // Find and remove the key mapping
            let keys_to_remove: Vec<Key> = table
                .iter()
                .filter(|(_, (rtype, id))| {
                    *rtype == IpcResourceType::MessageQueue && *id == msqid as IpcId
                })
                .map(|(k, _)| *k)
                .collect();

            for key in keys_to_remove {
                table.remove(&key);
            }

            Ok(0)
        }
        _ => Err(LinuxError::EINVAL),
    }
}

/// semget - get semaphore set identifier
pub fn semget(key: Key, nsems: i32, semflg: i32) -> LinuxResult<SemId> {
    inc_ops();

    if nsems < 0 || nsems > 256 {
        return Err(LinuxError::EINVAL);
    }

    let create = (semflg & IPC_CREAT) != 0;
    let exclusive = (semflg & IPC_EXCL) != 0;

    // Try to get existing or create new
    match key_to_id(key, IpcResourceType::Semaphore, create) {
        Ok(sem_id) => {
            if exclusive && create {
                return Err(LinuxError::EEXIST);
            }

            // Check if semaphore set exists
            let sem_table = SEMAPHORE_TABLE.read();
            if sem_table.contains_key(&sem_id) {
                return Ok(sem_id as SemId);
            }
            drop(sem_table);

            // Create new semaphore set if requested
            if create {
                let mut semaphores = Vec::with_capacity(nsems as usize);
                for _ in 0..nsems {
                    semaphores.push(0); // Initialize all semaphores to 0
                }

                let sem_set = SemaphoreSet {
                    id: sem_id,
                    semaphores,
                    owner_pid: current_pid(),
                    waiters: Vec::new(),
                };

                let mut sem_table = SEMAPHORE_TABLE.write();
                sem_table.insert(sem_id, sem_set);

                Ok(sem_id as SemId)
            } else {
                Err(LinuxError::ENOENT)
            }
        }
        Err(e) => Err(e),
    }
}

/// Semaphore operation structure (struct sembuf in Linux)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct SemBuf {
    sem_num: u16, // Semaphore number
    sem_op: i16,  // Semaphore operation
    sem_flg: i16, // Operation flags
}

/// IPC_NOWAIT flag for semop
const SEM_IPC_NOWAIT: i16 = 0o4000;

/// Check if all operations in a semop batch can proceed without blocking.
/// Returns Ok(()) if all can proceed, or Err(index) indicating the first
/// operation that would block.
fn try_semops(sem_set: &SemaphoreSet, ops: &[SemBuf]) -> Result<(), usize> {
    // Work on a temporary copy to handle atomicity (all ops or none)
    let mut temp: Vec<i32> = sem_set.semaphores.clone();

    for (i, op) in ops.iter().enumerate() {
        let sem_num = op.sem_num as usize;
        if sem_num >= temp.len() {
            return Err(usize::MAX); // EFBIG indicator
        }

        if op.sem_op > 0 {
            temp[sem_num] += op.sem_op as i32;
        } else if op.sem_op < 0 {
            let new_val = temp[sem_num] + op.sem_op as i32;
            if new_val < 0 {
                return Err(i); // Would block at operation i
            }
            temp[sem_num] = new_val;
        } else {
            // Wait for zero
            if temp[sem_num] != 0 {
                return Err(i); // Would block at operation i
            }
        }
    }

    Ok(())
}

/// Apply semaphore operations (assumes they won't block — caller verified with try_semops).
/// Wakes any waiters that can now proceed after the operations are applied.
fn apply_semops(sem_set: &mut SemaphoreSet, ops: &[SemBuf]) {
    for op in ops {
        let sem_num = op.sem_num as usize;
        if op.sem_op > 0 {
            sem_set.semaphores[sem_num] += op.sem_op as i32;
        } else if op.sem_op < 0 {
            sem_set.semaphores[sem_num] += op.sem_op as i32;
        }
        // sem_op == 0: wait-for-zero is a no-op on the value
    }

    // Wake any waiters that can now proceed
    let mut to_wake: Vec<u32> = Vec::new();
    sem_set.waiters.retain(|entry| {
        let sem_num = entry.sem_num as usize;
        let can_proceed = if entry.sem_op < 0 {
            (sem_set.semaphores[sem_num] + entry.sem_op as i32) >= 0
        } else if entry.sem_op == 0 {
            sem_set.semaphores[sem_num] == 0
        } else {
            true
        };

        if can_proceed {
            to_wake.push(entry.pid);
            false // Remove from waiters
        } else {
            true // Keep waiting
        }
    });

    for pid in to_wake {
        let _ = crate::process::get_process_manager().unblock_process(pid);
    }
}

/// semop - semaphore operations
pub fn semop(semid: SemId, sops: *mut u8, nsops: usize) -> LinuxResult<i32> {
    inc_ops();

    if sops.is_null() && nsops > 0 {
        return Err(LinuxError::EFAULT);
    }

    if nsops == 0 {
        return Ok(0);
    }

    // Parse semaphore operations
    let sembuf_ptr = sops as *const SemBuf;
    let operations: Vec<SemBuf> = (0..nsops).map(|i| unsafe { *sembuf_ptr.add(i) }).collect();

    // Check for IPC_NOWAIT on any operation
    let nowait = operations.iter().any(|op| op.sem_flg & SEM_IPC_NOWAIT != 0);

    let pid = crate::process::current_pid();

    loop {
        let mut sem_table = SEMAPHORE_TABLE.write();
        let sem_set = sem_table
            .get_mut(&(semid as IpcId))
            .ok_or(LinuxError::EINVAL)?;

        // Try to perform all operations atomically
        match try_semops(sem_set, &operations) {
            Ok(()) => {
                apply_semops(sem_set, &operations);
                drop(sem_table);
                return Ok(0);
            }
            Err(usize::MAX) => {
                // EFBIG — semaphore number out of range
                return Err(LinuxError::EFBIG);
            }
            Err(block_idx) => {
                // Operation block_idx would block
                if nowait {
                    return Err(LinuxError::EAGAIN);
                }

                // Add this process to the wait queue for the blocking semaphore
                let block_op = operations[block_idx];
                sem_set.waiters.push(SemWaitEntry {
                    pid: pid as u32,
                    sem_num: block_op.sem_num,
                    sem_op: block_op.sem_op,
                });
                drop(sem_table);

                // Block the current process
                let pm = crate::process::get_process_manager();
                let _ = pm.block_process(pid);

                // Yield CPU — when we're unblocked, we'll retry the operations
                crate::process::scheduler::yield_cpu();

                // After yield_cpu returns, we've been rescheduled.
                // Loop back and retry the operations.
                continue;
            }
        }
    }
}

/// semctl - semaphore control operations
pub fn semctl(semid: SemId, semnum: i32, cmd: i32, arg: u64) -> LinuxResult<i32> {
    inc_ops();

    // Command constants
    const IPC_STAT: i32 = 2;
    const IPC_SET: i32 = 1;
    const IPC_RMID: i32 = 0;
    const GETVAL: i32 = 12;
    const SETVAL: i32 = 16;

    match cmd {
        IPC_STAT => {
            let buf = arg as *mut u8;
            if buf.is_null() {
                return Err(LinuxError::EFAULT);
            }
            let sem_table = SEMAPHORE_TABLE.read();
            let sem_set = sem_table.get(&(semid as IpcId)).ok_or(LinuxError::EINVAL)?;

            let key_table = IPC_KEY_TABLE.read();
            let key = key_table
                .iter()
                .find(|(_, (rtype, id))| {
                    *rtype == IpcResourceType::Semaphore && *id == semid as IpcId
                })
                .map(|(k, _)| *k)
                .unwrap_or(0);

            let ds = SemidDs {
                sem_perm_key: key,
                sem_perm_uid: sem_set.owner_pid,
                sem_perm_cuid: sem_set.owner_pid,
                sem_nsems: sem_set.semaphores.len() as u64,
                ..SemidDs::default()
            };
            unsafe {
                *(buf as *mut SemidDs) = ds;
            }
            Ok(0)
        }
        IPC_SET => {
            // IPC_SET updates semaphore permissions.  We accept but
            // ignore since our permission model is minimal.
            Ok(0)
        }
        IPC_RMID => {
            // Remove semaphore set
            let mut sem_table = SEMAPHORE_TABLE.write();
            sem_table.remove(&(semid as IpcId));

            // Remove from key table
            let mut table = IPC_KEY_TABLE.write();
            let keys_to_remove: Vec<Key> = table
                .iter()
                .filter(|(_, (rtype, id))| {
                    *rtype == IpcResourceType::Semaphore && *id == semid as IpcId
                })
                .map(|(k, _)| *k)
                .collect();

            for key in keys_to_remove {
                table.remove(&key);
            }

            Ok(0)
        }
        GETVAL => {
            // Get semaphore value
            let sem_table = SEMAPHORE_TABLE.read();
            let sem_set = sem_table.get(&(semid as IpcId)).ok_or(LinuxError::EINVAL)?;

            if semnum < 0 || semnum as usize >= sem_set.semaphores.len() {
                return Err(LinuxError::EINVAL);
            }

            Ok(sem_set.semaphores[semnum as usize])
        }
        SETVAL => {
            // Set semaphore value
            let mut sem_table = SEMAPHORE_TABLE.write();
            let sem_set = sem_table
                .get_mut(&(semid as IpcId))
                .ok_or(LinuxError::EINVAL)?;

            if semnum < 0 || semnum as usize >= sem_set.semaphores.len() {
                return Err(LinuxError::EINVAL);
            }

            sem_set.semaphores[semnum as usize] = arg as i32;
            Ok(0)
        }
        _ => Err(LinuxError::EINVAL),
    }
}

/// semtimedop - semaphore operations with timeout
///
/// Delegates to sysv_ipc::semtimedop which implements proper blocking
/// with timeout support via the process scheduler.
pub fn semtimedop(
    semid: SemId,
    sops: *mut u8,
    nsops: usize,
    timeout: *const u8,
) -> LinuxResult<i32> {
    inc_ops();
    let ret = crate::sysv_ipc::semtimedop(
        semid as i32,
        sops as *const crate::sysv_ipc::SemBuf,
        nsops as u32,
        timeout,
    );
    if ret < 0 {
        Err(LinuxError::from_errno(-ret))
    } else {
        Ok(ret)
    }
}

/// shmget - get shared memory segment identifier
pub fn shmget(key: Key, size: usize, shmflg: i32) -> LinuxResult<ShmId> {
    inc_ops();

    if size == 0 {
        return Err(LinuxError::EINVAL);
    }

    let create = (shmflg & IPC_CREAT) != 0;
    let exclusive = (shmflg & IPC_EXCL) != 0;

    // Try to get existing or create new
    match key_to_id(key, IpcResourceType::SharedMemory, create) {
        Ok(shm_id) => {
            if exclusive && create {
                return Err(LinuxError::EEXIST);
            }

            // Create shared memory segment if requested
            if create {
                let ipc_manager = get_ipc_manager();

                // Determine permissions from flags (lower 9 bits)
                let mode = shmflg & 0o777;
                let permissions = if mode & 0o200 != 0 {
                    SharedMemoryPermissions::ReadWrite
                } else {
                    SharedMemoryPermissions::ReadOnly
                };

                match ipc_manager.create_shared_memory(size, permissions) {
                    Ok(new_id) => {
                        // Update mapping
                        let mut table = IPC_KEY_TABLE.write();
                        table.insert(key, (IpcResourceType::SharedMemory, new_id));
                        Ok(new_id as ShmId)
                    }
                    Err(_) => Err(LinuxError::ENOMEM),
                }
            } else {
                Ok(shm_id as ShmId)
            }
        }
        Err(e) => Err(e),
    }
}

/// Shared memory attachment table (maps addresses to IPC IDs)
static SHM_ATTACH_TABLE: RwLock<BTreeMap<u64, IpcId>> = RwLock::new(BTreeMap::new());

/// shmat - attach shared memory segment
pub fn shmat(shmid: ShmId, _shmaddr: *const u8, _shmflg: i32) -> LinuxResult<*mut u8> {
    inc_ops();

    let ipc_manager = get_ipc_manager();
    let pid = current_pid();

    match ipc_manager.attach_shared_memory(shmid as IpcId, pid) {
        Ok(virt_addr) => {
            let addr = virt_addr.as_u64();

            // Store mapping for detachment
            let mut attach_table = SHM_ATTACH_TABLE.write();
            attach_table.insert(addr, shmid as IpcId);

            Ok(addr as *mut u8)
        }
        Err(_) => Err(LinuxError::EINVAL),
    }
}

/// shmdt - detach shared memory segment
pub fn shmdt(shmaddr: *const u8) -> LinuxResult<i32> {
    inc_ops();

    if shmaddr.is_null() {
        return Err(LinuxError::EINVAL);
    }

    let addr = shmaddr as u64;

    // Find the shared memory ID from the address
    let mut attach_table = SHM_ATTACH_TABLE.write();
    let shm_id = attach_table.remove(&addr).ok_or(LinuxError::EINVAL)?;

    let ipc_manager = get_ipc_manager();
    let pid = current_pid();

    match ipc_manager.detach_shared_memory(shm_id, pid) {
        Ok(_) => Ok(0),
        Err(_) => Err(LinuxError::EINVAL),
    }
}

/// shmctl - shared memory control operations
pub fn shmctl(shmid: ShmId, cmd: i32, buf: *mut u8) -> LinuxResult<i32> {
    inc_ops();

    // Command constants
    const IPC_STAT: i32 = 2;
    const IPC_SET: i32 = 1;
    const IPC_RMID: i32 = 0;

    match cmd {
        IPC_STAT => {
            if buf.is_null() {
                return Err(LinuxError::EFAULT);
            }
            let key_table = IPC_KEY_TABLE.read();
            let key = key_table
                .iter()
                .find(|(_, (rtype, id))| {
                    *rtype == IpcResourceType::SharedMemory && *id == shmid as IpcId
                })
                .map(|(k, _)| *k)
                .unwrap_or(0);

            // Count attachments for this segment.
            let attach_table = SHM_ATTACH_TABLE.read();
            let nattch = attach_table
                .values()
                .filter(|id| **id == shmid as IpcId)
                .count() as u64;

            let ds = ShmidDs {
                shm_perm_key: key,
                shm_perm_uid: current_pid(),
                shm_perm_cuid: current_pid(),
                shm_perm_mode: 0o666,
                shm_nattch: nattch,
                ..ShmidDs::default()
            };
            unsafe {
                *(buf as *mut ShmidDs) = ds;
            }
            Ok(0)
        }
        IPC_SET => {
            // IPC_SET updates segment permissions.  We accept but
            // ignore since our permission model is minimal.
            Ok(0)
        }
        IPC_RMID => {
            // Mark segment for deletion
            // It will be removed when all processes detach

            // Remove from key table
            let mut table = IPC_KEY_TABLE.write();
            let keys_to_remove: Vec<Key> = table
                .iter()
                .filter(|(_, (rtype, id))| {
                    *rtype == IpcResourceType::SharedMemory && *id == shmid as IpcId
                })
                .map(|(k, _)| *k)
                .collect();

            for key in keys_to_remove {
                table.remove(&key);
            }

            Ok(0)
        }
        _ => Err(LinuxError::EINVAL),
    }
}

pub fn mq_open(name: *const u8, oflag: i32, _mode: u32, attr: *const MqAttr) -> LinuxResult<i32> {
    inc_ops();
    let name = normalize_mq_name(name)?;
    let mut names = MQ_BY_NAME.write();
    let mut queues = MQ_QUEUES.write();

    let queue_id = if let Some(existing) = names.get(&name).copied() {
        if (oflag & O_CREAT) != 0 && (oflag & O_EXCL) != 0 {
            return Err(LinuxError::EEXIST);
        }
        existing
    } else {
        if (oflag & O_CREAT) == 0 {
            return Err(LinuxError::ENOENT);
        }
        let queue_id = NEXT_MQ_ID.fetch_add(1, Ordering::Relaxed);
        let attr = mq_attr_from_user(attr, oflag)?;
        queues.insert(
            queue_id,
            PosixMessageQueue {
                name: name.clone(),
                attr,
                messages: VecDeque::new(),
                unlinked: false,
                notify_pid: None,
            },
        );
        names.insert(name, queue_id);
        queue_id
    };

    let desc = NEXT_MQ_DESC.fetch_add(1, Ordering::Relaxed) as i32;
    MQ_DESCRIPTORS.write().insert(
        desc,
        MqDescriptor {
            queue_id,
            flags: oflag,
        },
    );
    Ok(desc)
}

pub fn mq_unlink(name: *const u8) -> LinuxResult<i32> {
    inc_ops();
    let name = normalize_mq_name(name)?;
    let queue_id = MQ_BY_NAME.write().remove(&name).ok_or(LinuxError::ENOENT)?;
    if let Some(queue) = MQ_QUEUES.write().get_mut(&queue_id) {
        queue.unlinked = true;
    }
    Ok(0)
}

pub fn mq_timedsend(
    mqd: i32,
    msg_ptr: *const u8,
    msg_len: usize,
    msg_prio: u32,
    _abs_timeout: *const super::types::TimeSpec,
) -> LinuxResult<i32> {
    inc_ops();
    if msg_ptr.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let desc = mq_descriptor(mqd)?;
    let mut queues = MQ_QUEUES.write();
    let queue = queues.get_mut(&desc.queue_id).ok_or(LinuxError::EBADF)?;
    if msg_len > queue.attr.mq_msgsize as usize {
        return Err(LinuxError::EINVAL);
    }
    if queue.messages.len() >= queue.attr.mq_maxmsg as usize {
        return Err(LinuxError::EAGAIN);
    }

    let mut data = Vec::with_capacity(msg_len);
    for i in 0..msg_len {
        data.push(unsafe { *msg_ptr.add(i) });
    }
    let sequence = NEXT_MQ_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let insert_at = queue
        .messages
        .iter()
        .position(|msg| msg.priority < msg_prio)
        .unwrap_or(queue.messages.len());
    queue.messages.insert(
        insert_at,
        MqMessage {
            priority: msg_prio,
            data,
            sequence,
        },
    );
    queue.attr.mq_curmsgs = queue.messages.len() as i64;
    Ok(0)
}

pub fn mq_timedreceive(
    mqd: i32,
    msg_ptr: *mut u8,
    msg_len: usize,
    msg_prio: *mut u32,
    _abs_timeout: *const super::types::TimeSpec,
) -> LinuxResult<isize> {
    inc_ops();
    if msg_ptr.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let desc = mq_descriptor(mqd)?;
    let mut queues = MQ_QUEUES.write();
    let queue = queues.get_mut(&desc.queue_id).ok_or(LinuxError::EBADF)?;
    if msg_len < queue.attr.mq_msgsize as usize {
        return Err(LinuxError::EINVAL);
    }
    let msg = queue.messages.pop_front().ok_or(LinuxError::EAGAIN)?;
    for (i, byte) in msg.data.iter().enumerate() {
        unsafe {
            *msg_ptr.add(i) = *byte;
        }
    }
    if !msg_prio.is_null() {
        unsafe {
            *msg_prio = msg.priority;
        }
    }
    queue.attr.mq_curmsgs = queue.messages.len() as i64;
    Ok(msg.data.len() as isize)
}

pub fn mq_notify(mqd: i32, sevp: *const u8) -> LinuxResult<i32> {
    inc_ops();
    let desc = mq_descriptor(mqd)?;
    let mut queues = MQ_QUEUES.write();
    let queue = queues.get_mut(&desc.queue_id).ok_or(LinuxError::EBADF)?;
    if sevp.is_null() {
        queue.notify_pid = None;
    } else if queue.notify_pid.is_some() {
        return Err(LinuxError::EBUSY);
    } else {
        queue.notify_pid = Some(current_pid() as i32);
    }
    Ok(0)
}

pub fn mq_getsetattr(mqd: i32, newattr: *const MqAttr, oldattr: *mut MqAttr) -> LinuxResult<i32> {
    inc_ops();
    let desc = mq_descriptor(mqd)?;
    let mut queues = MQ_QUEUES.write();
    let queue = queues.get_mut(&desc.queue_id).ok_or(LinuxError::EBADF)?;
    let mut current = queue.attr;
    current.mq_flags = (desc.flags & O_NONBLOCK) as i64;
    current.mq_curmsgs = queue.messages.len() as i64;
    copy_mq_attr_to_user(oldattr, current)?;

    if !newattr.is_null() {
        let new_flags = unsafe { (*newattr).mq_flags } as i32;
        if let Some(desc) = MQ_DESCRIPTORS.write().get_mut(&mqd) {
            desc.flags = (desc.flags & !O_NONBLOCK) | (new_flags & O_NONBLOCK);
        }
    }
    Ok(0)
}

/// pipe - create pipe (returns read and write file descriptors)
pub fn pipe(pipefd: *mut [Fd; 2]) -> LinuxResult<i32> {
    inc_ops();
    super::special_fd::pipe(pipefd)
}

/// pipe2 - create pipe with flags
pub fn pipe2(pipefd: *mut [Fd; 2], flags: i32) -> LinuxResult<i32> {
    inc_ops();
    super::special_fd::pipe2(pipefd, flags)
}

/// eventfd - create file descriptor for event notification
pub fn eventfd(initval: u32, flags: i32) -> LinuxResult<Fd> {
    inc_ops();
    super::special_fd::eventfd2(initval, flags)
}

/// eventfd2 - create file descriptor for event notification with flags
pub fn eventfd2(initval: u32, flags: i32) -> LinuxResult<Fd> {
    inc_ops();
    super::special_fd::eventfd2(initval, flags)
}

/// signalfd - create file descriptor for accepting signals
pub fn signalfd(fd: Fd, mask: *const SigSet, flags: i32) -> LinuxResult<Fd> {
    inc_ops();

    if mask.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let signal_mask = unsafe { *(mask as *const u64) };
    super::special_fd::signalfd(fd, signal_mask, flags)
}

/// timerfd_create - create a timer that delivers events via file descriptor
pub fn timerfd_create(clockid: i32, flags: i32) -> LinuxResult<Fd> {
    inc_ops();
    super::special_fd::timerfd_create(clockid, flags)
}

/// Timer specification structure (struct itimerspec)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct ITimerSpec {
    it_interval_sec: u64,
    it_interval_nsec: u64,
    it_value_sec: u64,
    it_value_nsec: u64,
}

/// timerfd_settime - arm/disarm timer via file descriptor
pub fn timerfd_settime(
    fd: Fd,
    flags: i32,
    new_value: *const u8,
    old_value: *mut u8,
) -> LinuxResult<i32> {
    inc_ops();
    super::special_fd::timerfd_settime(fd, flags, new_value, old_value)
}

/// timerfd_gettime - get current setting of timer via file descriptor
pub fn timerfd_gettime(fd: Fd, curr_value: *mut u8) -> LinuxResult<i32> {
    inc_ops();
    super::special_fd::timerfd_gettime(fd, curr_value)
}

/// memfd_create - create an anonymous file (re-exported from memory_ops)
pub use super::memory_ops::memfd_create;

#[cfg(any())]
mod tests {
    use super::*;

    #[cfg(feature = "disabled-tests")]
    #[test_case]
    fn test_ipc_key_operations() {
        assert!(msgget(1234, 0).is_ok());
        assert!(semget(5678, 1, 0).is_ok());
        assert!(shmget(9012, 4096, 0).is_ok());
    }

    #[cfg(feature = "disabled-tests")]
    #[test_case]
    fn test_event_fd_creation() {
        assert!(eventfd(0, 0).is_ok());
        assert!(timerfd_create(clock::CLOCK_MONOTONIC, 0).is_ok());
    }
}
