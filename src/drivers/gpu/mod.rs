//! GPU/DRM subsystem
//!
//! Provides DRM/KMS (Direct Rendering Manager / Kernel Mode Setting) framework
//! for display controllers, framebuffers, planes, CRTCs, and encoders.
//! Mirrors Linux's `drivers/gpu/drm/drm_*`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// DRM pixel format (Linux `u32 fourcc`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrmFourCc(pub u32);

impl DrmFourCc {
    pub const XRGB8888: Self = DrmFourCc(0x34325258);
    pub const ARGB8888: Self = DrmFourCc(0x34325241);
    pub const RGB565: Self = DrmFourCc(0x36314752);
    pub const RGB888: Self = DrmFourCc(0x34324752);
}

/// DRM plane type (Linux `enum drm_plane_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaneType {
    Overlay,
    Primary,
    Cursor,
}

/// DRM connector status (Linux `enum drm_connector_status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorStatus {
    Unknown,
    Connected,
    Disconnected,
}

/// DRM connector type (Linux `enum drm_connector_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorType {
    Unknown,
    Vga,
    Dvii,
    Dvid,
    Dvia,
    Composite,
    Svideo,
    Lvds,
    Component,
    NinePinDin,
    DisplayPort,
    HdmiA,
    HdmiB,
    Tv,
    Edp,
    Virtual,
    Dsi,
    Dpi,
    Writeback,
}

/// DRM mode info (Linux `struct drm_mode_modeinfo`).
#[derive(Debug, Clone)]
pub struct DrmMode {
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
    pub name: String,
}

/// DRM framebuffer (Linux `struct drm_framebuffer`).
pub struct DrmFramebuffer {
    pub id: u32,
    pub width: u32,
    pub height: u32,
    pub pixel_format: DrmFourCc,
    pub pitches: Vec<u32>,
    pub offsets: Vec<u32>,
    pub bpp: u32,
    pub depth: u8,
}

/// DRM plane (Linux `struct drm_plane`).
pub struct DrmPlane {
    pub id: u32,
    pub plane_type: PlaneType,
    pub possible_crtcs: u32,
    pub fb_id: Option<u32>,
    pub crtc_id: Option<u32>,
    pub src_x: u32,
    pub src_y: u32,
    pub src_w: u32,
    pub src_h: u32,
    pub crtc_x: i32,
    pub crtc_y: i32,
    pub crtc_w: u32,
    pub crtc_h: u32,
    pub formats: Vec<DrmFourCc>,
}

/// DRM CRTC (Linux `struct drm_crtc`).
pub struct DrmCrtc {
    pub id: u32,
    pub index: u32,
    pub enabled: bool,
    pub active: bool,
    pub primary_plane: Option<u32>,
    pub cursor_plane: Option<u32>,
    pub mode: Option<DrmMode>,
    pub framebuffer_id: Option<u32>,
    pub x: u32,
    pub y: u32,
    pub gamma_size: u32,
}

/// DRM encoder (Linux `struct drm_encoder`).
pub struct DrmEncoder {
    pub id: u32,
    pub encoder_type: u32,
    pub possible_crtcs: u32,
    pub possible_clones: u32,
    pub crtc_id: Option<u32>,
}

/// DRM connector (Linux `struct drm_connector`).
pub struct DrmConnector {
    pub id: u32,
    pub connector_type: ConnectorType,
    pub connector_type_id: u32,
    pub status: ConnectorStatus,
    pub encoder_id: Option<u32>,
    pub modes: Vec<DrmMode>,
    pub width_mm: u32,
    pub height_mm: u32,
}

/// DRM device (Linux `struct drm_device`).
pub struct DrmDevice {
    pub name: String,
    pub driver_id: u32,
    pub fb_ids: Vec<u32>,
    pub plane_ids: Vec<u32>,
    pub crtc_ids: Vec<u32>,
    pub encoder_ids: Vec<u32>,
    pub connector_ids: Vec<u32>,
    pub mode_config_width: u32,
    pub mode_config_height: u32,
}

