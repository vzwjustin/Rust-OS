//! # Bochs/QEMU VBE I/O Port Driver
//!
//! Programs VBE graphics modes via I/O ports (0x1CE/0x1CF) without BIOS interrupts.
//! Works in long mode on QEMU's Bochs VBE extension.

use crate::graphics::framebuffer::PixelFormat;
use x86_64::instructions::port::Port;

/// Raw serial write to COM1 (0x3F8) bypassing Mutex — for debugging only.
/// SAFETY: Direct I/O port access. See docs/SAFETY.md#io-port-access.
unsafe fn raw_serial_str(s: &str) {
    let mut data_port: Port<u8> = Port::new(0x3F8);
    let mut line_status: Port<u8> = Port::new(0x3FD);
    for byte in s.bytes() {
        while line_status.read() & 0x20 == 0 {}
        data_port.write(byte);
    }
}

unsafe fn raw_serial_hex(val: u64) {
    let mut buf = [0u8; 17];
    buf[16] = b'\n';
    for i in (0..16).rev() {
        let nibble = (val >> ((15 - i) * 4)) & 0xF;
        buf[i] = match nibble {
            0..=9 => b'0' + nibble as u8,
            _ => b'a' + (nibble - 10) as u8,
        };
    }
    raw_serial_str(core::str::from_utf8(&buf).unwrap_or("?"));
}

const VBE_INDEX_PORT: u16 = 0x1CE;
const VBE_DATA_PORT: u16 = 0x1CF;

const VBE_DISPI_INDEX_ID: u16 = 0x00;
const VBE_DISPI_INDEX_XRES: u16 = 0x01;
const VBE_DISPI_INDEX_YRES: u16 = 0x02;
const VBE_DISPI_INDEX_BPP: u16 = 0x03;
const VBE_DISPI_INDEX_ENABLE: u16 = 0x04;
const VBE_DISPI_INDEX_BANK: u16 = 0x05;
const VBE_DISPI_INDEX_VIRT_WIDTH: u16 = 0x06;
const VBE_DISPI_INDEX_VIRT_HEIGHT: u16 = 0x07;
const VBE_DISPI_INDEX_X_OFFSET: u16 = 0x08;
const VBE_DISPI_INDEX_Y_OFFSET: u16 = 0x09;
const VBE_DISPI_INDEX_VIDEO_MEMORY_64K: u16 = 0x0A;

const VBE_DISPI_DISABLED: u16 = 0x00;
const VBE_DISPI_ENABLED: u16 = 0x01;
const VBE_DISPI_LFB_ENABLED: u16 = 0x40;
const VBE_DISPI_NOCLEARMEM: u16 = 0x80;

const QEMU_VBE_FRAMEBUFFER_PHYS: u64 = 0xE0000000;

#[derive(Debug, Clone, Copy)]
pub struct VbeIoMode {
    pub width: u16,
    pub height: u16,
    pub bpp: u8,
    pub framebuffer_phys: u64,
    pub pitch: usize,
    pub pixel_format: PixelFormat,
}

fn vbe_read(index: u16) -> u16 {
    let mut idx_port = Port::new(VBE_INDEX_PORT);
    let mut data_port = Port::new(VBE_DATA_PORT);
    unsafe {
        idx_port.write(index);
        data_port.read()
    }
}

fn vbe_write(index: u16, value: u16) {
    let mut idx_port = Port::new(VBE_INDEX_PORT);
    let mut data_port = Port::new(VBE_DATA_PORT);
    unsafe {
        idx_port.write(index);
        data_port.write(value);
    }
}

pub fn detect_vbe() -> bool {
    let version = vbe_read(VBE_DISPI_INDEX_ID);
    version >= 0xB0C0 || version >= 0x0200
}

pub fn get_vbe_version() -> u16 {
    vbe_read(VBE_DISPI_INDEX_ID)
}

pub fn get_video_memory_bytes() -> u64 {
    let blocks = vbe_read(VBE_DISPI_INDEX_VIDEO_MEMORY_64K);
    blocks as u64 * 64 * 1024
}

