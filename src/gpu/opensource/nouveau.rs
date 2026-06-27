//! Nouveau Driver Integration for NVIDIA GPUs
//!
//! This module provides integration with the Nouveau opensource driver
//! for NVIDIA graphics cards, supporting multiple GPU generations.

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

/// Nouveau driver context for NVIDIA GPUs
#[derive(Debug)]
pub struct NouveauContext {
    pub device_id: u16,
    pub generation: NouveauGeneration,
    pub channel_count: u32,
    pub vram_size: u64,
    pub gart_size: u64,
    pub compute_class: Option<u16>,
    pub copy_engines: u8,
    pub display_engines: u8,
}

/// NVIDIA GPU generations supported by Nouveau
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NouveauGeneration {
    NV04,  // TNT, TNT2
    NV10,  // GeForce 256, GeForce2
    NV20,  // GeForce3, GeForce4 Ti
    NV30,  // GeForce FX
    NV40,  // GeForce 6
    NV50,  // GeForce 8, 9, GTX 200
    NVC0,  // GeForce GTX 400, 500 (Fermi)
    NVE0,  // GeForce GTX 600, 700 (Kepler)
    NV110, // GeForce GTX 900 (Maxwell)
    NV130, // GeForce GTX 10xx (Pascal)
    NV140, // GeForce RTX 20xx (Turing)
    NV170, // GeForce RTX 30xx (Ampere)
}

impl NouveauContext {
    pub fn new(device_id: u16) -> Self {
        let generation = Self::detect_generation(device_id);
        let (vram_size, gart_size) = Self::estimate_memory_sizes(device_id, generation);
        let channel_count = Self::get_channel_count(generation);
        let compute_class = Self::get_compute_class(generation);
        let (copy_engines, display_engines) = Self::get_engine_counts(generation);

        Self {
            device_id,
            generation,
            channel_count,
            vram_size,
            gart_size,
            compute_class,
            copy_engines,
            display_engines,
        }
    }

    #[allow(unreachable_patterns)]
    fn detect_generation(device_id: u16) -> NouveauGeneration {
        match device_id {
            // NV04 generation
            0x0020..=0x002F | 0x00A0 => NouveauGeneration::NV04,

            // NV10 generation
            0x0100..=0x01FF | 0x0110..=0x011F | 0x0150..=0x015F => NouveauGeneration::NV10,

            // NV20 generation
            0x0200..=0x02FF | 0x0250..=0x025F => NouveauGeneration::NV20,

            // NV30 generation
            0x0300..=0x03FF | 0x0330..=0x033F => NouveauGeneration::NV30,

            // NV40 generation
            0x0040..=0x004F | 0x00C0..=0x00CF | 0x0140..=0x014F => NouveauGeneration::NV40,

            // NV50 generation (Tesla)
            0x0191..=0x0197
            | 0x019D..=0x019E
            | 0x0400..=0x042F
            | 0x05E0..=0x05FF
            | 0x0600..=0x061F
            | 0x06E0..=0x06FF => NouveauGeneration::NV50,

            // NVC0 generation (Fermi)
            0x06C0..=0x06DF | 0x0DC0..=0x0DFF | 0x1040..=0x109F => NouveauGeneration::NVC0,

            // NVE0 generation (Kepler)
            0x0FC0..=0x0FFF | 0x1000..=0x103F | 0x1180..=0x11FA | 0x1280..=0x12BA => {
                NouveauGeneration::NVE0
            }

            // NV110 generation (Maxwell)
            0x1340..=0x13FF | 0x1380..=0x139F | 0x17C0..=0x17FF => NouveauGeneration::NV110,

            // NV130 generation (Pascal)
            0x15F0..=0x15FF | 0x1B00..=0x1BFF | 0x1C00..=0x1CFF | 0x1D00..=0x1DFF => {
                NouveauGeneration::NV130
            }

            // NV140 generation (Turing)
            0x1E00..=0x1EFF | 0x1F00..=0x1FFF | 0x2180..=0x21FF => NouveauGeneration::NV140,

            // NV170 generation (Ampere)
            0x2200..=0x22FF | 0x2400..=0x24FF | 0x2500..=0x25FF => NouveauGeneration::NV170,

            _ => NouveauGeneration::NV50, // Default fallback
        }
    }

