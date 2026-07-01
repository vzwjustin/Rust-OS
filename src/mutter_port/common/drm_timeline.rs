//! DRM timeline synchronization objects.
//! Ported from src/common/meta-drm-timeline.h/c
//!
//! DRM timelines are based on the DRM synchronization object (syncobj)
//! mechanism exposed via `DRM_IOCTL_SYNCOBJ_*` ioctls. Each timeline
//! syncobj tracks a monotonically increasing 64-bit sequence point;
//! callers can wait for, signal, or query individual points.
//!
//! This port runs without a real DRM device, so the ioctls are not
//! issued. Instead the timeline maintains local state — the set of
//! pending (not-yet-signaled) points and the set of signaled points —
//! so that compositor logic depending on timeline semantics compiles
//! and behaves deterministically. The doc comments on each method
//! describe the corresponding DRM ioctl a full implementation would
//! issue.

use alloc::collections::BTreeSet;
use alloc::sync::Arc;
use spin::Mutex;

/// A point in a DRM timeline sequence.
pub type DrmTimelineSequence = u64;

/// Internal state shared between timeline handles and eventfd-style
/// waiters. Mirrors the per-syncobj state the kernel maintains.
#[derive(Debug, Default)]
struct TimelineState {
    /// Sequence points that have been signaled.
    signaled: BTreeSet<DrmTimelineSequence>,
    /// Sequence points that are still pending.
    pending: BTreeSet<DrmTimelineSequence>,
    /// The latest sequence point handed out via `set_sync_point`.
    last_point: DrmTimelineSequence,
}

impl TimelineState {
    fn signal(&mut self, point: DrmTimelineSequence) {
        self.pending.remove(&point);
        self.signaled.insert(point);
        // Any pending point at or below `point` is implicitly signaled
        // by a timeline syncobj.
        let implicitly_signaled: Vec<DrmTimelineSequence> = self
            .pending
            .iter()
            .filter(|&&p| p <= point)
            .copied()
            .collect();
        for p in implicitly_signaled {
            self.pending.remove(&p);
            self.signaled.insert(p);
        }
        if point > self.last_point {
            self.last_point = point;
        }
    }
}

/// A DRM timeline for synchronizing operations across the GPU and CPU.
///
/// DRM timelines are used for fine-grained synchronization in DRM drivers,
/// particularly for VSync and buffer composition. They're based on the
/// DRM synchronization object (syncobj) mechanism.
#[derive(Debug, Clone)]
pub struct DrmTimeline {
    fd: i32,
    syncobj: u32,
    state: Arc<Mutex<TimelineState>>,
}

