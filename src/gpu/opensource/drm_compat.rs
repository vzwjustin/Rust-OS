//! DRM (Direct Rendering Manager) Compatibility Layer
//!
//! This module provides a compatibility layer that emulates Linux DRM
//! functionality for RustOS, enabling opensource drivers to work.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

/// DRM device types
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DRMDeviceType {
    Primary, // /dev/dri/cardN
    Control, // /dev/dri/controlD64+N
    Render,  // /dev/dri/renderD128+N
}

/// DRM capability flags
#[derive(Debug, Clone, Copy)]
pub struct DRMCapabilities {
    pub dumb_buffer: bool,
    pub vblank_high_crtc: bool,
    pub dumb_preferred_depth: u32,
    pub dumb_prefer_shadow: bool,
    pub prime: bool,
    pub timestamping: bool,
    pub async_page_flip: bool,
    pub cursor_width: u32,
    pub cursor_height: u32,
    pub addfb2_modifiers: bool,
    pub page_flip_target: bool,
    pub crtc_in_vblank_event: bool,
    pub syncobj: bool,
    pub syncobj_timeline: bool,
}

/// DRM framebuffer information
#[derive(Debug, Clone)]
pub struct DRMFramebuffer {
    pub fb_id: u32,
    pub width: u32,
    pub height: u32,
    pub pitch: u32,
    pub bpp: u32,
    pub depth: u32,
    pub handle: u32,
    pub format: u32, // DRM_FORMAT_*
    pub modifier: u64,
}

/// DRM CRTC (Cathode Ray Tube Controller) information
#[derive(Debug, Clone)]
pub struct DRMCRTC {
    pub crtc_id: u32,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub fb_id: u32,
    pub gamma_size: u32,
    pub mode_valid: bool,
    pub mode: DRMDisplayMode,
}

/// DRM display mode
#[derive(Debug, Clone)]
pub struct DRMDisplayMode {
    pub clock: u32,
    pub hdisplay: u16,
    pub hsync_start: u16,
    pub hsync_end: u16,
    pub htotal: u16,
    pub hskew: u16,
    pub vdisplay: u16,
    pub vsync_start: u16,
    pub vsync_end: u16,
    pub vtotal: u16,
    pub vscan: u16,
    pub vrefresh: u32,
    pub flags: u32,
    pub type_flags: u32,
    pub name: String,
}

/// DRM encoder information
#[derive(Debug, Clone)]
pub struct DRMEncoder {
    pub encoder_id: u32,
    pub encoder_type: u32,
    pub crtc_id: u32,
    pub possible_crtcs: u32,
    pub possible_clones: u32,
}

/// DRM connector information
#[derive(Debug, Clone)]
pub struct DRMConnector {
    pub connector_id: u32,
    pub encoder_id: u32,
    pub connector_type: u32,
    pub connector_type_id: u32,
    pub connection: u32,
    pub mmwidth: u32,
    pub mmheight: u32,
    pub subpixel: u32,
    pub modes: Vec<DRMDisplayMode>,
    pub properties: BTreeMap<String, u64>,
}

/// DRM plane information
#[derive(Debug, Clone)]
pub struct DRMPlane {
    pub plane_id: u32,
    pub crtc_id: u32,
    pub fb_id: u32,
    pub crtc_x: i32,
    pub crtc_y: i32,
    pub crtc_w: u32,
    pub crtc_h: u32,
    pub src_x: u32,
    pub src_y: u32,
    pub src_w: u32,
    pub src_h: u32,
    pub possible_crtcs: u32,
    pub formats: Vec<u32>,
    pub modifiers: Vec<u64>,
}

/// DRM object property
#[derive(Debug, Clone)]
pub struct DRMProperty {
    pub prop_id: u32,
    pub name: String,
    pub flags: u32,
    pub values: Vec<u64>,
    pub enum_blobs: Vec<DRMPropertyEnum>,
}

/// DRM property enumeration
#[derive(Debug, Clone)]
pub struct DRMPropertyEnum {
    pub value: u64,
    pub name: String,
}

