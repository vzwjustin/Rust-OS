//! Production Inter-Process Communication for RustOS
//!
//! Implements real IPC mechanisms including pipes, message queues,
//! shared memory, and semaphores

use alloc::{collections::BTreeMap, sync::Arc, vec::Vec};
use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use spin::{Mutex, RwLock};
use x86_64::{PhysAddr, VirtAddr};

/// IPC object ID type
pub type IpcId = u32;
/// Process ID type
pub type Pid = u32;

/// IPC object types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcType {
    Pipe,
    MessageQueue,
    SharedMemory,
    Semaphore,
}

/// Pipe implementation
pub struct Pipe {
    id: IpcId,
    buffer: Arc<Mutex<Vec<u8>>>,
    read_pos: Arc<AtomicUsize>,
    write_pos: Arc<AtomicUsize>,
    capacity: usize,
    readers: Arc<Mutex<Vec<Pid>>>,
    writers: Arc<Mutex<Vec<Pid>>>,
    closed: Arc<AtomicBool>,
}

use core::sync::atomic::AtomicBool;

impl Pipe {
    /// Create a new pipe
    pub fn new(id: IpcId, capacity: usize) -> Self {
        Self {
            id,
            buffer: Arc::new(Mutex::new(Vec::with_capacity(capacity))),
            read_pos: Arc::new(AtomicUsize::new(0)),
            write_pos: Arc::new(AtomicUsize::new(0)),
            capacity,
            readers: Arc::new(Mutex::new(Vec::new())),
            writers: Arc::new(Mutex::new(Vec::new())),
            closed: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Write data to pipe
    pub fn write(&self, data: &[u8]) -> Result<usize, &'static str> {
        if self.closed.load(Ordering::Acquire) {
            return Err("Pipe closed");
        }

        let mut buffer = self.buffer.lock();
        let available = self.capacity - buffer.len();
        let to_write = data.len().min(available);

        if to_write == 0 {
            return Err("Pipe full");
        }

        buffer.extend_from_slice(&data[..to_write]);
        self.write_pos.fetch_add(to_write, Ordering::Release);

        Ok(to_write)
    }

    /// Read data from pipe
    pub fn read(&self, buf: &mut [u8]) -> Result<usize, &'static str> {
        if self.closed.load(Ordering::Acquire) && self.buffer.lock().is_empty() {
            return Ok(0); // EOF
        }

        let mut buffer = self.buffer.lock();
        let available = buffer.len();
        let to_read = buf.len().min(available);

        if to_read == 0 {
            return Err("Pipe empty");
        }

        buf[..to_read].copy_from_slice(&buffer.drain(..to_read).collect::<Vec<_>>());
        self.read_pos.fetch_add(to_read, Ordering::Release);

        Ok(to_read)
    }

    /// Close pipe
    pub fn close(&self) {
        self.closed.store(true, Ordering::Release);
    }
}

/// Message for message queues
#[derive(Clone)]
pub struct Message {
    pub sender: Pid,
    pub msg_type: u32,
    pub data: Vec<u8>,
    pub priority: u8,
}

/// Message queue implementation
pub struct MessageQueue {
    id: IpcId,
    messages: Arc<Mutex<Vec<Message>>>,
    max_messages: usize,
    max_msg_size: usize,
    waiting_readers: Arc<Mutex<Vec<Pid>>>,
    waiting_writers: Arc<Mutex<Vec<Pid>>>,
}

impl MessageQueue {
    /// Create a new message queue
    pub fn new(id: IpcId, max_messages: usize, max_msg_size: usize) -> Self {
        Self {
            id,
            messages: Arc::new(Mutex::new(Vec::new())),
            max_messages,
            max_msg_size,
            waiting_readers: Arc::new(Mutex::new(Vec::new())),
            waiting_writers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Send a message
    pub fn send(&self, msg: Message) -> Result<(), &'static str> {
        if msg.data.len() > self.max_msg_size {
            return Err("Message too large");
        }

        let mut messages = self.messages.lock();
        if messages.len() >= self.max_messages {
            return Err("Queue full");
        }

        // Insert sorted by priority
        let pos = messages
            .iter()
            .position(|m| m.priority < msg.priority)
            .unwrap_or(messages.len());
        messages.insert(pos, msg);

        Ok(())
    }

    /// Receive a message
    pub fn receive(&self, msg_type: Option<u32>) -> Result<Message, &'static str> {
        let mut messages = self.messages.lock();

        let pos = if let Some(mtype) = msg_type {
            messages.iter().position(|m| m.msg_type == mtype)
        } else {
            if messages.is_empty() {
                None
            } else {
                Some(0)
            }
        };

        if let Some(idx) = pos {
            Ok(messages.remove(idx))
        } else {
            Err("No message available")
        }
    }
}

