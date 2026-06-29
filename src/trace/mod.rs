//! ftrace / tracepoints framework with function tracer ring buffer.
//!
//! Tracepoints can be registered, enabled, and disabled. Function traces are
//! stored in a circular buffer. Userspace visibility is via `/sys/kernel/tracing/`.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use spin::Mutex;

use crate::vfs::{DirEntry, InodeOps, InodeType, Stat, VfsError, VfsResult};

pub const TRACE_RING_CAPACITY: usize = 1024;
pub const MAX_TRACEPOINTS: usize = 256;

#[derive(Debug, Clone)]
pub struct TraceEntry {
    pub timestamp_ns: u64,
    pub cpu: u32,
    pub pid: u32,
    pub function: String,
    pub ip: u64,
    pub data: u64,
}

#[derive(Debug, Clone)]
pub struct Tracepoint {
    pub id: u32,
    pub name: String,
    pub enabled: bool,
    pub hit_count: u64,
}

struct TraceState {
    tracepoints: BTreeMap<u32, Tracepoint>,
    name_to_id: BTreeMap<String, u32>,
    ring: Vec<TraceEntry>,
    ring_head: usize,
    ring_count: usize,
    function_tracer_enabled: bool,
    tracing_on: bool,
    next_id: u32,
}

static TRACING_ON: AtomicBool = AtomicBool::new(false);
static FUNCTION_TRACER: AtomicBool = AtomicBool::new(false);
static TRACE_HITS: AtomicU64 = AtomicU64::new(0);
static NEXT_TRACEPOINT_ID: AtomicU32 = AtomicU32::new(1);

static STATE: Mutex<TraceState> = Mutex::new(TraceState {
    tracepoints: BTreeMap::new(),
    name_to_id: BTreeMap::new(),
    ring: Vec::new(),
    ring_head: 0,
    ring_count: 0,
    function_tracer_enabled: false,
    tracing_on: false,
    next_id: 1,
});

fn push_ring(entry: TraceEntry) {
    let mut state = STATE.lock();
    if state.ring.len() < TRACE_RING_CAPACITY {
        state.ring.push(entry);
        state.ring_count = state.ring.len();
        return;
    }
    let head = state.ring_head;
    state.ring[head] = entry;
    state.ring_head = (head + 1) % TRACE_RING_CAPACITY;
    state.ring_count = TRACE_RING_CAPACITY;
}

/// Register a named tracepoint. Returns tracepoint id.
pub fn register_tracepoint(name: &str) -> u32 {
    let mut state = STATE.lock();
    if let Some(&id) = state.name_to_id.get(name) {
        return id;
    }
    if state.tracepoints.len() >= MAX_TRACEPOINTS {
        return 0;
    }
    let id = state.next_id;
    state.next_id = state.next_id.saturating_add(1);
    state.name_to_id.insert(name.to_string(), id);
    state.tracepoints.insert(
        id,
        Tracepoint {
            id,
            name: name.to_string(),
            enabled: false,
            hit_count: 0,
        },
    );
    id
}

pub fn enable_tracepoint(name: &str) -> bool {
    let mut state = STATE.lock();
    let id = match state.name_to_id.get(name) {
        Some(id) => *id,
        None => return false,
    };
    if let Some(tp) = state.tracepoints.get_mut(&id) {
        tp.enabled = true;
        true
    } else {
        false
    }
}

pub fn disable_tracepoint(name: &str) -> bool {
    let mut state = STATE.lock();
    let id = match state.name_to_id.get(name) {
        Some(id) => *id,
        None => return false,
    };
    if let Some(tp) = state.tracepoints.get_mut(&id) {
        tp.enabled = false;
        true
    } else {
        false
    }
}

pub fn enable_tracepoint_by_id(id: u32) -> bool {
    let mut state = STATE.lock();
    if let Some(tp) = state.tracepoints.get_mut(&id) {
        tp.enabled = true;
        true
    } else {
        false
    }
}