/// DRM compatibility layer
pub struct DRMCompatLayer {
    pub device_nodes: BTreeMap<u32, String>, // card_number -> device_path
    pub capabilities: DRMCapabilities,
    pub framebuffers: BTreeMap<u32, DRMFramebuffer>,
    pub crtcs: BTreeMap<u32, DRMCRTC>,
    pub encoders: BTreeMap<u32, DRMEncoder>,
    pub connectors: BTreeMap<u32, DRMConnector>,
    pub planes: BTreeMap<u32, DRMPlane>,
    pub properties: BTreeMap<u32, DRMProperty>,
    pub dumb_buffers: BTreeMap<u32, DumbBuffer>,
    pub next_object_id: u32,
}

impl DRMCompatLayer {
    pub fn new() -> Self {
        Self {
            device_nodes: BTreeMap::new(),
            capabilities: DRMCapabilities {
                dumb_buffer: true,
                vblank_high_crtc: true,
                dumb_preferred_depth: 24,
                dumb_prefer_shadow: false,
                prime: true,
                timestamping: true,
                async_page_flip: true,
                cursor_width: 64,
                cursor_height: 64,
                addfb2_modifiers: true,
                page_flip_target: true,
                crtc_in_vblank_event: true,
                syncobj: true,
                syncobj_timeline: true,
            },
            framebuffers: BTreeMap::new(),
            crtcs: BTreeMap::new(),
            encoders: BTreeMap::new(),
            connectors: BTreeMap::new(),
            planes: BTreeMap::new(),
            properties: BTreeMap::new(),
            dumb_buffers: BTreeMap::new(),
            next_object_id: 1,
        }
    }

    /// Register a new DRM device
    pub fn register_device(
        &mut self,
        card_number: u32,
        _driver_name: &str,
    ) -> Result<(), &'static str> {
        let device_path = format!("/dev/dri/card{}", card_number);
        self.device_nodes.insert(card_number, device_path);

        // Create default CRTC
        let crtc_id = self.next_object_id;
        self.next_object_id += 1;

        let default_mode = DRMDisplayMode {
            clock: 148500,
            hdisplay: 1920,
            hsync_start: 2008,
            hsync_end: 2052,
            htotal: 2200,
            hskew: 0,
            vdisplay: 1080,
            vsync_start: 1084,
            vsync_end: 1089,
            vtotal: 1125,
            vscan: 0,
            vrefresh: 60,
            flags: 0x5,       // DRM_MODE_FLAG_NHSYNC | DRM_MODE_FLAG_NVSYNC
            type_flags: 0x40, // DRM_MODE_TYPE_DRIVER
            name: "1920x1080".to_string(),
        };

        let crtc = DRMCRTC {
            crtc_id,
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            fb_id: 0,
            gamma_size: 256,
            mode_valid: true,
            mode: default_mode.clone(),
        };

        self.crtcs.insert(crtc_id, crtc);

        // Create default encoder
        let encoder_id = self.next_object_id;
        self.next_object_id += 1;

        let encoder = DRMEncoder {
            encoder_id,
            encoder_type: 2, // DRM_MODE_ENCODER_TMDS
            crtc_id,
            possible_crtcs: 1,
            possible_clones: 0,
        };

        self.encoders.insert(encoder_id, encoder);

        // Create default connector
        let connector_id = self.next_object_id;
        self.next_object_id += 1;

        let connector = DRMConnector {
            connector_id,
            encoder_id,
            connector_type: 11, // DRM_MODE_CONNECTOR_HDMIA
            connector_type_id: 1,
            connection: 1, // DRM_MODE_CONNECTED
            mmwidth: 510,  // 21.25 inches * 24 mm/inch
            mmheight: 287, // 11.95 inches * 24 mm/inch
            subpixel: 1,   // DRM_MODE_SUBPIXEL_UNKNOWN
            modes: vec![default_mode],
            properties: BTreeMap::new(),
        };

        self.connectors.insert(connector_id, connector);

        // Create primary plane
        let plane_id = self.next_object_id;
        self.next_object_id += 1;

