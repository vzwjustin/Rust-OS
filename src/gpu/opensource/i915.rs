//! Intel i915 Driver Integration for Intel GPUs
//!
//! This module provides integration with the Intel i915 opensource driver
//! for Intel integrated graphics processors.

use alloc::string::ToString;
use alloc::vec;

/// Intel i915 driver context for Intel GPUs
#[derive(Debug)]
pub struct I915Context {
    pub device_id: u16,
    pub generation: IntelGeneration,
    pub platform: IntelPlatform,
    pub execution_units: u32,
    pub base_frequency: u32,
    pub max_frequency: u32,
    pub shared_memory: u64,
    pub has_gt1: bool,
    pub has_gt2: bool,
    pub has_gt3: bool,
    pub has_gt4: bool,
    pub video_engines: VideoEngineSupport,
}

/// Intel GPU generations
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IntelGeneration {
    Gen2,   // i8xx
    Gen3,   // i915, i945
    Gen4,   // i965, G35
    Gen45,  // G45, Q45
    Gen5,   // Ironlake
    Gen6,   // Sandy Bridge
    Gen7,   // Ivy Bridge
    Gen75,  // Haswell
    Gen8,   // Broadwell
    Gen9,   // Skylake, Kaby Lake
    Gen95,  // Coffee Lake, Whiskey Lake
    Gen11,  // Ice Lake
    Gen12,  // Tiger Lake, Rocket Lake
    Gen125, // Alder Lake, DG1
}

/// Intel GPU platforms
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IntelPlatform {
    I830,
    I845,
    I855,
    I865, // Gen2
    I915,
    I945, // Gen3
    I965,
    G35,
    Q35,
    Q33, // Gen4
    G45,
    Q45,
    G41,         // Gen4.5
    Ironlake,    // Gen5
    SandyBridge, // Gen6
    IvyBridge,   // Gen7
    Haswell,
    BayTrail, // Gen7.5
    Broadwell,
    Cherryview, // Gen8
    Skylake,
    Broxton,
    KabyLake,
    GeminiLake, // Gen9
    CoffeeLake,
    WhiskeyLake,
    CometLake, // Gen9.5
    IceLake,
    ElkhartLake,
    JasperLake, // Gen11
    TigerLake,
    RocketLake,
    DG1, // Gen12
    AlderLake,
    DG2, // Gen12.5
}

/// Video engine support for Intel GPUs
#[derive(Debug, Clone)]
pub struct VideoEngineSupport {
    pub has_bsd: bool,   // Bit Stream Decoder
    pub has_blt: bool,   // Blitter engine
    pub has_vebox: bool, // Video Enhancement Box
    pub has_vcs: bool,   // Video Command Streamer
    pub has_vcs2: bool,  // Second Video Command Streamer
    pub has_hevc_decode: bool,
    pub has_hevc_encode: bool,
    pub has_vp9_decode: bool,
    pub has_av1_decode: bool,
}

impl I915Context {
    pub fn new(device_id: u16) -> Self {
        let generation = Self::detect_generation(device_id);
        let platform = Self::detect_platform(device_id, generation);
        let execution_units = Self::get_execution_units(device_id, generation);
        let (base_freq, max_freq) = Self::get_frequencies(device_id, generation);
        let shared_memory = Self::estimate_shared_memory(generation);
        let (has_gt1, has_gt2, has_gt3, has_gt4) = Self::detect_gt_level(device_id);
        let video_engines = Self::get_video_engines(generation, platform);

        Self {
            device_id,
            generation,
            platform,
            execution_units,
            base_frequency: base_freq,
            max_frequency: max_freq,
            shared_memory,
            has_gt1,
            has_gt2,
            has_gt3,
            has_gt4,
            video_engines,
        }
    }

