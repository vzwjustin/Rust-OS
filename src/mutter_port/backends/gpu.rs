//! GPU ported from GNOME Mutter's src/backends/meta-gpu.c
//!
//! A GPU owns the collection of outputs, CRTCs, and modes discovered on a
//! graphics device. It is the base object that backend-specific GPU types
//! (KMS, native, X11) derive from.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-gpu.c

use alloc::vec::Vec;

/// A display output (connector) on a GPU.
///
/// Faithful data subset of MetaOutput. Hardware access (DRM/KMS probing) is
/// stubbed; `hotplug_mode_update` and `is_presentation` come from the EDID/DRM
/// output info in real Mutter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    pub id: u64,
    /// Set when the output's mode list can change on hotplug.
    pub hotplug_mode_update: bool,
    pub is_presentation: bool,
}

impl Output {
    pub fn new(id: u64) -> Self {
        Output {
            id,
            hotplug_mode_update: false,
            is_presentation: false,
        }
    }

    /// Whether this output refers to the same physical connector as `other`.
    /// Mirrors meta_output_matches (identity by stable output id here).
    pub fn matches(&self, other: &Output) -> bool {
        self.id == other.id
    }
}

/// A CRTC (scanout engine) on a GPU. Data subset of MetaCrtc; hardware stubbed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Crtc {
    pub id: u64,
}

impl Crtc {
    pub fn new(id: u64) -> Self {
        Crtc { id }
    }
}

/// A display mode (resolution/refresh) on a GPU. Data subset of MetaCrtcMode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mode {
    pub id: u64,
    pub width: u32,
    pub height: u32,
    pub refresh_rate_mhz: u32,
}

impl Mode {
    pub fn new(id: u64, width: u32, height: u32, refresh_rate_mhz: u32) -> Self {
        Mode {
            id,
            width,
            height,
            refresh_rate_mhz,
        }
    }
}

/// A GPU: owns the outputs, CRTCs, and modes of one graphics device.
#[derive(Debug)]
pub struct Gpu {
    /// Opaque backend handle. In Mutter this is the owning MetaBackend pointer;
    /// stubbed as an id since the backend object is not modeled in-kernel.
    pub backend_id: u64,
    outputs: Vec<Output>,
    crtcs: Vec<Crtc>,
    modes: Vec<Mode>,
}

impl Gpu {
    pub fn new(backend_id: u64) -> Self {
        Gpu {
            backend_id,
            outputs: Vec::new(),
            crtcs: Vec::new(),
            modes: Vec::new(),
        }
    }

    /// Whether any output can change its mode list on hotplug.
    /// Faithful port of meta_gpu_has_hotplug_mode_update.
    pub fn has_hotplug_mode_update(&self) -> bool {
        self.outputs.iter().any(|o| o.hotplug_mode_update)
    }

    /// Re-read the current hardware configuration.
    ///
    /// Stub: in Mutter this is a virtual method that probes DRM/KMS. Kernel
    /// hardware probing is not wired here.
    pub fn read_current(&mut self) -> bool {
        // Would call the backend-specific read_current() and repopulate
        // outputs/crtcs/modes from hardware. Stubbed for no_std kernel.
        true
    }

    pub fn get_backend_id(&self) -> u64 {
        self.backend_id
    }

    pub fn get_outputs(&self) -> &[Output] {
        &self.outputs
    }

    pub fn get_crtcs(&self) -> &[Crtc] {
        &self.crtcs
    }

    pub fn get_modes(&self) -> &[Mode] {
        &self.modes
    }

    /// Replace the output list, taking ownership. Mirrors meta_gpu_take_outputs.
    pub fn take_outputs(&mut self, outputs: Vec<Output>) {
        self.outputs = outputs;
    }

    /// Replace the CRTC list, taking ownership. Mirrors meta_gpu_take_crtcs.
    pub fn take_crtcs(&mut self, crtcs: Vec<Crtc>) {
        self.crtcs = crtcs;
    }

    /// Replace the mode list, taking ownership. Mirrors meta_gpu_take_modes.
    pub fn take_modes(&mut self, modes: Vec<Mode>) {
        self.modes = modes;
    }

    /// Find the current output matching an output from a previous configuration.
    /// Faithful port of meta_gpu_find_output.
    pub fn find_output(&self, old_output: &Output) -> Option<&Output> {
        self.outputs.iter().find(|o| o.matches(old_output))
    }
}
