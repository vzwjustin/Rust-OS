//! SysV IPC — semaphores, shared memory, and message queues
//!
//! Ported from Linux ipc/ (sem.c, shm.c, msg.c).
//! Implements semctl/semop/semtimedop and shmat/shmctl/shmget.
//! Message queues (msgget/msgsnd/msgrcv/msgctl) are also included.

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
    };
    SEM_SETS.write().insert(id, Mutex::new(set));
    id as i32
}

/// semop — operate on semaphore set
pub fn semop(semid: i32, sops: *const SemBuf, nsops: u32) -> i32 {
    semtimedop(semid, sops, nsops, core::ptr::null())
}

/// semtimedop — timed semaphore operation
pub fn semtimedop(semid: i32, sops: *const SemBuf, nsops: u32, _timeout: *const u8) -> i32 {
    if semid < 0 || sops.is_null() || nsops == 0 || nsops > 500 {
        return -22;
    }

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

    // Check if all operations can succeed
    let nowait = ops.iter().any(|op| (op.sem_flg as i32) & IPC_NOWAIT != 0);
    for op in &ops {
        if op.sem_num as usize >= set.sems.len() {
            return -34; // ERANGE
        }
    }

    // Try to apply all operations atomically
    let mut would_block = false;
    for op in &ops {
        let current = set.sems[op.sem_num as usize];
        let new_val = current as i32 + op.sem_op as i32;
        if op.sem_op > 0 {
            // Increment — always succeeds
        } else if op.sem_op < 0 {
            if new_val < 0 {
                if nowait {
                    return -11; // EAGAIN
                }
                would_block = true;
            }
        }
        // sem_op == 0: wait until sem becomes 0
        if op.sem_op == 0 && current != 0 {
            if nowait {
                return -11;
            }
            would_block = true;
        }
    }

    if would_block {
        return -11; // EAGAIN — we don't block in this implementation
    }

    // Apply operations
    for op in &ops {
        set.sems[op.sem_num as usize] =
            (set.sems[op.sem_num as usize] as i32 + op.sem_op as i32) as i16;
    }
    set.otime = crate::time::uptime_ns() / 1_000_000_000;

    0
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
            drop(set);
            drop(sets);
            SEM_SETS.write().remove(&(semid as u32));
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
            return 0;
        }
        GETPID => {
            return 0; // No tracking
        }
        GETNCNT | GETZCNT => {
            return 0; // No waiters
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

    // In a real kernel, we'd map the segment into the process address space.
    // For now, return a kernel-allocated address that can be used to access the data.
    let addr = if shmaddr == 0 {
        // Let kernel choose — use the data vector's address
        seg.data.as_ptr() as u64
    } else {
        if shmflg & SHM_RND != 0 {
            shmaddr & !0xFFF
        } else {
            shmaddr
        }
    };

    seg.nattch += 1;
    seg.atime = crate::time::uptime_ns() / 1_000_000_000;
    seg.lpid = crate::process::current_pid();

    addr as i64
}

/// shmdt — detach shared memory segment
pub fn shmdt(_shmaddr: u64) -> i32 {
    // Find segment by address and decrement nattch
    let segs = SHM_SEGS.read();
    for (_, seg_mutex) in segs.iter() {
        let mut seg = seg_mutex.lock();
        if seg.data.as_ptr() as u64 == _shmaddr {
            if seg.nattch > 0 {
                seg.nattch -= 1;
            }
            seg.dtime = crate::time::uptime_ns() / 1_000_000_000;
            seg.lpid = crate::process::current_pid();
            return 0;
        }
    }
    -22 // EINVAL
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
    let mtype = unsafe { *(msgp as *const i64) };
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
            *(msgp as *mut i64) = mtype;
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