/// DRM driver operations (Linux `struct drm_driver` fops subset).
pub struct DrmDriverOps {
    pub open: fn(device_id: u32) -> Result<(), &'static str>,
    pub postclose: fn(device_id: u32) -> Result<(), &'static str>,
    pub dumb_create:
        fn(device_id: u32, width: u32, height: u32, bpp: u32) -> Result<DumbBuffer, &'static str>,
    pub dumb_destroy: fn(device_id: u32, handle: u32) -> Result<(), &'static str>,
    pub dumb_map: fn(device_id: u32, handle: u32) -> Result<u64, &'static str>,
}

/// Dumb buffer allocation result.
#[derive(Debug, Clone)]
pub struct DumbBuffer {
    pub handle: u32,
    pub width: u32,
    pub height: u32,
    pub bpp: u32,
    pub pitch: u32,
    pub size: u64,
}

// ── Registry ────────────────────────────────────────────────────────────

static DEVICE_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static FB_ID_COUNTER: AtomicU32 = AtomicU32::new(1);
static PLANE_ID_COUNTER: AtomicU32 = AtomicU32::new(1);
static CRTC_ID_COUNTER: AtomicU32 = AtomicU32::new(1);
static ENCODER_ID_COUNTER: AtomicU32 = AtomicU32::new(1);
static CONNECTOR_ID_COUNTER: AtomicU32 = AtomicU32::new(1);
static DUMB_HANDLE_COUNTER: AtomicU32 = AtomicU32::new(1);

static DRM_DEVICES: RwLock<BTreeMap<u32, DrmDevice>> = RwLock::new(BTreeMap::new());
static DRM_FRAMEBUFFERS: RwLock<BTreeMap<u32, DrmFramebuffer>> = RwLock::new(BTreeMap::new());
static DRM_PLANES: RwLock<BTreeMap<u32, DrmPlane>> = RwLock::new(BTreeMap::new());
static DRM_CRTCS: RwLock<BTreeMap<u32, DrmCrtc>> = RwLock::new(BTreeMap::new());
static DRM_ENCODERS: RwLock<BTreeMap<u32, DrmEncoder>> = RwLock::new(BTreeMap::new());
static DRM_CONNECTORS: RwLock<BTreeMap<u32, DrmConnector>> = RwLock::new(BTreeMap::new());
static DRM_DRIVER_OPS: RwLock<BTreeMap<u32, DrmDriverOps>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a DRM device with its driver operations.
pub fn register_device(name: &str, ops: DrmDriverOps) -> Result<u32, &'static str> {
    let id = DEVICE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = DrmDevice {
        name: String::from(name),
        driver_id: id,
        fb_ids: Vec::new(),
        plane_ids: Vec::new(),
        crtc_ids: Vec::new(),
        encoder_ids: Vec::new(),
        connector_ids: Vec::new(),
        mode_config_width: 0,
        mode_config_height: 0,
    };
    DRM_DEVICES.write().insert(id, dev);
    DRM_DRIVER_OPS.write().insert(id, ops);
    Ok(id)
}

/// Create a framebuffer for a DRM device.
pub fn framebuffer_create(
    device_id: u32,
    width: u32,
    height: u32,
    pixel_format: DrmFourCc,
    pitches: Vec<u32>,
    offsets: Vec<u32>,
    bpp: u32,
    depth: u8,
) -> Result<u32, &'static str> {
    let fb_id = FB_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let fb = DrmFramebuffer {
        id: fb_id,
        width,
        height,
        pixel_format,
        pitches,
        offsets,
        bpp,
        depth,
    };
    DRM_FRAMEBUFFERS.write().insert(fb_id, fb);

    let mut devices = DRM_DEVICES.write();
    let dev = devices.get_mut(&device_id).ok_or("DRM device not found")?;
    dev.fb_ids.push(fb_id);
    Ok(fb_id)
}

/// Destroy a framebuffer.
pub fn framebuffer_destroy(fb_id: u32) -> Result<(), &'static str> {
    if DRM_FRAMEBUFFERS.write().remove(&fb_id).is_none() {
        return Err("Framebuffer not found");
    }
    // Remove from device list
    let mut devices = DRM_DEVICES.write();
    for dev in devices.values_mut() {
        dev.fb_ids.retain(|&id| id != fb_id);
    }
    Ok(())
}