pub fn disable_tracepoint_by_id(id: u32) -> bool {
    let mut state = STATE.lock();
    if let Some(tp) = state.tracepoints.get_mut(&id) {
        tp.enabled = false;
        true
    } else {
        false
    }
}

pub fn is_tracepoint_enabled(name: &str) -> bool {
    let state = STATE.lock();
    state
        .name_to_id
        .get(name)
        .and_then(|id| state.tracepoints.get(id))
        .map(|tp| tp.enabled)
        .unwrap_or(false)
}

/// Emit a tracepoint hit if the named tracepoint is enabled.
pub fn tracepoint_emit(name: &str, data: u64) {
    if !TRACING_ON.load(Ordering::Acquire) {
        return;
    }

    let mut enabled = false;
    {
        let mut state = STATE.lock();
        if let Some(id) = state.name_to_id.get(name).copied() {
            if let Some(tp) = state.tracepoints.get_mut(&id) {
                if tp.enabled {
                    tp.hit_count += 1;
                    enabled = true;
                }
            }
        }
    }

    if !enabled {
        return;
    }

    TRACE_HITS.fetch_add(1, Ordering::Relaxed);
    record_function_trace(name, 0, data);
}

/// Record a function trace entry in the ring buffer when the function tracer is on.
pub fn record_function_trace(function: &str, ip: u64, data: u64) {
    if !FUNCTION_TRACER.load(Ordering::Acquire) && !TRACING_ON.load(Ordering::Acquire) {
        return;
    }

    let entry = TraceEntry {
        timestamp_ns: crate::time::uptime_ns(),
        cpu: crate::smp::current_cpu(),
        pid: crate::process::current_pid(),
        function: function.to_string(),
        ip,
        data,
    };
    push_ring(entry);
}

pub fn set_tracing_on(on: bool) {
    TRACING_ON.store(on, Ordering::Release);
    STATE.lock().tracing_on = on;
}

pub fn set_function_tracer_enabled(on: bool) {
    FUNCTION_TRACER.store(on, Ordering::Release);
    STATE.lock().function_tracer_enabled = on;
}

pub fn tracing_on() -> bool {
    TRACING_ON.load(Ordering::Acquire)
}

pub fn function_tracer_enabled() -> bool {
    FUNCTION_TRACER.load(Ordering::Acquire)
}

pub fn tracepoint_count() -> usize {
    STATE.lock().tracepoints.len()
}

pub fn ring_snapshot(limit: usize) -> Vec<TraceEntry> {
    let state = STATE.lock();
    if state.ring.is_empty() {
        return Vec::new();
    }
    let total = state.ring.len();
    let take = core::cmp::min(limit, total);
    let mut out = Vec::with_capacity(take);
    if total < TRACE_RING_CAPACITY {
        let start = total.saturating_sub(take);
        out.extend(state.ring[start..].iter().cloned());
    } else {
        for i in 0..take {
            let idx = (state.ring_head + total - take + i) % total;
            out.push(state.ring[idx].clone());
        }
    }
    out
}

pub fn tracepoint_list_text() -> String {
    let state = STATE.lock();
    let mut out = String::new();
    for tp in state.tracepoints.values() {
        out.push_str(&format!(
            "id={} name={} enabled={} hits={}\n",
            tp.id,
            tp.name,
            if tp.enabled { 1 } else { 0 },
            tp.hit_count
        ));
    }
    out
}

pub fn trace_pipe_text(limit: usize) -> String {
    let entries = ring_snapshot(limit);
    let mut out = String::new();
    for e in entries {
        out.push_str(&format!(
            "cpu={} pid={} ts={} fn={} ip={:#x} data={:#x}\n",
            e.cpu, e.pid, e.timestamp_ns, e.function, e.ip, e.data
        ));
    }
    out
}

pub fn available_events_text() -> String {
    let state = STATE.lock();
    let mut out = String::new();
    for tp in state.tracepoints.values() {
        out.push_str(&format!("rustos:{} id={}\n", tp.name, tp.id));
    }
    out
}

