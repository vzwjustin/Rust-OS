//! IIO (Industrial I/O) subsystem
//!
//! Provides ADC, DAC, accelerometer, gyroscope, and other sensor
//! registration with channel-based data access. Mirrors Linux's
//! `drivers/iio/industrialio-core.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// IIO channel type (Linux `enum iio_chan_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IioChanType {
    Voltage,
    Current,
    Power,
    Energy,
    Charge,
    Temp,
    Accel,
    Gyro,
    Magn,
    Pressure,
    Humidity,
    Proximity,
    Light,
    Intensity,
    Illuminance,
    Rotation,
    Angle,
    Timestamp,
}

impl IioChanType {
    pub fn unit(self) -> &'static str {
        match self {
            IioChanType::Voltage => "mV",
            IioChanType::Current => "mA",
            IioChanType::Power => "mW",
            IioChanType::Energy => "J",
            IioChanType::Charge => "C",
            IioChanType::Temp => "mC",
            IioChanType::Accel => "m/s^2",
            IioChanType::Gyro => "rad/s",
            IioChanType::Magn => "Gauss",
            IioChanType::Pressure => "kPa",
            IioChanType::Humidity => "%",
            IioChanType::Proximity => "raw",
            IioChanType::Light => "lux",
            IioChanType::Intensity => "raw",
            IioChanType::Illuminance => "lux",
            IioChanType::Rotation => "rpm",
            IioChanType::Angle => "deg",
            IioChanType::Timestamp => "ns",
        }
    }
}

/// Channel info (Linux `struct iio_chan_spec`).
#[derive(Debug, Clone)]
pub struct IioChannel {
    pub chan_type: IioChanType,
    pub channel: u32,
    pub label: String,
    pub scale: f64,  // Multiplier to convert raw to standard unit
    pub offset: f64, // Offset added to raw * scale
    pub raw_min: i64,
    pub raw_max: i64,
}

/// IIO device operations (Linux `struct iio_info`).
pub struct IioOps {
    pub read_raw: fn(chan_type: IioChanType, channel: u32) -> Result<i64, &'static str>,
    pub write_raw: fn(chan_type: IioChanType, channel: u32, val: i64) -> Result<(), &'static str>,
    pub get_channels: fn() -> Vec<IioChannel>,
    pub get_name: fn() -> &'static str,
}

struct IioDevice {
    id: u32,
    name: String,
    ops: &'static IioOps,
    channels: Vec<IioChannel>,
    buffer_enabled: bool,
    buffer_length: usize,
}

// ── CPU voltage ADC ─────────────────────────────────────────────────────

fn cpu_adc_read(chan_type: IioChanType, channel: u32) -> Result<i64, &'static str> {
    match (chan_type, channel) {
        (IioChanType::Voltage, 0) => Ok(1000), // 1.000V (raw, scale=1000 → mV)
        (IioChanType::Voltage, 1) => Ok(3300), // 3.300V
        (IioChanType::Voltage, 2) => Ok(5000), // 5.000V
        (IioChanType::Voltage, 3) => Ok(12000), // 12.000V
        (IioChanType::Temp, 0) => Ok(45000),   // 45.000°C
        _ => Err("Invalid IIO channel"),
    }
}

fn cpu_adc_write(_t: IioChanType, _c: u32, _v: i64) -> Result<(), &'static str> {
    Err("IIO ADC is read-only")
}

fn cpu_adc_channels() -> Vec<IioChannel> {
    let mut chs = Vec::new();
    chs.push(IioChannel {
        chan_type: IioChanType::Voltage,
        channel: 0,
        label: String::from("vcore"),
        scale: 1.0,
        offset: 0.0,
        raw_min: 0,
        raw_max: 65535,
    });
    chs.push(IioChannel {
        chan_type: IioChanType::Voltage,
        channel: 1,
        label: String::from("3v3"),
        scale: 1.0,
        offset: 0.0,
        raw_min: 0,
        raw_max: 65535,
    });
    chs.push(IioChannel {
        chan_type: IioChanType::Voltage,
        channel: 2,
        label: String::from("5v"),
        scale: 1.0,
        offset: 0.0,
        raw_min: 0,
        raw_max: 65535,
    });
    chs.push(IioChannel {
        chan_type: IioChanType::Voltage,
        channel: 3,
        label: String::from("12v"),
        scale: 1.0,
        offset: 0.0,
        raw_min: 0,
        raw_max: 65535,
    });
    chs.push(IioChannel {
        chan_type: IioChanType::Temp,
        channel: 0,
        label: String::from("cpu_temp"),
        scale: 1.0,
        offset: 0.0,
        raw_min: -40000,
        raw_max: 125000,
    });
    chs
}

