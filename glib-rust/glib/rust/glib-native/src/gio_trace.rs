//! `gio_trace` matching `gio/gio_trace.h`.
//!
//! Tracing/probe macros for GIO. In the original C code, these are either
//! SystemTap/DTrace probes or no-ops depending on `HAVE_DTRACE`.
//!
//! In this no_std Rust port, tracing is a compile-time no-op (matching the
//! `#else` branch of the C header), but we provide a minimal runtime
//! trace buffer for debugging.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// Maximum number of trace entries kept in the ring buffer.
const TRACE_BUF_SIZE: usize = 256;

/// A single trace event.
#[derive(Debug, Clone)]
pub struct TraceEvent {
    pub message: String,
    pub seq: u64,
}

/// Global trace buffer (ring buffer).
static TRACE_BUF: Mutex<TraceBuffer> = Mutex::new(TraceBuffer::new());

struct TraceBuffer {
    entries: Vec<TraceEvent>,
    head: usize,
    seq: u64,
}

impl TraceBuffer {
    const fn new() -> Self {
        Self {
            entries: Vec::new(),
            head: 0,
            seq: 0,
        }
    }

    fn push(&mut self, msg: &str) {
        self.seq += 1;
        let event = TraceEvent {
            message: msg.to_string(),
            seq: self.seq,
        };
        if self.entries.len() < TRACE_BUF_SIZE {
            self.entries.push(event);
        } else {
            self.entries[self.head] = event;
            self.head = (self.head + 1) % TRACE_BUF_SIZE;
        }
    }

    fn drain(&mut self) -> Vec<TraceEvent> {
        let mut out = Vec::new();
        if self.entries.len() < TRACE_BUF_SIZE {
            out.extend(self.entries.drain(..));
        } else {
            // Ring buffer: order from head to end, then start to head
            let (left, right) = self.entries.split_at(self.head);
            out.extend(right.iter().cloned());
            out.extend(left.iter().cloned());
            self.entries.clear();
            self.head = 0;
        }
        out
    }
}

/// Record a trace event (no-op when tracing is disabled, like the C macro).
#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => {
        $crate::gio_trace::trace_record(&$crate::gformat!($($arg)*))
    };
}

/// Record a trace message into the global trace buffer.
pub fn trace_record(msg: &str) {
    TRACE_BUF.lock().push(msg);
}

/// Drain and return all trace events.
pub fn drain_trace() -> Vec<TraceEvent> {
    TRACE_BUF.lock().drain()
}

/// Get the current number of trace events.
pub fn trace_count() -> usize {
    TRACE_BUF.lock().entries.len()
}

/// Clear all trace events.
pub fn clear_trace() {
    TRACE_BUF.lock().entries.clear();
    TRACE_BUF.lock().head = 0;
    TRACE_BUF.lock().seq = 0;
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_trace_buffer() {
        let _guard = TEST_LOCK.lock();
        clear_trace();
        trace_record("event 1");
        trace_record("event 2");
        assert_eq!(trace_count(), 2);
        let events = drain_trace();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].message, "event 1");
        assert_eq!(events[1].message, "event 2");
        assert_eq!(trace_count(), 0);
    }

    #[test]
    fn test_ring_buffer() {
        let _guard = TEST_LOCK.lock();
        clear_trace();
        for i in 0..300 {
            trace_record(&format!("event {i}"));
        }
        let events = drain_trace();
        assert_eq!(events.len(), 256);
        // First entry should be event 44 (oldest after wrap)
        assert!(events[0].message.contains("44"));
    }
}
