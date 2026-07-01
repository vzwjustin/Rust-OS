//! Mutter compositor foundation staging.
//!
//! GNOME on Wayland expects Mutter to own the display socket and compositor
//! role. Until a real Mutter binary ships in the rootfs, the kernel provides
//! the pre-bound Wayland socket and wire-protocol handshake via
//! [`crate::wayland::server`].
//!
//! In addition to readiness checks, this module launches an in-kernel
//! "Mutter" client that exercises the full Wayland pipeline: it connects
//! through the wire protocol, creates a compositor surface, fills an SHM
//! buffer with GNOME-style top bar content, and commits it for compositing.

extern crate alloc;

use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use crate::wayland::server;
use crate::wayland::{self, Arg, Message, DISPLAY_OBJECT_ID};

/// Pipe ID for the in-kernel Mutter client (distinct from smoke-test pipe).
const MUTTER_PIPE: u32 = 0xA001;

/// Next object ID for the Mutter client's Wayland objects.
static NEXT_OBJECT_ID: AtomicU32 = AtomicU32::new(100);

/// Whether the Mutter client has been launched.
static MUTTER_LAUNCHED: AtomicBool = AtomicBool::new(false);

/// Whether the Mutter client has a committed surface.
static MUTTER_SURFACE_COMMITTED: AtomicBool = AtomicBool::new(false);

/// Stored surface object ID for update_client.
static MUTTER_SURFACE_ID: AtomicU32 = AtomicU32::new(0);
/// Stored buffer object ID for update_client.
static MUTTER_BUFFER_ID: AtomicU32 = AtomicU32::new(0);
/// Stored SHM pool object ID for update_client.
static MUTTER_POOL_ID: AtomicU32 = AtomicU32::new(0);
/// Stored framebuffer width for update_client.
static MUTTER_FB_WIDTH: AtomicU32 = AtomicU32::new(0);
/// Stored bar height for update_client.
static MUTTER_BAR_HEIGHT: AtomicU32 = AtomicU32::new(32);

fn alloc_obj() -> u32 {
    NEXT_OBJECT_ID.fetch_add(1, Ordering::Relaxed)
}

/// Returns true when the GNOME overlay, Wayland compositor, and wire handshake
/// path are all ready for Mutter-style clients.
pub fn is_ready() -> bool {
    crate::gnome_overlay::is_ready()
        && crate::wayland::is_ready()
        && crate::wayland::server::is_handshake_ready()
}

/// Initialize the Mutter foundation — verifies that the GNOME overlay,
/// Wayland compositor, and wire handshake are all operational.
pub fn init() -> Result<(), &'static str> {
    if !crate::gnome_overlay::is_ready() {
        return Err("GNOME overlay required for Mutter foundation");
    }
    if !crate::wayland::is_ready() {
        return Err("Wayland compositor is not initialized");
    }
    crate::wayland::server::smoke_check()?;
    crate::early_serial_write_str("RustOS: Mutter foundation initialized\r\n");
    Ok(())
}

/// Verify Mutter foundation prerequisites and the in-kernel Wayland handshake.
pub fn smoke_check() -> Result<(), &'static str> {
    if !crate::gnome_overlay::is_ready() {
        return Err("GNOME overlay required for Mutter foundation");
    }
    if !crate::wayland::is_ready() {
        return Err("Wayland compositor is not initialized");
    }
    crate::wayland::server::smoke_check()
}

/// Send a Wayland wire message through the Mutter client pipe and process
/// the reply. Returns the reply bytes if any.
fn send_message(msg: &Message) -> Option<Vec<u8>> {
    let encoded = msg.encode();
    server::process_wire_request(&encoded, MUTTER_PIPE)
}