pub fn set_mode(width: u16, height: u16, bpp: u8) -> Result<VbeIoMode, &'static str> {
    if width == 0 || height == 0 {
        return Err("Invalid resolution");
    }

    let version = vbe_read(VBE_DISPI_INDEX_ID);
    unsafe {
        raw_serial_str("vbe_io: set_mode version=");
        raw_serial_hex(version as u64);
    }
    if version < 0x0200 {
        return Err("VBE 2.0+ required");
    }

    let vram = get_video_memory_bytes();
    unsafe {
        raw_serial_str("vbe_io: set_mode vram=");
        raw_serial_hex(vram);
    }

    let pitch = width as usize * (bpp as usize / 8);
    let fb_size = pitch * height as usize;
    if fb_size as u64 > vram {
        return Err("Insufficient video memory for requested mode");
    }

    unsafe {
        raw_serial_str("vbe_io: set_mode begin writes\n");
    }

    vbe_write(VBE_DISPI_INDEX_ENABLE, VBE_DISPI_DISABLED);
    unsafe {
        raw_serial_str("vbe_io: disabled\n");
    }
    vbe_write(VBE_DISPI_INDEX_XRES, width);
    unsafe {
        raw_serial_str("vbe_io: xres set\n");
    }
    vbe_write(VBE_DISPI_INDEX_YRES, height);
    unsafe {
        raw_serial_str("vbe_io: yres set\n");
    }
    vbe_write(VBE_DISPI_INDEX_BPP, bpp as u16);
    unsafe {
        raw_serial_str("vbe_io: bpp set\n");
    }
    vbe_write(VBE_DISPI_INDEX_X_OFFSET, 0);
    vbe_write(VBE_DISPI_INDEX_Y_OFFSET, 0);
    vbe_write(VBE_DISPI_INDEX_VIRT_WIDTH, width);
    vbe_write(VBE_DISPI_INDEX_VIRT_HEIGHT, height);
    unsafe {
        raw_serial_str("vbe_io: offsets/virt set\n");
    }

    vbe_write(
        VBE_DISPI_INDEX_ENABLE,
        VBE_DISPI_ENABLED | VBE_DISPI_LFB_ENABLED,
    );
    unsafe {
        raw_serial_str("vbe_io: enabled LFB\n");
    }

    let verify_xres = vbe_read(VBE_DISPI_INDEX_XRES);
    let verify_yres = vbe_read(VBE_DISPI_INDEX_YRES);
    let verify_bpp = vbe_read(VBE_DISPI_INDEX_BPP);
    unsafe {
        raw_serial_str("vbe_io: Verified mode\n");
        raw_serial_hex(verify_xres as u64);
        raw_serial_hex(verify_yres as u64);
        raw_serial_hex(verify_bpp as u64);
    }

    let pixel_format = match bpp {
        32 => PixelFormat::XRGB8888,
        24 => PixelFormat::RGB888,
        16 => PixelFormat::RGB565,
        15 => PixelFormat::RGB555,
        _ => return Err("Unsupported BPP"),
    };

    Ok(VbeIoMode {
        width,
        height,
        bpp,
        framebuffer_phys: QEMU_VBE_FRAMEBUFFER_PHYS,
        pitch,
        pixel_format,
    })
}

pub fn disable_display() {
    vbe_write(VBE_DISPI_INDEX_ENABLE, VBE_DISPI_DISABLED);
}

pub fn enable_lfb() {
    vbe_write(
        VBE_DISPI_INDEX_ENABLE,
        VBE_DISPI_ENABLED | VBE_DISPI_LFB_ENABLED,
    );
}

pub fn init_32bit_desktop(phys_mem_offset: u64) -> Result<VbeIoMode, &'static str> {
    if !detect_vbe() {
        unsafe {
            raw_serial_str("vbe_io: VBE not detected\n");
        }
        return Err("VBE not detected");
    }

    let width = 800u16;
    let height = 600u16;
    let bpp = 32u8;

    let mode = set_mode(width, height, bpp)?;

    let fb_size = mode.pitch * mode.height as usize;
    let fb_virt = phys_mem_offset + mode.framebuffer_phys;

    unsafe {
        raw_serial_str("vbe_io: fb_virt=\n");
        raw_serial_hex(fb_virt);
        raw_serial_str("vbe_io: fb_size=\n");
        raw_serial_hex(fb_size as u64);
    }

    // The bootloader's map_physical_memory feature maps the entire physical
    // address space at phys_mem_offset, so 0xE0000000 is already mapped.
    // Test with a single write to verify.
    let test_ptr = fb_virt as *mut u32;
    unsafe {
        raw_serial_str("vbe_io: testing fb write...\n");
        core::ptr::write_volatile(test_ptr, 0x00000000);
        raw_serial_str("vbe_io: fb write OK\n");
    }

    Ok(mode)
}
