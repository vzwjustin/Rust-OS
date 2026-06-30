// SPDX-License-Identifier: GPL-2.0-compatible
//! Linux locking subsystem port for RustOS.
//!
//! Mirrors `kernel/locking/` from the Linux kernel, providing sleeping
//! mutexes, reader-writer semaphores, counting semaphores, priority-
//! inheritance mutexes, and completion variables.
//!
//! All primitives are `#[no_std]` and use spin-wait stubs where a real
//! scheduler integration would call `schedule()` / `wake_up()`.

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
