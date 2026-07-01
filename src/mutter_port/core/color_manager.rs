//! MetaColorManager ported from GNOME Mutter's src/core/meta-color-manager.c
//!
//! MetaColorManager manages color profiles for outputs. It loads ICC profiles
//! from disk, creates color devices for each output, and applies the
//! appropriate color transformations during compositing.
//!
//! In Mutter this is a GObject that interfaces with colord (a D-Bus system
//! service for color device management). In the kernel, colord is not
//! available. The color manager is modeled as a plain struct that tracks
//! color devices and their profiles; callers can feed ICC profile data
//! directly.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/core/meta-color-manager.c

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

/// Color space, mirrors MetaColorSpace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorSpace {
    /// Unknown/default color space.
    Default,
    /// sRGB (standard RGB).
    Srgb,
    /// BT.2020 (wide gamut).
    Bt2020,
    /// Display P3 (Apple wide gamut).
    DisplayP3,
    /// Adobe RGB.
    AdobeRgb,
}

impl Default for ColorSpace {
    fn default() -> Self {
        ColorSpace::Default
    }
}

/// EOTF (Electro-Optical Transfer Function), mirrors MetaEotf.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Eotf {
    /// Traditional gamma (sRGB curve).
    TraditionalGammaSdr,
    /// Traditional gamma for HDR.
    TraditionalGammaHdr,
    /// PQ (Perceptual Quantizer) — SMPTE ST 2084.
    Pq,
    /// HLG (Hybrid Log-Gamma).
    Hlg,
}

impl Default for Eotf {
    fn default() -> Self {
        Eotf::TraditionalGammaSdr
    }
}

/// HDR metadata for a color profile.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HdrMetadata {
    pub active: bool,
    pub eotf: Eotf,
    pub max_luminance: f64,
    pub min_luminance: f64,
    pub max_cll: f64,
    pub max_fall: f64,
}

impl Default for HdrMetadata {
    fn default() -> Self {
        HdrMetadata {
            active: false,
            eotf: Eotf::TraditionalGammaSdr,
            max_luminance: 0.0,
            min_luminance: 0.0,
            max_cll: 0.0,
            max_fall: 0.0,
        }
    }
}

/// An ICC color profile. Mirrors MetaColorProfile.
#[derive(Debug, Clone)]
pub struct ColorProfile {
    /// Unique profile id.
    pub id: u32,
    /// Profile name (from colord).
    pub name: String,
    /// Color space the profile defines.
    pub color_space: ColorSpace,
    /// Transfer function.
    pub eotf: Eotf,
    /// HDR metadata, if this is an HDR profile.
    pub hdr: HdrMetadata,
    /// Raw ICC profile data (stubbed — would be parsed from file).
    pub icc_data: Vec<u8>,
}

impl ColorProfile {
    /// Create a new SDR sRGB profile (the default).
    pub fn new_srgb(id: u32) -> Self {
        ColorProfile {
            id,
            name: String::from("sRGB"),
            color_space: ColorSpace::Srgb,
            eotf: Eotf::TraditionalGammaSdr,
            hdr: HdrMetadata::default(),
            icc_data: Vec::new(),
        }
    }

    /// Create a new HDR profile.
    pub fn new_hdr(
        id: u32,
        name: &str,
        color_space: ColorSpace,
        eotf: Eotf,
        hdr: HdrMetadata,
    ) -> Self {
        ColorProfile {
            id,
            name: String::from(name),
            color_space,
            eotf,
            hdr,
            icc_data: Vec::new(),
        }
    }

    /// Whether this is an HDR profile.
    pub fn is_hdr(&self) -> bool {
        self.hdr.active
    }
}

/// A color device: one per output. Mirrors MetaColorDevice.
#[derive(Debug)]
pub struct ColorDevice {
    /// Unique device id.
    pub id: u32,
    /// Output connector name this device is for.
    pub connector: String,
    /// Currently assigned color profile, if any.
    pub profile: Option<ColorProfile>,
    /// Whether the device is color-managed (has a profile assigned).
    pub is_managed: bool,
    /// The color state derived from the current profile.
    pub color_state: ColorState,
}

/// Color state computed from a profile. Mirrors the ClutterColorState
/// that MetaColorDevice produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorState {
    /// Unique id for this color state.
    pub id: u64,
    /// Color space.
    pub color_space: ColorSpace,
    /// Transfer function.
    pub eotf: Eotf,
    /// Whether the color state supports HDR.
    pub is_hdr: bool,
}

impl Default for ColorState {
    fn default() -> Self {
        ColorState {
            id: 0,
            color_space: ColorSpace::Srgb,
            eotf: Eotf::TraditionalGammaSdr,
            is_hdr: false,
        }
    }
}

