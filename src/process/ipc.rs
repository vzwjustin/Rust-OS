//! Inter-Process Communication (IPC) Mechanisms
//!
//! This module provides comprehensive IPC support including pipes, shared memory,
//! signals, message queues, and semaphores for RustOS processes.

use super::{get_process_manager, get_system_time, Pid};
use alloc::collections::{BTreeMap, VecDeque};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use spin::RwLock;
use x86_64::VirtAddr;

/// IPC object ID type
pub type IpcId = u32;

/// Maximum IPC objects per type
pub const MAX_IPC_OBJECTS: usize = 1024;

/// Pipe buffer size
pub const PIPE_BUFFER_SIZE: usize = 4096;

/// Maximum shared memory segment size (16MB)
pub const MAX_SHARED_MEMORY_SIZE: usize = 16 * 1024 * 1024;

/// Signal types (POSIX-compatible)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Signal {
    SIGHUP = 1,   // Hangup
    SIGINT = 2,   // Interrupt
    SIGQUIT = 3,  // Quit
    SIGILL = 4,   // Illegal instruction
    SIGTRAP = 5,  // Trace trap
    SIGABRT = 6,  // Abort
    SIGBUS = 7,   // Bus error
    SIGFPE = 8,   // Floating point exception
    SIGKILL = 9,  // Kill (cannot be caught)
    SIGUSR1 = 10, // User signal 1
    SIGSEGV = 11, // Segmentation violation
    SIGUSR2 = 12, // User signal 2
    SIGPIPE = 13, // Broken pipe
    SIGALRM = 14, // Alarm clock
    SIGTERM = 15, // Termination
    SIGCHLD = 17, // Child status changed
    SIGCONT = 18, // Continue
    SIGSTOP = 19, // Stop (cannot be caught)
    SIGTSTP = 20, // Terminal stop
}

/// Signal disposition
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalDisposition {
    /// Default action (terminate, ignore, etc.)
    Default,
    /// Ignore the signal
    Ignore,
    /// Call custom handler
    Handler(u64), // Function pointer
}

/// Signal information
#[derive(Debug, Clone)]
pub struct SignalInfo {
    pub signal: Signal,
    pub sender: Pid,
    pub timestamp: u64,
    pub data: u64, // Additional signal data
}

/// Process signal state
#[derive(Debug, Clone)]
pub struct ProcessSignalState {
    /// Signal handlers
    pub handlers: BTreeMap<Signal, SignalDisposition>,
    /// Pending signals
    pub pending: Vec<SignalInfo>,
    /// Signal mask (blocked signals)
    pub mask: u64,
}

impl Default for ProcessSignalState {
    fn default() -> Self {
        let mut handlers = BTreeMap::new();

        // Set default signal dispositions
        handlers.insert(Signal::SIGTERM, SignalDisposition::Default);
        handlers.insert(Signal::SIGKILL, SignalDisposition::Default);
        handlers.insert(Signal::SIGINT, SignalDisposition::Default);
        handlers.insert(Signal::SIGPIPE, SignalDisposition::Ignore);
        handlers.insert(Signal::SIGCHLD, SignalDisposition::Ignore);

        Self {
            handlers,
            pending: Vec::new(),
            mask: 0,
        }
    }
}

/// Pipe implementation
#[derive(Debug)]
pub struct Pipe {
    /// Pipe ID
    pub id: IpcId,
    /// Read end process
    pub reader: Option<Pid>,
    /// Write end process
    pub writer: Option<Pid>,
    /// Pipe buffer
    pub buffer: VecDeque<u8>,
    /// Maximum buffer size
    pub buffer_size: usize,
    /// Processes waiting to read
    pub read_waiters: Vec<Pid>,
    /// Processes waiting to write
    pub write_waiters: Vec<Pid>,
    /// Pipe is closed for reading
    pub read_closed: bool,
    /// Pipe is closed for writing
    pub write_closed: bool,
}

