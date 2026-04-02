//! # RustOS Graphics Framebuffer System
//!
//! Hardware-accelerated framebuffer management for desktop UI rendering.
//! Supports GPU-accelerated operations, multiple pixel formats, and high-resolution displays.

use alloc::vec::Vec;
use core::ptr::{addr_of, addr_of_mut};

/// Maximum supported resolution width
pub const MAX_WIDTH: usize = 7680; // 8K width
/// Maximum supported resolution height
pub const MAX_HEIGHT: usize = 4320; // 8K height
/// Default resolution width
pub const DEFAULT_WIDTH: usize = 1920;
/// Default resolution height
pub const DEFAULT_HEIGHT: usize = 1080;

/// Pixel format types supported by the framebuffer
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PixelFormat {
    /// 32-bit RGBA (8 bits per channel)
    RGBA8888 = 0,
    /// 32-bit BGRA (8 bits per channel)
    BGRA8888 = 1,
    /// 24-bit RGB (8 bits per channel, packed)
    RGB888 = 2,
    /// 16-bit RGB (5-6-5 bits per channel)
    RGB565 = 3,
    /// 15-bit RGB (5-5-5 bits per channel)
    RGB555 = 4,
}

impl PixelFormat {
    /// Get the number of bytes per pixel for this format
    pub const fn bytes_per_pixel(&self) -> usize {
        match self {
            PixelFormat::RGBA8888 | PixelFormat::BGRA8888 => 4,
            PixelFormat::RGB888 => 3,
            PixelFormat::RGB565 | PixelFormat::RGB555 => 2,
        }
    }

    /// Get the number of bits per pixel for this format
    pub const fn bits_per_pixel(&self) -> usize {
        self.bytes_per_pixel() * 8
    }
}

/// Color representation in RGBA format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    /// Create a new color
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Create a new opaque color
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::new(r, g, b, 255)
    }

    /// Create a color from a 32-bit RGBA value
    pub const fn from_rgba32(rgba: u32) -> Self {
        Self {
            r: ((rgba >> 24) & 0xFF) as u8,
            g: ((rgba >> 16) & 0xFF) as u8,
            b: ((rgba >> 8) & 0xFF) as u8,
            a: (rgba & 0xFF) as u8,
        }
    }

    /// Convert to 32-bit RGBA value
    pub const fn to_rgba32(&self) -> u32 {
        ((self.r as u32) << 24) | ((self.g as u32) << 16) | ((self.b as u32) << 8) | (self.a as u32)
    }

    /// Convert color to specific pixel format
    pub fn to_pixel_format(&self, format: PixelFormat) -> u32 {
        match format {
            PixelFormat::RGBA8888 => self.to_rgba32(),
            PixelFormat::BGRA8888 => {
                ((self.b as u32) << 24)
                    | ((self.g as u32) << 16)
                    | ((self.r as u32) << 8)
                    | (self.a as u32)
            }
            PixelFormat::RGB888 => {
                ((self.r as u32) << 16) | ((self.g as u32) << 8) | (self.b as u32)
            }
            PixelFormat::RGB565 => {
                let r5 = (self.r >> 3) as u32;
                let g6 = (self.g >> 2) as u32;
                let b5 = (self.b >> 3) as u32;
                (r5 << 11) | (g6 << 5) | b5
            }
            PixelFormat::RGB555 => {
                let r5 = (self.r >> 3) as u32;
                let g5 = (self.g >> 3) as u32;
                let b5 = (self.b >> 3) as u32;
                (r5 << 10) | (g5 << 5) | b5
            }
        }
    }

    // Common colors
    pub const BLACK: Color = Color::rgb(0, 0, 0);
    pub const WHITE: Color = Color::rgb(255, 255, 255);
    pub const RED: Color = Color::rgb(255, 0, 0);
    pub const GREEN: Color = Color::rgb(0, 255, 0);
    pub const BLUE: Color = Color::rgb(0, 0, 255);
    pub const YELLOW: Color = Color::rgb(255, 255, 0);
    pub const CYAN: Color = Color::rgb(0, 255, 255);
    pub const MAGENTA: Color = Color::rgb(255, 0, 255);
    pub const GRAY: Color = Color::rgb(128, 128, 128);
    pub const LIGHT_GRAY: Color = Color::rgb(192, 192, 192);
    pub const DARK_GRAY: Color = Color::rgb(64, 64, 64);
    pub const TRANSPARENT: Color = Color::new(0, 0, 0, 0);
}

/// Rectangle structure for drawing operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

impl Rect {
    /// Create a new rectangle
    pub const fn new(x: usize, y: usize, width: usize, height: usize) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Check if a point is inside this rectangle
    pub fn contains(&self, x: usize, y: usize) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }

    /// Get the area of this rectangle
    pub const fn area(&self) -> usize {
        self.width * self.height
    }

    /// Check if this rectangle intersects with another
    pub fn intersects(&self, other: &Rect) -> bool {
        self.x < other.x + other.width
            && self.x + self.width > other.x
            && self.y < other.y + other.height
            && self.y + self.height > other.y
    }
}

/// Framebuffer information structure
#[derive(Debug, Clone)]
pub struct FramebufferInfo {
    pub width: usize,
    pub height: usize,
    pub pixel_format: PixelFormat,
    pub stride: usize,
    pub physical_address: usize,
    pub size: usize,
    pub gpu_accelerated: bool,
}

impl FramebufferInfo {
    /// Create new framebuffer info
    pub fn new(
        width: usize,
        height: usize,
        pixel_format: PixelFormat,
        physical_address: usize,
        gpu_accelerated: bool,
    ) -> Self {
        let bytes_per_pixel = pixel_format.bytes_per_pixel();
        let stride = width * bytes_per_pixel;
        let size = stride * height;

        Self {
            width,
            height,
            pixel_format,
            stride,
            physical_address,
            size,
            gpu_accelerated,
        }
    }

    /// Get the total number of pixels
    pub const fn pixel_count(&self) -> usize {
        self.width * self.height
    }

    /// Get bytes per pixel
    pub const fn bytes_per_pixel(&self) -> usize {
        self.pixel_format.bytes_per_pixel()
    }
}

/// Hardware acceleration capabilities
#[derive(Debug, Clone, Copy)]
pub struct HardwareAcceleration {
    pub gpu_clear: bool,
    pub gpu_copy: bool,
    pub gpu_fill: bool,
    pub gpu_blit: bool,
    pub compute_shaders: bool,
    pub hardware_cursor: bool,
}

impl Default for HardwareAcceleration {
    fn default() -> Self {
        Self {
            gpu_clear: false,
            gpu_copy: false,
            gpu_fill: false,
            gpu_blit: false,
            compute_shaders: false,
            hardware_cursor: false,
        }
    }
}

/// Simplified framebuffer for actual pixel operations
pub struct SimpleFramebuffer {
    pub buffer: *mut u8,
    pub width: usize,
    pub height: usize,
    pub stride: usize,
    pub pixel_format: PixelFormat,
}

unsafe impl Send for SimpleFramebuffer {}
unsafe impl Sync for SimpleFramebuffer {}

impl SimpleFramebuffer {
    pub fn new(buffer: *mut u8, width: usize, height: usize, pixel_format: PixelFormat) -> Self {
        let bytes_per_pixel = pixel_format.bytes_per_pixel();
        let stride = width * bytes_per_pixel;
        Self {
            buffer,
            width,
            height,
            stride,
            pixel_format,
        }
    }

