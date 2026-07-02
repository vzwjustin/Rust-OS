//! Filesystem quota subsystem.
//!
//! Enforces per-user, per-group, and per-project disk usage and inode
//! limits on filesystems that support it (ext4, XFS, etc.).  The quota
//! manager tracks limits and current usage in a global registry keyed
//! by (filesystem type, quota type, id).  Hard limit of 0 means unlimited;
//! violations return `NoSpaceLeft`.

use crate::fs::{FileSystemType, FsError, FsResult};
use alloc::{
    collections::BTreeMap,
    format,
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};
use spin::RwLock;

/// Quota type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum QuotaType {
    /// User quota.
    User,
    /// Group quota.
    Group,
    /// Project quota.
    Project,
}

/// Quota limits for a user/group/project.
#[derive(Debug, Clone)]
pub struct QuotaLimits {
    /// Hard limit in blocks (0 = unlimited).
    pub hard_limit: u64,
    /// Soft limit in blocks (warnings above this).
    pub soft_limit: u64,
    /// Inode hard limit (0 = unlimited).
    pub inode_hard_limit: u64,
    /// Inode soft limit (warnings above this).
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

/// Current quota usage.
#[derive(Debug, Clone, Default)]
pub struct QuotaUsage {
    /// Blocks currently used.
    pub used_blocks: u64,
    /// Inodes currently used.
    pub used_inodes: u64,
}

/// A quota entry combining limits and usage.
#[derive(Debug, Clone)]
pub struct QuotaEntry {
    /// Limits for this quota.
    pub limits: QuotaLimits,
    /// Current usage.
    pub usage: QuotaUsage,
}

/// A disk quota handle (`dquot` in Linux).
#[derive(Debug, Clone)]
pub struct Dquot {
    /// Filesystem type this quota belongs to.
    pub fs_type: FileSystemType,
    /// Quota type.
    pub quota_type: QuotaType,
    /// User/group/project ID.
    pub id: u32,
    /// The quota entry.
    pub entry: QuotaEntry,
}

/// Quota manager: global registry of quota entries.
pub struct QuotaManager {
    entries: RwLock<BTreeMap<(FileSystemType, QuotaType, u32), QuotaEntry>>,
}

impl QuotaManager {
    /// Create a new empty quota manager.
    pub const fn new() -> Self {
        Self {
            entries: RwLock::new(BTreeMap::new()),
        }
    }

    /// Set quota limits for a user/group/project on a filesystem.
    pub fn set_limit(
        &self,
        fs_type: FileSystemType,
        quota_type: QuotaType,
        id: u32,
        limits: QuotaLimits,
    ) -> FsResult<()> {
        let mut entries = self.entries.write();
        let entry = entries
            .entry((fs_type, quota_type, id))
            .or_insert_with(|| QuotaEntry {
                limits: QuotaLimits::default(),
                usage: QuotaUsage::default(),
            });
        entry.limits = limits;
        Ok(())
    }

    /// Get current usage for a user/group/project.
    pub fn get_usage(
        &self,
        fs_type: FileSystemType,
        quota_type: QuotaType,
        id: u32,
    ) -> FsResult<QuotaUsage> {
        let entries = self.entries.read();
        let usage = entries
            .get(&(fs_type, quota_type, id))
            .map(|e| e.usage.clone())
            .unwrap_or_default();
        Ok(usage)
    }

    /// Check if a block allocation would exceed quota.
    /// Returns Ok(()) if allowed, Err(NoSpaceLeft) if it would exceed.
    pub fn check_quota(
        &self,
        fs_type: FileSystemType,
        quota_type: QuotaType,
        id: u32,
        additional_blocks: u64,
    ) -> FsResult<()> {
        let entries = self.entries.read();
        if let Some(entry) = entries.get(&(fs_type, quota_type, id)) {
            // Hard limit 0 = unlimited
            if entry.limits.hard_limit > 0 {
                let new_total = entry.usage.used_blocks + additional_blocks;
                if new_total > entry.limits.hard_limit {
                    return Err(FsError::NoSpaceLeft);
                }
            }
        }
        Ok(())
    }

