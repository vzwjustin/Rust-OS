//! # RustOS VBE/VESA BIOS Extensions Graphics Driver
//!
//! This driver provides support for VBE (VESA BIOS Extensions) graphics modes,
//! enabling high-resolution framebuffer graphics on x86 systems.

use crate::graphics::framebuffer::{FramebufferInfo, PixelFormat};
use core::mem;

/// VBE function numbers for BIOS interrupt 0x10
#[repr(u16)]
#[derive(Debug, Clone, Copy)]
pub enum VbeFunction {
    /// Get VBE Controller Information
    GetControllerInfo = 0x4F00,
    /// Get VBE Mode Information
    GetModeInfo = 0x4F01,
    /// Set VBE Mode
    SetMode = 0x4F02,
    /// Get Current VBE Mode
    GetCurrentMode = 0x4F03,
    /// Save/Restore VBE State
    SaveRestoreState = 0x4F04,
    /// VBE Display Window Control
    WindowControl = 0x4F05,
    /// Set/Get Logical Scan Line Length
    SetScanLineLength = 0x4F06,
    /// Set/Get Display Start
    SetDisplayStart = 0x4F07,
    /// Set/Get DAC Palette Format
    SetDacFormat = 0x4F08,
    /// Set/Get Palette Data
    SetPaletteData = 0x4F09,
    /// Get VBE Protected Mode Interface
    GetPModeInterface = 0x4F0A,
}

/// VBE return status codes
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VbeStatus {
    Success = 0x004F,
    Failed = 0x014F,
    NotSupported = 0x024F,
    InvalidInCurrentMode = 0x034F,
}

impl core::fmt::Display for VbeStatus {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            VbeStatus::Success => write!(f, "Success"),
            VbeStatus::Failed => write!(f, "Failed"),
            VbeStatus::NotSupported => write!(f, "Not Supported"),
            VbeStatus::InvalidInCurrentMode => write!(f, "Invalid In Current Mode"),
        }
    }
}

impl VbeStatus {
    pub fn from_ax(ax: u16) -> Self {
        match ax {
            0x004F => VbeStatus::Success,
            0x014F => VbeStatus::Failed,
            0x024F => VbeStatus::NotSupported,
            0x034F => VbeStatus::InvalidInCurrentMode,
            _ => VbeStatus::Failed,
        }
    }

    pub fn is_success(&self) -> bool {
        matches!(self, VbeStatus::Success)
    }
}

/// VBE Controller Information Block (VbeInfoBlock)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct VbeInfoBlock {
    pub signature: [u8; 4],        // "VESA" signature
    pub version: u16,              // VBE version (BCD format)
    pub oem_string_ptr: u32,       // Pointer to OEM string
    pub capabilities: u32,         // Capabilities bitfield
    pub video_mode_ptr: u32,       // Pointer to video mode list
    pub total_memory: u16,         // Total video memory in 64KB blocks
    pub oem_software_rev: u16,     // OEM software revision
    pub oem_vendor_name_ptr: u32,  // Pointer to vendor name
    pub oem_product_name_ptr: u32, // Pointer to product name
    pub oem_product_rev_ptr: u32,  // Pointer to product revision
    pub reserved: [u8; 222],       // Reserved for VBE implementation
    pub oem_data: [u8; 256],       // OEM-specific data
}

impl Default for VbeInfoBlock {
    fn default() -> Self {
        Self {
            signature: *b"VBE2", // Request VBE 2.0+ information
            version: 0,
            oem_string_ptr: 0,
            capabilities: 0,
            video_mode_ptr: 0,
            total_memory: 0,
            oem_software_rev: 0,
            oem_vendor_name_ptr: 0,
            oem_product_name_ptr: 0,
            oem_product_rev_ptr: 0,
            reserved: [0; 222],
            oem_data: [0; 256],
        }
    }
}

