//! Mesa3D Compatibility Layer for RustOS
//!
//! This module provides compatibility interfaces for Mesa3D drivers
//! and OpenGL/Vulkan implementations on RustOS.

use alloc::vec::Vec;
use alloc::vec;
use alloc::string::{String, ToString};
use alloc::collections::BTreeMap;
use alloc::format;

/// Mesa driver types
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MesaDriverType {
    Gallium,     // Modern unified driver architecture
    Classic,     // Legacy DRI drivers
    Software,    // Software rasterizer (swrast)
}

/// Mesa pipe driver information
#[derive(Debug, Clone)]
pub struct PipeDriver {
    pub name: String,
    pub vendor: String,
    pub description: String,
    pub driver_type: MesaDriverType,
    pub opengl_version: (u8, u8, u8),
    pub glsl_version: u16,
    pub opengl_es_version: (u8, u8),
    pub glsl_es_version: u16,
    pub vulkan_version: Option<(u8, u8, u8)>,
    pub supported_extensions: Vec<String>,
    pub max_texture_size: u32,
    pub max_renderbuffer_size: u32,
    pub max_viewport_size: (u32, u32),
    pub max_texture_units: u32,
    pub max_vertex_attribs: u32,
    pub max_uniform_locations: u32,
}

/// Mesa screen information
#[derive(Debug, Clone)]
pub struct MesaScreen {
    pub name: String,
    pub vendor: String,
    pub device_uuid: [u8; 16],
    pub driver_uuid: [u8; 16],
    pub memory_size: u64,
    pub unified_memory: bool,
    pub supports_shader_cache: bool,
    pub supports_disk_cache: bool,
    pub compute_units: u32,
    pub timestamp_frequency: u64,
}

/// Mesa context capabilities
#[derive(Debug, Clone)]
pub struct MesaContextCaps {
    pub max_texture_image_units: u32,
    pub max_texture_coord_units: u32,
    pub max_vertex_texture_units: u32,
    pub max_combined_texture_units: u32,
    pub max_geometry_texture_units: u32,
    pub max_tess_ctrl_texture_units: u32,
    pub max_tess_eval_texture_units: u32,
    pub max_compute_texture_units: u32,
    pub max_texture_buffer_size: u32,
    pub max_texture_array_layers: u32,
    pub max_texture_cube_levels: u32,
    pub max_texture_3d_levels: u32,
    pub max_texture_lod_bias: f32,
    pub max_vertex_attrib_stride: u32,
    pub max_vertex_attrib_relative_offset: u32,
    pub max_vertex_attrib_bindings: u32,
    pub max_elements_vertices: u32,
    pub max_elements_indices: u32,
    pub min_map_buffer_alignment: u32,
    pub max_viewports: u32,
    pub viewport_subpixel_bits: u32,
    pub max_geometry_output_vertices: u32,
    pub max_geometry_total_output_components: u32,
    pub max_tess_gen_level: u32,
    pub max_patch_vertices: u32,
    pub max_tess_ctrl_total_output_components: u32,
    pub max_tess_eval_output_components: u32,
    pub max_compute_work_group_count: [u32; 3],
    pub max_compute_work_group_size: [u32; 3],
    pub max_compute_work_group_invocations: u32,
    pub max_compute_shared_memory_size: u32,
}

/// Mesa format support information
#[derive(Debug, Clone)]
pub struct MesaFormatSupport {
    pub format: u32, // GL/Vulkan format enum
    pub vertex_buffer: bool,
    pub texture: bool,
    pub color_attachment: bool,
    pub depth_stencil_attachment: bool,
    pub blendable: bool,
    pub multisample: bool,
    pub storage_image: bool,
    pub sampled_image: bool,
}