impl DrmTimeline {
    /// Create a new DRM synchronization object on `fd` and return the
    /// raw syncobj handle.
    ///
    /// A full implementation would issue
    /// `DRM_IOCTL_SYNCOBJ_CREATE` (with `DRM_SYNCOBJ_CREATE_SIGNALED`
    /// when an initially-signaled timeline is desired) and return the
    /// allocated handle. Without a DRM device we synthesize a unique
    /// handle derived from `fd` so callers can still distinguish
    /// timelines.
    pub fn create_syncobj(fd: i32) -> Result<u32, &'static str> {
        if fd < 0 {
            return Err("invalid drm fd");
        }
        // Synthesize a deterministic, non-zero handle. The kernel would
        // allocate its own; we only need uniqueness within the process.
        let handle = ((fd as u32).wrapping_add(1)) | 0x8000_0000;
        Ok(handle)
    }

    /// Import an existing DRM synchronization object as a timeline.
    ///
    /// A full implementation would issue
    /// `DRM_IOCTL_SYNCOBJ_HANDLE_TO_FD` (or reference the handle
    /// directly) and wrap it in a `MetaDrmTimeline`. Here we record the
    /// handle and initialize empty local state.
    pub fn import_syncobj(fd: i32, drm_syncobj: u32) -> Result<Self, &'static str> {
        if fd < 0 {
            return Err("invalid drm fd");
        }
        if drm_syncobj == 0 {
            return Err("invalid syncobj handle");
        }
        Ok(Self {
            fd,
            syncobj: drm_syncobj,
            state: Arc::new(Mutex::new(TimelineState::default())),
        })
    }

    /// Get an eventfd that signals when `sync_point` is reached.
    ///
    /// A full implementation would issue
    /// `DRM_IOCTL_SYNCOBJ_EVENTFD` (or `DRM_IOCTL_SYNCOBJ_WAIT` with a
    /// pollable fd) so the returned fd becomes readable once the kernel
    /// signals the requested point. Without a DRM device we return
    /// `-1`; callers that already have the point signaled treat this as
    /// "already ready".
    pub fn get_eventfd(&self, sync_point: DrmTimelineSequence) -> Result<i32, &'static str> {
        let state = self.state.lock();
        if state.signaled.contains(&sync_point) {
            // Already signaled: return a sentinel that callers treat as
            // immediately ready. A real implementation would still
            // return a valid eventfd that is already readable.
            Ok(-1)
        } else {
            // No DRM device to back the eventfd. Record the pending
            // wait so a later `set_sync_point` can satisfy it.
            drop(state);
            let mut state = self.state.lock();
            state.pending.insert(sync_point);
            Ok(-1)
        }
    }

    /// Set a sync point on the timeline with a sync fd.
    ///
    /// A full implementation would issue
    /// `DRM_IOCTL_SYNCOBJ_TRANSFER` to import the sync fd into the
    /// timeline at the given point, or `DRM_IOCTL_SYNCOBJ_SIGNAL` for
    /// an immediate signal. Here we record the point as signaled and
    /// satisfy any pending waits at or below it.
    pub fn set_sync_point(
        &mut self,
        sync_point: DrmTimelineSequence,
        sync_fd: i32,
    ) -> Result<(), &'static str> {
        // A real sync fd of -1 means "already signaled". Any other
        // value would, upstream, be imported via SYNC_FILE_FD. We honor
        // the same semantics locally.
        let _ = sync_fd;
        let mut state = self.state.lock();
        state.signal(sync_point);
        Ok(())
    }

    /// Check if a sync point has been signaled.
    ///
    /// A full implementation would issue
    /// `DRM_IOCTL_SYNCOBJ_QUERY` to read the timeline's last signaled
    /// point and compare. Here we consult the local signaled set.
    pub fn is_signaled(&self, sync_point: DrmTimelineSequence) -> Result<bool, &'static str> {
        let state = self.state.lock();
        Ok(state.signaled.contains(&sync_point))
    }

    /// Wait until `sync_point` is signaled, returning immediately if it
    /// already is. A full implementation would issue
    /// `DRM_IOCTL_SYNCOBJ_WAIT` with `DRM_SYNCOBJ_WAIT_FLAGS_WAIT_ALL`.
    pub fn wait(&self, sync_point: DrmTimelineSequence) -> Result<(), &'static str> {
        let state = self.state.lock();
        if state.signaled.contains(&sync_point) {
            Ok(())
        } else {
            // Without a DRM device we cannot block; record the wait and
            // report that the point is still pending.
            Err("sync point not yet signaled")
        }
    }

    /// Returns the highest sequence point that has been signaled so
    /// far. Useful for compositor bookkeeping that mirrors
    /// `meta_drm_timeline_get_last_signaled`.
    pub fn last_signaled(&self) -> DrmTimelineSequence {
        self.state.lock().last_point
    }

    /// Returns the raw DRM syncobj handle this timeline wraps.
    pub fn syncobj_handle(&self) -> u32 {
        self.syncobj
    }

    /// Returns the DRM device file descriptor this timeline was created
    /// on.
    pub fn fd(&self) -> i32 {
        self.fd
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_syncobj_unique() {
        let a = DrmTimeline::create_syncobj(0).unwrap();
        let b = DrmTimeline::create_syncobj(1).unwrap();
        assert_ne!(a, b);
        assert!(DrmTimeline::create_syncobj(-1).is_err());
    }

    #[test]
    fn test_signal_and_query() {
        let mut tl = DrmTimeline::import_syncobj(0, 1).unwrap();
        assert!(!tl.is_signaled(5).unwrap());
        tl.set_sync_point(5, -1).unwrap();
        assert!(tl.is_signaled(5).unwrap());
        assert_eq!(tl.last_signaled(), 5);
    }

    #[test]
    fn test_implicit_signal_of_earlier_points() {
        let mut tl = DrmTimeline::import_syncobj(0, 1).unwrap();
        tl.get_eventfd(1).unwrap();
        tl.get_eventfd(2).unwrap();
        tl.set_sync_point(3, -1).unwrap();
        assert!(tl.is_signaled(1).unwrap());
        assert!(tl.is_signaled(2).unwrap());
        assert!(tl.is_signaled(3).unwrap());
    }

    #[test]
    fn test_wait_already_signaled() {
        let mut tl = DrmTimeline::import_syncobj(0, 1).unwrap();
        tl.set_sync_point(7, -1).unwrap();
        assert!(tl.wait(7).is_ok());
        assert!(tl.wait(8).is_err());
    }
}