/// VBE Mode Information Block (ModeInfoBlock)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct ModeInfoBlock {
    // Mandatory information for all VBE revisions
    pub mode_attributes: u16,     // Mode attributes
    pub win_a_attributes: u8,     // Window A attributes
    pub win_b_attributes: u8,     // Window B attributes
    pub win_granularity: u16,     // Window granularity
    pub win_size: u16,            // Window size
    pub win_a_segment: u16,       // Window A start segment
    pub win_b_segment: u16,       // Window B start segment
    pub win_func_ptr: u32,        // Pointer to window function
    pub bytes_per_scan_line: u16, // Bytes per scan line

    // Mandatory information for VBE 1.2 and above
    pub x_resolution: u16,         // Horizontal resolution in pixels
    pub y_resolution: u16,         // Vertical resolution in pixels
    pub x_char_size: u8,           // Character cell width in pixels
    pub y_char_size: u8,           // Character cell height in pixels
    pub number_of_planes: u8,      // Number of memory planes
    pub bits_per_pixel: u8,        // Bits per pixel
    pub number_of_banks: u8,       // Number of banks
    pub memory_model: u8,          // Memory model type
    pub bank_size: u8,             // Bank size in KB
    pub number_of_image_pages: u8, // Number of image pages
    pub reserved1: u8,             // Reserved for page function

    // Direct Color fields (required for direct/6 and YUV/7 memory models)
    pub red_mask_size: u8,          // Size of direct color red mask
    pub red_field_position: u8,     // Bit position of LSB of red mask
    pub green_mask_size: u8,        // Size of direct color green mask
    pub green_field_position: u8,   // Bit position of LSB of green mask
    pub blue_mask_size: u8,         // Size of direct color blue mask
    pub blue_field_position: u8,    // Bit position of LSB of blue mask
    pub rsvd_mask_size: u8,         // Size of direct color reserved mask
    pub rsvd_field_position: u8,    // Bit position of LSB of reserved mask
    pub direct_color_mode_info: u8, // Direct color mode attributes

    // Mandatory information for VBE 2.0 and above
    pub phys_base_ptr: u32, // Physical address for flat frame buffer
    pub reserved2: u32,     // Reserved - always set to 0
    pub reserved3: u16,     // Reserved - always set to 0

    // Mandatory information for VBE 3.0 and above
    pub lin_bytes_per_scan_line: u16, // Bytes per scan line for linear modes
    pub bnk_number_of_image_pages: u8, // Number of images for banked modes
    pub lin_number_of_image_pages: u8, // Number of images for linear modes
    pub lin_red_mask_size: u8,        // Size of direct color red mask (linear)
    pub lin_red_field_position: u8,   // Bit position of red mask (linear)
    pub lin_green_mask_size: u8,      // Size of direct color green mask (linear)
    pub lin_green_field_position: u8, // Bit position of green mask (linear)
    pub lin_blue_mask_size: u8,       // Size of direct color blue mask (linear)
    pub lin_blue_field_position: u8,  // Bit position of blue mask (linear)
    pub lin_rsvd_mask_size: u8,       // Size of reserved mask (linear)
    pub lin_rsvd_field_position: u8,  // Bit position of reserved mask (linear)
    pub max_pixel_clock: u32,         // Maximum pixel clock (in Hz) for mode
    pub reserved4: [u8; 189],         // Reserved
}

impl Default for ModeInfoBlock {
    fn default() -> Self {
        unsafe { mem::zeroed() }
    }
}

/// VBE mode attributes bitfield
pub mod mode_attributes {
    pub const SUPPORTED: u16 = 0x0001; // Mode supported
    pub const TTY_OUTPUT: u16 = 0x0002; // TTY output functions supported
    pub const COLOR_MODE: u16 = 0x0004; // Color mode
    pub const GRAPHICS_MODE: u16 = 0x0008; // Graphics mode
    pub const VGA_COMPATIBLE: u16 = 0x0010; // VGA compatible mode
    pub const VGA_WINDOWED: u16 = 0x0020; // VGA compatible windowed mode
    pub const LINEAR_FRAMEBUFFER: u16 = 0x0080; // Linear frame buffer available
    pub const DOUBLESCAN: u16 = 0x0100; // Double scan mode available
    pub const INTERLACED: u16 = 0x0200; // Interlaced mode available
    pub const TRIPLE_BUFFERING: u16 = 0x0400; // Triple buffering supported
    pub const STEREOSCOPIC: u16 = 0x0800; // Stereoscopic display supported
    pub const DUAL_START_ADDRESS: u16 = 0x1000; // Dual start address supported
}