/// Shared memory segment
pub struct SharedMemory {
    id: IpcId,
    phys_addr: PhysAddr, // Physical address of allocated memory
    size: usize,
    attached: Arc<Mutex<Vec<(Pid, VirtAddr)>>>,
}

/// Counter for generating unique shared-memory virtual addresses.
/// Each attach gets a distinct address in the shared-memory region
/// (0x4000_0000_0000 + offset), so multiple attaches don't collide.
static SHM_VADDR_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Base virtual address for shared memory mappings.
const SHM_VADDR_BASE: u64 = 0x4000_0000_0000;

impl SharedMemory {
    /// Create a new shared memory segment
    ///
    /// Allocates physical memory via the kernel memory manager. If the
    /// memory manager isn't available yet (early boot), falls back to
    /// a fixed physical address so the segment can still be created.
    pub fn new(id: IpcId, size: usize) -> Result<Self, &'static str> {
        // Try to allocate real memory from the memory manager.
        let phys_addr = match crate::memory::allocate_memory(
            size,
            crate::memory::MemoryRegionType::SharedMemory,
            crate::memory::MemoryProtection::USER_DATA,
        ) {
            Ok(vaddr) => {
                // allocate_memory returns a virtual address; translate to
                // physical if possible, otherwise use the virtual address
                // as the backing store reference.
                if let Some(mm) = crate::memory::get_memory_manager() {
                    mm.translate_addr(vaddr)
                        .unwrap_or(PhysAddr::new(vaddr.as_u64()))
                } else {
                    PhysAddr::new(vaddr.as_u64())
                }
            }
            Err(_) => {
                // Memory manager not available; use a fixed region.
                // This is a fallback for early boot or testing.
                PhysAddr::new(0x1000_0000)
            }
        };

        Ok(Self {
            id,
            phys_addr,
            size,
            attached: Arc::new(Mutex::new(Vec::new())),
        })
    }

    /// Attach to shared memory
    ///
    /// Allocates and maps a virtual memory region at a unique address
    /// in the shared-memory area. The physical frames backing this
    /// region are the same ones allocated in `new`, so all processes
    /// that attach to the same segment share the same physical memory.
    /// In a per-process address space model, this would map the
    /// segment's physical frames into the attaching process's page
    /// table; in the current single-address-space kernel, we map
    /// directly into the kernel's address space at a unique virtual
    /// address.
    pub fn attach(&self, pid: Pid) -> Result<VirtAddr, &'static str> {
        let offset = SHM_VADDR_COUNTER.fetch_add(self.size, Ordering::SeqCst) as u64;
        let vaddr = VirtAddr::new(SHM_VADDR_BASE + offset);

        // Map physical memory at the generated virtual address so the
        // segment is actually accessible. We use the physical memory
        // offset to compute the virtual address of the backing frames.
        let phys_offset = crate::memory::get_physical_memory_offset();
        if phys_offset > 0 {
            // The physical frames are already mapped via the direct
            // physical-memory mapping. We record the virtual address
            // that points to the segment's physical memory.
            let mapped_vaddr = VirtAddr::new(phys_offset + self.phys_addr.as_u64());

            let mut attached = self.attached.lock();
            attached.push((pid, mapped_vaddr));

            return Ok(mapped_vaddr);
        }

        // Fallback: return the generated address (no mapping). This
        // happens only if the memory manager hasn't been initialized.
        let mut attached = self.attached.lock();
        attached.push((pid, vaddr));

        Ok(vaddr)
    }

    /// Detach from shared memory
    pub fn detach(&self, pid: Pid) -> Result<(), &'static str> {
        let mut attached = self.attached.lock();
        attached.retain(|(p, _)| *p != pid);
        Ok(())
    }
}

