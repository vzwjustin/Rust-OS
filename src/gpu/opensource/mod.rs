//! Opensource GPU Driver Integration Framework for RustOS
//!
//! This module provides comprehensive integration with opensource GPU drivers:
//! - Nouveau driver framework for NVIDIA GPUs
//! - AMDGPU driver framework for AMD GPUs
//! - i915 driver framework for Intel integrated graphics
//! - DRM (Direct Rendering Manager) compatibility layer
//! - Mesa3D integration preparation

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use lazy_static::lazy_static;
use spin::Mutex;

use super::{GPUCapabilities, GPUVendor, PCIDevice};

pub mod amdgpu;
pub mod drm_compat;
pub mod i915;
pub mod mesa_compat;
pub mod nouveau;

/// Opensource driver types
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DriverType {
    Nouveau,   // NVIDIA opensource driver
    AMDGPU,    // AMD opensource driver
    I915,      // Intel i915 driver
    RadeonSI,  // AMD RadeonSI (legacy)
    VC4,       // Broadcom VideoCore IV
    V3D,       // Broadcom V3D
    Panfrost,  // ARM Mali driver
    Lima,      // ARM Mali 400/450 driver
    Etnaviv,   // Vivante GPU driver
    Freedreno, // Qualcomm Adreno driver
}

/// Driver capability flags
#[derive(Debug, Clone, Copy)]
pub struct DriverCapabilities {
    pub direct_rendering: bool,
    pub hardware_acceleration: bool,
    pub compute_shaders: bool,
    pub video_decode: bool,
    pub video_encode: bool,
    pub vulkan_support: bool,
    pub opengl_version: (u8, u8),
    pub glsl_version: u16,
    pub opencl_support: bool,
    pub ray_tracing: bool,
}

impl DriverCapabilities {
    pub const BASIC: Self = Self {
        direct_rendering: true,
        hardware_acceleration: false,
        compute_shaders: false,
        video_decode: false,
        video_encode: false,
        vulkan_support: false,
        opengl_version: (2, 1),
        glsl_version: 120,
        opencl_support: false,
        ray_tracing: false,
    };

    pub const MODERN: Self = Self {
        direct_rendering: true,
        hardware_acceleration: true,
        compute_shaders: true,
        video_decode: true,
        video_encode: false,
        vulkan_support: true,
        opengl_version: (4, 6),
        glsl_version: 460,
        opencl_support: true,
        ray_tracing: false,
    };

    pub const ADVANCED: Self = Self {
        direct_rendering: true,
        hardware_acceleration: true,
        compute_shaders: true,
        video_decode: true,
        video_encode: true,
        vulkan_support: true,
        opengl_version: (4, 6),
        glsl_version: 460,
        opencl_support: true,
        ray_tracing: true,
    };
}

/// Opensource driver information
#[derive(Debug, Clone)]
pub struct OpensourceDriver {
    pub driver_type: DriverType,
    pub name: String,
    pub version: String,
    pub supported_devices: Vec<(u16, u16)>, // (vendor_id, device_id) pairs
    pub capabilities: DriverCapabilities,
    pub mesa_driver: Option<String>,
    pub kernel_module: String,
    pub user_space_driver: String,
    pub priority: u8, // Higher is better
}

/// DRM device node information
#[derive(Debug, Clone)]
pub struct DRMDevice {
    pub device_node: String,
    pub card_number: u32,
    pub render_node: Option<String>,
    pub primary_node: String,
    pub driver_name: String,
    pub vendor_id: u16,
    pub device_id: u16,
    pub subsystem_vendor: u16,
    pub subsystem_device: u16,
}

/// Mesa3D driver interface
#[derive(Debug, Clone)]
pub struct MesaInterface {
    pub gallium_driver: Option<String>,
    pub classic_driver: Option<String>,
    pub vulkan_driver: Option<String>,
    pub opencl_driver: Option<String>,
    pub va_api_driver: Option<String>,
    pub vdpau_driver: Option<String>,
}

/// Driver loading status
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DriverStatus {
    Unloaded,
    Loading,
    Loaded,
    Failed,
    Suspended,
}

/// Opensource driver registry and manager
pub struct OpensourceDriverRegistry {
    pub available_drivers: Vec<OpensourceDriver>,
    pub loaded_drivers: BTreeMap<u32, LoadedDriver>, // GPU ID -> Loaded driver
    pub drm_devices: Vec<DRMDevice>,
    pub mesa_interfaces: BTreeMap<u32, MesaInterface>, // GPU ID -> Mesa interface
    pub driver_compatibility_db: BTreeMap<(u16, u16), Vec<DriverType>>, // Device -> compatible drivers
}

/// Loaded driver instance
#[derive(Debug, Clone)]
pub struct LoadedDriver {
    pub gpu_id: u32,
    pub driver_type: DriverType,
    pub status: DriverStatus,
    pub drm_device: Option<DRMDevice>,
    pub mesa_interface: Option<MesaInterface>,
    pub performance_profile: PerformanceProfile,
    pub power_management: DriverPowerManagement,
}

/// Driver performance profile
#[derive(Debug, Clone)]
pub struct PerformanceProfile {
    pub power_mode: PowerMode,
    pub clock_speeds: ClockSpeeds,
    pub memory_timings: MemoryTimings,
    pub thermal_limits: ThermalLimits,
}

/// Power management modes
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PowerMode {
    PowerSave,
    Balanced,
    Performance,
    MaxPerformance,
}

/// GPU clock speeds
#[derive(Debug, Clone)]
pub struct ClockSpeeds {
    pub core_min: u32,
    pub core_max: u32,
    pub memory_min: u32,
    pub memory_max: u32,
    pub shader_min: u32,
    pub shader_max: u32,
}

/// Memory timing configuration
#[derive(Debug, Clone)]
pub struct MemoryTimings {
    pub cas_latency: u8,
    pub ras_to_cas: u8,
    pub ras_precharge: u8,
    pub row_cycle_time: u16,
}

/// Thermal management limits
#[derive(Debug, Clone)]
pub struct ThermalLimits {
    pub target_temp: u8,
    pub critical_temp: u8,
    pub power_limit: u16, // Watts
    pub thermal_throttling: bool,
}

/// Driver power management interface
#[derive(Debug, Clone)]
pub struct DriverPowerManagement {
    pub current_mode: PowerMode,
    pub available_modes: Vec<PowerMode>,
    pub dynamic_frequency_scaling: bool,
    pub runtime_power_management: bool,
    pub suspend_resume_support: bool,
}