/// VBE memory models
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryModel {
    Text = 0x00,
    CGA = 0x01,
    Hercules = 0x02,
    Planar = 0x03,
    PackedPixel = 0x04,
    NonChain4 = 0x05,
    DirectColor = 0x06,
    YUV = 0x07,
}

impl core::fmt::Display for MemoryModel {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            MemoryModel::Text => write!(f, "Text"),
            MemoryModel::CGA => write!(f, "CGA"),
            MemoryModel::Hercules => write!(f, "Hercules"),
            MemoryModel::Planar => write!(f, "Planar"),
            MemoryModel::PackedPixel => write!(f, "PackedPixel"),
            MemoryModel::NonChain4 => write!(f, "NonChain4"),
            MemoryModel::DirectColor => write!(f, "DirectColor"),
            MemoryModel::YUV => write!(f, "YUV"),
        }
    }
}

/// VBE video mode information
#[derive(Debug, Clone)]
pub struct VideoMode {
    pub mode_number: u16,
    pub width: u16,
    pub height: u16,
    pub bits_per_pixel: u8,
    pub bytes_per_pixel: u8,
    pub bytes_per_scanline: u16,
    pub framebuffer_addr: u32,
    pub framebuffer_size: u32,
    pub pixel_format: PixelFormat,
    pub linear_mode: bool,
    pub memory_model: MemoryModel,
}

impl VideoMode {
    /// Create a new video mode from mode info block
    pub fn from_mode_info(mode_number: u16, info: &ModeInfoBlock) -> Option<Self> {
        if info.mode_attributes & mode_attributes::SUPPORTED == 0 {
            return None;
        }

        if info.mode_attributes & mode_attributes::GRAPHICS_MODE == 0 {
            return None;
        }

        let bytes_per_pixel = (info.bits_per_pixel + 7) / 8;
        let framebuffer_size = info.y_resolution as u32 * info.bytes_per_scan_line as u32;

        let pixel_format = match (info.bits_per_pixel, info.memory_model) {
            (32, 6) => {
                // Direct color mode - check RGB mask positions
                if info.red_field_position == 16
                    && info.green_field_position == 8
                    && info.blue_field_position == 0
                {
                    PixelFormat::BGRA8888
                } else {
                    PixelFormat::RGBA8888
                }
            }
            (24, 6) => PixelFormat::RGB888,
            (16, 6) => {
                if info.red_mask_size == 5 && info.green_mask_size == 6 && info.blue_mask_size == 5
                {
                    PixelFormat::RGB565
                } else {
                    PixelFormat::RGB555
                }
            }
            (15, 6) => PixelFormat::RGB555,
            _ => return None, // Unsupported format
        };

        let linear_mode = (info.mode_attributes & mode_attributes::LINEAR_FRAMEBUFFER) != 0;

        Some(Self {
            mode_number,
            width: info.x_resolution,
            height: info.y_resolution,
            bits_per_pixel: info.bits_per_pixel,
            bytes_per_pixel,
            bytes_per_scanline: info.bytes_per_scan_line,
            framebuffer_addr: info.phys_base_ptr,
            framebuffer_size,
            pixel_format,
            linear_mode,
            memory_model: unsafe { mem::transmute(info.memory_model) },
        })
    }

    /// Check if this mode is suitable for desktop use
    pub fn is_desktop_suitable(&self) -> bool {
        self.linear_mode
            && self.width >= 800
            && self.height >= 600
            && self.bits_per_pixel >= 16
            && self.memory_model == MemoryModel::DirectColor
    }

