//! Filesystem quota subsystem.
//!
//! Provides per-user and per-group disk usage and inode limits for filesystems
//! that support it (ext4, XFS, etc.). This implementation keeps the quota
//! tables in memory, keyed by `(QuotaType, id)`. Each entry records the hard
//! and soft limits, the current usage, and a grace period after which a soft
//! limit violation is treated as a hard violation.

use alloc::collections::BTreeMap;
use spin::RwLock;

use crate::fs::{FsError, FsResult};

/// Quota type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum QuotaType {
    /// User quota (tracked by uid)
    User,
    /// Group quota (tracked by gid)
    Group,
}

/// Quota limits for a user or group.
#[derive(Debug, Clone)]
pub struct QuotaLimits {
    /// Hard limit in blocks (cannot be exceeded).
    pub hard_limit: u64,
    /// Soft limit in blocks (can be exceeded within the grace period).
    pub soft_limit: u64,
    /// Inode hard limit (cannot be exceeded).
    pub inode_hard_limit: u64,
    /// Inode soft limit (can be exceeded within the grace period).
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

/// Current usage for a user or group.
#[derive(Debug, Clone, Default)]
pub struct QuotaUsage {
    /// Number of blocks currently in use.
    pub used_blocks: u64,
    /// Number of inodes currently in use.
    pub used_inodes: u64,
    /// Timestamp (in seconds since boot/epoch) when the soft limit was first
    /// exceeded. `0` means the soft limit is not currently exceeded.
    pub soft_exceeded_since: u64,
}

/// A complete quota entry: limits plus current usage.
#[derive(Debug, Clone)]
struct QuotaEntry {
    limits: QuotaLimits,
    usage: QuotaUsage,
    /// Grace period in seconds. A soft-limit violation older than this is
    /// treated as a hard-limit violation.
    grace_period_secs: u64,
}

impl QuotaEntry {
    fn new(limits: QuotaLimits, grace_period_secs: u64) -> Self {
        Self {
            limits,
            usage: QuotaUsage::default(),
            grace_period_secs,
        }
    }
}

/// Global quota table keyed by `(quota_type, id)`.
static QUOTA_TABLE: RwLock<BTreeMap<(QuotaType, u32), QuotaEntry>> = RwLock::new(BTreeMap::new());

/// Default grace period (7 days, in seconds).
pub const DEFAULT_GRACE_PERIOD_SECS: u64 = 7 * 24 * 60 * 60;

/// Initialize the quota subsystem.
///
/// Clears all tracked quota entries. Safe to call multiple times.
pub fn init() -> FsResult<()> {
    QUOTA_TABLE.write().clear();
    Ok(())
}

/// Set quota limits for a user or group.
///
/// Replaces any previously configured limits for the given `(quota_type, id)`.
/// Current usage is preserved if the entry already existed; otherwise it starts
/// at zero. The grace period is set to `DEFAULT_GRACE_PERIOD_SECS` for new
/// entries and left unchanged for existing ones.
pub fn set_limits(quota_type: QuotaType, id: u32, limits: &QuotaLimits) -> FsResult<()> {
    let mut table = QUOTA_TABLE.write();
    let entry = table
        .entry((quota_type, id))
        .and_modify(|e| {
            e.limits = limits.clone();
        })
        .or_insert_with(|| QuotaEntry::new(limits.clone(), DEFAULT_GRACE_PERIOD_SECS));
    // `and_modify` already updated limits for existing entries; for new entries
    // `or_insert_with` created the entry. The borrow of `entry` is unused here
    // but kept to ensure the insert happened.
    let _ = &entry;
    Ok(())
}

/// Set the grace period (in seconds) for a user or group quota.
///
/// Returns `NotFound` if no quota entry exists for the given key.
pub fn set_grace_period(quota_type: QuotaType, id: u32, grace_secs: u64) -> FsResult<()> {
    let mut table = QUOTA_TABLE.write();
    let entry = table.get_mut(&(quota_type, id)).ok_or(FsError::NotFound)?;
    entry.grace_period_secs = grace_secs;
    Ok(())
}

/// Get the configured limits for a user or group.
///
/// Returns `NotFound` if no quota has been configured for the key.
pub fn get_limits(quota_type: QuotaType, id: u32) -> FsResult<QuotaLimits> {
    let table = QUOTA_TABLE.read();
    let entry = table.get(&(quota_type, id)).ok_or(FsError::NotFound)?;
    Ok(entry.limits.clone())
}

/// Get the current usage for a user or group.
///
/// Returns a zeroed usage record if no quota entry exists yet (the entry is
/// not implicitly created).
pub fn get_usage(quota_type: QuotaType, id: u32) -> QuotaUsage {
    let table = QUOTA_TABLE.read();
    table
        .get(&(quota_type, id))
        .map(|e| e.usage.clone())
        .unwrap_or_default()
}

/// Account a block-usage delta against a user or group.
///
/// `delta` may be negative (represented as a signed value) to release blocks.
/// Returns `NoSpaceLeft` if the delta would exceed the hard limit (or an
/// expired soft limit). The soft-limit timer is updated when the soft limit is
/// crossed.
pub fn add_blocks(
    quota_type: QuotaType,
    id: u32,
    delta: i64,
    now_secs: u64,
) -> FsResult<()> {
    let mut table = QUOTA_TABLE.write();
    let entry = table.get_mut(&(quota_type, id)).ok_or(FsError::NotFound)?;

    let new_used = (entry.usage.used_blocks as i64 + delta).max(0) as u64;
    let hard = entry.limits.hard_limit;
    let soft = entry.limits.soft_limit;

    // Hard limit is absolute.
    if hard > 0 && new_used > hard {
        return Err(FsError::NoSpaceLeft);
    }

    // Soft limit: allowed within the grace period.
    if soft > 0 && new_used > soft {
        if entry.usage.soft_exceeded_since == 0 {
            entry.usage.soft_exceeded_since = now_secs;
        } else if now_secs
            .saturating_sub(entry.usage.soft_exceeded_since)
            > entry.grace_period_secs
        {
            return Err(FsError::NoSpaceLeft);
        }
    } else {
        // Back under the soft limit — clear the timer.
        entry.usage.soft_exceeded_since = 0;
    }

    entry.usage.used_blocks = new_used;
    Ok(())
}

/// Account an inode-usage delta against a user or group.
///
/// Returns `NoSpaceLeft` if the inode hard limit (or an expired soft limit)
/// would be exceeded.
pub fn add_inodes(
    quota_type: QuotaType,
    id: u32,
    delta: i64,
    now_secs: u64,
) -> FsResult<()> {
    let mut table = QUOTA_TABLE.write();
    let entry = table.get_mut(&(quota_type, id)).ok_or(FsError::NotFound)?;

    let new_used = (entry.usage.used_inodes as i64 + delta).max(0) as u64;
    let hard = entry.limits.inode_hard_limit;
    let soft = entry.limits.inode_soft_limit;

    if hard > 0 && new_used > hard {
        return Err(FsError::NoSpaceLeft);
    }

    if soft > 0 && new_used > soft {
        if entry.usage.soft_exceeded_since == 0 {
            entry.usage.soft_exceeded_since = now_secs;
        } else if now_secs
            .saturating_sub(entry.usage.soft_exceeded_since)
            > entry.grace_period_secs
        {
            return Err(FsError::NoSpaceLeft);
        }
    } else {
        entry.usage.soft_exceeded_since = 0;
    }

    entry.usage.used_inodes = new_used;
    Ok(())
}

/// Remove a quota entry, stopping enforcement for the given key.
pub fn remove_quota(quota_type: QuotaType, id: u32) -> FsResult<()> {
    QUOTA_TABLE.write().remove(&(quota_type, id));
    Ok(())
}

/// Number of quota entries currently tracked.
pub fn entry_count() -> usize {
    QUOTA_TABLE.read().len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_and_get_limits() {
        init().unwrap();
        let limits = QuotaLimits {
            hard_limit: 1000,
            soft_limit: 800,
            inode_hard_limit: 100,
            inode_soft_limit: 80,
        };
        set_limits(QuotaType::User, 1000, &limits).unwrap();
        let got = get_limits(QuotaType::User, 1000).unwrap();
        assert_eq!(got.hard_limit, 1000);
        assert_eq!(got.soft_limit, 800);
    }

    #[test]
    fn test_block_enforcement() {
        init().unwrap();
        let limits = QuotaLimits {
            hard_limit: 100,
            soft_limit: 80,
            ..QuotaLimits::default()
        };
        set_limits(QuotaType::Group, 500, &limits).unwrap();
        // Within soft limit.
        assert!(add_blocks(QuotaType::Group, 500, 50, 0).is_ok());
        // Over soft but within grace.
        assert!(add_blocks(QuotaType::Group, 500, 20, 1).is_ok());
        // Over hard.
        assert_eq!(
            add_blocks(QuotaType::Group, 500, 100, 2),
            Err(FsError::NoSpaceLeft)
        );
    }
}