fn cpu_adc_name() -> &'static str {
    "cpu-adc"
}

pub static CPU_ADC_OPS: IioOps = IioOps {
    read_raw: cpu_adc_read,
    write_raw: cpu_adc_write,
    get_channels: cpu_adc_channels,
    get_name: cpu_adc_name,
};

// ── Registry ────────────────────────────────────────────────────────────

static IIO_DEVICES: RwLock<BTreeMap<u32, IioDevice>> = RwLock::new(BTreeMap::new());
static NEXT_IIO_ID: AtomicU32 = AtomicU32::new(0);

// ── Public API ──────────────────────────────────────────────────────────

/// Register an IIO device (Linux `iio_device_register`).
pub fn register_device(name: &str, ops: &'static IioOps) -> Result<u32, &'static str> {
    let channels = (ops.get_channels)();
    if channels.is_empty() {
        return Err("IIO device must have at least one channel");
    }
    let id = NEXT_IIO_ID.fetch_add(1, Ordering::SeqCst);
    IIO_DEVICES.write().insert(
        id,
        IioDevice {
            id,
            name: String::from(name),
            ops,
            channels,
            buffer_enabled: false,
            buffer_length: 0,
        },
    );
    Ok(id)
}

/// Read a raw value from a channel (Linux `iio_read_channel_raw`).
pub fn read_raw(device_id: u32, chan_type: IioChanType, channel: u32) -> Result<i64, &'static str> {
    let ops = {
        let devices = IIO_DEVICES.read();
        let dev = devices.get(&device_id).ok_or("IIO device not found")?;
        dev.ops
    };
    (ops.read_raw)(chan_type, channel)
}

/// Read a processed value (raw * scale + offset) (Linux `iio_read_channel_processed`).
pub fn read_processed(
    device_id: u32,
    chan_type: IioChanType,
    channel: u32,
) -> Result<f64, &'static str> {
    let (ops, scale, offset) = {
        let devices = IIO_DEVICES.read();
        let dev = devices.get(&device_id).ok_or("IIO device not found")?;
        let ch = dev
            .channels
            .iter()
            .find(|c| c.chan_type == chan_type && c.channel == channel)
            .ok_or("IIO channel not found")?;
        (dev.ops, ch.scale, ch.offset)
    };
    let raw = (ops.read_raw)(chan_type, channel)? as f64;
    Ok(raw * scale + offset)
}

/// Write a raw value to a channel (Linux `iio_write_channel_raw`).
pub fn write_raw(
    device_id: u32,
    chan_type: IioChanType,
    channel: u32,
    val: i64,
) -> Result<(), &'static str> {
    let ops = {
        let devices = IIO_DEVICES.read();
        let dev = devices.get(&device_id).ok_or("IIO device not found")?;
        dev.ops
    };
    (ops.write_raw)(chan_type, channel, val)
}

/// Get all channels for a device.
pub fn get_channels(device_id: u32) -> Result<Vec<IioChannel>, &'static str> {
    let devices = IIO_DEVICES.read();
    let dev = devices.get(&device_id).ok_or("IIO device not found")?;
    Ok(dev.channels.clone())
}

/// Enable/disable buffer mode (Linux `iio_buffer_enable`).
pub fn set_buffer_enabled(
    device_id: u32,
    enabled: bool,
    length: usize,
) -> Result<(), &'static str> {
    let mut devices = IIO_DEVICES.write();
    let dev = devices.get_mut(&device_id).ok_or("IIO device not found")?;
    dev.buffer_enabled = enabled;
    dev.buffer_length = if enabled { length } else { 0 };
    Ok(())
}

/// Number of registered IIO devices.
pub fn device_count() -> usize {
    IIO_DEVICES.read().len()
}

/// Total number of channels across all devices.
pub fn total_channels() -> usize {
    IIO_DEVICES.read().values().map(|d| d.channels.len()).sum()
}

/// Initialize IIO subsystem with CPU ADC.
pub fn init() -> Result<(), &'static str> {
    if !IIO_DEVICES.read().is_empty() {
        return Ok(());
    }

    register_device("cpu-adc", &CPU_ADC_OPS)?;

    crate::serial_println!("iio: cpu-adc registered ({} channels)", total_channels());
    Ok(())
}
