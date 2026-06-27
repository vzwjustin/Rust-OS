//! `gio` matching `gio/gio.h`.
//!
//! Umbrella header that re-exports all GIO modules.
//! In Rust, this role is served by `lib.rs` which declares all modules as `pub mod`.
//! This module provides a convenient re-export of the key public types.

pub use crate::gaction::Action;
pub use crate::gactiongroup::ActionGroup;
pub use crate::gappinfo::AppInfo;
pub use crate::gcancellable::GCancellable;
pub use crate::gfile::File;
pub use crate::gfileattribute::FileAttributeType;
pub use crate::gfileinfo::FileInfo;
pub use crate::ginputstream::InputStream;
pub use crate::gioscheduler::io_scheduler_push_job;
pub use crate::goutputstream::OutputStream;
pub use crate::gvfs::Vfs;
pub use crate::gvolume::Volume;
pub use crate::gvolumemonitor::VolumeMonitor;
