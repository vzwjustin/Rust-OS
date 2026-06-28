//! Wayland display socket server — per-connection wire dispatch.
//!
//! Bridges AF_UNIX Wayland socket I/O from `socket_ops` to the in-kernel
//! compositor. Handles the minimum client handshake (`sync`, `get_registry`,
//! `bind`) that Mutter and gnome-shell need before real compositor binaries
//! ship in the rootfs.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;

use super::{
    compositor, compositor_mut, display_error, event_global, event_output_geometry,
    event_output_mode, interfaces, Arg, ArgType, ClientConnection, GlobalEntry, Message,
    ObjectId, DISPLAY_OBJECT_ID,
};

static PIPE_CLIENTS: Mutex<BTreeMap<u32, u32>> = Mutex::new(BTreeMap::new());
static READ_BUFFERS: Mutex<BTreeMap<u32, Vec<u8>>> = Mutex::new(BTreeMap::new());
static HANDSHAKE_READY: AtomicBool = AtomicBool::new(false);

/// Returns true once a successful Wayland wire handshake has been observed.
pub fn is_handshake_ready() -> bool {
    HANDSHAKE_READY.load(Ordering::Acquire)
}

/// Attach a compositor client to a Unix socket connection pipe.
pub fn attach_connection(pipe_id: u32) -> Result<(), &'static str> {
    let mut map = PIPE_CLIENTS.lock();
    if map.contains_key(&pipe_id) {
        return Ok(());
    }

    if !compositor().is_initialized() {
        compositor_mut().init()?;
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
    let comp = compositor();
    let client = comp.get_client(client_id)?;
    let object = client.objects.get(&header.object_id)?;

    if object.interface == interfaces::WL_REGISTRY && header.opcode == 0 {
        return Some(vec![ArgType::UInt, ArgType::NewId, ArgType::UInt]);
    }

    match object.interface {
        interfaces::WL_COMPOSITOR if header.opcode == 0 => Some(vec![ArgType::NewId]),
        interfaces::WL_SURFACE => match header.opcode {
            0 | 3 => Some(Vec::new()),
            1 => Some(vec![ArgType::Object, ArgType::Int, ArgType::Int]),
            2 | 6 => Some(vec![
                ArgType::Int,
                ArgType::Int,
                ArgType::Int,
                ArgType::Int,
            ]),
            4 => Some(vec![ArgType::UInt]),
            5 => Some(vec![ArgType::Int]),
            7 => Some(vec![ArgType::NewId]),
            _ => None,
        },
        interfaces::WL_SEAT => match header.opcode {
            0 | 1 | 2 => Some(vec![ArgType::NewId]),
            3 => Some(Vec::new()),
            _ => None,
        },
        interfaces::WL_SHM if header.opcode == 0 => {
            Some(vec![ArgType::Fd, ArgType::NewId, ArgType::Int])
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
        _ => None,
    }
}

fn dispatch_message(_pipe_id: u32, client_id: u32, message: &Message) -> Option<Vec<u8>> {
    let mut comp = compositor_mut();

    if message.header.object_id == DISPLAY_OBJECT_ID {
        return match message.header.opcode {
            0 => {
                let client = comp.get_client_mut(client_id)?;
                handle_display_sync(client, message)
            }
            1 => {
                let globals = comp.global_list();
                let client = comp.get_client_mut(client_id)?;
                handle_display_get_registry(client, &globals, message)
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
        return handle_registry_bind(client, &globals, &outputs, message);
    }

    let object_iface = comp
        .get_client(client_id)?
        .objects
        .get(&message.header.object_id)
        .map(|obj| obj.interface)
        .unwrap_or("");

    match object_iface {
        interfaces::WL_COMPOSITOR if message.header.opcode == 0 => {
            let client = comp.get_client_mut(client_id)?;
            handle_compositor_create_surface(client, message)
        }
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
        interfaces::WL_SEAT => {
            let client = comp.get_client_mut(client_id)?;
            handle_seat_request(client, message)
        }
        _ => None,
    }
}

fn handle_display_sync(client: &mut ClientConnection, message: &Message) -> Option<Vec<u8>> {
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
    HANDSHAKE_READY.store(true, Ordering::Release);
    Some(done.encode())
}

fn handle_display_get_registry(
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

    HANDSHAKE_READY.store(true, Ordering::Release);
    Some(out)
}

fn handle_registry_bind(
    client: &mut ClientConnection,
    globals: &[GlobalEntry],
    outputs: &[super::Output],
    message: &Message,
) -> Option<Vec<u8>> {
    let (name, new_id, version) = match (&message.args[0], &message.args[1], &message.args[2]) {
        (Arg::UInt(name), Arg::NewId(id), Arg::UInt(version)) => (*name, *id, *version),
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
        interfaces::WL_COMPOSITOR | interfaces::WL_SHM
        | interfaces::WL_DATA_DEVICE_MANAGER | interfaces::WL_SUBCOMPOSITOR => {}
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
            }
        }
        _ => {}
    }

    HANDSHAKE_READY.store(true, Ordering::Release);

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
    client.surfaces.insert(surface_id, super::Surface::new(surface_id));
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
        2 | 6 => {
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
            if let Some(surface) = client.surfaces.get_mut(&surface_id) {
                surface.commit();
            }

            let mut events = if let Some(surface) = client.surfaces.get(&surface_id) {
                super::render::render_surface(client, surface);
                super::render::surface_commit_events(client, surface)
            } else {
                Vec::new()
            };

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
        7 => {
            let callback_id = match message.args.first() {
                Some(Arg::NewId(id)) => *id,
                _ => return None,
            };
            if let Some(surface) = client.surfaces.get_mut(&surface_id) {
                surface.frame_callback = Some(callback_id);
            }
            None
        }
        4 => {
            if let Some(surface) = client.surfaces.get_mut(&surface_id) {
                surface.buffer_transform = arg_u32(&message.args, 0);
            }
            None
        }
        5 => {
            if let Some(surface) = client.surfaces.get_mut(&surface_id) {
                surface.buffer_scale = arg_i32(&message.args, 0);
            }
            None
        }
        _ => None,
    }
}

fn handle_shm_create_pool(client: &mut ClientConnection, message: &Message) -> Option<Vec<u8>> {
    let size = match message.args.get(2) {
        Some(Arg::Int(v)) if *v > 0 => *v,
        _ => return None,
    };
    let pool_id = match message.args.get(1) {
        Some(Arg::NewId(id)) => *id,
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
    client.shm_pools.insert(pool_id, super::ShmPool::new(pool_id, size));
    let _ = message.args.first(); // fd is out-of-band; kernel pool backs SHM
    None
}

fn handle_shm_pool_request(client: &mut ClientConnection, message: &Message) -> Option<Vec<u8>> {
    let pool_id = message.header.object_id;
    match message.header.opcode {
        0 => {
            let offset = arg_i32(&message.args, 0);
            let width = arg_i32(&message.args, 1);
            let height = arg_i32(&message.args, 2);
            let stride = arg_i32(&message.args, 3);
            let format = arg_u32(&message.args, 4);
            let buffer_id = match message.args.get(5) {
                Some(Arg::NewId(id)) => *id,
                _ => return None,
            };

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

fn handle_seat_request(client: &mut ClientConnection, message: &Message) -> Option<Vec<u8>> {
    let seat_id = message.header.object_id;
    match message.header.opcode {
        0 => {
            let pointer_id = match message.args.first() {
                Some(Arg::NewId(id)) => *id,
                _ => return None,
            };
            Some(super::input::seat_get_pointer(client, seat_id, pointer_id))
        }
        1 => {
            let keyboard_id = match message.args.first() {
                Some(Arg::NewId(id)) => *id,
                _ => return None,
            };
            Some(super::input::seat_get_keyboard(client, seat_id, keyboard_id))
        }
        2 | 3 => None,
        _ => None,
    }
    .filter(|events| !events.is_empty())
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
    const TEST_PIPE: u32 = 0x9001;

    detach_connection(TEST_PIPE);
    attach_connection(TEST_PIPE)?;

    let sync = Message::new(DISPLAY_OBJECT_ID, 0, vec![Arg::NewId(2)]);
    let sync_reply = process_wire_request(&sync.encode(), TEST_PIPE)
        .ok_or("wl_display.sync produced no reply")?;
    if sync_reply.is_empty() {
        return Err("wl_display.sync reply empty");
    }

    let registry = Message::new(DISPLAY_OBJECT_ID, 1, vec![Arg::NewId(3)]);
    let registry_reply = process_wire_request(&registry.encode(), TEST_PIPE)
        .ok_or("wl_display.get_registry produced no reply")?;
    if registry_reply.len() < super::MessageHeader::SIZE {
        return Err("wl_display.get_registry reply too short");
    }

    if !is_handshake_ready() {
        return Err("Wayland handshake flag not set");
    }

    super::render::smoke_check()?;

    detach_connection(TEST_PIPE);
    Ok(())
}

/// Forward queued kernel input events to connected Wayland clients.
pub fn poll_kernel_input() {
    use crate::drivers::input_manager::{self, InputEvent, MouseButton};
    use crate::keyboard::KeyEvent;
    use crate::process::ipc::get_ipc_manager;

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

            let mut comp = compositor_mut();
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
                        super::input::inject_pointer_motion(
                            client,
                            surface_id,
                            x as i32,
                            y as i32,
                        )
                    })
                    .unwrap_or_default(),
                InputEvent::MouseButtonDown { button, x, y } => {
                    pointer_button_events(client, focused_surface, button, x, y, true)
                }
                InputEvent::MouseButtonUp { button, x, y } => {
                    pointer_button_events(client, focused_surface, button, x, y, false)
                }
                InputEvent::KeyPress(key) => super::input::inject_keyboard_key(
                    client,
                    key_event_code(key),
                    true,
                ),
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
        out.extend_from_slice(
            &Message::new(pointer_id, 5, vec![Arg::UInt(frame_serial)]).encode(),
        );
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
