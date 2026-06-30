//! GPU/DRM subsystem
//!
//! Provides DRM/KMS (Direct Rendering Manager / Kernel Mode Setting) framework
//! for display controllers, framebuffers, planes, CRTCs, and encoders.
//! Mirrors Linux's `drivers/gpu/drm/drm_*`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use spin::RwLock;

// Software-backed fbdev / framebuffer-console consumer. Files live in
// `src/drivers/video/`; declared here via `#[path]` because `src/drivers/mod.rs`
// does not carry a `pub mod video;` declaration and must not be edited.
#[path = "../video/mod.rs"]
pub mod video;

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
    /// Backing GEM/dumb buffer-object handle this framebuffer scans out from.
    pub bo_handle: Option<u32>,
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
    /// Monotonic vblank sequence counter, bumped on every page flip.
    pub vblank_seq: u64,
    /// True once a flip has been requested and not yet completed (latched).
    pub pending_flip: bool,
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

/// GEM-lite backing storage: dumb-buffer handle -> allocated pixel bytes.
static DRM_BUFFER_OBJECTS: RwLock<BTreeMap<u32, BufferObject>> = RwLock::new(BTreeMap::new());

/// Idempotency latch for [`init`].
static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Total vblank events observed across all CRTCs (diagnostics).
static VBLANK_TOTAL: AtomicU64 = AtomicU64::new(0);

/// A software buffer object: an allocated, CPU-mappable pixel buffer.
pub struct BufferObject {
    pub handle: u32,
    pub width: u32,
    pub height: u32,
    pub bpp: u32,
    pub pitch: u32,
    pub bytes: Vec<u8>,
}

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
        bo_handle: None,
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
        vblank_seq: 0,
        pending_flip: false,
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

// ── GEM-lite dumb buffers + framebuffer binding ─────────────────────────

/// Create a dumb buffer and return its handle (Linux `DRM_IOCTL_MODE_CREATE_DUMB`).
///
/// Backed by an allocated `Vec<u8>`; use [`buffer_object_addr`] / [`dumb_buffer_map`]
/// to obtain a CPU-visible pointer to the pixels.
pub fn create_dumb(device_id: u32, width: u32, height: u32, bpp: u32) -> Result<u32, &'static str> {
    let bo = dumb_buffer_create(device_id, width, height, bpp)?;
    Ok(bo.handle)
}

/// Return the CPU address of a dumb buffer's backing storage, or an error if unmapped.
pub fn buffer_object_addr(handle: u32) -> Result<u64, &'static str> {
    let objs = DRM_BUFFER_OBJECTS.read();
    let bo = objs.get(&handle).ok_or("buffer object not found")?;
    Ok(bo.bytes.as_ptr() as u64)
}

/// Return `(pitch, size)` for a dumb buffer's backing storage.
pub fn buffer_object_info(handle: u32) -> Result<(u32, usize), &'static str> {
    let objs = DRM_BUFFER_OBJECTS.read();
    let bo = objs.get(&handle).ok_or("buffer object not found")?;
    Ok((bo.pitch, bo.bytes.len()))
}

/// Run `f` against the mutable bytes of a dumb buffer (software scanout writes).
pub fn with_buffer_object<R>(
    handle: u32,
    f: impl FnOnce(&mut [u8]) -> R,
) -> Result<R, &'static str> {
    let mut objs = DRM_BUFFER_OBJECTS.write();
    let bo = objs.get_mut(&handle).ok_or("buffer object not found")?;
    Ok(f(&mut bo.bytes))
}

