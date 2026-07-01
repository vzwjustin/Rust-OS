//! SysV IPC — semaphores, shared memory, and message queues
//!
//! Ported from Linux ipc/ (sem.c, shm.c, msg.c).
//! Implements semctl/semop/semtimedop and shmat/shmctl/shmget.
//! Message queues (msgget/msgsnd/msgrcv/msgctl) are also included.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::{Mutex, RwLock};

// ── IPC constants ───────────────────────────────────────────────────────

pub const IPC_CREAT: i32 = 0o1000;
pub const IPC_EXCL: i32 = 0o2000;
pub const IPC_NOWAIT: i32 = 0o4000;

pub const IPC_RMID: i32 = 0;
pub const IPC_SET: i32 = 1;
pub const IPC_STAT: i32 = 2;
pub const IPC_INFO: i32 = 3;

pub const SEM_UNDO: i16 = 0x1000;
pub const GETPID: i32 = 11;
pub const GETVAL: i32 = 12;
pub const GETALL: i32 = 13;
pub const GETNCNT: i32 = 14;
pub const GETZCNT: i32 = 15;
pub const SETVAL: i32 = 16;
pub const SETALL: i32 = 17;
pub const SEM_STAT: i32 = 18;

pub const SHM_RDONLY: i32 = 0o10000;
pub const SHM_RND: i32 = 0o20000;
pub const SHM_REMAP: i32 = 0o40000;
pub const SHM_EXEC: i32 = 0o100000;
pub const SHM_LOCK: i32 = 11;
pub const SHM_UNLOCK: i32 = 12;
pub const SHM_STAT: i32 = 13;
pub const SHM_INFO: i32 = 14;

