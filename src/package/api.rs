//! Package repository API adapters
//!
//! This module provides adapters for interacting with package repositories
//! and app stores to download and query package information.
//!
//! All adapters read from the kernel VFS.  Package indices live at
//! `/var/lib/pkg/<repo>/index` as pipe-delimited lines:
//!   `name|version|arch|description|size|installed_size`
//! Cached package files live at `/var/cache/packages/<name>-<version>.<ext>`.

use crate::package::{PackageError, PackageMetadata, PackageResult, Repository};
use crate::vfs::{vfs_close, vfs_open, vfs_read, vfs_readdir};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

const O_RDONLY: u32 = 0;

fn index_path(repo_dir: &str) -> String {
    format!("/var/lib/pkg/{}/index", repo_dir)
}

fn cache_path(name: &str, version: &str) -> String {
    format!("/var/cache/packages/{}-{}.deb", name, version)
}

fn read_vfs_file(path: &str) -> PackageResult<Vec<u8>> {
    let fd = vfs_open(path, O_RDONLY, 0)
        .map_err(|_| PackageError::IoError(format!("Failed to open {}", path)))?;
    let mut data = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let n = vfs_read(fd, &mut buf)
            .map_err(|_| PackageError::IoError(format!("Read error on {}", path)))?;
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n]);
    }
    let _ = vfs_close(fd);
    Ok(data)
}

fn parse_index_line(line: &str) -> Option<PackageMetadata> {
    let parts: Vec<&str> = line.split('|').collect();
    if parts.len() < 3 {
        return None;
    }
    let mut meta = PackageMetadata::new(
        parts[0].to_string(),
        parts[1].to_string(),
        parts[2].to_string(),
    );
    if parts.len() > 3 {
        meta.description = parts[3].to_string();
    }
    if parts.len() > 4 {
        meta.size = parts[4].parse().unwrap_or(0);
    }
    if parts.len() > 5 {
        meta.installed_size = parts[5].parse().unwrap_or(0);
    }
    Some(meta)
}

fn parse_index(data: &[u8]) -> Vec<PackageMetadata> {
    let text = core::str::from_utf8(data).unwrap_or("");
    text.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                None
            } else {
                parse_index_line(trimmed)
            }
        })
        .collect()
}

fn load_index(repo_dir: &str) -> PackageResult<Vec<PackageMetadata>> {
    let path = index_path(repo_dir);
    let data = read_vfs_file(&path)?;
    Ok(parse_index(&data))
}

fn load_cache(name: &str, version: &str) -> PackageResult<Vec<u8>> {
    let path = cache_path(name, version);
    read_vfs_file(&path)
}

fn list_cached_packages() -> Vec<String> {
    vfs_readdir("/var/cache/packages")
        .map(|entries| {
            entries
                .into_iter()
                .map(|e| e.name)
                .filter(|n| !n.starts_with('.'))
                .collect()
        })
        .unwrap_or_default()
}

fn rebuild_index(repo_dir: &str, ext: &str) -> PackageResult<()> {
    let entries = vfs_readdir("/var/cache/packages")
        .map_err(|_| PackageError::IoError("Cannot read /var/cache/packages".to_string()))?;

    let mut lines = Vec::new();
    for entry in entries.iter() {
        if entry.name.starts_with('.') {
            continue;
        }
        if !entry.name.ends_with(ext) {
            continue;
        }
        let stem = &entry.name[..entry.name.len() - ext.len()];
        if let Some(idx) = stem.rfind('-') {
            let name = &stem[..idx];
            let version = &stem[idx + 1..];
            lines.push(format!("{}|{}|all|cached package|0|0", name, version));
        }
    }

    let index_path = index_path(repo_dir);
    let dir_path = format!("/var/lib/pkg/{}", repo_dir);

    let _ = vfs_readdir(&dir_path);

    const O_WRONLY: u32 = 1;
    const O_CREAT: u32 = 64;
    const O_TRUNC: u32 = 512;

    let fd = vfs_open(&index_path, O_WRONLY | O_CREAT | O_TRUNC, 0o644)
        .map_err(|_| PackageError::IoError(format!("Cannot create {}", index_path)))?;

    let mut all_bytes = Vec::new();
    for line in &lines {
        all_bytes.extend_from_slice(line.as_bytes());
        all_bytes.push(b'\n');
    }
    let _ = crate::vfs::vfs_write(fd, &all_bytes);
    let _ = vfs_close(fd);

    Ok(())
}

/// Trait for repository API adapters
pub trait RepositoryAdapter {
    /// Search for packages in the repository
    fn search(&self, query: &str) -> PackageResult<Vec<PackageMetadata>>;

    /// Get package metadata
    fn get_metadata(&self, name: &str, version: Option<&str>) -> PackageResult<PackageMetadata>;

    /// Download package
    fn download(&self, name: &str, version: &str) -> PackageResult<Vec<u8>>;

    /// Update repository index
    fn update_index(&self) -> PackageResult<()>;

    /// Get repository information
    fn get_repository_info(&self) -> &Repository;
}

/// APT repository adapter (Debian/Ubuntu)
pub struct AptRepositoryAdapter {
    repository: Repository,
}

impl AptRepositoryAdapter {
    /// Create a new APT repository adapter
    pub fn new(url: String) -> Self {
        AptRepositoryAdapter {
            repository: Repository {
                name: "APT Repository".to_string(),
                url,
                repo_type: crate::package::types::RepositoryType::Apt,
                enabled: true,
            },
        }
    }

