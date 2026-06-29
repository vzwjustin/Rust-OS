//! Wayland display socket server — per-connection wire dispatch.
//!
//! Bridges AF_UNIX Wayland socket I/O from `socket_ops` to the in-kernel
//! compositor. Handles the minimum client handshake (`sync`, `get_registry`,
//! `bind`) that Mutter and gnome-shell need before real compositor binaries
//! ship in the rootfs.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;

use super::{
    compositor, compositor_mut, core_protocol, display_error, event_global, event_output_geometry,
    event_output_mode, interfaces, Arg, ArgType, ClientConnection, GlobalEntry, Message, ObjectId,
    DISPLAY_OBJECT_ID,
};

static PIPE_CLIENTS: Mutex<BTreeMap<u32, u32>> = Mutex::new(BTreeMap::new());
static READ_BUFFERS: Mutex<BTreeMap<u32, Vec<u8>>> = Mutex::new(BTreeMap::new());
static HANDSHAKE_READY: AtomicBool = AtomicBool::new(false);
const SMOKE_TEST_PIPE: u32 = 0x9001;

/// Returns true once a successful Wayland wire handshake has been observed.
pub fn is_handshake_ready() -> bool {
    HANDSHAKE_READY.load(Ordering::Acquire)
}

fn mark_runtime_handshake_ready(pipe_id: u32) {
    if pipe_id != SMOKE_TEST_PIPE {
        HANDSHAKE_READY.store(true, Ordering::Release);
    }
}

/// Mark the Wayland wire handshake as ready. Called by `wayland::init()`
/// after the smoke check verifies the wire protocol end-to-end.
pub fn mark_handshake_ready() {
    HANDSHAKE_READY.store(true, Ordering::Release);
}

/// Attach a compositor client to a Unix socket connection pipe.
pub fn attach_connection(pipe_id: u32) -> Result<(), &'static str> {
    let mut map = PIPE_CLIENTS.lock();
    if map.contains_key(&pipe_id) {
        return Ok(());
    }

    if !compositor().is_initialized() {
        return Err("Wayland compositor is not initialized");
    }

    let client_id = compositor_mut().connect_client();
    map.insert(pipe_id, client_id);
    Ok(())
}

/// Detach a compositor client when its socket connection closes.
pub fn detach_connection(pipe_id: u32) {
    if let Some(client_id) = PIPE_CLIENTS.lock().remove(&pipe_id) {
        compositor_mut().disconnect_client(client_id);
    }
    READ_BUFFERS.lock().remove(&pipe_id);
}

/// Get the compositor client ID associated with a pipe.
pub fn pipe_client_id(pipe_id: u32) -> Option<u32> {
    PIPE_CLIENTS.lock().get(&pipe_id).copied()
}

/// Process Wayland wire data from a connected client and return reply bytes.
pub fn process_wire_request(data: &[u8], pipe_id: u32) -> Option<Vec<u8>> {
    attach_connection(pipe_id).ok()?;

    {
        let mut buffers = READ_BUFFERS.lock();
        let buffer = buffers.entry(pipe_id).or_insert_with(Vec::new);
        buffer.extend_from_slice(data);
    }

    let mut replies = Vec::new();

    loop {
        let message = {
            let buffers = READ_BUFFERS.lock();
            let buffer = buffers.get(&pipe_id)?;
            if buffer.len() < super::MessageHeader::SIZE {
                break;
            }
            let header = super::MessageHeader::parse(buffer).ok()?;
            if buffer.len() < header.size as usize {
                break;
            }
            let end = header.size as usize;
            let arg_types = request_arg_types(pipe_id, &buffer[..end])?;
            Message::decode(&buffer[..end], &arg_types).ok()?
        };

        let consumed = message.header.size as usize;
        {
            let mut buffers = READ_BUFFERS.lock();
            if let Some(buffer) = buffers.get_mut(&pipe_id) {
                buffer.drain(..consumed);
            }
        }

        let client_id = *PIPE_CLIENTS.lock().get(&pipe_id)?;
        if let Some(response) = dispatch_message(pipe_id, client_id, &message) {
            replies.extend_from_slice(&response);
        }
    }

    if replies.is_empty() {
        None
    } else {
        Some(replies)
    }
}

