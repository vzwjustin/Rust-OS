//! AMDGPU Driver Integration for AMD GPUs
//!
//! This module provides integration with the AMDGPU opensource driver
//! for modern AMD graphics cards (GCN and RDNA architectures).

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

/// AMDGPU driver context for AMD GPUs
#[derive(Debug)]
pub struct AMDGPUContext {
    pub device_id: u16,
    pub family: AMDGPUFamily,
    pub chip_class: AMDChipClass,
    pub compute_units: u32,
    pub vram_size: u64,
    pub gart_size: u64,
    pub has_uvd: bool, // Unified Video Decoder
    pub has_vce: bool, // Video Compression Engine
    pub has_vcn: bool, // Video Core Next
    pub ring_count: u8,
    pub queue_count: u32,
}

/// AMD GPU families supported by AMDGPU
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AMDGPUFamily {
    // GCN generations
    SouthernIslands, // GCN 1.0 (HD 7000, R7/R9 200)
    SeaIslands,      // GCN 2.0 (R7/R9 300)
    VolcanicIslands, // GCN 3.0 (R9 Fury, RX 400/500)
    ArcticIslands,   // GCN 4.0 (RX 500)
    Vega,            // GCN 5.0 (RX Vega)

    // RDNA generations
    Navi10, // RDNA 1.0 (RX 5000)
    Navi14, // RDNA 1.0 (RX 5500)
    Navi21, // RDNA 2.0 (RX 6800/6900)
    Navi22, // RDNA 2.0 (RX 6700)
    Navi23, // RDNA 2.0 (RX 6600)
    Navi24, // RDNA 2.0 (RX 6500/6400)
    Navi31, // RDNA 3.0 (RX 7900)
    Navi32, // RDNA 3.0 (RX 7800/7700)
    Navi33, // RDNA 3.0 (RX 7600)
}

/// AMD chip classes for feature detection
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AMDChipClass {
    GCN1,
    GCN2,
    GCN3,
    GCN4,
    GCN5,
    RDNA1,
    RDNA2,
    RDNA3,
}

impl AMDGPUContext {
    pub fn new(device_id: u16) -> Self {
        let family = Self::detect_family(device_id);
        let chip_class = Self::get_chip_class(family);
        let compute_units = Self::estimate_compute_units(device_id, family);
        let (vram_size, gart_size) = Self::estimate_memory_sizes(device_id, family);
        let (has_uvd, has_vce, has_vcn) = Self::get_video_engines(family);
        let ring_count = Self::get_ring_count(chip_class);
        let queue_count = Self::get_queue_count(chip_class);

        Self {
            device_id,
            family,
            chip_class,
            compute_units,
            vram_size,
            gart_size,
            has_uvd,
            has_vce,
            has_vcn,
            ring_count,
            queue_count,
        }
    }

    fn detect_family(device_id: u16) -> AMDGPUFamily {
        match device_id {
            // Southern Islands (GCN 1.0)
            0x6780..=0x679F | 0x6800..=0x683F => AMDGPUFamily::SouthernIslands,

            // Sea Islands (GCN 2.0)
            0x6600..=0x665F | 0x9830..=0x983F => AMDGPUFamily::SeaIslands,

            // Volcanic Islands (GCN 3.0)
            0x6900..=0x692F | 0x7300..=0x731F => AMDGPUFamily::VolcanicIslands,

            // Arctic Islands (GCN 4.0)
            0x67C0..=0x67FF | 0x6980..=0x699F => AMDGPUFamily::ArcticIslands,

            // Vega (GCN 5.0)
            0x6860..=0x687F | 0x69A0..=0x69AF => AMDGPUFamily::Vega,

            // Navi 10 (RDNA 1.0)
            0x7310..=0x731F => AMDGPUFamily::Navi10,

            // Navi 14 (RDNA 1.0)
            0x7340..=0x7347 => AMDGPUFamily::Navi14,

            // Navi 21 (RDNA 2.0)
            0x73A0..=0x73AF => AMDGPUFamily::Navi21,

            // Navi 22 (RDNA 2.0)
            0x73C0..=0x73CF => AMDGPUFamily::Navi22,

            // Navi 23 (RDNA 2.0)
            0x73E0..=0x73FF => AMDGPUFamily::Navi23,

            // Navi 24 (RDNA 2.0)
            0x7480..=0x748F => AMDGPUFamily::Navi24,

            // Navi 31 (RDNA 3.0)
            0x744C => AMDGPUFamily::Navi31,

            // Navi 32 (RDNA 3.0)
            0x7448..=0x7449 => AMDGPUFamily::Navi32,

            // Navi 33 (RDNA 3.0)
            0x747E | 0x7480 => AMDGPUFamily::Navi33,

            _ => AMDGPUFamily::SeaIslands, // Default fallback
        }
    }

