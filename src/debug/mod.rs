//! Kernel debugging and Linux compatibility instrumentation.

pub mod syscall_trace;

pub use syscall_trace::{is_enabled, set_enabled, trace_syscall};
