//! Wayland Linux DRM Syncobj — explicit synchronization with DRM sync objects.
//!
//! Implements explicit synchronization for Wayland surfaces using DRM syncobj
//! timeline semantics. Tracks sync points across GPU/CPU execution.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-linux-drm-syncobj.h
//!
//! # DRM syncobj ioctls
//!
//! The DRM syncobj protocol is backed by a small set of ioctls on a DRM
//! render node file descriptor (e.g. `/dev/dri/renderD128`):
//!
//! - `DRM_IOCTL_SYNCOBJ_CREATE` — allocate a new syncobj, returning a
//!   32-bit handle. Flags select binary (`DRM_SYNCOBJ_CREATE_SIGNALED`)
//!   or timeline semantics.
//! - `DRM_IOCTL_SYNCOBJ_DESTROY` — free a syncobj handle.
//! - `DRM_IOCTL_SYNCOBJ_HANDLE_TO_FD` / `DRM_IOCTL_SYNCOBJ_FD_TO_HANDLE` —
//!   translate between a DRM syncobj handle and a pollable file
//!   descriptor that can be passed across processes (e.g. to a client
//!   via `wl_drm.syncobj` or as a Wayland array fd).
//! - `DRM_IOCTL_SYNCOBJ_TRANSFER` — copy signal state between two
//!   syncobjs (used when bridging a binary sync fd into a timeline
//!   point).
//! - `DRM_IOCTL_SYNCOBJ_TIMELINE_SIGNAL` — signal one or more points on
//!   a timeline syncobj. Used by `meta_wayland_sync_timeline_set_sync_point`
//!   after the compositor's GPU work for a frame completes.
//! - `DRM_IOCTL_SYNCOBJ_QUERY` — query the set of signalled points on a
//!   timeline syncobj; used to discover the current timeline point when
//!   validating explicit-sync state.
//! - `DRM_IOCTL_SYNCOBJ_EVENTFD` (Linux >= 5.16) — register an eventfd
//!   that is signalled when a given timeline point is reached. Used by
//!   `meta_wayland_sync_timeline_get_eventfd` so the compositor can poll
//!   for GPU completion without busy-waiting.
//!
//! In this no_std port we cannot issue ioctls (no file-descriptor table,
//! no DRM driver binding), so the functions below track the sync state
//! in memory and document the ioctl side effects. A full kernel-side
//! implementation would open a DRM render node and issue the ioctls
//! above via `ioctl(2)`.

use alloc::collections::BTreeSet;
use alloc::vec::Vec;

/// A DRM sync point tied to a timeline (GPU execution fence).
#[derive(Debug, Clone)]
pub struct MetaWaylandSyncPoint {
    /// Reference to the syncobj timeline.
    pub timeline: *mut core::ffi::c_void,
    /// Numeric sync point on the timeline.
    pub sync_point: u64,
}

impl MetaWaylandSyncPoint {
    /// Create a new sync point.
    pub fn new(timeline: *mut core::ffi::c_void, sync_point: u64) -> Self {
        Self {
            timeline,
            sync_point,
        }
    }
}

impl Default for MetaWaylandSyncPoint {
    fn default() -> Self {
        Self {
            timeline: core::ptr::null_mut(),
            sync_point: 0,
        }
    }
}

/// A DRM syncobj timeline managing multiple sync points.
#[derive(Debug)]
pub struct MetaWaylandSyncobjTimeline {
    /// The underlying DRM timeline.
    pub timeline: *mut core::ffi::c_void,
    /// Set of sync points that have been signalled on this timeline.
    /// Kept sorted via BTreeSet so we can answer "is point N reached?"
    /// in O(log n) and report the latest reached point cheaply.
    pub signalled_points: BTreeSet<u64>,
}

impl MetaWaylandSyncobjTimeline {
    /// Create a new syncobj timeline with no signalled points.
    pub fn new(timeline: *mut core::ffi::c_void) -> Self {
        Self {
            timeline,
            signalled_points: BTreeSet::new(),
        }
    }

    /// Whether a given sync point has been signalled on this timeline.
    pub fn is_point_signalled(&self, point: u64) -> bool {
        self.signalled_points.contains(&point)
    }

    /// The highest signalled point, or 0 if none have been signalled.
    pub fn latest_signalled_point(&self) -> u64 {
        self.signalled_points
            .iter()
            .copied()
            .next_back()
            .unwrap_or(0)
    }
}

impl Default for MetaWaylandSyncobjTimeline {
    fn default() -> Self {
        Self::new(core::ptr::null_mut())
    }
}