fn request_arg_types(pipe_id: u32, data: &[u8]) -> Option<Vec<ArgType>> {
    let header = super::MessageHeader::parse(data).ok()?;

    if header.object_id == DISPLAY_OBJECT_ID {
        return match header.opcode {
            0 => Some(vec![ArgType::NewId]), // wl_display.sync
            1 => Some(vec![ArgType::NewId]), // wl_display.get_registry
            _ => None,
        };
    }

    let client_id = *PIPE_CLIENTS.lock().get(&pipe_id)?;
    let interface = {
        let comp = compositor();
        let client = comp.get_client(client_id)?;
        let object = client.objects.get(&header.object_id)?;
        object.interface
    };

    if let Some(types) = core_protocol::request_arg_types(interface, header.opcode) {
        return Some(types.to_vec());
    }

    match interface {
        interfaces::WL_COMPOSITOR => match header.opcode {
            0 => Some(vec![ArgType::NewId]),
            1 => Some(vec![ArgType::NewId]),
            _ => None,
        },
        interfaces::WL_SUBCOMPOSITOR if header.opcode == 0 => {
            Some(vec![ArgType::NewId, ArgType::Object, ArgType::Object])
        }
        interfaces::WL_SURFACE => match header.opcode {
            0 => Some(Vec::new()),                                        // destroy
            1 => Some(vec![ArgType::Object, ArgType::Int, ArgType::Int]), // attach
            2 => Some(vec![ArgType::Int, ArgType::Int, ArgType::Int, ArgType::Int]), // damage
            3 => Some(vec![ArgType::NewId]),                              // frame
            4 | 5 => Some(vec![ArgType::Object]), // set_opaque_region, set_input_region
            6 => Some(Vec::new()),                // commit
            7 | 8 => Some(vec![ArgType::Int]),    // set_buffer_transform, set_buffer_scale
            9 => Some(vec![ArgType::Int, ArgType::Int, ArgType::Int, ArgType::Int]), // damage_buffer
            _ => None,
        },
        interfaces::WL_SEAT => match header.opcode {
            0 | 1 | 2 => Some(vec![ArgType::NewId]),
            3 => Some(Vec::new()),
            _ => None,
        },
        interfaces::WL_SHM if header.opcode == 0 => {
            Some(vec![ArgType::NewId, ArgType::Fd, ArgType::Int])
        }
        interfaces::WL_SHM_POOL => match header.opcode {
            0 => Some(vec![
                ArgType::Int,
                ArgType::Int,
                ArgType::Int,
                ArgType::Int,
                ArgType::UInt,
                ArgType::NewId,
            ]),
            1 => Some(Vec::new()),
            2 => Some(vec![ArgType::Int]),
            _ => None,
        },
        interfaces::WL_BUFFER if header.opcode == 0 => Some(Vec::new()),
        interfaces::WL_DATA_DEVICE_MANAGER => match header.opcode {
            0 => Some(vec![ArgType::NewId]),
            1 => Some(vec![ArgType::NewId, ArgType::Object]),
            _ => None,
        },
        interfaces::WL_DATA_SOURCE => match header.opcode {
            0 | 2 | 3 => Some(vec![ArgType::String]),
            1 => Some(Vec::new()),
            _ => None,
        },
        interfaces::WL_DATA_DEVICE => match header.opcode {
            0 => Some(vec![ArgType::Object]),
            1 | 2 | 4 => Some(Vec::new()),
            3 => Some(vec![ArgType::UInt, ArgType::String, ArgType::Object]),
            _ => None,
        },
        interfaces::WL_REGION => match header.opcode {
            0 | 1 => Some(vec![ArgType::Int, ArgType::Int, ArgType::Int, ArgType::Int]),
            2 => Some(Vec::new()),
            _ => None,
        },
        interfaces::XDG_WM_BASE => match header.opcode {
            0 => Some(Vec::new()),
            1 => Some(vec![ArgType::NewId]),
            2 => Some(vec![ArgType::NewId, ArgType::Object]),
            3 => Some(vec![ArgType::UInt]),
            _ => None,
        },
        interfaces::XDG_SURFACE => match header.opcode {
            0 => Some(Vec::new()),
            1 => Some(vec![ArgType::NewId]),
            2 => Some(vec![ArgType::NewId, ArgType::Object, ArgType::Object]),
            3 => Some(vec![ArgType::Int, ArgType::Int, ArgType::Int, ArgType::Int]),
            4 => Some(vec![ArgType::UInt]),
            _ => None,
        },
        interfaces::XDG_TOPLEVEL => match header.opcode {
            0 => Some(Vec::new()),
            1 => Some(vec![ArgType::Object]),
            2 => Some(vec![ArgType::String]),
            3 => Some(vec![ArgType::String]),
            4 => Some(vec![ArgType::UInt, ArgType::Int, ArgType::Int]),
            5 => Some(vec![ArgType::UInt, ArgType::UInt]),
            6 => Some(vec![ArgType::UInt, ArgType::UInt, ArgType::UInt]),
            7 | 8 => Some(vec![ArgType::Int, ArgType::Int]),
            9 | 10 | 12 | 13 => Some(Vec::new()),
            11 => Some(vec![ArgType::UInt, ArgType::Object]),
            _ => None,
        },
        interfaces::XDG_POPUP => match header.opcode {
            0 => Some(Vec::new()),
            1 => Some(vec![ArgType::Object, ArgType::UInt]),
            2 => Some(vec![ArgType::Object, ArgType::UInt]),
            _ => None,
        },
        interfaces::XDG_POSITIONER => match header.opcode {
            0 => Some(Vec::new()),
            1 => Some(vec![ArgType::Int, ArgType::Int, ArgType::Int, ArgType::Int]),
            2 => Some(vec![ArgType::Int, ArgType::Int]),
            3 => Some(vec![ArgType::UInt]),
            4 => Some(vec![ArgType::UInt]),
            5 => Some(vec![ArgType::Int]),
            6 => Some(vec![ArgType::UInt]),
            _ => None,
        },
        _ => None,
    }
}

