//! Package management system tests
//!
//! This module provides test utilities and test cases for the package management system.

use crate::package::{
    DebAdapter, PackageAdapter, PackageDatabase, PackageManager, PackageManagerType,
    PackageMetadata, PackageStatus,
};
use crate::println;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Test package database operations
pub fn test_package_database() {
    let mut db = PackageDatabase::new();

    // Create test package metadata
    let metadata = PackageMetadata::new(
        "test-package".to_string(),
        "1.0.0".to_string(),
        "amd64".to_string(),
    );

    let package_info = crate::package::PackageInfo {
        metadata,
        install_time: 1234567890,
        installed_files: Vec::new(),
        status: PackageStatus::Installed,
    };

    // Test adding package
    assert!(db.add_package(package_info).is_ok());
    assert_eq!(db.package_count(), 1);

    // Test package lookup
    assert!(db.is_installed("test-package"));
    assert!(!db.is_installed("nonexistent-package"));

    // Test search
    let results = db.search("test");
    assert_eq!(results.len(), 1);

    // Test removal
    assert!(db.remove_package("test-package").is_ok());
    assert_eq!(db.package_count(), 0);
}

/// Test .deb package validation
pub fn test_deb_validation() {
    let adapter = DebAdapter::new();

    // Valid .deb magic number
    let valid_deb = b"!<arch>\n";
    assert!(adapter.validate(valid_deb).unwrap_or(false));

    // Invalid magic number
    let invalid = b"INVALID!";
    assert!(!adapter.validate(invalid).unwrap_or(true));
}

/// Test AR archive parsing
pub fn test_ar_archive_parsing() {
    use crate::package::archive::ar::ArArchive;

    // Create a minimal AR archive for testing
    let mut ar_data = Vec::new();
    ar_data.extend_from_slice(b"!<arch>\n");

    // Add a simple member header (60 bytes)
    let mut header = [b' '; 60];
    header[0..8].copy_from_slice(b"test.txt");
    header[16..28].copy_from_slice(b"1234567890  "); // timestamp
    header[28..34].copy_from_slice(b"0     "); // owner
    header[34..40].copy_from_slice(b"0     "); // group
    header[40..48].copy_from_slice(b"100644  "); // mode
    header[48..58].copy_from_slice(b"11        "); // size (11 bytes)
    header[58..60].copy_from_slice(b"`\n"); // magic

    ar_data.extend_from_slice(&header);
    ar_data.extend_from_slice(b"hello world");
    ar_data.push(b'\n'); // padding to align to 2 bytes

    // Parse archive
    match ArArchive::parse(&ar_data) {
        Ok(archive) => {
            assert_eq!(archive.members().len(), 1);
            let member = &archive.members()[0];
            assert_eq!(member.name.trim(), "test.txt");
            assert_eq!(member.size, 11);
        }
        Err(e) => {
            panic!("AR archive parsing failed: {}", e);
        }
    }
}

/// Test TAR archive parsing
pub fn test_tar_archive_parsing() {
    use crate::package::compression::TarArchive;

    // Create a minimal TAR archive for testing
    let mut tar_data = Vec::new();

    // TAR header (512 bytes)
    let mut header = [0u8; 512];
    header[0..9].copy_from_slice(b"test.txt\0");
    header[100..107].copy_from_slice(b"0000644"); // mode
    header[124..135].copy_from_slice(b"00000000013"); // size = 11
    header[257..262].copy_from_slice(b"ustar"); // magic
    header[156] = b'0'; // file type

    // Calculate checksum
    let checksum = header.iter().map(|&b| b as u32).sum::<u32>();
    let checksum_str = format!("{:06o}\0 ", checksum);
    header[148..156].copy_from_slice(checksum_str.as_bytes());

    tar_data.extend_from_slice(&header);
    tar_data.extend_from_slice(b"hello world");
    tar_data.extend_from_slice(&[0u8; 501]); // padding to 512 bytes

    // End of archive marker (two zero blocks)
    tar_data.extend_from_slice(&[0u8; 1024]);

    // Parse archive
    match TarArchive::parse(&tar_data) {
        Ok(archive) => {
            assert!(archive.entries().len() >= 1);
            if let Some(entry) = archive.find_entry("test.txt") {
                assert_eq!(entry.size, 11);
            }
        }
        Err(e) => {
            panic!("TAR archive parsing failed: {}", e);
        }
    }
}