/// Create a plane for a DRM device.
pub fn plane_create(
    device_id: u32,
    plane_type: PlaneType,
    possible_crtcs: u32,
    formats: Vec<DrmFourCc>,
) -> Result<u32, &'static str> {
    let plane_id = PLANE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let plane = DrmPlane {
        id: plane_id,
        plane_type,
        possible_crtcs,
        fb_id: None,
        crtc_id: None,
        src_x: 0,
        src_y: 0,
        src_w: 0,
        src_h: 0,
        crtc_x: 0,
        crtc_y: 0,
        crtc_w: 0,
        crtc_h: 0,
        formats,
    };
    DRM_PLANES.write().insert(plane_id, plane);

    let mut devices = DRM_DEVICES.write();
    let dev = devices.get_mut(&device_id).ok_or("DRM device not found")?;
    dev.plane_ids.push(plane_id);
    Ok(plane_id)
}

/// Create a CRTC for a DRM device.
pub fn crtc_create(
    device_id: u32,
    index: u32,
    primary_plane: u32,
    cursor_plane: Option<u32>,
    gamma_size: u32,
) -> Result<u32, &'static str> {
    let crtc_id = CRTC_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let crtc = DrmCrtc {
        id: crtc_id,
        index,
        enabled: false,
        active: false,
        primary_plane: Some(primary_plane),
        cursor_plane,
        mode: None,
        framebuffer_id: None,
        x: 0,
        y: 0,
        gamma_size,
    };
    DRM_CRTCS.write().insert(crtc_id, crtc);

    let mut devices = DRM_DEVICES.write();
    let dev = devices.get_mut(&device_id).ok_or("DRM device not found")?;
    dev.crtc_ids.push(crtc_id);
    Ok(crtc_id)
}

/// Create an encoder for a DRM device.
pub fn encoder_create(
    device_id: u32,
    encoder_type: u32,
    possible_crtcs: u32,
    possible_clones: u32,
) -> Result<u32, &'static str> {
    let enc_id = ENCODER_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let enc = DrmEncoder {
        id: enc_id,
        encoder_type,
        possible_crtcs,
        possible_clones,
        crtc_id: None,
    };
    DRM_ENCODERS.write().insert(enc_id, enc);

    let mut devices = DRM_DEVICES.write();
    let dev = devices.get_mut(&device_id).ok_or("DRM device not found")?;
    dev.encoder_ids.push(enc_id);
    Ok(enc_id)
}

/// Create a connector for a DRM device.
pub fn connector_create(
    device_id: u32,
    connector_type: ConnectorType,
    connector_type_id: u32,
    modes: Vec<DrmMode>,
    width_mm: u32,
    height_mm: u32,
) -> Result<u32, &'static str> {
    let conn_id = CONNECTOR_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let conn = DrmConnector {
        id: conn_id,
        connector_type,
        connector_type_id,
        status: ConnectorStatus::Connected,
        encoder_id: None,
        modes,
        width_mm,
        height_mm,
    };
    DRM_CONNECTORS.write().insert(conn_id, conn);

    let mut devices = DRM_DEVICES.write();
    let dev = devices.get_mut(&device_id).ok_or("DRM device not found")?;
    dev.connector_ids.push(conn_id);
    Ok(conn_id)
}

/// Set a CRTC mode (Linux `drm_mode_setcrtc`).
pub fn set_crtc(
    crtc_id: u32,
    fb_id: u32,
    x: u32,
    y: u32,
    mode: DrmMode,
) -> Result<(), &'static str> {
    {
        let fbs = DRM_FRAMEBUFFERS.read();
        if !fbs.contains_key(&fb_id) {
            return Err("Framebuffer not found");
        }
    }
    let mut crtcs = DRM_CRTCS.write();
    let crtc = crtcs.get_mut(&crtc_id).ok_or("CRTC not found")?;
    crtc.enabled = true;
    crtc.active = true;
    crtc.framebuffer_id = Some(fb_id);
    crtc.x = x;
    crtc.y = y;
    crtc.mode = Some(mode);
    Ok(())
}

/// Disable a CRTC.
pub fn disable_crtc(crtc_id: u32) -> Result<(), &'static str> {
    let mut crtcs = DRM_CRTCS.write();
    let crtc = crtcs.get_mut(&crtc_id).ok_or("CRTC not found")?;
    crtc.enabled = false;
    crtc.active = false;
    crtc.framebuffer_id = None;
    crtc.mode = None;
    Ok(())
}