fn dispatch_message(pipe_id: u32, client_id: u32, message: &Message) -> Option<Vec<u8>> {
    let mut comp = compositor_mut();

    if message.header.object_id == DISPLAY_OBJECT_ID {
        return match message.header.opcode {
            0 => {
                let client = comp.get_client_mut(client_id)?;
                handle_display_sync(pipe_id, client, message)
            }
            1 => {
                let globals = comp.global_list();
                let client = comp.get_client_mut(client_id)?;
                handle_display_get_registry(pipe_id, client, &globals, message)
            }
            _ => Some(encode_error(
                DISPLAY_OBJECT_ID,
                display_error::INVALID_METHOD,
                "unknown wl_display request",
            )),
        };
    }

    let is_registry = comp
        .get_client(client_id)?
        .objects
        .get(&message.header.object_id)
        .map(|obj| obj.interface == interfaces::WL_REGISTRY)
        .unwrap_or(false);

    if is_registry && message.header.opcode == 0 {
        let globals = comp.global_list();
        let outputs: Vec<_> = comp.list_outputs().into_iter().cloned().collect();
        let client = comp.get_client_mut(client_id)?;
        return handle_registry_bind(pipe_id, client, &globals, &outputs, message);
    }

    let object_iface = comp
        .get_client(client_id)?
        .objects
        .get(&message.header.object_id)
        .map(|obj| obj.interface)
        .unwrap_or("");

    match object_iface {
        interfaces::WL_COMPOSITOR => match message.header.opcode {
            0 => {
                let client = comp.get_client_mut(client_id)?;
                handle_compositor_create_surface(client, message)
            }
            1 => {
                let client = comp.get_client_mut(client_id)?;
                handle_compositor_create_region(client, message)
            }
            _ => None,
        },
        interfaces::WL_SURFACE => {
            let client = comp.get_client_mut(client_id)?;
            handle_surface_request(client, message)
        }
        interfaces::WL_SHM if message.header.opcode == 0 => {
            let client = comp.get_client_mut(client_id)?;
            handle_shm_create_pool(client, message)
        }
        interfaces::WL_SHM_POOL => {
            let client = comp.get_client_mut(client_id)?;
            handle_shm_pool_request(client, message)
        }
        interfaces::WL_BUFFER if message.header.opcode == 0 => {
            let client = comp.get_client_mut(client_id)?;
            handle_buffer_destroy(client, message.header.object_id);
            None
        }
        interfaces::WL_DATA_DEVICE_MANAGER => {
            let client = comp.get_client_mut(client_id)?;
            handle_data_device_manager_request(client, message)
        }
        interfaces::WL_DATA_SOURCE => {
            let client = comp.get_client_mut(client_id)?;
            handle_data_source_request(client, message)
        }
        interfaces::WL_DATA_DEVICE => {
            let client = comp.get_client_mut(client_id)?;
            handle_data_device_request(client, message)
        }
        interfaces::WL_SEAT => {
            let client = comp.get_client_mut(client_id)?;
            handle_seat_request(pipe_id, client, message)
        }
        interfaces::WL_REGION => {
            let client = comp.get_client_mut(client_id)?;
            handle_region_request(client, message)
        }
        interfaces::WL_SUBCOMPOSITOR if message.header.opcode == 0 => {
            let client = comp.get_client_mut(client_id)?;
            handle_subcompositor_get_subsurface(client, message)
        }
        interfaces::XDG_WM_BASE => {
            let client = comp.get_client_mut(client_id)?;
            handle_xdg_wm_base_request(client, message)
        }
        interfaces::XDG_SURFACE => {
            let client = comp.get_client_mut(client_id)?;
            handle_xdg_surface_request(client, message)
        }
        interfaces::XDG_TOPLEVEL => {
            let client = comp.get_client_mut(client_id)?;
            handle_xdg_toplevel_request(client, message)
        }
        interfaces::XDG_POPUP => {
            let client = comp.get_client_mut(client_id)?;
            handle_xdg_popup_request(client, message)
        }
        interfaces::XDG_POSITIONER => {
            let client = comp.get_client_mut(client_id)?;
            handle_xdg_positioner_request(client, message)
        }
        _ => None,
    }
}

fn handle_display_sync(
    pipe_id: u32,
    client: &mut ClientConnection,
    message: &Message,
) -> Option<Vec<u8>> {
    let callback_id = match message.args.first() {
        Some(Arg::NewId(id)) => *id,
        _ => return None,
    };

    client.objects.insert(
        callback_id,
        super::ProtocolObject {
            id: callback_id,
            interface: interfaces::WL_CALLBACK,
            version: 1,
        },
    );

    let serial = client.next_serial();
    let done = Message::new(callback_id, 0, vec![Arg::UInt(serial)]);
    mark_runtime_handshake_ready(pipe_id);
    Some(done.encode())
}

fn handle_display_get_registry(
    pipe_id: u32,
    client: &mut ClientConnection,
    globals: &[GlobalEntry],
    message: &Message,
) -> Option<Vec<u8>> {
    let registry_id = match message.args.first() {
        Some(Arg::NewId(id)) => *id,
        _ => return None,
    };

    client.objects.insert(
        registry_id,
        super::ProtocolObject {
            id: registry_id,
            interface: interfaces::WL_REGISTRY,
            version: 1,
        },
    );

    let mut out = Vec::new();
    for global in globals {
        out.extend_from_slice(
            &event_global(registry_id, global.name, global.interface, global.version).encode(),
        );
    }

    mark_runtime_handshake_ready(pipe_id);
    Some(out)
}

