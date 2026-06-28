//! Framebuffer compositing for Wayland SHM buffers.

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;

use crate::graphics::framebuffer::{self, Color, PixelFormat};

use super::{formats, Buffer, ClientConnection, DamageRect, Surface};

/// Blit a committed surface to the kernel framebuffer.
pub fn render_surface(client: &ClientConnection, surface: &Surface) {
    let Some(buffer_id) = surface.buffer else {
        return;
    };

    let Some(buffer) = client.buffers.get(&buffer_id) else {
        return;
    };

    let Some(pool) = client.shm_pools.get(&buffer.pool_id) else {
        return;
    };

    let fb = match framebuffer::framebuffer() {
        Some(fb) => fb,
        None => return,
    };

    let (fb_width, fb_height) = (fb.width, fb.height);
    let pixel_format = fb.pixel_format;

    let regions: Vec<DamageRect> = if surface.damage.is_empty() {
        vec![DamageRect {
            x: 0,
            y: 0,
            width: buffer.width,
            height: buffer.height,
        }]
    } else {
        surface.damage.clone()
    };

    for rect in regions {
        blit_region(
            &pool.data,
            buffer,
            rect,
            surface.x,
            surface.y,
            fb_width,
            fb_height,
            pixel_format,
            fb,
        );
    }

    framebuffer::present();
}

fn blit_region(
    pool_data: &[u8],
    buffer: &Buffer,
    rect: DamageRect,
    surface_x: i32,
    surface_y: i32,
    fb_width: usize,
    fb_height: usize,
    pixel_format: PixelFormat,
    fb: &mut framebuffer::SimpleFramebuffer,
) {
    let bpp = 4usize;
    let start = buffer.offset as usize + (rect.y as usize * buffer.stride as usize);
    if start >= pool_data.len() {
        return;
    }

    for row in 0..rect.height.max(0) as usize {
        let src_y = rect.y as usize + row;
        if src_y >= buffer.height as usize {
            break;
        }

        for col in 0..rect.width.max(0) as usize {
            let src_x = rect.x as usize + col;
            if src_x >= buffer.width as usize {
                break;
            }

            let offset = start + row * buffer.stride as usize + col * bpp;
            if offset + bpp > pool_data.len() {
                continue;
            }

            let pixel = u32::from_le_bytes([
                pool_data[offset],
                pool_data[offset + 1],
                pool_data[offset + 2],
                pool_data[offset + 3],
            ]);

            let color = shm_pixel_to_color(pixel, buffer.format);
            if color.a == 0 {
                continue;
            }

            let dst_x = surface_x as isize + rect.x as isize + col as isize;
            let dst_y = surface_y as isize + rect.y as isize + row as isize;
            if dst_x < 0 || dst_y < 0 {
                continue;
            }

            let dst_x = dst_x as usize;
            let dst_y = dst_y as usize;
            if dst_x >= fb_width || dst_y >= fb_height {
                continue;
            }

            fb.set_pixel(dst_x, dst_y, color);
            let _ = pixel_format;
        }
    }
}

fn shm_pixel_to_color(pixel: u32, format: u32) -> Color {
    match format {
        formats::ARGB8888 => Color::new(
            ((pixel >> 16) & 0xFF) as u8,
            ((pixel >> 8) & 0xFF) as u8,
            (pixel & 0xFF) as u8,
            ((pixel >> 24) & 0xFF) as u8,
        ),
        formats::XRGB8888 | formats::RGB888 => Color::rgb(
            ((pixel >> 16) & 0xFF) as u8,
            ((pixel >> 8) & 0xFF) as u8,
            (pixel & 0xFF) as u8,
        ),
        formats::RGB565 => {
            let r = (((pixel >> 11) & 0x1F) * 255 / 31) as u8;
            let g = (((pixel >> 5) & 0x3F) * 255 / 63) as u8;
            let b = ((pixel & 0x1F) * 255 / 31) as u8;
            Color::rgb(r, g, b)
        }
        _ => Color::rgb(
            ((pixel >> 16) & 0xFF) as u8,
            ((pixel >> 8) & 0xFF) as u8,
            (pixel & 0xFF) as u8,
        ),
    }
}

/// Build wl_buffer.release and wl_surface.enter post-commit events.
///
/// Returns reply bytes and, when a new `wl_surface.enter` was emitted, the output id.
pub fn surface_commit_events(
    client: &ClientConnection,
    surface: &Surface,
) -> (Vec<u8>, Option<super::ObjectId>) {
    let mut out = Vec::new();
    let mut entered = None;

    if let Some(buffer_id) = surface.buffer {
        if client.buffers.contains_key(&buffer_id) {
            out.extend_from_slice(&super::Message::new(buffer_id, 0, Vec::new()).encode());
        }
    }

    let output_id = client
        .objects
        .iter()
        .find(|(_, obj)| obj.interface == super::interfaces::WL_OUTPUT)
        .map(|(id, _)| *id);

    if let Some(output_id) = output_id {
        if surface.entered_output != Some(output_id) {
            out.extend_from_slice(&super::event_surface_enter(surface.id, output_id).encode());
            entered = Some(output_id);
        }
    }

    (out, entered)
}

/// Smoke-test compositing by blitting a synthetic SHM buffer.
pub fn smoke_check() -> Result<(), &'static str> {
    let mut client = ClientConnection::new(1);
    let pool_id = client.create_shm_pool(4096);
    if let Some(pool) = client.shm_pools.get_mut(&pool_id) {
        for (i, byte) in pool.data.iter_mut().enumerate().take(256) {
            *byte = match i % 4 {
                0 => 0xFF,
                1 => 0x00,
                2 => 0x00,
                _ => 0x00,
            };
        }
    }

    let buffer_id = client.create_buffer(pool_id, 0, 4, 4, 16, formats::XRGB8888);
    let surface_id = client.create_surface();
    {
        let surface = client
            .surfaces
            .get_mut(&surface_id)
            .ok_or("surface missing")?;
        surface.attach(Some(buffer_id));
        surface.commit();
    }
    if let Some(surface) = client.surfaces.get(&surface_id) {
        render_surface(&client, surface);
    }

    Ok(())
}
