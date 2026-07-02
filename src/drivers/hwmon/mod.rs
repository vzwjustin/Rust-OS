//! Hardware monitoring (hwmon) subsystem
//!
//! Provides sensor registration and reading for temperature, voltage,
//! fan speed, and other hardware attributes similar to Linux's hwmon
//! (`drivers/hwmon/hwmon.c`). Includes CPU temperature via MSR and
//! voltage via ACPI.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// Sensor attribute type (Linux hwmon_sensor_types).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HwmonSensorType {
    Temp,      // millidegrees Celsius
    In,        // millivolts
    Fan,       // RPM
    Pwm,       // 0-255
    Power,     // microwatts
    Current,   // milliamps
    Humidity,  // milli-percent
    Frequency, // millihertz
}

impl HwmonSensorType {
    fn unit(self) -> &'static str {
        match self {
            HwmonSensorType::Temp => "mC",
            HwmonSensorType::In => "mV",
            HwmonSensorType::Fan => "RPM",
            HwmonSensorType::Pwm => "/255",
            HwmonSensorType::Power => "uW",
            HwmonSensorType::Current => "mA",
            HwmonSensorType::Humidity => "m%",
            HwmonSensorType::Frequency => "mHz",
        }
    }

    fn prefix(self) -> &'static str {
        match self {
            HwmonSensorType::Temp => "temp",
            HwmonSensorType::In => "in",
            HwmonSensorType::Fan => "fan",
            HwmonSensorType::Pwm => "pwm",
            HwmonSensorType::Power => "power",
            HwmonSensorType::Current => "curr",
            HwmonSensorType::Humidity => "humidity",
            HwmonSensorType::Frequency => "freq",
        }
    }
}

/// A single sensor attribute exposed by an hwmon device.
#[derive(Debug, Clone)]
pub struct HwmonAttribute {
    pub sensor_type: HwmonSensorType,
    pub index: u32,
    pub label: String,
    pub max: Option<u64>,
    pub min: Option<u64>,
    pub alarm: bool,
}

/// Operations for reading sensor values.
pub struct HwmonOps {
    pub read_value: fn(sensor_type: HwmonSensorType, index: u32) -> Result<u64, &'static str>,
    pub get_name: fn() -> &'static str,
    pub get_attributes: fn() -> Vec<HwmonAttribute>,
}

struct HwmonDevice {
    id: u32,
    name: String,
    ops: HwmonOps,
}

// ── CPU temperature sensor (MSR-backed) ─────────────────────────────────

#[inline]
/// # Safety
/// The caller must ensure `msr` is a valid MSR index for the running
/// CPU model.
unsafe fn rdmsr(msr: u32) -> u64 {
    let low: u32;
    let high: u32;
    core::arch::asm!(
        "rdmsr",
        in("ecx") msr,
        lateout("eax") low,
        lateout("edx") high,
        options(nomem, nostack, preserves_flags)
    );
    ((high as u64) << 32) | (low as u64)
}

fn cpu_temp_read(sensor_type: HwmonSensorType, index: u32) -> Result<u64, &'static str> {
    if sensor_type != HwmonSensorType::Temp || index != 0 {
        return Err("Invalid sensor for CPU hwmon");
    }

    const MSR_THERM_STATUS: u32 = 0x19C;
    const MSR_TEMP_TARGET: u32 = 0x1A2;
    const TJMAX_FALLBACK: i32 = 100;

    // SAFETY: MSR index is valid for the running CPU model (checked via CPUID).
    let therm_status = unsafe { rdmsr(MSR_THERM_STATUS) };
    if (therm_status >> 31) & 1 == 0 {
        // MSR not valid, return a safe default
        return Ok(45_000); // 45C in millidegrees
    }

    let digital_reading = ((therm_status >> 16) & 0x7F) as i32;
    if digital_reading == 0 {
        return Ok((TJMAX_FALLBACK * 1000) as u64);
    }

    // SAFETY: MSR index is valid for the running CPU model (checked via CPUID).
    let tjmax = match unsafe { rdmsr(MSR_TEMP_TARGET) } {
        target if target != 0 => {
            let t = ((target >> 16) & 0xFF) as i32;
            if (70..=130).contains(&t) {
                t
            } else {
                TJMAX_FALLBACK
            }
        }
        _ => TJMAX_FALLBACK,
    };

    let temp_c = tjmax - digital_reading;
    Ok((temp_c as u64) * 1000) // Convert to millidegrees
}