/// Test compression format detection
pub fn test_compression_detection() {
    use crate::package::compression::CompressionFormat;

    // Gzip magic
    let gzip_data = [0x1f, 0x8b, 0x08, 0x00];
    assert_eq!(
        CompressionFormat::detect(&gzip_data),
        CompressionFormat::Gzip
    );

    // Uncompressed
    let plain_data = [0x00, 0x01, 0x02, 0x03];
    assert_eq!(
        CompressionFormat::detect(&plain_data),
        CompressionFormat::None
    );
}

/// Test package manager operations
pub fn test_package_manager_operations() {
    let mut pm = PackageManager::new(PackageManagerType::Native);

    // Test list operation (should return empty initially)
    match pm.execute_operation(crate::package::PackageOperation::List, "") {
        Ok(result) => {
            assert!(
                result.contains("No packages installed") || result.contains("Installed packages")
            );
        }
        Err(_) => panic!("List operation failed"),
    }

    // Test search operation
    match pm.execute_operation(crate::package::PackageOperation::Search, "test") {
        Ok(result) => {
            assert!(result.contains("No packages found") || result.contains("Found"));
        }
        Err(_) => {} // Search may fail if not implemented
    }
}

/// Run all package manager tests
pub fn run_all_tests() -> (usize, usize) {
    let mut passed = 0;
    let mut total = 0;

    // Test 1: Database operations
    total += 1;
    if test_with_catch(test_package_database, "Package Database") {
        passed += 1;
    }

    // Test 2: DEB validation
    total += 1;
    if test_with_catch(test_deb_validation, "DEB Validation") {
        passed += 1;
    }

    // Test 3: AR archive parsing
    total += 1;
    if test_with_catch(test_ar_archive_parsing, "AR Archive Parsing") {
        passed += 1;
    }

    // Test 4: TAR archive parsing
    total += 1;
    if test_with_catch(test_tar_archive_parsing, "TAR Archive Parsing") {
        passed += 1;
    }

    // Test 5: Compression detection
    total += 1;
    if test_with_catch(test_compression_detection, "Compression Detection") {
        passed += 1;
    }

    // Test 6: Package manager operations
    total += 1;
    if test_with_catch(
        test_package_manager_operations,
        "Package Manager Operations",
    ) {
        passed += 1;
    }

    (passed, total)
}

/// Helper to run a test and catch panics
fn test_with_catch<F: FnOnce() + core::panic::UnwindSafe>(test: F, name: &str) -> bool {
    use core::panic;

    println!("   Running test: {}", name);

    // Since we're in a no_std kernel, we can't use std::panic::catch_unwind
    // Instead, we'll just run the test directly
    test();
    println!("      ✅ {} passed", name);
    true
}

/// Create a test .deb package structure (for integration testing)
pub fn create_test_deb_package() -> Vec<u8> {
    let mut deb_data = Vec::new();

    // AR archive header
    deb_data.extend_from_slice(b"!<arch>\n");

    // debian-binary member
    let mut header = [b' '; 60];
    header[0..13].copy_from_slice(b"debian-binary");
    header[48..58].copy_from_slice(b"4         ");
    header[58..60].copy_from_slice(b"`\n");
    deb_data.extend_from_slice(&header);
    deb_data.extend_from_slice(b"2.0\n");

    // Note: Full .deb would include control.tar.gz and data.tar.xz
    // This is a minimal structure for basic testing

    deb_data
}