    /// Get the aspect ratio as a tuple (width_ratio, height_ratio)
    pub fn aspect_ratio(&self) -> (u32, u32) {
        let gcd = Self::gcd(self.width as u32, self.height as u32);
        (self.width as u32 / gcd, self.height as u32 / gcd)
    }

    /// Calculate greatest common divisor
    fn gcd(mut a: u32, mut b: u32) -> u32 {
        while b != 0 {
            let temp = b;
            b = a % b;
            a = temp;
        }
        a
    }
}

/// VBE driver structure
pub struct VbeDriver {
    controller_info: Option<VbeInfoBlock>,
    available_modes: heapless::Vec<VideoMode, 256>,
    current_mode: Option<VideoMode>,
    initialized: bool,
}

impl VbeDriver {
    /// Create a new VBE driver instance
    pub const fn new() -> Self {
        Self {
            controller_info: None,
            available_modes: heapless::Vec::new(),
            current_mode: None,
            initialized: false,
        }
    }

    /// Initialize the VBE driver and detect available modes
    pub fn init(&mut self) -> Result<(), &'static str> {
        // VBE BIOS calls (int 0x10) are not callable from 64-bit long mode.
        // A real implementation would require either:
        //   1. A bootloader that passes VBE info to the kernel, or
        //   2. Switching to real mode temporarily (complex and fragile).
        // The kernel currently relies on the bootloader for framebuffer info
        // and does not use VBE directly.  Return an error so callers know
        // VBE is unavailable and can fall back to other graphics init paths.
        Err("VBE BIOS calls not available in 64-bit long mode")
    }

    /// Get VBE controller information
    fn get_controller_info(&self) -> Result<VbeInfoBlock, &'static str> {
        // VBE controller info is obtained via BIOS int 0x10, which is not
        // callable from 64-bit long mode.  The bootloader should collect VBE
        // info before entering long mode and pass it to the kernel.
        Err("VBE BIOS calls not available in 64-bit long mode")
    }

    /// Enumerate available video modes
    fn enumerate_modes(&mut self) -> Result<(), &'static str> {
        // Mode enumeration requires reading the mode list pointer from the
        // VBE controller info block, which is obtained via BIOS int 0x10.
        // Since BIOS calls are not available in 64-bit long mode, we cannot
        // enumerate modes.
        Err("VBE mode enumeration not available in 64-bit long mode")
    }

    /// Add common video modes for testing/simulation
    fn add_common_modes(&mut self) -> Result<(), &'static str> {
        // Common video modes would be enumerated from the VBE controller info.
        // Since BIOS calls are not available in 64-bit long mode, there are no
        // modes to add.
        Err("VBE mode enumeration not available in 64-bit long mode")
    }

    /// Create mode info structure for a given resolution and bit depth
    /// In a full VBE implementation, this would query the BIOS for actual mode capabilities
    fn create_mode_info(&self, width: u16, height: u16, bpp: u8) -> ModeInfoBlock {
        let mut info = ModeInfoBlock::default();

        info.mode_attributes = mode_attributes::SUPPORTED
            | mode_attributes::GRAPHICS_MODE
            | mode_attributes::LINEAR_FRAMEBUFFER;

        info.x_resolution = width;
        info.y_resolution = height;
        info.bits_per_pixel = bpp;
        info.bytes_per_scan_line = width * ((bpp as u16 + 7) / 8);
        info.memory_model = MemoryModel::DirectColor as u8;
        info.number_of_planes = 1;
        info.number_of_image_pages = 1;

        // Set up color masks based on pixel format
        match bpp {
            32 => {
                info.red_mask_size = 8;
                info.red_field_position = 16;
                info.green_mask_size = 8;
                info.green_field_position = 8;
                info.blue_mask_size = 8;
                info.blue_field_position = 0;
                info.rsvd_mask_size = 8;
                info.rsvd_field_position = 24;
            }
            24 => {
                info.red_mask_size = 8;
                info.red_field_position = 16;
                info.green_mask_size = 8;
                info.green_field_position = 8;
                info.blue_mask_size = 8;
                info.blue_field_position = 0;
            }
            16 => {
                info.red_mask_size = 5;
                info.red_field_position = 11;
                info.green_mask_size = 6;
                info.green_field_position = 5;
                info.blue_mask_size = 5;
                info.blue_field_position = 0;
            }
            _ => {}
        }

        // Framebuffer physical address would be obtained from the VBE mode info
        // returned by BIOS int 0x10.  Since BIOS calls are not available in
        // 64-bit long mode, leave it as zero.
        info.phys_base_ptr = 0;

        info
    }

    /// Set video mode
    pub fn set_mode(&mut self, mode_number: u16, linear: bool) -> Result<(), &'static str> {
        if !self.initialized {
            return Err("VBE driver not initialized");
        }

        // Find the requested mode
        let video_mode = self
            .available_modes
            .iter()
            .find(|mode| mode.mode_number == mode_number)
            .ok_or("Video mode not found")?
            .clone();

        if linear && !video_mode.linear_mode {
            return Err("Linear mode not supported for this mode");
        }

        // Set the mode using BIOS call
        let mode_flags = if linear { 0x4000 } else { 0x0000 } | mode_number;
        let status = self.bios_call(VbeFunction::SetMode, mode_flags as u32, 0, 0, 0);

        if !status.is_success() {
            return Err("Failed to set video mode");
        }

        self.current_mode = Some(video_mode);
        Ok(())
    }

    /// Get current video mode
    pub fn get_current_mode(&self) -> Option<&VideoMode> {
        self.current_mode.as_ref()
    }

    /// Get all available modes
    pub fn get_available_modes(&self) -> &[VideoMode] {
        &self.available_modes
    }

    /// Find best mode for given resolution and color depth
    pub fn find_best_mode(
        &self,
        min_width: u16,
        min_height: u16,
        preferred_bpp: u8,
    ) -> Option<&VideoMode> {
        let mut best_mode = None;
        let mut best_score = 0u32;

        for mode in &self.available_modes {
            if !mode.is_desktop_suitable() {
                continue;
            }

            if mode.width < min_width || mode.height < min_height {
                continue;
            }

            // Calculate score based on resolution match and color depth
            let width_score = if mode.width == min_width {
                1000
            } else {
                1000u32.saturating_sub((mode.width - min_width) as u32)
            };
            let height_score = if mode.height == min_height {
                1000
            } else {
                1000u32.saturating_sub((mode.height - min_height) as u32)
            };
            let bpp_score = if mode.bits_per_pixel == preferred_bpp {
                500
            } else {
                500 - (mode.bits_per_pixel as i32 - preferred_bpp as i32).abs() as u32
            };

            let score = width_score + height_score + bpp_score;

            if score > best_score {
                best_score = score;
                best_mode = Some(mode);
            }
        }

        best_mode
    }

    /// Get framebuffer information for current mode
    pub fn get_framebuffer_info(&self) -> Option<FramebufferInfo> {
        let mode = self.current_mode.as_ref()?;

        Some(FramebufferInfo::new(
            mode.width as usize,
            mode.height as usize,
            mode.pixel_format,
            mode.framebuffer_addr as usize,
            true, // GPU accelerated (VBE modes typically have some hardware support)
        ))
    }

    /// Check if VBE is initialized
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Get VBE version
    pub fn get_version(&self) -> Option<u16> {
        self.controller_info.as_ref().map(|info| info.version)
    }

    /// Get total video memory in bytes
    pub fn get_total_memory(&self) -> Option<u32> {
        self.controller_info
            .as_ref()
            .map(|info| info.total_memory as u32 * 64 * 1024)
    }

    /// BIOS interrupt call — not available in 64-bit long mode.
    fn bios_call(
        &self,
        _function: VbeFunction,
        _ebx: u32,
        _ecx: u32,
        _edx: u32,
        _edi: u32,
    ) -> VbeStatus {
        // BIOS interrupts (int 0x10) cannot be called from 64-bit long mode.
        // VBE information must be collected by the bootloader before entering
        // long mode and passed to the kernel via boot info structures.
        VbeStatus::NotSupported
    }
}