    fn detect_generation(device_id: u16) -> IntelGeneration {
        match device_id {
            // Gen2 (i8xx)
            0x3577 | 0x2562 | 0x3582 => IntelGeneration::Gen2,

            // Gen3 (i915, i945)
            0x2582 | 0x258A | 0x2592 | 0x2772 | 0x27A2 | 0x27AE => IntelGeneration::Gen3,

            // Gen4 (i965)
            0x2972 | 0x2982 | 0x2992 | 0x29A2 | 0x29B2 | 0x29C2 | 0x29D2 => IntelGeneration::Gen4,

            // Gen4.5 (G45)
            0x2E02 | 0x2E12 | 0x2E22 | 0x2E32 | 0x2E42 | 0x2E92 => IntelGeneration::Gen45,

            // Gen5 (Ironlake)
            0x0042 | 0x0046 => IntelGeneration::Gen5,

            // Gen6 (Sandy Bridge)
            0x0102 | 0x0106 | 0x010A | 0x0112 | 0x0116 | 0x0122 | 0x0126 => IntelGeneration::Gen6,

            // Gen7 (Ivy Bridge)
            0x0152 | 0x0156 | 0x015A | 0x0162 | 0x0166 | 0x016A => IntelGeneration::Gen7,

            // Gen7.5 (Haswell)
            0x0402 | 0x0406 | 0x040A | 0x0412 | 0x0416 | 0x041A | 0x041E | 0x0422 | 0x0426
            | 0x042A | 0x042B | 0x042E | 0x0A02 | 0x0A06 | 0x0A0A | 0x0A0B | 0x0A0E | 0x0A12
            | 0x0A16 | 0x0A1A | 0x0A1E | 0x0A22 | 0x0A26 | 0x0A2A | 0x0A2B | 0x0A2E | 0x0D12
            | 0x0D16 | 0x0D1A | 0x0D1B | 0x0D1E | 0x0D22 | 0x0D26 | 0x0D2A | 0x0D2B | 0x0D2E => {
                IntelGeneration::Gen75
            }

            // Gen8 (Broadwell)
            0x1602 | 0x1606 | 0x160A | 0x160B | 0x160D | 0x160E | 0x1612 | 0x1616 | 0x161A
            | 0x161B | 0x161D | 0x161E | 0x1622 | 0x1626 | 0x162A | 0x162B | 0x162D | 0x162E => {
                IntelGeneration::Gen8
            }

            // Gen9 (Skylake, Kaby Lake)
            0x1902 | 0x1906 | 0x190A | 0x190B | 0x190E | 0x1912 | 0x1913 | 0x1915 | 0x1916
            | 0x1917 | 0x191A | 0x191B | 0x191D | 0x191E | 0x1921 | 0x1923 | 0x1926 | 0x1927
            | 0x192A | 0x192B | 0x192D | 0x5902 | 0x5906 | 0x590A | 0x590B | 0x590E | 0x5912
            | 0x5913 | 0x5915 | 0x5916 | 0x5917 | 0x591A | 0x591B | 0x591C | 0x591D | 0x591E
            | 0x5921 | 0x5923 | 0x5926 | 0x5927 => IntelGeneration::Gen9,

            // Gen9.5 (Coffee Lake, Whiskey Lake)
            0x3E90 | 0x3E91 | 0x3E92 | 0x3E93 | 0x3E94 | 0x3E96 | 0x3E98 | 0x3E9A | 0x3E9B
            | 0x3EA0 | 0x3EA5 | 0x3EA6 | 0x3EA7 | 0x3EA8 => IntelGeneration::Gen95,

            // Gen11 (Ice Lake)
            0x8A50 | 0x8A51 | 0x8A52 | 0x8A53 | 0x8A5A | 0x8A5B | 0x8A5C | 0x8A5D => {
                IntelGeneration::Gen11
            }

            // Gen12 (Tiger Lake)
            0x9A40 | 0x9A49 | 0x9A60 | 0x9A68 | 0x9A70 | 0x9A78 => IntelGeneration::Gen12,

            _ => IntelGeneration::Gen9, // Default fallback
        }
    }

