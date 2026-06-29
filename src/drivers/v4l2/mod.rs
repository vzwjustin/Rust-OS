//! V4L2 (Video4Linux2) subsystem
//!
//! Provides V4L2 framework for video capture, output, and streaming.
//! Mirrors Linux's `drivers/media/v4l2-core/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// V4L2 device (Linux `struct video_device`).
pub struct V4l2Device {
    pub id: u32,
    pub name: String,
    pub dev_type: V4l2DevType,
    pub ops: V4l2Ops,
    pub minor: u32,
    pub vfl_dir: V4l2Dir,
    pub capabilities: u32,
    pub current_input: u32,
    pub current_norm: u32,
    pub streaming: bool,
    pub queue: Vec<V4l2Buffer>,
}

/// V4L2 device type (Linux `enum vfl_devnode_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V4l2DevType {
    Video,
    Vbi,
    Radio,
    Subdev,
    Sdr,
    Touch,
    Meta,
}

/// V4L2 direction (Linux `enum vfl_devnode_direction`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V4l2Dir {
    M2m, // Memory-to-memory
    Capture,
    Output,
}

/// V4L2 operations (Linux `struct v4l2_ioctl_ops` subset).
pub struct V4l2Ops {
    pub querycap: fn(dev_id: u32) -> Result<V4l2Capability, &'static str>,
    pub enum_fmt:
        fn(dev_id: u32, index: u32, type_: V4l2BufType) -> Result<V4l2FmtDesc, &'static str>,
    pub g_fmt: fn(dev_id: u32, type_: V4l2BufType) -> Result<V4l2Format, &'static str>,
    pub s_fmt: fn(dev_id: u32, format: &V4l2Format) -> Result<V4l2Format, &'static str>,
    pub reqbufs: fn(
        dev_id: u32,
        count: u32,
        type_: V4l2BufType,
        memory: V4l2Memory,
    ) -> Result<u32, &'static str>,
    pub qbuf: fn(dev_id: u32, buf: &V4l2Buffer) -> Result<(), &'static str>,
    pub dqbuf: fn(dev_id: u32, type_: V4l2BufType) -> Result<V4l2Buffer, &'static str>,
    pub streamon: fn(dev_id: u32, type_: V4l2BufType) -> Result<(), &'static str>,
    pub streamoff: fn(dev_id: u32, type_: V4l2BufType) -> Result<(), &'static str>,
    pub enum_input: fn(dev_id: u32, index: u32) -> Result<V4l2Input, &'static str>,
    pub s_input: fn(dev_id: u32, index: u32) -> Result<(), &'static str>,
    pub g_std: fn(dev_id: u32) -> Result<u32, &'static str>,
    pub s_std: fn(dev_id: u32, std: u32) -> Result<(), &'static str>,
}

/// V4L2 capability (Linux `struct v4l2_capability`).
#[derive(Debug, Clone)]
pub struct V4l2Capability {
    pub driver: String,
    pub card: String,
    pub bus_info: String,
    pub version: u32,
    pub capabilities: u32,
    pub device_caps: u32,
}

/// V4L2 format description (Linux `struct v4l2_fmtdesc`).
#[derive(Debug, Clone)]
pub struct V4l2FmtDesc {
    pub index: u32,
    pub type_: V4l2BufType,
    pub pixelformat: u32,
    pub description: String,
}

/// V4L2 buffer type (Linux `enum v4l2_buf_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V4l2BufType {
    VideoCapture,
    VideoOutput,
    VideoOverlay,
    VbiCapture,
    VbiOutput,
    SlicedVbiCapture,
    SlicedVbiOutput,
    VideoOutputOverlay,
    VideoCaptureMplane,
    VideoOutputMplane,
    SdrCapture,
    SdrOutput,
    MetaCapture,
    MetaOutput,
}

/// V4L2 memory type (Linux `enum v4l2_memory`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V4l2Memory {
    Mmap,
    Userptr,
    Overlay,
    Dmabuf,
}

/// V4L2 format (Linux `struct v4l2_format`).
#[derive(Debug, Clone)]
pub struct V4l2Format {
    pub type_: V4l2BufType,
    pub width: u32,
    pub height: u32,
    pub pixelformat: u32,
    pub field: V4l2Field,
    pub bytesperline: u32,
    pub sizeimage: u32,
    pub colorspace: u32,
}

