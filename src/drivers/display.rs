//! # Unified Display Driver
//!
//! Provides a single entry point for display controller detection, mode setting,
//! framebuffer mapping, and graphics subsystem initialization.
//!
//! The driver probes PCI for a display controller (class 0x03), uses VBE I/O
//! ports to set a graphics mode, maps the linear framebuffer into virtual
//! memory, and hands off to the `graphics` module for pixel-level rendering.

use crate::drivers::vbe_io;
use crate::graphics::framebuffer::PixelFormat;
use crate::graphics::{self, Color};
use crate::pci;
use alloc::format;
use alloc::string::String;
use spin::Mutex;

/// Display driver status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayStatus {
    Uninitialized,
    Probing,
    ModeSet,
    FramebufferMapped,
    Ready,
    Error,
    TextModeFallback,
}

impl core::fmt::Display for DisplayStatus {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DisplayStatus::Uninitialized => write!(f, "Uninitialized"),
            DisplayStatus::Probing => write!(f, "Probing"),
            DisplayStatus::ModeSet => write!(f, "Mode Set"),
            DisplayStatus::FramebufferMapped => write!(f, "Framebuffer Mapped"),
            DisplayStatus::Ready => write!(f, "Ready"),
            DisplayStatus::Error => write!(f, "Error"),
            DisplayStatus::TextModeFallback => write!(f, "Text Mode Fallback"),
        }
    }
}

/// Information about the detected display controller
#[derive(Debug, Clone)]
pub struct DisplayControllerInfo {
    pub vendor_id: u16,
    pub device_id: u16,
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub vendor_name: &'static str,
    pub framebuffer_phys: u64,
    pub bar0: u32,
}

impl DisplayControllerInfo {
    fn get_vendor_name(vendor_id: u16) -> &'static str {
        match vendor_id {
            0x8086 => "Intel",
            0x10DE => "NVIDIA",
            0x1002 => "AMD",
            0x1234 => "QEMU/Bochs",
            0x80EE => "VirtualBox",
            0x15AD => "VMware",
            0x1AF4 => "Virtio",
            _ => "Unknown",
        }
    }
}

/// Current display mode information
#[derive(Debug, Clone, Copy)]
pub struct DisplayMode {
    pub width: usize,
    pub height: usize,
    pub bpp: u8,
    pub pixel_format: PixelFormat,
    pub pitch: usize,
    pub framebuffer_phys: u64,
    pub framebuffer_virt: usize,
}

/// Unified display driver state
pub struct DisplayDriver {
    status: DisplayStatus,
    controller: Option<DisplayControllerInfo>,
    mode: Option<DisplayMode>,
    vbe_version: u16,
    vbe_video_memory_bytes: u64,
    error_msg: Option<String>,
}

impl DisplayDriver {
    /// Create a new uninitialized display driver
    pub const fn new() -> Self {
        Self {
            status: DisplayStatus::Uninitialized,
            controller: None,
            mode: None,
            vbe_version: 0,
            vbe_video_memory_bytes: 0,
            error_msg: None,
        }
    }

    /// Get current status
    pub fn status(&self) -> DisplayStatus {
        self.status
    }

    /// Check if display is ready for rendering
    pub fn is_ready(&self) -> bool {
        self.status == DisplayStatus::Ready
    }

    /// Check if we fell back to text mode
    pub fn is_text_mode(&self) -> bool {
        self.status == DisplayStatus::TextModeFallback
    }

    /// Get display controller info
    pub fn controller(&self) -> Option<&DisplayControllerInfo> {
        self.controller.as_ref()
    }

    /// Get current display mode
    pub fn mode(&self) -> Option<&DisplayMode> {
        self.mode.as_ref()
    }

    /// Get screen dimensions (width, height)
    pub fn dimensions(&self) -> Option<(usize, usize)> {
        self.mode.map(|m| (m.width, m.height))
    }

    /// Get error message if any
    pub fn error(&self) -> Option<&str> {
        self.error_msg.as_deref()
    }

    /// Get VBE version
    pub fn vbe_version(&self) -> u16 {
        self.vbe_version
    }

    /// Get VBE video memory in bytes
    pub fn vbe_video_memory(&self) -> u64 {
        self.vbe_video_memory_bytes
    }

