//! Media (V4L2 media controller) driver subsystem
//!
//! Provides media device and entity management for video capture/display.
//! Mirrors Linux's `drivers/media/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// Media entity type (Linux `enum media_entity_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaType {
    Device,
    Entity,
    Pad,
    Link,
    Interface,
}

/// Media entity function (Linux `MEDIA_ENT_F_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaFunction {
    Unknown,
    V4l2Subdev,
    VideoDevice,
    AudioDevice,
    DvbDevice,
    Sensor,
    Decoder,
    Encoder,
    M2mDevice,
    IfVidBridge,
    IfAudBridge,
}

/// Media entity (Linux `struct media_entity`).
pub struct MediaEntity {
    pub id: u32,
    pub name: String,
    pub function: MediaFunction,
    pub pad_ids: Vec<u32>,
    pub link_ids: Vec<u32>,
}

/// Media pad (Linux `struct media_pad`).
pub struct MediaPad {
    pub id: u32,
    pub entity_id: u32,
    pub index: u32,
    pub flags: u32,
}

/// Media link (Linux `struct media_link`).
pub struct MediaLink {
    pub id: u32,
    pub source_pad: u32,
    pub sink_pad: u32,
    pub flags: u32,
}

/// Media device (Linux `struct media_device`).
pub struct MediaDevice {
    pub id: u32,
    pub name: String,
    pub model: String,
    pub serial: String,
    pub entity_ids: Vec<u32>,
}

// ── Registry ────────────────────────────────────────────────────────────

static DEV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static ENTITY_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static PAD_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static LINK_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static MEDIA_DEVS: RwLock<BTreeMap<u32, MediaDevice>> = RwLock::new(BTreeMap::new());
static MEDIA_ENTITIES: RwLock<BTreeMap<u32, MediaEntity>> = RwLock::new(BTreeMap::new());
static MEDIA_PADS: RwLock<BTreeMap<u32, MediaPad>> = RwLock::new(BTreeMap::new());
static MEDIA_LINKS: RwLock<BTreeMap<u32, MediaLink>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a media device (Linux `media_device_register`).
pub fn register_device(name: &str, model: &str, serial: &str) -> Result<u32, &'static str> {
    let id = DEV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = MediaDevice {
        id,
        name: String::from(name),
        model: String::from(model),
        serial: String::from(serial),
        entity_ids: Vec::new(),
    };
    MEDIA_DEVS.write().insert(id, dev);
    Ok(id)
}

/// Register a media entity (Linux `media_entity_register`).
pub fn register_entity(
    dev_id: u32,
    name: &str,
    function: MediaFunction,
) -> Result<u32, &'static str> {
    if !MEDIA_DEVS.read().contains_key(&dev_id) {
        return Err("Media device not found");
    }
    let id = ENTITY_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let entity = MediaEntity {
        id,
        name: String::from(name),
        function,
        pad_ids: Vec::new(),
        link_ids: Vec::new(),
    };
    MEDIA_ENTITIES.write().insert(id, entity);
    let mut devs = MEDIA_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.entity_ids.push(id);
    }
    Ok(id)
}

/// Register a media pad (Linux `media_entity_pads_init`).
pub fn register_pad(entity_id: u32, index: u32, flags: u32) -> Result<u32, &'static str> {
    if !MEDIA_ENTITIES.read().contains_key(&entity_id) {
        return Err("Media entity not found");
    }
    let id = PAD_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let pad = MediaPad {
        id,
        entity_id,
        index,
        flags,
    };
    MEDIA_PADS.write().insert(id, pad);
    let mut entities = MEDIA_ENTITIES.write();
    if let Some(entity) = entities.get_mut(&entity_id) {
        entity.pad_ids.push(id);
    }
    Ok(id)
}

/// Create a link between two pads (Linux `media_create_pad_link`).
pub fn create_link(source_pad: u32, sink_pad: u32, flags: u32) -> Result<u32, &'static str> {
    if !MEDIA_PADS.read().contains_key(&source_pad) {
        return Err("Source pad not found");
    }
    if !MEDIA_PADS.read().contains_key(&sink_pad) {
        return Err("Sink pad not found");
    }
    let id = LINK_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let link = MediaLink {
        id,
        source_pad,
        sink_pad,
        flags,
    };
    MEDIA_LINKS.write().insert(id, link);
    Ok(id)
}

/// List media devices.
pub fn list_devices() -> Vec<(u32, String, String, usize)> {
    MEDIA_DEVS
        .read()
        .iter()
        .map(|(id, d)| (*id, d.name.clone(), d.model.clone(), d.entity_ids.len()))
        .collect()
}

/// List entities for a device.
pub fn list_entities(dev_id: u32) -> Vec<(u32, String, MediaFunction)> {
    let dev = MEDIA_DEVS.read();
    let entity_ids = match dev.get(&dev_id) {
        Some(d) => d.entity_ids.clone(),
        None => return Vec::new(),
    };
    let entities = MEDIA_ENTITIES.read();
    entity_ids
        .iter()
        .filter_map(|eid| {
            entities
                .get(eid)
                .map(|e| (*eid, e.name.clone(), e.function))
        })
        .collect()
}

/// Count registered devices.
pub fn device_count() -> usize {
    MEDIA_DEVS.read().len()
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    if !MEDIA_DEVS.read().is_empty() {
        return Ok(());
    }

    let dev_id = register_device("sw-media", "RustOS Media", "SW0001")?;
    let sensor_id = register_entity(dev_id, "sw-sensor", MediaFunction::Sensor)?;
    let proc_id = register_entity(dev_id, "sw-processor", MediaFunction::M2mDevice)?;

    let sensor_pad = register_pad(sensor_id, 0, 0x01)?;
    let proc_sink = register_pad(proc_id, 0, 0x02)?;
    let proc_src = register_pad(proc_id, 1, 0x01)?;

    create_link(sensor_pad, proc_sink, 0)?;
    let _ = proc_src;

    crate::serial_println!(
        "media: software media device registered (dev_id={}, 2 entities, 1 link)",
        dev_id
    );
    Ok(())
}