    fn detect_platform(device_id: u16, generation: IntelGeneration) -> IntelPlatform {
        match generation {
            IntelGeneration::Gen2 => match device_id {
                0x3577 => IntelPlatform::I830,
                0x2562 => IntelPlatform::I845,
                0x3582 => IntelPlatform::I855,
                _ => IntelPlatform::I865,
            },
            IntelGeneration::Gen3 => match device_id {
                0x2582 | 0x258A => IntelPlatform::I915,
                _ => IntelPlatform::I945,
            },
            IntelGeneration::Gen4 => IntelPlatform::I965,
            IntelGeneration::Gen45 => IntelPlatform::G45,
            IntelGeneration::Gen5 => IntelPlatform::Ironlake,
            IntelGeneration::Gen6 => IntelPlatform::SandyBridge,
            IntelGeneration::Gen7 => IntelPlatform::IvyBridge,
            IntelGeneration::Gen75 => IntelPlatform::Haswell,
            IntelGeneration::Gen8 => IntelPlatform::Broadwell,
            IntelGeneration::Gen9 => match device_id {
                0x5900..=0x5927 => IntelPlatform::KabyLake,
                _ => IntelPlatform::Skylake,
            },
            IntelGeneration::Gen95 => IntelPlatform::CoffeeLake,
            IntelGeneration::Gen11 => IntelPlatform::IceLake,
            IntelGeneration::Gen12 => IntelPlatform::TigerLake,
            IntelGeneration::Gen125 => IntelPlatform::AlderLake,
        }
    }

    fn get_execution_units(device_id: u16, generation: IntelGeneration) -> u32 {
        match generation {
            IntelGeneration::Gen2
            | IntelGeneration::Gen3
            | IntelGeneration::Gen4
            | IntelGeneration::Gen45 => 0,
            IntelGeneration::Gen5 => 12,
            IntelGeneration::Gen6 => match device_id {
                0x0102 | 0x0106 | 0x010A => 6, // HD 2000
                0x0112 | 0x0116 => 12,         // HD 3000
                0x0122 | 0x0126 => 12,         // HD 3000
                _ => 6,
            },
            IntelGeneration::Gen7 => match device_id {
                0x0152 | 0x0156 | 0x015A => 16, // HD 2500
                0x0162 | 0x0166 | 0x016A => 16, // HD 4000
                _ => 16,
            },
            IntelGeneration::Gen75 => match device_id {
                0x0402 | 0x0406 | 0x040A => 20,          // HD Graphics
                0x0412 | 0x0416 | 0x041A | 0x041E => 20, // HD 4600
                0x0422 | 0x0426 | 0x042A | 0x042B => 40, // Iris 5100
                0x0A22 | 0x0A26 | 0x0A2A | 0x0A2B => 40, // Iris 5100
                0x0D22 | 0x0D26 | 0x0D2A | 0x0D2B => 40, // Iris Pro 5200
                _ => 20,
            },
            IntelGeneration::Gen8 => match device_id {
                0x1602 | 0x1606 | 0x160A | 0x160B => 12, // HD Graphics
                0x1612 | 0x1616 | 0x161A | 0x161B => 24, // HD 5600/5500
                0x1622 | 0x1626 | 0x162A | 0x162B => 48, // Iris Pro 6200
                _ => 24,
            },
            IntelGeneration::Gen9 => match device_id {
                0x1902 | 0x1906 | 0x190A | 0x190B => 12,          // HD 510
                0x1912 | 0x1913 | 0x1915 | 0x1916 | 0x1917 => 24, // HD 530/520
                0x1926 | 0x1927 => 48,                            // Iris 540/550
                0x192A | 0x192B | 0x192D => 72,                   // Iris Pro 580/555
                0x5902 | 0x5906 | 0x590A | 0x590B => 12,          // HD 610
                0x5912 | 0x5913 | 0x5915 | 0x5916 | 0x5917 => 24, // HD 630/620
                0x5926 | 0x5927 => 48,                            // Iris Plus 640/650
                _ => 24,
            },
            IntelGeneration::Gen95 => match device_id {
                0x3E90 | 0x3E93 => 12,                   // UHD 610
                0x3E91 | 0x3E92 | 0x3E98 | 0x3E9B => 24, // UHD 630
                0x3EA5 | 0x3EA6 | 0x3EA7 | 0x3EA8 => 48, // Iris Plus 655/645
                _ => 24,
            },
            IntelGeneration::Gen11 => match device_id {
                0x8A50 | 0x8A5A => 32,                   // Iris Plus G1
                0x8A51 | 0x8A5B => 48,                   // Iris Plus G4
                0x8A52 | 0x8A53 | 0x8A5C | 0x8A5D => 64, // Iris Plus G7
                _ => 64,
            },
            IntelGeneration::Gen12 => match device_id {
                0x9A60 | 0x9A68 | 0x9A70 | 0x9A78 => 32, // UHD Graphics
                0x9A40 | 0x9A49 => 96,                   // Iris Xe G7
                _ => 96,
            },
            IntelGeneration::Gen125 => 96, // Xe Graphics
        }
    }