/// V4L2 field order (Linux `enum v4l2_field`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V4l2Field {
    Any,
    None,
    Top,
    Bottom,
    Interlaced,
    SeqTb,
    SeqBt,
    Alternate,
    InterlacedTb,
    InterlacedBt,
}

/// V4L2 buffer (Linux `struct v4l2_buffer`).
#[derive(Debug, Clone)]
pub struct V4l2Buffer {
    pub index: u32,
    pub type_: V4l2BufType,
    pub memory: V4l2Memory,
    pub flags: u32,
    pub field: V4l2Field,
    pub length: u32,
    pub bytesused: u32,
    pub sequence: u32,
    pub timestamp_ns: u64,
}

/// V4L2 input (Linux `struct v4l2_input`).
#[derive(Debug, Clone)]
pub struct V4l2Input {
    pub index: u32,
    pub name: String,
    pub type_: u32,
    pub std: u32,
    pub status: u32,
}

// ── Registry ────────────────────────────────────────────────────────────

static DEV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static MINOR_COUNTER: AtomicU32 = AtomicU32::new(0);

static V4L2_DEVS: RwLock<BTreeMap<u32, V4l2Device>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a V4L2 device (Linux `video_register_device`).
pub fn register_device(
    name: &str,
    dev_type: V4l2DevType,
    vfl_dir: V4l2Dir,
    ops: V4l2Ops,
    capabilities: u32,
) -> Result<u32, &'static str> {
    let id = DEV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let minor = MINOR_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = V4l2Device {
        id,
        name: String::from(name),
        dev_type,
        ops,
        minor,
        vfl_dir,
        capabilities,
        current_input: 0,
        current_norm: 0,
        streaming: false,
        queue: Vec::new(),
    };
    V4L2_DEVS.write().insert(id, dev);
    Ok(id)
}

/// Query capabilities (Linux `VIDIOC_QUERYCAP`).
pub fn querycap(dev_id: u32) -> Result<V4l2Capability, &'static str> {
    let query_fn = {
        let devs = V4L2_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("V4L2 device not found")?;
        dev.ops.querycap
    };
    (query_fn)(dev_id)
}

/// Enumerate formats (Linux `VIDIOC_ENUM_FMT`).
pub fn enum_fmt(dev_id: u32, index: u32, type_: V4l2BufType) -> Result<V4l2FmtDesc, &'static str> {
    let enum_fn = {
        let devs = V4L2_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("V4L2 device not found")?;
        dev.ops.enum_fmt
    };
    (enum_fn)(dev_id, index, type_)
}

/// Get format (Linux `VIDIOC_G_FMT`).
pub fn g_fmt(dev_id: u32, type_: V4l2BufType) -> Result<V4l2Format, &'static str> {
    let get_fn = {
        let devs = V4L2_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("V4L2 device not found")?;
        dev.ops.g_fmt
    };
    (get_fn)(dev_id, type_)
}

/// Set format (Linux `VIDIOC_S_FMT`).
pub fn s_fmt(dev_id: u32, format: &V4l2Format) -> Result<V4l2Format, &'static str> {
    let set_fn = {
        let devs = V4L2_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("V4L2 device not found")?;
        dev.ops.s_fmt
    };
    (set_fn)(dev_id, format)
}

/// Request buffers (Linux `VIDIOC_REQBUFS`).
pub fn reqbufs(
    dev_id: u32,
    count: u32,
    type_: V4l2BufType,
    memory: V4l2Memory,
) -> Result<u32, &'static str> {
    let req_fn = {
        let devs = V4L2_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("V4L2 device not found")?;
        dev.ops.reqbufs
    };
    let allocated = (req_fn)(dev_id, count, type_, memory)?;

    let mut devs = V4L2_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.queue.clear();
        for i in 0..allocated {
            dev.queue.push(V4l2Buffer {
                index: i,
                type_,
                memory,
                flags: 0,
                field: V4l2Field::None,
                length: 0,
                bytesused: 0,
                sequence: 0,
                timestamp_ns: 0,
            });
        }
    }
    Ok(allocated)
}