    /// Acquire a quota handle (dquot) for a user/group/project.
    pub fn acquire(
        &self,
        fs_type: FileSystemType,
        quota_type: QuotaType,
        id: u32,
    ) -> FsResult<Dquot> {
        let entries = self.entries.read();
        let entry = entries
            .get(&(fs_type, quota_type, id))
            .cloned()
            .unwrap_or_else(|| QuotaEntry {
                limits: QuotaLimits::default(),
                usage: QuotaUsage::default(),
            });
        Ok(Dquot {
            fs_type,
            quota_type,
            id,
            entry,
        })
    }

    /// Release a quota handle, committing usage changes.
    pub fn release(&self, dquot: &Dquot) -> FsResult<()> {
        let mut entries = self.entries.write();
        entries.insert(
            (dquot.fs_type, dquot.quota_type, dquot.id),
            dquot.entry.clone(),
        );
        Ok(())
    }

    /// Allocate blocks for a user/group/project.
    /// Checks quota and updates usage.
    pub fn alloc_block(
        &self,
        fs_type: FileSystemType,
        quota_type: QuotaType,
        id: u32,
        count: u64,
    ) -> FsResult<()> {
        // Check quota first
        self.check_quota(fs_type, quota_type, id, count)?;

        let mut entries = self.entries.write();
        let entry = entries
            .entry((fs_type, quota_type, id))
            .or_insert_with(|| QuotaEntry {
                limits: QuotaLimits::default(),
                usage: QuotaUsage::default(),
            });
        entry.usage.used_blocks += count;
        Ok(())
    }

    /// Free blocks for a user/group/project.
    pub fn free_block(
        &self,
        fs_type: FileSystemType,
        quota_type: QuotaType,
        id: u32,
        count: u64,
    ) -> FsResult<()> {
        let mut entries = self.entries.write();
        if let Some(entry) = entries.get_mut(&(fs_type, quota_type, id)) {
            entry.usage.used_blocks = entry.usage.used_blocks.saturating_sub(count);
        }
        Ok(())
    }

    /// Allocate an inode for a user/group/project.
    /// Checks inode quota and updates usage.
    pub fn alloc_inode(
        &self,
        fs_type: FileSystemType,
        quota_type: QuotaType,
        id: u32,
    ) -> FsResult<()> {
        let mut entries = self.entries.write();
        let entry = entries
            .entry((fs_type, quota_type, id))
            .or_insert_with(|| QuotaEntry {
                limits: QuotaLimits::default(),
                usage: QuotaUsage::default(),
            });

        // Check inode hard limit (0 = unlimited)
        if entry.limits.inode_hard_limit > 0 {
            if entry.usage.used_inodes + 1 > entry.limits.inode_hard_limit {
                return Err(FsError::NoSpaceLeft);
            }
        }

        entry.usage.used_inodes += 1;
        Ok(())
    }

    /// Free an inode for a user/group/project.
    pub fn free_inode(
        &self,
        fs_type: FileSystemType,
        quota_type: QuotaType,
        id: u32,
    ) -> FsResult<()> {
        let mut entries = self.entries.write();
        if let Some(entry) = entries.get_mut(&(fs_type, quota_type, id)) {
            entry.usage.used_inodes = entry.usage.used_inodes.saturating_sub(1);
        }
        Ok(())
    }