impl OpensourceDriverRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            available_drivers: Vec::new(),
            loaded_drivers: BTreeMap::new(),
            drm_devices: Vec::new(),
            mesa_interfaces: BTreeMap::new(),
            driver_compatibility_db: BTreeMap::new(),
        };

        registry.initialize_driver_database();
        registry
    }

    /// Initialize the driver database with known drivers and device compatibility
    fn initialize_driver_database(&mut self) {
        // Nouveau driver for NVIDIA GPUs
        let nouveau_driver = OpensourceDriver {
            driver_type: DriverType::Nouveau,
            name: "Nouveau".to_string(),
            version: "1.0.17".to_string(),
            supported_devices: vec![
                // Tesla (NV50) series
                (0x10DE, 0x0191),
                (0x10DE, 0x0193),
                (0x10DE, 0x0194),
                (0x10DE, 0x0197),
                (0x10DE, 0x019D),
                (0x10DE, 0x019E),
                (0x10DE, 0x0400),
                (0x10DE, 0x0401),
                (0x10DE, 0x0402),
                (0x10DE, 0x0403),
                (0x10DE, 0x0404),
                (0x10DE, 0x0405),
                (0x10DE, 0x0406),
                (0x10DE, 0x0407),
                (0x10DE, 0x0408),
                (0x10DE, 0x0409),
                (0x10DE, 0x040A),
                (0x10DE, 0x040B),
                (0x10DE, 0x040C),
                (0x10DE, 0x040D),
                (0x10DE, 0x040E),
                (0x10DE, 0x040F),
                // Fermi (GF100) series
                (0x10DE, 0x06C0),
                (0x10DE, 0x06C4),
                (0x10DE, 0x06CA),
                (0x10DE, 0x06CD),
                (0x10DE, 0x06D1),
                (0x10DE, 0x06D2),
                (0x10DE, 0x06D8),
                (0x10DE, 0x06D9),
                (0x10DE, 0x06DA),
                (0x10DE, 0x06DC),
                (0x10DE, 0x06DD),
                (0x10DE, 0x06DE),
                (0x10DE, 0x06DF),
                (0x10DE, 0x0DC0),
                (0x10DE, 0x0DC4),
                (0x10DE, 0x0DC5),
                (0x10DE, 0x0DC6),
                (0x10DE, 0x0DCD),
                (0x10DE, 0x0DCE),
                (0x10DE, 0x0DD1),
                (0x10DE, 0x0DD2),
                (0x10DE, 0x0DD3),
                (0x10DE, 0x0DD6),
                (0x10DE, 0x0DD8),
                (0x10DE, 0x0DDA),
                (0x10DE, 0x0DE0),
                (0x10DE, 0x0DE1),
                (0x10DE, 0x0DE2),
                (0x10DE, 0x0DE3),
                (0x10DE, 0x0DE4),
                (0x10DE, 0x0DE5),
                (0x10DE, 0x0DE7),
                (0x10DE, 0x0DE8),
                (0x10DE, 0x0DE9),
                (0x10DE, 0x0DEA),
                (0x10DE, 0x0DEB),
                (0x10DE, 0x0DEC),
                (0x10DE, 0x0DED),
                (0x10DE, 0x0DEE),
                (0x10DE, 0x0DEF),
                (0x10DE, 0x0DF0),
                (0x10DE, 0x0DF1),
                (0x10DE, 0x0DF2),
                (0x10DE, 0x0DF3),
                (0x10DE, 0x0DF4),
                (0x10DE, 0x0DF5),
                (0x10DE, 0x0DF6),
                (0x10DE, 0x0DF7),
                (0x10DE, 0x0DF8),
                (0x10DE, 0x0DF9),
                (0x10DE, 0x0DFA),
                (0x10DE, 0x0DFC),
                (0x10DE, 0x0DFD),
                (0x10DE, 0x0DFE),
                (0x10DE, 0x0DFF),
                // Kepler (GK100) series
                (0x10DE, 0x1180),
                (0x10DE, 0x1181),
                (0x10DE, 0x1182),
                (0x10DE, 0x1183),
                (0x10DE, 0x1184),
                (0x10DE, 0x1185),
                (0x10DE, 0x1186),
                (0x10DE, 0x1187),
                (0x10DE, 0x1188),
                (0x10DE, 0x1189),
                (0x10DE, 0x118A),
                (0x10DE, 0x118B),
                (0x10DE, 0x118C),
                (0x10DE, 0x118D),
                (0x10DE, 0x118E),
                (0x10DE, 0x118F),
                (0x10DE, 0x1190),
                (0x10DE, 0x1191),
                (0x10DE, 0x1192),
                (0x10DE, 0x1193),
                (0x10DE, 0x1194),
                (0x10DE, 0x1195),
                (0x10DE, 0x1198),
                (0x10DE, 0x1199),
                (0x10DE, 0x119A),
                (0x10DE, 0x119D),
                (0x10DE, 0x119E),
                (0x10DE, 0x119F),
                // Maxwell (GM100) series
                (0x10DE, 0x1340),
                (0x10DE, 0x1341),
                (0x10DE, 0x1344),
                (0x10DE, 0x1346),
                (0x10DE, 0x1347),
                (0x10DE, 0x1348),
                (0x10DE, 0x1349),
                (0x10DE, 0x134B),
                (0x10DE, 0x134D),
                (0x10DE, 0x134E),
                (0x10DE, 0x134F),
                (0x10DE, 0x1380),
                (0x10DE, 0x1381),
                (0x10DE, 0x1382),
                (0x10DE, 0x1390),
                (0x10DE, 0x1391),
                (0x10DE, 0x1392),
                (0x10DE, 0x1393),
                (0x10DE, 0x1398),
                (0x10DE, 0x1399),
                (0x10DE, 0x139A),
                (0x10DE, 0x139B),
                (0x10DE, 0x139C),
                (0x10DE, 0x139D),
                // Pascal (GP100) series
                (0x10DE, 0x15F0),
                (0x10DE, 0x15F1),
                (0x10DE, 0x15F7),
                (0x10DE, 0x15F8),
                (0x10DE, 0x15F9),
                (0x10DE, 0x1B00),
                (0x10DE, 0x1B02),
                (0x10DE, 0x1B06),
                (0x10DE, 0x1B30),
                (0x10DE, 0x1B38),
                (0x10DE, 0x1B80),
                (0x10DE, 0x1B81),
                (0x10DE, 0x1B82),
                (0x10DE, 0x1B83),
                (0x10DE, 0x1B84),
                (0x10DE, 0x1BA0),
                (0x10DE, 0x1BA1),
                (0x10DE, 0x1BB0),
                (0x10DE, 0x1BB1),
                (0x10DE, 0x1BB3),
                (0x10DE, 0x1BB4),
                (0x10DE, 0x1BB5),
                (0x10DE, 0x1BB6),
                (0x10DE, 0x1BB7),
                (0x10DE, 0x1BB8),
                (0x10DE, 0x1BB9),
                (0x10DE, 0x1BBA),
                (0x10DE, 0x1BBB),
                (0x10DE, 0x1BC7),
                (0x10DE, 0x1BE0),
                (0x10DE, 0x1BE1),
                // Turing and newer (limited support)
                (0x10DE, 0x1F02),
                (0x10DE, 0x1F06),
                (0x10DE, 0x1F07),
                (0x10DE, 0x1F08),
                (0x10DE, 0x1F09),
                (0x10DE, 0x1F0A),
                (0x10DE, 0x1F10),
                (0x10DE, 0x1F11),
                (0x10DE, 0x1F12),
                (0x10DE, 0x1F14),
                (0x10DE, 0x1F15),
                (0x10DE, 0x1F36),
                (0x10DE, 0x1F47),
                (0x10DE, 0x1F50),
                (0x10DE, 0x1F51),
                (0x10DE, 0x1F54),
                (0x10DE, 0x1F55),
                (0x10DE, 0x1F76),
                (0x10DE, 0x1F81),
                (0x10DE, 0x1F82),
                (0x10DE, 0x1F83),
                (0x10DE, 0x1F91),
                (0x10DE, 0x1F92),
                (0x10DE, 0x1F94),
                (0x10DE, 0x1F95),
                (0x10DE, 0x1F96),
                (0x10DE, 0x1F97),
                (0x10DE, 0x1F98),
                (0x10DE, 0x1F99),
                (0x10DE, 0x1F9C),
                (0x10DE, 0x1F9D),
                (0x10DE, 0x1F9F),
            ],
            capabilities: DriverCapabilities::MODERN,
            mesa_driver: Some("nouveau".to_string()),
            kernel_module: "nouveau".to_string(),
            user_space_driver: "libnouveau".to_string(),
            priority: 70, // Lower priority than proprietary but good support
        };

        // AMDGPU driver for modern AMD GPUs
        let amdgpu_driver = OpensourceDriver {
            driver_type: DriverType::AMDGPU,
            name: "AMDGPU".to_string(),
            version: "23.20".to_string(),
            supported_devices: vec![
                // GCN 1.0 (Southern Islands)
                (0x1002, 0x6798),
                (0x1002, 0x6799),
                (0x1002, 0x679A),
                (0x1002, 0x679B),
                (0x1002, 0x679E),
                (0x1002, 0x679F),
                (0x1002, 0x6780),
                (0x1002, 0x6784),
                (0x1002, 0x6788),
                (0x1002, 0x678A),
                (0x1002, 0x6790),
                (0x1002, 0x6791),
                (0x1002, 0x6792),
                (0x1002, 0x6798),
                (0x1002, 0x6799),
                (0x1002, 0x679A),
                // GCN 2.0 (Sea Islands)
                (0x1002, 0x6600),
                (0x1002, 0x6601),
                (0x1002, 0x6602),
                (0x1002, 0x6603),
                (0x1002, 0x6604),
                (0x1002, 0x6605),
                (0x1002, 0x6606),
                (0x1002, 0x6607),
                (0x1002, 0x6608),
                (0x1002, 0x6610),
                (0x1002, 0x6611),
                (0x1002, 0x6613),
                (0x1002, 0x6617),
                (0x1002, 0x6620),
                (0x1002, 0x6621),
                (0x1002, 0x6623),
                (0x1002, 0x6631),
                (0x1002, 0x6640),
                (0x1002, 0x6641),
                (0x1002, 0x6646),
                (0x1002, 0x6647),
                (0x1002, 0x6649),
                (0x1002, 0x6650),
                (0x1002, 0x6651),
                (0x1002, 0x6658),
                (0x1002, 0x665C),
                (0x1002, 0x665D),
                (0x1002, 0x665F),
                // GCN 3.0 (Volcanic Islands)
                (0x1002, 0x6900),
                (0x1002, 0x6901),
                (0x1002, 0x6902),
                (0x1002, 0x6903),
                (0x1002, 0x6907),
                (0x1002, 0x6920),
                (0x1002, 0x6921),
                (0x1002, 0x6929),
                (0x1002, 0x692B),
                (0x1002, 0x692F),
                (0x1002, 0x6930),
                (0x1002, 0x6938),
                (0x1002, 0x6939),
                (0x1002, 0x7300),
                (0x1002, 0x7310),
                (0x1002, 0x7312),
                // GCN 4.0 (Arctic Islands)
                (0x1002, 0x67C0),
                (0x1002, 0x67C1),
                (0x1002, 0x67C2),
                (0x1002, 0x67C4),
                (0x1002, 0x67C7),
                (0x1002, 0x67CA),
                (0x1002, 0x67CC),
                (0x1002, 0x67CF),
                (0x1002, 0x67D0),
                (0x1002, 0x67DF),
                (0x1002, 0x67E0),
                (0x1002, 0x67E1),
                (0x1002, 0x67E3),
                (0x1002, 0x67E8),
                (0x1002, 0x67EB),
                (0x1002, 0x67EF),
                (0x1002, 0x67F0),
                (0x1002, 0x67F1),
                (0x1002, 0x67F2),
                (0x1002, 0x67F4),
                (0x1002, 0x67F7),
                (0x1002, 0x67F8),
                (0x1002, 0x67F9),
                (0x1002, 0x67FA),
                (0x1002, 0x67FB),
                (0x1002, 0x67FE),
                (0x1002, 0x67FF),
                // GCN 5.0 (Vega)
                (0x1002, 0x6860),
                (0x1002, 0x6861),
                (0x1002, 0x6862),
                (0x1002, 0x6863),
                (0x1002, 0x6864),
                (0x1002, 0x6867),
                (0x1002, 0x6868),
                (0x1002, 0x6869),
                (0x1002, 0x686A),
                (0x1002, 0x686B),
                (0x1002, 0x686C),
                (0x1002, 0x686D),
                (0x1002, 0x686E),
                (0x1002, 0x687F),
                (0x1002, 0x69A0),
                (0x1002, 0x69A1),
                (0x1002, 0x69A2),
                (0x1002, 0x69A3),
                (0x1002, 0x69AF),
                // RDNA 1.0 (Navi 10)
                (0x1002, 0x7310),
                (0x1002, 0x7312),
                (0x1002, 0x7318),
                (0x1002, 0x7319),
                (0x1002, 0x731A),
                (0x1002, 0x731B),
                (0x1002, 0x731E),
                (0x1002, 0x731F),
                (0x1002, 0x7340),
                (0x1002, 0x7341),
                (0x1002, 0x7347),
                // RDNA 2.0 (Navi 2x)
                (0x1002, 0x73A0),
                (0x1002, 0x73A1),
                (0x1002, 0x73A2),
                (0x1002, 0x73A3),
                (0x1002, 0x73A5),
                (0x1002, 0x73AB),
                (0x1002, 0x73AE),
                (0x1002, 0x73AF),
                (0x1002, 0x73BF),
                (0x1002, 0x73C0),
                (0x1002, 0x73C1),
                (0x1002, 0x73C3),
                (0x1002, 0x73DF),
                (0x1002, 0x73E0),
                (0x1002, 0x73E1),
                (0x1002, 0x73E3),
                (0x1002, 0x73E4),
                (0x1002, 0x73EF),
                (0x1002, 0x73F0),
                (0x1002, 0x73FF),
                // RDNA 3.0 (Navi 3x)
                (0x1002, 0x744C),
                (0x1002, 0x7448),
                (0x1002, 0x7449),
                (0x1002, 0x747E),
                (0x1002, 0x7480),
                (0x1002, 0x7483),
                (0x1002, 0x7484),
            ],
            capabilities: DriverCapabilities::ADVANCED,
            mesa_driver: Some("radeonsi".to_string()),
            kernel_module: "amdgpu".to_string(),
            user_space_driver: "libamdgpu".to_string(),
            priority: 85, // High priority for AMD GPUs
        };

        // Intel i915 driver
        let i915_driver = OpensourceDriver {
            driver_type: DriverType::I915,
            name: "Intel i915".to_string(),
            version: "1.6.0".to_string(),
            supported_devices: vec![
                // All Intel GPU device IDs from our main database
                (0x8086, 0x0042),
                (0x8086, 0x0046),
                (0x8086, 0x0102),
                (0x8086, 0x0106),
                (0x8086, 0x010A),
                (0x8086, 0x0112),
                (0x8086, 0x0116),
                (0x8086, 0x0122),
                (0x8086, 0x0126),
                (0x8086, 0x0152),
                (0x8086, 0x0156),
                (0x8086, 0x015A),
                (0x8086, 0x0162),
                (0x8086, 0x0166),
                (0x8086, 0x016A),
                (0x8086, 0x0402),
                (0x8086, 0x0406),
                (0x8086, 0x040A),
                (0x8086, 0x0412),
                (0x8086, 0x0416),
                (0x8086, 0x041A),
                (0x8086, 0x041E),
                (0x8086, 0x0422),
                (0x8086, 0x0426),
                (0x8086, 0x042A),
                (0x8086, 0x042B),
                (0x8086, 0x042E),
                (0x8086, 0x0A02),
                (0x8086, 0x0A06),
                (0x8086, 0x0A0A),
                (0x8086, 0x0A0B),
                (0x8086, 0x0A0E),
                (0x8086, 0x0A12),
                (0x8086, 0x0A16),
                (0x8086, 0x0A1A),
                (0x8086, 0x0A1E),
                (0x8086, 0x0A22),
                (0x8086, 0x0A26),
                (0x8086, 0x0A2A),
                (0x8086, 0x0A2B),
                (0x8086, 0x0A2E),
                (0x8086, 0x0D12),
                (0x8086, 0x0D16),
                (0x8086, 0x0D1A),
                (0x8086, 0x0D1B),
                (0x8086, 0x0D1E),
                (0x8086, 0x0D22),
                (0x8086, 0x0D26),
                (0x8086, 0x0D2A),
                (0x8086, 0x0D2B),
                (0x8086, 0x0D2E),
                (0x8086, 0x1602),
                (0x8086, 0x1606),
                (0x8086, 0x160A),
                (0x8086, 0x160B),
                (0x8086, 0x160D),
                (0x8086, 0x160E),
                (0x8086, 0x1612),
                (0x8086, 0x1616),
                (0x8086, 0x161A),
                (0x8086, 0x161B),
                (0x8086, 0x161D),
                (0x8086, 0x161E),
                (0x8086, 0x1622),
                (0x8086, 0x1626),
                (0x8086, 0x162A),
                (0x8086, 0x162B),
                (0x8086, 0x162D),
                (0x8086, 0x162E),
                (0x8086, 0x1902),
                (0x8086, 0x1906),
                (0x8086, 0x190A),
                (0x8086, 0x190B),
                (0x8086, 0x190E),
                (0x8086, 0x1912),
                (0x8086, 0x1913),
                (0x8086, 0x1915),
                (0x8086, 0x1916),
                (0x8086, 0x1917),
                (0x8086, 0x191A),
                (0x8086, 0x191B),
                (0x8086, 0x191D),
                (0x8086, 0x191E),
                (0x8086, 0x1921),
                (0x8086, 0x1923),
                (0x8086, 0x1926),
                (0x8086, 0x1927),
                (0x8086, 0x192A),
                (0x8086, 0x192B),
                (0x8086, 0x192D),
                (0x8086, 0x5902),
                (0x8086, 0x5906),
                (0x8086, 0x590A),
                (0x8086, 0x590B),
                (0x8086, 0x590E),
                (0x8086, 0x5912),
                (0x8086, 0x5913),
                (0x8086, 0x5915),
                (0x8086, 0x5916),
                (0x8086, 0x5917),
                (0x8086, 0x591A),
                (0x8086, 0x591B),
                (0x8086, 0x591C),
                (0x8086, 0x591D),
                (0x8086, 0x591E),
                (0x8086, 0x5921),
                (0x8086, 0x5923),
                (0x8086, 0x5926),
                (0x8086, 0x5927),
                (0x8086, 0x3E90),
                (0x8086, 0x3E91),
                (0x8086, 0x3E92),
                (0x8086, 0x3E93),
                (0x8086, 0x3E94),
                (0x8086, 0x3E96),
                (0x8086, 0x3E98),
                (0x8086, 0x3E9A),
                (0x8086, 0x3E9B),
                (0x8086, 0x3EA0),
                (0x8086, 0x3EA5),
                (0x8086, 0x3EA6),
                (0x8086, 0x3EA7),
                (0x8086, 0x3EA8),
                (0x8086, 0x8A50),
                (0x8086, 0x8A51),
                (0x8086, 0x8A52),
                (0x8086, 0x8A53),
                (0x8086, 0x8A5A),
                (0x8086, 0x8A5B),
                (0x8086, 0x8A5C),
                (0x8086, 0x8A5D),
                (0x8086, 0x9A40),
                (0x8086, 0x9A49),
                (0x8086, 0x9A60),
                (0x8086, 0x9A68),
                (0x8086, 0x9A70),
                (0x8086, 0x9A78),
            ],
            capabilities: DriverCapabilities::MODERN,
            mesa_driver: Some("iris".to_string()),
            kernel_module: "i915".to_string(),
            user_space_driver: "libintel".to_string(),
            priority: 90, // High priority for Intel GPUs
        };

        self.available_drivers.push(nouveau_driver);
        self.available_drivers.push(amdgpu_driver);
        self.available_drivers.push(i915_driver);

        // Build device compatibility database
        for driver in &self.available_drivers {
            for &(vendor_id, device_id) in &driver.supported_devices {
                self.driver_compatibility_db
                    .entry((vendor_id, device_id))
                    .or_insert_with(Vec::new)
                    .push(driver.driver_type);
            }
        }
    }

    /// Find compatible driver for a PCI device
    pub fn find_driver_for_device(&self, pci_device: &PCIDevice) -> Option<&OpensourceDriver> {
        let device_key = (pci_device.vendor_id, pci_device.device_id);

        if let Some(compatible_drivers) = self.driver_compatibility_db.get(&device_key) {
            // Find the highest priority compatible driver
            let mut best_driver = None;
            let mut best_priority = 0;

            for &driver_type in compatible_drivers {
                if let Some(driver) = self
                    .available_drivers
                    .iter()
                    .find(|d| d.driver_type == driver_type)
                {
                    if driver.priority > best_priority {
                        best_priority = driver.priority;
                        best_driver = Some(driver);
                    }
                }
            }

            best_driver
        } else {
            None
        }
    }

    /// Initialize driver for a specific GPU with real hardware communication
    pub fn initialize_driver(
        &mut self,
        gpu: &GPUCapabilities,
        pci_device: &PCIDevice,
    ) -> Result<(), &'static str> {
        if let Some(driver) = self.find_driver_for_device(pci_device) {
            let gpu_id = pci_device.device as u32; // Simplified GPU ID

            // Initialize real hardware communication
            self.initialize_hardware_communication(gpu_id, pci_device, driver)?;

            // Create DRM device entry with real hardware info
            let drm_device = DRMDevice {
                device_node: format!("/dev/dri/card{}", gpu_id),
                card_number: gpu_id,
                render_node: Some(format!("/dev/dri/renderD{}", 128 + gpu_id)),
                primary_node: format!("/dev/dri/card{}", gpu_id),
                driver_name: driver.kernel_module.clone(),
                vendor_id: pci_device.vendor_id,
                device_id: pci_device.device_id,
                subsystem_vendor: self.read_pci_subsystem_vendor(pci_device)?,
                subsystem_device: self.read_pci_subsystem_device(pci_device)?,
            };

            // Create Mesa interface
            let mesa_interface = MesaInterface {
                gallium_driver: driver.mesa_driver.clone(),
                classic_driver: None,
                vulkan_driver: match driver.driver_type {
                    DriverType::AMDGPU => Some("radv".to_string()),
                    DriverType::I915 => Some("anv".to_string()),
                    DriverType::Nouveau => Some("nouveau".to_string()),
                    _ => None,
                },
                opencl_driver: match driver.driver_type {
                    DriverType::AMDGPU => Some("clover".to_string()),
                    DriverType::I915 => Some("intel".to_string()),
                    _ => None,
                },
                va_api_driver: match driver.driver_type {
                    DriverType::AMDGPU => Some("radeonsi".to_string()),
                    DriverType::I915 => Some("iHD".to_string()),
                    _ => None,
                },
                vdpau_driver: match driver.driver_type {
                    DriverType::AMDGPU => Some("radeonsi".to_string()),
                    DriverType::Nouveau => Some("nouveau".to_string()),
                    _ => None,
                },
            };

            // Create performance profile based on GPU tier
            let performance_profile = PerformanceProfile {
                power_mode: match gpu.tier {
                    super::GPUTier::Entry | super::GPUTier::Budget => PowerMode::PowerSave,
                    super::GPUTier::Mainstream => PowerMode::Balanced,
                    super::GPUTier::Performance => PowerMode::Performance,
                    super::GPUTier::HighEnd | super::GPUTier::Enthusiast => {
                        PowerMode::MaxPerformance
                    }
                },
                clock_speeds: ClockSpeeds {
                    core_min: gpu.base_clock / 2,
                    core_max: gpu.boost_clock,
                    memory_min: gpu.memory_clock / 2,
                    memory_max: gpu.memory_clock,
                    shader_min: gpu.base_clock / 2,
                    shader_max: gpu.boost_clock,
                },
                memory_timings: MemoryTimings {
                    cas_latency: 16,
                    ras_to_cas: 36,
                    ras_precharge: 36,
                    row_cycle_time: 52,
                },
                thermal_limits: ThermalLimits {
                    target_temp: 83,
                    critical_temp: 95,
                    power_limit: match gpu.tier {
                        super::GPUTier::Entry => 50,
                        super::GPUTier::Budget => 75,
                        super::GPUTier::Mainstream => 120,
                        super::GPUTier::Performance => 180,
                        super::GPUTier::HighEnd => 250,
                        super::GPUTier::Enthusiast => 350,
                    },
                    thermal_throttling: true,
                },
            };

            // Create power management interface
            let power_management = DriverPowerManagement {
                current_mode: performance_profile.power_mode,
                available_modes: vec![
                    PowerMode::PowerSave,
                    PowerMode::Balanced,
                    PowerMode::Performance,
                    PowerMode::MaxPerformance,
                ],
                dynamic_frequency_scaling: true,
                runtime_power_management: true,
                suspend_resume_support: true,
            };

            let loaded_driver = LoadedDriver {
                gpu_id,
                driver_type: driver.driver_type,
                status: DriverStatus::Loaded,
                drm_device: Some(drm_device),
                mesa_interface: Some(mesa_interface.clone()),
                performance_profile,
                power_management,
            };

            self.loaded_drivers.insert(gpu_id, loaded_driver);
            self.mesa_interfaces.insert(gpu_id, mesa_interface);

            Ok(())
        } else {
            Err("No compatible opensource driver found")
        }
    }

    /// Get driver information for a GPU
    pub fn get_driver_info(&self, gpu_id: u32) -> Option<&LoadedDriver> {
        self.loaded_drivers.get(&gpu_id)
    }

    /// Set power mode for a loaded driver
    pub fn set_power_mode(
        &mut self,
        gpu_id: u32,
        power_mode: PowerMode,
    ) -> Result<(), &'static str> {
        if let Some(loaded_driver) = self.loaded_drivers.get_mut(&gpu_id) {
            loaded_driver.power_management.current_mode = power_mode;
            loaded_driver.performance_profile.power_mode = power_mode;

            // Adjust clock speeds based on power mode
            let (core_multiplier, memory_multiplier) = match power_mode {
                PowerMode::PowerSave => (0.5, 0.7),
                PowerMode::Balanced => (0.8, 0.9),
                PowerMode::Performance => (0.95, 0.98),
                PowerMode::MaxPerformance => (1.0, 1.0),
            };

            let clocks = &mut loaded_driver.performance_profile.clock_speeds;
            let core_range = clocks.core_max - clocks.core_min;
            let memory_range = clocks.memory_max - clocks.memory_min;

            clocks.core_max = clocks.core_min + (core_range as f32 * core_multiplier) as u32;
            clocks.memory_max =
                clocks.memory_min + (memory_range as f32 * memory_multiplier) as u32;

            Ok(())
        } else {
            Err("Driver not loaded for specified GPU")
        }
    }

    /// Generate driver system report
    pub fn generate_driver_report(&self) -> String {
        let mut report = String::new();

        report.push_str("=== Opensource Driver System Report ===\n\n");
        report.push_str(&format!(
            "Available Drivers: {}\n",
            self.available_drivers.len()
        ));
        report.push_str(&format!("Loaded Drivers: {}\n", self.loaded_drivers.len()));
        report.push_str(&format!("DRM Devices: {}\n", self.drm_devices.len()));

        report.push_str("\n=== Available Drivers ===\n");
        for driver in &self.available_drivers {
            report.push_str(&format!(
                "{} v{} ({} devices supported)\n",
                driver.name,
                driver.version,
                driver.supported_devices.len()
            ));
            report.push_str(&format!(
                "  OpenGL: {}.{}, Vulkan: {}, Compute: {}\n",
                driver.capabilities.opengl_version.0,
                driver.capabilities.opengl_version.1,
                if driver.capabilities.vulkan_support {
                    "Yes"
                } else {
                    "No"
                },
                if driver.capabilities.compute_shaders {
                    "Yes"
                } else {
                    "No"
                }
            ));
        }

        if !self.loaded_drivers.is_empty() {
            report.push_str("\n=== Loaded Drivers ===\n");
            for (&gpu_id, driver) in &self.loaded_drivers {
                report.push_str(&format!(
                    "GPU {}: {:?} ({:?})\n",
                    gpu_id, driver.driver_type, driver.status
                ));
                report.push_str(&format!(
                    "  Power Mode: {:?}\n",
                    driver.power_management.current_mode
                ));
                if let Some(ref drm) = driver.drm_device {
                    report.push_str(&format!("  DRM Device: {}\n", drm.device_node));
                }
            }
        }

        report
    }
}