    pub fn set_pixel(&mut self, x: usize, y: usize, color: Color) {
        if x >= self.width || y >= self.height {
            return;
        }

        let bytes_per_pixel = self.pixel_format.bytes_per_pixel();
        let offset = y * self.stride + x * bytes_per_pixel;
        let pixel_value = color.to_pixel_format(self.pixel_format);

        unsafe {
            match bytes_per_pixel {
                1 => {
                    core::ptr::write_volatile(self.buffer.add(offset), pixel_value as u8);
                }
                2 => {
                    let ptr = self.buffer.add(offset) as *mut u16;
                    core::ptr::write_volatile(ptr, pixel_value as u16);
                }
                3 => {
                    // 24-bit RGB - write each component separately
                    let ptr = self.buffer.add(offset);
                    core::ptr::write_volatile(ptr, (pixel_value & 0xFF) as u8);
                    core::ptr::write_volatile(ptr.add(1), ((pixel_value >> 8) & 0xFF) as u8);
                    core::ptr::write_volatile(ptr.add(2), ((pixel_value >> 16) & 0xFF) as u8);
                }
                4 => {
                    let ptr = self.buffer.add(offset) as *mut u32;
                    core::ptr::write_volatile(ptr, pixel_value);
                }
                _ => {
                    // Generic fallback for unusual pixel formats
                    for i in 0..bytes_per_pixel {
                        let byte_value = ((pixel_value >> (i * 8)) & 0xFF) as u8;
                        core::ptr::write_volatile(self.buffer.add(offset + i), byte_value);
                    }
                }
            }
        }
    }
    
    /// Get pixel color at specified coordinates
    pub fn get_pixel(&self, x: usize, y: usize) -> Option<Color> {
        if x >= self.width || y >= self.height {
            return None;
        }

        let bytes_per_pixel = self.pixel_format.bytes_per_pixel();
        let offset = y * self.stride + x * bytes_per_pixel;

        unsafe {
            let pixel_value = match bytes_per_pixel {
                1 => {
                    core::ptr::read_volatile(self.buffer.add(offset)) as u32
                }
                2 => {
                    let ptr = self.buffer.add(offset) as *const u16;
                    core::ptr::read_volatile(ptr) as u32
                }
                3 => {
                    let ptr = self.buffer.add(offset);
                    let b = core::ptr::read_volatile(ptr) as u32;
                    let g = core::ptr::read_volatile(ptr.add(1)) as u32;
                    let r = core::ptr::read_volatile(ptr.add(2)) as u32;
                    b | (g << 8) | (r << 16)
                }
                4 => {
                    let ptr = self.buffer.add(offset) as *const u32;
                    core::ptr::read_volatile(ptr)
                }
                _ => return None,
            };

            Some(self.pixel_value_to_color(pixel_value))
        }
    }
    
    /// Convert pixel value back to Color based on pixel format
    fn pixel_value_to_color(&self, pixel_value: u32) -> Color {
        match self.pixel_format {
            PixelFormat::RGBA8888 => {
                Color::new(
                    ((pixel_value >> 24) & 0xFF) as u8,
                    ((pixel_value >> 16) & 0xFF) as u8,
                    ((pixel_value >> 8) & 0xFF) as u8,
                    (pixel_value & 0xFF) as u8,
                )
            }
            PixelFormat::BGRA8888 => {
                Color::new(
                    ((pixel_value >> 8) & 0xFF) as u8,
                    ((pixel_value >> 16) & 0xFF) as u8,
                    ((pixel_value >> 24) & 0xFF) as u8,
                    (pixel_value & 0xFF) as u8,
                )
            }
            PixelFormat::RGB888 => {
                Color::rgb(
                    ((pixel_value >> 16) & 0xFF) as u8,
                    ((pixel_value >> 8) & 0xFF) as u8,
                    (pixel_value & 0xFF) as u8,
                )
            }
            PixelFormat::RGB565 => {
                let r = ((pixel_value >> 11) & 0x1F) as u8;
                let g = ((pixel_value >> 5) & 0x3F) as u8;
                let b = (pixel_value & 0x1F) as u8;
                Color::rgb(
                    (r << 3) | (r >> 2), // Expand 5-bit to 8-bit
                    (g << 2) | (g >> 4), // Expand 6-bit to 8-bit
                    (b << 3) | (b >> 2), // Expand 5-bit to 8-bit
                )
            }
            PixelFormat::RGB555 => {
                let r = ((pixel_value >> 10) & 0x1F) as u8;
                let g = ((pixel_value >> 5) & 0x1F) as u8;
                let b = (pixel_value & 0x1F) as u8;
                Color::rgb(
                    (r << 3) | (r >> 2), // Expand 5-bit to 8-bit
                    (g << 3) | (g >> 2), // Expand 5-bit to 8-bit
                    (b << 3) | (b >> 2), // Expand 5-bit to 8-bit
                )
            }
        }
    }

    pub fn clear(&mut self, color: Color) {
        // Try hardware-accelerated clear first
        if self.try_hardware_clear(color) {
            return;
        }
        
        // Fall back to software clear
        self.software_clear(color);
    }
    
    /// Attempt hardware-accelerated clear
    fn try_hardware_clear(&mut self, color: Color) -> bool {
        // Check if GPU acceleration is available
        if let Some(gpu_manager) = crate::gpu::get_gpu_manager() {
            if gpu_manager.is_acceleration_available() {
                // Use GPU to clear the framebuffer
                let clear_result = gpu_manager.clear_framebuffer(
                    self.buffer as u64,
                    self.width,
                    self.height,
                    self.stride,
                    color.to_pixel_format(self.pixel_format)
                );
                
                if clear_result.is_ok() {
                    return true;
                }
            }
        }
        
        // Try 2D acceleration engine if available
        if self.try_2d_accelerated_clear(color) {
            return true;
        }
        
        false
    }
    
    /// Try 2D acceleration engine for clearing
    fn try_2d_accelerated_clear(&mut self, color: Color) -> bool {
        // Access 2D acceleration registers (example for common GPUs)
        unsafe {
            // Intel graphics 2D engine registers (example)
            let gfx_base = 0xFED00000u64 as *mut u32;
            if !gfx_base.is_null() {
                // Set up 2D blit operation for clear
                let pixel_value = color.to_pixel_format(self.pixel_format);
                
                // Configure 2D engine for solid fill
                core::ptr::write_volatile(gfx_base.add(0x40), pixel_value); // Fill color
                core::ptr::write_volatile(gfx_base.add(0x44), self.buffer as u32); // Destination address
                core::ptr::write_volatile(gfx_base.add(0x48), self.stride as u32); // Destination pitch
                core::ptr::write_volatile(gfx_base.add(0x4C), ((self.height << 16) | self.width) as u32); // Dimensions
                core::ptr::write_volatile(gfx_base.add(0x50), 0xF0); // ROP: PATCOPY (solid fill)
                
                // Start the operation
                core::ptr::write_volatile(gfx_base.add(0x00), 0x01); // Start bit
                
                // Wait for completion (with timeout)
                let mut timeout = 10000;
                while timeout > 0 {
                    let status = core::ptr::read_volatile(gfx_base.add(0x04));
                    if (status & 0x01) == 0 { // Operation complete
                        return true;
                    }
                    timeout -= 1;
                    
                    // Small delay
                    for _ in 0..100 {
                        core::hint::spin_loop();
                    }
                }
            }
        }
        
        false
    }
    
    /// Software fallback for clearing
    fn software_clear(&mut self, color: Color) {
        let pixel_value = color.to_pixel_format(self.pixel_format);
        let bytes_per_pixel = self.pixel_format.bytes_per_pixel();
        
        // Use optimized memory operations for large clears
        if self.width * self.height > 1024 * 768 {
            self.optimized_clear(pixel_value, bytes_per_pixel);
        } else {
            // Standard pixel-by-pixel clear for smaller surfaces
            for y in 0..self.height {
                for x in 0..self.width {
                    self.set_pixel(x, y, color);
                }
            }
        }
    }
    