/// Set a plane (Linux `drm_mode_setplane`).
pub fn set_plane(
    plane_id: u32,
    crtc_id: u32,
    fb_id: u32,
    crtc_x: i32,
    crtc_y: i32,
    crtc_w: u32,
    crtc_h: u32,
    src_x: u32,
    src_y: u32,
    src_w: u32,
    src_h: u32,
) -> Result<(), &'static str> {
    let mut planes = DRM_PLANES.write();
    let plane = planes.get_mut(&plane_id).ok_or("Plane not found")?;
    plane.crtc_id = Some(crtc_id);
    plane.fb_id = Some(fb_id);
    plane.crtc_x = crtc_x;
    plane.crtc_y = crtc_y;
    plane.crtc_w = crtc_w;
    plane.crtc_h = crtc_h;
    plane.src_x = src_x;
    plane.src_y = src_y;
    plane.src_w = src_w;
    plane.src_h = src_h;
    Ok(())
}

/// Attach connector to encoder.
pub fn connector_attach_encoder(connector_id: u32, encoder_id: u32) -> Result<(), &'static str> {
    let mut conns = DRM_CONNECTORS.write();
    let conn = conns.get_mut(&connector_id).ok_or("Connector not found")?;
    conn.encoder_id = Some(encoder_id);
    Ok(())
}

/// Create a dumb buffer (Linux `DRM_IOCTL_MODE_CREATE_DUMB`).
pub fn dumb_buffer_create(
    device_id: u32,
    width: u32,
    height: u32,
    bpp: u32,
) -> Result<DumbBuffer, &'static str> {
    let create_fn = {
        let ops = DRM_DRIVER_OPS.read();
        let driver_ops = ops.get(&device_id).ok_or("DRM driver ops not found")?;
        driver_ops.dumb_create
    };
    (create_fn)(device_id, width, height, bpp)
}

/// Destroy a dumb buffer.
pub fn dumb_buffer_destroy(device_id: u32, handle: u32) -> Result<(), &'static str> {
    let destroy_fn = {
        let ops = DRM_DRIVER_OPS.read();
        let driver_ops = ops.get(&device_id).ok_or("DRM driver ops not found")?;
        driver_ops.dumb_destroy
    };
    (destroy_fn)(device_id, handle)
}

/// Map a dumb buffer for CPU access.
pub fn dumb_buffer_map(device_id: u32, handle: u32) -> Result<u64, &'static str> {
    let map_fn = {
        let ops = DRM_DRIVER_OPS.read();
        let driver_ops = ops.get(&device_id).ok_or("DRM driver ops not found")?;
        driver_ops.dumb_map
    };
    (map_fn)(device_id, handle)
}

/// Get connector modes.
pub fn get_connector_modes(connector_id: u32) -> Result<Vec<DrmMode>, &'static str> {
    let conns = DRM_CONNECTORS.read();
    let conn = conns.get(&connector_id).ok_or("Connector not found")?;
    Ok(conn.modes.clone())
}

/// List all DRM devices.
pub fn list_devices() -> Vec<(u32, String)> {
    DRM_DEVICES
        .read()
        .iter()
        .map(|(id, d)| (*id, d.name.clone()))
        .collect()
}

/// Count registered devices.
pub fn device_count() -> usize {
    DRM_DEVICES.read().len()
}

// ── Software DRM driver ─────────────────────────────────────────────────

fn sw_open(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_postclose(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}

fn sw_dumb_create(
    _dev_id: u32,
    width: u32,
    height: u32,
    bpp: u32,
) -> Result<DumbBuffer, &'static str> {
    let handle = DUMB_HANDLE_COUNTER.fetch_add(1, Ordering::SeqCst);
    let pitch = width * (bpp / 8);
    let size = (pitch as u64) * (height as u64);
    Ok(DumbBuffer {
        handle,
        width,
        height,
        bpp,
        pitch,
        size,
    })
}

fn sw_dumb_destroy(_dev_id: u32, _handle: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_dumb_map(_dev_id: u32, _handle: u32) -> Result<u64, &'static str> {
    Ok(0)
}

/// Software DRM driver ops.
pub fn software_drm_ops() -> DrmDriverOps {
    DrmDriverOps {
        open: sw_open,
        postclose: sw_postclose,
        dumb_create: sw_dumb_create,
        dumb_destroy: sw_dumb_destroy,
        dumb_map: sw_dumb_map,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("gpu: subsystem ready");
    Ok(())
}
