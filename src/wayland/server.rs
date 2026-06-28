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

    None
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

    None
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
        interfaces::WL_COMPOSITOR | interfaces::WL_SHM | interfaces::WL_SEAT
        | interfaces::WL_DATA_DEVICE_MANAGER | interfaces::WL_SUBCOMPOSITOR => {}
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

    detach_connection(TEST_PIPE);
    Ok(())
}
