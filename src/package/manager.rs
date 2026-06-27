//! Package Manager - Main orchestration module
//!
//! This module provides the main package manager interface that coordinates
//! between adapters, database, and operations.

use crate::package::adapters::{ApkAdapter, DebAdapter, NativeAdapter, PackageAdapter, RpmAdapter};
use crate::package::database::{PackageCache, PackageDatabase};
use crate::package::{PackageError, PackageManagerType, PackageOperation, PackageResult};
use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};

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

        // This is experimental - actual installation requires:
        // 1. Download package from repository
        // 2. Extract and validate package
        // 3. Resolve dependencies
        // 4. Install files to filesystem
        // 5. Run post-installation scripts
        // 6. Update database

        Err(PackageError::NotImplemented(format!(
            "Package installation requires network stack and filesystem support. \
                    See docs/LINUX_APP_SUPPORT.md for implementation requirements."
        )))
    }

    /// Remove a package
    fn remove(&mut self, package_name: &str) -> PackageResult<String> {
        let _package_info = self.database.remove_package(package_name)?;

        // This is experimental - actual removal requires:
        // 1. Check for reverse dependencies
        // 2. Run pre-removal scripts
        // 3. Remove installed files
        // 4. Update database

        Ok(format!(
            "Package {} marked for removal (experimental)",
            package_name
        ))
    }

    /// Update package database
    fn update(&mut self) -> PackageResult<String> {
        Err(PackageError::NotImplemented(
            "Package database update requires network and repository API support".to_string(),
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
    fn upgrade(&mut self, package_name: &str) -> PackageResult<String> {
        Err(PackageError::NotImplemented(format!(
            "Package upgrade not yet implemented for {}",
            package_name
        )))
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