    fn get_chip_class(family: AMDGPUFamily) -> AMDChipClass {
        match family {
            AMDGPUFamily::SouthernIslands => AMDChipClass::GCN1,
            AMDGPUFamily::SeaIslands => AMDChipClass::GCN2,
            AMDGPUFamily::VolcanicIslands => AMDChipClass::GCN3,
            AMDGPUFamily::ArcticIslands => AMDChipClass::GCN4,
            AMDGPUFamily::Vega => AMDChipClass::GCN5,
            AMDGPUFamily::Navi10 | AMDGPUFamily::Navi14 => AMDChipClass::RDNA1,
            AMDGPUFamily::Navi21
            | AMDGPUFamily::Navi22
            | AMDGPUFamily::Navi23
            | AMDGPUFamily::Navi24 => AMDChipClass::RDNA2,
            AMDGPUFamily::Navi31 | AMDGPUFamily::Navi32 | AMDGPUFamily::Navi33 => {
                AMDChipClass::RDNA3
            }
        }
    }

    fn estimate_compute_units(device_id: u16, family: AMDGPUFamily) -> u32 {
        match family {
            AMDGPUFamily::SouthernIslands => {
                match device_id {
                    0x6798..=0x679B => 32, // HD 7970/7950
                    0x6818..=0x681F => 28, // HD 7870
                    0x6800..=0x6809 => 20, // HD 7770
                    _ => 16,
                }
            }
            AMDGPUFamily::SeaIslands => {
                match device_id {
                    0x6600..=0x6603 => 44, // R9 290X/290
                    0x6610..=0x6613 => 40, // R9 280X
                    0x6640..=0x6641 => 28, // R9 270X
                    _ => 20,
                }
            }
            AMDGPUFamily::VolcanicIslands => {
                match device_id {
                    0x7300 => 64,          // R9 Fury X
                    0x7310..=0x7312 => 56, // R9 Fury
                    _ => 36,
                }
            }
            AMDGPUFamily::ArcticIslands => {
                match device_id {
                    0x67C0..=0x67C7 => 64, // RX 480/580
                    0x67D0..=0x67DF => 36, // RX 470/570
                    0x67E0..=0x67EF => 32, // RX 460/560
                    _ => 28,
                }
            }
            AMDGPUFamily::Vega => {
                match device_id {
                    0x6860..=0x6863 => 64, // RX Vega 64
                    0x6864..=0x6867 => 56, // RX Vega 56
                    _ => 64,
                }
            }
            AMDGPUFamily::Navi10 => {
                match device_id {
                    0x7310 => 40, // RX 5700 XT
                    0x7312 => 36, // RX 5700
                    _ => 40,
                }
            }
            AMDGPUFamily::Navi14 => {
                match device_id {
                    0x7340 => 24, // RX 5500 XT
                    0x7341 => 22, // RX 5500
                    _ => 24,
                }
            }
            AMDGPUFamily::Navi21 => {
                match device_id {
                    0x73A1 => 80, // RX 6900 XT
                    0x73A2 => 72, // RX 6800 XT
                    0x73A3 => 60, // RX 6800
                    _ => 80,
                }
            }
            AMDGPUFamily::Navi22 => {
                match device_id {
                    0x73C0 => 40, // RX 6700 XT
                    0x73C1 => 36, // RX 6700
                    _ => 40,
                }
            }
            AMDGPUFamily::Navi23 => {
                match device_id {
                    0x73AB => 32, // RX 6600 XT
                    0x73AE => 28, // RX 6600
                    _ => 32,
                }
            }
            AMDGPUFamily::Navi24 => 16, // RX 6500/6400
            AMDGPUFamily::Navi31 => 96, // RX 7900 XTX/XT
            AMDGPUFamily::Navi32 => 60, // RX 7800/7700
            AMDGPUFamily::Navi33 => 32, // RX 7600
        }
    }

    fn estimate_memory_sizes(device_id: u16, family: AMDGPUFamily) -> (u64, u64) {
        let (vram_gb, gart_gb) = match family {
            AMDGPUFamily::SouthernIslands => (2, 4),
            AMDGPUFamily::SeaIslands => (4, 8),
            AMDGPUFamily::VolcanicIslands => (4, 8),
            AMDGPUFamily::ArcticIslands => {
                match device_id {
                    0x67C0..=0x67C7 => (8, 16), // RX 480/580
                    _ => (4, 8),
                }
            }
            AMDGPUFamily::Vega => (8, 16),
            AMDGPUFamily::Navi10 => (8, 16),
            AMDGPUFamily::Navi14 => (8, 16),
            AMDGPUFamily::Navi21 => {
                match device_id {
                    0x73A1 | 0x73A2 => (16, 32), // RX 6900/6800 XT
                    _ => (16, 32),
                }
            }
            AMDGPUFamily::Navi22 => (12, 24),
            AMDGPUFamily::Navi23 => (8, 16),
            AMDGPUFamily::Navi24 => (4, 8),
            AMDGPUFamily::Navi31 => (24, 48), // RX 7900 XTX
            AMDGPUFamily::Navi32 => (16, 32),
            AMDGPUFamily::Navi33 => (8, 16),
        };

        (vram_gb * 1024 * 1024 * 1024, gart_gb * 1024 * 1024 * 1024)
    }