/// Per-surface explicit-sync state tracked by the compositor.
///
/// Mirrors `MetaWaylandSyncobjSurfaceState` in the C code: the acquire
/// and release sync points attached to a pending buffer commit, plus
/// the set of timeline points the compositor is still waiting on.
#[derive(Debug, Default)]
pub struct MetaWaylandSyncobjSurfaceState {
    /// MetaWaylandSurface pointer this state belongs to.
    pub surface: *mut core::ffi::c_void,
    /// Acquire sync point: the client asserts the buffer is not
    /// readable until this point on `acquire_timeline` is signalled.
    pub acquire_point: Option<MetaWaylandSyncPoint>,
    /// Release sync point: the compositor must signal this point on
    /// `release_timeline` once it is done presenting the buffer.
    pub release_point: Option<MetaWaylandSyncPoint>,
    /// Timeline points the compositor is still waiting on before it can
    /// commit this surface's state. A commit is valid only when every
    /// pending point has been signalled.
    pub pending_points: BTreeSet<u64>,
}

impl MetaWaylandSyncobjSurfaceState {
    /// Create empty sync state for a surface.
    pub fn new(surface: *mut core::ffi::c_void) -> Self {
        Self {
            surface,
            acquire_point: None,
            release_point: None,
            pending_points: BTreeSet::new(),
        }
    }

    /// Set the acquire sync point for the pending commit. Adds the
    /// point to the pending set so the compositor waits for it.
    pub fn set_acquire_point(&mut self, timeline: *mut core::ffi::c_void, point: u64) {
        self.acquire_point = Some(MetaWaylandSyncPoint::new(timeline, point));
        self.pending_points.insert(point);
    }

    /// Set the release sync point for the pending commit.
    pub fn set_release_point(&mut self, timeline: *mut core::ffi::c_void, point: u64) {
        self.release_point = Some(MetaWaylandSyncPoint::new(timeline, point));
    }

    /// Mark a timeline point as signalled, removing it from the pending
    /// set. Returns true if the point was pending.
    pub fn mark_point_signalled(&mut self, point: u64) -> bool {
        self.pending_points.remove(&point)
    }

    /// Whether all pending acquire points have been signalled and the
    /// commit can proceed.
    pub fn is_ready(&self) -> bool {
        self.pending_points.is_empty()
    }
}

/// Global DRM syncobj manager state held by the compositor.
#[derive(Debug, Default)]
pub struct MetaWaylandDrmSyncobjManager {
    /// Per-surface sync state. Linear lookup; the C code uses a
    /// GHashTable keyed by MetaWaylandSurface.
    pub sync_surfaces: Vec<MetaWaylandSyncobjSurfaceState>,
}

impl MetaWaylandDrmSyncobjManager {
    /// Create a new empty manager.
    pub fn new() -> Self {
        Self {
            sync_surfaces: Vec::new(),
        }
    }

    /// Get the sync state for a surface, creating an empty entry if
    /// none exists yet.
    pub fn get_or_create_state(
        &mut self,
        surface: *mut core::ffi::c_void,
    ) -> &mut MetaWaylandSyncobjSurfaceState {
        if !self
            .sync_surfaces
            .iter()
            .any(|s| core::ptr::eq(s.surface, surface))
        {
            self.sync_surfaces
                .push(MetaWaylandSyncobjSurfaceState::new(surface));
        }
        self.sync_surfaces
            .iter_mut()
            .find(|s| core::ptr::eq(s.surface, surface))
            .expect("entry just inserted")
    }

    /// Look up the sync state for a surface, if any.
    pub fn get_state(
        &self,
        surface: *mut core::ffi::c_void,
    ) -> Option<&MetaWaylandSyncobjSurfaceState> {
        self.sync_surfaces
            .iter()
            .find(|s| core::ptr::eq(s.surface, surface))
    }

    /// Remove the sync state for a surface (e.g. on surface destroy).
    /// Returns true if state existed.
    pub fn remove_state(&mut self, surface: *mut core::ffi::c_void) -> bool {
        let before = self.sync_surfaces.len();
        self.sync_surfaces
            .retain(|s| !core::ptr::eq(s.surface, surface));
        self.sync_surfaces.len() != before
    }

    /// Number of surfaces with explicit-sync state.
    pub fn surface_count(&self) -> usize {
        self.sync_surfaces.len()
    }
}