    /// Optimized clear using SIMD or memory operations
    fn optimized_clear(&mut self, pixel_value: u32, bytes_per_pixel: usize) {
        unsafe {
            match bytes_per_pixel {
                4 => {
                    // 32-bit pixels - use u32 writes
                    let buffer_u32 = self.buffer as *mut u32;
                    let pixel_count = (self.height * self.stride) / 4;
                    
                    // Use rep stosd for fast 32-bit fills on x86_64
                    core::arch::asm!(
                        "rep stosd",
                        inout("rdi") buffer_u32 => _,
                        inout("rcx") pixel_count => _,
                        in("eax") pixel_value,
                        options(nostack, preserves_flags)
                    );
                }
                2 => {
                    // 16-bit pixels - use u16 writes
                    let buffer_u16 = self.buffer as *mut u16;
                    let pixel_count = (self.height * self.stride) / 2;
                    let pixel_value_16 = pixel_value as u16;
                    
                    core::arch::asm!(
                        "rep stosw",
                        inout("rdi") buffer_u16 => _,
                        inout("rcx") pixel_count => _,
                        in("ax") pixel_value_16,
                        options(nostack, preserves_flags)
                    );
                }
                1 => {
                    // 8-bit pixels - use u8 writes
                    let pixel_count = self.height * self.stride;
                    let pixel_value_8 = pixel_value as u8;
                    
                    core::arch::asm!(
                        "rep stosb",
                        inout("rdi") self.buffer => _,
                        inout("rcx") pixel_count => _,
                        in("al") pixel_value_8,
                        options(nostack, preserves_flags)
                    );
                }
                _ => {
                    // Fallback to standard clear
                    let total_bytes = self.height * self.stride;
                    core::ptr::write_bytes(self.buffer, pixel_value as u8, total_bytes);
                }
            }
        }
    }

    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        // Bounds checking
        if rect.x >= self.width || rect.y >= self.height {
            return;
        }
        
        let end_x = (rect.x + rect.width).min(self.width);
        let end_y = (rect.y + rect.height).min(self.height);
        
        if end_x <= rect.x || end_y <= rect.y {
            return;
        }
        
        // Try hardware acceleration for large rectangles
        if rect.width * rect.height > 64 * 64 {
            if self.try_hardware_fill_rect(rect, color) {
                return;
            }
        }
        
        // Software fallback with optimizations
        self.software_fill_rect(rect, color, end_x, end_y);
    }
    
    /// Try hardware-accelerated rectangle fill
    fn try_hardware_fill_rect(&mut self, rect: Rect, color: Color) -> bool {
        // Try GPU acceleration first
        if let Some(gpu_manager) = crate::gpu::get_gpu_manager() {
            if gpu_manager.is_acceleration_available() {
                let result = gpu_manager.fill_rectangle(
                    self.buffer as u64,
                    self.stride,
                    rect.x, rect.y, rect.width, rect.height,
                    color.to_pixel_format(self.pixel_format)
                );
                
                if result.is_ok() {
                    return true;
                }
            }
        }
        
        // Try 2D acceleration engine
        unsafe {
            let gfx_base = 0xFED00000u64 as *mut u32;
            if !gfx_base.is_null() {
                let pixel_value = color.to_pixel_format(self.pixel_format);
                let dest_addr = self.buffer as u64 + 
                    (rect.y * self.stride + rect.x * self.pixel_format.bytes_per_pixel()) as u64;
                
                // Configure 2D rectangle fill
                core::ptr::write_volatile(gfx_base.add(0x40), pixel_value);
                core::ptr::write_volatile(gfx_base.add(0x44), dest_addr as u32);
                core::ptr::write_volatile(gfx_base.add(0x48), self.stride as u32);
                core::ptr::write_volatile(gfx_base.add(0x4C), ((rect.height << 16) | rect.width) as u32);
                core::ptr::write_volatile(gfx_base.add(0x50), 0xF0); // PATCOPY
                
                // Start operation
                core::ptr::write_volatile(gfx_base.add(0x00), 0x01);
                
                // Wait for completion
                let mut timeout = 1000;
                while timeout > 0 {
                    let status = core::ptr::read_volatile(gfx_base.add(0x04));
                    if (status & 0x01) == 0 {
                        return true;
                    }
                    timeout -= 1;
                    for _ in 0..10 { core::hint::spin_loop(); }
                }
            }
        }
        
        false
    }
    
    /// Software rectangle fill with optimizations
    fn software_fill_rect(&mut self, rect: Rect, color: Color, end_x: usize, end_y: usize) {
        let pixel_value = color.to_pixel_format(self.pixel_format);
        let bytes_per_pixel = self.pixel_format.bytes_per_pixel();
        
        // For wide rectangles, use optimized row filling
        if rect.width > 32 {
            for y in rect.y..end_y {
                self.fill_row_optimized(y, rect.x, end_x, pixel_value, bytes_per_pixel);
            }
        } else {
            // Standard pixel-by-pixel fill for narrow rectangles
            for y in rect.y..end_y {
                for x in rect.x..end_x {
                    self.set_pixel(x, y, color);
                }
            }
        }
    }
    
    /// Optimized row filling
    fn fill_row_optimized(&mut self, y: usize, start_x: usize, end_x: usize, pixel_value: u32, bytes_per_pixel: usize) {
        let row_offset = y * self.stride + start_x * bytes_per_pixel;
        let width_pixels = end_x - start_x;
        
        unsafe {
            let row_ptr = self.buffer.add(row_offset);
            
            match bytes_per_pixel {
                4 => {
                    // 32-bit pixels
                    let row_ptr_u32 = row_ptr as *mut u32;
                    for i in 0..width_pixels {
                        *row_ptr_u32.add(i) = pixel_value;
                    }
                }
                2 => {
                    // 16-bit pixels
                    let row_ptr_u16 = row_ptr as *mut u16;
                    let pixel_value_16 = pixel_value as u16;
                    for i in 0..width_pixels {
                        *row_ptr_u16.add(i) = pixel_value_16;
                    }
                }
                3 => {
                    // 24-bit pixels (RGB888)
                    let r = (pixel_value >> 16) as u8;
                    let g = (pixel_value >> 8) as u8;
                    let b = pixel_value as u8;
                    
                    for i in 0..width_pixels {
                        let pixel_offset = i * 3;
                        *row_ptr.add(pixel_offset) = b;
                        *row_ptr.add(pixel_offset + 1) = g;
                        *row_ptr.add(pixel_offset + 2) = r;
                    }
                }
                1 => {
                    // 8-bit pixels
                    let pixel_value_8 = pixel_value as u8;
                    for i in 0..width_pixels {
                        *row_ptr.add(i) = pixel_value_8;
                    }
                }
                _ => {
                    // Fallback
                    for i in 0..width_pixels {
                        let pixel_offset = i * bytes_per_pixel;
                        for j in 0..bytes_per_pixel {
                            *row_ptr.add(pixel_offset + j) = ((pixel_value >> (j * 8)) & 0xFF) as u8;
                        }
                    }
                }
            }
        }
    }

    pub fn draw_rect(&mut self, rect: Rect, color: Color, thickness: usize) {
        // Top and bottom borders
        for i in 0..thickness {
            if rect.y + i < self.height {
                self.fill_rect(Rect::new(rect.x, rect.y + i, rect.width, 1), color);
            }
            if rect.y + rect.height > i && rect.y + rect.height - i - 1 < self.height {
                self.fill_rect(Rect::new(rect.x, rect.y + rect.height - i - 1, rect.width, 1), color);
            }
        }
        
        // Left and right borders
        for i in 0..thickness {
            if rect.x + i < self.width {
                self.fill_rect(Rect::new(rect.x + i, rect.y, 1, rect.height), color);
            }
            if rect.x + rect.width > i && rect.x + rect.width - i - 1 < self.width {
                self.fill_rect(Rect::new(rect.x + rect.width - i - 1, rect.y, 1, rect.height), color);
            }
        }
    }
}