/// Queue a buffer (Linux `VIDIOC_QBUF`).
pub fn qbuf(dev_id: u32, buf: &V4l2Buffer) -> Result<(), &'static str> {
    let q_fn = {
        let devs = V4L2_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("V4L2 device not found")?;
        dev.ops.qbuf
    };
    (q_fn)(dev_id, buf)?;

    let mut devs = V4L2_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        if let Some(existing) = dev.queue.get_mut(buf.index as usize) {
            *existing = buf.clone();
        }
    }
    Ok(())
}

/// Dequeue a buffer (Linux `VIDIOC_DQBUF`).
pub fn dqbuf(dev_id: u32, type_: V4l2BufType) -> Result<V4l2Buffer, &'static str> {
    let dq_fn = {
        let devs = V4L2_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("V4L2 device not found")?;
        dev.ops.dqbuf
    };
    (dq_fn)(dev_id, type_)
}

/// Start streaming (Linux `VIDIOC_STREAMON`).
pub fn streamon(dev_id: u32, type_: V4l2BufType) -> Result<(), &'static str> {
    let on_fn = {
        let devs = V4L2_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("V4L2 device not found")?;
        dev.ops.streamon
    };
    (on_fn)(dev_id, type_)?;

    let mut devs = V4L2_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.streaming = true;
    }
    Ok(())
}

/// Stop streaming (Linux `VIDIOC_STREAMOFF`).
pub fn streamoff(dev_id: u32, type_: V4l2BufType) -> Result<(), &'static str> {
    let off_fn = {
        let devs = V4L2_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("V4L2 device not found")?;
        dev.ops.streamoff
    };
    (off_fn)(dev_id, type_)?;

    let mut devs = V4L2_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.streaming = false;
        dev.queue.clear();
    }
    Ok(())
}

/// Enumerate inputs (Linux `VIDIOC_ENUMINPUT`).
pub fn enum_input(dev_id: u32, index: u32) -> Result<V4l2Input, &'static str> {
    let enum_fn = {
        let devs = V4L2_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("V4L2 device not found")?;
        dev.ops.enum_input
    };
    (enum_fn)(dev_id, index)
}

/// Set input (Linux `VIDIOC_S_INPUT`).
pub fn s_input(dev_id: u32, index: u32) -> Result<(), &'static str> {
    let set_fn = {
        let devs = V4L2_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("V4L2 device not found")?;
        dev.ops.s_input
    };
    (set_fn)(dev_id, index)?;

    let mut devs = V4L2_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.current_input = index;
    }
    Ok(())
}

/// List all V4L2 devices.
pub fn list_devices() -> Vec<(u32, String, V4l2DevType, V4l2Dir, bool)> {
    V4L2_DEVS
        .read()
        .iter()
        .map(|(id, d)| (*id, d.name.clone(), d.dev_type, d.vfl_dir, d.streaming))
        .collect()
}

/// Count registered devices.
pub fn device_count() -> usize {
    V4L2_DEVS.read().len()
}

// ── Software V4L2 ───────────────────────────────────────────────────────

