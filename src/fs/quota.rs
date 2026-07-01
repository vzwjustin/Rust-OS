//! Filesystem quota subsystem stub.
//!
//! Provides a stub for the quota subsystem, which enforces per-user and per-group
//! disk usage and inode limits on filesystems that support it (ext4, XFS, etc.).
//! Real implementation would track quotas, issue warnings, and enforce limits.
//! See linux-master fs/quota/ for reference.

// TODO: port from linux-master fs/quota/

/// Quota types (stub).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotaType {
    /// User quota (stub)
    User,
    /// Group quota (stub)
    Group,
}

/// Quota limits (stub).
#[derive(Debug, Clone)]
pub struct QuotaLimits {
    /// Hard limit in blocks (stub)
    pub hard_limit: u64,
    /// Soft limit in blocks (stub)
    pub soft_limit: u64,
    /// Inode hard limit (stub)
    pub inode_hard_limit: u64,
    /// Inode soft limit (stub)
    pub inode_soft_limit: u64,
}

impl Default for QuotaLimits {
    fn default() -> Self {
        Self {
            hard_limit: 0,
            soft_limit: 0,
            inode_hard_limit: 0,
            inode_soft_limit: 0,
        }
    }
}

/// Initialize quota subsystem (stub).
pub fn init() -> crate::fs::FsResult<()> {
    // TODO: port from linux-master fs/quota/dquot.c dquot_init()
    Ok(())
}

/// Set quota limits for a user or group (stub).
pub fn set_limits(_quota_type: QuotaType, _id: u32, _limits: &QuotaLimits) -> crate::fs::FsResult<()> {
    // TODO: port from linux-master fs/quota/dquot.c dquot_acquire()
    Err(crate::fs::FsError::NotSupported)
}