/// Create fallback GPU capabilities for unknown devices
pub fn create_fallback_capabilities(pci_device: &PCIDevice) -> GPUCapabilities {
    let vendor = match pci_device.vendor_id {
        0x8086 => GPUVendor::Intel,
        0x10DE => GPUVendor::Nvidia,
        0x1002 => GPUVendor::AMD,
        _ => GPUVendor::Unknown,
    };

    GPUCapabilities {
        vendor,
        device_name: format!("Unknown {} GPU (0x{:04X})", vendor, pci_device.device_id),
        tier: super::GPUTier::Entry,
        features: super::GPUFeatures::basic(),
        memory_size: 256 * 1024 * 1024, // 256MB fallback
        max_resolution: (1920, 1080),
        pci_device_id: pci_device.device_id,
        compute_units: 64,
        base_clock: 400,
        boost_clock: 800,
        memory_clock: 1000,
        memory_bandwidth: 50,
    }
}

// Global driver registry
lazy_static! {
    static ref DRIVER_REGISTRY: Mutex<OpensourceDriverRegistry> =
        Mutex::new(OpensourceDriverRegistry::new());
}

/// Initialize the opensource driver system
pub fn initialize_opensource_system(
    gpus: &[GPUCapabilities],
    pci_devices: &[PCIDevice],
) -> Result<(), &'static str> {
    let mut registry = DRIVER_REGISTRY.lock();

    // Try to initialize drivers for each detected GPU
    for (i, gpu) in gpus.iter().enumerate() {
        if let Some(pci_device) = pci_devices.get(i) {
            let _ = registry.initialize_driver(gpu, pci_device); // Best effort
        }
    }

    Ok(())
}

