//! GNSS (Global Navigation Satellite System) subsystem
//!
//! Provides GNSS receiver framework for GPS, GLONASS, Galileo, BeiDou receivers.
//! Mirrors Linux's `drivers/gnss/gnss.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// GNSS receiver type (Linux `enum gnss_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GnssType {
    Unknown,
    Gps,
    Glonass,
    Galileo,
    Beidou,
    Combined,
}

/// GNSS operations (Linux `struct gnss_operations`).
pub struct GnssOps {
    pub open: fn(device_id: u32) -> Result<(), &'static str>,
    pub close: fn(device_id: u32) -> Result<(), &'static str>,
    pub write_raw: fn(device_id: u32, data: &[u8]) -> Result<usize, &'static str>,
    pub read_raw: fn(device_id: u32, buf: &mut [u8]) -> Result<usize, &'static str>,
    pub set_power: fn(device_id: u32, on: bool) -> Result<(), &'static str>,
}

/// GNSS device (Linux `struct gnss_device`).
pub struct GnssDevice {
    pub name: String,
    pub gnss_type: GnssType,
    pub ops: GnssOps,
    pub powered: bool,
    pub active: bool,
    pub read_buf: Vec<u8>,
    pub nmea_sentences: u32,
}

/// NMEA sentence types (subset of NMEA 0183).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NmeaType {
    Gga,
    Rmc,
    Gsa,
    Gsv,
    Vtg,
    Gll,
    Unknown,
}

/// Parsed NMEA sentence.
#[derive(Debug, Clone)]
pub struct NmeaSentence {
    pub sentence_type: NmeaType,
    pub raw: String,
    pub fields: Vec<String>,
}

/// GPS fix data (from GGA sentence).
#[derive(Debug, Clone, Default)]
pub struct GpsFix {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: f64,
    pub satellites: u32,
    pub hdop: f64,
    pub fix_quality: u32,
    pub timestamp: f64,
}

// ── Registry ────────────────────────────────────────────────────────────

static DEVICE_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static GNSS_DEVICES: RwLock<BTreeMap<u32, GnssDevice>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a GNSS receiver device.
pub fn register_device(name: &str, gnss_type: GnssType, ops: GnssOps) -> Result<u32, &'static str> {
    let id = DEVICE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = GnssDevice {
        name: String::from(name),
        gnss_type,
        ops,
        powered: false,
        active: false,
        read_buf: Vec::new(),
        nmea_sentences: 0,
    };
    GNSS_DEVICES.write().insert(id, dev);
    Ok(id)
}

/// Unregister a GNSS device.
pub fn unregister_device(device_id: u32) -> Result<(), &'static str> {
    if GNSS_DEVICES.write().remove(&device_id).is_none() {
        return Err("GNSS device not found");
    }
    Ok(())
}

/// Open a GNSS device for reading.
pub fn open_device(device_id: u32) -> Result<(), &'static str> {
    let open_fn = {
        let devices = GNSS_DEVICES.read();
        let dev = devices.get(&device_id).ok_or("GNSS device not found")?;
        dev.ops.open
    };
    (open_fn)(device_id)?;

    let mut devices = GNSS_DEVICES.write();
    if let Some(dev) = devices.get_mut(&device_id) {
        dev.active = true;
    }
    Ok(())
}

/// Close a GNSS device.
pub fn close_device(device_id: u32) -> Result<(), &'static str> {
    let close_fn = {
        let devices = GNSS_DEVICES.read();
        let dev = devices.get(&device_id).ok_or("GNSS device not found")?;
        dev.ops.close
    };
    (close_fn)(device_id)?;

    let mut devices = GNSS_DEVICES.write();
    if let Some(dev) = devices.get_mut(&device_id) {
        dev.active = false;
    }
    Ok(())
}

/// Write raw data to a GNSS device (e.g., configuration commands).
pub fn write_raw(device_id: u32, data: &[u8]) -> Result<usize, &'static str> {
    let write_fn = {
        let devices = GNSS_DEVICES.read();
        let dev = devices.get(&device_id).ok_or("GNSS device not found")?;
        if !dev.active {
            return Err("GNSS device not open");
        }
        dev.ops.write_raw
    };
    (write_fn)(device_id, data)
}

/// Read raw data from a GNSS device (NMEA sentences).
pub fn read_raw(device_id: u32, buf: &mut [u8]) -> Result<usize, &'static str> {
    let read_fn = {
        let devices = GNSS_DEVICES.read();
        let dev = devices.get(&device_id).ok_or("GNSS device not found")?;
        if !dev.active {
            return Err("GNSS device not open");
        }
        dev.ops.read_raw
    };

    let n = (read_fn)(device_id, buf)?;

    let mut devices = GNSS_DEVICES.write();
    if let Some(dev) = devices.get_mut(&device_id) {
        dev.read_buf.extend_from_slice(&buf[..n]);
        dev.nmea_sentences += 1;
    }
    Ok(n)
}