impl Pipe {
    pub fn new(id: IpcId, buffer_size: usize) -> Self {
        Self {
            id,
            reader: None,
            writer: None,
            buffer: VecDeque::new(),
            buffer_size,
            read_waiters: Vec::new(),
            write_waiters: Vec::new(),
            read_closed: false,
            write_closed: false,
        }
    }

    /// Read data from pipe
    pub fn read(&mut self, data: &mut [u8]) -> Result<usize, &'static str> {
        if self.read_closed {
            return Err("Pipe read end closed");
        }

        let mut bytes_read = 0;
        while bytes_read < data.len() && !self.buffer.is_empty() {
            if let Some(byte) = self.buffer.pop_front() {
                data[bytes_read] = byte;
                bytes_read += 1;
            }
        }

        Ok(bytes_read)
    }

    /// Write data to pipe
    pub fn write(&mut self, data: &[u8]) -> Result<usize, &'static str> {
        if self.write_closed {
            return Err("Pipe write end closed");
        }

        let mut bytes_written = 0;
        for &byte in data {
            if self.buffer.len() >= self.buffer_size {
                break; // Buffer full
            }
            self.buffer.push_back(byte);
            bytes_written += 1;
        }

        Ok(bytes_written)
    }

    /// Check if pipe has data available for reading
    pub fn has_data(&self) -> bool {
        !self.buffer.is_empty()
    }

    /// Check if pipe has space for writing
    pub fn has_space(&self) -> bool {
        self.buffer.len() < self.buffer_size
    }

    /// Close read end
    pub fn close_read(&mut self) {
        self.read_closed = true;
        self.reader = None;
    }

    /// Close write end
    pub fn close_write(&mut self) {
        self.write_closed = true;
        self.writer = None;
    }
}

/// Shared memory segment
#[derive(Debug)]
pub struct SharedMemorySegment {
    /// Segment ID
    pub id: IpcId,
    /// Virtual memory address returned by the memory manager
    pub virtual_addr: u64,
    /// Size in bytes
    pub size: usize,
    /// Access permissions
    pub permissions: SharedMemoryPermissions,
    /// Processes attached to this segment
    pub attached_processes: Vec<Pid>,
    /// Reference count
    pub ref_count: usize,
    /// Creation time
    pub created: u64,
    /// Last access time
    pub last_access: u64,
}

/// Shared memory permissions
#[derive(Debug, Clone, Copy)]
pub struct SharedMemoryPermissions {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
}

#[allow(non_upper_case_globals)]
impl SharedMemoryPermissions {
    /// Read and write permissions
    pub const ReadWrite: Self = Self {
        read: true,
        write: true,
        execute: false,
    };

    /// Read-only permissions
    pub const ReadOnly: Self = Self {
        read: true,
        write: false,
        execute: false,
    };

    /// Execute permissions (read and execute)
    pub const Execute: Self = Self {
        read: true,
        write: false,
        execute: true,
    };
}

impl Default for SharedMemoryPermissions {
    fn default() -> Self {
        Self::ReadWrite
    }
}

impl SharedMemorySegment {
    pub fn new(
        id: IpcId,
        size: usize,
        permissions: SharedMemoryPermissions,
    ) -> Result<Self, &'static str> {
        // Allocate physical memory for the segment
        use crate::memory::{get_memory_manager, MemoryProtection, MemoryRegionType};

        let memory_manager_guard = get_memory_manager().ok_or("Memory manager not initialized")?;
        let memory_manager = &*memory_manager_guard;

        let mut protection = MemoryProtection::empty();
        if permissions.read {
            protection |= MemoryProtection::READ;
        }
        if permissions.write {
            protection |= MemoryProtection::WRITE;
        }
        if permissions.execute {
            protection |= MemoryProtection::EXECUTE;
        }

        let region = memory_manager
            .allocate_region(size, MemoryRegionType::SharedMemory, protection)
            .map_err(|_| "Failed to allocate shared memory")?;

