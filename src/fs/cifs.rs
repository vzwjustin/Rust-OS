//! SMB/CIFS client mount registry.
//!
//! Tracks connection parameters and remote-to-local path mappings for CIFS
//! mounts. Wire protocol I/O is deferred; this module integrates with the VFS
//! mount table and `linux_compat` mount syscall path.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::RwLock;

use super::{FileSystemType, FsError, FsResult};

/// Parsed CIFS connection parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CifsConnection {
    pub server: String,
    pub share: String,
    pub username: String,
    pub domain: String,
    pub port: u16,
    pub vers: String,
    pub sec: String,
}

/// Active CIFS mount: remote share mapped to a local mount point.
#[derive(Debug, Clone)]
pub struct CifsMount {
    pub connection: CifsConnection,
    pub mount_point: String,
    pub remote_root: String,
    pub read_only: bool,
    pub mtime: u64,
}

struct CifsClientState {
    mounts: BTreeMap<String, CifsMount>,
}

impl CifsClientState {
    const fn new() -> Self {
        Self {
            mounts: BTreeMap::new(),
        }
    }
}

static CIFS_CLIENT: RwLock<CifsClientState> = RwLock::new(CifsClientState::new());

/// Parse `//server/share` or `\\server\share` source strings.
pub fn parse_server_share(source: &str) -> FsResult<(String, String)> {
    let trimmed = source.trim();
    let path = trimmed
        .strip_prefix("//")
        .or_else(|| trimmed.strip_prefix("\\\\"))
        .ok_or(FsError::InvalidArgument)?;
    let mut parts = path.split(['/', '\\']).filter(|p| !p.is_empty());
    let server = parts.next().ok_or(FsError::InvalidArgument)?.to_string();
    let share = parts.next().ok_or(FsError::InvalidArgument)?.to_string();
    if server.is_empty() || share.is_empty() {
        return Err(FsError::InvalidArgument);
    }
    Ok((server, share))
}

/// Parse comma-separated mount options (`user=`, `domain=`, `port=`, etc.).
pub fn parse_mount_options(data: &str) -> CifsConnection {
    let mut username = String::new();
    let mut domain = String::new();
    let mut port = 445u16;
    let mut vers = String::from("3.0");
    let mut sec = String::from("ntlmssp");

    for part in data.split(',') {
        let part = part.trim();
        if let Some(v) = part.strip_prefix("username=") {
            username = String::from(v.trim());
        } else if let Some(v) = part.strip_prefix("user=") {
            username = String::from(v.trim());
        } else if let Some(v) = part.strip_prefix("domain=") {
            domain = String::from(v.trim());
        } else if let Some(v) = part.strip_prefix("port=") {
            if let Ok(p) = v.trim().parse::<u16>() {
                port = p;
            }
        } else if let Some(v) = part.strip_prefix("vers=") {
            vers = String::from(v.trim());
        } else if let Some(v) = part.strip_prefix("sec=") {
            sec = String::from(v.trim());
        }
    }

    CifsConnection {
        server: String::new(),
        share: String::new(),
        username,
        domain,
        port,
        vers,
        sec,
    }
}

/// Register a CIFS mount from mount(2) arguments.
pub fn mount_from_options(
    source: &str,
    mount_point: &str,
    options: Option<&str>,
    read_only: bool,
) -> FsResult<()> {
    if mount_point.is_empty() {
        return Err(FsError::InvalidArgument);
    }
    let (server, share) = parse_server_share(source)?;
    let mut conn = parse_mount_options(options.unwrap_or(""));
    conn.server = server;
    conn.share = share;

    let mount = CifsMount {
        connection: conn,
        mount_point: String::from(mount_point),
        remote_root: String::from("/"),
        read_only,
        mtime: crate::time::uptime_ns(),
    };

    CIFS_CLIENT
        .write()
        .mounts
        .insert(String::from(mount_point), mount);
    Ok(())
}

/// Unregister a CIFS mount point.
pub fn unmount(mount_point: &str) -> FsResult<()> {
    if CIFS_CLIENT.write().mounts.remove(mount_point).is_some() {
        Ok(())
    } else {
        Err(FsError::NotFound)
    }
}

/// List active CIFS mounts.
pub fn list_mounts() -> Vec<CifsMount> {
    CIFS_CLIENT.read().mounts.values().cloned().collect()
}

/// Resolve a local path to its remote equivalent on a CIFS mount.
pub fn map_local_to_remote(mount_point: &str, local_path: &str) -> FsResult<String> {
    let state = CIFS_CLIENT.read();
    let mount = state.mounts.get(mount_point).ok_or(FsError::NotFound)?;
    let rel = local_path
        .strip_prefix(mount_point)
        .unwrap_or(local_path)
        .trim_start_matches('/');
    if rel.is_empty() {
        Ok(mount.remote_root.clone())
    } else {
        Ok(format!(
            "{}/{}",
            mount.remote_root.trim_end_matches('/'),
            rel
        ))
    }
}

/// Lookup mount covering `path`.
pub fn mount_for_path(path: &str) -> Option<CifsMount> {
    CIFS_CLIENT
        .read()
        .mounts
        .iter()
        .filter(|(mp, _)| path == mp.as_str() || path.starts_with(&format!("{}/", mp)))
        .max_by_key(|(mp, _)| mp.len())
        .map(|(_, m)| m.clone())
}

pub fn fs_type() -> FileSystemType {
    FileSystemType::Cifs
}

/// Initialize CIFS client registry.
pub fn init() {
    CIFS_CLIENT.write().mounts.clear();
}