    fn estimate_memory_sizes(device_id: u16, generation: NouveauGeneration) -> (u64, u64) {
        let (vram_mb, gart_mb) = match generation {
            NouveauGeneration::NV04 | NouveauGeneration::NV10 => (32, 64),
            NouveauGeneration::NV20 | NouveauGeneration::NV30 => (128, 256),
            NouveauGeneration::NV40 => (256, 512),
            NouveauGeneration::NV50 => {
                match device_id {
                    0x0191..=0x0194 => (256, 512),  // Low-end
                    0x0400..=0x040F => (512, 1024), // Mid-range
                    _ => (1024, 2048),              // High-end
                }
            }
            NouveauGeneration::NVC0 => {
                match device_id {
                    0x0DC0..=0x0DCF => (1024, 2048), // GTX 400 series
                    0x1040..=0x104F => (1024, 2048), // GTX 500 series
                    _ => (2048, 4096),
                }
            }
            NouveauGeneration::NVE0 => {
                match device_id {
                    0x1180..=0x118F => (2048, 4096), // GTX 600 series
                    0x1190..=0x119F => (4096, 8192), // GTX 700 series
                    _ => (2048, 4096),
                }
            }
            NouveauGeneration::NV110 => {
                match device_id {
                    0x1340..=0x134F => (2048, 4096), // GTX 900 series
                    0x1380..=0x138F => (4096, 8192), // GTX 900 Ti series
                    _ => (4096, 8192),
                }
            }
            NouveauGeneration::NV130 => {
                match device_id {
                    0x1B80..=0x1B8F => (8192, 16384), // GTX 1080 series
                    0x1C00..=0x1C0F => (6144, 12288), // GTX 1060 series
                    _ => (4096, 8192),
                }
            }
            NouveauGeneration::NV140 => {
                match device_id {
                    0x1E00..=0x1E0F => (24576, 32768), // RTX 2080 Ti
                    0x1F00..=0x1F0F => (8192, 16384),  // RTX 2070
                    _ => (6144, 12288),
                }
            }
            NouveauGeneration::NV170 => {
                match device_id {
                    0x2204 => (24576, 32768), // RTX 3090
                    0x2206 => (10240, 16384), // RTX 3080
                    0x2484 => (8192, 16384),  // RTX 3070
                    _ => (8192, 16384),
                }
            }
        };

        (vram_mb * 1024 * 1024, gart_mb * 1024 * 1024)
    }

    fn get_channel_count(generation: NouveauGeneration) -> u32 {
        match generation {
            NouveauGeneration::NV04 | NouveauGeneration::NV10 => 8,
            NouveauGeneration::NV20 | NouveauGeneration::NV30 => 16,
            NouveauGeneration::NV40 => 32,
            NouveauGeneration::NV50 => 128,
            NouveauGeneration::NVC0 => 512,
            NouveauGeneration::NVE0 => 1024,
            NouveauGeneration::NV110 => 2048,
            NouveauGeneration::NV130 => 4096,
            NouveauGeneration::NV140 | NouveauGeneration::NV170 => 8192,
        }
    }

    fn get_compute_class(generation: NouveauGeneration) -> Option<u16> {
        match generation {
            NouveauGeneration::NV04
            | NouveauGeneration::NV10
            | NouveauGeneration::NV20
            | NouveauGeneration::NV30
            | NouveauGeneration::NV40
            | NouveauGeneration::NV50 => None,
            NouveauGeneration::NVC0 => Some(0x90C0), // Fermi compute
            NouveauGeneration::NVE0 => Some(0xA0C0), // Kepler compute
            NouveauGeneration::NV110 => Some(0xB0C0), // Maxwell compute
            NouveauGeneration::NV130 => Some(0xC0C0), // Pascal compute
            NouveauGeneration::NV140 => Some(0xC5C0), // Turing compute
            NouveauGeneration::NV170 => Some(0xC7C0), // Ampere compute
        }
    }

