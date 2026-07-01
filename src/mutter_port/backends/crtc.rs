//! CRTC ported from GNOME Mutter's src/backends/meta-crtc.c
//!
//! A CRTC (CRT controller) scans out a framebuffer to one or more outputs.
//! This port keeps the data model (id, assigned outputs, current config),
//! the gamma LUT geometry/resampling math, and the color transform matrix
//! (CTM) helpers. Hardware/DRM programming (the class `set_config`,
//! `get_gamma_lut`, etc. vfuncs) is left to backend implementations.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-crtc.c

use alloc::vec;
use alloc::vec::Vec;

use super::crtc_mode::MetaCrtcMode;

/// Monitor transform bitmask placeholder (mirrors `MtkMonitorTransform`).
///
/// Real transform composition lives in the mtk layer; here it is carried as
/// an opaque value so the CRTC data model stays intact.
pub type MtkMonitorTransform = u32;

/// All-transforms sentinel used by `meta_crtc_init` in C.
pub const MTK_MONITOR_ALL_TRANSFORMS: MtkMonitorTransform = 0xff;

/// Floating-point rectangle (mirrors `graphene_rect_t`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RectF {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// A gamma lookup table with per-channel ramps.
#[derive(Debug, Clone)]
pub struct MetaGammaLut {
    pub red: Vec<u16>,
    pub green: Vec<u16>,
    pub blue: Vec<u16>,
}

impl MetaGammaLut {
    /// Create from existing channel ramps (copies the data).
    pub fn new(red: &[u16], green: &[u16], blue: &[u16]) -> Self {
        MetaGammaLut {
            red: red.to_vec(),
            green: green.to_vec(),
            blue: blue.to_vec(),
        }
    }

    /// Create a zero-initialized LUT with `size` entries per channel.
    pub fn new_sized(size: usize) -> Self {
        MetaGammaLut {
            red: vec![0; size],
            green: vec![0; size],
            blue: vec![0; size],
        }
    }

    /// Create an identity ramp (linear from 0 to `u16::MAX`).
    pub fn new_identity(size: usize) -> Self {
        let mut lut = Self::new_sized(size);

        if size < 2 {
            return lut;
        }

        for i in 0..size {
            let value = i as f64 / (size - 1) as f64;
            let v = (value * u16::MAX as f64) as u16;
            lut.red[i] = v;
            lut.green[i] = v;
            lut.blue[i] = v;
        }

        lut
    }

    /// Number of entries per channel.
    pub fn size(&self) -> usize {
        self.red.len()
    }

    /// Whether this LUT is (approximately) the identity ramp.
    pub fn is_identity(&self) -> bool {
        let size = self.size();
        if size == 0 {
            return true;
        }

        for i in 0..size {
            let value = ((i as f64 / (size - 1) as f64) * u16::MAX as f64) as u16;

            if (self.red[i] as i32 - value as i32).abs() > 1
                || (self.green[i] as i32 - value as i32).abs() > 1
                || (self.blue[i] as i32 - value as i32).abs() > 1
            {
                return false;
            }
        }

        true
    }

    /// Copy resampled to `target_size`, replicating or decimating entries.
    pub fn copy_to_size(&self, target_size: usize) -> MetaGammaLut {
        let size = self.size();

        if size == target_size {
            return self.clone();
        }

        let mut out = Self::new_sized(target_size);

        if target_size >= size {
            let slots = if size == 0 { 0 } else { target_size / size };
            let mut i = 0;
            while i < size {
                for j in 0..slots {
                    out.red[i * slots + j] = self.red[i];
                    out.green[i * slots + j] = self.green[i];
                    out.blue[i * slots + j] = self.blue[i];
                }
                i += 1;
            }

            for j in (i * slots)..target_size {
                out.red[j] = self.red[i - 1];
                out.green[j] = self.green[i - 1];
                out.blue[j] = self.blue[i - 1];
            }
        } else {
            for i in 0..target_size {
                let idx = i * (size - 1) / (target_size - 1);
                out.red[i] = self.red[idx];
                out.green[i] = self.green[idx];
                out.blue[i] = self.blue[idx];
            }
        }

        out
    }
}