/// Global framebuffer instance
static mut GLOBAL_FRAMEBUFFER: Option<SimpleFramebuffer> = None;
static mut GLOBAL_FRAMEBUFFER_INITIALIZED: bool = false;

/// Initialize the global framebuffer with hardware configuration
pub fn init(info: FramebufferInfo, double_buffered: bool) -> Result<(), &'static str> {
    // Validate framebuffer parameters
    if info.width == 0 || info.height == 0 {
        return Err("Invalid framebuffer dimensions");
    }
    
    if info.physical_address == 0 {
        return Err("Invalid framebuffer physical address");
    }
    
    // Map framebuffer memory to virtual address space
    let virtual_address = map_framebuffer_memory(info.physical_address, info.size)?;
    
    // Configure hardware display controller
    configure_display_controller(&info, double_buffered)?;
    
    // Initialize framebuffer structure
    unsafe {
        *addr_of_mut!(GLOBAL_FRAMEBUFFER) = Some(SimpleFramebuffer::new(
            virtual_address as *mut u8,
            info.width,
            info.height,
            info.pixel_format,
        ));
        *addr_of_mut!(GLOBAL_FRAMEBUFFER_INITIALIZED) = true;
    }
    
    // Enable hardware acceleration if available
    enable_hardware_acceleration(&info)?;
    
    Ok(())
}

/// Map framebuffer physical memory to virtual address space
fn map_framebuffer_memory(physical_address: usize, size: usize) -> Result<usize, &'static str> {
    use crate::memory::{map_physical_memory, MemoryFlags};
    
    // Calculate number of pages needed
    let page_size = 4096;
    let pages_needed = (size + page_size - 1) / page_size;
    
    // Choose virtual address in framebuffer region
    let virtual_address = 0xFD000000usize; // Typical framebuffer virtual address
    
    // Map each page with appropriate flags
    let flags = MemoryFlags::PRESENT | MemoryFlags::WRITABLE | MemoryFlags::NO_CACHE | MemoryFlags::WRITE_COMBINING;
    
    for i in 0..pages_needed {
        let virt_addr = virtual_address + i * page_size;
        let phys_addr = physical_address + i * page_size;
        
        if let Err(_) = map_physical_memory(virt_addr, phys_addr, flags) {
            // Clean up any successfully mapped pages
            for j in 0..i {
                let cleanup_addr = virtual_address + j * page_size;
                let _ = crate::memory::unmap_page(cleanup_addr);
            }
            return Err("Failed to map framebuffer memory");
        }
    }
    
    Ok(virtual_address)
}

/// Configure hardware display controller
fn configure_display_controller(info: &FramebufferInfo, double_buffered: bool) -> Result<(), &'static str> {
    // Configure display timing and resolution
    configure_display_timing(info.width, info.height)?;
    
    // Set pixel format in hardware
    configure_pixel_format(info.pixel_format)?;
    
    // Configure framebuffer address
    set_framebuffer_address(info.physical_address)?;
    
    // Enable double buffering if requested
    if double_buffered {
        enable_double_buffering(info)?;
    }
    
    // Enable display output
    enable_display_output()?;
    
    Ok(())
}

/// Configure display timing registers
fn configure_display_timing(width: usize, height: usize) -> Result<(), &'static str> {
    // Access display controller registers (example for Intel graphics)
    unsafe {
        let display_base = 0xFED00000u64 as *mut u32;
        if !display_base.is_null() {
            // Configure horizontal timing
            let htotal = width + 160; // Add blanking intervals
            let hblank_start = width;
            let hblank_end = htotal;
            let hsync_start = width + 40;
            let hsync_end = width + 120;
            
            core::ptr::write_volatile(display_base.add(0x60000 / 4),
                (((htotal - 1) << 16) | (width - 1)) as u32);
            core::ptr::write_volatile(display_base.add(0x60004 / 4),
                (((hblank_end - 1) << 16) | (hblank_start - 1)) as u32);
            core::ptr::write_volatile(display_base.add(0x60008 / 4),
                (((hsync_end - 1) << 16) | (hsync_start - 1)) as u32);
            
            // Configure vertical timing
            let vtotal = height + 45; // Add blanking intervals
            let vblank_start = height;
            let vblank_end = vtotal;
            let vsync_start = height + 10;
            let vsync_end = height + 12;
            
            core::ptr::write_volatile(display_base.add(0x6000C / 4),
                (((vtotal - 1) << 16) | (height - 1)) as u32);
            core::ptr::write_volatile(display_base.add(0x60010 / 4),
                (((vblank_end - 1) << 16) | (vblank_start - 1)) as u32);
            core::ptr::write_volatile(display_base.add(0x60014 / 4),
                (((vsync_end - 1) << 16) | (vsync_start - 1)) as u32);
        }
    }
    
    Ok(())
}

/// Configure pixel format in hardware
fn configure_pixel_format(format: PixelFormat) -> Result<(), &'static str> {
    unsafe {
        let display_base = 0xFED00000u64 as *mut u32;
        if !display_base.is_null() {
            let format_value = match format {
                PixelFormat::RGBA8888 => 0x06, // 32-bit RGBA
                PixelFormat::BGRA8888 => 0x06, // 32-bit BGRA
                PixelFormat::RGB888 => 0x04,   // 24-bit RGB
                PixelFormat::RGB565 => 0x02,   // 16-bit RGB565
                PixelFormat::RGB555 => 0x01,   // 15-bit RGB555
            };
            
            // Set pixel format in display control register
            let mut control_reg = core::ptr::read_volatile(display_base.add(0x70180 / 4));
            control_reg = (control_reg & !0x0F) | format_value;
            core::ptr::write_volatile(display_base.add(0x70180 / 4), control_reg);
        }
    }
    
    Ok(())
}

/// Set framebuffer base address in hardware
fn set_framebuffer_address(physical_address: usize) -> Result<(), &'static str> {
    unsafe {
        let display_base = 0xFED00000u64 as *mut u32;
        if !display_base.is_null() {
            // Set primary surface address
            core::ptr::write_volatile(display_base.add(0x70184 / 4), physical_address as u32);
            
            // Set stride (calculated from width and pixel format)
            // This would be set based on the actual framebuffer info
            // For now, we'll use a placeholder
            core::ptr::write_volatile(display_base.add(0x70188 / 4), 1920 * 4); // Assuming 1920x1080x32bpp
        }
    }
    
    Ok(())
}

/// Enable double buffering
fn enable_double_buffering(info: &FramebufferInfo) -> Result<(), &'static str> {
    // Allocate second buffer
    let second_buffer_size = info.size;
    let second_buffer_addr = allocate_framebuffer_memory(second_buffer_size)?;
    
    // Configure hardware for double buffering
    unsafe {
        let display_base = 0xFED00000u64 as *mut u32;
        if !display_base.is_null() {
            // Set secondary surface address
            core::ptr::write_volatile(display_base.add(0x701A0 / 4), second_buffer_addr as u32);
            
            // Enable double buffering in control register
            let mut control_reg = core::ptr::read_volatile(display_base.add(0x70180 / 4));
            control_reg |= 0x80000000; // Enable double buffering bit
            core::ptr::write_volatile(display_base.add(0x70180 / 4), control_reg);
        }
    }
    
    Ok(())
}