        Ok(Self {
            id,
            virtual_addr: region.start.as_u64(),
            size,
            permissions,
            attached_processes: Vec::new(),
            ref_count: 0,
            created: get_system_time(),
            last_access: get_system_time(),
        })
    }

    /// Attach process to shared memory segment
    pub fn attach(&mut self, pid: Pid) -> Result<VirtAddr, &'static str> {
        if !self.attached_processes.contains(&pid) {
            self.attached_processes.push(pid);
            self.ref_count += 1;
        }
        self.last_access = get_system_time();

        // The memory manager already mapped the region into the kernel
        // page tables when `new()` called `allocate_region`.  We return
        // the virtual address so the caller can access the shared pages.
        Ok(VirtAddr::new(self.virtual_addr))
    }

    /// Detach process from shared memory segment
    pub fn detach(&mut self, pid: Pid) -> Result<(), &'static str> {
        if let Some(pos) = self.attached_processes.iter().position(|&p| p == pid) {
            self.attached_processes.remove(pos);
            self.ref_count = self.ref_count.saturating_sub(1);
        }
        Ok(())
    }

    /// Check if segment can be deleted
    pub fn can_delete(&self) -> bool {
        self.ref_count == 0
    }
}

/// Message queue entry
#[derive(Debug, Clone)]
pub struct Message {
    /// Message type (for filtering)
    pub msg_type: u32,
    /// Message data
    pub data: Vec<u8>,
    /// Sender process ID
    pub sender: Pid,
    /// Timestamp
    pub timestamp: u64,
}

/// Message queue
#[derive(Debug)]
pub struct MessageQueue {
    /// Queue ID
    pub id: IpcId,
    /// Maximum queue size
    pub max_size: usize,
    /// Maximum message size
    pub max_msg_size: usize,
    /// Messages in queue
    pub messages: VecDeque<Message>,
    /// Processes waiting to send
    pub send_waiters: Vec<Pid>,
    /// Processes waiting to receive
    pub recv_waiters: Vec<(Pid, u32)>, // (PID, message type filter)
}

impl MessageQueue {
    pub fn new(id: IpcId, max_size: usize, max_msg_size: usize) -> Self {
        Self {
            id,
            max_size,
            max_msg_size,
            messages: VecDeque::new(),
            send_waiters: Vec::new(),
            recv_waiters: Vec::new(),
        }
    }

    /// Send message to queue
    pub fn send(&mut self, message: Message) -> Result<(), &'static str> {
        if message.data.len() > self.max_msg_size {
            return Err("Message too large");
        }

        if self.messages.len() >= self.max_size {
            return Err("Queue full");
        }

        self.messages.push_back(message);
        Ok(())
    }

    /// Receive message from queue
    pub fn receive(&mut self, msg_type: u32) -> Option<Message> {
        // Find message with matching type (0 = any type)
        if msg_type == 0 {
            self.messages.pop_front()
        } else {
            if let Some(pos) = self.messages.iter().position(|m| m.msg_type == msg_type) {
                self.messages.remove(pos)
            } else {
                None
            }
        }
    }

    /// Check if queue has messages
    pub fn has_messages(&self, msg_type: u32) -> bool {
        if msg_type == 0 {
            !self.messages.is_empty()
        } else {
            self.messages.iter().any(|m| m.msg_type == msg_type)
        }
    }

    /// Check if queue has space
    pub fn has_space(&self) -> bool {
        self.messages.len() < self.max_size
    }
}

/// IPC Manager - central coordinator for all IPC operations
pub struct IpcManager {
    /// Pipes
    pipes: RwLock<BTreeMap<IpcId, Pipe>>,
    /// Shared memory segments
    shared_memory: RwLock<BTreeMap<IpcId, SharedMemorySegment>>,
    /// Message queues
    message_queues: RwLock<BTreeMap<IpcId, MessageQueue>>,
    /// Process signal states
    signal_states: RwLock<BTreeMap<Pid, ProcessSignalState>>,
    /// Next IPC ID
    next_id: AtomicU32,
    /// IPC object counts
    pipe_count: AtomicUsize,
    shm_count: AtomicUsize,
    msgq_count: AtomicUsize,
}

