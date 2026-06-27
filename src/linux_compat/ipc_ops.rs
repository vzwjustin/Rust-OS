//! Linux IPC operation APIs
//!
//! This module implements Linux-compatible IPC operations including
//! message queues, semaphores, shared memory, and event file descriptors.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use spin::RwLock;

use super::types::*;
use super::{LinuxError, LinuxResult};
use crate::process::current_pid;
use crate::process::ipc::{get_ipc_manager, IpcId, SharedMemoryPermissions};

/// Operation counter for statistics
static IPC_OPS_COUNT: AtomicU64 = AtomicU64::new(0);

/// Initialize IPC operations subsystem
pub fn init_ipc_operations() {
    IPC_OPS_COUNT.store(0, Ordering::Relaxed);
}

/// Get number of IPC operations performed
pub fn get_operation_count() -> u64 {
    IPC_OPS_COUNT.load(Ordering::Relaxed)
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

/// Semaphore structure for System V semaphores
#[derive(Debug)]
struct SemaphoreSet {
    id: IpcId,
    semaphores: Vec<i32>,
    owner_pid: u32,
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
pub fn msgctl(msqid: MsqId, cmd: i32, _buf: *mut u8) -> LinuxResult<i32> {
    inc_ops();

    // Command constants
    const IPC_STAT: i32 = 2;
    const IPC_SET: i32 = 1;
    const IPC_RMID: i32 = 0;

    match cmd {
        IPC_STAT => {
            // Return message queue statistics
            // For now, just return success
            Ok(0)
        }
        IPC_SET => {
            // Set message queue parameters
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

    let mut sem_table = SEMAPHORE_TABLE.write();
    let sem_set = sem_table
        .get_mut(&(semid as IpcId))
        .ok_or(LinuxError::EINVAL)?;

    // Perform all operations
    for op in operations {
        let sem_num = op.sem_num as usize;

        if sem_num >= sem_set.semaphores.len() {
            return Err(LinuxError::EFBIG);
        }

        if op.sem_op > 0 {
            // Increment semaphore (V operation)
            sem_set.semaphores[sem_num] += op.sem_op as i32;
        } else if op.sem_op < 0 {
            // Decrement semaphore (P operation)
            let new_val = sem_set.semaphores[sem_num] + op.sem_op as i32;
            if new_val < 0 {
                // Would block - for now return error
                return Err(LinuxError::EAGAIN);
            }
            sem_set.semaphores[sem_num] = new_val;
        } else {
            // Wait for zero
            if sem_set.semaphores[sem_num] != 0 {
                return Err(LinuxError::EAGAIN);
            }
        }
    }

    Ok(0)
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
            // Return semaphore set statistics
            Ok(0)
        }
        IPC_SET => {
            // Set semaphore set parameters
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
pub fn shmctl(shmid: ShmId, cmd: i32, _buf: *mut u8) -> LinuxResult<i32> {
    inc_ops();

    // Command constants
    const IPC_STAT: i32 = 2;
    const IPC_SET: i32 = 1;
    const IPC_RMID: i32 = 0;

    match cmd {
        IPC_STAT => {
            // Return shared memory segment statistics
            // For now, just return success
            Ok(0)
        }
        IPC_SET => {
            // Set shared memory segment parameters
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

    // Read the signal mask
    let signal_mask = unsafe { *(mask as *const u64) };

    if fd < 0 {
        // Create new signalfd
        let new_fd = NEXT_SIGNALFD.fetch_add(1, Ordering::SeqCst) as Fd;

        let signal_fd = SignalFd {
            mask: signal_mask,
            flags,
        };

        let mut table = SIGNALFD_TABLE.write();
        table.insert(new_fd, signal_fd);

        Ok(new_fd)
    } else {
        // Modify existing signalfd
        let mut table = SIGNALFD_TABLE.write();
        if let Some(signal_fd) = table.get_mut(&fd) {
            signal_fd.mask = signal_mask;
            signal_fd.flags = flags;
            Ok(fd)
        } else {
            Err(LinuxError::EBADF)
        }
    }
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