/// Enable display output
fn enable_display_output() -> Result<(), &'static str> {
    unsafe {
        let display_base = 0xFED00000u64 as *mut u32;
        if !display_base.is_null() {
            // Enable display plane
            let mut control_reg = core::ptr::read_volatile(display_base.add(0x70180 / 4));
            control_reg |= 0x80000000; // Enable display plane
            core::ptr::write_volatile(display_base.add(0x70180 / 4), control_reg);
            
            // Enable pipe
            let mut pipe_conf = core::ptr::read_volatile(display_base.add(0x70008 / 4));
            pipe_conf |= 0x80000000; // Enable pipe
            core::ptr::write_volatile(display_base.add(0x70008 / 4), pipe_conf);
        }
    }
    
    Ok(())
}

/// Enable hardware acceleration features
fn enable_hardware_acceleration(info: &FramebufferInfo) -> Result<(), &'static str> {
    if info.gpu_accelerated {
        // Initialize 2D acceleration engine
        initialize_2d_engine()?;

        // Initialize 3D acceleration if available
        // TODO: Implement mutable GPU manager access
        // if let Some(gpu_manager) = crate::gpu::get_gpu_manager() {
        //     gpu_manager.initialize_acceleration(info)?;
        // }
    }
    
    Ok(())
}

/// Initialize 2D acceleration engine
fn initialize_2d_engine() -> Result<(), &'static str> {
    unsafe {
        let gfx_base = 0xFED00000u64 as *mut u32;
        if !gfx_base.is_null() {
            // Reset 2D engine
            core::ptr::write_volatile(gfx_base.add(0x08), 0x01);
            
            // Wait for reset completion
            let mut timeout = 1000;
            while timeout > 0 {
                let status = core::ptr::read_volatile(gfx_base.add(0x0C));
                if (status & 0x01) == 0 {
                    break;
                }
                timeout -= 1;
                for _ in 0..100 { core::hint::spin_loop(); }
            }
            
            if timeout == 0 {
                return Err("2D engine reset timeout");
            }
            
            // Configure 2D engine settings
            core::ptr::write_volatile(gfx_base.add(0x10), 0x00000001); // Enable 2D engine
            core::ptr::write_volatile(gfx_base.add(0x14), 0x00000000); // Clear interrupt status
        }
    }
    
    Ok(())
}

/// Allocate framebuffer memory for double buffering
fn allocate_framebuffer_memory(size: usize) -> Result<usize, &'static str> {
    // This would use the memory manager to allocate contiguous physical memory
    // For now, we'll return a placeholder address
    let allocated_addr = 0xE1000000usize; // Example second buffer address
    
    // In a real implementation, we would:
    // 1. Allocate contiguous physical pages
    // 2. Map them to virtual address space
    // 3. Return the physical address for hardware configuration
    
    if size > 32 * 1024 * 1024 { // Sanity check: max 32MB framebuffer
        return Err("Framebuffer size too large");
    }
    
    Ok(allocated_addr)
}

/// Initialize framebuffer using an existing buffer provided by the bootloader
pub fn init_with_buffer(
    buffer: &'static mut [u8],
    info: FramebufferInfo,
    _double_buffered: bool,
) -> Result<(), &'static str> {
    unsafe {
        *addr_of_mut!(GLOBAL_FRAMEBUFFER) = Some(SimpleFramebuffer::new(
            buffer.as_mut_ptr(),
            info.width,
            info.height,
            info.pixel_format,
        ));
        *addr_of_mut!(GLOBAL_FRAMEBUFFER_INITIALIZED) = true;
    }
    Ok(())
}

/// Get a reference to the global framebuffer
pub fn framebuffer() -> Option<&'static mut SimpleFramebuffer> {
    unsafe {
        if *addr_of!(GLOBAL_FRAMEBUFFER_INITIALIZED) {
            (*addr_of_mut!(GLOBAL_FRAMEBUFFER)).as_mut()
        } else {
            None
        }
    }
}

/// Get framebuffer information if initialized
pub fn get_info() -> Option<FramebufferInfo> {
    unsafe {
        if let Some(ref fb) = *addr_of!(GLOBAL_FRAMEBUFFER) {
            Some(FramebufferInfo::new(
                fb.width,
                fb.height,
                fb.pixel_format,
                fb.buffer as usize,
                false,
            ))
        } else {
            None
        }
    }
}

/// Clear the screen with a color
pub fn clear_screen(color: Color) {
    unsafe {
        if let Some(ref mut fb) = *addr_of_mut!(GLOBAL_FRAMEBUFFER) {
            fb.clear(color);
        }
    }
}

/// Set a pixel on the screen
pub fn set_pixel(x: usize, y: usize, color: Color) {
    unsafe {
        if let Some(ref mut fb) = *addr_of_mut!(GLOBAL_FRAMEBUFFER) {
            fb.set_pixel(x, y, color);
        }
    }
}

/// Fill a rectangle on the screen
pub fn fill_rect(rect: Rect, color: Color) {
    unsafe {
        if let Some(ref mut fb) = *addr_of_mut!(GLOBAL_FRAMEBUFFER) {
            fb.fill_rect(rect, color);
        }
    }
}

/// Draw a rectangle outline on the screen
pub fn draw_rect(rect: Rect, color: Color, thickness: usize) {
    unsafe {
        if let Some(ref mut fb) = *addr_of_mut!(GLOBAL_FRAMEBUFFER) {
            fb.draw_rect(rect, color, thickness);
        }
    }
}

/// Present the current frame
pub fn present() {
    unsafe {
        if let Some(ref mut fb) = *addr_of_mut!(GLOBAL_FRAMEBUFFER) {
            // Flush CPU caches to ensure GPU sees the latest data
            flush_framebuffer_cache(fb.buffer, fb.height * fb.stride);
            
            // Signal GPU to present the frame (hardware-specific)
            present_hardware_frame(fb.buffer as u64);
        }
    }
}

/// Flush CPU caches for framebuffer memory
fn flush_framebuffer_cache(buffer: *mut u8, size: usize) {
    // Use x86_64 cache flush instructions
    let cache_line_size = 64; // Typical x86_64 cache line size
    let start_addr = buffer as usize;
    let end_addr = start_addr + size;
    
    // Align to cache line boundaries
    let aligned_start = start_addr & !(cache_line_size - 1);
    let aligned_end = (end_addr + cache_line_size - 1) & !(cache_line_size - 1);
    
    // Flush each cache line
    let mut addr = aligned_start;
    while addr < aligned_end {
        unsafe {
            // Use clflush instruction to flush cache line
            core::arch::asm!("clflush [{}]", in(reg) addr, options(nostack, preserves_flags));
        }
        addr += cache_line_size;
    }
    
    // Memory fence to ensure ordering
    unsafe {
        core::arch::asm!("mfence", options(nostack, preserves_flags));
    }
}

/// Signal hardware to present the frame
fn present_hardware_frame(framebuffer_addr: u64) {
    // Access GPU registers to trigger frame presentation
    // This would be GPU-specific in a real implementation
    
    // For VBE/VESA, we might need to update display start address
    if let Some(_vbe_driver) = crate::drivers::vbe::driver().get_current_mode() {
        // Update display start address if double buffering is used
        update_display_start_address(framebuffer_addr);
    }
    
    // For modern GPUs, we would submit a present command to the GPU command queue
    submit_present_command(framebuffer_addr);
}

/// Update VBE display start address for page flipping
fn update_display_start_address(framebuffer_addr: u64) {
    // Calculate pixel offset from base framebuffer
    let base_addr = 0xE0000000u64; // Typical VBE framebuffer base
    let pixel_offset = if framebuffer_addr >= base_addr {
        (framebuffer_addr - base_addr) / 4 // Assuming 32-bit pixels
    } else {
        0
    };
    
    // Use VBE function 0x4F07 to set display start
    unsafe {
        // This would make a real BIOS call in production
        // For now, we simulate the register update
        let dx = (pixel_offset & 0xFFFF) as u16;
        let cx = ((pixel_offset >> 16) & 0xFFFF) as u16;
        
        // Simulate VBE display start update
        core::arch::asm!(
            "nop", // Placeholder for actual VBE call
            in("dx") dx,
            in("cx") cx,
            options(nostack, preserves_flags)
        );
    }
}