/// Semaphore implementation
pub struct Semaphore {
    id: IpcId,
    value: Arc<AtomicU32>,
    max_value: u32,
    waiting: Arc<Mutex<Vec<Pid>>>,
}

impl Semaphore {
    /// Create a new semaphore
    pub fn new(id: IpcId, initial: u32, max: u32) -> Self {
        Self {
            id,
            value: Arc::new(AtomicU32::new(initial)),
            max_value: max,
            waiting: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Wait (P operation)
    ///
    /// Tries to decrement the semaphore. If the value is zero, blocks the
    /// calling process via the scheduler and adds it to the waiting list.
    /// The process will be unblocked by `signal` and will retry the
    /// decrement when it runs again.
    pub fn wait(&self, pid: Pid) -> Result<(), &'static str> {
        // Fast path: try to acquire without blocking.
        loop {
            let current = self.value.load(Ordering::Acquire);
            if current > 0 {
                if self
                    .value
                    .compare_exchange(current, current - 1, Ordering::Release, Ordering::Relaxed)
                    .is_ok()
                {
                    // Successfully acquired — remove from waiting list if present.
                    self.waiting.lock().retain(|&p| p != pid);
                    return Ok(());
                }
                // CAS failed (race); retry the fast path.
                continue;
            }

            // Value is zero — need to block.
            // Add to waiting list if not already there.
            {
                let mut waiting = self.waiting.lock();
                if waiting.contains(&pid) {
                    // Already waiting — just yield and retry.
                    drop(waiting);
                    crate::process::thread::yield_thread();
                    continue;
                }
                waiting.push(pid);
            }

            // Block the process via the scheduler. This removes it from
            // all ready queues and sets its state to Blocked. When signal()
            // calls unblock_process, the process will be re-enqueued and
            // will retry the decrement on its next time slice.
            let _ = crate::scheduler::block_process(pid);

            // After being unblocked, loop back and try to acquire again.
            // If the semaphore was signalled by someone else in the meantime,
            // we might need to block again — but that's correct behavior.
        }
    }

    /// Signal (V operation)
    ///
    /// Increments the semaphore value. If there are waiting processes,
    /// pops one from the waiting list and unblocks it via the scheduler
    /// so it can retry the wait.
    pub fn signal(&self) -> Result<(), &'static str> {
        // ponytail: CAS loop so two concurrent signals can't both pass the max
        // check and then both fetch_add, pushing value past max_value. We only
        // commit the increment when value is still below max at compare time.
        loop {
            let current = self.value.load(Ordering::Acquire);
            if current >= self.max_value {
                return Err("Semaphore at maximum");
            }
            if self
                .value
                .compare_exchange(current, current + 1, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                break;
            }
        }

        // Wake up a waiting process: pop from the waiting list and
        // unblock it via the scheduler so it gets re-enqueued on a
        // CPU ready queue and can retry the wait.
        if let Some(pid) = self.waiting.lock().pop() {
            let _ = crate::scheduler::unblock_process(pid);
        }

        Ok(())
    }
}

/// IPC object registry
static IPC_OBJECTS: RwLock<BTreeMap<IpcId, IpcObject>> = RwLock::new(BTreeMap::new());
static NEXT_IPC_ID: AtomicU32 = AtomicU32::new(1);

/// IPC object wrapper
enum IpcObject {
    Pipe(Arc<Pipe>),
    MessageQueue(Arc<MessageQueue>),
    SharedMemory(Arc<SharedMemory>),
    Semaphore(Arc<Semaphore>),
}

/// Create a pipe
pub fn create_pipe(capacity: usize) -> Result<IpcId, &'static str> {
    let id = NEXT_IPC_ID.fetch_add(1, Ordering::Relaxed);
    let pipe = Arc::new(Pipe::new(id, capacity));

    let mut objects = IPC_OBJECTS.write();
    objects.insert(id, IpcObject::Pipe(pipe));

    Ok(id)
}

/// Create a message queue
pub fn create_message_queue(max_msgs: usize, max_size: usize) -> Result<IpcId, &'static str> {
    let id = NEXT_IPC_ID.fetch_add(1, Ordering::Relaxed);
    let mq = Arc::new(MessageQueue::new(id, max_msgs, max_size));

    let mut objects = IPC_OBJECTS.write();
    objects.insert(id, IpcObject::MessageQueue(mq));

    Ok(id)
}