    fn get_engine_counts(generation: NouveauGeneration) -> (u8, u8) {
        // Returns (copy_engines, display_engines)
        match generation {
            NouveauGeneration::NV04 | NouveauGeneration::NV10 => (0, 1),
            NouveauGeneration::NV20 | NouveauGeneration::NV30 => (0, 2),
            NouveauGeneration::NV40 => (0, 2),
            NouveauGeneration::NV50 => (1, 2),
            NouveauGeneration::NVC0 => (2, 4),
            NouveauGeneration::NVE0 => (3, 4),
            NouveauGeneration::NV110 => (2, 4),
            NouveauGeneration::NV130 => (3, 4),
            NouveauGeneration::NV140 => (5, 4),
            NouveauGeneration::NV170 => (9, 4),
        }
    }

    pub fn supports_compute(&self) -> bool {
        self.compute_class.is_some()
    }

    pub fn supports_video_decode(&self) -> bool {
        matches!(
            self.generation,
            NouveauGeneration::NV50
                | NouveauGeneration::NVC0
                | NouveauGeneration::NVE0
                | NouveauGeneration::NV110
                | NouveauGeneration::NV130
                | NouveauGeneration::NV140
                | NouveauGeneration::NV170
        )
    }

    pub fn supports_video_encode(&self) -> bool {
        matches!(
            self.generation,
            NouveauGeneration::NVE0
                | NouveauGeneration::NV110
                | NouveauGeneration::NV130
                | NouveauGeneration::NV140
                | NouveauGeneration::NV170
        )
    }

    pub fn get_opengl_version(&self) -> (u8, u8) {
        match self.generation {
            NouveauGeneration::NV04 => (1, 3),
            NouveauGeneration::NV10 => (1, 5),
            NouveauGeneration::NV20 => (2, 0),
            NouveauGeneration::NV30 => (2, 1),
            NouveauGeneration::NV40 => (3, 0),
            NouveauGeneration::NV50 => (3, 3),
            NouveauGeneration::NVC0 => (4, 1),
            NouveauGeneration::NVE0 => (4, 3),
            NouveauGeneration::NV110 => (4, 5),
            NouveauGeneration::NV130 => (4, 6),
            NouveauGeneration::NV140 | NouveauGeneration::NV170 => (4, 6),
        }
    }

    pub fn get_vulkan_support(&self) -> bool {
        matches!(
            self.generation,
            NouveauGeneration::NVE0
                | NouveauGeneration::NV110
                | NouveauGeneration::NV130
                | NouveauGeneration::NV140
                | NouveauGeneration::NV170
        )
    }

    pub fn get_required_firmware(&self) -> Vec<String> {
        let mut firmware = Vec::new();

        match self.generation {
            NouveauGeneration::NV50 => {
                firmware.push("nouveau/nv50_ctxprog".to_string());
                firmware.push("nouveau/nv84_xuc103".to_string());
                firmware.push("nouveau/nv84_xuc003".to_string());
            }
            NouveauGeneration::NVC0 => {
                firmware.push("nouveau/nvc0_ctxsw".to_string());
                firmware.push("nouveau/nvc0_fuc409c".to_string());
                firmware.push("nouveau/nvc0_fuc409d".to_string());
                firmware.push("nouveau/nvc0_fuc41ac".to_string());
                firmware.push("nouveau/nvc0_fuc41ad".to_string());
            }
            NouveauGeneration::NVE0 => {
                firmware.push("nouveau/nve0_ctxsw".to_string());
                firmware.push("nouveau/nve0_fuc409c".to_string());
                firmware.push("nouveau/nve0_fuc409d".to_string());
                firmware.push("nouveau/nve0_fuc41ac".to_string());
                firmware.push("nouveau/nve0_fuc41ad".to_string());
            }
            NouveauGeneration::NV110 => {
                firmware.push("nouveau/nv110_ctxsw".to_string());
                firmware.push("nouveau/gm200_gr_gpccs_inst".to_string());
                firmware.push("nouveau/gm200_gr_gpccs_data".to_string());
            }
            NouveauGeneration::NV130 => {
                firmware.push("nouveau/gp100_gr_gpccs_inst".to_string());
                firmware.push("nouveau/gp100_gr_gpccs_data".to_string());
                firmware.push("nouveau/gp100_gr_fecs_inst".to_string());
                firmware.push("nouveau/gp100_gr_fecs_data".to_string());
            }
            NouveauGeneration::NV140 => {
                firmware.push("nouveau/tu102_gr_gpccs_inst".to_string());
                firmware.push("nouveau/tu102_gr_gpccs_data".to_string());
                firmware.push("nouveau/tu102_gr_fecs_inst".to_string());
                firmware.push("nouveau/tu102_gr_fecs_data".to_string());
            }
            NouveauGeneration::NV170 => {
                firmware.push("nouveau/ga102_gr_gpccs_inst".to_string());
                firmware.push("nouveau/ga102_gr_gpccs_data".to_string());
                firmware.push("nouveau/ga102_gr_fecs_inst".to_string());
                firmware.push("nouveau/ga102_gr_fecs_data".to_string());
            }
            _ => {} // Older generations don't require firmware
        }

        firmware
    }