/// Add a framebuffer that references an existing dumb buffer object
/// (Linux `DRM_IOCTL_MODE_ADDFB2`). The bo's allocation backs scanout.
pub fn add_framebuffer(
    device_id: u32,
    handle: u32,
    width: u32,
    height: u32,
    pitch: u32,
    format: DrmFourCc,
) -> Result<u32, &'static str> {
    let bpp = {
        let objs = DRM_BUFFER_OBJECTS.read();
        let bo = objs.get(&handle).ok_or("add_framebuffer: bo handle not found")?;
        let needed = (pitch as u64) * (height as u64);
        if (bo.bytes.len() as u64) < needed {
            return Err("add_framebuffer: bo too small for pitch*height");
        }
        bo.bpp
    };
    let depth = match format {
        DrmFourCc::ARGB8888 => 32,
        DrmFourCc::XRGB8888 => 24,
        DrmFourCc::RGB888 => 24,
        DrmFourCc::RGB565 => 16,
        _ => 24,
    };
    let fb_id = FB_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let fb = DrmFramebuffer {
        id: fb_id,
        width,
        height,
        pixel_format: format,
        pitches: vec![pitch],
        offsets: vec![0],
        bpp,
        depth,
        bo_handle: Some(handle),
    };
    DRM_FRAMEBUFFERS.write().insert(fb_id, fb);
    let mut devices = DRM_DEVICES.write();
    let dev = devices.get_mut(&device_id).ok_or("DRM device not found")?;
    dev.fb_ids.push(fb_id);
    Ok(fb_id)
}

// ── Atomic-style modeset commit ──────────────────────────────────────────

/// Atomic-style modeset commit (Linux `drm_atomic_commit` shape).
///
/// Validates the full display pipeline — connector → encoder → crtc → plane → fb —
/// then records active state on the CRTC and primary plane in one step.
pub fn set_mode(
    crtc_id: u32,
    connector_id: u32,
    mode: DrmMode,
    fb_id: u32,
) -> Result<(), &'static str> {
    // 1. connector must be connected and offer the requested mode.
    let encoder_id = {
        let conns = DRM_CONNECTORS.read();
        let conn = conns.get(&connector_id).ok_or("set_mode: connector not found")?;
        if conn.status != ConnectorStatus::Connected {
            return Err("set_mode: connector not connected");
        }
        if !conn
            .modes
            .iter()
            .any(|m| m.hdisplay == mode.hdisplay && m.vdisplay == mode.vdisplay)
        {
            return Err("set_mode: requested mode not in connector mode list");
        }
        conn.encoder_id.ok_or("set_mode: connector has no encoder")?
    };

    // 2. encoder must route to this CRTC.
    {
        let encs = DRM_ENCODERS.read();
        let enc = encs.get(&encoder_id).ok_or("set_mode: encoder not found")?;
        let crtc_mask = 1u32 << (crtc_index(crtc_id)?);
        if enc.possible_crtcs & crtc_mask == 0 {
            return Err("set_mode: encoder cannot drive this CRTC");
        }
    }

    // 3. CRTC must exist and have a primary plane.
    let plane_id = {
        let crtcs = DRM_CRTCS.read();
        let crtc = crtcs.get(&crtc_id).ok_or("set_mode: CRTC not found")?;
        crtc.primary_plane.ok_or("set_mode: CRTC has no primary plane")?
    };

    // 4. framebuffer must exist and its format must be supported by the plane.
    let (fb_w, fb_h) = {
        let fbs = DRM_FRAMEBUFFERS.read();
        let fb = fbs.get(&fb_id).ok_or("set_mode: framebuffer not found")?;
        let planes = DRM_PLANES.read();
        let plane = planes.get(&plane_id).ok_or("set_mode: primary plane not found")?;
        if !plane.formats.is_empty() && !plane.formats.contains(&fb.pixel_format) {
            return Err("set_mode: fb format not supported by primary plane");
        }
        (fb.width, fb.height)
    };

    // 5. commit: record active pipeline state.
    let (mw, mh) = (mode.hdisplay as u32, mode.vdisplay as u32);
    {
        let mut crtcs = DRM_CRTCS.write();
        let crtc = crtcs.get_mut(&crtc_id).ok_or("set_mode: CRTC not found")?;
        crtc.enabled = true;
        crtc.active = true;
        crtc.framebuffer_id = Some(fb_id);
        crtc.mode = Some(mode);
        crtc.x = 0;
        crtc.y = 0;
    }
    {
        let mut planes = DRM_PLANES.write();
        let plane = planes.get_mut(&plane_id).ok_or("set_mode: primary plane not found")?;
        plane.crtc_id = Some(crtc_id);
        plane.fb_id = Some(fb_id);
        plane.crtc_x = 0;
        plane.crtc_y = 0;
        plane.crtc_w = mw;
        plane.crtc_h = mh;
        plane.src_x = 0;
        plane.src_y = 0;
        plane.src_w = fb_w << 16; // 16.16 fixed point, DRM convention.
        plane.src_h = fb_h << 16;
    }
    Ok(())
}

