//! Debian package (.deb) adapter
//!
//! This adapter provides experimental support for .deb package format.
//! .deb packages use the ar archive format containing:
//! - debian-binary: Format version
//! - control.tar.gz: Metadata and control scripts
//! - data.tar.gz/xz: Actual package files
//!
//! **EXPERIMENTAL**: This is a foundational implementation. Full functionality
//! requires archive extraction support (ar, tar, gzip) and filesystem operations.

use crate::package::adapters::PackageAdapter;
use crate::package::archive::ar::ArArchive;
use crate::package::compression::{decompress, TarArchive};
use crate::package::{ExtractedPackage, PackageError, PackageMetadata, PackageResult};
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Debian package adapter
pub struct DebAdapter;

impl DebAdapter {
    /// Create a new Debian package adapter
    pub fn new() -> Self {
        DebAdapter
    }

    /// Parse control file content
    fn parse_control_file(&self, content: &str) -> PackageResult<PackageMetadata> {
        let mut name = String::new();
        let mut version = String::new();
        let mut architecture = String::from("amd64");
        let mut description = String::new();
        let mut maintainer = None;
        let mut homepage = None;
        let mut dependencies = Vec::new();
        let mut size = 0u64;
        let mut installed_size = 0u64;

        for line in content.lines() {
            if line.is_empty() {
                continue;
            }

            if let Some(colon_pos) = line.find(':') {
                let key = &line[..colon_pos].trim();
                let value = line[colon_pos + 1..].trim();

                match *key {
                    "Package" => name = value.to_string(),
                    "Version" => version = value.to_string(),
                    "Architecture" => architecture = value.to_string(),
                    "Description" => description = value.to_string(),
                    "Maintainer" => maintainer = Some(value.to_string()),
                    "Homepage" => homepage = Some(value.to_string()),
                    "Depends" => {
                        // Parse dependencies (simplified)
                        for dep in value.split(',') {
                            let dep_name = dep
                                .trim()
                                .split_whitespace()
                                .next()
                                .unwrap_or("")
                                .to_string();
                            if !dep_name.is_empty() {
                                dependencies.push(dep_name);
                            }
                        }
                    }
                    "Size" => {
                        size = value.parse().unwrap_or(0);
                    }
                    "Installed-Size" => {
                        installed_size = value.parse::<u64>().unwrap_or(0) * 1024;
                        // Convert KB to bytes
                    }
                    _ => {}
                }
            }
        }

        if name.is_empty() || version.is_empty() {
            return Err(PackageError::InvalidFormat(
                "Missing required fields in control file".to_string(),
            ));
        }

        let mut metadata = PackageMetadata::new(name, version, architecture);
        metadata.description = description;
        metadata.maintainer = maintainer;
        metadata.homepage = homepage;
        metadata.dependencies = dependencies;
        metadata.size = size;
        metadata.installed_size = installed_size;

        Ok(metadata)
    }
}

impl PackageAdapter for DebAdapter {
    fn extract(&self, data: &[u8]) -> PackageResult<ExtractedPackage> {
        // Validate .deb format (ar archive)
        if !self.validate(data)? {
            return Err(PackageError::InvalidFormat(
                ".deb file format validation failed".to_string(),
            ));
        }

        // Parse ar archive
        let ar_archive = ArArchive::parse(data)?;

        // Extract metadata from control archive
        let metadata = self.parse_metadata(data)?;
        let mut package = ExtractedPackage::new(metadata);

        // Find and extract control scripts
        let control_compressed = ar_archive
            .find_member("control.tar.gz")
            .or_else(|| ar_archive.find_member("control.tar.xz"))
            .ok_or_else(|| {
                PackageError::InvalidFormat("Missing control archive in .deb".to_string())
            })?;

        let control_tar = decompress(control_compressed)?;
        let control_archive = TarArchive::parse(&control_tar)?;

        // Extract control scripts (postinst, prerm, postrm, preinst)
        for entry in control_archive.entries() {
            let path = entry.path.trim_start_matches("./");
            if path.starts_with("post") || path.starts_with("pre") || path == "config" {
                if let Ok(content) = core::str::from_utf8(&entry.data) {
                    package.add_script(path.to_string(), content.to_string());
                }
            }
        }

        // Find and extract data archive
        let data_compressed = ar_archive
            .find_member("data.tar.gz")
            .or_else(|| ar_archive.find_member("data.tar.xz"))
            .or_else(|| ar_archive.find_member("data.tar.bz2"))
            .ok_or_else(|| {
                PackageError::InvalidFormat("Missing data archive in .deb".to_string())
            })?;

        let data_tar = decompress(data_compressed)?;
        let data_archive = TarArchive::parse(&data_tar)?;

        // Extract all data files
        for entry in data_archive.entries() {
            package.add_file(entry.path.clone(), entry.data.clone());
        }

        Ok(package)
    }

    fn parse_metadata(&self, data: &[u8]) -> PackageResult<PackageMetadata> {
        // Parse ar archive to get control file
        let ar_archive = ArArchive::parse(data)?;

        // Find control.tar.gz or control.tar.xz
        let control_compressed = ar_archive
            .find_member("control.tar.gz")
            .or_else(|| ar_archive.find_member("control.tar.xz"))
            .ok_or_else(|| PackageError::InvalidFormat("Missing control archive".to_string()))?;

        // Decompress the control archive
        let control_tar = decompress(control_compressed)?;

        // Extract tar archive
        let tar_archive = TarArchive::parse(&control_tar)?;

        // Find the control file
        let control_entry = tar_archive
            .find_entry("control")
            .or_else(|| tar_archive.find_entry("./control"))
            .ok_or_else(|| {
                PackageError::InvalidFormat("Missing control file in control archive".to_string())
            })?;

        // Parse control file content
        let control_content = core::str::from_utf8(&control_entry.data).map_err(|_| {
            PackageError::InvalidFormat("Invalid UTF-8 in control file".to_string())
        })?;

        self.parse_control_file(control_content)
    }

    fn validate(&self, data: &[u8]) -> PackageResult<bool> {
        // Check ar archive signature
        if data.len() < 8 {
            return Ok(false);
        }

        // .deb files are ar archives starting with "!<arch>\n"
        let signature = &data[0..8];
        let expected = b"!<arch>\n";

        Ok(signature == expected)
    }

    fn format_name(&self) -> &str {
        "Debian Package (.deb)"
    }
}

impl Default for DebAdapter {
    fn default() -> Self {
        Self::new()
    }
}