static COLOR_STATE_ID: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

fn next_color_state_id() -> u64 {
    COLOR_STATE_ID.fetch_add(1, core::sync::atomic::Ordering::Relaxed) + 1
}

impl ColorDevice {
    /// Create a new color device for an output. Mirrors
    /// meta_color_device_new().
    pub fn new(id: u32, connector: &str) -> Self {
        ColorDevice {
            id,
            connector: String::from(connector),
            profile: None,
            is_managed: false,
            color_state: ColorState::default(),
        }
    }

    /// Assign a color profile to this device. Mirrors
    /// meta_color_device_set_profile().
    pub fn set_profile(&mut self, profile: ColorProfile) {
        self.color_state = ColorState {
            id: next_color_state_id(),
            color_space: profile.color_space,
            eotf: profile.eotf,
            is_hdr: profile.is_hdr(),
        };
        self.profile = Some(profile);
        self.is_managed = true;
    }

    /// Clear the assigned profile. Mirrors meta_color_device_clear().
    pub fn clear_profile(&mut self) {
        self.profile = None;
        self.is_managed = false;
        self.color_state = ColorState::default();
    }
}

/// The color manager. Mirrors MetaColorManager.
///
/// Stubbed: colord D-Bus interaction (cd_client_connect, cd_device_get_profile,
/// cd_profile_get_icc_data) is not available. Callers feed profiles directly
/// via `set_device_profile()`.
#[derive(Debug)]
pub struct MetaColorManager {
    /// Color devices keyed by device id.
    devices: BTreeMap<u32, ColorDevice>,
    /// All known profiles keyed by id.
    profiles: BTreeMap<u32, ColorProfile>,
    /// Next device id.
    next_device_id: u32,
    /// Next profile id.
    next_profile_id: u32,
}

impl MetaColorManager {
    /// Create a new color manager. Mirrors meta_color_manager_new().
    pub fn new() -> Self {
        MetaColorManager {
            devices: BTreeMap::new(),
            profiles: BTreeMap::new(),
            next_device_id: 1,
            next_profile_id: 1,
        }
    }

    /// Create a color device for an output. Mirrors
    /// meta_color_manager_create_device().
    pub fn create_device(&mut self, connector: &str) -> u32 {
        let id = self.next_device_id;
        self.next_device_id += 1;
        self.devices.insert(id, ColorDevice::new(id, connector));
        id
    }

    /// Remove a color device. Mirrors meta_color_manager_destroy_device().
    pub fn destroy_device(&mut self, device_id: u32) -> bool {
        self.devices.remove(&device_id).is_some()
    }

    /// Get a color device by id.
    pub fn get_device(&self, device_id: u32) -> Option<&ColorDevice> {
        self.devices.get(&device_id)
    }

    /// Get a mutable color device by id.
    pub fn get_device_mut(&mut self, device_id: u32) -> Option<&mut ColorDevice> {
        self.devices.get_mut(&device_id)
    }

    /// Get the color device for a connector name.
    pub fn get_device_for_connector(&self, connector: &str) -> Option<&ColorDevice> {
        self.devices.values().find(|d| d.connector == connector)
    }

    /// Get mutable color device for a connector name.
    pub fn get_device_for_connector_mut(&mut self, connector: &str) -> Option<&mut ColorDevice> {
        self.devices.values_mut().find(|d| d.connector == connector)
    }

    /// Create a new sRGB color profile. Mirrors the default profile
    /// that colord would provide for an unmanaged output.
    pub fn create_srgb_profile(&mut self) -> u32 {
        let id = self.next_profile_id;
        self.next_profile_id += 1;
        let profile = ColorProfile::new_srgb(id);
        self.profiles.insert(id, profile);
        id
    }

    /// Create a custom color profile.
    pub fn create_profile(
        &mut self,
        name: &str,
        color_space: ColorSpace,
        eotf: Eotf,
        hdr: HdrMetadata,
    ) -> u32 {
        let id = self.next_profile_id;
        self.next_profile_id += 1;
        let profile = ColorProfile::new_hdr(id, name, color_space, eotf, hdr);
        self.profiles.insert(id, profile);
        id
    }

    /// Get a profile by id.
    pub fn get_profile(&self, profile_id: u32) -> Option<&ColorProfile> {
        self.profiles.get(&profile_id)
    }