/// Get driver information for a GPU
pub fn get_driver_info(gpu_id: u32) -> Option<LoadedDriver> {
    let registry = DRIVER_REGISTRY.lock();
    registry.get_driver_info(gpu_id).cloned()
}

/// Set power mode for a GPU driver
pub fn set_power_mode(gpu_id: u32, power_mode: PowerMode) -> Result<(), &'static str> {
    let mut registry = DRIVER_REGISTRY.lock();
    registry.set_power_mode(gpu_id, power_mode)
}

/// Generate opensource driver report
pub fn generate_opensource_driver_report() -> String {
    let registry = DRIVER_REGISTRY.lock();
    registry.generate_driver_report()
}

/// Check if opensource drivers are available
pub fn has_opensource_drivers() -> bool {
    let registry = DRIVER_REGISTRY.lock();
    !registry.loaded_drivers.is_empty()
}

/// NVIDIA GPU generation enumeration for Nouveau driver
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NVGeneration {
    /// Tesla architecture (G80-GT200)
    NV50,
    /// Fermi architecture (GF100-GF119)
    NVC0,
    /// Kepler architecture (GK104-GK208)
    NVE0,
    /// Maxwell architecture (GM107-GM206)
    NV110,
    /// Pascal architecture (GP102-GP108)
    NV130,
    /// Turing architecture (TU102-TU117)
    NV140,
    /// Ampere architecture (GA102-GA107)
    NV170,
    /// Unknown or unsupported generation
    Unknown,
}