/// Validate explicit sync for a wayland surface.
///
/// Checks that the surface's pending sync state is consistent: any
/// acquire point must be present and all pending timeline points must
/// have been signalled (i.e. the surface is ready to commit). A full
/// implementation would additionally issue `DRM_IOCTL_SYNCOBJ_QUERY`
/// against the acquire timeline to confirm the GPU has reached the
/// acquire point, and reject commits whose release timeline is invalid.
/// Without DRM access we validate the in-memory pending set only.
pub fn meta_wayland_surface_explicit_sync_validate(
    surface: *mut core::ffi::c_void,
    state: *mut core::ffi::c_void,
) -> bool {
    if surface.is_null() {
        return false;
    }
    // `state` is a MetaWaylandSyncobjSurfaceState* in the full build.
    // We treat null state as "no explicit sync requested" which is valid.
    if state.is_null() {
        return true;
    }
    // Safety: the caller is responsible for passing a valid pointer to
    // a MetaWaylandSyncobjSurfaceState. We only read the pending set.
    let s = unsafe { &*(state as *const MetaWaylandSyncobjSurfaceState) };
    // An acquire point must have been set if any release point was set,
    // and all pending acquire points must be signalled.
    if s.release_point.is_some() && s.acquire_point.is_none() {
        return false;
    }
    s.is_ready()
}

/// Initialize DRM syncobj support for the compositor.
///
/// A full implementation would open a DRM render node, create the
/// `wp_linux_drm_syncobj_manager_v1` global via `wl_global_create`, and
/// register bind/destroy handlers that allocate per-surface timeline
/// syncobjs using `DRM_IOCTL_SYNCOBJ_CREATE`. Without libwayland and a
/// DRM fd, this is a no-op; callers should construct a
/// `MetaWaylandDrmSyncobjManager` to track state instead.
pub fn meta_wayland_drm_syncobj_init(_compositor: *mut core::ffi::c_void) {
    // With libwayland + DRM:
    //   compositor->drm_fd = open("/dev/dri/renderD128", O_RDWR | O_CLOEXEC);
    //   wl_global_create(compositor->wl_display,
    //     &wp_linux_drm_syncobj_manager_v1_interface, 1, compositor,
    //     bind_syncobj_manager);
    // The bind handler exposes `get_surface` / `create_timeline` requests;
    // `create_timeline` wraps `DRM_IOCTL_SYNCOBJ_CREATE` with timeline
    // flags and stores the handle in a MetaWaylandSyncobjTimeline.
}

/// Set a sync point on a timeline with an optional sync FD.
///
/// In a full implementation this would:
///   1. Import the sync FD into a DRM syncobj via
///      `DRM_IOCTL_SYNCOBJ_FD_TO_HANDLE` (if `sync_fd >= 0`).
///   2. Transfer the signal state from that binary syncobj into the
///      timeline at `sync_point` via `DRM_IOCTL_SYNCOBJ_TRANSFER`.
///   3. Signal the timeline point directly via
///      `DRM_IOCTL_SYNCOBJ_TIMELINE_SIGNAL` if no sync FD is given.
///   4. Close the imported syncobj with `DRM_IOCTL_SYNCOBJ_DESTROY`.
/// Without DRM access we record the point as signalled in the timeline's
/// in-memory set and return Ok.
pub fn meta_wayland_sync_timeline_set_sync_point(
    timeline: *mut MetaWaylandSyncobjTimeline,
    sync_point: u64,
    _sync_fd: i32,
) -> Result<(), &'static str> {
    if timeline.is_null() {
        return Err("null timeline");
    }
    // Safety: caller guarantees the pointer is a valid
    // MetaWaylandSyncobjTimeline.
    let t = unsafe { &mut *timeline };
    t.signalled_points.insert(sync_point);
    Ok(())
}

/// Get an eventfd for a sync point on a timeline.
///
/// In a full implementation this would:
///   1. Create an eventfd via `eventfd(0, EFD_CLOEXEC | EFD_NONBLOCK)`.
///   2. Register it with the DRM syncobj via
///      `DRM_IOCTL_SYNCOBJ_EVENTFD`, asking the kernel to signal the
///      eventfd when `sync_point` is reached on the timeline.
///   3. Return the fd for the compositor to poll on.
/// Without DRM access we cannot produce a real fd; we return `Err` so
/// callers do not treat a sentinel value as a valid fd.
pub fn meta_wayland_sync_timeline_get_eventfd(
    timeline: *mut MetaWaylandSyncobjTimeline,
    sync_point: u64,
) -> Result<i32, &'static str> {
    if timeline.is_null() {
        return Err("null timeline");
    }
    // Safety: caller guarantees the pointer is a valid
    // MetaWaylandSyncobjTimeline.
    let t = unsafe { &*timeline };
    // If the point is already signalled there is nothing to wait for;
    // a full implementation would return an already-signalled eventfd.
    if t.is_point_signalled(sync_point) {
        return Err("sync point already signalled");
    }
    // No DRM fd available in this port; cannot create a real eventfd.
    Err("DRM eventfd unavailable without a DRM render node")
}