impl PartialEq for MetaGammaLut {
    fn eq(&self, other: &Self) -> bool {
        self.red == other.red && self.green == other.green && self.blue == other.blue
    }
}

/// Color transform matrix in S31.32 fixed-point format (mirrors `MetaCtm`).
#[derive(Debug, Clone, Copy)]
pub struct MetaCtm {
    pub matrix: [u64; 9],
}

impl MetaCtm {
    /// Create an identity matrix (diagonal = 1.0 in S31.32 fixed-point).
    pub fn new() -> Self {
        let mut ctm = MetaCtm { matrix: [0; 9] };
        ctm.matrix[0] = 1u64 << 32;
        ctm.matrix[4] = 1u64 << 32;
        ctm.matrix[8] = 1u64 << 32;
        ctm
    }
}

impl Default for MetaCtm {
    fn default() -> Self {
        Self::new()
    }
}

impl PartialEq for MetaCtm {
    fn eq(&self, other: &Self) -> bool {
        self.matrix == other.matrix
    }
}

/// Current scan-out configuration of a CRTC (mirrors `MetaCrtcConfig`).
#[derive(Debug, Clone)]
pub struct MetaCrtcConfig {
    pub layout: RectF,
    pub mode: MetaCrtcMode,
    pub transform: MtkMonitorTransform,
}

impl MetaCrtcConfig {
    pub fn new(layout: RectF, mode: MetaCrtcMode, transform: MtkMonitorTransform) -> Self {
        MetaCrtcConfig {
            layout,
            mode,
            transform,
        }
    }
}

/// A CRT controller.
///
/// Backend GPU/DRM plumbing (`backend`, `gpu` object pointers, and the
/// hardware `set_config`/`gamma_lut` vfuncs) is intentionally omitted; those
/// belong to a concrete backend implementation.
#[derive(Debug)]
pub struct MetaCrtc {
    id: u64,
    all_transforms: MtkMonitorTransform,
    /// Ids of outputs currently assigned to this CRTC.
    outputs: Vec<u64>,
    config: Option<MetaCrtcConfig>,
}

impl MetaCrtc {
    pub fn new(id: u64) -> Self {
        MetaCrtc {
            id,
            all_transforms: MTK_MONITOR_ALL_TRANSFORMS,
            outputs: Vec::new(),
            config: None,
        }
    }

    pub fn get_id(&self) -> u64 {
        self.id
    }

    pub fn get_all_transforms(&self) -> MtkMonitorTransform {
        self.all_transforms
    }

    pub fn set_all_transforms(&mut self, all_transforms: MtkMonitorTransform) {
        self.all_transforms = all_transforms;
    }

    /// Assigned output ids.
    pub fn get_outputs(&self) -> &[u64] {
        &self.outputs
    }

    /// Assign an output id to this CRTC.
    pub fn assign_output(&mut self, output_id: u64) {
        self.outputs.push(output_id);
    }

    /// Unassign an output id; returns false if it was not assigned.
    pub fn unassign_output(&mut self, output_id: u64) -> bool {
        if let Some(pos) = self.outputs.iter().position(|&o| o == output_id) {
            self.outputs.remove(pos);
            true
        } else {
            false
        }
    }

    pub fn get_config(&self) -> Option<&MetaCrtcConfig> {
        self.config.as_ref()
    }

    /// Set the current config (backend hardware programming stubbed out).
    pub fn set_config(&mut self, config: MetaCrtcConfig) {
        self.unset_config();
        self.config = Some(config);
    }

    /// Clear the current config.
    pub fn unset_config(&mut self) {
        self.config = None;
    }
}
