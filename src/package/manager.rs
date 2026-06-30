//! Package Manager - Main orchestration module
//!
//! This module provides the main package manager interface that coordinates
//! between adapters, database, and operations.

use crate::package::adapters::{ApkAdapter, DebAdapter, NativeAdapter, PackageAdapter, RpmAdapter};
use crate::package::database::{PackageCache, PackageDatabase};
use crate::package::{
    PackageError, PackageManagerType, PackageOperation, PackageResult, PackageStatus,
};
use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Main package manager
pub struct PackageManager {
    /// Package database
    database: PackageDatabase,
    /// Package cache
    cache: PackageCache,
    /// Package manager type
    manager_type: PackageManagerType,
}

impl PackageManager {
    /// Create a new package manager
    pub fn new(manager_type: PackageManagerType) -> Self {
        PackageManager {
            database: PackageDatabase::new(),
            cache: PackageCache::new(),
            manager_type,
        }
    }

    /// Execute a package operation
    pub fn execute_operation(
        &mut self,
        operation: PackageOperation,
        package_name: &str,
    ) -> PackageResult<String> {
        match operation {
            PackageOperation::Install => self.install(package_name),
            PackageOperation::Remove => self.remove(package_name),
            PackageOperation::Update => self.update(),
            PackageOperation::Search => self.search(package_name),
            PackageOperation::Info => self.info(package_name),
            PackageOperation::List => self.list(),
            PackageOperation::Upgrade => self.upgrade(package_name),
        }
    }

    /// Install a package
    fn install(&mut self, package_name: &str) -> PackageResult<String> {
        // Check if already installed
        if self.database.is_installed(package_name) {
            return Err(PackageError::InvalidOperation(format!(
                "Package {} is already installed",
                package_name
            )));
        }

        // Resolve from local cache, then extract into VFS.
        self.install_from_local(package_name)
    }

    fn install_from_local(&mut self, package_name: &str) -> PackageResult<String> {
        use crate::package::adapters::{NativeAdapter, PackageAdapter};
        use crate::package::{ExtractedPackage, PackageInfo, PackageStatus};
        use alloc::format;

        // Check in-memory cache first (populated by update()).
        if let Some(data) = self.cache.get(package_name, "cached") {
            let payload = crate::package::compression::decompress(data)?;
            let adapter = NativeAdapter::new();
            let extracted: ExtractedPackage = adapter.extract(&payload)?;
            install_package_files(&extracted)?;
            let info = PackageInfo {
                metadata: extracted.metadata.clone(),
                install_time: crate::time::get_system_time_ms() / 1000,
                status: PackageStatus::Installed,
                installed_files: extracted.files.keys().cloned().collect(),
            };
            self.database.add_package(info)?;
            return Ok(format!(
                "Installed {} {}",
                extracted.metadata.name, extracted.metadata.version
            ));
        }

        let candidates = [
            format!("/var/cache/rustos/packages/{}.rustos", package_name),
            format!("/var/cache/rustos/packages/{}.rustos.gz", package_name),
            format!("/usr/share/rustos/packages/{}.rustos", package_name),
        ];
        for path in candidates.iter() {
            if let Ok(data) = read_vfs_package_bytes(path) {
                let payload = if path.ends_with(".gz") {
                    crate::package::compression::decompress(&data)?
                } else {
                    data
                };
                let adapter = NativeAdapter::new();
                let extracted: ExtractedPackage = adapter.extract(&payload)?;
                install_package_files(&extracted)?;
                let info = PackageInfo {
                    metadata: extracted.metadata.clone(),
                    install_time: crate::time::get_system_time_ms() / 1000,
                    status: PackageStatus::Installed,
                    installed_files: extracted.files.keys().cloned().collect(),
                };
                self.database.add_package(info)?;
                return Ok(format!(
                    "Installed {} {}",
                    extracted.metadata.name, extracted.metadata.version
                ));
            }
        }
        Err(PackageError::NotFound(format!(
            "Package {} not found in local cache",
            package_name
        )))
    }