/// Mesa compatibility layer
pub struct MesaCompatLayer {
    pub pipe_drivers: BTreeMap<String, PipeDriver>,
    pub screens: BTreeMap<u32, MesaScreen>, // GPU ID -> Screen
    pub context_capabilities: BTreeMap<u32, MesaContextCaps>,
    pub format_support: Vec<MesaFormatSupport>,
    pub shader_cache_enabled: bool,
    pub disk_cache_enabled: bool,
    pub debug_output_enabled: bool,
}

impl MesaCompatLayer {
    pub fn new() -> Self {
        let mut layer = Self {
            pipe_drivers: BTreeMap::new(),
            screens: BTreeMap::new(),
            context_capabilities: BTreeMap::new(),
            format_support: Vec::new(),
            shader_cache_enabled: true,
            disk_cache_enabled: true,
            debug_output_enabled: false,
        };

        layer.initialize_pipe_drivers();
        layer.initialize_format_support();
        layer
    }

    /// Initialize Mesa pipe drivers
    fn initialize_pipe_drivers(&mut self) {
        // RadeonSI driver (AMD)
        let radeonsi_driver = PipeDriver {
            name: "radeonsi".to_string(),
            vendor: "AMD".to_string(),
            description: "AMD Radeon Graphics (radeonsi)".to_string(),
            driver_type: MesaDriverType::Gallium,
            opengl_version: (4, 6, 0),
            glsl_version: 460,
            opengl_es_version: (3, 2),
            glsl_es_version: 320,
            vulkan_version: Some((1, 3, 0)),
            supported_extensions: vec![
                "GL_ARB_gpu_shader5".to_string(),
                "GL_ARB_compute_shader".to_string(),
                "GL_ARB_tessellation_shader".to_string(),
                "GL_ARB_shader_image_load_store".to_string(),
                "GL_ARB_shader_storage_buffer_object".to_string(),
                "GL_ARB_indirect_parameters".to_string(),
                "GL_ARB_gpu_shader_fp64".to_string(),
                "GL_ARB_vertex_attrib_64bit".to_string(),
                "GL_KHR_debug".to_string(),
                "GL_KHR_robustness".to_string(),
            ],
            max_texture_size: 16384,
            max_renderbuffer_size: 16384,
            max_viewport_size: (16384, 16384),
            max_texture_units: 32,
            max_vertex_attribs: 32,
            max_uniform_locations: 4096,
        };

        // Iris driver (Intel)
        let iris_driver = PipeDriver {
            name: "iris".to_string(),
            vendor: "Intel".to_string(),
            description: "Intel Graphics (iris)".to_string(),
            driver_type: MesaDriverType::Gallium,
            opengl_version: (4, 6, 0),
            glsl_version: 460,
            opengl_es_version: (3, 2),
            glsl_es_version: 320,
            vulkan_version: Some((1, 3, 0)),
            supported_extensions: vec![
                "GL_ARB_compute_shader".to_string(),
                "GL_ARB_tessellation_shader".to_string(),
                "GL_ARB_shader_image_load_store".to_string(),
                "GL_ARB_shader_storage_buffer_object".to_string(),
                "GL_ARB_indirect_parameters".to_string(),
                "GL_KHR_debug".to_string(),
                "GL_KHR_robustness".to_string(),
                "GL_INTEL_performance_query".to_string(),
            ],
            max_texture_size: 16384,
            max_renderbuffer_size: 16384,
            max_viewport_size: (16384, 16384),
            max_texture_units: 32,
            max_vertex_attribs: 32,
            max_uniform_locations: 4096,
        };

        // Nouveau driver (NVIDIA)
        let nouveau_driver = PipeDriver {
            name: "nouveau".to_string(),
            vendor: "NVIDIA".to_string(),
            description: "NVIDIA Graphics (nouveau)".to_string(),
            driver_type: MesaDriverType::Gallium,
            opengl_version: (4, 3, 0),
            glsl_version: 430,
            opengl_es_version: (3, 1),
            glsl_es_version: 310,
            vulkan_version: Some((1, 0, 0)),
            supported_extensions: vec![
                "GL_ARB_compute_shader".to_string(),
                "GL_ARB_tessellation_shader".to_string(),
                "GL_ARB_shader_image_load_store".to_string(),
                "GL_ARB_shader_storage_buffer_object".to_string(),
                "GL_KHR_debug".to_string(),
                "GL_KHR_robustness".to_string(),
            ],
            max_texture_size: 16384,
            max_renderbuffer_size: 16384,
            max_viewport_size: (16384, 16384),
            max_texture_units: 32,
            max_vertex_attribs: 32,
            max_uniform_locations: 4096,
        };

        // Software rasterizer
        let swrast_driver = PipeDriver {
            name: "swrast".to_string(),
            vendor: "Mesa".to_string(),
            description: "Software Rasterizer".to_string(),
            driver_type: MesaDriverType::Software,
            opengl_version: (4, 5, 0),
            glsl_version: 450,
            opengl_es_version: (3, 2),
            glsl_es_version: 320,
            vulkan_version: None,
            supported_extensions: vec![
                "GL_ARB_compute_shader".to_string(),
                "GL_ARB_tessellation_shader".to_string(),
                "GL_KHR_debug".to_string(),
            ],
            max_texture_size: 16384,
            max_renderbuffer_size: 16384,
            max_viewport_size: (16384, 16384),
            max_texture_units: 16,
            max_vertex_attribs: 16,
            max_uniform_locations: 1024,
        };

        self.pipe_drivers.insert("radeonsi".to_string(), radeonsi_driver);
        self.pipe_drivers.insert("iris".to_string(), iris_driver);
        self.pipe_drivers.insert("nouveau".to_string(), nouveau_driver);
        self.pipe_drivers.insert("swrast".to_string(), swrast_driver);
    }

