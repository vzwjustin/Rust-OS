//! Wayland seat, pointer, and keyboard event synthesis.

extern crate alloc;

use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;

use super::{interfaces, Arg, ClientConnection, Message, ObjectId, SeatRole};

/// wl_seat capability flags.
const SEAT_CAP_POINTER: u32 = 1;
const SEAT_CAP_KEYBOARD: u32 = 2;
const SEAT_CAP_TOUCH: u32 = 4;

/// wl_keyboard.keymap_format XKB_V1
const KEYMAP_FORMAT_XKB_V1: u32 = 1;

/// Minimal US-layout keymap for Mutter/gnome-shell keyboard setup.
const MINIMAL_XKB_KEYMAP: &[u8] = b"xkb_keymap {
  xkb_keycodes \"minimum\" {
    <ESC> = 9;
    <AE01> = 10;
    <AE02> = 11;
    <AE03> = 12;
    <SPCE> = 65;
  };
  xkb_types \"minimum\" {
    type \"ALPHANUMERIC\" {
      modifiers= none;
      map[none]= Level1;
      level_name[Level1]= \"Any\";
    };
  };
  xkb_compatibility \"minimum\" { };
  xkb_symbols \"minimum\" {
    key <ESC> { [ Escape ] };
    key <AE01> { [ 1 ] };
    key <AE02> { [ 2 ] };
    key <AE03> { [ 3 ] };
    key <SPCE> { [ space ] };
  };
};
";

fn create_keymap_pipe() -> Result<u32, &'static str> {
    let ipc = crate::process::ipc::get_ipc_manager();
    let (pipe_id, _) = ipc
        .create_pipe()
        .map_err(|_| "keymap pipe allocation failed")?;
    ipc.pipe_write(pipe_id, MINIMAL_XKB_KEYMAP)
        .map_err(|_| "keymap pipe write failed")?;
    Ok(pipe_id)
}

/// Events emitted when a client binds `wl_seat`.
pub fn seat_bind_events(seat_id: ObjectId) -> Vec<u8> {
    let caps = SEAT_CAP_POINTER | SEAT_CAP_KEYBOARD | SEAT_CAP_TOUCH;
    let mut out = Vec::new();
    out.extend_from_slice(&Message::new(seat_id, 0, vec![Arg::UInt(caps)]).encode());
    out.extend_from_slice(
        &Message::new(seat_id, 1, vec![Arg::String("seat0".to_string())]).encode(),
    );
    out
}

/// Register seat state after bind.
pub fn register_seat(client: &mut ClientConnection, seat_id: ObjectId) {
    client.seats.insert(
        seat_id,
        SeatRole {
            seat_id,
            pointer_id: None,
            keyboard_id: None,
            focused_surface: None,
            serial: 0,
        },
    );
}

/// Handle `wl_seat.get_pointer`.
pub fn seat_get_pointer(
    client: &mut ClientConnection,
    seat_id: ObjectId,
    pointer_id: ObjectId,
) -> Vec<u8> {
    client.objects.insert(
        pointer_id,
        super::ProtocolObject {
            id: pointer_id,
            interface: interfaces::WL_POINTER,
            version: 7,
        },
    );
    client.pointers.insert(pointer_id, seat_id);
    if let Some(seat) = client.seats.get_mut(&seat_id) {
        seat.pointer_id = Some(pointer_id);
    }
    Vec::new()
}

/// Handle `wl_seat.get_keyboard`.
pub fn seat_get_keyboard(
    client: &mut ClientConnection,
    seat_id: ObjectId,
    keyboard_id: ObjectId,
) -> Option<Vec<u8>> {
    client.objects.insert(
        keyboard_id,
        super::ProtocolObject {
            id: keyboard_id,
            interface: interfaces::WL_KEYBOARD,
            version: 7,
        },
    );
    client.keyboards.insert(keyboard_id, seat_id);
    if let Some(seat) = client.seats.get_mut(&seat_id) {
        seat.keyboard_id = Some(keyboard_id);
    }

    let keymap_pipe = create_keymap_pipe().ok()?;
    client
        .keyboard_keymap_pipes
        .insert(keyboard_id, keymap_pipe);
    let keymap_size = MINIMAL_XKB_KEYMAP.len() as u32;

    let serial = client.next_input_serial();
    let mut out = Vec::new();
    out.extend_from_slice(
        &Message::new(
            keyboard_id,
            0,
            vec![
                Arg::UInt(KEYMAP_FORMAT_XKB_V1),
                Arg::Fd(0),
                Arg::UInt(keymap_size),
            ],
        )
        .encode(),
    );
    out.extend_from_slice(
        &Message::new(
            keyboard_id,
            4,
            vec![
                Arg::UInt(serial),
                Arg::UInt(0),
                Arg::UInt(0),
                Arg::UInt(0),
                Arg::UInt(0),
            ],
        )
        .encode(),
    );
    Some(out)
}