    fn get_frequencies(device_id: u16, generation: IntelGeneration) -> (u32, u32) {
        // Returns (base_freq_mhz, max_freq_mhz)
        match generation {
            IntelGeneration::Gen2 | IntelGeneration::Gen3 => (100, 200),
            IntelGeneration::Gen4 | IntelGeneration::Gen45 => (200, 400),
            IntelGeneration::Gen5 => (250, 500),
            IntelGeneration::Gen6 => (350, 650),
            IntelGeneration::Gen7 => (350, 850),
            IntelGeneration::Gen75 => (200, 1000),
            IntelGeneration::Gen8 => (100, 1000),
            IntelGeneration::Gen9 => match device_id {
                0x1926 | 0x1927 | 0x192A | 0x192B | 0x192D => (300, 1100), // Iris
                0x5926 | 0x5927 => (300, 1100),                            // Iris Plus
                _ => (300, 950),                                           // HD Graphics
            },
            IntelGeneration::Gen95 => (350, 1150),
            IntelGeneration::Gen11 => (300, 1100),
            IntelGeneration::Gen12 => (400, 1350),
            IntelGeneration::Gen125 => (400, 1400),
        }
    }

    fn estimate_shared_memory(generation: IntelGeneration) -> u64 {
        let mb = match generation {
            IntelGeneration::Gen2 | IntelGeneration::Gen3 => 64,
            IntelGeneration::Gen4 | IntelGeneration::Gen45 => 128,
            IntelGeneration::Gen5 | IntelGeneration::Gen6 => 256,
            IntelGeneration::Gen7 | IntelGeneration::Gen75 => 512,
            IntelGeneration::Gen8 | IntelGeneration::Gen9 => 1024,
            IntelGeneration::Gen95 | IntelGeneration::Gen11 => 2048,
            IntelGeneration::Gen12 | IntelGeneration::Gen125 => 4096,
        };
        mb * 1024 * 1024
    }

    fn detect_gt_level(device_id: u16) -> (bool, bool, bool, bool) {
        // Returns (gt1, gt2, gt3, gt4)
        match device_id {
            // GT1 devices
            0x0102 | 0x0106 | 0x010A | 0x0152 | 0x0156 | 0x015A | 0x0402 | 0x0406 | 0x040A
            | 0x0A02 | 0x0A06 | 0x0A0A | 0x0A0B | 0x0A0E | 0x1602 | 0x1606 | 0x160A | 0x160B
            | 0x160D | 0x160E | 0x1902 | 0x1906 | 0x190A | 0x190B | 0x190E | 0x5902 | 0x5906
            | 0x590A | 0x590B | 0x590E | 0x3E90 | 0x3E93 | 0x9A60 | 0x9A68 | 0x9A70 | 0x9A78 => {
                (true, false, false, false)
            }

            // GT2 devices
            0x0112 | 0x0116 | 0x0122 | 0x0126 | 0x0162 | 0x0166 | 0x016A | 0x0412 | 0x0416
            | 0x041A | 0x041E | 0x0A12 | 0x0A16 | 0x0A1A | 0x0A1E | 0x0D12 | 0x0D16 | 0x0D1A
            | 0x0D1B | 0x0D1E | 0x1612 | 0x1616 | 0x161A | 0x161B | 0x161D | 0x161E | 0x1912
            | 0x1913 | 0x1915 | 0x1916 | 0x1917 | 0x191A | 0x191B | 0x191D | 0x191E | 0x1921
            | 0x1923 | 0x5912 | 0x5913 | 0x5915 | 0x5916 | 0x5917 | 0x591A | 0x591B | 0x591C
            | 0x591D | 0x591E | 0x5921 | 0x5923 | 0x3E91 | 0x3E92 | 0x3E94 | 0x3E96 | 0x3E98
            | 0x3E9A | 0x3E9B | 0x8A50 | 0x8A51 | 0x8A5A | 0x8A5B | 0x9A40 | 0x9A49 => {
                (false, true, false, false)
            }

            // GT3 devices
            0x0422 | 0x0426 | 0x042A | 0x042B | 0x042E | 0x0A22 | 0x0A26 | 0x0A2A | 0x0A2B
            | 0x0A2E | 0x1622 | 0x1626 | 0x162A | 0x162B | 0x162D | 0x162E | 0x1926 | 0x1927
            | 0x192A | 0x192B | 0x192D | 0x5926 | 0x5927 | 0x3EA5 | 0x3EA6 | 0x3EA7 | 0x3EA8
            | 0x8A52 | 0x8A53 | 0x8A5C | 0x8A5D => (false, false, true, false),

            // GT4 devices
            0x0D22 | 0x0D26 | 0x0D2A | 0x0D2B | 0x0D2E => (false, false, false, true),

            _ => (false, true, false, false), // Default to GT2
        }
    }