/// Power on/off a GNSS device.
pub fn set_power(device_id: u32, on: bool) -> Result<(), &'static str> {
    let power_fn = {
        let devices = GNSS_DEVICES.read();
        let dev = devices.get(&device_id).ok_or("GNSS device not found")?;
        dev.ops.set_power
    };
    (power_fn)(device_id, on)?;

    let mut devices = GNSS_DEVICES.write();
    if let Some(dev) = devices.get_mut(&device_id) {
        dev.powered = on;
    }
    Ok(())
}

/// Parse an NMEA sentence from a raw string.
pub fn parse_nmea(raw: &str) -> Result<NmeaSentence, &'static str> {
    let trimmed = raw.trim();
    if !trimmed.starts_with('$') {
        return Err("Invalid NMEA sentence: missing $ prefix");
    }

    let fields: Vec<String> = trimmed.split(',').map(String::from).collect();
    if fields.is_empty() {
        return Err("Empty NMEA sentence");
    }

    let sentence_type = if fields[0].len() >= 6 {
        match &fields[0][3..6] {
            "GGA" => NmeaType::Gga,
            "RMC" => NmeaType::Rmc,
            "GSA" => NmeaType::Gsa,
            "GSV" => NmeaType::Gsv,
            "VTG" => NmeaType::Vtg,
            "GLL" => NmeaType::Gll,
            _ => NmeaType::Unknown,
        }
    } else {
        NmeaType::Unknown
    };

    Ok(NmeaSentence {
        sentence_type,
        raw: String::from(trimmed),
        fields,
    })
}

/// Parse a GGA sentence into a GPS fix.
pub fn parse_gga(sentence: &NmeaSentence) -> Result<GpsFix, &'static str> {
    if sentence.sentence_type != NmeaType::Gga {
        return Err("Not a GGA sentence");
    }
    let f = &sentence.fields;
    if f.len() < 10 {
        return Err("GGA sentence too short");
    }

    let timestamp = f.get(1).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
    let lat_raw = f.get(2).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
    let lat_dir = f.get(3).map(|s| s.as_str()).unwrap_or("N");
    let lon_raw = f.get(4).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
    let lon_dir = f.get(5).map(|s| s.as_str()).unwrap_or("E");
    let fix_quality = f.get(6).and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
    let satellites = f.get(7).and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
    let hdop = f.get(8).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
    let altitude = f.get(9).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);

    let latitude = nmea_to_degrees(lat_raw, lat_dir == "S");
    let longitude = nmea_to_degrees(lon_raw, lon_dir == "W");

    Ok(GpsFix {
        latitude,
        longitude,
        altitude,
        satellites,
        hdop,
        fix_quality,
        timestamp,
    })
}

/// Convert NMEA coordinate (ddmm.mmmm) to decimal degrees.
fn nmea_to_degrees(nmea: f64, negative: bool) -> f64 {
    let degrees = (nmea / 100.0) as i64 as f64;
    let minutes = nmea - degrees * 100.0;
    let mut decimal = degrees + minutes / 60.0;
    if negative {
        decimal = -decimal;
    }
    decimal
}

/// List all registered GNSS devices.
pub fn list_devices() -> Vec<(u32, String, GnssType)> {
    GNSS_DEVICES
        .read()
        .iter()
        .map(|(id, d)| (*id, d.name.clone(), d.gnss_type))
        .collect()
}

/// Count registered devices.
pub fn device_count() -> usize {
    GNSS_DEVICES.read().len()
}

/// Get NMEA sentence count for a device.
pub fn get_nmea_count(device_id: u32) -> Result<u32, &'static str> {
    let devices = GNSS_DEVICES.read();
    let dev = devices.get(&device_id).ok_or("GNSS device not found")?;
    Ok(dev.nmea_sentences)
}

// ── Software GNSS receiver ──────────────────────────────────────────────

fn sw_open(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_close(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_write_raw(_dev_id: u32, data: &[u8]) -> Result<usize, &'static str> {
    Ok(data.len())
}
fn sw_read_raw(_dev_id: u32, buf: &mut [u8]) -> Result<usize, &'static str> {
    let sample = b"$GPGGA,123519,4807.038,N,01131.000,E,1,08,0.9,545.4,M,46.9,M,,*47\r\n";
    let n = core::cmp::min(buf.len(), sample.len());
    buf[..n].copy_from_slice(&sample[..n]);
    Ok(n)
}
fn sw_set_power(_dev_id: u32, _on: bool) -> Result<(), &'static str> {
    Ok(())
}

/// Software GNSS ops (emits a fixed GGA sentence).
pub fn software_gnss_ops() -> GnssOps {
    GnssOps {
        open: sw_open,
        close: sw_close,
        write_raw: sw_write_raw,
        read_raw: sw_read_raw,
        set_power: sw_set_power,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    if !GNSS_DEVICES.read().is_empty() {
        return Ok(());
    }

    let ops = software_gnss_ops();
    register_device("software-gnss", GnssType::Combined, ops)?;
    crate::serial_println!("gnss: subsystem ready");
    Ok(())
}