/// Look up a CRTC's pipe index (used to build encoder `possible_crtcs` masks).
fn crtc_index(crtc_id: u32) -> Result<u32, &'static str> {
    let crtcs = DRM_CRTCS.read();
    let crtc = crtcs.get(&crtc_id).ok_or("CRTC not found")?;
    Ok(crtc.index)
}

// ── vblank + page flip ───────────────────────────────────────────────────

/// Flip the scanout framebuffer on a CRTC and bump its vblank/sequence counter
/// (Linux `DRM_IOCTL_MODE_PAGE_FLIP`). Returns the new vblank sequence.
pub fn page_flip(crtc_id: u32, fb_id: u32) -> Result<u64, &'static str> {
    {
        let fbs = DRM_FRAMEBUFFERS.read();
        if !fbs.contains_key(&fb_id) {
            return Err("page_flip: framebuffer not found");
        }
    }
    let mut crtcs = DRM_CRTCS.write();
    let crtc = crtcs.get_mut(&crtc_id).ok_or("page_flip: CRTC not found")?;
    if !crtc.active {
        return Err("page_flip: CRTC not active (set a mode first)");
    }
    // Latch the new framebuffer and complete the vblank in one software step.
    crtc.pending_flip = true;
    crtc.framebuffer_id = Some(fb_id);
    // Mirror onto the primary plane so scanout state stays coherent.
    if let Some(plane_id) = crtc.primary_plane {
        if let Some(plane) = DRM_PLANES.write().get_mut(&plane_id) {
            plane.fb_id = Some(fb_id);
        }
    }
    crtc.vblank_seq += 1;
    crtc.pending_flip = false;
    VBLANK_TOTAL.fetch_add(1, Ordering::SeqCst);
    Ok(crtc.vblank_seq)
}

/// Return the current vblank sequence counter for a CRTC.
pub fn vblank_count(crtc_id: u32) -> Result<u64, &'static str> {
    let crtcs = DRM_CRTCS.read();
    let crtc = crtcs.get(&crtc_id).ok_or("CRTC not found")?;
    Ok(crtc.vblank_seq)
}

/// The framebuffer currently scanned out by a CRTC, if any.
pub fn crtc_scanout_fb(crtc_id: u32) -> Option<u32> {
    DRM_CRTCS.read().get(&crtc_id).and_then(|c| c.framebuffer_id)
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
    if bpp == 0 || bpp % 8 != 0 {
        return Err("dumb_create: bpp must be a non-zero multiple of 8");
    }
    if width == 0 || height == 0 {
        return Err("dumb_create: zero dimension");
    }
    let handle = DUMB_HANDLE_COUNTER.fetch_add(1, Ordering::SeqCst);
    let pitch = width * (bpp / 8);
    let size = (pitch as u64) * (height as u64);
    // Allocate real backing storage so map/addr yields a usable pointer.
    let bo = BufferObject {
        handle,
        width,
        height,
        bpp,
        pitch,
        bytes: vec![0u8; size as usize],
    };
    DRM_BUFFER_OBJECTS.write().insert(handle, bo);
    Ok(DumbBuffer {
        handle,
        width,
        height,
        bpp,
        pitch,
        size,
    })
}

fn sw_dumb_destroy(_dev_id: u32, handle: u32) -> Result<(), &'static str> {
    if DRM_BUFFER_OBJECTS.write().remove(&handle).is_none() {
        return Err("dumb_destroy: handle not found");
    }
    Ok(())
}
fn sw_dumb_map(_dev_id: u32, handle: u32) -> Result<u64, &'static str> {
    let objs = DRM_BUFFER_OBJECTS.read();
    let bo = objs.get(&handle).ok_or("dumb_map: handle not found")?;
    Ok(bo.bytes.as_ptr() as u64)
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