    fn get_video_engines(family: AMDGPUFamily) -> (bool, bool, bool) {
        // Returns (has_uvd, has_vce, has_vcn)
        match family {
            AMDGPUFamily::SouthernIslands => (true, false, false),
            AMDGPUFamily::SeaIslands => (true, true, false),
            AMDGPUFamily::VolcanicIslands => (true, true, false),
            AMDGPUFamily::ArcticIslands => (true, true, false),
            AMDGPUFamily::Vega => (false, false, true), // VCN 1.0
            AMDGPUFamily::Navi10 | AMDGPUFamily::Navi14 => (false, false, true), // VCN 2.0
            AMDGPUFamily::Navi21
            | AMDGPUFamily::Navi22
            | AMDGPUFamily::Navi23
            | AMDGPUFamily::Navi24 => (false, false, true), // VCN 3.0
            AMDGPUFamily::Navi31 | AMDGPUFamily::Navi32 | AMDGPUFamily::Navi33 => {
                (false, false, true)
            } // VCN 4.0
        }
    }

    fn get_ring_count(chip_class: AMDChipClass) -> u8 {
        match chip_class {
            AMDChipClass::GCN1 | AMDChipClass::GCN2 => 5,
            AMDChipClass::GCN3 | AMDChipClass::GCN4 => 6,
            AMDChipClass::GCN5 => 8,
            AMDChipClass::RDNA1 => 10,
            AMDChipClass::RDNA2 => 12,
            AMDChipClass::RDNA3 => 16,
        }
    }

    fn get_queue_count(chip_class: AMDChipClass) -> u32 {
        match chip_class {
            AMDChipClass::GCN1 => 64,
            AMDChipClass::GCN2 => 128,
            AMDChipClass::GCN3 | AMDChipClass::GCN4 => 256,
            AMDChipClass::GCN5 => 512,
            AMDChipClass::RDNA1 => 1024,
            AMDChipClass::RDNA2 => 2048,
            AMDChipClass::RDNA3 => 4096,
        }
    }

    pub fn supports_compute(&self) -> bool {
        true // All AMDGPU devices support compute
    }

    pub fn supports_video_decode(&self) -> bool {
        self.has_uvd || self.has_vcn
    }

    pub fn supports_video_encode(&self) -> bool {
        self.has_vce || self.has_vcn
    }

    pub fn supports_ray_tracing(&self) -> bool {
        matches!(self.chip_class, AMDChipClass::RDNA2 | AMDChipClass::RDNA3)
    }

    pub fn supports_variable_rate_shading(&self) -> bool {
        matches!(self.chip_class, AMDChipClass::RDNA2 | AMDChipClass::RDNA3)
    }

    pub fn supports_mesh_shaders(&self) -> bool {
        matches!(self.chip_class, AMDChipClass::RDNA3)
    }

    pub fn get_opengl_version(&self) -> (u8, u8) {
        match self.chip_class {
            AMDChipClass::GCN1 | AMDChipClass::GCN2 => (4, 2),
            AMDChipClass::GCN3 | AMDChipClass::GCN4 => (4, 5),
            AMDChipClass::GCN5 => (4, 6),
            AMDChipClass::RDNA1 | AMDChipClass::RDNA2 | AMDChipClass::RDNA3 => (4, 6),
        }
    }

    pub fn get_vulkan_version(&self) -> Option<(u8, u8, u8)> {
        match self.chip_class {
            AMDChipClass::GCN1 | AMDChipClass::GCN2 => None,
            AMDChipClass::GCN3 | AMDChipClass::GCN4 => Some((1, 0, 0)),
            AMDChipClass::GCN5 => Some((1, 2, 0)),
            AMDChipClass::RDNA1 => Some((1, 2, 0)),
            AMDChipClass::RDNA2 => Some((1, 3, 0)),
            AMDChipClass::RDNA3 => Some((1, 3, 0)),
        }
    }

    pub fn get_opencl_version(&self) -> Option<(u8, u8)> {
        match self.chip_class {
            AMDChipClass::GCN1 => Some((1, 2)),
            AMDChipClass::GCN2 | AMDChipClass::GCN3 => Some((2, 0)),
            AMDChipClass::GCN4 | AMDChipClass::GCN5 => Some((2, 1)),
            AMDChipClass::RDNA1 | AMDChipClass::RDNA2 | AMDChipClass::RDNA3 => Some((2, 1)),
        }
    }

