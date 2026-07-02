//! Disk quota subsystem — in-memory per-mount block/inode accounting.
//!
//! Implements Linux `quotactl(2)` commands with `fs_disk_quota` / `dqinfo` layouts
//! from `uapi/linux/quota.h` and hooks VFS write/truncate/create/unlink paths.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use spin::Mutex;

use crate::linux_compat::process_ops;
use crate::linux_compat::{LinuxError, LinuxResult};
use crate::memory::user_space::UserSpaceMemory;
use crate::vfs::{Stat, VfsError, VfsResult};

// ── Linux quota constants (uapi/linux/quota.h) ──────────────────────────

pub const USRQUOTA: u32 = 0;
pub const GRPQUOTA: u32 = 1;

pub const QFMT_VFS_V0: u32 = 2;

pub const QIF_BLIMITS: u32 = 1 << 0;
pub const QIF_SPACE: u32 = 1 << 1;
pub const QIF_ILIMITS: u32 = 1 << 2;
pub const QIF_INODES: u32 = 1 << 3;
pub const QIF_BTIME: u32 = 1 << 4;
pub const QIF_ITIME: u32 = 1 << 5;

/// quotactl subcommand in bits 8..15 (matches `fs_ops` encoding).
pub const Q_QUOTAON: i32 = 0x0100;
pub const Q_QUOTAOFF: i32 = 0x0200;
pub const Q_GETQUOTA: i32 = 0x0300;
pub const Q_SETQUOTA: i32 = 0x0400;
pub const Q_GETINFO: i32 = 0x0500;
pub const Q_SETINFO: i32 = 0x0600;
pub const Q_GETFMT: i32 = 0x0700;
pub const Q_SYNC: i32 = 0x0800;

/// `struct fs_disk_quota` — returned by Q_GETQUOTA / accepted by Q_SETQUOTA.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct FsDiskQuota {
    pub dqb_bhardlimit: i64,
    pub dqb_bsoftlimit: i64,
    pub dqb_curspace: i64,
    pub dqb_ihardlimit: i64,
    pub dqb_isoftlimit: i64,
    pub dqb_curinodes: i64,
    pub dqb_btime: i64,
    pub dqb_itime: i64,
    pub dqb_valid: u32,
}

/// `struct dqinfo` — grace periods and global flags for a mount.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DqInfo {
    pub dqi_bgrace: u64,
    pub dqi_igrace: u64,
    pub dqi_flags: u32,
    pub dqi_valid: u32,
}

const DQI_FLAGS: u32 = 1 << 0;
const DQI_BGRACE: u32 = 1 << 1;
const DQI_IGRACE: u32 = 1 << 2;

#[derive(Clone, Debug, Default)]
struct QuotaEntry {
    bhard: i64,
    bsoft: i64,
    ihard: i64,
    isoft: i64,
    curspace: i64,
    curinodes: i64,
    btime: i64,
    itime: i64,
}

#[derive(Clone, Debug)]
struct QuotaTypeState {
    enabled: bool,
    entries: BTreeMap<u32, QuotaEntry>,
}

impl Default for QuotaTypeState {
    fn default() -> Self {
        Self {
            enabled: false,
            entries: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug)]
struct MountQuotaState {
    mount_point: String,
    device: String,
    usr: QuotaTypeState,
    grp: QuotaTypeState,
    info: DqInfo,
}

impl MountQuotaState {
    fn new(device: &str, mount_point: &str) -> Self {
        Self {
            mount_point: mount_point.to_string(),
            device: device.to_string(),
            usr: QuotaTypeState::default(),
            grp: QuotaTypeState::default(),
            info: DqInfo {
                dqi_bgrace: 7 * 86_400,
                dqi_igrace: 7 * 86_400,
                dqi_flags: 0,
                dqi_valid: DQI_BGRACE | DQI_IGRACE,
            },
        }
    }

    fn type_state(&self, quota_type: u32) -> &QuotaTypeState {
        if quota_type == GRPQUOTA {
            &self.grp
        } else {
            &self.usr
        }
    }

    fn type_state_mut(&mut self, quota_type: u32) -> &mut QuotaTypeState {
        if quota_type == GRPQUOTA {
            &mut self.grp
        } else {
            &mut self.usr
        }
    }
}

struct QuotaRegistry {
    /// Keyed by mount point path (`/`, `/mnt/data`, …).
    by_mount: BTreeMap<String, MountQuotaState>,
    /// Device/special path → mount point.
    device_to_mount: BTreeMap<String, String>,
}

impl QuotaRegistry {
    const fn new() -> Self {
        Self {
            by_mount: BTreeMap::new(),
            device_to_mount: BTreeMap::new(),
        }
    }

