//! Kernel debugging and Linux compatibility instrumentation.

pub mod syscall_trace;

pub use syscall_trace::trace_syscall;
