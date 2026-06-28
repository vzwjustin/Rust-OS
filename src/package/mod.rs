//! Experimental Package Management System for RustOS
//!
//! This module provides experimental adapters for working with Linux packages,
//! APIs, and app stores. This is a foundational implementation that provides
//! the architecture and interfaces for package management.
//!
//! **EXPERIMENTAL STATUS**: This implementation is experimental and provides
//! basic structure for package management. Full functionality requires:
//! - Dynamic linker implementation
//! - C library port (glibc/musl)
//! - Extended syscall coverage
//! - Filesystem support (ext4, etc.)
//! - Userspace tools (bash, coreutils, tar, gzip)
//!
//! See docs/LINUX_APP_SUPPORT.md for detailed requirements.

extern crate alloc;

use alloc::string::String;
use core::fmt;

pub mod adapters;
pub mod api;
pub mod archive;
pub mod compression;
pub mod database;
pub mod manager;
pub mod syscalls;
pub mod types;

#[cfg(test)]
pub mod tests;

pub use adapters::{DebAdapter, PackageAdapter};
pub use database::PackageDatabase;
pub use manager::PackageManager;
pub use syscalls::{handle_package_syscall, init_package_manager};
pub use types::*;

/// Result type for package operations
pub type PackageResult<T> = Result<T, PackageError>;

/// Errors that can occur during package operations
#[derive(Debug, Clone)]
pub enum PackageError {
    /// Package not found
    NotFound(String),
    /// Invalid package format
    InvalidFormat(String),
    /// Dependency resolution failed
    DependencyError(String),
    /// I/O error
    IoError(String),
    /// Archive extraction failed
    ExtractionError(String),
    /// Installation failed
    InstallError(String),
    /// Not implemented
    NotImplemented(String),
    /// Invalid operation
    InvalidOperation(String),
}

impl fmt::Display for PackageError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            PackageError::NotFound(msg) => write!(f, "Package not found: {}", msg),
            PackageError::InvalidFormat(msg) => write!(f, "Invalid package format: {}", msg),
            PackageError::DependencyError(msg) => write!(f, "Dependency error: {}", msg),
            PackageError::IoError(msg) => write!(f, "I/O error: {}", msg),
            PackageError::ExtractionError(msg) => write!(f, "Extraction error: {}", msg),
            PackageError::InstallError(msg) => write!(f, "Installation error: {}", msg),
            PackageError::NotImplemented(msg) => write!(f, "Not implemented: {}", msg),
            PackageError::InvalidOperation(msg) => write!(f, "Invalid operation: {}", msg),
        }
    }
}

/// Package operation types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageOperation {
    /// Install a package
    Install,
    /// Remove a package
    Remove,
    /// Update package database
    Update,
    /// Search for packages
    Search,
    /// Get package information
    Info,
    /// List installed packages
    List,
    /// Upgrade packages
    Upgrade,
}

/// Package manager backend type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageManagerType {
    /// Debian/Ubuntu APT (.deb)
    Apt,
    /// Fedora/RHEL DNF (.rpm)
    Dnf,
    /// Arch Linux Pacman
    Pacman,
    /// Alpine Linux APK
    Apk,
    /// Native RustOS packages
    Native,
    /// Unknown/Custom
    Unknown,
}

impl fmt::Display for PackageManagerType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            PackageManagerType::Apt => write!(f, "APT"),
            PackageManagerType::Dnf => write!(f, "DNF"),
            PackageManagerType::Pacman => write!(f, "Pacman"),
            PackageManagerType::Apk => write!(f, "APK"),
            PackageManagerType::Native => write!(f, "RustOS Native"),
            PackageManagerType::Unknown => write!(f, "Unknown"),
        }
    }
}