/// Return the keymap pipe queued for a keyboard object, if any.
pub fn take_keyboard_keymap_pipe(
    client: &mut ClientConnection,
    keyboard_id: ObjectId,
) -> Option<u32> {
    client.keyboard_keymap_pipes.remove(&keyboard_id)
}

/// Pointer/keyboard follow-up events after a surface commit.
pub fn surface_post_commit_events(client: &mut ClientConnection, surface_id: ObjectId) -> Vec<u8> {
    let seat_ids: Vec<ObjectId> = client.seats.keys().copied().collect();
    let mut out = Vec::new();

    for seat_id in seat_ids {
        let pointer_id = client.seats.get(&seat_id).and_then(|seat| seat.pointer_id);
        let Some(pointer_id) = pointer_id else {
            continue;
        };

        let needs_enter = client
            .seats
            .get(&seat_id)
            .map(|seat| seat.focused_surface != Some(surface_id))
            .unwrap_or(false);

        if needs_enter {
            let serial = client.next_input_serial();
            if let Some(seat) = client.seats.get_mut(&seat_id) {
                seat.focused_surface = Some(surface_id);
                seat.serial = serial;
            }
            out.extend_from_slice(
                &Message::new(
                    pointer_id,
                    0,
                    vec![
                        Arg::UInt(serial),
                        Arg::Object(Some(surface_id)),
                        Arg::Fixed(0),
                        Arg::Fixed(0),
                    ],
                )
                .encode(),
            );
        }

        let frame_serial = client.next_input_serial();
        out.extend_from_slice(&Message::new(pointer_id, 5, vec![Arg::UInt(frame_serial)]).encode());
    }

    out
}

/// Deliver a synthetic pointer motion event from the kernel input stack.
pub fn inject_pointer_motion(
    client: &mut ClientConnection,
    surface_id: ObjectId,
    x: i32,
    y: i32,
) -> Vec<u8> {
    let seat_ids: Vec<ObjectId> = client.seats.keys().copied().collect();
    let mut out = Vec::new();
    let time = 0u32;

    for seat_id in seat_ids {
        let pointer_id = client.seats.get(&seat_id).and_then(|seat| seat.pointer_id);
        let Some(pointer_id) = pointer_id else {
            continue;
        };

        let needs_enter = client
            .seats
            .get(&seat_id)
            .map(|seat| seat.focused_surface != Some(surface_id))
            .unwrap_or(false);

        if needs_enter {
            let enter_serial = client.next_input_serial();
            if let Some(seat) = client.seats.get_mut(&seat_id) {
                seat.focused_surface = Some(surface_id);
            }
            out.extend_from_slice(
                &Message::new(
                    pointer_id,
                    0,
                    vec![
                        Arg::UInt(enter_serial),
                        Arg::Object(Some(surface_id)),
                        Arg::Fixed(x),
                        Arg::Fixed(y),
                    ],
                )
                .encode(),
            );
        }

        let motion_serial = client.next_input_serial();
        out.extend_from_slice(
            &Message::new(
                pointer_id,
                2,
                vec![
                    Arg::UInt(motion_serial),
                    Arg::UInt(time),
                    Arg::Fixed(x),
                    Arg::Fixed(y),
                ],
            )
            .encode(),
        );
        let frame_serial = client.next_input_serial();
        out.extend_from_slice(&Message::new(pointer_id, 5, vec![Arg::UInt(frame_serial)]).encode());
    }

    out
}

/// Deliver a synthetic keyboard key event.
pub fn inject_keyboard_key(client: &mut ClientConnection, key: u32, pressed: bool) -> Vec<u8> {
    let keyboard_ids: Vec<ObjectId> = client
        .seats
        .values()
        .filter_map(|seat| seat.keyboard_id)
        .collect();
    let mut out = Vec::new();
    let time = 0u32;
    let state = if pressed { 1u32 } else { 0u32 };

    for keyboard_id in keyboard_ids {
        let serial = client.next_input_serial();
        out.extend_from_slice(
            &Message::new(
                keyboard_id,
                2,
                vec![
                    Arg::UInt(serial),
                    Arg::UInt(time),
                    Arg::UInt(key),
                    Arg::UInt(state),
                ],
            )
            .encode(),
        );
    }

    out
}