fn send_required(msg: &Message, error: &'static str) -> Result<(), &'static str> {
    let _ = send_message(msg).ok_or(error)?;
    Ok(())
}

fn object_registered(object_id: u32, interface: &'static str) -> bool {
    let Some(client_id) = server::pipe_client_id(MUTTER_PIPE) else {
        return false;
    };
    wayland::compositor()
        .get_client(client_id)
        .and_then(|client| client.objects.get(&object_id))
        .map(|object| object.interface == interface)
        .unwrap_or(false)
}

fn send_and_expect_object(
    msg: &Message,
    object_id: u32,
    interface: &'static str,
    error: &'static str,
) -> Result<(), &'static str> {
    let _ = send_message(msg);
    if object_registered(object_id, interface) {
        Ok(())
    } else {
        Err(error)
    }
}

#[allow(dead_code)]
fn bind_global(
    registry_id: u32,
    name: u32,
    new_id: u32,
    version: u32,
    interface: &'static str,
    error: &'static str,
) -> Result<(), &'static str> {
    let bind = Message::new(
        registry_id,
        0, // wl_registry.bind
        vec![
            Arg::UInt(name),
            Arg::String(interface.to_string()),
            Arg::UInt(version),
            Arg::NewId(new_id),
        ],
    );
    let _ = send_message(&bind);
    if object_registered(new_id, interface) {
        Ok(())
    } else {
        Err(error)
    }
}