    /// Transfer quota ownership from one user/group to another.
    /// Moves `blocks` and `inodes` from `from_id` to `to_id`.
    pub fn transfer(
        &self,
        fs_type: FileSystemType,
        quota_type: QuotaType,
        from_id: u32,
        to_id: u32,
        blocks: u64,
        inodes: u64,
    ) -> FsResult<()> {
        // Check destination quota
        self.check_quota(fs_type, quota_type, to_id, blocks)?;

        let mut entries = self.entries.write();

        // Decrement source
        if let Some(entry) = entries.get_mut(&(fs_type, quota_type, from_id)) {
            entry.usage.used_blocks = entry.usage.used_blocks.saturating_sub(blocks);
            entry.usage.used_inodes = entry.usage.used_inodes.saturating_sub(inodes);
        }

        // Increment destination
        let dest = entries
            .entry((fs_type, quota_type, to_id))
            .or_insert_with(|| QuotaEntry {
                limits: QuotaLimits::default(),
                usage: QuotaUsage::default(),
            });
        dest.usage.used_blocks += blocks;
        dest.usage.used_inodes += inodes;

        Ok(())
    }
}

/// Global quota manager instance.
static GLOBAL_QUOTA_MANAGER: RwLock<Option<QuotaManager>> = RwLock::new(None);

/// Initialize the quota subsystem.
pub fn init() -> FsResult<()> {
    let mut mgr = GLOBAL_QUOTA_MANAGER.write();
    *mgr = Some(QuotaManager::new());
    Ok(())
}

/// Get a reference to the global quota manager.
fn with_manager<F, R>(f: F) -> FsResult<R>
where
    F: FnOnce(&QuotaManager) -> FsResult<R>,
{
    let mgr = GLOBAL_QUOTA_MANAGER.read();
    let manager = mgr.as_ref().ok_or(FsError::IoError)?;
    f(manager)
}

/// Set quota limits for a user or group (legacy API).
pub fn set_limits(quota_type: QuotaType, id: u32, limits: &QuotaLimits) -> FsResult<()> {
    with_manager(|mgr| mgr.set_limit(FileSystemType::Ext2, quota_type, id, limits.clone()))
}

/// dquot_init — initialize the quota subsystem.
pub fn dquot_init() -> FsResult<()> {
    init()
}

/// dquot_acquire — acquire a quota handle.
pub fn dquot_acquire(
    fs_type: FileSystemType,
    quota_type: QuotaType,
    id: u32,
) -> FsResult<Dquot> {
    with_manager(|mgr| mgr.acquire(fs_type, quota_type, id))
}

/// dquot_release — release a quota handle.
pub fn dquot_release(dquot: &Dquot) -> FsResult<()> {
    with_manager(|mgr| mgr.release(dquot))
}

/// dquot_alloc_block — allocate blocks under quota.
pub fn dquot_alloc_block(
    fs_type: FileSystemType,
    quota_type: QuotaType,
    id: u32,
    count: u64,
) -> FsResult<()> {
    with_manager(|mgr| mgr.alloc_block(fs_type, quota_type, id, count))
}

/// dquot_free_block — free blocks from quota.
pub fn dquot_free_block(
    fs_type: FileSystemType,
    quota_type: QuotaType,
    id: u32,
    count: u64,
) -> FsResult<()> {
    with_manager(|mgr| mgr.free_block(fs_type, quota_type, id, count))
}

/// dquot_alloc_inode — allocate an inode under quota.
pub fn dquot_alloc_inode(
    fs_type: FileSystemType,
    quota_type: QuotaType,
    id: u32,
) -> FsResult<()> {
    with_manager(|mgr| mgr.alloc_inode(fs_type, quota_type, id))
}

/// dquot_free_inode — free an inode from quota.
pub fn dquot_free_inode(
    fs_type: FileSystemType,
    quota_type: QuotaType,
    id: u32,
) -> FsResult<()> {
    with_manager(|mgr| mgr.free_inode(fs_type, quota_type, id))
}

/// dquot_transfer — transfer quota between users/groups.
pub fn dquot_transfer(
    fs_type: FileSystemType,
    quota_type: QuotaType,
    from_id: u32,
    to_id: u32,
    blocks: u64,
    inodes: u64,
) -> FsResult<()> {
    with_manager(|mgr| mgr.transfer(fs_type, quota_type, from_id, to_id, blocks, inodes))
}