/// Global VBE driver instance
static mut VBE_DRIVER: VbeDriver = VbeDriver::new();

/// Initialize the global VBE driver
pub fn init() -> Result<(), &'static str> {
    unsafe { (&mut *core::ptr::addr_of_mut!(VBE_DRIVER)).init() }
}

/// Get a reference to the global VBE driver
pub fn driver() -> &'static VbeDriver {
    unsafe { &*core::ptr::addr_of!(VBE_DRIVER) }
}

/// Get a mutable reference to the global VBE driver
pub unsafe fn driver_mut() -> &'static mut VbeDriver {
    &mut *core::ptr::addr_of_mut!(VBE_DRIVER)
}

/// Set video mode using the global driver
pub fn set_video_mode(mode_number: u16, linear: bool) -> Result<(), &'static str> {
    unsafe { (&mut *core::ptr::addr_of_mut!(VBE_DRIVER)).set_mode(mode_number, linear) }
}

/// Find and set the best video mode for desktop use
pub fn set_desktop_mode(min_width: u16, min_height: u16) -> Result<VideoMode, &'static str> {
    unsafe {
        let driver = &mut *core::ptr::addr_of_mut!(VBE_DRIVER);
        if !driver.is_initialized() {
            driver.init()?;
        }

        let best_mode = driver
            .find_best_mode(min_width, min_height, 32)
            .ok_or("No suitable video mode found")?;

        let mode_info = best_mode.clone();
        driver.set_mode(best_mode.mode_number, true)?;

        Ok(mode_info)
    }
}