    fn repo_dir(&self) -> &'static str {
        "apt"
    }
}

impl RepositoryAdapter for AptRepositoryAdapter {
    fn search(&self, query: &str) -> PackageResult<Vec<PackageMetadata>> {
        let index = load_index(self.repo_dir())?;
        let results: Vec<PackageMetadata> = index
            .into_iter()
            .filter(|m| m.name.contains(query) || m.description.contains(query))
            .collect();
        Ok(results)
    }

    fn get_metadata(&self, name: &str, version: Option<&str>) -> PackageResult<PackageMetadata> {
        let index = load_index(self.repo_dir())?;
        for m in index.iter() {
            if m.name == name {
                if let Some(v) = version {
                    if m.version == v {
                        return Ok(m.clone());
                    }
                } else {
                    return Ok(m.clone());
                }
            }
        }
        Err(PackageError::NotFound(format!(
            "Package {} not found in APT index",
            name
        )))
    }

    fn download(&self, name: &str, version: &str) -> PackageResult<Vec<u8>> {
        load_cache(name, version)
    }

    fn update_index(&self) -> PackageResult<()> {
        rebuild_index(self.repo_dir(), ".deb")
    }

    fn get_repository_info(&self) -> &Repository {
        &self.repository
    }
}

/// DNF repository adapter (Fedora/RHEL)
pub struct DnfRepositoryAdapter {
    repository: Repository,
}

impl DnfRepositoryAdapter {
    /// Create a new DNF repository adapter
    pub fn new(url: String) -> Self {
        DnfRepositoryAdapter {
            repository: Repository {
                name: "DNF Repository".to_string(),
                url,
                repo_type: crate::package::types::RepositoryType::Dnf,
                enabled: true,
            },
        }
    }

    fn repo_dir(&self) -> &'static str {
        "dnf"
    }
}

impl RepositoryAdapter for DnfRepositoryAdapter {
    fn search(&self, query: &str) -> PackageResult<Vec<PackageMetadata>> {
        let index = load_index(self.repo_dir())?;
        let results: Vec<PackageMetadata> = index
            .into_iter()
            .filter(|m| m.name.contains(query) || m.description.contains(query))
            .collect();
        Ok(results)
    }

    fn get_metadata(&self, name: &str, version: Option<&str>) -> PackageResult<PackageMetadata> {
        let index = load_index(self.repo_dir())?;
        for m in index.iter() {
            if m.name == name {
                if let Some(v) = version {
                    if m.version == v {
                        return Ok(m.clone());
                    }
                } else {
                    return Ok(m.clone());
                }
            }
        }
        Err(PackageError::NotFound(format!(
            "Package {} not found in DNF index",
            name
        )))
    }

    fn download(&self, name: &str, version: &str) -> PackageResult<Vec<u8>> {
        let path = format!("/var/cache/packages/{}-{}.rpm", name, version);
        read_vfs_file(&path)
    }

    fn update_index(&self) -> PackageResult<()> {
        rebuild_index(self.repo_dir(), ".rpm")
    }

    fn get_repository_info(&self) -> &Repository {
        &self.repository
    }
}

/// App store adapter trait
pub trait AppStoreAdapter {
    /// Search for apps in the store
    fn search_apps(&self, query: &str) -> PackageResult<Vec<PackageMetadata>>;

    /// Get app details
    fn get_app_details(&self, app_id: &str) -> PackageResult<PackageMetadata>;

    /// Download app package
    fn download_app(&self, app_id: &str) -> PackageResult<Vec<u8>>;

    /// Get featured apps
    fn get_featured(&self) -> PackageResult<Vec<PackageMetadata>>;
}

/// Generic app store adapter
pub struct GenericAppStoreAdapter {
    name: String,
    api_url: String,
}

impl GenericAppStoreAdapter {
    /// Create a new app store adapter
    pub fn new(name: String, api_url: String) -> Self {
        GenericAppStoreAdapter { name, api_url }
    }

    fn repo_dir(&self) -> String {
        "appstore".to_string()
    }
}

impl AppStoreAdapter for GenericAppStoreAdapter {
    fn search_apps(&self, query: &str) -> PackageResult<Vec<PackageMetadata>> {
        let index = load_index(&self.repo_dir())?;
        let results: Vec<PackageMetadata> = index
            .into_iter()
            .filter(|m| m.name.contains(query) || m.description.contains(query))
            .collect();
        Ok(results)
    }

    fn get_app_details(&self, app_id: &str) -> PackageResult<PackageMetadata> {
        let index = load_index(&self.repo_dir())?;
        for m in index.iter() {
            if m.name == app_id {
                return Ok(m.clone());
            }
        }
        Err(PackageError::NotFound(format!(
            "App {} not found in store index",
            app_id
        )))
    }

    fn download_app(&self, app_id: &str) -> PackageResult<Vec<u8>> {
        let cached = list_cached_packages();
        for filename in cached.iter() {
            if filename.starts_with(app_id) {
                let path = format!("/var/cache/packages/{}", filename);
                return read_vfs_file(&path);
            }
        }
        Err(PackageError::NotFound(format!(
            "App package {} not found in cache",
            app_id
        )))
    }

    fn get_featured(&self) -> PackageResult<Vec<PackageMetadata>> {
        let index = load_index(&self.repo_dir())?;
        Ok(index.into_iter().take(10).collect())
    }
}