    fn get_video_engines(
        generation: IntelGeneration,
        _platform: IntelPlatform,
    ) -> VideoEngineSupport {
        VideoEngineSupport {
            has_bsd: matches!(
                generation,
                IntelGeneration::Gen5
                    | IntelGeneration::Gen6
                    | IntelGeneration::Gen7
                    | IntelGeneration::Gen75
                    | IntelGeneration::Gen8
                    | IntelGeneration::Gen9
                    | IntelGeneration::Gen95
                    | IntelGeneration::Gen11
                    | IntelGeneration::Gen12
                    | IntelGeneration::Gen125
            ),
            has_blt: matches!(
                generation,
                IntelGeneration::Gen6
                    | IntelGeneration::Gen7
                    | IntelGeneration::Gen75
                    | IntelGeneration::Gen8
                    | IntelGeneration::Gen9
                    | IntelGeneration::Gen95
                    | IntelGeneration::Gen11
                    | IntelGeneration::Gen12
                    | IntelGeneration::Gen125
            ),
            has_vebox: matches!(
                generation,
                IntelGeneration::Gen75
                    | IntelGeneration::Gen8
                    | IntelGeneration::Gen9
                    | IntelGeneration::Gen95
                    | IntelGeneration::Gen11
                    | IntelGeneration::Gen12
                    | IntelGeneration::Gen125
            ),
            has_vcs: matches!(
                generation,
                IntelGeneration::Gen8
                    | IntelGeneration::Gen9
                    | IntelGeneration::Gen95
                    | IntelGeneration::Gen11
                    | IntelGeneration::Gen12
                    | IntelGeneration::Gen125
            ),
            has_vcs2: matches!(
                generation,
                IntelGeneration::Gen11 | IntelGeneration::Gen12 | IntelGeneration::Gen125
            ),
            has_hevc_decode: matches!(
                generation,
                IntelGeneration::Gen9
                    | IntelGeneration::Gen95
                    | IntelGeneration::Gen11
                    | IntelGeneration::Gen12
                    | IntelGeneration::Gen125
            ),
            has_hevc_encode: matches!(
                generation,
                IntelGeneration::Gen95
                    | IntelGeneration::Gen11
                    | IntelGeneration::Gen12
                    | IntelGeneration::Gen125
            ),
            has_vp9_decode: matches!(
                generation,
                IntelGeneration::Gen11 | IntelGeneration::Gen12 | IntelGeneration::Gen125
            ),
            has_av1_decode: matches!(generation, IntelGeneration::Gen12 | IntelGeneration::Gen125),
        }
    }

    pub fn supports_hardware_acceleration(&self) -> bool {
        matches!(
            self.generation,
            IntelGeneration::Gen6
                | IntelGeneration::Gen7
                | IntelGeneration::Gen75
                | IntelGeneration::Gen8
                | IntelGeneration::Gen9
                | IntelGeneration::Gen95
                | IntelGeneration::Gen11
                | IntelGeneration::Gen12
                | IntelGeneration::Gen125
        )
    }

    pub fn supports_compute(&self) -> bool {
        matches!(
            self.generation,
            IntelGeneration::Gen75
                | IntelGeneration::Gen8
                | IntelGeneration::Gen9
                | IntelGeneration::Gen95
                | IntelGeneration::Gen11
                | IntelGeneration::Gen12
                | IntelGeneration::Gen125
        )
    }

    pub fn supports_video_decode(&self) -> bool {
        self.video_engines.has_bsd || self.video_engines.has_vcs
    }

    pub fn supports_video_encode(&self) -> bool {
        self.video_engines.has_vcs
    }