// ── Data structures ─────────────────────────────────────────────────────

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SemBuf {
    pub sem_num: u16,
    pub sem_op: i16,
    pub sem_flg: i16,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct IpcPerm {
    pub key: u32,
    pub uid: u32,
    pub gid: u32,
    pub cuid: u32,
    pub cgid: u32,
    pub mode: u16,
    pub __seq: u16,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SemidDs {
    pub sem_perm: IpcPerm,
    pub sem_otime: u64,
    pub sem_ctime: u64,
    pub sem_nsems: u32,
    pub __unused: [u64; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct ShmidDs {
    pub shm_perm: IpcPerm,
    pub shm_segsz: u64,
    pub shm_atime: u64,
    pub shm_dtime: u64,
    pub shm_ctime: u64,
    pub shm_cpid: u32,
    pub shm_lpid: u32,
    pub shm_nattch: u32,
    pub __unused: [u64; 2],
}

// ── Semaphore set ───────────────────────────────────────────────────────

pub struct SemaphoreSet {
    pub id: u32,
    pub key: u32,
    pub sems: Vec<i16>,
    pub perm: IpcPerm,
    pub ctime: u64,
    pub otime: u64,
    /// PID of the last process to perform a semop on this set (GETPID).
    pub last_pid: u32,
    /// PIDs waiting for semaphore values to change.
    /// Each entry records which sem_num and sem_op the waiter needs,
    /// plus an optional deadline in nanoseconds since boot (0 = no timeout).
    pub waiters: Vec<SemWaiter>,
}

/// A pending semaphore waiter.
#[derive(Debug, Clone, Copy)]
pub struct SemWaiter {
    pub pid: u32,
    pub sem_num: u16,
    /// The sem_op value from the blocking operation:
    /// negative means waiting for semval + sem_op >= 0,
    /// zero means waiting for semval == 0.
    pub sem_op: i16,
    /// Deadline in nanoseconds since boot (0 = no timeout).
    pub deadline_ns: u64,
}

// ── Shared memory segment ───────────────────────────────────────────────

pub struct ShmSegment {
    pub id: u32,
    pub key: u32,
    pub size: u64,
    pub perm: IpcPerm,
    pub cpid: u32,
    pub lpid: u32,
    pub nattch: u32,
    pub ctime: u64,
    pub atime: u64,
    pub dtime: u64,
    pub data: Vec<u8>,
    /// Address returned to the caller by `shmat`.  This is a separately
    /// allocated, fixed-size buffer (`Box<[u8]>`) whose pointer will not
    /// change even if `data` is reallocated by a future `shmctl` resize.
    pub attach_ptr: u64,
}

// ── Message queue ───────────────────────────────────────────────────────

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MsgBuf {
    pub mtype: i64,
    pub mtext: [u8; 0], // Flexible array
}

pub struct MessageQueue {
    pub id: u32,
    pub key: u32,
    pub perm: IpcPerm,
    pub messages: Vec<(i64, Vec<u8>)>,
    pub ctime: u64,
    pub rtime: u64,
    pub stime: u64,
}

// ── Global state ────────────────────────────────────────────────────────

static SEM_SETS: RwLock<BTreeMap<u32, Mutex<SemaphoreSet>>> = RwLock::new(BTreeMap::new());
static SHM_SEGS: RwLock<BTreeMap<u32, Mutex<ShmSegment>>> = RwLock::new(BTreeMap::new());
/// Maps attach addresses to their `Box<[u8]>` buffers so they stay alive
/// until `shmdt` frees them.
static SHM_ATTACHMENTS: RwLock<BTreeMap<u64, (u32, Box<[u8]>)>> = RwLock::new(BTreeMap::new());
static MSG_QUEUES: RwLock<BTreeMap<u32, Mutex<MessageQueue>>> = RwLock::new(BTreeMap::new());
static NEXT_SEM_ID: AtomicU32 = AtomicU32::new(1);
static NEXT_SHM_ID: AtomicU32 = AtomicU32::new(1);
static NEXT_MSG_ID: AtomicU32 = AtomicU32::new(1);

// ── Semaphore syscalls ──────────────────────────────────────────────────

/// semget — get semaphore set
pub fn semget(key: u32, nsems: i32, semflg: i32) -> i32 {
    if nsems < 0 || nsems > 32000 {
        return -22;
    }

    let create = semflg & IPC_CREAT != 0;
    let excl = semflg & IPC_EXCL != 0;

    if key == 0 {
        // IPC_PRIVATE — always create
        let id = NEXT_SEM_ID.fetch_add(1, Ordering::SeqCst);
        let now = crate::time::uptime_ns() / 1_000_000_000;
        let set = SemaphoreSet {
            id,
            key: 0,
            sems: vec![0i16; nsems as usize],
            perm: IpcPerm {
                key: 0,
                uid: 0,
                gid: 0,
                cuid: 0,
                cgid: 0,
                mode: (semflg & 0o777) as u16,
                __seq: 0,
            },
            ctime: now,
            otime: 0,
            last_pid: 0,
            waiters: Vec::new(),
        };
        SEM_SETS.write().insert(id, Mutex::new(set));
        return id as i32;
    }

    // Search for existing key
    let sets = SEM_SETS.read();
    for (&id, set_mutex) in sets.iter() {
        let set = set_mutex.lock();
        if set.key == key {
            if excl && create {
                return -17; // EEXIST
            }
            return id as i32;
        }
    }
    drop(sets);

    if !create {
        return -2; // ENOENT
    }

    let id = NEXT_SEM_ID.fetch_add(1, Ordering::SeqCst);
    let now = crate::time::uptime_ns() / 1_000_000_000;
    let set = SemaphoreSet {
        id,
        key,
        sems: vec![0i16; nsems as usize],
        perm: IpcPerm {
            key,
            uid: 0,
            gid: 0,
            cuid: 0,
            cgid: 0,
            mode: (semflg & 0o777) as u16,
            __seq: 0,
        },
        ctime: now,
        otime: 0,
        last_pid: 0,
        waiters: Vec::new(),
    };
    SEM_SETS.write().insert(id, Mutex::new(set));
    id as i32
}

/// semop — operate on semaphore set
pub fn semop(semid: i32, sops: *const SemBuf, nsops: u32) -> i32 {
    semtimedop(semid, sops, nsops, core::ptr::null())
}

/// Parse a `struct timespec` pointer into a deadline in nanoseconds since boot.
/// Returns 0 if no timeout (null pointer), or the absolute deadline.
fn parse_timeout(timeout: *const u8) -> u64 {
    if timeout.is_null() {
        return 0;
    }
    // struct timespec { tv_sec: i64, tv_nsec: i64 }
    // `timeout` may be unaligned for i64, so use unaligned reads to avoid UB.
    let secs = unsafe { core::ptr::read_unaligned(timeout as *const i64) };
    let nsecs = unsafe { core::ptr::read_unaligned((timeout as *const i64).add(1)) };
    if secs < 0 || nsecs < 0 || nsecs >= 1_000_000_000 {
        return 0; // Invalid — treat as no timeout
    }
    // Overflow-safe seconds→nanoseconds conversion and deadline addition,
    // saturating to u64::MAX (an effectively-never deadline) on overflow.
    let secs_ns = (secs as u64).checked_mul(1_000_000_000).unwrap_or(u64::MAX);
    let rel_ns = secs_ns.checked_add(nsecs as u64).unwrap_or(u64::MAX);
    crate::time::uptime_ns()
        .checked_add(rel_ns)
        .unwrap_or(u64::MAX)
}

/// Check whether a waiter's blocking condition is satisfied.
fn waiter_can_proceed(sems: &[i16], waiter: &SemWaiter) -> bool {
    let sem_num = waiter.sem_num as usize;
    if sem_num >= sems.len() {
        return true; // Semaphore was removed — let caller retry and get ERANGE
    }
    let current = sems[sem_num] as i32;
    if waiter.sem_op < 0 {
        current + waiter.sem_op as i32 >= 0
    } else if waiter.sem_op == 0 {
        current == 0
    } else {
        true
    }
}

/// Wake waiters that can now proceed after semaphore values changed.
/// Called while holding the set mutex.
fn wake_sem_waiters(set: &mut SemaphoreSet) {
    let mut to_wake: Vec<u32> = Vec::new();
    set.waiters.retain(|entry| {
        if waiter_can_proceed(&set.sems, entry) {
            to_wake.push(entry.pid);
            false
        } else {
            true
        }
    });
    for pid in to_wake {
        let _ = crate::process::get_process_manager().unblock_process(pid);
    }
}

/// Check for expired semaphore timeouts.  Called from the timer tick to
/// wake waiters whose deadline has passed with ETIMEDOUT.
pub fn check_sem_timeouts() {
    let now = crate::time::uptime_ns();
    // This runs from the timer interrupt. Process-context paths
    // (semtimedop/semctl/semget) hold these same locks with interrupts
    // enabled, so blocking here would deadlock the CPU. Use non-blocking
    // acquisitions and simply skip this tick if any lock is contended.
    let sets = match SEM_SETS.try_read() {
        Some(sets) => sets,
        None => return,
    };
    for (_id, set_mutex) in sets.iter() {
        let mut set = match set_mutex.try_lock() {
            Some(set) => set,
            None => continue,
        };
        let mut expired: Vec<u32> = Vec::new();
        set.waiters.retain(|entry| {
            if entry.deadline_ns != 0 && now >= entry.deadline_ns {
                expired.push(entry.pid);
                false
            } else {
                true
            }
        });
        for pid in expired {
            let _ = crate::process::get_process_manager().unblock_process(pid);
        }
    }
}

/// semtimedop — timed semaphore operation with blocking
///
/// If the operation cannot complete immediately and IPC_NOWAIT is not set,
/// the calling process blocks until the semaphore values change to allow
/// the operation, or until the timeout expires (if a timeout is given).
pub fn semtimedop(semid: i32, sops: *const SemBuf, nsops: u32, timeout: *const u8) -> i32 {
    if semid < 0 || sops.is_null() || nsops == 0 || nsops > 500 {
        return -22; // EINVAL
    }

    let deadline = parse_timeout(timeout);
    // If a timeout was provided but resolved to 0, it was invalid.
    if !timeout.is_null() && deadline == 0 {
        return -22; // EINVAL
    }

    let pid = crate::process::current_pid();

    loop {
        let sets = SEM_SETS.read();
        let set_mutex = match sets.get(&(semid as u32)) {
            Some(s) => s,
            None => return -43, // EIDRM
        };
        let mut set = set_mutex.lock();

        // Read operations
        let ops: Vec<SemBuf> = (0..nsops)
            .map(|i| unsafe { *sops.add(i as usize) })
            .collect();

        let nowait = ops.iter().any(|op| (op.sem_flg as i32) & IPC_NOWAIT != 0);

        // Validate sem_num range
        for op in &ops {
            if op.sem_num as usize >= set.sems.len() {
                return -34; // ERANGE
            }
        }

        // Try to apply all operations atomically
        let mut would_block_idx: Option<usize> = None;
        {
            let mut temp: Vec<i32> = set.sems.iter().map(|&v| v as i32).collect();
            for (i, op) in ops.iter().enumerate() {
                let sem_num = op.sem_num as usize;
                if op.sem_op > 0 {
                    temp[sem_num] += op.sem_op as i32;
                } else if op.sem_op < 0 {
                    let new_val = temp[sem_num] + op.sem_op as i32;
                    if new_val < 0 {
                        would_block_idx = Some(i);
                        break;
                    }
                    temp[sem_num] = new_val;
                } else {
                    // sem_op == 0: wait for zero
                    if temp[sem_num] != 0 {
                        would_block_idx = Some(i);
                        break;
                    }
                }
            }
        }

        if let Some(block_idx) = would_block_idx {
            if nowait {
                return -11; // EAGAIN
            }

            // Register as a waiter on the blocking semaphore
            let block_op = ops[block_idx];
            set.waiters.retain(|waiter| waiter.pid != pid);
            set.waiters.push(SemWaiter {
                pid,
                sem_num: block_op.sem_num,
                sem_op: block_op.sem_op,
                deadline_ns: deadline,
            });
            drop(set);
            drop(sets);

            // Block the current process and yield CPU
            let pm = crate::process::get_process_manager();
            let _ = pm.block_process(pid);
            crate::process::scheduler::yield_cpu();

            // When we get rescheduled, check for timeout
            if deadline != 0 && crate::time::uptime_ns() >= deadline {
                // Remove ourselves from waiters if still present
                let sets = SEM_SETS.read();
                if let Some(set_mutex) = sets.get(&(semid as u32)) {
                    let mut set = set_mutex.lock();
                    set.waiters.retain(|w| w.pid != pid);
                }
                return -110; // ETIMEDOUT
            }

            // Retry the operations
            continue;
        }

        // Apply operations
        for op in &ops {
            set.sems[op.sem_num as usize] =
                (set.sems[op.sem_num as usize] as i32 + op.sem_op as i32) as i16;
        }
        set.otime = crate::time::uptime_ns() / 1_000_000_000;
        set.last_pid = crate::process::current_pid();

        // Wake any waiters that can now proceed
        wake_sem_waiters(&mut set);

        return 0;
    }
}

/// semctl — semaphore control
pub fn semctl(semid: i32, semnum: i32, cmd: i32, arg: u64) -> i32 {
    if semid < 0 {
        return -22;
    }

    let sets = SEM_SETS.read();
    let set_mutex = match sets.get(&(semid as u32)) {
        Some(s) => s,
        None => return -43,
    };
    let mut set = set_mutex.lock();

    match cmd {
        IPC_RMID => {
            let waiters: Vec<u32> = set.waiters.iter().map(|waiter| waiter.pid).collect();
            set.waiters.clear();
            drop(set);
            drop(sets);
            SEM_SETS.write().remove(&(semid as u32));
            for pid in waiters {
                let _ = crate::process::get_process_manager().unblock_process(pid);
            }
            return 0;
        }
        IPC_STAT => {
            let ds = SemidDs {
                sem_perm: set.perm,
                sem_otime: set.otime,
                sem_ctime: set.ctime,
                sem_nsems: set.sems.len() as u32,
                __unused: [0; 2],
            };
            unsafe {
                *(arg as *mut SemidDs) = ds;
            }
            return 0;
        }
        GETVAL => {
            if semnum < 0 || semnum as usize >= set.sems.len() {
                return -34;
            }
            return set.sems[semnum as usize] as i32;
        }
        SETVAL => {
            if semnum < 0 || semnum as usize >= set.sems.len() {
                return -34;
            }
            set.sems[semnum as usize] = arg as i16;
            set.ctime = crate::time::uptime_ns() / 1_000_000_000;
            set.last_pid = crate::process::current_pid();
            wake_sem_waiters(&mut set);
            return 0;
        }
        GETPID => {
            return set.last_pid as i32;
        }
        GETNCNT => {
            if semnum < 0 || semnum as usize >= set.sems.len() {
                return -34;
            }
            return set
                .waiters
                .iter()
                .filter(|w| w.sem_num == semnum as u16 && w.sem_op < 0)
                .count() as i32;
        }
        GETZCNT => {
            if semnum < 0 || semnum as usize >= set.sems.len() {
                return -34;
            }
            return set
                .waiters
                .iter()
                .filter(|w| w.sem_num == semnum as u16 && w.sem_op == 0)
                .count() as i32;
        }
        GETALL => {
            let arr = arg as *mut u16;
            if arr.is_null() {
                return -14;
            }
            for (i, &val) in set.sems.iter().enumerate() {
                unsafe {
                    *arr.add(i) = val as u16;
                }
            }
            return 0;
        }
        SETALL => {
            let arr = arg as *const u16;
            if arr.is_null() {
                return -14;
            }
            let n = set.sems.len();
            for i in 0..n {
                set.sems[i] = unsafe { *arr.add(i) } as i16;
            }
            set.ctime = crate::time::uptime_ns() / 1_000_000_000;
            set.last_pid = crate::process::current_pid();
            wake_sem_waiters(&mut set);
            return 0;
        }
        IPC_SET => {
            let ds = unsafe { *(arg as *const SemidDs) };
            set.perm.uid = ds.sem_perm.uid;
            set.perm.gid = ds.sem_perm.gid;
            set.perm.mode = ds.sem_perm.mode;
            set.ctime = crate::time::uptime_ns() / 1_000_000_000;
            return 0;
        }
        _ => return -22,
    }
}

// ── Shared memory syscalls ──────────────────────────────────────────────

/// shmget — get shared memory segment
pub fn shmget(key: u32, size: usize, shmflg: i32) -> i32 {
    if size == 0 && key != 0 {
        // Lookup existing
        let segs = SHM_SEGS.read();
        for (&id, seg_mutex) in segs.iter() {
            let seg = seg_mutex.lock();
            if seg.key == key {
                return id as i32;
            }
        }
        return -2;
    }

    let create = shmflg & IPC_CREAT != 0;
    let excl = shmflg & IPC_EXCL != 0;

    if key == 0 {
        // IPC_PRIVATE
        let id = NEXT_SHM_ID.fetch_add(1, Ordering::SeqCst);
        let now = crate::time::uptime_ns() / 1_000_000_000;
        let pid = crate::process::current_pid();
        let seg = ShmSegment {
            id,
            key: 0,
            size: size as u64,
            perm: IpcPerm {
                key: 0,
                uid: 0,
                gid: 0,
                cuid: 0,
                cgid: 0,
                mode: (shmflg & 0o777) as u16,
                __seq: 0,
            },
            cpid: pid,
            lpid: pid,
            nattch: 0,
            ctime: now,
            atime: 0,
            dtime: 0,
            data: vec![0u8; size],
            attach_ptr: 0,
        };
        SHM_SEGS.write().insert(id, Mutex::new(seg));
        return id as i32;
    }

    // Search for existing key
    let segs = SHM_SEGS.read();
    for (&id, seg_mutex) in segs.iter() {
        let seg = seg_mutex.lock();
        if seg.key == key {
            if excl && create {
                return -17;
            }
            return id as i32;
        }
    }
    drop(segs);

    if !create {
        return -2;
    }

    if size > 256 * 1024 * 1024 {
        return -22;
    }

    let id = NEXT_SHM_ID.fetch_add(1, Ordering::SeqCst);
    let now = crate::time::uptime_ns() / 1_000_000_000;
    let pid = crate::process::current_pid();
    let seg = ShmSegment {
        id,
        key,
        size: size as u64,
        perm: IpcPerm {
            key,
            uid: 0,
            gid: 0,
            cuid: 0,
            cgid: 0,
            mode: (shmflg & 0o777) as u16,
            __seq: 0,
        },
        cpid: pid,
        lpid: pid,
        nattch: 0,
        ctime: now,
        atime: 0,
        dtime: 0,
        data: vec![0u8; size],
        attach_ptr: 0,
    };
    SHM_SEGS.write().insert(id, Mutex::new(seg));
    id as i32
}

/// shmat — attach shared memory segment
pub fn shmat(shmid: i32, shmaddr: u64, shmflg: i32) -> i64 {
    if shmid < 0 {
        return -22;
    }

    let segs = SHM_SEGS.read();
    let seg_mutex = match segs.get(&(shmid as u32)) {
        Some(s) => s,
        None => return -43,
    };
    let mut seg = seg_mutex.lock();

    // Allocate a fixed-size kernel buffer and copy the segment data into
    // it.  Unlike `Vec::as_ptr()`, a `Box<[u8]>`'s pointer is stable for
    // the lifetime of the allocation and won't be invalidated by a Vec
    // reallocation in `shmctl`.
    let buf_size = seg.data.len();
    let mut attach_buf = vec![0u8; buf_size].into_boxed_slice();
    attach_buf.copy_from_slice(&seg.data);
    let attach_addr = attach_buf.as_ptr() as u64;

    // Store the buffer so it lives as long as the attachment.  We keep it
    // in a static map keyed by the attach address so `shmdt` can find and
    // free it.
    SHM_ATTACHMENTS
        .write()
        .insert(attach_addr, (shmid as u32, attach_buf));

    // This implementation cannot place mappings at caller-requested virtual
    // addresses yet, so return the actual stable attachment handle that shmdt()
    // can later detach.
    let addr = attach_addr;

    seg.nattch += 1;
    seg.atime = crate::time::uptime_ns() / 1_000_000_000;
    seg.lpid = crate::process::current_pid();

    addr as i64
}

/// shmdt — detach shared memory segment
pub fn shmdt(shmaddr: u64) -> i32 {
    // Free the attachment buffer and remove it from the map.
    let shmid = match SHM_ATTACHMENTS.write().remove(&shmaddr) {
        Some((shmid, _buf)) => shmid,
        None => {
            return -22; // EINVAL — not a valid attachment
        }
    };

    // Find segment by recorded attachment id and decrement nattch.
    let segs = SHM_SEGS.read();
    let Some(seg_mutex) = segs.get(&shmid) else {
        return -22; // EINVAL — not a valid attachment
    };
    let mut seg = seg_mutex.lock();
    if seg.nattch > 0 {
        seg.nattch -= 1;
    }
    seg.dtime = crate::time::uptime_ns() / 1_000_000_000;
    seg.lpid = crate::process::current_pid();
    0
}

/// shmctl — shared memory control
pub fn shmctl(shmid: i32, cmd: i32, buf: u64) -> i32 {
    if shmid < 0 {
        return -22;
    }

    let segs = SHM_SEGS.read();
    let seg_mutex = match segs.get(&(shmid as u32)) {
        Some(s) => s,
        None => return -43,
    };
    let mut seg = seg_mutex.lock();

    match cmd {
        IPC_RMID => {
            drop(seg);
            drop(segs);
            SHM_SEGS.write().remove(&(shmid as u32));
            return 0;
        }
        IPC_STAT => {
            let ds = ShmidDs {
                shm_perm: seg.perm,
                shm_segsz: seg.size,
                shm_atime: seg.atime,
                shm_dtime: seg.dtime,
                shm_ctime: seg.ctime,
                shm_cpid: seg.cpid,
                shm_lpid: seg.lpid,
                shm_nattch: seg.nattch,
                __unused: [0; 2],
            };
            if buf != 0 {
                unsafe {
                    *(buf as *mut ShmidDs) = ds;
                }
            }
            return 0;
        }
        SHM_LOCK | SHM_UNLOCK => {
            return 0; // No swapping — no-op
        }
        IPC_SET => {
            if buf == 0 {
                return -14;
            }
            let ds = unsafe { *(buf as *const ShmidDs) };
            seg.perm.uid = ds.shm_perm.uid;
            seg.perm.gid = ds.shm_perm.gid;
            seg.perm.mode = ds.shm_perm.mode;
            seg.ctime = crate::time::uptime_ns() / 1_000_000_000;
            return 0;
        }
        _ => return -22,
    }
}

// ── Message queue syscalls ──────────────────────────────────────────────

/// msgget — get message queue
pub fn msgget(key: u32, msgflg: i32) -> i32 {
    let create = msgflg & IPC_CREAT != 0;
    let excl = msgflg & IPC_EXCL != 0;

    if key == 0 {
        let id = NEXT_MSG_ID.fetch_add(1, Ordering::SeqCst);
        let now = crate::time::uptime_ns() / 1_000_000_000;
        let q = MessageQueue {
            id,
            key: 0,
            perm: IpcPerm {
                key: 0,
                uid: 0,
                gid: 0,
                cuid: 0,
                cgid: 0,
                mode: (msgflg & 0o777) as u16,
                __seq: 0,
            },
            messages: Vec::new(),
            ctime: now,
            rtime: 0,
            stime: 0,
        };
        MSG_QUEUES.write().insert(id, Mutex::new(q));
        return id as i32;
    }

    let queues = MSG_QUEUES.read();
    for (&id, q_mutex) in queues.iter() {
        let q = q_mutex.lock();
        if q.key == key {
            if excl && create {
                return -17;
            }
            return id as i32;
        }
    }
    drop(queues);

    if !create {
        return -2;
    }

    let id = NEXT_MSG_ID.fetch_add(1, Ordering::SeqCst);
    let now = crate::time::uptime_ns() / 1_000_000_000;
    let q = MessageQueue {
        id,
        key,
        perm: IpcPerm {
            key,
            uid: 0,
            gid: 0,
            cuid: 0,
            cgid: 0,
            mode: (msgflg & 0o777) as u16,
            __seq: 0,
        },
        messages: Vec::new(),
        ctime: now,
        rtime: 0,
        stime: 0,
    };
    MSG_QUEUES.write().insert(id, Mutex::new(q));
    id as i32
}

/// msgsnd — send message to queue
pub fn msgsnd(msqid: i32, msgp: *const u8, msgsz: usize, msgflg: i32) -> i32 {
    if msqid < 0 || msgp.is_null() {
        return -22;
    }

    let queues = MSG_QUEUES.read();
    let q_mutex = match queues.get(&(msqid as u32)) {
        Some(q) => q,
        None => return -43,
    };
    let mut q = q_mutex.lock();

    // Read mtype (first 8 bytes) and mtext
    let mtype = unsafe { (msgp as *const i64).read_unaligned() };
    if mtype <= 0 {
        return -22;
    }
    let text = unsafe { core::slice::from_raw_parts(msgp.add(8), msgsz) }.to_vec();

    q.messages.push((mtype, text));
    q.stime = crate::time::uptime_ns() / 1_000_000_000;

    let _ = msgflg;
    0
}

/// msgrcv — receive message from queue
pub fn msgrcv(msqid: i32, msgp: *mut u8, msgsz: usize, msgtyp: i64, msgflg: i32) -> i32 {
    if msqid < 0 || msgp.is_null() {
        return -22;
    }

    let queues = MSG_QUEUES.read();
    let q_mutex = match queues.get(&(msqid as u32)) {
        Some(q) => q,
        None => return -43,
    };
    let mut q = q_mutex.lock();

    // Find matching message
    let mut found_idx = None;
    for (i, (mtype, _)) in q.messages.iter().enumerate() {
        if msgtyp == 0 {
            found_idx = Some(i);
            break;
        } else if msgtyp > 0 && *mtype == msgtyp {
            found_idx = Some(i);
            break;
        } else if msgtyp < 0 && *mtype <= (-msgtyp) as i64 {
            found_idx = Some(i);
            break;
        }
    }

    if let Some(idx) = found_idx {
        let (mtype, text) = q.messages.remove(idx);
        q.rtime = crate::time::uptime_ns() / 1_000_000_000;
        let copy_len = core::cmp::min(text.len(), msgsz);
        unsafe {
            (msgp as *mut i64).write_unaligned(mtype);
            core::ptr::copy_nonoverlapping(text.as_ptr(), msgp.add(8), copy_len);
        }
        return copy_len as i32;
    }

    let _ = msgflg;
    -42 // ENOMSG
}

/// msgctl — message queue control
pub fn msgctl(msqid: i32, cmd: i32, _buf: u64) -> i32 {
    if msqid < 0 {
        return -22;
    }
    match cmd {
        IPC_RMID => {
            MSG_QUEUES.write().remove(&(msqid as u32));
            0
        }
        IPC_STAT | IPC_SET => {
            // Would fill/set msqid_ds — accept silently
            0
        }
        _ => -22,
    }
}

/// Initialize the SysV IPC subsystem.
pub fn init() {
    crate::serial_println!("[ipc] SysV IPC subsystem initialized");
}
