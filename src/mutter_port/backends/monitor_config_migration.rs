//! Monitor Config Migration
//!
//! Native Rust implementation (no direct Mutter C counterpart).
//! Handles migration of monitor configurations between kernel/display versions.

use alloc::string::String;

/// Monitor Configuration Migration Handler.
/// Manages version upgrades and compatibility for monitor configuration formats.
#[derive(Debug, Clone)]
pub struct MonitorConfigMigration {
    pub from_version: u32,
    pub to_version: u32,
    pub migration_applied: bool,
}

impl MonitorConfigMigration {
    /// Create a new migration handler.
    pub fn new(from_version: u32, to_version: u32) -> Self {
        MonitorConfigMigration {
            from_version,
            to_version,
            migration_applied: false,
        }
    }
}

impl Default for MonitorConfigMigration {
    fn default() -> Self {
        MonitorConfigMigration {
            from_version: 1,
            to_version: 1,
            migration_applied: true,
        }
    }
}