    /// Detect the display controller by scanning PCI bus 0 for class 0x03.
    /// Returns the controller info if found.
    fn detect_controller(&mut self) -> Result<DisplayControllerInfo, &'static str> {
        let scanner = pci::get_pci_scanner().lock();
        for dev in 0u8..32 {
            let vendor = scanner.read_config_dword(0, dev, 0, 0x00) & 0xFFFF;
            if vendor == 0xFFFF {
                continue;
            }
            let class = scanner.read_config_dword(0, dev, 0, 0x08);
            if (class >> 24) & 0xFF == 0x03 {
                let device_id =
                    ((scanner.read_config_dword(0, dev, 0, 0x00) >> 16) & 0xFFFF) as u16;
                let bar0 = scanner.read_config_dword(0, dev, 0, 0x10);
                let fb_phys = if bar0 & 0x1 == 0 {
                    (bar0 & 0xFFFF_FFF0) as u64
                } else {
                    0
                };

                let info = DisplayControllerInfo {
                    vendor_id: vendor as u16,
                    device_id,
                    bus: 0,
                    device: dev,
                    function: 0,
                    vendor_name: DisplayControllerInfo::get_vendor_name(vendor as u16),
                    framebuffer_phys: fb_phys,
                    bar0,
                };

                self.controller = Some(info.clone());
                return Ok(info);
            }
        }
        Err("No display controller found on PCI bus")
    }

    /// Probe VBE I/O capabilities and store version/video memory info
    fn probe_vbe(&mut self) -> Result<(), &'static str> {
        if !vbe_io::detect_vbe() {
            return Err("VBE I/O not detected");
        }
        self.vbe_version = vbe_io::get_vbe_version();
        self.vbe_video_memory_bytes = vbe_io::get_video_memory_bytes();
        Ok(())
    }

    /// Set a graphics mode via VBE I/O ports, map the framebuffer, and
    /// initialize the graphics subsystem.
    fn set_mode_and_init(
        &mut self,
        width: u16,
        height: u16,
        bpp: u8,
        phys_mem_offset: u64,
    ) -> Result<DisplayMode, &'static str> {
        // Set the VBE mode
        let vbe_mode = vbe_io::set_mode(width, height, bpp)?;

        self.status = DisplayStatus::ModeSet;

        // Calculate framebuffer virtual address and size
        let fb_virt = (phys_mem_offset + vbe_mode.framebuffer_phys) as usize;
        let fb_size = vbe_mode.pitch * vbe_mode.height as usize;
        // Map the full video memory to prevent page faults from rendering
        // that may write beyond pitch*height (e.g. double-buffering, cursor planes)
        let video_mem_bytes = vbe_io::get_video_memory_bytes().max(fb_size as u64) as usize;
        let fb_size_mapped = ((video_mem_bytes + 0xFFF) & !0xFFF) as usize;

        // Map the framebuffer as MMIO (write-combining, uncached)
        crate::serial_println!(
            "display: mapping framebuffer phys=0x{:X} virt=0x{:X} size={} (video_mem={})",
            vbe_mode.framebuffer_phys,
            fb_virt,
            fb_size,
            fb_size_mapped
        );

        if let Err(e) = crate::memory::map_mmio_region(fb_virt, fb_size_mapped) {
            self.error_msg = Some(format!("framebuffer map failed: {}", e));
            return Err("framebuffer MMIO map failed");
        }

        self.status = DisplayStatus::FramebufferMapped;

        // Verify the framebuffer is writable
        let test_ptr = fb_virt as *mut u32;
        unsafe {
            core::ptr::write_volatile(test_ptr, 0x00000000);
        }

        // Initialize the graphics subsystem with the raw framebuffer
        crate::graphics::init_graphics_from_raw(
            fb_virt as *mut u8,
            vbe_mode.width as usize,
            vbe_mode.height as usize,
            vbe_mode.pixel_format,
        )?;

        let mode = DisplayMode {
            width: vbe_mode.width as usize,
            height: vbe_mode.height as usize,
            bpp: vbe_mode.bpp,
            pixel_format: vbe_mode.pixel_format,
            pitch: vbe_mode.pitch,
            framebuffer_phys: vbe_mode.framebuffer_phys,
            framebuffer_virt: fb_virt,
        };

        self.mode = Some(mode);
        self.status = DisplayStatus::Ready;
        Ok(mode)
    }

    /// Full initialization sequence: detect controller, probe VBE, set mode,
    /// map framebuffer, init graphics.
    ///
    /// `phys_mem_offset` is the physical memory offset from the bootloader
    /// (used to compute the framebuffer virtual address).
    pub fn init(&mut self, phys_mem_offset: u64) -> Result<DisplayMode, &'static str> {
        self.status = DisplayStatus::Probing;
        self.error_msg = None;

        // Step 1: Detect display controller via PCI
        match self.detect_controller() {
            Ok(info) => {
                crate::serial_println!(
                    "display: found {} (vendor=0x{:04X} device=0x{:04X}) fb_phys=0x{:X}",
                    info.vendor_name,
                    info.vendor_id,
                    info.device_id,
                    info.framebuffer_phys
                );
            }
            Err(e) => {
                crate::serial_println!("display: PCI detection failed: {}", e);
            }
        }

        // Step 2: Probe VBE I/O
        if let Err(e) = self.probe_vbe() {
            self.error_msg = Some(format!("VBE probe failed: {}", e));
            self.status = DisplayStatus::Error;
            return Err("VBE probe failed");
        }

        crate::serial_println!(
            "display: VBE version=0x{:04X} video_memory={} bytes",
            self.vbe_version,
            self.vbe_video_memory_bytes
        );

        // Step 3: Set default desktop mode (800x600x32 — safe for QEMU)
        let mode = self.set_mode_and_init(800, 600, 32, phys_mem_offset)?;

        // Step 4: Clear screen with boot background
        graphics::framebuffer::clear_screen(Color::rgb(28, 34, 54));
        graphics::framebuffer::present();

        crate::serial_println!("display: ready {}x{}x{}", mode.width, mode.height, mode.bpp);

        Ok(mode)
    }

    /// Initialize from a bootloader-provided framebuffer (fallback path when
    /// VBE I/O mode setting is not available).
    pub fn init_from_bootloader(
        &mut self,
        buffer_ptr: *mut u8,
        width: usize,
        height: usize,
        bytes_per_pixel: usize,
    ) -> Result<DisplayMode, &'static str> {
        self.status = DisplayStatus::Probing;
        self.error_msg = None;

        let pixel_format = match bytes_per_pixel {
            4 => PixelFormat::RGBA8888,
            3 => PixelFormat::RGB888,
            2 => PixelFormat::RGB565,
            _ => PixelFormat::RGBA8888,
        };

        let pitch = width * bytes_per_pixel;

        crate::graphics::init_graphics_from_raw(buffer_ptr, width, height, pixel_format)?;

        let mode = DisplayMode {
            width,
            height,
            bpp: (bytes_per_pixel * 8) as u8,
            pixel_format,
            pitch,
            framebuffer_phys: buffer_ptr as u64,
            framebuffer_virt: buffer_ptr as usize,
        };

        self.mode = Some(mode);
        self.status = DisplayStatus::Ready;

        graphics::framebuffer::clear_screen(Color::rgb(28, 34, 54));
        graphics::framebuffer::present();

        crate::serial_println!(
            "display: ready (bootloader) {}x{}x{}",
            mode.width,
            mode.height,
            mode.bpp
        );

        Ok(mode)
    }

    /// Change display resolution at runtime
    pub fn change_mode(
        &mut self,
        width: u16,
        height: u16,
        bpp: u8,
        phys_mem_offset: u64,
    ) -> Result<DisplayMode, &'static str> {
        if self.status != DisplayStatus::Ready {
            return Err("Display not initialized");
        }
        self.set_mode_and_init(width, height, bpp, phys_mem_offset)
    }

    /// Clear the screen to a solid color
    pub fn clear(&self, color: Color) {
        graphics::framebuffer::clear_screen(color);
    }

    /// Present the current framebuffer to the display
    pub fn present(&self) {
        graphics::framebuffer::present();
    }

    /// Set a single pixel
    pub fn set_pixel(&self, x: usize, y: usize, color: Color) {
        graphics::framebuffer::set_pixel(x, y, color);
    }

    /// Fill a rectangle
    pub fn fill_rect(&self, x: usize, y: usize, width: usize, height: usize, color: Color) {
        graphics::framebuffer::fill_rect(
            graphics::framebuffer::Rect::new(x, y, width, height),
            color,
        );
    }

    /// Get a driver info struct for reporting
    pub fn driver_info(&self) -> DisplayDriverInfo {
        DisplayDriverInfo {
            status: self.status,
            controller: self.controller.as_ref().map(|c| {
                format!(
                    "{} 0x{:04X}:0x{:04X}",
                    c.vendor_name, c.vendor_id, c.device_id
                )
            }),
            mode: self
                .mode
                .map(|m| format!("{}x{}x{}bpp", m.width, m.height, m.bpp)),
            vbe_version: self.vbe_version,
            vbe_video_memory_bytes: self.vbe_video_memory_bytes,
            framebuffer_virt: self.mode.map(|m| m.framebuffer_virt),
        }
    }
}