        let plane = DRMPlane {
            plane_id,
            crtc_id,
            fb_id: 0,
            crtc_x: 0,
            crtc_y: 0,
            crtc_w: 1920,
            crtc_h: 1080,
            src_x: 0,
            src_y: 0,
            src_w: 1920 << 16, // Fixed point 16.16
            src_h: 1080 << 16,
            possible_crtcs: 1,
            formats: vec![
                0x34325258, // DRM_FORMAT_XR24 (XRGB8888)
                0x34324152, // DRM_FORMAT_AR24 (ARGB8888)
                0x36314752, // DRM_FORMAT_RG16 (RG88)
                0x38384752, // DRM_FORMAT_RG88
            ],
            modifiers: vec![
                0x0, // DRM_FORMAT_MOD_LINEAR
            ],
        };

        self.planes.insert(plane_id, plane);

        Ok(())
    }

    fn display_mode_for(width: u32, height: u32, refresh: u32) -> DRMDisplayMode {
        if width == 800 && height == 600 && refresh == 60 {
            return DRMDisplayMode {
                clock: 40000,
                hdisplay: 800,
                hsync_start: 840,
                hsync_end: 968,
                htotal: 1056,
                hskew: 0,
                vdisplay: 600,
                vsync_start: 601,
                vsync_end: 605,
                vtotal: 628,
                vscan: 0,
                vrefresh: 60,
                flags: 0x5,
                type_flags: 0x40,
                name: "800x600".to_string(),
            };
        }

        let h_front_porch = (width / 16).max(8);
        let h_sync = (width / 32).max(8);
        let h_back_porch = (width / 8).max(32);
        let hsync_start = width.saturating_add(h_front_porch);
        let hsync_end = hsync_start.saturating_add(h_sync);
        let htotal = hsync_end.saturating_add(h_back_porch);

        let vsync_start = height.saturating_add(3);
        let vsync_end = vsync_start.saturating_add(5);
        let vtotal = vsync_end.saturating_add((height / 25).max(20));
        let clock_hz = (htotal as u64)
            .saturating_mul(vtotal as u64)
            .saturating_mul(refresh as u64);
        let clock = core::cmp::min(clock_hz / 1000, u32::MAX as u64) as u32;
        let timing = |value: u32| core::cmp::min(value, u16::MAX as u32) as u16;

        DRMDisplayMode {
            clock,
            hdisplay: timing(width),
            hsync_start: timing(hsync_start),
            hsync_end: timing(hsync_end),
            htotal: timing(htotal),
            hskew: 0,
            vdisplay: timing(height),
            vsync_start: timing(vsync_start),
            vsync_end: timing(vsync_end),
            vtotal: timing(vtotal),
            vscan: 0,
            vrefresh: refresh,
            flags: 0x5,
            type_flags: 0x40,
            name: format!("{}x{}", width, height),
        }
    }

    pub fn configure_primary_mode(
        &mut self,
        width: u32,
        height: u32,
        bpp: u32,
    ) -> Result<(), &'static str> {
        if width == 0 || height == 0 {
            return Err("Invalid DRM primary mode dimensions");
        }

        let format = match bpp {
            16 => 0x36314752, // DRM_FORMAT_RGB565
            32 => 0x34325258, // DRM_FORMAT_XRGB8888
            _ => return Err("Unsupported DRM primary mode bpp"),
        };

        if self.device_nodes.is_empty() {
            self.register_device(0, "rustosdrm")?;
        }

        let primary_crtc_id = self
            .crtcs
            .keys()
            .next()
            .copied()
            .ok_or("DRM primary mode missing CRTC")?;
        if self.connectors.is_empty() {
            return Err("DRM primary mode missing connector");
        }
        if self.planes.is_empty() {
            return Err("DRM primary mode missing plane");
        }

        let fb_id = match self
            .framebuffers
            .iter()
            .find(|(_, fb)| {
                fb.width == width && fb.height == height && fb.bpp == bpp && fb.format == format
            })
            .map(|(id, _)| *id)
        {
            Some(id) => id,
            None => self.create_framebuffer(width, height, format, 0)?,
        };

        let mode = Self::display_mode_for(width, height, 60);
        for crtc in self.crtcs.values_mut() {
            crtc.x = 0;
            crtc.y = 0;
            crtc.width = width;
            crtc.height = height;
            crtc.fb_id = fb_id;
            crtc.mode = mode.clone();
            crtc.mode_valid = true;
        }

        for connector in self.connectors.values_mut() {
            connector.modes = vec![mode.clone()];
            connector.mmwidth = ((width as u64 * 254 + 479) / 960) as u32;
            connector.mmheight = ((height as u64 * 254 + 479) / 960) as u32;
        }

        let src_w = width.saturating_mul(1 << 16);
        let src_h = height.saturating_mul(1 << 16);
        for plane in self.planes.values_mut() {
            plane.crtc_id = primary_crtc_id;
            plane.fb_id = fb_id;
            plane.crtc_x = 0;
            plane.crtc_y = 0;
            plane.crtc_w = width;
            plane.crtc_h = height;
            plane.src_x = 0;
            plane.src_y = 0;
            plane.src_w = src_w;
            plane.src_h = src_h;
            if !plane.formats.contains(&format) {
                plane.formats.push(format);
            }
            if !plane.modifiers.contains(&0) {
                plane.modifiers.push(0);
            }
        }

        Ok(())
    }

    /// Create a framebuffer object
    pub fn create_framebuffer(
        &mut self,
        width: u32,
        height: u32,
        format: u32,
        handle: u32,
    ) -> Result<u32, &'static str> {
        let fb_id = self.next_object_id;
        self.next_object_id += 1;

        let (bpp, depth) = self.get_format_info(format);
        let pitch = width * (bpp / 8);

        let framebuffer = DRMFramebuffer {
            fb_id,
            width,
            height,
            pitch,
            bpp,
            depth,
            handle,
            format,
            modifier: 0, // DRM_FORMAT_MOD_LINEAR
        };

        self.framebuffers.insert(fb_id, framebuffer);
        Ok(fb_id)
    }

    /// Set CRTC configuration
    pub fn set_crtc(
        &mut self,
        crtc_id: u32,
        fb_id: u32,
        x: u32,
        y: u32,
        mode: Option<DRMDisplayMode>,
    ) -> Result<(), &'static str> {
        let crtc = self.crtcs.get_mut(&crtc_id).ok_or("Invalid CRTC ID")?;

        crtc.fb_id = fb_id;
        crtc.x = x;
        crtc.y = y;

        if let Some(new_mode) = mode {
            crtc.mode = new_mode;
            crtc.width = crtc.mode.hdisplay as u32;
            crtc.height = crtc.mode.vdisplay as u32;
            crtc.mode_valid = true;
        }

        Ok(())
    }

    /// Page flip operation
    pub fn page_flip(&mut self, crtc_id: u32, fb_id: u32, flags: u32) -> Result<(), &'static str> {
        let crtc = self.crtcs.get_mut(&crtc_id).ok_or("Invalid CRTC ID")?;

        if !self.framebuffers.contains_key(&fb_id) {
            return Err("Invalid framebuffer ID");
        }

        crtc.fb_id = fb_id;

        // Handle page flip flags
        if flags & 0x1 != 0 { // DRM_MODE_PAGE_FLIP_EVENT
             // Would queue vblank event in real implementation
        }

        if flags & 0x2 != 0 { // DRM_MODE_PAGE_FLIP_ASYNC
             // Immediate flip without waiting for vblank
        }

        Ok(())
    }

    /// Create a dumb buffer
    pub fn create_dumb_buffer(
        &mut self,
        width: u32,
        height: u32,
        bpp: u32,
    ) -> Result<DumbBuffer, &'static str> {
        let handle = self.next_object_id;
        self.next_object_id += 1;

        let pitch = width * ((bpp + 7) / 8);
        let size = pitch * height;

        // Allocate kernel heap memory for the dumb buffer backing store.
        let backing = vec![0u8; size as usize];
        let kernel_addr = backing.as_ptr() as u64;
        // Leak the backing store so it persists until the buffer is destroyed.
        // A full implementation would track and free it on destroy_dumb_buffer.
        core::mem::forget(backing);

        let dumb_buffer = DumbBuffer {
            handle,
            pitch,
            size: size as u64,
            width,
            height,
            bpp,
            kernel_addr,
        };

        self.dumb_buffers.insert(handle, dumb_buffer.clone());

        Ok(dumb_buffer)
    }

    /// Map a dumb buffer
    ///
    /// Returns the kernel virtual address of the dumb buffer's backing
    /// storage so the caller can read/write pixels directly.  The buffer
    /// was allocated from the kernel heap in `create_dumb_buffer`, so we
    /// look it up and return its address.
    pub fn map_dumb_buffer(&self, handle: u32) -> Result<u64, &'static str> {
        let buf = self
            .dumb_buffers
            .get(&handle)
            .ok_or("Invalid dumb buffer handle")?;
        Ok(buf.kernel_addr)
    }

    /// Get connector information
    pub fn get_connector(&self, connector_id: u32) -> Option<&DRMConnector> {
        self.connectors.get(&connector_id)
    }

    /// Get CRTC information
    pub fn get_crtc(&self, crtc_id: u32) -> Option<&DRMCRTC> {
        self.crtcs.get(&crtc_id)
    }

    /// Get encoder information
    pub fn get_encoder(&self, encoder_id: u32) -> Option<&DRMEncoder> {
        self.encoders.get(&encoder_id)
    }

    /// Get all connectors
    pub fn get_connectors(&self) -> Vec<u32> {
        self.connectors.keys().copied().collect()
    }

    /// Get all CRTCs
    pub fn get_crtcs(&self) -> Vec<u32> {
        self.crtcs.keys().copied().collect()
    }

    /// Get all encoders
    pub fn get_encoders(&self) -> Vec<u32> {
        self.encoders.keys().copied().collect()
    }

    /// Get all planes
    pub fn get_planes(&self) -> Vec<u32> {
        self.planes.keys().copied().collect()
    }

    /// Wait for vblank
    pub fn wait_vblank(&self, crtc_id: u32, sequence: u32) -> Result<u32, &'static str> {
        if !self.crtcs.contains_key(&crtc_id) {
            return Err("Invalid CRTC ID");
        }

        // Without a real display controller interrupt there is no hardware
        // vblank to wait for.  Return the next sequence number so callers
        // can proceed; on real hardware this would block until the vblank
        // IRQ fires and return the actual scanout sequence.
        crate::serial_println!(
            "drm: wait_vblank on crtc {} seq {} — no vblank IRQ, returning next seq",
            crtc_id,
            sequence
        );
        Ok(sequence + 1)
    }

    /// Get device capabilities
    pub fn get_capability(&self, capability: u64) -> Result<u64, &'static str> {
        match capability {
            0x1 => Ok(if self.capabilities.dumb_buffer { 1 } else { 0 }),
            0x2 => Ok(if self.capabilities.vblank_high_crtc {
                1
            } else {
                0
            }),
            0x3 => Ok(self.capabilities.dumb_preferred_depth as u64),
            0x4 => Ok(if self.capabilities.dumb_prefer_shadow {
                1
            } else {
                0
            }),
            0x5 => Ok(if self.capabilities.prime { 1 } else { 0 }),
            0x6 => Ok(if self.capabilities.timestamping { 1 } else { 0 }),
            0x7 => Ok(if self.capabilities.async_page_flip {
                1
            } else {
                0
            }),
            0x8 => Ok(self.capabilities.cursor_width as u64),
            0x9 => Ok(self.capabilities.cursor_height as u64),
            0xA => Ok(if self.capabilities.addfb2_modifiers {
                1
            } else {
                0
            }),
            0xB => Ok(if self.capabilities.page_flip_target {
                1
            } else {
                0
            }),
            0xC => Ok(if self.capabilities.crtc_in_vblank_event {
                1
            } else {
                0
            }),
            0xD => Ok(if self.capabilities.syncobj { 1 } else { 0 }),
            0xE => Ok(if self.capabilities.syncobj_timeline {
                1
            } else {
                0
            }),
            _ => Err("Unknown capability"),
        }
    }

    fn get_format_info(&self, format: u32) -> (u32, u32) {
        // Returns (bpp, depth)
        match format {
            0x20203852 => (8, 8),   // DRM_FORMAT_R8
            0x36314752 => (16, 16), // DRM_FORMAT_RG88
            0x34325258 => (32, 24), // DRM_FORMAT_XR24
            0x34324152 => (32, 32), // DRM_FORMAT_AR24
            0x34325242 => (32, 24), // DRM_FORMAT_XB24
            0x34324142 => (32, 32), // DRM_FORMAT_AB24
            _ => (32, 24),          // Default to XRGB8888
        }
    }
}

