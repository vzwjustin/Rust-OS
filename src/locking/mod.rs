// SPDX-License-Identifier: GPL-2.0-compatible
//! Linux locking subsystem port for RustOS.
//!
//! Mirrors `kernel/locking/` from the Linux kernel, providing sleeping
//! mutexes, reader-writer semaphores, counting semaphores, priority-
//! inheritance mutexes, and completion variables.
//!
//! All primitives are `#[no_std]` and yield the CPU to the scheduler
//! (`crate::scheduler::yield_cpu()`) in the contended case, matching
//! Linux's sleeping lock behaviour.

#![allow(dead_code)]

pub mod completion;
pub mod lockdep;
pub mod mutex;
pub mod percpu_rwsem;
pub mod qspinlock;
pub mod rtmutex;
pub mod rwsem;
pub mod semaphore;

pub use completion::Completion;
pub use mutex::Mutex;
pub use percpu_rwsem::PercpuRwSemaphore;
pub use qspinlock::QSpinLock;
pub use rtmutex::RtMutex;
pub use rwsem::RwSemaphore;
pub use semaphore::Semaphore;