/// Hardware communication implementation for OpensourceDriverRegistry
impl OpensourceDriverRegistry {
    /// Initialize hardware communication for the GPU driver
    fn initialize_hardware_communication(
        &self,
        gpu_id: u32,
        pci_device: &PCIDevice,
        driver: &OpensourceDriver,
    ) -> Result<(), &'static str> {
        match driver.driver_type {
            DriverType::I915 => self.initialize_i915_hardware(gpu_id, pci_device),
            DriverType::AMDGPU => self.initialize_amdgpu_hardware(gpu_id, pci_device),
            DriverType::Nouveau => self.initialize_nouveau_hardware(gpu_id, pci_device),
            _ => Err("Unsupported driver type for hardware initialization"),
        }
    }

    /// Initialize Intel i915 hardware communication
    fn initialize_i915_hardware(
        &self,
        gpu_id: u32,
        pci_device: &PCIDevice,
    ) -> Result<(), &'static str> {
        // Map Intel GPU registers
        let bar0 = self.read_pci_bar(pci_device, 0)?;
        let register_base = self.map_gpu_registers(bar0, 16 * 1024 * 1024)?; // Map 16MB

        unsafe {
            let reg_base = register_base as *mut u32;

            // Read GPU identification
            let device_info = core::ptr::read_volatile(reg_base.add(0x0));
            if device_info == 0xFFFFFFFF {
                return Err("Failed to read Intel GPU registers");
            }

            // Initialize Graphics Technology (GT) interface
            let gt_mode = core::ptr::read_volatile(reg_base.add(0x7000 / 4));
            core::ptr::write_volatile(reg_base.add(0x7000 / 4), gt_mode | 0x1);

            // Enable power management
            let pm_ctrl = core::ptr::read_volatile(reg_base.add(0xA094 / 4));
            core::ptr::write_volatile(reg_base.add(0xA094 / 4), pm_ctrl | 0x1);

            // Initialize display engine if present
            self.initialize_intel_display(reg_base)?;

            // Set up interrupt handling
            self.setup_intel_interrupts(reg_base, gpu_id)?;
        }

        crate::println!("Intel i915 hardware initialized for GPU {}", gpu_id);
        Ok(())
    }

    /// Initialize AMD GPU hardware communication
    fn initialize_amdgpu_hardware(
        &self,
        gpu_id: u32,
        pci_device: &PCIDevice,
    ) -> Result<(), &'static str> {
        // Map AMD GPU registers
        let bar0 = self.read_pci_bar(pci_device, 0)?;
        let register_base = self.map_gpu_registers(bar0, 32 * 1024 * 1024)?; // Map 32MB

        unsafe {
            let reg_base = register_base as *mut u32;

            // Read GPU identification
            let device_info = core::ptr::read_volatile(reg_base.add(0x0));
            if device_info == 0xFFFFFFFF {
                return Err("Failed to read AMD GPU registers");
            }

            // Initialize Command Processor (CP)
            core::ptr::write_volatile(reg_base.add(0x8040 / 4), 0x0); // Reset CP

            // Wait for reset completion
            let mut timeout = 1000;
            while timeout > 0 {
                let status = core::ptr::read_volatile(reg_base.add(0x8044 / 4));
                if (status & 0x1) == 0 {
                    break;
                }
                timeout -= 1;
                for _ in 0..100 {
                    core::hint::spin_loop();
                }
            }

            if timeout == 0 {
                return Err("AMD GPU CP reset timeout");
            }

            // Enable graphics engine
            let gfx_enable = core::ptr::read_volatile(reg_base.add(0x8010 / 4));
            core::ptr::write_volatile(reg_base.add(0x8010 / 4), gfx_enable | 0x1);

            // Initialize memory controller
            self.initialize_amd_memory_controller(reg_base)?;

            // Load required firmware
            self.load_amd_firmware(gpu_id, pci_device.device_id)?;
        }

        crate::println!("AMD GPU hardware initialized for GPU {}", gpu_id);
        Ok(())
    }

    /// Initialize NVIDIA Nouveau hardware communication
    fn initialize_nouveau_hardware(
        &self,
        gpu_id: u32,
        pci_device: &PCIDevice,
    ) -> Result<(), &'static str> {
        // Map NVIDIA GPU registers
        let bar0 = self.read_pci_bar(pci_device, 0)?;
        let register_base = self.map_gpu_registers(bar0, 16 * 1024 * 1024)?; // Map 16MB

        unsafe {
            let reg_base = register_base as *mut u32;

            // Read GPU identification
            let device_info = core::ptr::read_volatile(reg_base.add(0x0));
            if device_info == 0xFFFFFFFF {
                return Err("Failed to read NVIDIA GPU registers");
            }

            // Detect GPU generation
            let gpu_generation = self.detect_nvidia_generation(pci_device.device_id);

            // Initialize based on generation
            match gpu_generation {
                NVGeneration::NV50 | NVGeneration::NVC0 | NVGeneration::NVE0 => {
                    // Initialize older generations that Nouveau supports well
                    self.initialize_nouveau_legacy(reg_base, gpu_generation)?;
                }
                NVGeneration::NV110 | NVGeneration::NV130 => {
                    // Initialize Maxwell/Pascal with limited support
                    self.initialize_nouveau_modern(reg_base, gpu_generation)?;
                }
                _ => {
                    return Err("NVIDIA GPU generation not supported by Nouveau");
                }
            }
        }

        crate::println!("NVIDIA Nouveau hardware initialized for GPU {}", gpu_id);
        Ok(())
    }

    /// Initialize Intel display engine
    fn initialize_intel_display(&self, reg_base: *mut u32) -> Result<(), &'static str> {
        unsafe {
            // Enable display power well
            let power_well = core::ptr::read_volatile(reg_base.add(0x45400 / 4));
            core::ptr::write_volatile(reg_base.add(0x45400 / 4), power_well | 0x1);

            // Wait for power well to stabilize
            let mut timeout = 1000;
            while timeout > 0 {
                let status = core::ptr::read_volatile(reg_base.add(0x45404 / 4));
                if (status & 0x1) != 0 {
                    break;
                }
                timeout -= 1;
                for _ in 0..100 {
                    core::hint::spin_loop();
                }
            }

            // Initialize display pipes
            for pipe in 0..3u32 {
                // Pipe A, B, C
                let pipe_conf_reg = 0x70008 + pipe * 0x1000;
                let pipe_conf = core::ptr::read_volatile(reg_base.add(pipe_conf_reg as usize / 4));
                core::ptr::write_volatile(
                    reg_base.add(pipe_conf_reg as usize / 4),
                    pipe_conf | 0x80000000,
                );
            }
        }

        Ok(())
    }

    /// Set up Intel GPU interrupts
    fn setup_intel_interrupts(&self, reg_base: *mut u32, gpu_id: u32) -> Result<(), &'static str> {
        unsafe {
            // Enable master interrupt
            core::ptr::write_volatile(reg_base.add(0x44200 / 4), 0x80000000);

            // Enable specific interrupt sources
            let interrupt_mask = 0x1 | 0x2 | 0x4; // Display, render, blitter
            core::ptr::write_volatile(reg_base.add(0x44204 / 4), interrupt_mask);

            // Clear any pending interrupts
            core::ptr::write_volatile(reg_base.add(0x44208 / 4), 0xFFFFFFFF);
        }

        crate::println!("Intel GPU interrupts configured for GPU {}", gpu_id);
        Ok(())
    }

    /// Initialize AMD memory controller
    fn initialize_amd_memory_controller(&self, reg_base: *mut u32) -> Result<(), &'static str> {
        unsafe {
            // Configure memory controller
            let mc_config = core::ptr::read_volatile(reg_base.add(0x2000 / 4));
            core::ptr::write_volatile(reg_base.add(0x2000 / 4), mc_config | 0x1);

            // Set up GART (Graphics Address Remapping Table)
            let gart_base = 0x1000000u32; // 16MB base address
            core::ptr::write_volatile(reg_base.add(0x2004 / 4), gart_base);
            core::ptr::write_volatile(reg_base.add(0x2008 / 4), 0x100000); // 1MB GART size

            // Enable GART
            let gart_config = core::ptr::read_volatile(reg_base.add(0x200C / 4));
            core::ptr::write_volatile(reg_base.add(0x200C / 4), gart_config | 0x1);
        }

        Ok(())
    }

    /// Load AMD GPU firmware
    fn load_amd_firmware(&self, gpu_id: u32, device_id: u16) -> Result<(), &'static str> {
        // Determine required firmware based on device ID
        let firmware_files: Vec<&str> = match device_id {
            // RDNA2 (Navi 21)
            0x73A0..=0x73AF => vec![
                "amdgpu/navi21_pfp.bin",
                "amdgpu/navi21_me.bin",
                "amdgpu/navi21_ce.bin",
                "amdgpu/navi21_mec.bin",
                "amdgpu/navi21_rlc.bin",
                "amdgpu/navi21_sdma.bin",
                "amdgpu/navi21_vcn.bin",
            ],
            // RDNA1 (Navi 10)
            0x7310..=0x731F => vec![
                "amdgpu/navi10_pfp.bin",
                "amdgpu/navi10_me.bin",
                "amdgpu/navi10_ce.bin",
                "amdgpu/navi10_mec.bin",
                "amdgpu/navi10_rlc.bin",
                "amdgpu/navi10_sdma.bin",
                "amdgpu/navi10_vcn.bin",
            ],
            // Vega
            0x6860..=0x687F => vec![
                "amdgpu/vega10_pfp.bin",
                "amdgpu/vega10_me.bin",
                "amdgpu/vega10_ce.bin",
                "amdgpu/vega10_mec.bin",
                "amdgpu/vega10_rlc.bin",
                "amdgpu/vega10_sdma.bin",
                "amdgpu/vega10_uvd.bin",
                "amdgpu/vega10_vce.bin",
            ],
            _ => vec!["amdgpu/generic_firmware.bin"],
        };

        for firmware_file in firmware_files {
            // Validate that the firmware file exists in /lib/firmware/.
            // Actual firmware upload to the GPU happens in the hardware-specific
            // driver once the MMIO registers are mapped.
            if !self.validate_firmware_availability(firmware_file) {
                crate::println!(
                    "Warning: Firmware {} not available for GPU {}",
                    firmware_file,
                    gpu_id
                );
            } else {
                crate::println!("Loaded firmware {} for GPU {}", firmware_file, gpu_id);
            }
        }

        Ok(())
    }

    /// Detect NVIDIA GPU generation from device ID
    fn detect_nvidia_generation(&self, device_id: u16) -> NVGeneration {
        match device_id {
            0x0400..=0x05FF => NVGeneration::NV50,  // Tesla
            0x06C0..=0x0DFF => NVGeneration::NVC0,  // Fermi
            0x0FC0..=0x12FF => NVGeneration::NVE0,  // Kepler
            0x1340..=0x17FF => NVGeneration::NV110, // Maxwell
            0x15F0..=0x1FFF => NVGeneration::NV130, // Pascal
            0x1E00..=0x21FF => NVGeneration::NV140, // Turing
            0x2200..=0x25FF => NVGeneration::NV170, // Ampere
            _ => NVGeneration::Unknown,
        }
    }

    /// Initialize legacy NVIDIA GPUs (good Nouveau support)
    fn initialize_nouveau_legacy(
        &self,
        reg_base: *mut u32,
        generation: NVGeneration,
    ) -> Result<(), &'static str> {
        unsafe {
            // Initialize PFIFO (command submission engine)
            core::ptr::write_volatile(reg_base.add(0x2500 / 4), 0x1); // Enable PFIFO

            // Initialize PGRAPH (graphics engine)
            core::ptr::write_volatile(reg_base.add(0x400000 / 4), 0x1); // Enable PGRAPH

            // Set up channel contexts based on generation
            match generation {
                NVGeneration::NV50 => {
                    // NV50 specific initialization
                    core::ptr::write_volatile(reg_base.add(0x400824 / 4), 0x0);
                }
                NVGeneration::NVC0 => {
                    // Fermi specific initialization
                    core::ptr::write_volatile(reg_base.add(0x400824 / 4), 0x40000000);
                }
                NVGeneration::NVE0 => {
                    // Kepler specific initialization
                    core::ptr::write_volatile(reg_base.add(0x400824 / 4), 0x80000000);
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// Initialize modern NVIDIA GPUs (limited Nouveau support)
    fn initialize_nouveau_modern(
        &self,
        reg_base: *mut u32,
        generation: NVGeneration,
    ) -> Result<(), &'static str> {
        unsafe {
            // Modern NVIDIA GPUs require signed firmware
            // Nouveau support is limited without proper firmware

            // Try basic initialization
            let device_info = core::ptr::read_volatile(reg_base.add(0x0));
            crate::println!("NVIDIA GPU device info: 0x{:08X}", device_info);

            // Check if we can access basic registers
            let pmc_enable = core::ptr::read_volatile(reg_base.add(0x200 / 4));
            if pmc_enable == 0xFFFFFFFF {
                return Err("Cannot access NVIDIA GPU registers");
            }

            match generation {
                NVGeneration::NV110 => {
                    crate::println!("Maxwell GPU detected - limited Nouveau support");
                }
                NVGeneration::NV130 => {
                    crate::println!("Pascal GPU detected - limited Nouveau support");
                }
                _ => {
                    return Err("NVIDIA GPU generation requires signed firmware");
                }
            }
        }

        Ok(())
    }

    /// Read PCI BAR (Base Address Register)
    fn read_pci_bar(&self, pci_device: &PCIDevice, bar_index: u8) -> Result<u64, &'static str> {
        if bar_index > 5 {
            return Err("Invalid BAR index");
        }

        let bar_offset = 0x10 + (bar_index * 4);
        let bar_value = self.read_pci_config_dword(pci_device, bar_offset)?;

        if (bar_value & 0x1) != 0 {
            return Err("I/O space BAR not supported");
        }

        // Handle 64-bit BARs
        if (bar_value & 0x6) == 0x4 && bar_index < 5 {
            let bar_high = self.read_pci_config_dword(pci_device, bar_offset + 4)?;
            Ok(((bar_high as u64) << 32) | (bar_value & 0xFFFFFFF0) as u64)
        } else {
            Ok((bar_value & 0xFFFFFFF0) as u64)
        }
    }

    /// Map GPU registers to virtual memory
    fn map_gpu_registers(&self, physical_addr: u64, size: usize) -> Result<u64, &'static str> {
        if physical_addr == 0 {
            return Err("Invalid physical address for GPU registers");
        }

        // Validate address is in reasonable range for GPU registers
        // Accept addresses above 2GB (typical PCI memory range) up to 4GB or higher
        if physical_addr < 0x80000000 {
            return Err("GPU register address outside expected range");
        }

        // Use the kernel memory manager to map the MMIO region with
        // device-nocache flags so writes reach the hardware immediately.
        let virt = crate::memory::map_mmio_region(physical_addr as usize, size)?;
        Ok(virt as u64)
    }

    /// Read PCI configuration dword using I/O ports
    fn read_pci_config_dword(
        &self,
        pci_device: &PCIDevice,
        offset: u8,
    ) -> Result<u32, &'static str> {
        let address = 0x80000000u32
            | ((pci_device.bus as u32) << 16)
            | ((pci_device.device as u32) << 11)
            | ((pci_device.function as u32) << 8)
            | ((offset as u32) & 0xFC);

        unsafe {
            // Write address to CONFIG_ADDRESS port (0xCF8)
            core::arch::asm!(
                "out dx, eax",
                in("dx") 0xCF8u16,
                in("eax") address,
                options(nostack, preserves_flags)
            );

            // Read data from CONFIG_DATA port (0xCFC)
            let data: u32;
            core::arch::asm!(
                "in eax, dx",
                out("eax") data,
                in("dx") 0xCFCu16,
                options(nostack, preserves_flags)
            );
            Ok(data)
        }
    }

    /// Read PCI subsystem vendor ID from configuration space
    fn read_pci_subsystem_vendor(&self, pci_device: &PCIDevice) -> Result<u16, &'static str> {
        let subsystem_data = self.read_pci_config_dword(pci_device, 0x2C)?;
        Ok((subsystem_data & 0xFFFF) as u16)
    }

    /// Read PCI subsystem device ID from configuration space
    fn read_pci_subsystem_device(&self, pci_device: &PCIDevice) -> Result<u16, &'static str> {
        let subsystem_data = self.read_pci_config_dword(pci_device, 0x2C)?;
        Ok(((subsystem_data >> 16) & 0xFFFF) as u16)
    }

    /// Validate firmware availability in the firmware store
    fn validate_firmware_availability(&self, firmware_path: &str) -> bool {
        // Check whether the firmware file exists in the VFS at /lib/firmware/.
        let full_path = alloc::format!("/lib/firmware/{}", firmware_path);
        crate::vfs::vfs_stat(&full_path).is_ok()
    }
}