/// DRM dumb buffer
#[derive(Debug, Clone)]
pub struct DumbBuffer {
    pub handle: u32,
    pub pitch: u32,
    pub size: u64,
    pub width: u32,
    pub height: u32,
    pub bpp: u32,
    pub kernel_addr: u64,
}

/// DRM ioctl interface
pub struct DRMIoctl;

impl DRMIoctl {
    /// Get DRM version information
    pub fn version() -> DRMVersion {
        DRMVersion {
            version_major: 1,
            version_minor: 6,
            version_patchlevel: 0,
            name: "rustos_drm".to_string(),
            date: "20240101".to_string(),
            desc: "RustOS DRM Compatibility Layer".to_string(),
        }
    }

    /// Get DRM mode resources
    pub fn get_resources(drm: &DRMCompatLayer) -> DRMResources {
        DRMResources {
            fbs: drm.framebuffers.keys().copied().collect(),
            crtcs: drm.get_crtcs(),
            connectors: drm.get_connectors(),
            encoders: drm.get_encoders(),
            min_width: 320,
            max_width: 8192,
            min_height: 200,
            max_height: 8192,
        }
    }

    /// Get DRM plane resources
    pub fn get_plane_resources(drm: &DRMCompatLayer) -> DRMPlaneResources {
        DRMPlaneResources {
            planes: drm.get_planes(),
        }
    }
}