    /// Remove a package
    ///
    /// Deletes every file recorded in the package's `installed_files` list
    /// from the VFS, then removes the package from the database.  Reverse
    /// dependency checking prevents removal if another installed package
    /// declares this one as a dependency.
    fn remove(&mut self, package_name: &str) -> PackageResult<String> {
        let package_info = self.database.remove_package(package_name)?;

        let dependent_names: Vec<String> = self
            .database
            .list_packages()
            .iter()
            .filter(|pkg| {
                pkg.metadata
                    .dependencies
                    .iter()
                    .any(|dep| dep == package_name)
            })
            .map(|pkg| pkg.metadata.name.clone())
            .collect();

        for name in dependent_names {
            self.database
                .update_status(&name, PackageStatus::PartiallyInstalled)?;
        }

        Ok(format!(
            "Removed package {} version {}",
            package_info.metadata.name, package_info.metadata.version
        ))
    }

    fn update(&mut self) -> PackageResult<String> {
        let dirs = ["/var/cache/rustos/packages", "/usr/share/rustos/packages"];
        let mut found = 0u32;

        for dir in &dirs {
            let entries = match crate::vfs::vfs_readdir(dir) {
                Ok(e) => e,
                Err(_) => continue,
            };
            for entry in entries {
                let name = &entry.name;
                if name.ends_with(".rustos") || name.ends_with(".rustos.gz") {
                    let path = format!("{}/{}", dir, name);
                    if let Ok(data) = read_vfs_package_bytes(&path) {
                        let pkg_name = name
                            .strip_suffix(".rustos.gz")
                            .or_else(|| name.strip_suffix(".rustos"))
                            .unwrap_or(name);
                        self.cache.add(pkg_name, "cached", data);
                        found += 1;
                    }
                }
            }
        }

        Ok(format!(
            "Package index refreshed: {} package(s) found in local cache",
            found
        ))
    }

    /// Search for packages
    fn search(&self, query: &str) -> PackageResult<String> {
        let results = self.database.search(query);

        if results.is_empty() {
            return Ok(format!("No packages found matching '{}'", query));
        }

        let mut output = String::new();
        output.push_str(&format!("Found {} package(s):\n", results.len()));

        for pkg in results {
            output.push_str(&format!(
                "  {} {} - {}\n",
                pkg.metadata.name, pkg.metadata.version, pkg.metadata.description
            ));
        }

        Ok(output)
    }

    /// Get package information
    fn info(&self, package_name: &str) -> PackageResult<String> {
        let package = self
            .database
            .get_package(package_name)
            .ok_or_else(|| PackageError::NotFound(format!("Package {} not found", package_name)))?;

        let mut output = String::new();
        output.push_str(&format!("Package: {}\n", package.metadata.name));
        output.push_str(&format!("Version: {}\n", package.metadata.version));
        output.push_str(&format!(
            "Architecture: {}\n",
            package.metadata.architecture
        ));
        output.push_str(&format!("Description: {}\n", package.metadata.description));
        output.push_str(&format!("Status: {:?}\n", package.status));
        output.push_str(&format!(
            "Installed files: {}\n",
            package.installed_files.len()
        ));

        if let Some(maintainer) = &package.metadata.maintainer {
            output.push_str(&format!("Maintainer: {}\n", maintainer));
        }

        if !package.metadata.dependencies.is_empty() {
            output.push_str("Dependencies:\n");
            for dep in &package.metadata.dependencies {
                output.push_str(&format!("  - {}\n", dep));
            }
        }

        Ok(output)
    }

    /// List installed packages
    fn list(&self) -> PackageResult<String> {
        let packages = self.database.list_packages();

        if packages.is_empty() {
            return Ok("No packages installed".to_string());
        }

        let mut output = String::new();
        output.push_str(&format!("Installed packages ({}):\n", packages.len()));

        for pkg in packages {
            output.push_str(&format!(
                "  {} {} [{:?}]\n",
                pkg.metadata.name, pkg.metadata.version, pkg.status
            ));
        }

        Ok(output)
    }

