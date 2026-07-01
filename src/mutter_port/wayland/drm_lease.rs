//! Wayland DRM Lease module
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-drm-lease.h
//!
//! Manages DRM leases for Wayland clients requesting exclusive device access.
//! Tracks lease requests (connector lists, lessee IDs) and manages the
//! wp_drm_lease_v1 protocol lifecycle.

use alloc::vec::Vec;

/// A single DRM lease request from a client.
///
/// In the C original, `MetaWaylandDrmLeaseRequest` holds the list of
/// connectors the client wants to lease and the lessee ID assigned by
/// the DRM driver. A full implementation would call drmModeCreateLease
/// to create the lease and return the DRM fd to the client.
#[derive(Debug)]
pub struct MetaWaylandDrmLeaseRequest {
    /// List of DRM connector IDs requested for the lease.
    pub connectors: Vec<u32>,
    /// Lessee ID assigned by the DRM driver (0 if not yet leased).
    pub lessee_id: u32,
    /// wl_resource pointer for the wp_drm_lease_request_v1 object.
    pub resource: *mut core::ffi::c_void,
    /// Whether the lease has been granted.
    pub granted: bool,
}

impl MetaWaylandDrmLeaseRequest {
    /// Create a new lease request with an empty connector list.
    pub fn new(resource: *mut core::ffi::c_void) -> Self {
        MetaWaylandDrmLeaseRequest {
            connectors: Vec::new(),
            lessee_id: 0,
            resource,
            granted: false,
        }
    }

    /// Add a connector ID to the lease request.
    pub fn add_connector(&mut self, connector_id: u32) {
        self.connectors.push(connector_id);
    }

    /// Remove a connector ID from the lease request.
    pub fn remove_connector(&mut self, connector_id: u32) {
        self.connectors.retain(|&c| c != connector_id);
    }

    /// Get the list of requested connector IDs.
    pub fn get_connectors(&self) -> &[u32] {
        &self.connectors
    }

    /// Number of connectors in the lease request.
    pub fn connector_count(&self) -> usize {
        self.connectors.len()
    }

    /// Set the lessee ID assigned by the DRM driver.
    pub fn set_lessee_id(&mut self, lessee_id: u32) {
        self.lessee_id = lessee_id;
    }

    /// Get the lessee ID.
    pub fn get_lessee_id(&self) -> u32 {
        self.lessee_id
    }

    /// Mark the lease as granted.
    pub fn grant(&mut self, lessee_id: u32) {
        self.lessee_id = lessee_id;
        self.granted = true;
    }

    /// Mark the lease as revoked.
    pub fn revoke(&mut self) {
        self.granted = false;
        self.lessee_id = 0;
    }

    /// Check whether the lease has been granted.
    pub fn is_granted(&self) -> bool {
        self.granted
    }
}

/// DRM lease manager tracking lease requests and protocol resources.
///
/// In the C original, `MetaWaylandDrmLeaseManager` holds the DRM device
/// fd, the list of leaseable connectors, and tracks active leases. A
/// full implementation would register the wp_drm_lease_v1 global and
/// coordinate with the DRM backend.
#[derive(Debug)]
pub struct MetaWaylandDrmLeaseManager {
    /// Active and pending lease requests.
    pub lease_requests: Vec<MetaWaylandDrmLeaseRequest>,
    /// Whether the protocol global has been registered.
    pub initialized: bool,
}

impl MetaWaylandDrmLeaseManager {
    /// Create a new empty DRM lease manager.
    pub fn new() -> Self {
        MetaWaylandDrmLeaseManager {
            lease_requests: Vec::new(),
            initialized: false,
        }
    }

    /// Add a new lease request to the manager.
    pub fn add_lease_request(&mut self, request: MetaWaylandDrmLeaseRequest) {
        self.lease_requests.push(request);
    }

    /// Remove a lease request by its wl_resource pointer.
    pub fn remove_lease_request(&mut self, resource: *mut core::ffi::c_void) {
        self.lease_requests.retain(|r| r.resource != resource);
    }

    /// Number of active and pending lease requests.
    pub fn lease_count(&self) -> usize {
        self.lease_requests.len()
    }

    /// Number of granted leases.
    pub fn granted_count(&self) -> usize {
        self.lease_requests.iter().filter(|r| r.granted).count()
    }

    /// Clear all lease requests.
    pub fn clear(&mut self) {
        self.lease_requests.clear();
    }

    /// Initialize DRM lease manager for the compositor.
    /// A full implementation would enumerate DRM connectors, mark
    /// non-desktop connectors as leaseable, and register the
    /// wp_drm_lease_v1 global via wl_global_create.
    pub fn init(&mut self, compositor: *mut core::ffi::c_void) {
        if compositor.is_null() {
            return;
        }
        self.initialized = true;
    }
}

impl Default for MetaWaylandDrmLeaseManager {
    fn default() -> Self {
        Self::new()
    }
}