/// DRM version information
#[derive(Debug, Clone)]
pub struct DRMVersion {
    pub version_major: i32,
    pub version_minor: i32,
    pub version_patchlevel: i32,
    pub name: String,
    pub date: String,
    pub desc: String,
}

/// DRM resources
#[derive(Debug, Clone)]
pub struct DRMResources {
    pub fbs: Vec<u32>,
    pub crtcs: Vec<u32>,
    pub connectors: Vec<u32>,
    pub encoders: Vec<u32>,
    pub min_width: u32,
    pub max_width: u32,
    pub min_height: u32,
    pub max_height: u32,
}

/// DRM plane resources
#[derive(Debug, Clone)]
pub struct DRMPlaneResources {
    pub planes: Vec<u32>,
}

// Global DRM compatibility layer instance
static DRM_COMPAT: spin::Mutex<Option<DRMCompatLayer>> = spin::Mutex::new(None);

/// Initialize DRM compatibility layer
pub fn init_drm_compat() -> Result<(), &'static str> {
    let mut guard = DRM_COMPAT.lock();
    if guard.is_none() {
        *guard = Some(DRMCompatLayer::new());
    }
    Ok(())
}

/// Run `f` with the DRM compatibility layer instance, if initialized.
pub fn with_drm_compat<R>(f: impl FnOnce(&mut DRMCompatLayer) -> R) -> Option<R> {
    DRM_COMPAT.lock().as_mut().map(f)
}

pub fn configure_primary_mode(width: u32, height: u32, bpp: u32) -> Result<(), &'static str> {
    init_drm_compat()?;
    with_drm_compat(|drm| drm.configure_primary_mode(width, height, bpp))
        .unwrap_or(Err("DRM layer not initialized"))
}

/// Register a new GPU with DRM compatibility layer
pub fn register_drm_device(card_number: u32, driver_name: &str) -> Result<(), &'static str> {
    with_drm_compat(|drm| drm.register_device(card_number, driver_name))
        .unwrap_or(Err("DRM compatibility layer not initialized"))
}