/// Serializable driver info for status reporting
#[derive(Debug, Clone)]
pub struct DisplayDriverInfo {
    pub status: DisplayStatus,
    pub controller: Option<String>,
    pub mode: Option<String>,
    pub vbe_version: u16,
    pub vbe_video_memory_bytes: u64,
    pub framebuffer_virt: Option<usize>,
}

/// Global display driver instance
static DISPLAY_DRIVER: Mutex<DisplayDriver> = Mutex::new(DisplayDriver::new());

/// Initialize the display driver with VBE I/O mode setting.
///
/// This is the primary initialization path. It detects the display controller,
/// sets a VBE graphics mode, maps the framebuffer, and initializes the
/// graphics subsystem.
///
/// Returns the display mode on success.
pub fn init(phys_mem_offset: u64) -> Result<DisplayMode, &'static str> {
    DISPLAY_DRIVER.lock().init(phys_mem_offset)
}

/// Initialize the display driver from a bootloader-provided framebuffer.
///
/// This is the fallback path when VBE I/O is not available (e.g. real hardware
/// where the bootloader sets the graphics mode).
pub fn init_from_bootloader(
    buffer_ptr: *mut u8,
    width: usize,
    height: usize,
    bytes_per_pixel: usize,
) -> Result<DisplayMode, &'static str> {
    DISPLAY_DRIVER
        .lock()
        .init_from_bootloader(buffer_ptr, width, height, bytes_per_pixel)
}