    /// Initialize format support database
    fn initialize_format_support(&mut self) {
        // Common texture formats
        let formats = [
            // 8-bit formats
            (0x1903, true, true, false, false, false, false, false, true), // GL_RED
            (0x8229, true, true, false, false, false, false, false, true), // GL_R8
            (0x8227, true, true, false, false, false, false, false, true), // GL_RG
            (0x822B, true, true, false, false, false, false, false, true), // GL_RG8
            (0x1907, true, true, true, false, true, true, false, true),    // GL_RGB
            (0x8051, true, true, true, false, true, true, false, true),    // GL_RGB8
            (0x1908, true, true, true, false, true, true, false, true),    // GL_RGBA
            (0x8058, true, true, true, false, true, true, false, true),    // GL_RGBA8

            // 16-bit formats
            (0x822A, true, true, false, false, false, false, false, true), // GL_R16
            (0x822C, true, true, false, false, false, false, false, true), // GL_RG16
            (0x8054, true, true, true, false, true, true, false, true),    // GL_RGB16
            (0x805B, true, true, true, false, true, true, false, true),    // GL_RGBA16

            // Floating point formats
            (0x822D, true, true, false, false, false, false, true, true),  // GL_R16F
            (0x822F, true, true, false, false, false, false, true, true),  // GL_RG16F
            (0x881B, true, true, true, false, false, true, true, true),    // GL_RGB16F
            (0x881A, true, true, true, false, false, true, true, true),    // GL_RGBA16F
            (0x822E, true, true, false, false, false, false, true, true),  // GL_R32F
            (0x8230, true, true, false, false, false, false, true, true),  // GL_RG32F
            (0x8815, true, true, false, false, false, false, true, true),  // GL_RGB32F
            (0x8814, true, true, true, false, false, false, true, true),   // GL_RGBA32F

            // Compressed formats
            (0x83F0, false, true, false, false, false, false, false, true), // GL_COMPRESSED_RGB_S3TC_DXT1_EXT
            (0x83F1, false, true, false, false, false, false, false, true), // GL_COMPRESSED_RGBA_S3TC_DXT1_EXT
            (0x83F2, false, true, false, false, false, false, false, true), // GL_COMPRESSED_RGBA_S3TC_DXT3_EXT
            (0x83F3, false, true, false, false, false, false, false, true), // GL_COMPRESSED_RGBA_S3TC_DXT5_EXT

            // Depth/stencil formats
            (0x1902, false, true, false, true, false, false, false, true),  // GL_DEPTH_COMPONENT
            (0x81A5, false, true, false, true, false, false, false, true),  // GL_DEPTH_COMPONENT16
            (0x81A6, false, true, false, true, false, false, false, true),  // GL_DEPTH_COMPONENT24
            (0x81A7, false, true, false, true, false, false, false, true),  // GL_DEPTH_COMPONENT32
            (0x88F0, false, true, false, true, false, false, false, true),  // GL_DEPTH32F_STENCIL8
            (0x84F9, false, true, false, true, false, false, false, true),  // GL_DEPTH24_STENCIL8
        ];

        for &(format, vertex_buffer, texture, color_attachment, depth_stencil_attachment,
               blendable, multisample, storage_image, sampled_image) in &formats {
            self.format_support.push(MesaFormatSupport {
                format,
                vertex_buffer,
                texture,
                color_attachment,
                depth_stencil_attachment,
                blendable,
                multisample,
                storage_image,
                sampled_image,
            });
        }
    }