    fn register(&mut self, device: &str, mount_point: &str) {
        let mount_point = normalize_mount(mount_point);
        let device = device.to_string();
        self.by_mount
            .entry(mount_point.clone())
            .or_insert_with(|| MountQuotaState::new(&device, &mount_point));
        if !device.is_empty() {
            self.device_to_mount.insert(device, mount_point);
        }
    }

    fn unregister(&mut self, mount_point: &str) {
        let mount_point = normalize_mount(mount_point);
        if let Some(state) = self.by_mount.remove(&mount_point) {
            self.device_to_mount.retain(|_, mp| mp != &mount_point);
            let _ = state;
        }
    }

    fn resolve_special(&self, special: &str) -> Option<&MountQuotaState> {
        let special = special.trim();
        if special.is_empty() {
            return None;
        }
        if let Some(mp) = self.device_to_mount.get(special) {
            return self.by_mount.get(mp);
        }
        let mp = normalize_mount(special);
        self.by_mount.get(&mp)
    }

    fn resolve_special_mut(&mut self, special: &str) -> Option<&mut MountQuotaState> {
        let mp = self.resolve_mount_key(special)?;
        self.by_mount.get_mut(&mp)
    }

    fn resolve_mount_key(&self, special: &str) -> Option<String> {
        let special = special.trim();
        if special.is_empty() {
            return None;
        }
        if let Some(mp) = self.device_to_mount.get(special) {
            return Some(mp.clone());
        }
        let mp = normalize_mount(special);
        if self.by_mount.contains_key(&mp) {
            Some(mp)
        } else {
            None
        }
    }
}

static QUOTA: Mutex<QuotaRegistry> = Mutex::new(QuotaRegistry::new());

fn normalize_mount(path: &str) -> String {
    if path == "/" || path.is_empty() {
        String::from("/")
    } else {
        path.trim_end_matches('/').to_string()
    }
}

fn require_root() -> LinuxResult<()> {
    if process_ops::geteuid() != 0 {
        Err(LinuxError::EPERM)
    } else {
        Ok(())
    }
}

fn quota_type_from_cmd(cmd: i32) -> u32 {
    cmd as u32 & 0xff
}

fn entry_for_id<'a>(state: &'a mut QuotaTypeState, id: u32) -> &'a mut QuotaEntry {
    state.entries.entry(id).or_default()
}

fn would_exceed(entry: &QuotaEntry, space_delta: i64, inode_delta: i64) -> bool {
    if space_delta > 0 && entry.bhard > 0 {
        if entry.curspace.saturating_add(space_delta) > entry.bhard {
            return true;
        }
    }
    if inode_delta > 0 && entry.ihard > 0 {
        if entry.curinodes.saturating_add(inode_delta) > entry.ihard {
            return true;
        }
    }
    false
}

fn apply_delta(entry: &mut QuotaEntry, space_delta: i64, inode_delta: i64) {
    entry.curspace = entry.curspace.saturating_add(space_delta);
    if entry.curspace < 0 {
        entry.curspace = 0;
    }
    entry.curinodes = entry.curinodes.saturating_add(inode_delta);
    if entry.curinodes < 0 {
        entry.curinodes = 0;
    }
}

fn entry_to_dqblk(entry: &QuotaEntry) -> FsDiskQuota {
    FsDiskQuota {
        dqb_bhardlimit: entry.bhard,
        dqb_bsoftlimit: entry.bsoft,
        dqb_curspace: entry.curspace,
        dqb_ihardlimit: entry.ihard,
        dqb_isoftlimit: entry.isoft,
        dqb_curinodes: entry.curinodes,
        dqb_btime: entry.btime,
        dqb_itime: entry.itime,
        dqb_valid: QIF_BLIMITS | QIF_SPACE | QIF_ILIMITS | QIF_INODES | QIF_BTIME | QIF_ITIME,
    }
}

fn copy_to_user<T: Copy>(addr: *mut u8, value: &T) -> LinuxResult<()> {
    if addr.is_null() {
        return Err(LinuxError::EFAULT);
    }
    let bytes = crate::linux_compat::as_bytes(value);
    UserSpaceMemory::copy_to_user(addr as u64, bytes).map_err(|_| LinuxError::EFAULT)?;
    Ok(())
}