fn parse_bool_line(text: &str) -> Result<bool, ()> {
    match text.trim() {
        "0" => Ok(false),
        "1" => Ok(true),
        _ => Err(()),
    }
}

// ── sysfs: /sys/kernel/tracing/ ─────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TracingAttrKind {
    TracingOn,
    FunctionTracer,
    TracePipe,
    AvailableEvents,
    Tracepoints,
}

struct TracingAttrInode {
    ino: u64,
    kind: TracingAttrKind,
    mode: u32,
}

impl TracingAttrInode {
    fn new(ino: u64, kind: TracingAttrKind, mode: u32) -> Arc<Self> {
        Arc::new(Self { ino, kind, mode })
    }

    fn read_content(&self) -> String {
        match self.kind {
            TracingAttrKind::TracingOn => {
                if tracing_on() {
                    String::from("1\n")
                } else {
                    String::from("0\n")
                }
            }
            TracingAttrKind::FunctionTracer => {
                if function_tracer_enabled() {
                    String::from("1\n")
                } else {
                    String::from("0\n")
                }
            }
            TracingAttrKind::TracePipe => trace_pipe_text(256),
            TracingAttrKind::AvailableEvents => available_events_text(),
            TracingAttrKind::Tracepoints => tracepoint_list_text(),
        }
    }

    fn write_content(&self, buf: &[u8]) -> VfsResult<usize> {
        let text = core::str::from_utf8(buf).map_err(|_| VfsError::InvalidArgument)?;
        match self.kind {
            TracingAttrKind::TracingOn => {
                set_tracing_on(parse_bool_line(text).map_err(|_| VfsError::InvalidArgument)?);
                Ok(buf.len())
            }
            TracingAttrKind::FunctionTracer => {
                set_function_tracer_enabled(
                    parse_bool_line(text).map_err(|_| VfsError::InvalidArgument)?,
                );
                Ok(buf.len())
            }
            _ => Err(VfsError::ReadOnly),
        }
    }
}

impl InodeOps for TracingAttrInode {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        let content = self.read_content();
        let bytes = content.as_bytes();
        let start = offset as usize;
        if start >= bytes.len() {
            return Ok(0);
        }
        let end = core::cmp::min(start + buf.len(), bytes.len());
        let n = end - start;
        buf[..n].copy_from_slice(&bytes[start..end]);
        Ok(n)
    }

    fn write_at(&self, offset: u64, buf: &[u8]) -> VfsResult<usize> {
        if offset != 0 {
            return Err(VfsError::InvalidArgument);
        }
        self.write_content(buf)
    }

    fn stat(&self) -> VfsResult<Stat> {
        let size = self.read_content().len() as u64;
        Ok(Stat {
            ino: self.ino,
            inode_type: InodeType::File,
            size,
            blksize: 4096,
            blocks: (size + 511) / 512,
            mode: self.mode,
            nlink: 1,
            uid: 0,
            gid: 0,
            rdev: 0,
            atime: 0,
            mtime: 0,
            ctime: 0,
        })
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }

    fn sync(&self) -> VfsResult<()> {
        Ok(())
    }

    fn lookup(&self, _name: &str) -> VfsResult<Arc<dyn InodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn create(
        &self,
        _name: &str,
        _inode_type: InodeType,
        _mode: u32,
    ) -> VfsResult<Arc<dyn InodeOps>> {
        Err(VfsError::ReadOnly)
    }

    fn unlink(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn link(&self, _name: &str, _target: Arc<dyn InodeOps>) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn rename(
        &self,
        _old_name: &str,
        _new_dir: Arc<dyn InodeOps>,
        _new_name: &str,
    ) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn readdir(&self) -> VfsResult<Vec<DirEntry>> {
        Err(VfsError::NotDirectory)
    }

    fn inode_type(&self) -> InodeType {
        InodeType::File
    }

    fn attach_child(&self, _name: &str, _child: Arc<dyn InodeOps>) -> VfsResult<()> {
        Err(VfsError::NotDirectory)
    }
}