    pub fn get_required_firmware(&self) -> Vec<String> {
        let mut firmware = Vec::new();

        let family_name = match self.family {
            AMDGPUFamily::SouthernIslands => "si",
            AMDGPUFamily::SeaIslands => "cik",
            AMDGPUFamily::VolcanicIslands => "vi",
            AMDGPUFamily::ArcticIslands => "polaris",
            AMDGPUFamily::Vega => "vega",
            AMDGPUFamily::Navi10 => "navi10",
            AMDGPUFamily::Navi14 => "navi14",
            AMDGPUFamily::Navi21 => "navi21",
            AMDGPUFamily::Navi22 => "navi22",
            AMDGPUFamily::Navi23 => "navi23",
            AMDGPUFamily::Navi24 => "navi24",
            AMDGPUFamily::Navi31 => "gc_11_0_0",
            AMDGPUFamily::Navi32 => "gc_11_0_1",
            AMDGPUFamily::Navi33 => "gc_11_0_2",
        };

        // Graphics firmware
        firmware.push(format!("amdgpu/{}_pfp.bin", family_name));
        firmware.push(format!("amdgpu/{}_me.bin", family_name));
        firmware.push(format!("amdgpu/{}_ce.bin", family_name));

        // MEC (compute) firmware
        if matches!(
            self.chip_class,
            AMDChipClass::GCN3
                | AMDChipClass::GCN4
                | AMDChipClass::GCN5
                | AMDChipClass::RDNA1
                | AMDChipClass::RDNA2
                | AMDChipClass::RDNA3
        ) {
            firmware.push(format!("amdgpu/{}_mec.bin", family_name));
            firmware.push(format!("amdgpu/{}_mec2.bin", family_name));
        }

        // RLC (RunList Controller) firmware
        firmware.push(format!("amdgpu/{}_rlc.bin", family_name));

        // SDMA (System DMA) firmware
        firmware.push(format!("amdgpu/{}_sdma.bin", family_name));
        if matches!(
            self.chip_class,
            AMDChipClass::GCN4
                | AMDChipClass::GCN5
                | AMDChipClass::RDNA1
                | AMDChipClass::RDNA2
                | AMDChipClass::RDNA3
        ) {
            firmware.push(format!("amdgpu/{}_sdma1.bin", family_name));
        }

        // Video firmware
        if self.has_uvd {
            firmware.push(format!("amdgpu/{}_uvd.bin", family_name));
        }
        if self.has_vce {
            firmware.push(format!("amdgpu/{}_vce.bin", family_name));
        }
        if self.has_vcn {
            firmware.push(format!("amdgpu/{}_vcn.bin", family_name));
        }

        firmware
    }

    pub fn get_driver_features(&self) -> super::DriverCapabilities {
        let (opengl_major, opengl_minor) = self.get_opengl_version();

        super::DriverCapabilities {
            direct_rendering: true,
            hardware_acceleration: true,
            compute_shaders: self.supports_compute(),
            video_decode: self.supports_video_decode(),
            video_encode: self.supports_video_encode(),
            vulkan_support: self.get_vulkan_version().is_some(),
            opengl_version: (opengl_major, opengl_minor),
            glsl_version: match self.chip_class {
                AMDChipClass::GCN1 | AMDChipClass::GCN2 => 420,
                AMDChipClass::GCN3 | AMDChipClass::GCN4 => 450,
                AMDChipClass::GCN5
                | AMDChipClass::RDNA1
                | AMDChipClass::RDNA2
                | AMDChipClass::RDNA3 => 460,
            },
            opencl_support: self.get_opencl_version().is_some(),
            ray_tracing: self.supports_ray_tracing(),
        }
    }
}

/// Initialize AMDGPU driver for a specific AMD GPU
pub fn initialize_amdgpu_driver(device_id: u16) -> Result<AMDGPUContext, &'static str> {
    let context = AMDGPUContext::new(device_id);

    // Check if the device is supported
    if matches!(context.family, AMDGPUFamily::SouthernIslands) {
        // Southern Islands requires special handling
        return Ok(context); // Still supported but with limitations
    }

    Ok(context)
}

/// Get AMDGPU driver information
pub fn get_amdgpu_info() -> super::OpensourceDriver {
    super::OpensourceDriver {
        driver_type: super::DriverType::AMDGPU,
        name: "AMDGPU".to_string(),
        version: "23.20".to_string(),
        supported_devices: vec![], // Would be populated from main database
        capabilities: super::DriverCapabilities::ADVANCED,
        mesa_driver: Some("radeonsi".to_string()),
        kernel_module: "amdgpu".to_string(),
        user_space_driver: "libamdgpu".to_string(),
        priority: 85,
    }
}