fn copy_from_user<T: Copy>(addr: *mut u8, value: &mut T) -> LinuxResult<()> {
    if addr.is_null() {
        return Err(LinuxError::EFAULT);
    }
    let bytes = crate::linux_compat::as_bytes_mut(value);
    UserSpaceMemory::copy_from_user(addr as u64, bytes).map_err(|_| LinuxError::EFAULT)?;
    Ok(())
}

fn special_to_string(special: *const u8) -> LinuxResult<String> {
    if special.is_null() {
        return Err(LinuxError::EFAULT);
    }
    UserSpaceMemory::copy_string_from_user(special as u64, 4096).map_err(|_| LinuxError::EFAULT)
}

// ── Public VFS integration ──────────────────────────────────────────────

/// Initialize quota state and register the root mount.
pub fn init() {
    let mut reg = QUOTA.lock();
    reg.register("rootfs", "/");
    crate::serial_println!("[quota] disk quota subsystem initialized");
}

/// Register a mount point for quota tracking.
pub fn register_mount(device: &str, mount_point: &str) {
    QUOTA.lock().register(device, mount_point);
}

/// Unregister quota state when a filesystem is unmounted.
pub fn unregister_mount(mount_point: &str) {
    QUOTA.lock().unregister(mount_point);
}

/// Resolve the mount point path covering `path`.
pub fn mount_for_path(path: &str) -> String {
    let path = if path.is_empty() { "/" } else { path };
    let reg = QUOTA.lock();
    reg.by_mount
        .keys()
        .filter(|mp| path == mp.as_str() || path.starts_with(&format!("{}/", mp)))
        .max_by_key(|mp| mp.len())
        .cloned()
        .unwrap_or_else(|| String::from("/"))
}

/// Check and apply a block/inode usage change for a file owner on a mount.
pub fn adjust_usage(
    mount_point: &str,
    uid: u32,
    gid: u32,
    space_delta: i64,
    inode_delta: i64,
) -> VfsResult<()> {
    if space_delta == 0 && inode_delta == 0 {
        return Ok(());
    }

    let mount_point = normalize_mount(mount_point);
    let mut reg = QUOTA.lock();
    let Some(state) = reg.by_mount.get_mut(&mount_point) else {
        return Ok(());
    };

    for (quota_type, id, delta_space, delta_inodes) in [
        (USRQUOTA, uid, space_delta, inode_delta),
        (GRPQUOTA, gid, space_delta, inode_delta),
    ] {
        let type_state = state.type_state(quota_type);
        if !type_state.enabled {
            continue;
        }
        let entry = type_state.entries.get(&id).cloned().unwrap_or_default();
        if would_exceed(&entry, delta_space, delta_inodes) {
            return Err(VfsError::DiskQuotaExceeded);
        }
    }

    for (quota_type, id, delta_space, delta_inodes) in [
        (USRQUOTA, uid, space_delta, inode_delta),
        (GRPQUOTA, gid, space_delta, inode_delta),
    ] {
        let type_state = state.type_state_mut(quota_type);
        if !type_state.enabled {
            continue;
        }
        let entry = entry_for_id(type_state, id);
        apply_delta(entry, delta_space, delta_inodes);
    }

    Ok(())
}

/// Reserve space for a write extending a file from `old_size` to `new_size`.
pub fn account_write(
    stat: &Stat,
    mount_point: &str,
    old_size: u64,
    new_size: u64,
) -> VfsResult<()> {
    let delta = new_size as i64 - old_size as i64;
    adjust_usage(mount_point, stat.uid, stat.gid, delta, 0)
}

/// Reserve/release space and inodes for file creation.
pub fn account_create(stat: &Stat, mount_point: &str) -> VfsResult<()> {
    adjust_usage(mount_point, stat.uid, stat.gid, stat.size as i64, 1)
}

/// Release space and inodes when a file is removed.
pub fn account_unlink(stat: &Stat, mount_point: &str) -> VfsResult<()> {
    adjust_usage(mount_point, stat.uid, stat.gid, -(stat.size as i64), -1)
}

// ── quotactl dispatch ───────────────────────────────────────────────────