struct TracingEventsDir {
    ino: u64,
}

impl TracingEventsDir {
    fn new(ino: u64) -> Arc<Self> {
        Arc::new(Self { ino })
    }
}

impl InodeOps for TracingEventsDir {
    fn read_at(&self, _offset: u64, _buf: &mut [u8]) -> VfsResult<usize> {
        Err(VfsError::IsDirectory)
    }

    fn write_at(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        Err(VfsError::IsDirectory)
    }

    fn stat(&self) -> VfsResult<Stat> {
        Ok(Stat {
            ino: self.ino,
            inode_type: InodeType::Directory,
            size: 4096,
            blksize: 4096,
            blocks: 8,
            mode: 0o755,
            nlink: 2,
            uid: 0,
            gid: 0,
            rdev: 0,
            atime: 0,
            mtime: 0,
            ctime: 0,
        })
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::IsDirectory)
    }

    fn sync(&self) -> VfsResult<()> {
        Ok(())
    }

    fn lookup(&self, name: &str) -> VfsResult<Arc<dyn InodeOps>> {
        let state = STATE.lock();
        if let Some(id) = state.name_to_id.get(name) {
            let tp = state.tracepoints.get(id).ok_or(VfsError::NotFound)?;
            let content = if tp.enabled { "1\n" } else { "0\n" };
            return Ok(Arc::new(TracingEventEnableInode {
                ino: self.ino + *id as u64,
                name: name.to_string(),
                content: content.to_string(),
            }));
        }
        Err(VfsError::NotFound)
    }

    fn create(
        &self,
        _name: &str,
        _inode_type: InodeType,
        _mode: u32,
    ) -> VfsResult<Arc<dyn InodeOps>> {
        Err(VfsError::ReadOnly)
    }

    fn unlink(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn link(&self, _name: &str, _target: Arc<dyn InodeOps>) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn rename(
        &self,
        _old_name: &str,
        _new_dir: Arc<dyn InodeOps>,
        _new_name: &str,
    ) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn readdir(&self) -> VfsResult<Vec<DirEntry>> {
        let state = STATE.lock();
        let mut entries = Vec::new();
        for tp in state.tracepoints.values() {
            entries.push(DirEntry {
                ino: tp.id as u64,
                name: tp.name.clone(),
                inode_type: InodeType::File,
            });
        }
        Ok(entries)
    }

    fn inode_type(&self) -> InodeType {
        InodeType::Directory
    }

    fn attach_child(&self, _name: &str, _child: Arc<dyn InodeOps>) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }
}

struct TracingEventEnableInode {
    ino: u64,
    name: String,
    content: String,
}

impl InodeOps for TracingEventEnableInode {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        let bytes = self.content.as_bytes();
        let start = offset as usize;
        if start >= bytes.len() {
            return Ok(0);
        }
        let end = core::cmp::min(start + buf.len(), bytes.len());
        let n = end - start;
        buf[..n].copy_from_slice(&bytes[start..end]);
        Ok(n)
    }

    fn write_at(&self, offset: u64, buf: &[u8]) -> VfsResult<usize> {
        if offset != 0 {
            return Err(VfsError::InvalidArgument);
        }
        let text = core::str::from_utf8(buf).map_err(|_| VfsError::InvalidArgument)?;
        let on = parse_bool_line(text).map_err(|_| VfsError::InvalidArgument)?;
        if on {
            enable_tracepoint(&self.name);
        } else {
            disable_tracepoint(&self.name);
        }
        Ok(buf.len())
    }

    fn stat(&self) -> VfsResult<Stat> {
        Ok(Stat {
            ino: self.ino,
            inode_type: InodeType::File,
            size: self.content.len() as u64,
            blksize: 4096,
            blocks: 1,
            mode: 0o644,
            nlink: 1,
            uid: 0,
            gid: 0,
            rdev: 0,
            atime: 0,
            mtime: 0,
            ctime: 0,
        })
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }

    fn sync(&self) -> VfsResult<()> {
        Ok(())
    }

    fn lookup(&self, _name: &str) -> VfsResult<Arc<dyn InodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn create(
        &self,
        _name: &str,
        _inode_type: InodeType,
        _mode: u32,
    ) -> VfsResult<Arc<dyn InodeOps>> {
        Err(VfsError::ReadOnly)
    }

    fn unlink(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn link(&self, _name: &str, _target: Arc<dyn InodeOps>) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn rename(
        &self,
        _old_name: &str,
        _new_dir: Arc<dyn InodeOps>,
        _new_name: &str,
    ) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn readdir(&self) -> VfsResult<Vec<DirEntry>> {
        Err(VfsError::NotDirectory)
    }

    fn inode_type(&self) -> InodeType {
        InodeType::File
    }

    fn attach_child(&self, _name: &str, _child: Arc<dyn InodeOps>) -> VfsResult<()> {
        Err(VfsError::NotDirectory)
    }
}