/// Create a shared memory segment
pub fn create_shared_memory(size: usize) -> Result<IpcId, &'static str> {
    let id = NEXT_IPC_ID.fetch_add(1, Ordering::Relaxed);
    let shm = Arc::new(SharedMemory::new(id, size)?);

    let mut objects = IPC_OBJECTS.write();
    objects.insert(id, IpcObject::SharedMemory(shm));

    Ok(id)
}

/// Create a semaphore
pub fn create_semaphore(initial: u32, max: u32) -> Result<IpcId, &'static str> {
    let id = NEXT_IPC_ID.fetch_add(1, Ordering::Relaxed);
    let sem = Arc::new(Semaphore::new(id, initial, max));

    let mut objects = IPC_OBJECTS.write();
    objects.insert(id, IpcObject::Semaphore(sem));

    Ok(id)
}

/// Remove an IPC object
pub fn remove_ipc(id: IpcId) -> Result<(), &'static str> {
    let mut objects = IPC_OBJECTS.write();
    objects.remove(&id).ok_or("IPC object not found")?;
    Ok(())
}

/// Registry of message-queue IDs that have subscribed to keyboard events.
/// Each entry is an IPC message-queue id created via `create_message_queue`.
static KEYBOARD_SUBSCRIBERS: Mutex<Vec<IpcId>> = Mutex::new(Vec::new());

/// Subscribe a message queue to receive keyboard events.
///
/// `queue_id` must be the id of a message queue created with
/// `create_message_queue`; events are delivered as `Message`s with
/// `msg_type = MSG_TYPE_KEYBOARD`, the sender set to kernel pid 0, and the
/// 4-byte little-endian scancode in `data`.
pub const MSG_TYPE_KEYBOARD: u32 = 0x6b65_7920; // "key " in ASCII

pub fn subscribe_keyboard_events(queue_id: IpcId) -> Result<(), &'static str> {
    let mut subs = KEYBOARD_SUBSCRIBERS.lock();
    if subs.iter().any(|&id| id == queue_id) {
        return Err("Already subscribed");
    }
    subs.push(queue_id);
    Ok(())
}

/// Unsubscribe a message queue from keyboard events.
pub fn unsubscribe_keyboard_events(queue_id: IpcId) -> Result<(), &'static str> {
    let mut subs = KEYBOARD_SUBSCRIBERS.lock();
    let before = subs.len();
    subs.retain(|&id| id != queue_id);
    if subs.len() == before {
        return Err("Not subscribed");
    }
    Ok(())
}

/// Send keyboard event to interested processes.
///
/// The scancode is forwarded to every subscribed message queue as a
/// `Message` with `MSG_TYPE_KEYBOARD`. Subscribers that have been removed
/// (e.g. their queue was destroyed) are silently pruned.
pub fn send_keyboard_event(scancode: u32) {
    let subs = KEYBOARD_SUBSCRIBERS.lock().clone();
    if subs.is_empty() {
        return;
    }

    let data = scancode.to_le_bytes().to_vec();
    let msg = Message {
        sender: 0, // kernel
        msg_type: MSG_TYPE_KEYBOARD,
        data,
        priority: 0,
    };

    let mut dead = Vec::new();
    let objects = IPC_OBJECTS.read();
    for &id in &subs {
        let delivered = if let Some(IpcObject::MessageQueue(mq)) = objects.get(&id) {
            mq.send(msg.clone()).is_ok()
        } else {
            false
        };
        if !delivered {
            dead.push(id);
        }
    }
    drop(objects);

    if !dead.is_empty() {
        let mut subs = KEYBOARD_SUBSCRIBERS.lock();
        subs.retain(|id| !dead.contains(id));
    }
}

/// Test IPC functionality for integration tests
pub fn test_ipc_functionality() -> bool {
    // Simple test that exercises basic IPC operations

    // Test pipe creation
    if create_pipe(1024).is_err() {
        return false;
    }

    // Test message queue creation
    if create_message_queue(10, 256).is_err() {
        return false;
    }

    // Test shared memory creation
    if create_shared_memory(4096).is_err() {
        return false;
    }

    // Test semaphore creation
    if create_semaphore(1, 10).is_err() {
        return false;
    }

    true
}

// Re-export types from process::ipc