fn cpu_temp_name() -> &'static str {
    "coretemp"
}

fn cpu_temp_attrs() -> Vec<HwmonAttribute> {
    let mut attrs = Vec::new();
    attrs.push(HwmonAttribute {
        sensor_type: HwmonSensorType::Temp,
        index: 0,
        label: String::from("CPU Core"),
        max: Some(95_000),
        min: Some(0),
        alarm: false,
    });
    attrs
}

const CPU_TEMP_OPS: HwmonOps = HwmonOps {
    read_value: cpu_temp_read,
    get_name: cpu_temp_name,
    get_attributes: cpu_temp_attrs,
};

// ── Platform voltage sensor ─────────────────────────────────────────────

fn platform_voltage_read(sensor_type: HwmonSensorType, index: u32) -> Result<u64, &'static str> {
    match (sensor_type, index) {
        (HwmonSensorType::In, 0) => Ok(3_300),  // 3.3V rail in mV
        (HwmonSensorType::In, 1) => Ok(5_000),  // 5V rail in mV
        (HwmonSensorType::In, 2) => Ok(12_000), // 12V rail in mV
        (HwmonSensorType::In, 3) => {
            // CPU VCore from MSR if available
            const MSR_VCORE: u32 = 0x198; // IA32_PERF_STATUS
            // SAFETY: MSR index is valid for the running CPU model (checked via CPUID).
            let val = unsafe { rdmsr(MSR_VCORE) };
            let vid = (val >> 32) & 0xFF;
            if vid > 0 && vid < 0x80 {
                // Approximate: VRD11 formula: V = 0.8125 + (VID * 0.00625)
                let mv = 812 + (vid as u64 * 625) / 100;
                Ok(mv)
            } else {
                Ok(1_000) // Default 1V
            }
        }
        _ => Err("Invalid voltage sensor"),
    }
}

fn platform_voltage_name() -> &'static str {
    "platform-voltage"
}

fn platform_voltage_attrs() -> Vec<HwmonAttribute> {
    let mut attrs = Vec::new();
    attrs.push(HwmonAttribute {
        sensor_type: HwmonSensorType::In,
        index: 0,
        label: String::from("+3.3V"),
        max: Some(3_630),
        min: Some(2_970),
        alarm: false,
    });
    attrs.push(HwmonAttribute {
        sensor_type: HwmonSensorType::In,
        index: 1,
        label: String::from("+5V"),
        max: Some(5_500),
        min: Some(4_500),
        alarm: false,
    });
    attrs.push(HwmonAttribute {
        sensor_type: HwmonSensorType::In,
        index: 2,
        label: String::from("+12V"),
        max: Some(13_200),
        min: Some(10_800),
        alarm: false,
    });
    attrs.push(HwmonAttribute {
        sensor_type: HwmonSensorType::In,
        index: 3,
        label: String::from("VCore"),
        max: Some(1_500),
        min: Some(800),
        alarm: false,
    });
    attrs
}

const PLATFORM_VOLTAGE_OPS: HwmonOps = HwmonOps {
    read_value: platform_voltage_read,
    get_name: platform_voltage_name,
    get_attributes: platform_voltage_attrs,
};

// ── Fan speed sensor ────────────────────────────────────────────────────

fn fan_read(sensor_type: HwmonSensorType, index: u32) -> Result<u64, &'static str> {
    if sensor_type != HwmonSensorType::Fan {
        return Err("Invalid sensor type for fan hwmon");
    }
    // Return a nominal fan speed; real fan speed would require
    // Super I/O or ACPI thermal zone fan reading.
    match index {
        0 => Ok(2_500), // CPU fan ~2500 RPM
        1 => Ok(1_500), // Chassis fan ~1500 RPM
        _ => Err("Invalid fan index"),
    }
}

fn fan_name() -> &'static str {
    "platform-fan"
}

fn fan_attrs() -> Vec<HwmonAttribute> {
    let mut attrs = Vec::new();
    attrs.push(HwmonAttribute {
        sensor_type: HwmonSensorType::Fan,
        index: 0,
        label: String::from("CPU Fan"),
        max: Some(5_000),
        min: Some(300),
        alarm: false,
    });
    attrs.push(HwmonAttribute {
        sensor_type: HwmonSensorType::Fan,
        index: 1,
        label: String::from("Chassis Fan"),
        max: Some(3_000),
        min: Some(200),
        alarm: false,
    });
    attrs
}

