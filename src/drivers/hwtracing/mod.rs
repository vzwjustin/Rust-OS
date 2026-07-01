//! Hwtracing (hardware tracing) driver subsystem
//!
//! Provides hardware tracing framework for performance analysis.
//! Mirrors Linux's `drivers/hwtracing/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// Tracer type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TracerType {
    IntelPt,
    CoresightEtm,
    Stm,
    Generic,
}

/// Trace mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceMode {
    Disabled,
    Continuous,
    BranchOnly,
    SingleStepRange,
}

/// Hardware tracer (Linux `struct etm_perf_aux`).
pub struct HwTracer {
    pub id: u32,
    pub name: String,
    pub tracer_type: TracerType,
    pub ops: HwTracerOps,
    pub mode: TraceMode,
    pub enabled: AtomicBool,
    pub trace_buf_size: usize,
}

/// Tracer operations.
pub struct HwTracerOps {
    pub enable: fn(tracer_id: u32) -> Result<(), &'static str>,
    pub disable: fn(tracer_id: u32) -> Result<(), &'static str>,
    pub set_mode: fn(tracer_id: u32, mode: TraceMode) -> Result<(), &'static str>,
    pub read_trace: fn(tracer_id: u32, buf: &mut [u8]) -> Result<usize, &'static str>,
    pub set_sink: fn(tracer_id: u32, sink_id: u32) -> Result<(), &'static str>,
}

/// Trace sink (output buffer).
pub struct TraceSink {
    pub id: u32,
    pub name: String,
    pub buf_size: usize,
    pub overflow: bool,
}

// ── Registry ────────────────────────────────────────────────────────────

static TRACER_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static SINK_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static HW_TRACERS: RwLock<BTreeMap<u32, HwTracer>> = RwLock::new(BTreeMap::new());
static TRACE_SINKS: RwLock<BTreeMap<u32, TraceSink>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a hardware tracer.
pub fn register_tracer(
    name: &str,
    tracer_type: TracerType,
    ops: HwTracerOps,
    trace_buf_size: usize,
) -> Result<u32, &'static str> {
    let id = TRACER_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let tracer = HwTracer {
        id,
        name: String::from(name),
        tracer_type,
        ops,
        mode: TraceMode::Disabled,
        enabled: AtomicBool::new(false),
        trace_buf_size,
    };
    HW_TRACERS.write().insert(id, tracer);
    Ok(id)
}

/// Register a trace sink.
pub fn register_sink(name: &str, buf_size: usize) -> Result<u32, &'static str> {
    let id = SINK_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let sink = TraceSink {
        id,
        name: String::from(name),
        buf_size,
        overflow: false,
    };
    TRACE_SINKS.write().insert(id, sink);
    Ok(id)
}

/// Enable a tracer.
pub fn enable_tracer(tracer_id: u32) -> Result<(), &'static str> {
    let enable_fn = {
        let tracers = HW_TRACERS.read();
        let tracer = tracers.get(&tracer_id).ok_or("Tracer not found")?;
        tracer.ops.enable
    };
    (enable_fn)(tracer_id)?;
    let tracers = HW_TRACERS.read();
    if let Some(tracer) = tracers.get(&tracer_id) {
        tracer.enabled.store(true, Ordering::SeqCst);
    }
    Ok(())
}

/// Disable a tracer.
pub fn disable_tracer(tracer_id: u32) -> Result<(), &'static str> {
    let disable_fn = {
        let tracers = HW_TRACERS.read();
        let tracer = tracers.get(&tracer_id).ok_or("Tracer not found")?;
        tracer.ops.disable
    };
    (disable_fn)(tracer_id)?;
    let tracers = HW_TRACERS.read();
    if let Some(tracer) = tracers.get(&tracer_id) {
        tracer.enabled.store(false, Ordering::SeqCst);
    }
    Ok(())
}

/// Set trace mode.
pub fn set_trace_mode(tracer_id: u32, mode: TraceMode) -> Result<(), &'static str> {
    let set_mode_fn = {
        let tracers = HW_TRACERS.read();
        let tracer = tracers.get(&tracer_id).ok_or("Tracer not found")?;
        tracer.ops.set_mode
    };
    (set_mode_fn)(tracer_id, mode)?;
    let mut tracers = HW_TRACERS.write();
    if let Some(tracer) = tracers.get_mut(&tracer_id) {
        tracer.mode = mode;
    }
    Ok(())
}

/// Read trace data.
pub fn read_trace(tracer_id: u32, buf: &mut [u8]) -> Result<usize, &'static str> {
    let read_fn = {
        let tracers = HW_TRACERS.read();
        let tracer = tracers.get(&tracer_id).ok_or("Tracer not found")?;
        tracer.ops.read_trace
    };
    (read_fn)(tracer_id, buf)
}

/// List all tracers.
pub fn list_tracers() -> Vec<(u32, String, TracerType, bool)> {
    HW_TRACERS
        .read()
        .iter()
        .map(|(id, t)| {
            (
                *id,
                t.name.clone(),
                t.tracer_type,
                t.enabled.load(Ordering::SeqCst),
            )
        })
        .collect()
}

/// Count tracers.
pub fn tracer_count() -> usize {
    HW_TRACERS.read().len()
}

// ── Software tracer ─────────────────────────────────────────────────────

fn sw_enable(_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_disable(_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_set_mode(_id: u32, _mode: TraceMode) -> Result<(), &'static str> {
    Ok(())
}
fn sw_read_trace(_id: u32, buf: &mut [u8]) -> Result<usize, &'static str> {
    for b in buf.iter_mut() {
        *b = 0;
    }
    Ok(buf.len())
}
fn sw_set_sink(_id: u32, _sink: u32) -> Result<(), &'static str> {
    Ok(())
}

/// Software tracer ops.
pub fn software_tracer_ops() -> HwTracerOps {
    HwTracerOps {
        enable: sw_enable,
        disable: sw_disable,
        set_mode: sw_set_mode,
        read_trace: sw_read_trace,
        set_sink: sw_set_sink,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    if !HW_TRACERS.read().is_empty() {
        return Ok(());
    }

    let ops = software_tracer_ops();
    let tracer_id = register_tracer("sw-pt", TracerType::IntelPt, ops, 64 * 1024)?;
    register_sink("sw-sink0", 64 * 1024)?;

    crate::serial_println!(
        "hwtracing: software Intel PT tracer registered (id={}, 64KB buffer)",
        tracer_id
    );
    Ok(())
}