    /// Create a Mesa screen for a GPU
    pub fn create_screen(&mut self, gpu_id: u32, driver_name: &str, gpu_caps: &super::super::GPUCapabilities) -> Result<(), &'static str> {
        let screen = MesaScreen {
            name: gpu_caps.device_name.clone(),
            vendor: match gpu_caps.vendor {
                super::super::GPUVendor::Intel => "Intel".to_string(),
                super::super::GPUVendor::Nvidia => "NVIDIA Corporation".to_string(),
                super::super::GPUVendor::AMD => "AMD".to_string(),
                super::super::GPUVendor::Unknown => "Unknown".to_string(),
            },
            device_uuid: self.generate_device_uuid(gpu_caps.pci_device_id),
            driver_uuid: self.generate_driver_uuid(driver_name),
            memory_size: gpu_caps.memory_size,
            unified_memory: gpu_caps.vendor == super::super::GPUVendor::Intel,
            supports_shader_cache: true,
            supports_disk_cache: true,
            compute_units: gpu_caps.compute_units,
            timestamp_frequency: gpu_caps.boost_clock as u64 * 1_000_000, // Convert MHz to Hz
        };

        // Create context capabilities based on GPU tier
        let context_caps = self.create_context_capabilities(gpu_caps);

        self.screens.insert(gpu_id, screen);
        self.context_capabilities.insert(gpu_id, context_caps);