/// Build a standard CVT-ish DRM mode. `flags` mirrors `DRM_MODE_FLAG_*`.
pub fn display_mode(
    name: &str,
    clock_khz: u32,
    hdisplay: u16,
    hsync_start: u16,
    hsync_end: u16,
    htotal: u16,
    vdisplay: u16,
    vsync_start: u16,
    vsync_end: u16,
    vtotal: u16,
    vrefresh: u32,
    flags: u32,
) -> DrmMode {
    DrmMode {
        clock: clock_khz,
        hdisplay,
        hsync_start,
        hsync_end,
        htotal,
        hskew: 0,
        vdisplay,
        vsync_start,
        vsync_end,
        vtotal,
        vscan: 0,
        vrefresh,
        flags,
        name: String::from(name),
    }
}

/// Standard 1024x768@60 (XGA) mode.
pub fn mode_1024x768_60() -> DrmMode {
    display_mode(
        "1024x768", 65000, 1024, 1048, 1184, 1344, 768, 771, 777, 806, 60, 0,
    )
}

/// Probe a connector for its supported modes (Linux `drm_helper_probe_single_connector_modes`).
pub fn probe_connector(connector_id: u32) -> Result<Vec<DrmMode>, &'static str> {
    get_connector_modes(connector_id)
}

// ── Init ────────────────────────────────────────────────────────────────

/// Idempotent: bring up a virtual DRM device with a connected 1024x768@60
/// display, wire the KMS pipeline, allocate a dumb buffer + framebuffer, and
/// commit the mode. Safe to call multiple times.
pub fn init() -> Result<(), &'static str> {
    if INITIALIZED.swap(true, Ordering::SeqCst) {
        return Ok(());
    }

    // 1. Virtual DRM device backed by the software driver.
    let dev_id = register_device("virtual-drm", software_drm_ops())?;

    // 2. KMS objects: primary plane, CRTC (pipe 0), encoder, connector.
    let formats = vec![DrmFourCc::XRGB8888, DrmFourCc::ARGB8888];
    let primary = plane_create(dev_id, PlaneType::Primary, 0x1, formats.clone())?;
    let _cursor = plane_create(dev_id, PlaneType::Cursor, 0x1, vec![DrmFourCc::ARGB8888])?;
    let crtc = crtc_create(dev_id, 0, primary, None, 256)?;
    // possible_crtcs bit 0 == pipe index 0 (this CRTC's index).
    let encoder = encoder_create(dev_id, 0, 0x1, 0)?;
    let mode = mode_1024x768_60();
    let connector = connector_create(
        dev_id,
        ConnectorType::Virtual,
        1,
        vec![mode.clone()],
        260,
        195,
    )?;
    connector_attach_encoder(connector, encoder)?;
    {
        let mut encs = DRM_ENCODERS.write();
        if let Some(enc) = encs.get_mut(&encoder) {
            enc.crtc_id = Some(crtc);
        }
    }

    // 3. Dumb buffer + framebuffer referencing it.
    let (w, h, bpp) = (1024u32, 768u32, 32u32);
    let handle = create_dumb(dev_id, w, h, bpp)?;
    let (pitch, _size) = buffer_object_info(handle)?;
    let fb = add_framebuffer(dev_id, handle, w, h, pitch, DrmFourCc::XRGB8888)?;

    // 4. Atomic modeset commit across the validated pipeline.
    set_mode(crtc, connector, mode, fb)?;

    {
        let mut devices = DRM_DEVICES.write();
        if let Some(dev) = devices.get_mut(&dev_id) {
            dev.mode_config_width = w;
            dev.mode_config_height = h;
        }
    }

    // 5. Attach the fbdev/console consumer to the scanout framebuffer.
    video::init_with_framebuffer(dev_id, crtc, fb, handle, w, h, pitch)?;

    crate::serial_println!(
        "gpu: virtual-drm up: 1024x768@60 crtc={} conn={} fb={} bo={} pitch={} ({} bytes/scanline)",
        crtc,
        connector,
        fb,
        handle,
        pitch,
        pitch
    );
    Ok(())
}