/// Handle `quotactl(2)` — called from `linux_compat::fs_ops`.
pub fn quotactl(cmd: i32, special: *const u8, id: i32, addr: *mut u8) -> LinuxResult<i32> {
    let cmd_type = cmd & 0xff00;
    let quota_type = quota_type_from_cmd(cmd);

    match cmd_type {
        Q_SYNC => Ok(0),
        Q_GETFMT => {
            copy_to_user(addr, &QFMT_VFS_V0)?;
            Ok(0)
        }
        Q_GETINFO => {
            let special_str = special_to_string(special)?;
            let reg = QUOTA.lock();
            let Some(state) = reg.resolve_special(&special_str) else {
                return Err(LinuxError::ENOENT);
            };
            copy_to_user(addr, &state.info)?;
            Ok(0)
        }
        Q_SETINFO => {
            require_root()?;
            let special_str = special_to_string(special)?;
            let mut dqinfo = DqInfo::default();
            copy_from_user(addr, &mut dqinfo)?;
            let mut reg = QUOTA.lock();
            let Some(state) = reg.resolve_special_mut(&special_str) else {
                return Err(LinuxError::ENOENT);
            };
            if dqinfo.dqi_valid & DQI_BGRACE != 0 {
                state.info.dqi_bgrace = dqinfo.dqi_bgrace;
            }
            if dqinfo.dqi_valid & DQI_IGRACE != 0 {
                state.info.dqi_igrace = dqinfo.dqi_igrace;
            }
            if dqinfo.dqi_valid & DQI_FLAGS != 0 {
                state.info.dqi_flags = dqinfo.dqi_flags;
            }
            state.info.dqi_valid = DQI_BGRACE | DQI_IGRACE | DQI_FLAGS;
            Ok(0)
        }
        Q_GETQUOTA => {
            if id < 0 {
                return Err(LinuxError::EINVAL);
            }
            let special_str = special_to_string(special)?;
            let reg = QUOTA.lock();
            let Some(state) = reg.resolve_special(&special_str) else {
                return Err(LinuxError::ENOENT);
            };
            let type_state = state.type_state(quota_type);
            let entry = type_state.entries.get(&(id as u32));
            let dqblk = entry.map(entry_to_dqblk).unwrap_or_default();
            copy_to_user(addr, &dqblk)?;
            Ok(0)
        }
        Q_SETQUOTA => {
            require_root()?;
            if id < 0 {
                return Err(LinuxError::EINVAL);
            }
            let special_str = special_to_string(special)?;
            let mut dqblk = FsDiskQuota::default();
            copy_from_user(addr, &mut dqblk)?;
            let mut reg = QUOTA.lock();
            let Some(state) = reg.resolve_special_mut(&special_str) else {
                return Err(LinuxError::ENOENT);
            };
            let type_state = state.type_state_mut(quota_type);
            let entry = entry_for_id(type_state, id as u32);
            if dqblk.dqb_valid & QIF_BLIMITS != 0 {
                entry.bhard = dqblk.dqb_bhardlimit;
                entry.bsoft = dqblk.dqb_bsoftlimit;
            }
            if dqblk.dqb_valid & QIF_ILIMITS != 0 {
                entry.ihard = dqblk.dqb_ihardlimit;
                entry.isoft = dqblk.dqb_isoftlimit;
            }
            if dqblk.dqb_valid & QIF_SPACE != 0 {
                entry.curspace = dqblk.dqb_curspace;
            }
            if dqblk.dqb_valid & QIF_INODES != 0 {
                entry.curinodes = dqblk.dqb_curinodes;
            }
            if dqblk.dqb_valid & QIF_BTIME != 0 {
                entry.btime = dqblk.dqb_btime;
            }
            if dqblk.dqb_valid & QIF_ITIME != 0 {
                entry.itime = dqblk.dqb_itime;
            }
            Ok(0)
        }
        Q_QUOTAON => {
            require_root()?;
            if id as u32 != QFMT_VFS_V0 {
                return Err(LinuxError::EINVAL);
            }
            let special_str = special_to_string(special)?;
            let mut reg = QUOTA.lock();
            if reg.resolve_mount_key(&special_str).is_none() {
                // Allow enabling on unknown special by registering it as its own mount key.
                reg.register(&special_str, &special_str);
            }
            let Some(state) = reg.resolve_special_mut(&special_str) else {
                return Err(LinuxError::ENOENT);
            };
            state.type_state_mut(quota_type).enabled = true;
            Ok(0)
        }
        Q_QUOTAOFF => {
            require_root()?;
            let special_str = special_to_string(special)?;
            let mut reg = QUOTA.lock();
            let Some(state) = reg.resolve_special_mut(&special_str) else {
                return Err(LinuxError::ENOENT);
            };
            state.type_state_mut(quota_type).enabled = false;
            Ok(0)
        }
        _ => Err(LinuxError::EINVAL),
    }
}