fn handle_registry_bind(
    pipe_id: u32,
    client: &mut ClientConnection,
    globals: &[GlobalEntry],
    outputs: &[super::Output],
    message: &Message,
) -> Option<Vec<u8>> {
    let (name, new_id, version) = match (&message.args[0], &message.args[1], &message.args[2]) {
        (Arg::UInt(name), Arg::NewId(id), Arg::UInt(version)) => (*name, *id, *version),
        (Arg::UInt(name), Arg::String(_), Arg::UInt(version)) => match message.args.get(3) {
            Some(Arg::NewId(id)) => (*name, *id, *version),
            _ => return None,
        },
        _ => return None,
    };

    let global = globals.iter().find(|entry| entry.name == name)?;
    let bound_version = version.min(global.version);

    client.objects.insert(
        new_id,
        super::ProtocolObject {
            id: new_id,
            interface: global.interface,
            version: bound_version,
        },
    );

    let mut out = Vec::new();

    match global.interface {
        interfaces::WL_COMPOSITOR
        | interfaces::WL_DATA_DEVICE_MANAGER
        | interfaces::WL_SUBCOMPOSITOR
        | interfaces::XDG_WM_BASE => {}
        interfaces::WL_SHM => {
            for &fmt in &[
                super::formats::XRGB8888,
                super::formats::ARGB8888,
                super::formats::RGB888,
                super::formats::RGB565,
            ] {
                out.extend_from_slice(&Message::new(new_id, 0, vec![Arg::UInt(fmt)]).encode());
            }
        }
        interfaces::WL_SEAT => {
            super::input::register_seat(client, new_id);
            out.extend_from_slice(&super::input::seat_bind_events(new_id));
        }
        interfaces::WL_OUTPUT => {
            if let Some(output) = outputs.first() {
                out.extend_from_slice(
                    &event_output_geometry(
                        new_id,
                        output.x,
                        output.y,
                        output.physical_width,
                        output.physical_height,
                        output.subpixel,
                        &output.make,
                        &output.model,
                        output.transform,
                    )
                    .encode(),
                );

                let flags = (if output.mode.preferred { 1u32 } else { 0 })
                    | (if output.mode.current { 2u32 } else { 0 });
                out.extend_from_slice(
                    &event_output_mode(
                        new_id,
                        flags,
                        output.mode.width,
                        output.mode.height,
                        output.mode.refresh,
                    )
                    .encode(),
                );

                // wl_output.done (opcode 2) — signals output description complete
                out.extend_from_slice(&Message::new(new_id, 2, vec![]).encode());
                // wl_output.scale (opcode 3) — scale factor 1
                out.extend_from_slice(&Message::new(new_id, 3, vec![Arg::Int(1)]).encode());
            }
        }
        _ => {}
    }

    mark_runtime_handshake_ready(pipe_id);

    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn encode_error(object_id: ObjectId, code: u32, message: &str) -> Vec<u8> {
    super::event_error(object_id, code, message).encode()
}

fn handle_compositor_create_surface(
    client: &mut ClientConnection,
    message: &Message,
) -> Option<Vec<u8>> {
    let surface_id = match message.args.first() {
        Some(Arg::NewId(id)) => *id,
        _ => return None,
    };

    client.objects.insert(
        surface_id,
        super::ProtocolObject {
            id: surface_id,
            interface: interfaces::WL_SURFACE,
            version: 4,
        },
    );
    client
        .surfaces
        .insert(surface_id, super::Surface::new(surface_id));
    None
}

fn handle_surface_request(client: &mut ClientConnection, message: &Message) -> Option<Vec<u8>> {
    let surface_id = message.header.object_id;
    match message.header.opcode {
        0 => {
            client.destroy_object(surface_id);
            None
        }
        1 => {
            let buffer = match message.args.first() {
                Some(Arg::Object(id)) => *id,
                _ => None,
            };
            let x = match message.args.get(1) {
                Some(Arg::Int(v)) => *v,
                _ => 0,
            };
            let y = match message.args.get(2) {
                Some(Arg::Int(v)) => *v,
                _ => 0,
            };
            if let Some(surface) = client.surfaces.get_mut(&surface_id) {
                surface.attach(buffer);
                surface.x = x;
                surface.y = y;
            }
            None
        }
        2 => {
            // wl_surface.damage (opcode 2)
            let rect = super::DamageRect {
                x: arg_i32(&message.args, 0),
                y: arg_i32(&message.args, 1),
                width: arg_i32(&message.args, 2),
                height: arg_i32(&message.args, 3),
            };
            if let Some(surface) = client.surfaces.get_mut(&surface_id) {
                surface.damage(rect);
            }
            None
        }
        3 => {
            // wl_surface.frame (opcode 3) — register a frame callback
            let callback_id = match message.args.first() {
                Some(Arg::NewId(id)) => *id,
                _ => return None,
            };
            if let Some(surface) = client.surfaces.get_mut(&surface_id) {
                surface.frame_callback = Some(callback_id);
            }
            None
        }
        6 => {
            // wl_surface.commit (opcode 6)
            if let Some(surface) = client.surfaces.get_mut(&surface_id) {
                surface.commit();
            }

            let (mut events, entered_output) =
                if let Some(surface) = client.surfaces.get(&surface_id) {
                    super::render::render_surface(client, surface);
                    super::render::surface_commit_events(client, surface)
                } else {
                    (Vec::new(), None)
                };

            if let (Some(surface), Some(output_id)) =
                (client.surfaces.get_mut(&surface_id), entered_output)
            {
                surface.entered_output = Some(output_id);
            }

            events.extend(super::input::surface_post_commit_events(client, surface_id));

            if let Some(surface) = client.surfaces.get(&surface_id) {
                if let Some(callback_id) = surface.frame_callback {
                    client.objects.insert(
                        callback_id,
                        super::ProtocolObject {
                            id: callback_id,
                            interface: interfaces::WL_CALLBACK,
                            version: 1,
                        },
                    );
                    let serial = client.next_serial();
                    events.extend_from_slice(
                        &Message::new(callback_id, 0, vec![Arg::UInt(serial)]).encode(),
                    );
                }
            }

            if let Some(surface) = client.surfaces.get_mut(&surface_id) {
                surface.frame_callback = None;
                if let Some(buffer_id) = surface.buffer {
                    if let Some(buf) = client.buffers.get_mut(&buffer_id) {
                        buf.released = true;
                    }
                }
            }

            if events.is_empty() {
                None
            } else {
                Some(events)
            }
        }
        4 => {
            // wl_surface.set_opaque_region
            let region = match message.args.first() {
                Some(Arg::Object(id)) => *id,
                _ => None,
            };
            if let Some(surface) = client.surfaces.get_mut(&surface_id) {
                surface.opaque_region = region;
            }
            None
        }
        5 => {
            // wl_surface.set_input_region
            let region = match message.args.first() {
                Some(Arg::Object(id)) => *id,
                _ => None,
            };
            if let Some(surface) = client.surfaces.get_mut(&surface_id) {
                surface.input_region = region;
            }
            None
        }
        7 => {
            // wl_surface.set_buffer_transform
            if let Some(surface) = client.surfaces.get_mut(&surface_id) {
                surface.buffer_transform = arg_u32(&message.args, 0);
            }
            None
        }
        8 => {
            // wl_surface.set_buffer_scale
            if let Some(surface) = client.surfaces.get_mut(&surface_id) {
                surface.buffer_scale = arg_i32(&message.args, 0);
            }
            None
        }
        9 => {
            // wl_surface.damage_buffer
            let rect = super::DamageRect {
                x: arg_i32(&message.args, 0),
                y: arg_i32(&message.args, 1),
                width: arg_i32(&message.args, 2),
                height: arg_i32(&message.args, 3),
            };
            if let Some(surface) = client.surfaces.get_mut(&surface_id) {
                surface.damage(rect);
            }
            None
        }
        _ => None,
    }
}

fn handle_shm_create_pool(client: &mut ClientConnection, message: &Message) -> Option<Vec<u8>> {
    // Args: [NewId(pool_id), Fd, Int(size)]
    let pool_id = match message.args.first() {
        Some(Arg::NewId(id)) => *id,
        _ => return None,
    };
    let size = match message.args.get(2) {
        Some(Arg::Int(v)) if *v > 0 => *v,
        _ => return None,
    };

    client.objects.insert(
        pool_id,
        super::ProtocolObject {
            id: pool_id,
            interface: interfaces::WL_SHM_POOL,
            version: 1,
        },
    );
    client
        .shm_pools
        .insert(pool_id, super::ShmPool::new(pool_id, size));
    let _ = message.args.first(); // fd is out-of-band; kernel pool backs SHM
    None
}

fn handle_shm_pool_request(client: &mut ClientConnection, message: &Message) -> Option<Vec<u8>> {
    let pool_id = message.header.object_id;
    match message.header.opcode {
        0 => {
            let buffer_id = match message.args.first() {
                Some(Arg::NewId(id)) => *id,
                _ => return None,
            };
            let offset = arg_i32(&message.args, 1);
            let width = arg_i32(&message.args, 2);
            let height = arg_i32(&message.args, 3);
            let stride = arg_i32(&message.args, 4);
            let format = arg_u32(&message.args, 5);

            client.objects.insert(
                buffer_id,
                super::ProtocolObject {
                    id: buffer_id,
                    interface: interfaces::WL_BUFFER,
                    version: 1,
                },
            );
            client.buffers.insert(
                buffer_id,
                super::Buffer::new(buffer_id, pool_id, offset, width, height, stride, format),
            );
            None
        }
        1 => {
            client.destroy_object(pool_id);
            None
        }
        2 => {
            let new_size = arg_i32(&message.args, 0);
            if let Some(pool) = client.shm_pools.get_mut(&pool_id) {
                pool.resize(new_size);
            }
            None
        }
        _ => None,
    }
}

fn handle_buffer_destroy(client: &mut ClientConnection, buffer_id: ObjectId) {
    client.destroy_object(buffer_id);
}

fn handle_data_device_manager_request(
    client: &mut ClientConnection,
    message: &Message,
) -> Option<Vec<u8>> {
    match message.header.opcode {
        0 => {
            // create_data_source
            let source_id = match message.args.first() {
                Some(Arg::NewId(id)) => *id,
                _ => return None,
            };
            client.objects.insert(
                source_id,
                super::ProtocolObject {
                    id: source_id,
                    interface: interfaces::WL_DATA_SOURCE,
                    version: 3,
                },
            );
            None
        }
        1 => {
            // get_data_device — binds a wl_data_device to a wl_seat
            let device_id = match message.args.first() {
                Some(Arg::NewId(id)) => *id,
                _ => return None,
            };
            client.objects.insert(
                device_id,
                super::ProtocolObject {
                    id: device_id,
                    interface: interfaces::WL_DATA_DEVICE,
                    version: 3,
                },
            );
            None
        }
        _ => None,
    }
}

fn handle_data_source_request(client: &mut ClientConnection, message: &Message) -> Option<Vec<u8>> {
    let source_id = message.header.object_id;
    match message.header.opcode {
        0 | 2 | 3 => {
            // offer/set_actions/set_actions — accept silently
            None
        }
        1 => {
            // destroy
            client.destroy_object(source_id);
            None
        }
        _ => None,
    }
}

fn handle_data_device_request(client: &mut ClientConnection, message: &Message) -> Option<Vec<u8>> {
    let device_id = message.header.object_id;
    match message.header.opcode {
        0 => {
            // start_drag — accept silently
            None
        }
        1 => {
            // set_selection — accept silently
            None
        }
        2 => {
            // release (wl_data_device v3)
            client.destroy_object(device_id);
            None
        }
        3 | 4 => {
            // motion/drop — accept silently
            None
        }
        _ => None,
    }
}

fn handle_compositor_create_region(
    client: &mut ClientConnection,
    message: &Message,
) -> Option<Vec<u8>> {
    let region_id = match message.args.first() {
        Some(Arg::NewId(id)) => *id,
        _ => return None,
    };
    client.objects.insert(
        region_id,
        super::ProtocolObject {
            id: region_id,
            interface: interfaces::WL_REGION,
            version: 1,
        },
    );
    None
}

fn handle_region_request(client: &mut ClientConnection, message: &Message) -> Option<Vec<u8>> {
    let region_id = message.header.object_id;
    match message.header.opcode {
        0 | 1 => {
            // add/subtract — accept but no-op (compositor manages damage internally)
            let _ = arg_i32(&message.args, 0);
            let _ = arg_i32(&message.args, 1);
            let _ = arg_i32(&message.args, 2);
            let _ = arg_i32(&message.args, 3);
            None
        }
        2 => {
            client.destroy_object(region_id);
            None
        }
        _ => None,
    }
}

fn handle_subcompositor_get_subsurface(
    client: &mut ClientConnection,
    message: &Message,
) -> Option<Vec<u8>> {
    let subsurface_id = match message.args.first() {
        Some(Arg::NewId(id)) => *id,
        _ => return None,
    };
    client.objects.insert(
        subsurface_id,
        super::ProtocolObject {
            id: subsurface_id,
            interface: interfaces::WL_SUBSURFACE,
            version: 1,
        },
    );
    None
}

fn handle_xdg_wm_base_request(client: &mut ClientConnection, message: &Message) -> Option<Vec<u8>> {
    match message.header.opcode {
        0 => {
            // destroy
            client.destroy_object(message.header.object_id);
            None
        }
        1 => {
            // create_positioner
            let positioner_id = match message.args.first() {
                Some(Arg::NewId(id)) => *id,
                _ => return None,
            };
            client.objects.insert(
                positioner_id,
                super::ProtocolObject {
                    id: positioner_id,
                    interface: interfaces::XDG_POSITIONER,
                    version: 1,
                },
            );
            None
        }
        2 => {
            // get_xdg_surface — creates xdg_surface bound to a wl_surface
            let xdg_surface_id = match message.args.first() {
                Some(Arg::NewId(id)) => *id,
                _ => return None,
            };
            let surface_id = match message.args.get(1) {
                Some(Arg::Object(Some(id))) => *id,
                _ => return None,
            };
            client.objects.insert(
                xdg_surface_id,
                super::ProtocolObject {
                    id: xdg_surface_id,
                    interface: interfaces::XDG_SURFACE,
                    version: 6,
                },
            );
            client
                .xdg_surface_to_surface
                .insert(xdg_surface_id, surface_id);
            None
        }
        3 => {
            // pong — accept silently
            None
        }
        _ => None,
    }
}

fn handle_xdg_surface_request(client: &mut ClientConnection, message: &Message) -> Option<Vec<u8>> {
    let xdg_surface_id = message.header.object_id;
    match message.header.opcode {
        0 => {
            // destroy
            client.destroy_object(xdg_surface_id);
            client.xdg_surface_to_surface.remove(&xdg_surface_id);
            None
        }
        1 => {
            // get_toplevel — creates xdg_toplevel
            let toplevel_id = match message.args.first() {
                Some(Arg::NewId(id)) => *id,
                _ => return None,
            };
            client.objects.insert(
                toplevel_id,
                super::ProtocolObject {
                    id: toplevel_id,
                    interface: interfaces::XDG_TOPLEVEL,
                    version: 6,
                },
            );

            // Emit xdg_toplevel.configure event (opcode 0):
            // args: width (int), height (int), states (array), serial (uint)
            let serial = client.next_input_serial();
            let mut out = Vec::new();
            out.extend_from_slice(
                &Message::new(
                    toplevel_id,
                    0, // xdg_toplevel.configure
                    vec![
                        Arg::Int(0),            // width — 0 means use preferred size
                        Arg::Int(0),            // height
                        Arg::Array(Vec::new()), // states — empty = no special state
                    ],
                )
                .encode(),
            );

            // Emit xdg_surface.configure event (opcode 0): serial
            out.extend_from_slice(
                &Message::new(
                    xdg_surface_id,
                    0, // xdg_surface.configure
                    vec![Arg::UInt(serial)],
                )
                .encode(),
            );

            Some(out)
        }
        2 => {
            // get_popup — create xdg_popup (simplified: just register the object)
            let popup_id = match message.args.first() {
                Some(Arg::NewId(id)) => *id,
                _ => return None,
            };
            client.objects.insert(
                popup_id,
                super::ProtocolObject {
                    id: popup_id,
                    interface: interfaces::XDG_POPUP,
                    version: 6,
                },
            );

            // Emit xdg_popup.configure (opcode 0): x, y, width, height
            let serial = client.next_input_serial();
            let mut out = Vec::new();
            out.extend_from_slice(
                &Message::new(
                    popup_id,
                    0,
                    vec![Arg::Int(0), Arg::Int(0), Arg::Int(0), Arg::Int(0)],
                )
                .encode(),
            );
            out.extend_from_slice(
                &Message::new(
                    xdg_surface_id,
                    0, // xdg_surface.configure
                    vec![Arg::UInt(serial)],
                )
                .encode(),
            );
            Some(out)
        }
        3 => {
            // set_window_geometry — accept silently
            None
        }
        4 => {
            // ack_configure — accept silently
            None
        }
        _ => None,
    }
}

fn handle_xdg_popup_request(client: &mut ClientConnection, message: &Message) -> Option<Vec<u8>> {
    let popup_id = message.header.object_id;
    match message.header.opcode {
        0 => {
            client.destroy_object(popup_id);
            None
        }
        1 | 2 => None,
        _ => None,
    }
}

fn handle_xdg_toplevel_request(
    client: &mut ClientConnection,
    message: &Message,
) -> Option<Vec<u8>> {
    let toplevel_id = message.header.object_id;
    match message.header.opcode {
        0 => {
            // destroy
            client.destroy_object(toplevel_id);
            None
        }
        1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 | 12 | 13 => {
            // All toplevel configuration requests (set_parent, set_title,
            // set_app_id, show_window_menu, move, resize, set_max/min_size,
            // set/unset_maximized, set/unset_fullscreen, set_minimized) —
            // accept silently. The compositor acknowledges via configure events.
            None
        }
        _ => None,
    }
}

fn handle_xdg_positioner_request(
    client: &mut ClientConnection,
    message: &Message,
) -> Option<Vec<u8>> {
    let positioner_id = message.header.object_id;
    match message.header.opcode {
        0 => {
            // destroy
            client.destroy_object(positioner_id);
            None
        }
        _ => {
            // All positioner configuration requests (set_size, set_anchor,
            // set_anchor_rect, set_gravity, set_constraint_adjustment,
            // set_offset) — accept silently
            None
        }
    }
}

fn handle_seat_request(
    pipe_id: u32,
    client: &mut ClientConnection,
    message: &Message,
) -> Option<Vec<u8>> {
    let seat_id = message.header.object_id;
    match message.header.opcode {
        0 => {
            let pointer_id = match message.args.first() {
                Some(Arg::NewId(id)) => *id,
                _ => return None,
            };
            let events = super::input::seat_get_pointer(client, seat_id, pointer_id);
            if events.is_empty() {
                None
            } else {
                Some(events)
            }
        }
        1 => {
            let keyboard_id = match message.args.first() {
                Some(Arg::NewId(id)) => *id,
                _ => return None,
            };
            let events = super::input::seat_get_keyboard(client, seat_id, keyboard_id)?;
            if let Some(keymap_pipe) = super::input::take_keyboard_keymap_pipe(client, keyboard_id)
            {
                crate::linux_compat::socket_ops::queue_wayland_pipe_out_of_band_fds(
                    pipe_id,
                    &[keymap_pipe],
                );
            }
            Some(events)
        }
        2 | 3 => None,
        _ => None,
    }
}

fn arg_i32(args: &[Arg], index: usize) -> i32 {
    match args.get(index) {
        Some(Arg::Int(v)) => *v,
        _ => 0,
    }
}

fn arg_u32(args: &[Arg], index: usize) -> u32 {
    match args.get(index) {
        Some(Arg::UInt(v)) => *v,
        _ => 0,
    }
}

/// Verify attach + sync + get_registry dispatch without a live socket fd.
pub fn smoke_check() -> Result<(), &'static str> {
    detach_connection(SMOKE_TEST_PIPE);
    attach_connection(SMOKE_TEST_PIPE)?;

    let sync = Message::new(DISPLAY_OBJECT_ID, 0, vec![Arg::NewId(2)]);
    let sync_reply = process_wire_request(&sync.encode(), SMOKE_TEST_PIPE)
        .ok_or("wl_display.sync produced no reply")?;
    if sync_reply.is_empty() {
        return Err("wl_display.sync reply empty");
    }

    let registry = Message::new(DISPLAY_OBJECT_ID, 1, vec![Arg::NewId(3)]);
    let registry_reply = process_wire_request(&registry.encode(), SMOKE_TEST_PIPE)
        .ok_or("wl_display.get_registry produced no reply")?;
    if registry_reply.len() < super::MessageHeader::SIZE {
        return Err("wl_display.get_registry reply too short");
    }

    let bind_compositor = Message::new(
        3,
        0,
        vec![
            Arg::UInt(1),
            Arg::String(String::from(interfaces::WL_COMPOSITOR)),
            Arg::UInt(4),
            Arg::NewId(4),
        ],
    );
    let _ = process_wire_request(&bind_compositor.encode(), SMOKE_TEST_PIPE);
    let client_id = *PIPE_CLIENTS
        .lock()
        .get(&SMOKE_TEST_PIPE)
        .ok_or("smoke client missing after registry bind")?;
    let bound = compositor()
        .get_client(client_id)
        .and_then(|client| client.objects.get(&4))
        .map(|object| object.interface == interfaces::WL_COMPOSITOR)
        .unwrap_or(false);
    if !bound {
        return Err("standard wl_registry.bind did not bind wl_compositor");
    }

    super::render::smoke_check()?;

    detach_connection(SMOKE_TEST_PIPE);
    Ok(())
}