/// Launch the in-kernel Mutter client. This creates a real Wayland client
/// connection through the wire protocol, creates a surface with a GNOME-style
/// top bar, and commits it for compositing.
///
/// Called once from the desktop main loop after the desktop is initialized.
pub fn launch_client() -> Result<(), &'static str> {
    if MUTTER_LAUNCHED.load(Ordering::Acquire) {
        return Ok(());
    }

    // Detach any previous connection on this pipe (safe to call even if none)
    server::detach_connection(MUTTER_PIPE);

    // Step 1: wl_display.sync — verifies the wire round-trip
    let callback_id = alloc_obj();
    let sync = Message::new(DISPLAY_OBJECT_ID, 0, vec![Arg::NewId(callback_id)]);
    let sync_reply = send_message(&sync).ok_or("Mutter: wl_display.sync failed")?;
    if sync_reply.is_empty() {
        return Err("Mutter: wl_display.sync returned empty reply");
    }

    // Step 2: wl_display.get_registry — get the global object list
    let registry_id = alloc_obj();
    let registry = Message::new(DISPLAY_OBJECT_ID, 1, vec![Arg::NewId(registry_id)]);
    let registry_reply = send_message(&registry).ok_or("Mutter: wl_display.get_registry failed")?;
    if registry_reply.len() < wayland::MessageHeader::SIZE {
        return Err("Mutter: wl_display.get_registry reply too short");
    }

    // Step 3: Bind wl_compositor (global name 1, version 4)
    let compositor_id = alloc_obj();
    let bind_compositor = Message::new(
        registry_id,
        0, // wl_registry.bind
        vec![
            Arg::UInt(1), // name
            Arg::String(wayland::interfaces::WL_COMPOSITOR.to_string()),
            Arg::UInt(4),              // version
            Arg::NewId(compositor_id), // new_id
        ],
    );
    send_and_expect_object(
        &bind_compositor,
        compositor_id,
        wayland::interfaces::WL_COMPOSITOR,
        "Mutter: wl_compositor bind failed",
    )?;

    // Step 4: Bind wl_shm (global name 2, version 1)
    let shm_id = alloc_obj();
    let bind_shm = Message::new(
        registry_id,
        0, // wl_registry.bind
        vec![
            Arg::UInt(2), // name
            Arg::String(wayland::interfaces::WL_SHM.to_string()),
            Arg::UInt(1),       // version
            Arg::NewId(shm_id), // new_id
        ],
    );
    send_and_expect_object(
        &bind_shm,
        shm_id,
        wayland::interfaces::WL_SHM,
        "Mutter: wl_shm bind failed",
    )?;

    // Step 5: Bind xdg_wm_base (global name 7, version 6)
    let xdg_wm_base_id = alloc_obj();
    let bind_xdg_wm_base = Message::new(
        registry_id,
        0, // wl_registry.bind
        vec![
            Arg::UInt(7), // name
            Arg::String(wayland::interfaces::XDG_WM_BASE.to_string()),
            Arg::UInt(6),               // version
            Arg::NewId(xdg_wm_base_id), // new_id
        ],
    );
    send_and_expect_object(
        &bind_xdg_wm_base,
        xdg_wm_base_id,
        wayland::interfaces::XDG_WM_BASE,
        "Mutter: xdg_wm_base bind failed",
    )?;

    // Step 6: wl_compositor.create_surface
    let surface_id = alloc_obj();
    let create_surface = Message::new(
        compositor_id,
        0, // wl_compositor.create_surface
        vec![Arg::NewId(surface_id)],
    );
    send_and_expect_object(
        &create_surface,
        surface_id,
        wayland::interfaces::WL_SURFACE,
        "Mutter: wl_compositor.create_surface failed",
    )?;

    // Step 7: xdg_wm_base.get_xdg_surface + xdg_surface.get_toplevel
    let xdg_surface_id = alloc_obj();
    let get_xdg_surface = Message::new(
        xdg_wm_base_id,
        2, // xdg_wm_base.get_xdg_surface
        vec![Arg::NewId(xdg_surface_id), Arg::Object(Some(surface_id))],
    );
    send_and_expect_object(
        &get_xdg_surface,
        xdg_surface_id,
        wayland::interfaces::XDG_SURFACE,
        "Mutter: xdg_wm_base.get_xdg_surface failed",
    )?;

    let toplevel_id = alloc_obj();
    let get_toplevel = Message::new(
        xdg_surface_id,
        1, // xdg_surface.get_toplevel
        vec![Arg::NewId(toplevel_id)],
    );
    let configure = send_message(&get_toplevel).ok_or("Mutter: xdg_surface.get_toplevel failed")?;
    if configure.len() < wayland::MessageHeader::SIZE {
        return Err("Mutter: xdg_toplevel configure reply too short");
    }
    if !object_registered(toplevel_id, wayland::interfaces::XDG_TOPLEVEL) {
        return Err("Mutter: xdg_toplevel object was not registered");
    }

    let _ = send_message(&Message::new(
        toplevel_id,
        2, // xdg_toplevel.set_title
        vec![Arg::String("RustOS Mutter".to_string())],
    ));
    let _ = send_message(&Message::new(
        toplevel_id,
        3, // xdg_toplevel.set_app_id
        vec![Arg::String("org.rustos.Mutter".to_string())],
    ));
    let _ = send_message(&Message::new(
        xdg_surface_id,
        4, // xdg_surface.ack_configure
        vec![Arg::UInt(1)],
    ));

    // Step 8: wl_shm.create_pool — create an SHM pool covering the full screen
    // (top bar + desktop background + bottom dock), not just the top bar.
    let (fb_w, fb_h) = crate::graphics::get_screen_dimensions().unwrap_or((800, 600));
    let bar_height = 32usize;
    let dock_height = 56usize;
    let bar_stride = fb_w * 4;
    let pool_size = (bar_stride * fb_h) as i32;

    let pool_id = alloc_obj();
    let create_pool = Message::new(
        shm_id,
        0, // wl_shm.create_pool
        vec![Arg::NewId(pool_id), Arg::Fd(0), Arg::Int(pool_size)],
    );
    let _ = send_message(&create_pool);

    // Step 7: wl_shm_pool.create_buffer — create a buffer from the pool
    let buffer_id = alloc_obj();
    let create_buffer = Message::new(
        pool_id,
        0, // wl_shm_pool.create_buffer
        vec![
            Arg::NewId(buffer_id),       // buffer_id
            Arg::Int(0),                 // offset
            Arg::Int(fb_w as i32),       // width
            Arg::Int(fb_h as i32),       // height
            Arg::Int(bar_stride as i32), // stride
            Arg::UInt(wayland::formats::XRGB8888),
        ],
    );
    let _ = send_message(&create_buffer);

    // Step 8: Fill the SHM pool data with a GNOME-style desktop:
    // top bar, desktop background, and a bottom dock.
    //
    // This holds the Compositor write lock for a long pixel-fill loop. If a
    // timer interrupt preempts us while the lock is held, the scheduler can
    // switch to another context that also wants this lock and spins forever
    // (the original holder never resumes to release it) — the same
    // single-core IRQ/lock-ordering deadlock class fixed elsewhere this
    // session. Disable interrupts for the whole critical section.
    crate::interrupts::without_interrupts(|| -> Result<(), &'static str> {
        let mut comp = wayland::compositor_mut();
        let client_id = server::pipe_client_id(MUTTER_PIPE)
            .ok_or("Mutter: client not found after wire setup")?;
        let client = comp
            .get_client_mut(client_id)
            .ok_or("Mutter: client connection not found")?;

        if let Some(pool) = client.shm_pools.get_mut(&pool_id) {
            // Ubuntu dark panel: #1a1a1a → #131313 gradient
            for y in 0..bar_height {
                let t = y as f32 / bar_height as f32;
                let r = (26.0 + (19 - 26) as f32 * t) as u8;
                let g = (26.0 + (19 - 26) as f32 * t) as u8;
                let b = (26.0 + (19 - 26) as f32 * t) as u8;
                for x in 0..fb_w {
                    let offset = (y * bar_stride + x * 4) as usize;
                    if offset + 4 <= pool.data.len() {
                        // XRGB8888: [R, G, B, X]
                        pool.data[offset] = r;
                        pool.data[offset + 1] = g;
                        pool.data[offset + 2] = b;
                        pool.data[offset + 3] = 0xFF;
                    }
                }
            }

            // Activities hover area (Ubuntu orange accent strip)
            for y in 0..bar_height {
                for x in 0..90 {
                    let offset = (y * bar_stride + x * 4) as usize;
                    if offset + 4 <= pool.data.len() {
                        pool.data[offset] = 233;
                        pool.data[offset + 1] = 84;
                        pool.data[offset + 2] = 32;
                        pool.data[offset + 3] = 0xFF;
                    }
                }
            }

            // Draw clock area (center, slightly lighter)
            let clock_x = fb_w / 2 - 40;
            for y in 0..bar_height {
                for x in clock_x..(clock_x + 80) {
                    if x < fb_w {
                        let offset = (y * bar_stride + x * 4) as usize;
                        if offset + 4 <= pool.data.len() {
                            pool.data[offset] = 60;
                            pool.data[offset + 1] = 56;
                            pool.data[offset + 2] = 66;
                            pool.data[offset + 3] = 0xFF;
                        }
                    }
                }
            }

            // Draw system tray area (right side)
            let tray_x = fb_w - 80;
            for y in 0..bar_height {
                for x in tray_x..fb_w {
                    let offset = (y * bar_stride + x * 4) as usize;
                    if offset + 4 <= pool.data.len() {
                        pool.data[offset] = 50;
                        pool.data[offset + 1] = 46;
                        pool.data[offset + 2] = 56;
                        pool.data[offset + 3] = 0xFF;
                    }
                }
            }

            crate::serial_println!(
                "mutter: bar drawn, pool.data.len()={} expected={}",
                pool.data.len(),
                bar_stride * fb_h
            );
            // Desktop background — fill everything between the top bar and
            // the bottom dock with a flat GNOME-style wallpaper color.
            let desktop_top = bar_height;
            let desktop_bottom = fb_h.saturating_sub(dock_height);
            for y in desktop_top..desktop_bottom {
                for x in 0..fb_w {
                    let offset = (y * bar_stride + x * 4) as usize;
                    if offset + 4 <= pool.data.len() {
                        pool.data[offset] = 53;
                        pool.data[offset + 1] = 28;
                        pool.data[offset + 2] = 79;
                        pool.data[offset + 3] = 0xFF;
                    }
                }
            }

            crate::serial_println!("mutter: background fill done");
            // Bottom dock bar (Ubuntu-style dark dock background)
            for y in desktop_bottom..fb_h {
                if y % 10 == 0 {
                    crate::serial_println!("mutter: dock row y={}", y);
                }
                for x in 0..fb_w {
                    let offset = (y * bar_stride + x * 4) as usize;
                    if offset + 4 <= pool.data.len() {
                        pool.data[offset] = 18;
                        pool.data[offset + 1] = 16;
                        pool.data[offset + 2] = 20;
                        pool.data[offset + 3] = 0xFF;
                    }
                }
            }

            crate::serial_println!("mutter: dock fill done");
            // Dock launcher icons (evenly spaced colored squares)
            let icon_colors: [(u8, u8, u8); 5] = [
                (233, 84, 32),  // Files (orange)
                (53, 132, 228), // Browser (blue)
                (38, 162, 105), // Terminal (green)
                (192, 28, 40),  // Settings (red)
                (163, 71, 186), // Activities (purple)
            ];
            let icon_size = 36usize;
            let icon_gap = 16usize;
            let icons_total_w = icon_colors.len() * icon_size + (icon_colors.len() - 1) * icon_gap;
            let icon_start_x = fb_w.saturating_sub(icons_total_w) / 2;
            let icon_y = desktop_bottom + (dock_height.saturating_sub(icon_size)) / 2;
            for (i, (r, g, b)) in icon_colors.iter().enumerate() {
                let icon_x = icon_start_x + i * (icon_size + icon_gap);
                for y in icon_y..(icon_y + icon_size).min(fb_h) {
                    for x in icon_x..(icon_x + icon_size).min(fb_w) {
                        let offset = (y * bar_stride + x * 4) as usize;
                        if offset + 4 <= pool.data.len() {
                            pool.data[offset] = *r;
                            pool.data[offset + 1] = *g;
                            pool.data[offset + 2] = *b;
                            pool.data[offset + 3] = 0xFF;
                        }
                    }
                }
            }
            crate::serial_println!("mutter: icons fill done");
        }
        Ok(())
    })?;
    crate::serial_println!("mutter: SHM fill block exited");

    // Step 9: wl_surface.attach — attach the buffer to the surface
    let attach = Message::new(
        surface_id,
        1, // wl_surface.attach
        vec![
            Arg::Object(Some(buffer_id)),
            Arg::Int(0), // x
            Arg::Int(0), // y
        ],
    );
    let _ = send_message(&attach);

    // Step 10: wl_surface.damage — mark the whole surface as damaged
    let damage = Message::new(
        surface_id,
        2, // wl_surface.damage
        vec![
            Arg::Int(0),
            Arg::Int(0),
            Arg::Int(fb_w as i32),
            Arg::Int(fb_h as i32),
        ],
    );
    let _ = send_message(&damage);

    // Step 11: wl_surface.commit — commit the surface for compositing
    let commit = Message::new(
        surface_id,
        6, // wl_surface.commit
        vec![],
    );
    let _ = send_message(&commit);

    MUTTER_SURFACE_ID.store(surface_id, Ordering::Release);
    MUTTER_BUFFER_ID.store(buffer_id, Ordering::Release);
    MUTTER_POOL_ID.store(pool_id, Ordering::Release);
    MUTTER_FB_WIDTH.store(fb_w as u32, Ordering::Release);
    MUTTER_BAR_HEIGHT.store(bar_height as u32, Ordering::Release);

    MUTTER_LAUNCHED.store(true, Ordering::Release);
    MUTTER_SURFACE_COMMITTED.store(true, Ordering::Release);

    crate::early_serial_write_str("RustOS: Mutter client launched (surface committed)\r\n");
    Ok(())
}