    pub fn get_opengl_version(&self) -> (u8, u8) {
        match self.generation {
            IntelGeneration::Gen2 => (1, 3),
            IntelGeneration::Gen3 => (1, 4),
            IntelGeneration::Gen4 => (2, 0),
            IntelGeneration::Gen45 => (2, 1),
            IntelGeneration::Gen5 | IntelGeneration::Gen6 => (3, 0),
            IntelGeneration::Gen7 => (3, 1),
            IntelGeneration::Gen75 => (4, 0),
            IntelGeneration::Gen8 => (4, 3),
            IntelGeneration::Gen9 | IntelGeneration::Gen95 => (4, 5),
            IntelGeneration::Gen11 | IntelGeneration::Gen12 | IntelGeneration::Gen125 => (4, 6),
        }
    }

    pub fn get_vulkan_support(&self) -> bool {
        matches!(
            self.generation,
            IntelGeneration::Gen75
                | IntelGeneration::Gen8
                | IntelGeneration::Gen9
                | IntelGeneration::Gen95
                | IntelGeneration::Gen11
                | IntelGeneration::Gen12
                | IntelGeneration::Gen125
        )
    }

    pub fn get_opencl_support(&self) -> bool {
        matches!(
            self.generation,
            IntelGeneration::Gen75
                | IntelGeneration::Gen8
                | IntelGeneration::Gen9
                | IntelGeneration::Gen95
                | IntelGeneration::Gen11
                | IntelGeneration::Gen12
                | IntelGeneration::Gen125
        )
    }

    pub fn get_driver_features(&self) -> super::DriverCapabilities {
        let (opengl_major, opengl_minor) = self.get_opengl_version();

        super::DriverCapabilities {
            direct_rendering: true,
            hardware_acceleration: self.supports_hardware_acceleration(),
            compute_shaders: self.supports_compute(),
            video_decode: self.supports_video_decode(),
            video_encode: self.supports_video_encode(),
            vulkan_support: self.get_vulkan_support(),
            opengl_version: (opengl_major, opengl_minor),
            glsl_version: match self.generation {
                IntelGeneration::Gen2 | IntelGeneration::Gen3 => 110,
                IntelGeneration::Gen4 | IntelGeneration::Gen45 => 120,
                IntelGeneration::Gen5 | IntelGeneration::Gen6 => 130,
                IntelGeneration::Gen7 => 140,
                IntelGeneration::Gen75 => 400,
                IntelGeneration::Gen8 => 430,
                IntelGeneration::Gen9 | IntelGeneration::Gen95 => 450,
                IntelGeneration::Gen11 | IntelGeneration::Gen12 | IntelGeneration::Gen125 => 460,
            },
            opencl_support: self.get_opencl_support(),
            ray_tracing: false, // Intel integrated graphics don't support hardware ray tracing
        }
    }
}

/// Initialize Intel i915 driver for a specific Intel GPU
pub fn initialize_i915_driver(device_id: u16) -> Result<I915Context, &'static str> {
    let context = I915Context::new(device_id);

    // Check if the device is supported
    if matches!(
        context.generation,
        IntelGeneration::Gen2 | IntelGeneration::Gen3
    ) {
        return Err("Legacy Intel GPU not fully supported");
    }

    Ok(context)
}

/// Get Intel i915 driver information
pub fn get_i915_info() -> super::OpensourceDriver {
    super::OpensourceDriver {
        driver_type: super::DriverType::I915,
        name: "Intel i915".to_string(),
        version: "1.6.0".to_string(),
        supported_devices: vec![
            // Intel integrated GPUs (vendor 0x8086)
            (0x8086, 0x0166), // IvyBridge GT2
            (0x8086, 0x0416), // Haswell GT2
            (0x8086, 0x1616), // Broadwell GT2 (HD Graphics 5500)
            (0x8086, 0x1916), // Skylake GT2 (HD Graphics 520)
            (0x8086, 0x5916), // KabyLake GT2 (HD Graphics 620)
            (0x8086, 0x5A85), // Apollolake (HD Graphics 500)
            (0x8086, 0x3E9B), // CoffeeLake GT2 (UHD Graphics 630)
        ],
        capabilities: super::DriverCapabilities::MODERN,
        mesa_driver: Some("iris".to_string()),
        kernel_module: "i915".to_string(),
        user_space_driver: "libintel".to_string(),
        priority: 90,
    }
}