        Ok(())
    }

    /// Create context capabilities based on GPU capabilities
    fn create_context_capabilities(&self, gpu_caps: &super::super::GPUCapabilities) -> MesaContextCaps {
        let tier_multiplier = match gpu_caps.tier {
            super::super::GPUTier::Entry => 0.5,
            super::super::GPUTier::Budget => 0.7,
            super::super::GPUTier::Mainstream => 0.85,
            super::super::GPUTier::Performance => 1.0,
            super::super::GPUTier::HighEnd => 1.2,
            super::super::GPUTier::Enthusiast => 1.5,
        };

        MesaContextCaps {
            max_texture_image_units: (32.0 * tier_multiplier) as u32,
            max_texture_coord_units: 8,
            max_vertex_texture_units: (32.0 * tier_multiplier) as u32,
            max_combined_texture_units: (192.0 * tier_multiplier) as u32,
            max_geometry_texture_units: (32.0 * tier_multiplier) as u32,
            max_tess_ctrl_texture_units: (32.0 * tier_multiplier) as u32,
            max_tess_eval_texture_units: (32.0 * tier_multiplier) as u32,
            max_compute_texture_units: (32.0 * tier_multiplier) as u32,
            max_texture_buffer_size: (134217728.0 * tier_multiplier) as u32, // 128MB
            max_texture_array_layers: 2048,
            max_texture_cube_levels: 15,
            max_texture_3d_levels: 12,
            max_texture_lod_bias: 16.0,
            max_vertex_attrib_stride: 2048,
            max_vertex_attrib_relative_offset: 2047,
            max_vertex_attrib_bindings: 32,
            max_elements_vertices: 1048576,
            max_elements_indices: 1048576,
            min_map_buffer_alignment: 64,
            max_viewports: 16,
            viewport_subpixel_bits: 8,
            max_geometry_output_vertices: 1024,
            max_geometry_total_output_components: 4096,
            max_tess_gen_level: 64,
            max_patch_vertices: 32,
            max_tess_ctrl_total_output_components: 4096,
            max_tess_eval_output_components: 128,
            max_compute_work_group_count: [65535, 65535, 65535],
            max_compute_work_group_size: [1024, 1024, 64],
            max_compute_work_group_invocations: 1024,
            max_compute_shared_memory_size: (65536.0 * tier_multiplier) as u32,
        }
    }

    /// Generate device UUID
    fn generate_device_uuid(&self, device_id: u16) -> [u8; 16] {
        let mut uuid = [0u8; 16];
        uuid[0..2].copy_from_slice(&device_id.to_le_bytes());
        uuid[2] = 0x52; // 'R'
        uuid[3] = 0x75; // 'u'
        uuid[4] = 0x73; // 's'
        uuid[5] = 0x74; // 't'
        uuid[6] = 0x4F; // 'O'
        uuid[7] = 0x53; // 'S'
        // Rest remain zero
        uuid
    }

    /// Generate driver UUID
    fn generate_driver_uuid(&self, driver_name: &str) -> [u8; 16] {
        let mut uuid = [0u8; 16];
        let name_bytes = driver_name.as_bytes();
        let copy_len = core::cmp::min(name_bytes.len(), 16);
        uuid[..copy_len].copy_from_slice(&name_bytes[..copy_len]);
        uuid
    }

    /// Get pipe driver by name
    pub fn get_pipe_driver(&self, name: &str) -> Option<&PipeDriver> {
        self.pipe_drivers.get(name)
    }

    /// Get screen for GPU
    pub fn get_screen(&self, gpu_id: u32) -> Option<&MesaScreen> {
        self.screens.get(&gpu_id)
    }

    /// Get context capabilities for GPU
    pub fn get_context_caps(&self, gpu_id: u32) -> Option<&MesaContextCaps> {
        self.context_capabilities.get(&gpu_id)
    }

    /// Check format support
    pub fn is_format_supported(&self, format: u32, usage: FormatUsage) -> bool {
        self.format_support.iter().any(|support| {
            support.format == format && match usage {
                FormatUsage::VertexBuffer => support.vertex_buffer,
                FormatUsage::Texture => support.texture,
                FormatUsage::ColorAttachment => support.color_attachment,
                FormatUsage::DepthStencilAttachment => support.depth_stencil_attachment,
                FormatUsage::Blendable => support.blendable,
                FormatUsage::Multisample => support.multisample,
                FormatUsage::StorageImage => support.storage_image,
                FormatUsage::SampledImage => support.sampled_image,
            }
        })
    }

    /// Get OpenGL version string
    pub fn get_gl_version_string(&self, gpu_id: u32) -> String {
        if let Some(screen) = self.get_screen(gpu_id) {
            if let Some(driver) = self.pipe_drivers.values().find(|d| d.vendor == screen.vendor) {
                format!("{}.{}.{} Mesa 23.2.0",
                    driver.opengl_version.0,
                    driver.opengl_version.1,
                    driver.opengl_version.2)
            } else {
                "4.6.0 Mesa 23.2.0".to_string()
            }
        } else {
            "4.6.0 Mesa 23.2.0".to_string()
        }
    }

    /// Get OpenGL ES version string
    pub fn get_gles_version_string(&self, gpu_id: u32) -> String {
        if let Some(screen) = self.get_screen(gpu_id) {
            if let Some(driver) = self.pipe_drivers.values().find(|d| d.vendor == screen.vendor) {
                format!("OpenGL ES {}.{} Mesa 23.2.0",
                    driver.opengl_es_version.0,
                    driver.opengl_es_version.1)
            } else {
                "OpenGL ES 3.2 Mesa 23.2.0".to_string()
            }
        } else {
            "OpenGL ES 3.2 Mesa 23.2.0".to_string()
        }
    }

    /// Get GLSL version string
    pub fn get_glsl_version_string(&self, gpu_id: u32) -> String {
        if let Some(screen) = self.get_screen(gpu_id) {
            if let Some(driver) = self.pipe_drivers.values().find(|d| d.vendor == screen.vendor) {
                format!("{} core", driver.glsl_version)
            } else {
                "460 core".to_string()
            }
        } else {
            "460 core".to_string()
        }
    }

    /// Get vendor string
    pub fn get_vendor_string(&self, gpu_id: u32) -> String {
        if let Some(screen) = self.get_screen(gpu_id) {
            screen.vendor.clone()
        } else {
            "Unknown".to_string()
        }
    }

    /// Get renderer string
    pub fn get_renderer_string(&self, gpu_id: u32) -> String {
        if let Some(screen) = self.get_screen(gpu_id) {
            screen.name.clone()
        } else {
            "Unknown GPU".to_string()
        }
    }

    /// Get supported extensions
    pub fn get_extensions(&self, gpu_id: u32) -> Vec<String> {
        if let Some(screen) = self.get_screen(gpu_id) {
            if let Some(driver) = self.pipe_drivers.values().find(|d| d.vendor == screen.vendor) {
                driver.supported_extensions.clone()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        }
    }
}