    pub fn get_driver_features(&self) -> super::DriverCapabilities {
        let (opengl_major, opengl_minor) = self.get_opengl_version();

        super::DriverCapabilities {
            direct_rendering: true,
            hardware_acceleration: matches!(
                self.generation,
                NouveauGeneration::NV50
                    | NouveauGeneration::NVC0
                    | NouveauGeneration::NVE0
                    | NouveauGeneration::NV110
                    | NouveauGeneration::NV130
            ),
            compute_shaders: self.supports_compute(),
            video_decode: self.supports_video_decode(),
            video_encode: self.supports_video_encode(),
            vulkan_support: self.get_vulkan_support(),
            opengl_version: (opengl_major, opengl_minor),
            glsl_version: match self.generation {
                NouveauGeneration::NV04 | NouveauGeneration::NV10 => 110,
                NouveauGeneration::NV20 | NouveauGeneration::NV30 => 120,
                NouveauGeneration::NV40 => 130,
                NouveauGeneration::NV50 => 330,
                NouveauGeneration::NVC0 => 410,
                NouveauGeneration::NVE0 => 430,
                NouveauGeneration::NV110 => 450,
                NouveauGeneration::NV130 | NouveauGeneration::NV140 | NouveauGeneration::NV170 => {
                    460
                }
            },
            opencl_support: matches!(
                self.generation,
                NouveauGeneration::NVC0
                    | NouveauGeneration::NVE0
                    | NouveauGeneration::NV110
                    | NouveauGeneration::NV130
            ),
            ray_tracing: false, // Nouveau doesn't support RT acceleration yet
        }
    }
}

/// Initialize Nouveau driver for a specific NVIDIA GPU
pub fn initialize_nouveau_driver(device_id: u16) -> Result<NouveauContext, &'static str> {
    let context = NouveauContext::new(device_id);

    // Check if the device is supported
    if matches!(
        context.generation,
        NouveauGeneration::NV04 | NouveauGeneration::NV10
    ) {
        return Err("Legacy NVIDIA GPU not fully supported");
    }

    // Validate firmware availability for newer generations
    if matches!(
        context.generation,
        NouveauGeneration::NV140 | NouveauGeneration::NV170
    ) {
        // Note: Nouveau support for Turing+ is limited due to signed firmware
        return Err("Turing/Ampere GPUs require signed firmware (limited Nouveau support)");
    }

    Ok(context)
}

/// Get Nouveau driver information
pub fn get_nouveau_info() -> super::OpensourceDriver {
    super::OpensourceDriver {
        driver_type: super::DriverType::Nouveau,
        name: "Nouveau".to_string(),
        version: "1.0.17".to_string(),
        supported_devices: vec![], // Would be populated from main database
        capabilities: super::DriverCapabilities::MODERN,
        mesa_driver: Some("nouveau".to_string()),
        kernel_module: "nouveau".to_string(),
        user_space_driver: "libnouveau".to_string(),
        priority: 70,
    }
}
