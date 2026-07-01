//! DRM lease support for sharing display devices.
//!
//! DRM leases allow unprivileged applications to control specific display resources
//! without requiring full DRM master privileges. Ported from `meta-drm-lease.c`.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

/// DRM lease object
#[derive(Debug, Clone)]
pub struct DrmLease {
    /// Lease ID from kernel
    pub lease_id: u32,
    /// FD for this lease
    pub lease_fd: i32,
    /// Resources included in this lease
    pub resources: Vec<u32>,
}

impl DrmLease {
    /// Create a new lease
    pub fn new(lease_id: u32, lease_fd: i32) -> Self {
        DrmLease {
            lease_id,
            lease_fd,
            resources: Vec::new(),
        }
    }

    /// Add a resource to this lease
    pub fn add_resource(&mut self, resource_id: u32) {
        if !self.resources.contains(&resource_id) {
            self.resources.push(resource_id);
        }
    }

    /// Get resources in this lease
    pub fn get_resources(&self) -> &[u32] {
        &self.resources
    }

    /// Get number of resources
    pub fn get_resource_count(&self) -> usize {
        self.resources.len()
    }

    /// Get lease file descriptor
    pub fn get_fd(&self) -> i32 {
        self.lease_fd
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lease_creation() {
        let lease = DrmLease::new(1, 10);
        assert_eq!(lease.lease_id, 1);
        assert_eq!(lease.lease_fd, 10);
        assert_eq!(lease.get_resource_count(), 0);
    }

    #[test]
    fn test_add_resource() {
        let mut lease = DrmLease::new(1, 10);
        lease.add_resource(100);
        lease.add_resource(101);
        assert_eq!(lease.get_resource_count(), 2);
    }
}