/// Format usage types
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FormatUsage {
    VertexBuffer,
    Texture,
    ColorAttachment,
    DepthStencilAttachment,
    Blendable,
    Multisample,
    StorageImage,
    SampledImage,
}

// Global Mesa compatibility layer
static mut MESA_COMPAT: Option<MesaCompatLayer> = None;

/// Initialize Mesa compatibility layer
pub fn init_mesa_compat() -> Result<(), &'static str> {
    unsafe {
        if (&*core::ptr::addr_of!(MESA_COMPAT)).is_none() {
            MESA_COMPAT = Some(MesaCompatLayer::new());
        }
    }
    Ok(())
}

/// Get Mesa compatibility layer instance
pub fn get_mesa_compat() -> Option<&'static mut MesaCompatLayer> {
    unsafe { (&mut *core::ptr::addr_of_mut!(MESA_COMPAT)).as_mut() }
}

/// Create Mesa screen for GPU
pub fn create_mesa_screen(gpu_id: u32, driver_name: &str, gpu_caps: &super::super::GPUCapabilities) -> Result<(), &'static str> {
    if let Some(mesa) = get_mesa_compat() {
        mesa.create_screen(gpu_id, driver_name, gpu_caps)
    } else {
        Err("Mesa compatibility layer not initialized")
    }
}

/// Get OpenGL information for GPU
pub fn get_opengl_info(gpu_id: u32) -> Option<OpenGLInfo> {
    if let Some(mesa) = get_mesa_compat() {
        Some(OpenGLInfo {
            version: mesa.get_gl_version_string(gpu_id),
            vendor: mesa.get_vendor_string(gpu_id),
            renderer: mesa.get_renderer_string(gpu_id),
            glsl_version: mesa.get_glsl_version_string(gpu_id),
            extensions: mesa.get_extensions(gpu_id),
        })
    } else {
        None
    }
}

/// OpenGL information structure
#[derive(Debug, Clone)]
pub struct OpenGLInfo {
    pub version: String,
    pub vendor: String,
    pub renderer: String,
    pub glsl_version: String,
    pub extensions: Vec<String>,
}