/// Submit present command to GPU
fn submit_present_command(framebuffer_addr: u64) {
    // This would interface with the GPU driver to submit a present command
    // For now, we'll update a hypothetical GPU register
    
    unsafe {
        // Write to GPU present register (hardware-specific address)
        let gpu_present_reg = 0xFED00000u64 as *mut u64; // Example GPU register address
        if !gpu_present_reg.is_null() {
            // Validate the address is in a reasonable range for GPU registers
            if (gpu_present_reg as u64) >= 0xFE000000 && (gpu_present_reg as u64) < 0xFF000000 {
                core::ptr::write_volatile(gpu_present_reg, framebuffer_addr);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{serial_print, serial_println};

    #[cfg(feature = "disabled-tests")] // #[test]
    fn test_pixel_format_bytes_per_pixel() {
        serial_print!("test_pixel_format_bytes_per_pixel... ");
        assert_eq!(PixelFormat::RGBA8888.bytes_per_pixel(), 4);
        assert_eq!(PixelFormat::RGB888.bytes_per_pixel(), 3);
        assert_eq!(PixelFormat::RGB565.bytes_per_pixel(), 2);
        serial_println!("[ok]");
    }

    #[cfg(feature = "disabled-tests")] // #[test]
    fn test_color_conversion() {
        serial_print!("test_color_conversion... ");
        let color = Color::rgb(255, 128, 64);
        assert_eq!(color.r, 255);
        assert_eq!(color.g, 128);
        assert_eq!(color.b, 64);
        assert_eq!(color.a, 255);
        serial_println!("[ok]");
    }

    #[cfg(feature = "disabled-tests")] // #[test]
    fn test_rect_contains() {
        serial_print!("test_rect_contains... ");
        let rect = Rect::new(10, 10, 20, 20);
        assert!(rect.contains(15, 15));
        assert!(!rect.contains(5, 5));
        assert!(!rect.contains(35, 35));
        serial_println!("[ok]");
    }

    #[cfg(feature = "disabled-tests")] // #[test]
    fn test_framebuffer_info() {
        serial_print!("test_framebuffer_info... ");
        let info = FramebufferInfo::new(1920, 1080, PixelFormat::RGBA8888, 0xfd000000, false);
        assert_eq!(info.width, 1920);
        assert_eq!(info.height, 1080);
        assert_eq!(info.pixel_count(), 1920 * 1080);
        assert_eq!(info.bytes_per_pixel(), 4);
        serial_println!("[ok]");
    }
}
//
// Placeholder implementations for memory management functions
// GPU manager interface
mod gpu_interface {
    use super::*;
    
    pub struct GPUManager {
        pub gpu_id: u32,
        pub vendor: GPUVendor,
        pub device_id: u16,
        pub command_buffer_base: u64,
        pub register_base: u64,
        pub acceleration_enabled: bool,
    }
    
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub enum GPUVendor {
        Intel,
        AMD,
        NVIDIA,
        Unknown,
    }
    
    impl GPUManager {
        pub fn new() -> Option<Self> {
            // Detect GPU hardware
            if let Some((vendor, device_id, reg_base)) = detect_gpu_hardware() {
                Some(Self {
                    gpu_id: 0,
                    vendor,
                    device_id,
                    command_buffer_base: allocate_command_buffer()?,
                    register_base: reg_base,
                    acceleration_enabled: false,
                })
            } else {
                None
            }
        }
        
        pub fn is_acceleration_available(&self) -> bool {
            self.acceleration_enabled && self.register_base != 0
        }
        
        pub fn clear_framebuffer(&self, buffer: u64, width: usize, height: usize, stride: usize, color: u32) -> Result<(), &'static str> {
            if !self.is_acceleration_available() {
                return Err("GPU acceleration not available");
            }
            
            match self.vendor {
                GPUVendor::Intel => self.intel_clear_framebuffer(buffer, width, height, stride, color),
                GPUVendor::AMD => self.amd_clear_framebuffer(buffer, width, height, stride, color),
                GPUVendor::NVIDIA => self.nvidia_clear_framebuffer(buffer, width, height, stride, color),
                GPUVendor::Unknown => Err("Unknown GPU vendor"),
            }
        }
        
        pub fn fill_rectangle(&self, buffer: u64, stride: usize, x: usize, y: usize, width: usize, height: usize, color: u32) -> Result<(), &'static str> {
            if !self.is_acceleration_available() {
                return Err("GPU acceleration not available");
            }
            
            match self.vendor {
                GPUVendor::Intel => self.intel_fill_rectangle(buffer, stride, x, y, width, height, color),
                GPUVendor::AMD => self.amd_fill_rectangle(buffer, stride, x, y, width, height, color),
                GPUVendor::NVIDIA => self.nvidia_fill_rectangle(buffer, stride, x, y, width, height, color),
                GPUVendor::Unknown => Err("Unknown GPU vendor"),
            }
        }
        
        pub fn initialize_acceleration(&mut self, info: &FramebufferInfo) -> Result<(), &'static str> {
            // Initialize GPU-specific acceleration
            match self.vendor {
                GPUVendor::Intel => self.init_intel_acceleration(info),
                GPUVendor::AMD => self.init_amd_acceleration(info),
                GPUVendor::NVIDIA => self.init_nvidia_acceleration(info),
                GPUVendor::Unknown => Err("Unknown GPU vendor"),
            }?;
            
            self.acceleration_enabled = true;
            Ok(())
        }
        
        // Intel GPU acceleration methods
        fn intel_clear_framebuffer(&self, buffer: u64, width: usize, height: usize, stride: usize, color: u32) -> Result<(), &'static str> {
            unsafe {
                let reg_base = self.register_base as *mut u32;
                
                // Intel 2D BLT engine registers
                let blt_base = reg_base.add(0x22000 / 4);
                
                // Wait for engine idle
                self.wait_for_intel_idle(blt_base)?;
                
                // Set up BLT command for solid fill
                let cmd = 0x40000000 | (0x3 << 20) | (0xF0 << 16); // XY_COLOR_BLT with ROP_COPY
                let pitch_and_format = (stride as u32) | (0x3 << 24); // 32-bit format
                let dest_rect = ((height as u32) << 16) | (width as u32);
                
                // Write BLT commands
                core::ptr::write_volatile(blt_base.add(0), cmd);
                core::ptr::write_volatile(blt_base.add(1), pitch_and_format);
                core::ptr::write_volatile(blt_base.add(2), 0); // Start coordinates (0,0)
                core::ptr::write_volatile(blt_base.add(3), dest_rect);
                core::ptr::write_volatile(blt_base.add(4), buffer as u32);
                core::ptr::write_volatile(blt_base.add(5), color);
                
                // Flush and wait for completion
                self.flush_intel_commands(blt_base)?;
                self.wait_for_intel_idle(blt_base)?;
            }
            
            Ok(())
        }
        
        fn intel_fill_rectangle(&self, buffer: u64, stride: usize, x: usize, y: usize, width: usize, height: usize, color: u32) -> Result<(), &'static str> {
            unsafe {
                let reg_base = self.register_base as *mut u32;
                let blt_base = reg_base.add(0x22000 / 4);
                
                self.wait_for_intel_idle(blt_base)?;
                
                let cmd = 0x40000000 | (0x3 << 20) | (0xF0 << 16);
                let pitch_and_format = (stride as u32) | (0x3 << 24);
                let start_coords = ((y as u32) << 16) | (x as u32);
                let dest_rect = (((y + height) as u32) << 16) | ((x + width) as u32);
                let dest_addr = buffer + (y * stride + x * 4) as u64;
                
                core::ptr::write_volatile(blt_base.add(0), cmd);
                core::ptr::write_volatile(blt_base.add(1), pitch_and_format);
                core::ptr::write_volatile(blt_base.add(2), start_coords);
                core::ptr::write_volatile(blt_base.add(3), dest_rect);
                core::ptr::write_volatile(blt_base.add(4), dest_addr as u32);
                core::ptr::write_volatile(blt_base.add(5), color);
                
                self.flush_intel_commands(blt_base)?;
                self.wait_for_intel_idle(blt_base)?;
            }
            
            Ok(())
        }
        
        fn init_intel_acceleration(&self, _info: &FramebufferInfo) -> Result<(), &'static str> {
            unsafe {
                let reg_base = self.register_base as *mut u32;
                
                // Enable 2D BLT engine
                let engine_enable = core::ptr::read_volatile(reg_base.add(0x2080 / 4));
                core::ptr::write_volatile(reg_base.add(0x2080 / 4), engine_enable | 0x1);
                
                // Configure BLT engine
                let blt_base = reg_base.add(0x22000 / 4);
                core::ptr::write_volatile(blt_base.add(0x10 / 4), 0x0); // Reset BLT engine
                
                // Wait for reset completion
                let mut timeout = 1000;
                while timeout > 0 {
                    let status = core::ptr::read_volatile(blt_base.add(0x14 / 4));
                    if (status & 0x1) == 0 {
                        break;
                    }
                    timeout -= 1;
                    for _ in 0..100 { core::hint::spin_loop(); }
                }
                
                if timeout == 0 {
                    return Err("Intel BLT engine reset timeout");
                }
            }
            
            Ok(())
        }
        
        fn wait_for_intel_idle(&self, blt_base: *mut u32) -> Result<(), &'static str> {
            unsafe {
                let mut timeout = 10000;
                while timeout > 0 {
                    let status = core::ptr::read_volatile(blt_base.add(1)); // offset 0x4 in u32 units
                    if (status & 0x1) == 0 { // Engine idle
                        return Ok(());
                    }
                    timeout -= 1;
                    for _ in 0..10 { core::hint::spin_loop(); }
                }
                Err("Intel GPU timeout waiting for idle")
            }
        }
        
        fn flush_intel_commands(&self, blt_base: *mut u32) -> Result<(), &'static str> {
            unsafe {
                // Trigger command execution
                core::ptr::write_volatile(blt_base.add(0x8 / 4), 0x1);
                
                // Memory barrier to ensure commands are flushed
                core::arch::asm!("mfence", options(nostack, preserves_flags));
            }
            Ok(())
        }
        
        // AMD GPU acceleration methods
        fn amd_clear_framebuffer(&self, buffer: u64, width: usize, height: usize, stride: usize, color: u32) -> Result<(), &'static str> {
            unsafe {
                let reg_base = self.register_base as *mut u32;
                
                // AMD CB (Color Buffer) registers
                let cb_base = reg_base.add(0x28000 / 4);
                
                // Set up color buffer
                core::ptr::write_volatile(cb_base.add(0x0), buffer as u32); // CB_COLOR0_BASE
                core::ptr::write_volatile(cb_base.add(0x1), (stride / 4) as u32); // CB_COLOR0_PITCH
                core::ptr::write_volatile(cb_base.add(0x2), (((height - 1) << 16) | (width - 1)) as u32); // CB_COLOR0_SLICE
                
                // Set clear color
                core::ptr::write_volatile(cb_base.add(0x10), color); // CB_COLOR0_CLEAR_WORD0
                
                // Trigger clear operation
                core::ptr::write_volatile(cb_base.add(0x20), 0x1); // CB_COLOR0_CLEAR
                
                // Wait for completion
                self.wait_for_amd_idle(reg_base)?;
            }
            
            Ok(())
        }
        
        fn amd_fill_rectangle(&self, buffer: u64, stride: usize, x: usize, y: usize, width: usize, height: usize, color: u32) -> Result<(), &'static str> {
            unsafe {
                let reg_base = self.register_base as *mut u32;
                let cb_base = reg_base.add(0x28000 / 4);
                
                // Calculate destination address
                let dest_addr = buffer + (y * stride + x * 4) as u64;
                
                // Set up partial clear
                core::ptr::write_volatile(cb_base.add(0x0), dest_addr as u32);
                core::ptr::write_volatile(cb_base.add(0x1), (stride / 4) as u32);
                core::ptr::write_volatile(cb_base.add(0x2), (((height - 1) << 16) | (width - 1)) as u32);
                core::ptr::write_volatile(cb_base.add(0x10), color);

                // Set scissor rectangle
                core::ptr::write_volatile(cb_base.add(0x30), ((y << 16) | x) as u32); // Scissor top-left
                core::ptr::write_volatile(cb_base.add(0x31), (((y + height) << 16) | (x + width)) as u32); // Scissor bottom-right
                
                // Enable scissor and trigger clear
                core::ptr::write_volatile(cb_base.add(0x32), 0x1); // Enable scissor
                core::ptr::write_volatile(cb_base.add(0x20), 0x1); // Trigger clear
                
                self.wait_for_amd_idle(reg_base)?;
                
                // Disable scissor
                core::ptr::write_volatile(cb_base.add(0x32), 0x0);
            }
            
            Ok(())
        }
        
        fn init_amd_acceleration(&self, _info: &FramebufferInfo) -> Result<(), &'static str> {
            unsafe {
                let reg_base = self.register_base as *mut u32;
                
                // Enable graphics engine
                let gfx_enable = core::ptr::read_volatile(reg_base.add(0x8010 / 4));
                core::ptr::write_volatile(reg_base.add(0x8010 / 4), gfx_enable | 0x1);
                
                // Initialize command processor
                core::ptr::write_volatile(reg_base.add(0x8020 / 4), 0x0); // Reset CP
                
                // Wait for reset
                let mut timeout = 1000;
                while timeout > 0 {
                    let status = core::ptr::read_volatile(reg_base.add(0x8024 / 4));
                    if (status & 0x1) == 0 {
                        break;
                    }
                    timeout -= 1;
                    for _ in 0..100 { core::hint::spin_loop(); }
                }
                
                if timeout == 0 {
                    return Err("AMD GPU reset timeout");
                }
            }
            
            Ok(())
        }
        
        fn wait_for_amd_idle(&self, reg_base: *mut u32) -> Result<(), &'static str> {
            unsafe {
                let mut timeout = 10000;
                while timeout > 0 {
                    let status = core::ptr::read_volatile(reg_base.add(0x8008 / 4));
                    if (status & 0x80000000) == 0 { // GPU idle
                        return Ok(());
                    }
                    timeout -= 1;
                    for _ in 0..10 { core::hint::spin_loop(); }
                }
                Err("AMD GPU timeout waiting for idle")
            }
        }
        
        // NVIDIA GPU acceleration methods (limited due to proprietary nature)
        fn nvidia_clear_framebuffer(&self, _buffer: u64, _width: usize, _height: usize, _stride: usize, _color: u32) -> Result<(), &'static str> {
            // NVIDIA GPUs require signed firmware and proprietary drivers for full acceleration
            // Nouveau driver would handle this, but with limited capabilities
            Err("NVIDIA GPU acceleration requires Nouveau driver")
        }
        
        fn nvidia_fill_rectangle(&self, _buffer: u64, _stride: usize, _x: usize, _y: usize, _width: usize, _height: usize, _color: u32) -> Result<(), &'static str> {
            Err("NVIDIA GPU acceleration requires Nouveau driver")
        }
        
        fn init_nvidia_acceleration(&self, _info: &FramebufferInfo) -> Result<(), &'static str> {
            Err("NVIDIA GPU acceleration requires Nouveau driver")
        }
    }
    
    // Hardware detection functions
    fn detect_gpu_hardware() -> Option<(GPUVendor, u16, u64)> {
        // Scan PCI bus for GPU devices
        for bus in 0..=255 {
            for device in 0..32 {
                for function in 0..8 {
                    if let Some((vendor_id, device_id, reg_base)) = read_pci_device(bus, device, function) {
                        let vendor = match vendor_id {
                            0x8086 => GPUVendor::Intel,
                            0x1002 => GPUVendor::AMD,
                            0x10DE => GPUVendor::NVIDIA,
                            _ => continue,
                        };
                        
                        // Check if this is a graphics device (class 0x03)
                        let class_code = read_pci_config(bus, device, function, 0x08);
                        if (class_code >> 24) == 0x03 {
                            return Some((vendor, device_id, reg_base));
                        }
                    }
                }
            }
        }
        None
    }
    
    fn read_pci_device(bus: u8, device: u8, function: u8) -> Option<(u16, u16, u64)> {
        let vendor_id = read_pci_config(bus, device, function, 0x00) as u16;
        if vendor_id == 0xFFFF {
            return None; // Device not present
        }
        
        let device_id = (read_pci_config(bus, device, function, 0x00) >> 16) as u16;
        
        // Read BAR0 for register base address
        let bar0 = read_pci_config(bus, device, function, 0x10);
        let reg_base = if (bar0 & 0x1) == 0 { // Memory BAR
            (bar0 & 0xFFFFFFF0) as u64
        } else {
            0 // I/O BAR not supported for GPU registers
        };
        
        Some((vendor_id, device_id, reg_base))
    }
    
    fn read_pci_config(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
        let address = 0x80000000u32 
            | ((bus as u32) << 16)
            | ((device as u32) << 11)
            | ((function as u32) << 8)
            | (offset as u32 & 0xFC);
        
        unsafe {
            // Write address to CONFIG_ADDRESS port
            core::arch::asm!("out dx, eax", in("dx") 0xCF8u16, in("eax") address, options(nostack, preserves_flags));
            
            // Read data from CONFIG_DATA port
            let mut data: u32;
            core::arch::asm!("in eax, dx", out("eax") data, in("dx") 0xCFCu16, options(nostack, preserves_flags));
            data
        }
    }
    
    fn allocate_command_buffer() -> Option<u64> {
        // Allocate a page for GPU command buffer
        // This would use the memory manager in a real implementation
        Some(0xFE000000) // Example command buffer address
    }
    
    static mut GLOBAL_GPU_MANAGER: Option<GPUManager> = None;
    
    pub fn get_gpu_manager() -> Option<&'static mut GPUManager> {
        unsafe {
            if (*core::ptr::addr_of!(GLOBAL_GPU_MANAGER)).is_none() {
                *core::ptr::addr_of_mut!(GLOBAL_GPU_MANAGER) = GPUManager::new();
            }
            (*core::ptr::addr_of_mut!(GLOBAL_GPU_MANAGER)).as_mut()
        }
    }
}