/// Forward queued kernel input events to connected Wayland clients.
pub fn poll_kernel_input() {
    use crate::drivers::input_manager::{self, InputEvent};
    use crate::keyboard::KeyEvent;
    use crate::process::ipc::get_ipc_manager;

    let Some(mut comp) = super::try_compositor_mut() else {
        return;
    };

    loop {
        let event = match input_manager::get_event() {
            Some(event) => event,
            None => break,
        };

        let pipe_ids: Vec<u32> = PIPE_CLIENTS.lock().keys().copied().collect();
        for pipe_id in pipe_ids {
            let client_id = match PIPE_CLIENTS.lock().get(&pipe_id).copied() {
                Some(id) => id,
                None => continue,
            };

            let client = match comp.get_client_mut(client_id) {
                Some(client) => client,
                None => continue,
            };

            let focused_surface = client
                .seats
                .values()
                .find_map(|seat| seat.focused_surface)
                .or_else(|| client.surfaces.keys().next().copied());

            let wire_events = match event {
                InputEvent::MouseMove { x, y } => focused_surface
                    .map(|surface_id| {
                        super::input::inject_pointer_motion(client, surface_id, x as i32, y as i32)
                    })
                    .unwrap_or_default(),
                InputEvent::MouseButtonDown { button, x, y } => {
                    pointer_button_events(client, focused_surface, button, x, y, true)
                }
                InputEvent::MouseButtonUp { button, x, y } => {
                    pointer_button_events(client, focused_surface, button, x, y, false)
                }
                InputEvent::KeyPress(key) => {
                    super::input::inject_keyboard_key(client, key_event_code(key), true)
                }
                InputEvent::KeyRelease(key) => {
                    super::input::inject_keyboard_key(client, key_event_code(key), false)
                }
                InputEvent::MouseScroll { .. } => Vec::new(),
            };

            if wire_events.is_empty() {
                continue;
            }

            let ipc = get_ipc_manager();
            let _ = ipc.pipe_write(pipe_id, &wire_events);
        }
    }
}

