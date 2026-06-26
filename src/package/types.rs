//! Package type definitions and metadata structures

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

/// Package metadata information
#[derive(Debug, Clone)]
pub struct PackageMetadata {
    /// Package name
    pub name: String,
    /// Package version
    pub version: String,
    /// Package architecture (e.g., amd64, arm64)
    pub architecture: String,
    /// Package description
    pub description: String,
    /// Package maintainer
    pub maintainer: Option<String>,
    /// Package homepage
    pub homepage: Option<String>,
    /// Package dependencies
    pub dependencies: Vec<String>,
    /// Package size in bytes
    pub size: u64,
    /// Installed size in bytes
    pub installed_size: u64,
}

impl PackageMetadata {
    /// Create a new package metadata instance
    pub fn new(name: String, version: String, architecture: String) -> Self {
        PackageMetadata {
            name,
            version,
            architecture,
            description: String::new(),
            maintainer: None,
            homepage: None,
            dependencies: Vec::new(),
            size: 0,
            installed_size: 0,
        }
    }
}

/// Extracted package information
#[derive(Debug, Clone)]
pub struct ExtractedPackage {
    /// Package metadata
    pub metadata: PackageMetadata,
    /// Extracted files with their paths
    pub files: BTreeMap<String, Vec<u8>>,
    /// Control scripts (postinst, prerm, etc.)
    pub scripts: BTreeMap<String, String>,
}

impl ExtractedPackage {
    /// Create a new extracted package
    pub fn new(metadata: PackageMetadata) -> Self {
        ExtractedPackage {
            metadata,
            files: BTreeMap::new(),
            scripts: BTreeMap::new(),
        }
    }

    /// Add a file to the extracted package
    pub fn add_file(&mut self, path: String, data: Vec<u8>) {
        self.files.insert(path, data);
    }

    /// Add a control script
    pub fn add_script(&mut self, name: String, content: String) {
        self.scripts.insert(name, content);
    }
}

/// Package information for installed packages
#[derive(Debug, Clone)]
pub struct PackageInfo {
    /// Package metadata
    pub metadata: PackageMetadata,
    /// Installation timestamp (Unix timestamp)
    pub install_time: u64,
    /// List of installed files
    pub installed_files: Vec<String>,
    /// Installation status
    pub status: PackageStatus,
}

/// Package installation status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageStatus {
    /// Package is installed and operational
    Installed,
    /// Package is partially installed
    PartiallyInstalled,
    /// Package is marked for removal
    MarkedForRemoval,
    /// Package configuration is pending
    ConfigPending,
}

/// Package dependency information
#[derive(Debug, Clone)]
pub struct Dependency {
    /// Package name
    pub name: String,
    /// Version constraint (e.g., ">= 1.0", "= 2.3.4")
    pub version_constraint: Option<String>,
    /// Whether this is an optional dependency
    pub optional: bool,
}

impl Dependency {
    /// Create a new dependency
    pub fn new(name: String) -> Self {
        Dependency {
            name,
            version_constraint: None,
            optional: false,
        }
    }

    /// Create a new dependency with version constraint
    pub fn with_version(name: String, version: String) -> Self {
        Dependency {
            name,
            version_constraint: Some(version),
            optional: false,
        }
    }
}

/// Repository information
#[derive(Debug, Clone)]
pub struct Repository {
    /// Repository name
    pub name: String,
    /// Repository URL
    pub url: String,
    /// Repository type
    pub repo_type: RepositoryType,
    /// Whether the repository is enabled
    pub enabled: bool,
}

/// Repository type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryType {
    /// APT repository
    Apt,
    /// DNF/YUM repository
    Dnf,
    /// Pacman repository
    Pacman,
    /// APK repository
    Apk,
    /// Custom repository
    Custom,
}