fn sw_querycap(dev_id: u32) -> Result<V4l2Capability, &'static str> {
    let devs = V4L2_DEVS.read();
    let dev = devs.get(&dev_id).ok_or("V4L2 device not found")?;
    Ok(V4l2Capability {
        driver: String::from("sw-v4l2"),
        card: dev.name.clone(),
        bus_info: String::from("platform:sw-v4l2"),
        version: 0x0001_0000,
        capabilities: dev.capabilities,
        device_caps: dev.capabilities,
    })
}
fn sw_enum_fmt(_dev_id: u32, index: u32, _type_: V4l2BufType) -> Result<V4l2FmtDesc, &'static str> {
    match index {
        0 => Ok(V4l2FmtDesc {
            index: 0,
            type_: V4l2BufType::VideoCapture,
            pixelformat: 0x32595559, // YUYV
            description: String::from("YUYV 4:2:2"),
        }),
        1 => Ok(V4l2FmtDesc {
            index: 1,
            type_: V4l2BufType::VideoCapture,
            pixelformat: 0x47504A4D, // MJPG
            description: String::from("Motion-JPEG"),
        }),
        _ => Err("Format index out of range"),
    }
}
fn sw_g_fmt(_dev_id: u32, _type_: V4l2BufType) -> Result<V4l2Format, &'static str> {
    Ok(V4l2Format {
        type_: V4l2BufType::VideoCapture,
        width: 640,
        height: 480,
        pixelformat: 0x32595559, // YUYV
        field: V4l2Field::None,
        bytesperline: 640 * 2,
        sizeimage: 640 * 480 * 2,
        colorspace: 1, // SRGB
    })
}
fn sw_s_fmt(_dev_id: u32, format: &V4l2Format) -> Result<V4l2Format, &'static str> {
    Ok(format.clone())
}
fn sw_reqbufs(
    _dev_id: u32,
    count: u32,
    _type_: V4l2BufType,
    _memory: V4l2Memory,
) -> Result<u32, &'static str> {
    Ok(count)
}
fn sw_qbuf(_dev_id: u32, _buf: &V4l2Buffer) -> Result<(), &'static str> {
    Ok(())
}
fn sw_dqbuf(_dev_id: u32, _type_: V4l2BufType) -> Result<V4l2Buffer, &'static str> {
    Ok(V4l2Buffer {
        index: 0,
        type_: V4l2BufType::VideoCapture,
        memory: V4l2Memory::Mmap,
        flags: 0,
        field: V4l2Field::None,
        length: 640 * 480 * 2,
        bytesused: 640 * 480 * 2,
        sequence: 0,
        timestamp_ns: 0,
    })
}
fn sw_streamon(_dev_id: u32, _type_: V4l2BufType) -> Result<(), &'static str> {
    Ok(())
}
fn sw_streamoff(_dev_id: u32, _type_: V4l2BufType) -> Result<(), &'static str> {
    Ok(())
}
fn sw_enum_input(_dev_id: u32, index: u32) -> Result<V4l2Input, &'static str> {
    match index {
        0 => Ok(V4l2Input {
            index: 0,
            name: String::from("Camera 0"),
            type_: 2, // Camera
            std: 0,
            status: 0,
        }),
        _ => Err("Input index out of range"),
    }
}
fn sw_s_input(_dev_id: u32, _index: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_g_std(_dev_id: u32) -> Result<u32, &'static str> {
    Ok(0)
}
fn sw_s_std(_dev_id: u32, _std: u32) -> Result<(), &'static str> {
    Ok(())
}

/// Software V4L2 ops.
pub fn software_v4l2_ops() -> V4l2Ops {
    V4l2Ops {
        querycap: sw_querycap,
        enum_fmt: sw_enum_fmt,
        g_fmt: sw_g_fmt,
        s_fmt: sw_s_fmt,
        reqbufs: sw_reqbufs,
        qbuf: sw_qbuf,
        dqbuf: sw_dqbuf,
        streamon: sw_streamon,
        streamoff: sw_streamoff,
        enum_input: sw_enum_input,
        s_input: sw_s_input,
        g_std: sw_g_std,
        s_std: sw_s_std,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    let ops = software_v4l2_ops();
    // Register a video capture device
    let dev_id = register_device(
        "sw-v4l2-cam0",
        V4l2DevType::Video,
        V4l2Dir::Capture,
        ops,
        0x80000001,
    )?;

    // Query capabilities
    let _cap = querycap(dev_id)?;

    // Enumerate formats
    let _fmt = enum_fmt(dev_id, 0, V4l2BufType::VideoCapture)?;

    // Set format
    let format = V4l2Format {
        type_: V4l2BufType::VideoCapture,
        width: 640,
        height: 480,
        pixelformat: 0x32595559,
        field: V4l2Field::None,
        bytesperline: 1280,
        sizeimage: 614400,
        colorspace: 1,
    };
    let _result = s_fmt(dev_id, &format)?;

    // Request buffers
    let count = reqbufs(dev_id, 4, V4l2BufType::VideoCapture, V4l2Memory::Mmap)?;

    // Queue a buffer
    let buf = V4l2Buffer {
        index: 0,
        type_: V4l2BufType::VideoCapture,
        memory: V4l2Memory::Mmap,
        flags: 0,
        field: V4l2Field::None,
        length: 614400,
        bytesused: 0,
        sequence: 0,
        timestamp_ns: 0,
    };
    qbuf(dev_id, &buf)?;

    // Start streaming
    streamon(dev_id, V4l2BufType::VideoCapture)?;

    // Dequeue a buffer
    let _done = dqbuf(dev_id, V4l2BufType::VideoCapture)?;

    // Stop streaming
    streamoff(dev_id, V4l2BufType::VideoCapture)?;

    let _ = count;
    Ok(())
}