/// Get current framebuffer info
pub fn get_current_framebuffer_info() -> Option<FramebufferInfo> {
    unsafe { (&*core::ptr::addr_of!(VBE_DRIVER)).get_framebuffer_info() }
}

#[cfg(all(test, feature = "disabled-tests"))]
mod tests {
    use super::*;
    use crate::{serial_print, serial_println};

    #[cfg(feature = "disabled-tests")] // #[test_case]
    fn test_vbe_status() {
        serial_print!("test_vbe_status... ");
        assert_eq!(VbeStatus::from_ax(0x004F), VbeStatus::Success);
        assert_eq!(VbeStatus::from_ax(0x014F), VbeStatus::Failed);
        assert!(VbeStatus::Success.is_success());
        assert!(!VbeStatus::Failed.is_success());
        serial_println!("[ok]");
    }

    #[cfg(feature = "disabled-tests")] // #[test_case]
    fn test_video_mode_aspect_ratio() {
        serial_print!("test_video_mode_aspect_ratio... ");
        let mut info = ModeInfoBlock::default();
        info.mode_attributes = mode_attributes::SUPPORTED | mode_attributes::GRAPHICS_MODE;
        info.x_resolution = 1920;
        info.y_resolution = 1080;
        info.bits_per_pixel = 32;

        let mode = VideoMode::from_mode_info(0x13C, &info).unwrap();
        assert_eq!(mode.aspect_ratio(), (16, 9));
        serial_println!("[ok]");
    }

    #[cfg(feature = "disabled-tests")] // #[test_case]
    fn test_desktop_suitability() {
        serial_print!("test_desktop_suitability... ");
        let mut info = ModeInfoBlock::default();
        info.mode_attributes = mode_attributes::SUPPORTED
            | mode_attributes::GRAPHICS_MODE
            | mode_attributes::LINEAR_FRAMEBUFFER;
        info.x_resolution = 1920;
        info.y_resolution = 1080;
        info.bits_per_pixel = 32;
        info.memory_model = MemoryModel::DirectColor as u8;

        let mode = VideoMode::from_mode_info(0x13C, &info).unwrap();
        assert!(mode.is_desktop_suitable());
        serial_println!("[ok]");
    }
}