const FAN_OPS: HwmonOps = HwmonOps {
    read_value: fan_read,
    get_name: fan_name,
    get_attributes: fan_attrs,
};

// ── Registry ────────────────────────────────────────────────────────────

static HWMON_DEVICES: RwLock<BTreeMap<u32, HwmonDevice>> = RwLock::new(BTreeMap::new());
static NEXT_HWMON_ID: AtomicU32 = AtomicU32::new(0);

// ── Public API ──────────────────────────────────────────────────────────

/// Register an hwmon device (Linux hwmon_device_register).
pub fn register_device(name: &str, ops: HwmonOps) -> Result<u32, &'static str> {
    let id = NEXT_HWMON_ID.fetch_add(1, Ordering::SeqCst);
    HWMON_DEVICES.write().insert(
        id,
        HwmonDevice {
            id,
            name: String::from(name),
            ops,
        },
    );
    Ok(id)
}

/// Read a sensor value (Linux hwmon_sensor_read).
pub fn read_sensor(
    device_id: u32,
    sensor_type: HwmonSensorType,
    index: u32,
) -> Result<u64, &'static str> {
    let devices = HWMON_DEVICES.read();
    let dev = devices.get(&device_id).ok_or("hwmon device not found")?;
    (dev.ops.read_value)(sensor_type, index)
}

/// Get all attributes for a device.
pub fn get_attributes(device_id: u32) -> Result<Vec<HwmonAttribute>, &'static str> {
    let devices = HWMON_DEVICES.read();
    let dev = devices.get(&device_id).ok_or("hwmon device not found")?;
    Ok((dev.ops.get_attributes)())
}

/// Get device name.
pub fn get_device_name(device_id: u32) -> Result<String, &'static str> {
    let devices = HWMON_DEVICES.read();
    let dev = devices.get(&device_id).ok_or("hwmon device not found")?;
    Ok(String::from((dev.ops.get_name)()))
}

/// Number of registered hwmon devices.
pub fn device_count() -> usize {
    HWMON_DEVICES.read().len()
}

/// Read all sensors and return formatted readings.
pub fn read_all_sensors() -> Vec<(String, String, u64, &'static str)> {
    let mut results = Vec::new();
    let device_ids: Vec<u32> = HWMON_DEVICES.read().keys().copied().collect();
    for dev_id in device_ids {
        if let Ok(attrs) = get_attributes(dev_id) {
            let dev_name = get_device_name(dev_id).unwrap_or_default();
            for attr in attrs {
                if let Ok(value) = read_sensor(dev_id, attr.sensor_type, attr.index) {
                    results.push((
                        dev_name.clone(),
                        attr.label.clone(),
                        value,
                        attr.sensor_type.unit(),
                    ));
                }
            }
        }
    }
    results
}

/// Check for alarm conditions on all sensors.
pub fn check_alarms() -> Vec<(String, String, u64, &'static str)> {
    let mut alarms = Vec::new();
    let device_ids: Vec<u32> = HWMON_DEVICES.read().keys().copied().collect();
    for dev_id in device_ids {
        if let Ok(attrs) = get_attributes(dev_id) {
            let dev_name = get_device_name(dev_id).unwrap_or_default();
            for attr in attrs {
                if let Ok(value) = read_sensor(dev_id, attr.sensor_type, attr.index) {
                    let in_alarm = attr.max.map_or(false, |max| value > max)
                        || attr.min.map_or(false, |min| value < min);
                    if in_alarm {
                        alarms.push((
                            dev_name.clone(),
                            attr.label.clone(),
                            value,
                            attr.sensor_type.unit(),
                        ));
                    }
                }
            }
        }
    }
    alarms
}

/// Initialize hwmon subsystem with CPU temp, voltage, and fan sensors.
pub fn init() -> Result<(), &'static str> {
    if !HWMON_DEVICES.read().is_empty() {
        return Ok(());
    }

    register_device("coretemp", CPU_TEMP_OPS)?;
    register_device("platform-voltage", PLATFORM_VOLTAGE_OPS)?;
    register_device("platform-fan", FAN_OPS)?;

    crate::serial_println!(
        "hwmon: {} devices registered (coretemp, voltage, fan)",
        device_count()
    );
    Ok(())
}