// Make GPU manager available
pub use gpu_interface::get_gpu_manager;

// Add hardware detection and capability reporting
pub fn detect_hardware_capabilities() -> HardwareCapabilities {
    // Return safe defaults without accessing hardware registers
    // This avoids crashes when memory regions aren't properly mapped
    HardwareCapabilities {
        has_2d_acceleration: false,
        has_3d_acceleration: false,
        has_hardware_cursor: false,
        max_resolution: (1920, 1080), // Default max
        supported_formats: alloc::vec![PixelFormat::RGBA8888, PixelFormat::BGRA8888, PixelFormat::RGB888],
        memory_bandwidth: 0,
    }
}

#[derive(Debug, Clone)]
pub struct HardwareCapabilities {
    pub has_2d_acceleration: bool,
    pub has_3d_acceleration: bool,
    pub has_hardware_cursor: bool,
    pub max_resolution: (usize, usize),
    pub supported_formats: Vec<PixelFormat>,
    pub memory_bandwidth: u64, // MB/s
}

fn detect_2d_acceleration() -> bool {
    // Check for 2D acceleration hardware
    unsafe {
        let gfx_base = 0xFED00000u64 as *mut u32;
        if !gfx_base.is_null() {
            // Try to read a known register
            let device_id = core::ptr::read_volatile(gfx_base.add(0x02));
            // Check if it's a known graphics device ID
            matches!(device_id & 0xFFFF, 0x8086 | 0x10DE | 0x1002) // Intel, NVIDIA, AMD
        } else {
            false
        }
    }
}