/// Whether the Mutter client has been launched and has a committed surface.
pub fn is_client_active() -> bool {
    MUTTER_LAUNCHED.load(Ordering::Acquire)
}

/// Update the Mutter client's surface content. Called periodically from the
/// main loop to refresh the top bar (e.g. clock updates).
///
/// Redraws the clock area in the SHM pool with the current system time,
/// then sends wl_surface.damage + wl_surface.commit through the wire
/// protocol so the compositor re-renders the surface.
pub fn update_client() {
    if !MUTTER_SURFACE_COMMITTED.load(Ordering::Acquire) {
        return;
    }

    let surface_id = MUTTER_SURFACE_ID.load(Ordering::Acquire);
    let pool_id = MUTTER_POOL_ID.load(Ordering::Acquire);
    let fb_w = MUTTER_FB_WIDTH.load(Ordering::Acquire) as usize;
    let bar_height = MUTTER_BAR_HEIGHT.load(Ordering::Acquire) as usize;

    if surface_id == 0 || pool_id == 0 || fb_w == 0 {
        return;
    }

    let now = crate::time::system_time();
    let hours = (now / 3600) % 24;
    let mins = (now / 60) % 60;
    let clock_text = match (hours, mins) {
        (h, m) => {
            let h_str = if h < 10 {
                alloc::format!("0{}", h)
            } else {
                alloc::format!("{}", h)
            };
            let m_str = if m < 10 {
                alloc::format!("0{}", m)
            } else {
                alloc::format!("{}", m)
            };
            alloc::format!("{}:{}", h_str, m_str)
        }
    };

    let clock_x = fb_w / 2 - 40;
    let clock_w = 80usize;
    let bar_stride = fb_w * 4;

    {
        let mut comp = wayland::compositor_mut();
        let client_id = match server::pipe_client_id(MUTTER_PIPE) {
            Some(id) => id,
            None => return,
        };
        let client = match comp.get_client_mut(client_id) {
            Some(c) => c,
            None => return,
        };
        let pool = match client.shm_pools.get_mut(&pool_id) {
            Some(p) => p,
            None => return,
        };

        for y in 0..bar_height {
            for x in clock_x..(clock_x + clock_w) {
                if x >= fb_w {
                    continue;
                }
                let offset = (y * bar_stride + x * 4) as usize;
                if offset + 4 > pool.data.len() {
                    continue;
                }
                pool.data[offset] = 60;
                pool.data[offset + 1] = 56;
                pool.data[offset + 2] = 66;
                pool.data[offset + 3] = 0xFF;
            }
        }

        let clock_bytes = clock_text.as_bytes();
        let font = crate::graphics::get_default_font();
        let char_w = font.char_width;
        let char_h = font.char_height;
        let start_x = clock_x + (clock_w - clock_bytes.len() * char_w) / 2;
        let start_y = (bar_height - char_h) / 2;

        for (ci, &byte) in clock_bytes.iter().enumerate() {
            let glyph_idx = (byte as usize) * char_h;
            let char_start_x = start_x + ci * char_w;

            for gy in 0..char_h {
                if glyph_idx + gy >= font.data.len() {
                    continue;
                }
                let row_bits = font.data[glyph_idx + gy];
                for gx in 0..8 {
                    if row_bits & (0x80 >> gx) != 0 {
                        let px = char_start_x + gx;
                        let py = start_y + gy;
                        if px >= fb_w || py >= bar_height {
                            continue;
                        }
                        let offset = (py * bar_stride + px * 4) as usize;
                        if offset + 4 > pool.data.len() {
                            continue;
                        }
                        pool.data[offset] = 0xFF;
                        pool.data[offset + 1] = 0xFF;
                        pool.data[offset + 2] = 0xFF;
                        pool.data[offset + 3] = 0xFF;
                    }
                }
            }
        }
    }

    let damage = Message::new(
        surface_id,
        2,
        vec![
            Arg::Int(clock_x as i32),
            Arg::Int(0),
            Arg::Int(clock_w as i32),
            Arg::Int(bar_height as i32),
        ],
    );
    let _ = send_message(&damage);

    let commit = Message::new(surface_id, 6, vec![]);
    let _ = send_message(&commit);
}

/// Whether the Mutter client should be launched.
pub fn should_launch() -> bool {
    !MUTTER_LAUNCHED.load(Ordering::Acquire) && is_ready()
}
