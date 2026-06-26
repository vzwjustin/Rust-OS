//! Package database management
//!
//! This module manages the database of installed packages and their metadata.

use crate::package::{PackageError, PackageInfo, PackageMetadata, PackageResult, PackageStatus};
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Package database for tracking installed packages
pub struct PackageDatabase {
    /// Installed packages indexed by name
    packages: BTreeMap<String, PackageInfo>,
}

impl PackageDatabase {
    /// Create a new empty package database
    pub fn new() -> Self {
        PackageDatabase {
            packages: BTreeMap::new(),
        }
    }

    /// Add a package to the database
    pub fn add_package(&mut self, info: PackageInfo) -> PackageResult<()> {
        let name = info.metadata.name.clone();
        self.packages.insert(name, info);
        Ok(())
    }

    /// Remove a package from the database
    pub fn remove_package(&mut self, name: &str) -> PackageResult<PackageInfo> {
        self.packages
            .remove(name)
            .ok_or_else(|| PackageError::NotFound(format!("Package {} not found", name)))
    }

    /// Get package information
    pub fn get_package(&self, name: &str) -> Option<&PackageInfo> {
        self.packages.get(name)
    }

    /// Check if a package is installed
    pub fn is_installed(&self, name: &str) -> bool {
        self.packages.contains_key(name)
    }

    /// List all installed packages
    pub fn list_packages(&self) -> Vec<&PackageInfo> {
        self.packages.values().collect()
    }

    /// Search for packages by name pattern
    pub fn search(&self, pattern: &str) -> Vec<&PackageInfo> {
        self.packages
            .values()
            .filter(|info| info.metadata.name.contains(pattern))
            .collect()
    }

    /// Get total number of installed packages
    pub fn package_count(&self) -> usize {
        self.packages.len()
    }

    /// Update package status
    pub fn update_status(&mut self, name: &str, status: PackageStatus) -> PackageResult<()> {
        let package = self
            .packages
            .get_mut(name)
            .ok_or_else(|| PackageError::NotFound(format!("Package {} not found", name)))?;
        package.status = status;
        Ok(())
    }
}

impl Default for PackageDatabase {
    fn default() -> Self {
        Self::new()
    }
}

/// Package cache for downloaded packages
pub struct PackageCache {
    /// Cached package data indexed by package identifier (name-version)
    cache: BTreeMap<String, Vec<u8>>,
}

impl PackageCache {
    /// Create a new package cache
    pub fn new() -> Self {
        PackageCache {
            cache: BTreeMap::new(),
        }
    }

    /// Add package data to cache
    pub fn add(&mut self, name: &str, version: &str, data: Vec<u8>) {
        let key = format!("{}-{}", name, version);
        self.cache.insert(key, data);
    }

    /// Get package data from cache
    pub fn get(&self, name: &str, version: &str) -> Option<&[u8]> {
        let key = format!("{}-{}", name, version);
        self.cache.get(&key).map(|v| v.as_slice())
    }

    /// Remove package from cache
    pub fn remove(&mut self, name: &str, version: &str) -> Option<Vec<u8>> {
        let key = format!("{}-{}", name, version);
        self.cache.remove(&key)
    }

    /// Clear all cached packages
    pub fn clear(&mut self) {
        self.cache.clear();
    }

    /// Get cache size in bytes
    pub fn size(&self) -> usize {
        self.cache.values().map(|v| v.len()).sum()
    }
}

impl Default for PackageCache {
    fn default() -> Self {
        Self::new()
    }
}