/// Get the current display driver status
pub fn status() -> DisplayStatus {
    DISPLAY_DRIVER.lock().status()
}

/// Check if the display driver is ready for rendering
pub fn is_ready() -> bool {
    DISPLAY_DRIVER.lock().is_ready()
}

/// Check if the display fell back to text mode
pub fn is_text_mode() -> bool {
    DISPLAY_DRIVER.lock().is_text_mode()
}

/// Get current display mode
pub fn mode() -> Option<DisplayMode> {
    DISPLAY_DRIVER.lock().mode
}

/// Get screen dimensions (width, height)
pub fn dimensions() -> Option<(usize, usize)> {
    DISPLAY_DRIVER.lock().dimensions()
}

/// Get display controller info
pub fn controller() -> Option<DisplayControllerInfo> {
    DISPLAY_DRIVER.lock().controller.clone()
}

/// Get driver info for status reporting
pub fn driver_info() -> DisplayDriverInfo {
    DISPLAY_DRIVER.lock().driver_info()
}

/// Clear the screen to a solid color
pub fn clear(color: Color) {
    DISPLAY_DRIVER.lock().clear(color)
}

/// Present the current framebuffer to the display
pub fn present() {
    DISPLAY_DRIVER.lock().present()
}

/// Set a single pixel
pub fn set_pixel(x: usize, y: usize, color: Color) {
    DISPLAY_DRIVER.lock().set_pixel(x, y, color)
}

/// Fill a rectangle
pub fn fill_rect(x: usize, y: usize, width: usize, height: usize, color: Color) {
    DISPLAY_DRIVER.lock().fill_rect(x, y, width, height, color)
}

/// Change display resolution at runtime
pub fn change_mode(
    width: u16,
    height: u16,
    bpp: u8,
    phys_mem_offset: u64,
) -> Result<DisplayMode, &'static str> {
    DISPLAY_DRIVER
        .lock()
        .change_mode(width, height, bpp, phys_mem_offset)
}