    /// Assign a profile to a device. Mirrors meta_color_device_set_profile()
    /// called from the color manager's profile-changed handler.
    pub fn set_device_profile(
        &mut self,
        device_id: u32,
        profile_id: u32,
    ) -> Result<(), &'static str> {
        let profile = self
            .profiles
            .get(&profile_id)
            .ok_or("Profile not found")?
            .clone();
        let device = self.devices.get_mut(&device_id).ok_or("Device not found")?;
        device.set_profile(profile);
        Ok(())
    }

    /// Clear the profile from a device.
    pub fn clear_device_profile(&mut self, device_id: u32) -> Result<(), &'static str> {
        let device = self.devices.get_mut(&device_id).ok_or("Device not found")?;
        device.clear_profile();
        Ok(())
    }

    /// Number of color devices.
    pub fn device_count(&self) -> usize {
        self.devices.len()
    }

    /// Number of managed devices (with profiles).
    pub fn managed_count(&self) -> usize {
        self.devices.values().filter(|d| d.is_managed).count()
    }

    /// All color devices.
    pub fn devices(&self) -> impl Iterator<Item = &ColorDevice> {
        self.devices.values()
    }
}

impl Default for MetaColorManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_manager() {
        let cm = MetaColorManager::new();
        assert_eq!(cm.device_count(), 0);
    }

    #[test]
    fn test_create_device() {
        let mut cm = MetaColorManager::new();
        let id = cm.create_device("DP-1");
        assert!(cm.get_device(id).is_some());
        assert_eq!(cm.get_device(id).unwrap().connector, "DP-1");
        assert!(!cm.get_device(id).unwrap().is_managed);
    }

    #[test]
    fn test_destroy_device() {
        let mut cm = MetaColorManager::new();
        let id = cm.create_device("DP-1");
        assert!(cm.destroy_device(id));
        assert!(cm.get_device(id).is_none());
    }

    #[test]
    fn test_assign_srgb_profile() {
        let mut cm = MetaColorManager::new();
        let dev_id = cm.create_device("eDP-1");
        let prof_id = cm.create_srgb_profile();

        cm.set_device_profile(dev_id, prof_id).unwrap();

        let dev = cm.get_device(dev_id).unwrap();
        assert!(dev.is_managed);
        assert_eq!(dev.color_state.color_space, ColorSpace::Srgb);
        assert!(!dev.color_state.is_hdr);
    }

    #[test]
    fn test_assign_hdr_profile() {
        let mut cm = MetaColorManager::new();
        let dev_id = cm.create_device("DP-1");
        let prof_id = cm.create_profile(
            "HDR10",
            ColorSpace::Bt2020,
            Eotf::Pq,
            HdrMetadata {
                active: true,
                eotf: Eotf::Pq,
                max_luminance: 1000.0,
                min_luminance: 0.05,
                max_cll: 1000.0,
                max_fall: 400.0,
            },
        );

        cm.set_device_profile(dev_id, prof_id).unwrap();

        let dev = cm.get_device(dev_id).unwrap();
        assert!(dev.is_managed);
        assert!(dev.color_state.is_hdr);
        assert_eq!(dev.color_state.color_space, ColorSpace::Bt2020);
    }

    #[test]
    fn test_clear_profile() {
        let mut cm = MetaColorManager::new();
        let dev_id = cm.create_device("DP-1");
        let prof_id = cm.create_srgb_profile();
        cm.set_device_profile(dev_id, prof_id).unwrap();

        cm.clear_device_profile(dev_id).unwrap();
        let dev = cm.get_device(dev_id).unwrap();
        assert!(!dev.is_managed);
        assert!(dev.profile.is_none());
    }

    #[test]
    fn test_get_device_for_connector() {
        let mut cm = MetaColorManager::new();
        cm.create_device("DP-1");
        cm.create_device("HDMI-A-1");

        assert!(cm.get_device_for_connector("DP-1").is_some());
        assert!(cm.get_device_for_connector("HDMI-A-1").is_some());
        assert!(cm.get_device_for_connector("VGA-1").is_none());
    }

    #[test]
    fn test_set_device_profile_invalid_ids() {
        let mut cm = MetaColorManager::new();
        assert!(cm.set_device_profile(999, 999).is_err());

        let dev_id = cm.create_device("DP-1");
        assert!(cm.set_device_profile(dev_id, 999).is_err());
        assert!(cm.set_device_profile(999, 1).is_err());
    }

    #[test]
    fn test_managed_count() {
        let mut cm = MetaColorManager::new();
        let d1 = cm.create_device("DP-1");
        let d2 = cm.create_device("DP-2");
        let p = cm.create_srgb_profile();

        cm.set_device_profile(d1, p).unwrap();

        assert_eq!(cm.device_count(), 2);
        assert_eq!(cm.managed_count(), 1);
    }
}