fn attach_tracing_attr(
    dir: &Arc<dyn InodeOps>,
    name: &str,
    kind: TracingAttrKind,
    mode: u32,
    ino: u64,
) -> VfsResult<()> {
    dir.attach_child(name, TracingAttrInode::new(ino, kind, mode))
}

/// Populate `/sys/kernel/tracing/` with ftrace control files.
pub fn install_sysfs(sys: &Arc<dyn InodeOps>, ino: &mut u64) -> VfsResult<()> {
    sys.create("kernel", InodeType::Directory, 0o755)?;
    let kernel = sys.lookup("kernel")?;
    kernel.create("tracing", InodeType::Directory, 0o755)?;
    let tracing = kernel.lookup("tracing")?;

    *ino += 1;
    attach_tracing_attr(
        &tracing,
        "tracing_on",
        TracingAttrKind::TracingOn,
        0o644,
        *ino,
    )?;
    *ino += 1;
    attach_tracing_attr(
        &tracing,
        "function_tracer_enabled",
        TracingAttrKind::FunctionTracer,
        0o644,
        *ino,
    )?;
    *ino += 1;
    attach_tracing_attr(
        &tracing,
        "trace_pipe",
        TracingAttrKind::TracePipe,
        0o444,
        *ino,
    )?;
    *ino += 1;
    attach_tracing_attr(
        &tracing,
        "available_events",
        TracingAttrKind::AvailableEvents,
        0o444,
        *ino,
    )?;
    *ino += 1;
    attach_tracing_attr(
        &tracing,
        "tracepoints",
        TracingAttrKind::Tracepoints,
        0o444,
        *ino,
    )?;

    *ino += 1;
    tracing.create("events", InodeType::Directory, 0o755)?;
    let events = tracing.lookup("events")?;
    *ino += 1;
    events.attach_child("rustos", TracingEventsDir::new(*ino))?;

    Ok(())
}

fn register_builtin_tracepoints() {
    register_tracepoint("syscalls:sys_enter");
    register_tracepoint("syscalls:sys_exit");
    register_tracepoint("sched:sched_switch");
    register_tracepoint("kprobes:probe_hit");
    register_tracepoint("file:open");
    register_tracepoint("file:read");
    register_tracepoint("file:write");
}

pub fn init() {
    {
        let mut state = STATE.lock();
        state.tracepoints.clear();
        state.name_to_id.clear();
        state.ring.clear();
        state.ring_head = 0;
        state.ring_count = 0;
        state.function_tracer_enabled = false;
        state.tracing_on = false;
        state.next_id = 1;
    }
    NEXT_TRACEPOINT_ID.store(1, Ordering::Release);
    TRACING_ON.store(false, Ordering::Release);
    FUNCTION_TRACER.store(false, Ordering::Release);
    TRACE_HITS.store(0, Ordering::Release);

    register_builtin_tracepoints();

    crate::serial_println!("[trace] ftrace/tracepoints initialized");
}