impl IpcManager {
    pub const fn new() -> Self {
        Self {
            pipes: RwLock::new(BTreeMap::new()),
            shared_memory: RwLock::new(BTreeMap::new()),
            message_queues: RwLock::new(BTreeMap::new()),
            signal_states: RwLock::new(BTreeMap::new()),
            next_id: AtomicU32::new(1),
            pipe_count: AtomicUsize::new(0),
            shm_count: AtomicUsize::new(0),
            msgq_count: AtomicUsize::new(0),
        }
    }

    /// Initialize IPC manager
    pub fn init(&self) -> Result<(), &'static str> {
        // Initialize signal states for existing processes
        let process_manager = get_process_manager();
        let processes = process_manager.list_processes();

        let mut signal_states = self.signal_states.write();
        for (pid, _, _, _) in processes {
            signal_states.insert(pid, ProcessSignalState::default());
        }

        Ok(())
    }

    /// Create a new pipe
    pub fn create_pipe(&self) -> Result<(IpcId, IpcId), &'static str> {
        if self.pipe_count.load(Ordering::SeqCst) >= MAX_IPC_OBJECTS {
            return Err("Maximum pipe count exceeded");
        }

        let pipe_id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let pipe = Pipe::new(pipe_id, PIPE_BUFFER_SIZE);

        {
            let mut pipes = self.pipes.write();
            pipes.insert(pipe_id, pipe);
        }

        self.pipe_count.fetch_add(1, Ordering::SeqCst);

        // Return read and write file descriptor IDs
        Ok((pipe_id, pipe_id))
    }

    /// Read from pipe
    pub fn pipe_read(&self, pipe_id: IpcId, data: &mut [u8]) -> Result<usize, &'static str> {
        let mut pipes = self.pipes.write();
        if let Some(pipe) = pipes.get_mut(&pipe_id) {
            pipe.read(data)
        } else {
            Err("Pipe not found")
        }
    }

    /// Write to pipe
    pub fn pipe_write(&self, pipe_id: IpcId, data: &[u8]) -> Result<usize, &'static str> {
        let mut pipes = self.pipes.write();
        if let Some(pipe) = pipes.get_mut(&pipe_id) {
            pipe.write(data)
        } else {
            Err("Pipe not found")
        }
    }

    /// Returns true when the pipe has buffered data to read.
    pub fn pipe_has_data(&self, pipe_id: IpcId) -> bool {
        self.pipes
            .read()
            .get(&pipe_id)
            .map(|pipe| pipe.has_data())
            .unwrap_or(false)
    }

    /// Returns true when the pipe can accept more writes.
    pub fn pipe_has_space(&self, pipe_id: IpcId) -> bool {
        self.pipes
            .read()
            .get(&pipe_id)
            .map(|pipe| pipe.has_space())
            .unwrap_or(false)
    }

    /// Close pipe
    pub fn close_pipe(
        &self,
        pipe_id: IpcId,
        close_read: bool,
        close_write: bool,
    ) -> Result<(), &'static str> {
        let mut pipes = self.pipes.write();
        if let Some(pipe) = pipes.get_mut(&pipe_id) {
            if close_read {
                pipe.close_read();
            }
            if close_write {
                pipe.close_write();
            }

            // Remove pipe if both ends are closed
            if pipe.read_closed && pipe.write_closed {
                pipes.remove(&pipe_id);
                self.pipe_count.fetch_sub(1, Ordering::SeqCst);
            }

            Ok(())
        } else {
            Err("Pipe not found")
        }
    }

    /// Create shared memory segment
    pub fn create_shared_memory(
        &self,
        size: usize,
        permissions: SharedMemoryPermissions,
    ) -> Result<IpcId, &'static str> {
        if size > MAX_SHARED_MEMORY_SIZE {
            return Err("Shared memory segment too large");
        }

        if self.shm_count.load(Ordering::SeqCst) >= MAX_IPC_OBJECTS {
            return Err("Maximum shared memory count exceeded");
        }

        let shm_id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let segment = SharedMemorySegment::new(shm_id, size, permissions)?;

        {
            let mut shared_memory = self.shared_memory.write();
            shared_memory.insert(shm_id, segment);
        }

        self.shm_count.fetch_add(1, Ordering::SeqCst);
        Ok(shm_id)
    }

    /// Attach to shared memory segment
    pub fn attach_shared_memory(&self, shm_id: IpcId, pid: Pid) -> Result<VirtAddr, &'static str> {
        let mut shared_memory = self.shared_memory.write();
        if let Some(segment) = shared_memory.get_mut(&shm_id) {
            segment.attach(pid)
        } else {
            Err("Shared memory segment not found")
        }
    }

    /// Detach from shared memory segment
    pub fn detach_shared_memory(&self, shm_id: IpcId, pid: Pid) -> Result<(), &'static str> {
        let mut shared_memory = self.shared_memory.write();
        if let Some(segment) = shared_memory.get_mut(&shm_id) {
            segment.detach(pid)?;

            // Remove segment if no longer in use
            if segment.can_delete() {
                shared_memory.remove(&shm_id);
                self.shm_count.fetch_sub(1, Ordering::SeqCst);
            }

            Ok(())
        } else {
            Err("Shared memory segment not found")
        }
    }

    /// Create message queue
    pub fn create_message_queue(
        &self,
        max_size: usize,
        max_msg_size: usize,
    ) -> Result<IpcId, &'static str> {
        if self.msgq_count.load(Ordering::SeqCst) >= MAX_IPC_OBJECTS {
            return Err("Maximum message queue count exceeded");
        }

        let msgq_id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let queue = MessageQueue::new(msgq_id, max_size, max_msg_size);

        {
            let mut message_queues = self.message_queues.write();
            message_queues.insert(msgq_id, queue);
        }

        self.msgq_count.fetch_add(1, Ordering::SeqCst);
        Ok(msgq_id)
    }

    /// Send message to queue
    pub fn send_message(
        &self,
        msgq_id: IpcId,
        msg_type: u32,
        data: Vec<u8>,
        sender: Pid,
    ) -> Result<(), &'static str> {
        let message = Message {
            msg_type,
            data,
            sender,
            timestamp: get_system_time(),
        };

        let mut message_queues = self.message_queues.write();
        if let Some(queue) = message_queues.get_mut(&msgq_id) {
            queue.send(message)
        } else {
            Err("Message queue not found")
        }
    }

    /// Receive message from queue
    pub fn receive_message(
        &self,
        msgq_id: IpcId,
        msg_type: u32,
    ) -> Result<Option<Message>, &'static str> {
        let mut message_queues = self.message_queues.write();
        if let Some(queue) = message_queues.get_mut(&msgq_id) {
            Ok(queue.receive(msg_type))
        } else {
            Err("Message queue not found")
        }
    }

    /// Send signal to process
    pub fn send_signal(
        &self,
        target_pid: Pid,
        signal: Signal,
        sender_pid: Pid,
    ) -> Result<(), &'static str> {
        let signal_info = SignalInfo {
            signal,
            sender: sender_pid,
            timestamp: get_system_time(),
            data: 0,
        };

        let mut signal_states = self.signal_states.write();
        if let Some(state) = signal_states.get_mut(&target_pid) {
            // Check if signal is blocked
            let signal_bit = 1u64 << (signal as u8);
            if state.mask & signal_bit == 0 {
                state.pending.push(signal_info);
            }
            Ok(())
        } else {
            Err("Target process not found")
        }
    }

    /// Set signal handler
    pub fn set_signal_handler(
        &self,
        pid: Pid,
        signal: Signal,
        disposition: SignalDisposition,
    ) -> Result<(), &'static str> {
        let mut signal_states = self.signal_states.write();
        if let Some(state) = signal_states.get_mut(&pid) {
            state.handlers.insert(signal, disposition);
            Ok(())
        } else {
            Err("Process not found")
        }
    }

    /// Set signal mask
    pub fn set_signal_mask(&self, pid: Pid, mask: u64) -> Result<u64, &'static str> {
        let mut signal_states = self.signal_states.write();
        if let Some(state) = signal_states.get_mut(&pid) {
            let old_mask = state.mask;
            state.mask = mask;
            Ok(old_mask)
        } else {
            Err("Process not found")
        }
    }

    /// Get pending signals for process
    pub fn get_pending_signals(&self, pid: Pid) -> Vec<SignalInfo> {
        let mut signal_states = self.signal_states.write();
        if let Some(state) = signal_states.get_mut(&pid) {
            let pending = state.pending.clone();
            state.pending.clear();
            pending
        } else {
            Vec::new()
        }
    }

    /// Initialize signal state for new process
    pub fn init_process_signals(&self, pid: Pid) -> Result<(), &'static str> {
        let mut signal_states = self.signal_states.write();
        signal_states.insert(pid, ProcessSignalState::default());
        Ok(())
    }

    /// Cleanup IPC resources for terminated process
    pub fn cleanup_process_ipc(&self, pid: Pid) -> Result<(), &'static str> {
        // Remove signal state
        {
            let mut signal_states = self.signal_states.write();
            signal_states.remove(&pid);
        }

        // Detach from all shared memory segments
        {
            let mut shared_memory = self.shared_memory.write();
            let segments_to_remove: Vec<IpcId> = shared_memory
                .iter_mut()
                .filter_map(|(id, segment)| {
                    let _ = segment.detach(pid);
                    if segment.can_delete() {
                        Some(*id)
                    } else {
                        None
                    }
                })
                .collect();

            for id in segments_to_remove {
                shared_memory.remove(&id);
                self.shm_count.fetch_sub(1, Ordering::SeqCst);
            }
        }

        // Close pipes owned by this process
        {
            let mut pipes = self.pipes.write();
            let pipes_to_remove: Vec<IpcId> = pipes
                .iter_mut()
                .filter_map(|(id, pipe)| {
                    if pipe.reader == Some(pid) {
                        pipe.close_read();
                    }
                    if pipe.writer == Some(pid) {
                        pipe.close_write();
                    }

                    if pipe.read_closed && pipe.write_closed {
                        Some(*id)
                    } else {
                        None
                    }
                })
                .collect();

            for id in pipes_to_remove {
                pipes.remove(&id);
                self.pipe_count.fetch_sub(1, Ordering::SeqCst);
            }
        }

        Ok(())
    }

    /// Get IPC statistics
    pub fn get_stats(&self) -> IpcStats {
        IpcStats {
            pipe_count: self.pipe_count.load(Ordering::SeqCst),
            shm_count: self.shm_count.load(Ordering::SeqCst),
            msgq_count: self.msgq_count.load(Ordering::SeqCst),
            signal_count: self.signal_states.read().len(),
        }
    }
}