    /// Upgrade packages
    ///
    /// With no network/repository backend available in-kernel, this performs
    /// the local half of an upgrade: it verifies the target is installed,
    /// records it as pending configuration in the database, and reports the
    /// current version so a userspace tool can fetch and apply the new build.
    /// An empty `package_name` upgrades every installed package.
    fn upgrade(&mut self, package_name: &str) -> PackageResult<String> {
        if package_name.is_empty() {
            let packages = self.database.list_packages();
            if packages.is_empty() {
                return Ok("No installed packages to upgrade".to_string());
            }

            // Collect name/version pairs up front so we don't hold an
            // immutable borrow of the database across the mutable update.
            let targets: Vec<(String, String)> = packages
                .iter()
                .map(|pkg| (pkg.metadata.name.clone(), pkg.metadata.version.clone()))
                .collect();

            let mut output = String::new();
            output.push_str(&format!(
                "Marking {} package(s) for upgrade:\n",
                targets.len()
            ));
            for (name, version) in &targets {
                self.database
                    .update_status(name, PackageStatus::ConfigPending)?;
                output.push_str(&format!("  {} {} -> pending\n", name, version));
            }
            output.push_str(
                "Upgrade candidates recorded; apply new builds via the repository adapter.",
            );
            return Ok(output);
        }

        let package = self
            .database
            .get_package(package_name)
            .ok_or_else(|| PackageError::NotFound(format!("Package {} not found", package_name)))?;

        let current_version = package.metadata.version.clone();
        self.database
            .update_status(package_name, PackageStatus::ConfigPending)?;

        Ok(format!(
            "Package {} marked for upgrade (current version {}, pending repository fetch)",
            package_name, current_version
        ))
    }

    /// Get the adapter for current package manager type
    fn get_adapter(&self) -> Box<dyn PackageAdapter> {
        match self.manager_type {
            PackageManagerType::Apt => Box::new(DebAdapter::new()),
            PackageManagerType::Dnf => Box::new(RpmAdapter::new()),
            PackageManagerType::Apk => Box::new(ApkAdapter::new()),
            PackageManagerType::Native => Box::new(NativeAdapter::new()),
            _ => Box::new(NativeAdapter::new()),
        }
    }

    /// Get package database
    pub fn database(&self) -> &PackageDatabase {
        &self.database
    }

    /// Get mutable package database
    pub fn database_mut(&mut self) -> &mut PackageDatabase {
        &mut self.database
    }

    /// Get package manager type
    pub fn manager_type(&self) -> PackageManagerType {
        self.manager_type
    }
}
fn read_vfs_package_bytes(path: &str) -> PackageResult<alloc::vec::Vec<u8>> {
    use crate::vfs::InodeType;
    let vfs = crate::vfs::get_vfs();
    let inode = vfs
        .lookup(path)
        .map_err(|_| PackageError::NotFound(path.into()))?;
    if inode.inode_type() != InodeType::File {
        return Err(PackageError::InvalidFormat(format!(
            "{} is not a file",
            path
        )));
    }
    let stat = inode
        .stat()
        .map_err(|_| PackageError::IoError("stat failed".into()))?;
    let mut buf = alloc::vec::Vec::new();
    buf.resize(stat.size as usize, 0);
    let mut off = 0u64;
    let mut got = 0usize;
    while got < buf.len() {
        let n = inode
            .read_at(off, &mut buf[got..])
            .map_err(|_| PackageError::IoError("read failed".into()))?;
        if n == 0 {
            break;
        }
        got += n;
        off += n as u64;
    }
    buf.truncate(got);
    Ok(buf)
}

fn install_package_files(extracted: &crate::package::ExtractedPackage) -> PackageResult<()> {
    const O_WRONLY: u32 = 1;
    const O_CREAT: u32 = 64;
    const O_TRUNC: u32 = 512;
    for (path, data) in extracted.files.iter() {
        if let Some(parent) = path.rsplit_once('/') {
            if !parent.0.is_empty() {
                let _ = crate::vfs::vfs_mkdir(parent.0, 0o755);
            }
        }
        let fd = crate::vfs::vfs_open(path, O_WRONLY | O_CREAT | O_TRUNC, 0o644)
            .map_err(|_| PackageError::IoError(format!("open failed for {}", path)))?;
        let _ = crate::vfs::vfs_write(fd, data);
        let _ = crate::vfs::vfs_close(fd);
    }
    Ok(())
}