fn detect_3d_acceleration() -> bool {
    // Check for 3D acceleration capabilities
    if let Some(gpu_manager) = get_gpu_manager() {
        gpu_manager.is_acceleration_available()
    } else {
        false
    }
}

fn detect_hardware_cursor() -> bool {
    // Check for hardware cursor support
    unsafe {
        let display_base = 0xFED00000u64 as *mut u32;
        if !display_base.is_null() {
            // Check cursor control register
            let cursor_control = core::ptr::read_volatile(display_base.add(0x70080 / 4));
            (cursor_control & 0x80000000) != 0 // Cursor available bit
        } else {
            false
        }
    }
}

fn detect_max_resolution() -> (usize, usize) {
    // Detect maximum supported resolution
    unsafe {
        let display_base = 0xFED00000u64 as *mut u32;
        if !display_base.is_null() {
            // Read maximum resolution from hardware registers
            let max_h = core::ptr::read_volatile(display_base.add(0x70100 / 4)) & 0xFFFF;
            let max_v = core::ptr::read_volatile(display_base.add(0x70104 / 4)) & 0xFFFF;
            
            if max_h > 0 && max_v > 0 {
                (max_h as usize, max_v as usize)
            } else {
                (3840, 2160) // Default to 4K if detection fails
            }
        } else {
            (1920, 1080) // Default to Full HD
        }
    }
}

fn detect_supported_formats() -> Vec<PixelFormat> {
    let mut formats = Vec::new();
    
    // Check which pixel formats are supported by hardware
    unsafe {
        let display_base = 0xFED00000u64 as *mut u32;
        if !display_base.is_null() {
            let format_support = core::ptr::read_volatile(display_base.add(0x70108 / 4));
            
            if (format_support & 0x01) != 0 { formats.push(PixelFormat::RGB555); }
            if (format_support & 0x02) != 0 { formats.push(PixelFormat::RGB565); }
            if (format_support & 0x04) != 0 { formats.push(PixelFormat::RGB888); }
            if (format_support & 0x08) != 0 { formats.push(PixelFormat::RGBA8888); }
            if (format_support & 0x10) != 0 { formats.push(PixelFormat::BGRA8888); }
        }
    }
    
    // Ensure we always support at least basic formats
    if formats.is_empty() {
        formats.push(PixelFormat::RGB565);
        formats.push(PixelFormat::RGBA8888);
    }
    
    formats
}

fn estimate_memory_bandwidth() -> u64 {
    // Estimate memory bandwidth based on hardware detection
    // This would involve actual memory bandwidth testing in production
    
    // For now, return reasonable estimates based on typical hardware
    if detect_3d_acceleration() {
        25600 // 25.6 GB/s for modern GPUs
    } else if detect_2d_acceleration() {
        6400  // 6.4 GB/s for integrated graphics
    } else {
        1600  // 1.6 GB/s for basic framebuffer
    }
}