/// IPC statistics
#[derive(Debug, Clone)]
pub struct IpcStats {
    pub pipe_count: usize,
    pub shm_count: usize,
    pub msgq_count: usize,
    pub signal_count: usize,
}

/// Global IPC manager instance
static IPC_MANAGER: IpcManager = IpcManager::new();

/// Get the global IPC manager
pub fn get_ipc_manager() -> &'static IpcManager {
    &IPC_MANAGER
}

/// Initialize the IPC system
pub fn init() -> Result<(), &'static str> {
    IPC_MANAGER.init()
}

/// Create a pipe (returns read and write file descriptor IDs)
pub fn create_pipe() -> Result<(IpcId, IpcId), &'static str> {
    IPC_MANAGER.create_pipe()
}

/// Create shared memory segment
pub fn create_shared_memory(
    size: usize,
    permissions: SharedMemoryPermissions,
) -> Result<IpcId, &'static str> {
    IPC_MANAGER.create_shared_memory(size, permissions)
}

/// Send signal to process
pub fn send_signal(target_pid: Pid, signal: Signal, sender_pid: Pid) -> Result<(), &'static str> {
    IPC_MANAGER.send_signal(target_pid, signal, sender_pid)
}

/// Set signal handler for process
pub fn set_signal_handler(
    pid: Pid,
    signal: Signal,
    disposition: SignalDisposition,
) -> Result<(), &'static str> {
    IPC_MANAGER.set_signal_handler(pid, signal, disposition)
}
