//! Backend Types ported from GNOME Mutter's src/backends/
//!
//! Type definitions for core backend subsystems including monitors, GPUs, outputs,
//! and color management. Provides opaque forward declarations for hardware abstraction.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-backend-types.h

use core::ffi::c_void;

/// Opaque backend instance.
pub struct MetaBackend {
    _phantom: core::marker::PhantomData<c_void>,
}

/// Opaque color device.
pub struct MetaColorDevice;

/// Opaque color manager.
pub struct MetaColorManager;

/// Opaque color profile.
pub struct MetaColorProfile;

/// Opaque color store.
pub struct MetaColorStore;

/// Color mode enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetaColorMode {
    Unknown = 0,
}

/// Opaque monitor manager.
pub struct MetaMonitorManager;

/// Opaque monitor config manager.
pub struct MetaMonitorConfigManager;

/// Opaque monitor config store.
pub struct MetaMonitorConfigStore;

/// Opaque monitors configuration.
pub struct MetaMonitorsConfig;

/// Monitors config flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetaMonitorsConfigFlag {
    Migrated = 0,
}

/// Opaque monitor instance.
pub struct MetaMonitor;

/// Opaque normal (non-tiled) monitor.
pub struct MetaMonitorNormal;

/// Opaque tiled monitor.
pub struct MetaMonitorTiled;

/// Opaque monitor specification.
pub struct MetaMonitorSpec;

/// Opaque logical monitor.
pub struct MetaLogicalMonitor;

/// Opaque monitor mode.
pub struct MetaMonitorMode;

/// Opaque GPU.
pub struct MetaGpu;

/// Opaque CRTC (CRT controller).
pub struct MetaCrtc;

/// Opaque output.
pub struct MetaOutput;

/// Opaque CRTC mode.
pub struct MetaCrtcMode;

/// Opaque CRTC assignment.
pub struct MetaCrtcAssignment;

/// Opaque output assignment.
pub struct MetaOutputAssignment;

/// Opaque tile information.
pub struct MetaTileInfo;

/// Opaque renderer.
pub struct MetaRenderer;

/// Opaque renderer view.
pub struct MetaRendererView;

/// Opaque stream.
pub struct MetaStream;

/// Opaque stream source.
pub struct MetaStreamSource;

/// Opaque remote desktop.
pub struct MetaRemoteDesktop;

/// Opaque remote desktop session.
pub struct MetaRemoteDesktopSession;

/// Opaque screen cast.
pub struct MetaScreenCast;

/// Opaque screen cast session.
pub struct MetaScreenCastSession;

/// Opaque screen cast stream.
pub struct MetaScreenCastStream;

/// Opaque virtual monitor.
pub struct MetaVirtualMonitor;

/// Opaque virtual monitor info.
pub struct MetaVirtualMonitorInfo;

/// Opaque virtual mode info.
pub struct MetaVirtualModeInfo;

/// Opaque barrier.
pub struct MetaBarrier;

/// Opaque barrier implementation.
pub struct MetaBarrierImpl;

/// Opaque idle manager.
pub struct MetaIdleManager;

/// Opaque D-Bus session.
pub struct MetaDbusSession;

/// Opaque D-Bus session manager.
pub struct MetaDbusSessionManager;

/// Opaque D-Bus session watcher.
pub struct MetaDbusSessionWatcher;

/// CTM (Color Transform Matrix) — 3x3 matrix for color correction.
#[derive(Debug, Clone, Copy)]
pub struct MetaCtm {
    pub matrix: [u64; 9],
}

impl MetaCtm {
    /// Create a new CTM matrix.
    pub fn new(matrix: [u64; 9]) -> Self {
        MetaCtm { matrix }
    }

    /// Create an identity CTM.
    pub fn identity() -> Self {
        MetaCtm {
            matrix: [1, 0, 0, 0, 1, 0, 0, 0, 1],
        }
    }
}

impl Default for MetaCtm {
    fn default() -> Self {
        Self::identity()
    }
}

/// Gamma LUT (Lookup Table) — per-channel 16-bit ramps for gamma correction.
#[derive(Debug, Clone)]
pub struct MetaGammaLut {
    pub red: alloc::vec::Vec<u16>,
    pub green: alloc::vec::Vec<u16>,
    pub blue: alloc::vec::Vec<u16>,
}

impl MetaGammaLut {
    /// Create a new gamma LUT from channel ramps.
    pub fn new(
        red: alloc::vec::Vec<u16>,
        green: alloc::vec::Vec<u16>,
        blue: alloc::vec::Vec<u16>,
    ) -> Self {
        MetaGammaLut { red, green, blue }
    }
}

impl Default for MetaGammaLut {
    fn default() -> Self {
        MetaGammaLut {
            red: alloc::vec::Vec::new(),
            green: alloc::vec::Vec::new(),
            blue: alloc::vec::Vec::new(),
        }
    }
}

/// Opaque input capture.
pub struct MetaInputCapture;

/// Opaque input capture session.
pub struct MetaInputCaptureSession;

/// Opaque EIS (Emulated Input System).
pub struct MetaEis;

/// Opaque EIS client.
pub struct MetaEisClient;

/// Opaque launcher.
pub struct MetaLauncher;

/// Opaque udev instance.
pub struct MetaUdev;