fn pointer_button_events(
    client: &mut ClientConnection,
    surface_id: Option<ObjectId>,
    button: crate::drivers::input_manager::MouseButton,
    x: usize,
    y: usize,
    pressed: bool,
) -> Vec<u8> {
    let Some(surface_id) = surface_id else {
        return Vec::new();
    };

    let mut out = super::input::inject_pointer_motion(client, surface_id, x as i32, y as i32);
    let button_code = match button {
        crate::drivers::input_manager::MouseButton::Left => 272,
        crate::drivers::input_manager::MouseButton::Right => 273,
        crate::drivers::input_manager::MouseButton::Middle => 274,
        crate::drivers::input_manager::MouseButton::Button4 => 275,
        crate::drivers::input_manager::MouseButton::Button5 => 276,
    };
    let state = if pressed { 1u32 } else { 0u32 };

    for seat in client.seats.values() {
        let Some(pointer_id) = seat.pointer_id else {
            continue;
        };
        let serial = client.next_input_serial();
        out.extend_from_slice(
            &Message::new(
                pointer_id,
                3,
                vec![
                    Arg::UInt(serial),
                    Arg::UInt(0),
                    Arg::UInt(button_code),
                    Arg::UInt(state),
                ],
            )
            .encode(),
        );
        let frame_serial = client.next_input_serial();
        out.extend_from_slice(&Message::new(pointer_id, 5, vec![Arg::UInt(frame_serial)]).encode());
    }

    out
}

fn key_event_code(key: crate::keyboard::KeyEvent) -> u32 {
    use crate::keyboard::KeyEvent;
    match key {
        KeyEvent::RawPress(code) | KeyEvent::RawRelease(code) => code as u32,
        KeyEvent::CharacterPress(c) | KeyEvent::CharacterRelease(c) => c as u32,
        KeyEvent::SpecialPress(_) | KeyEvent::SpecialRelease(_) => 0,
    }
}
