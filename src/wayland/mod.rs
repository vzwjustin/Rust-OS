//! Wayland display server protocol for RustOS.
//!
//! Implements the Wayland wire protocol codec, core protocol object model
//! (wl_display, wl_registry, wl_compositor, wl_shm, wl_surface, wl_buffer,
//! wl_output), and a compositor that renders to the kernel framebuffer.
//!
//! This is the display server layer that GNOME Shell (as a Wayland compositor)
//! would sit on top of — it provides the protocol infrastructure and surface
//! management that a compositor needs.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

pub mod core_protocol;
pub mod input;
pub mod render;
pub mod server;

// ── Wire Protocol Constants ─────────────────────────────────────────────

/// Wayland wire protocol magic value for wl_display (always object 1)
pub const DISPLAY_OBJECT_ID: u32 = 1;

/// Maximum number of objects per client
pub const MAX_OBJECTS: u32 = 4096;

// ── Object IDs ──────────────────────────────────────────────────────────

/// Wayland object ID — identifies a protocol object within a client connection.
pub type ObjectId = u32;

// ── Interface IDs ───────────────────────────────────────────────────────

/// Well-known interface names in the core Wayland protocol.
pub mod interfaces {
    pub const WL_DISPLAY: &str = "wl_display";
    pub const WL_REGISTRY: &str = "wl_registry";
    pub const WL_CALLBACK: &str = "wl_callback";
    pub const WL_COMPOSITOR: &str = "wl_compositor";
    pub const WL_SHM: &str = "wl_shm";
    pub const WL_SHM_POOL: &str = "wl_shm_pool";
    pub const WL_BUFFER: &str = "wl_buffer";
    pub const WL_DATA_OFFER: &str = "wl_data_offer";
    pub const WL_DATA_SOURCE: &str = "wl_data_source";
    pub const WL_DATA_DEVICE: &str = "wl_data_device";
    pub const WL_DATA_DEVICE_MANAGER: &str = "wl_data_device_manager";
    pub const WL_SHELL: &str = "wl_shell";
    pub const WL_SHELL_SURFACE: &str = "wl_shell_surface";
    pub const WL_SURFACE: &str = "wl_surface";
    pub const WL_SEAT: &str = "wl_seat";
    pub const WL_POINTER: &str = "wl_pointer";
    pub const WL_KEYBOARD: &str = "wl_keyboard";
    pub const WL_TOUCH: &str = "wl_touch";
    pub const WL_OUTPUT: &str = "wl_output";
    pub const WL_REGION: &str = "wl_region";
    pub const WL_SUBCOMPOSITOR: &str = "wl_subcompositor";
    pub const WL_SUBSURFACE: &str = "wl_subsurface";
    pub const XDG_WM_BASE: &str = "xdg_wm_base";
    pub const XDG_SURFACE: &str = "xdg_surface";
    pub const XDG_TOPLEVEL: &str = "xdg_toplevel";
    pub const XDG_POPUP: &str = "xdg_popup";
    pub const XDG_POSITIONER: &str = "xdg_positioner";
}

// ── Pixel Formats ───────────────────────────────────────────────────────

/// wl_shm format constants ( DRM_FORMAT_* values)
pub mod formats {
    pub const XRGB8888: u32 = 0x34325258; // 'XR24' in little-endian
    pub const ARGB8888: u32 = 0x34324152; // 'AR24'
    pub const RGB888: u32 = 0x34324752; // 'RG24'
    pub const RGB565: u32 = 0x36314752; // 'RG16'
}

// ── wl_shm error codes ──────────────────────────────────────────────────

pub mod shm_error {
    pub const INVALID_FORMAT: u32 = 0;
    pub const INVALID_FD: u32 = 1;
}

// ── wl_display error codes ──────────────────────────────────────────────

pub mod display_error {
    pub const INVALID_OBJECT: u32 = 0;
    pub const INVALID_METHOD: u32 = 1;
    pub const NO_MEMORY: u32 = 2;
    pub const IMPLEMENTATION: u32 = 3;
}

// ── Wire Protocol Message ───────────────────────────────────────────────

/// Wayland wire protocol message header (8 bytes).
///
/// Layout: [object_id: u32, opcode: u16, size: u16]
#[derive(Debug, Clone, Copy)]
pub struct MessageHeader {
    pub object_id: ObjectId,
    pub opcode: u16,
    pub size: u16, // Total message size including header
}

impl MessageHeader {
    pub const SIZE: usize = 8;

    pub fn parse(data: &[u8]) -> Result<Self, &'static str> {
        if data.len() < Self::SIZE {
            return Err("Message too short for Wayland header");
        }
        let object_id = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let opcode = u16::from_le_bytes([data[4], data[5]]);
        let size = u16::from_le_bytes([data[6], data[7]]);
        Ok(Self {
            object_id,
            opcode,
            size,
        })
    }

    pub fn encode(&self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        buf[0..4].copy_from_slice(&self.object_id.to_le_bytes());
        buf[4..6].copy_from_slice(&self.opcode.to_le_bytes());
        buf[6..8].copy_from_slice(&self.size.to_le_bytes());
        buf
    }
}

// ── Argument Types ──────────────────────────────────────────────────────

/// Wayland wire protocol argument types.
#[derive(Debug, Clone)]
pub enum Arg {
    Int(i32),
    UInt(u32),
    Fixed(i32), // 24.8 fixed point
    String(String),
    Object(Option<ObjectId>),
    NewId(ObjectId),
    Array(Vec<u8>),
    Fd(i32),
}

impl Arg {
    /// Get the wire size of this argument in bytes (excluding padding).
    pub fn wire_size(&self) -> usize {
        match self {
            Arg::Int(_) | Arg::UInt(_) | Arg::Fixed(_) | Arg::Object(_) | Arg::NewId(_) => 4,
            Arg::String(s) => 4 + s.len() + 1, // length + string + null
            Arg::Array(a) => 4 + a.len(),      // length + data
            Arg::Fd(_) => 0,                   // FDs are passed out-of-band
        }
    }

    /// Marshal this argument into a byte buffer.
    pub fn marshal(&self, buf: &mut Vec<u8>) {
        match self {
            Arg::Int(v) => buf.extend_from_slice(&v.to_le_bytes()),
            Arg::UInt(v) => buf.extend_from_slice(&v.to_le_bytes()),
            Arg::Fixed(v) => buf.extend_from_slice(&v.to_le_bytes()),
            Arg::Object(v) => {
                buf.extend_from_slice(&v.unwrap_or(0).to_le_bytes());
            }
            Arg::NewId(v) => buf.extend_from_slice(&v.to_le_bytes()),
            Arg::String(s) => {
                buf.extend_from_slice(&(s.len() as u32).to_le_bytes());
                buf.extend_from_slice(s.as_bytes());
                buf.push(0); // null terminator
                             // Pad to 4-byte boundary
                let pad = (4 - ((s.len() + 1) % 4)) % 4;
                for _ in 0..pad {
                    buf.push(0);
                }
            }
            Arg::Array(a) => {
                buf.extend_from_slice(&(a.len() as u32).to_le_bytes());
                buf.extend_from_slice(a);
                let pad = (4 - (a.len() % 4)) % 4;
                for _ in 0..pad {
                    buf.push(0);
                }
            }
            Arg::Fd(_) => {} // Handled out-of-band
        }
    }

    /// Unmarshal an argument of the given type from a byte buffer.
    pub fn unmarshal(
        data: &[u8],
        offset: &mut usize,
        arg_type: &ArgType,
    ) -> Result<Self, &'static str> {
        match arg_type {
            ArgType::Int => {
                if *offset + 4 > data.len() {
                    return Err("Not enough data for int argument");
                }
                let v = i32::from_le_bytes([
                    data[*offset],
                    data[*offset + 1],
                    data[*offset + 2],
                    data[*offset + 3],
                ]);
                *offset += 4;
                Ok(Arg::Int(v))
            }
            ArgType::UInt => {
                if *offset + 4 > data.len() {
                    return Err("Not enough data for uint argument");
                }
                let v = u32::from_le_bytes([
                    data[*offset],
                    data[*offset + 1],
                    data[*offset + 2],
                    data[*offset + 3],
                ]);
                *offset += 4;
                Ok(Arg::UInt(v))
            }
            ArgType::Fixed => {
                if *offset + 4 > data.len() {
                    return Err("Not enough data for fixed argument");
                }
                let v = i32::from_le_bytes([
                    data[*offset],
                    data[*offset + 1],
                    data[*offset + 2],
                    data[*offset + 3],
                ]);
                *offset += 4;
                Ok(Arg::Fixed(v))
            }
            ArgType::String => {
                if *offset + 4 > data.len() {
                    return Err("Not enough data for string length");
                }
                let len = u32::from_le_bytes([
                    data[*offset],
                    data[*offset + 1],
                    data[*offset + 2],
                    data[*offset + 3],
                ]) as usize;
                *offset += 4;
                if *offset + len > data.len() {
                    return Err("String length exceeds message bounds");
                }
                let s = core::str::from_utf8(&data[*offset..*offset + len - 1])
                    .map_err(|_| "Invalid UTF-8 in Wayland string")?;
                *offset += len;
                // Pad to 4-byte boundary
                let pad = (4 - (len % 4)) % 4;
                *offset += pad;
                Ok(Arg::String(s.to_string()))
            }
            ArgType::Object => {
                if *offset + 4 > data.len() {
                    return Err("Not enough data for object argument");
                }
                let v = u32::from_le_bytes([
                    data[*offset],
                    data[*offset + 1],
                    data[*offset + 2],
                    data[*offset + 3],
                ]);
                *offset += 4;
                Ok(Arg::Object(if v == 0 { None } else { Some(v) }))
            }
            ArgType::NewId => {
                if *offset + 4 > data.len() {
                    return Err("Not enough data for new_id argument");
                }
                let v = u32::from_le_bytes([
                    data[*offset],
                    data[*offset + 1],
                    data[*offset + 2],
                    data[*offset + 3],
                ]);
                *offset += 4;
                Ok(Arg::NewId(v))
            }
            ArgType::Array => {
                if *offset + 4 > data.len() {
                    return Err("Not enough data for array length");
                }
                let len = u32::from_le_bytes([
                    data[*offset],
                    data[*offset + 1],
                    data[*offset + 2],
                    data[*offset + 3],
                ]) as usize;
                *offset += 4;
                if *offset + len > data.len() {
                    return Err("Array length exceeds message bounds");
                }
                let arr = data[*offset..*offset + len].to_vec();
                *offset += len;
                let pad = (4 - (len % 4)) % 4;
                *offset += pad;
                Ok(Arg::Array(arr))
            }
            ArgType::Fd => Ok(Arg::Fd(0)), // FDs handled out-of-band
        }
    }
}

/// Argument type descriptor for parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgType {
    Int,
    UInt,
    Fixed,
    String,
    Object,
    NewId,
    Array,
    Fd,
}

// ── Message ─────────────────────────────────────────────────────────────

/// A complete Wayland protocol message.
#[derive(Debug, Clone)]
pub struct Message {
    pub header: MessageHeader,
    pub args: Vec<Arg>,
}

impl Message {
    pub fn new(object_id: ObjectId, opcode: u16, args: Vec<Arg>) -> Self {
        let mut size = MessageHeader::SIZE as u16;
        for arg in &args {
            size += arg.wire_size() as u16;
            // Account for padding
            match arg {
                Arg::String(s) => {
                    let pad = (4 - ((s.len() + 1) % 4)) % 4;
                    size += pad as u16;
                }
                Arg::Array(a) => {
                    let pad = (4 - (a.len() % 4)) % 4;
                    size += pad as u16;
                }
                _ => {}
            }
        }
        Self {
            header: MessageHeader {
                object_id,
                opcode,
                size,
            },
            args,
        }
    }

    /// Encode this message into a byte buffer.
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(self.header.size as usize);
        buf.extend_from_slice(&self.header.encode());
        for arg in &self.args {
            arg.marshal(&mut buf);
        }
        buf
    }

    /// Decode a message from a byte buffer.
    pub fn decode(data: &[u8], arg_types: &[ArgType]) -> Result<Self, &'static str> {
        let header = MessageHeader::parse(data)?;
        if header.size as usize > data.len() {
            return Err("Message size exceeds available data");
        }

        let mut offset = MessageHeader::SIZE;
        let mut args = Vec::with_capacity(arg_types.len());
        for arg_type in arg_types {
            let arg = Arg::unmarshal(data, &mut offset, arg_type)?;
            args.push(arg);
        }

        Ok(Self { header, args })
    }
}

// ── Protocol Object ─────────────────────────────────────────────────────

/// A Wayland protocol object.
#[derive(Debug)]
pub struct ProtocolObject {
    pub id: ObjectId,
    pub interface: &'static str,
    pub version: u32,
}

// ── Surface ─────────────────────────────────────────────────────────────

/// Surface damage region (rectangle)
#[derive(Debug, Clone, Copy)]
pub struct DamageRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// A composited surface with attached buffer.
#[derive(Debug)]
pub struct Surface {
    pub id: ObjectId,
    pub buffer: Option<ObjectId>,
    pub buffer_scale: i32,
    pub buffer_transform: u32,
    pub damage: Vec<DamageRect>,
    pub opaque_region: Option<ObjectId>,
    pub input_region: Option<ObjectId>,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub committed: bool,
    pub frame_callback: Option<ObjectId>,
    /// Output object this surface has received wl_surface.enter for.
    pub entered_output: Option<ObjectId>,
}

impl Surface {
    pub fn new(id: ObjectId) -> Self {
        Self {
            id,
            buffer: None,
            buffer_scale: 1,
            buffer_transform: 0,
            damage: Vec::new(),
            opaque_region: None,
            input_region: None,
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            committed: false,
            frame_callback: None,
            entered_output: None,
        }
    }

    pub fn attach(&mut self, buffer: Option<ObjectId>) {
        self.buffer = buffer;
    }

    pub fn damage(&mut self, rect: DamageRect) {
        self.damage.push(rect);
    }

    pub fn commit(&mut self) {
        self.committed = true;
    }
}

// ── Buffer ──────────────────────────────────────────────────────────────

/// A buffer backed by shared memory.
#[derive(Debug)]
pub struct Buffer {
    pub id: ObjectId,
    pub pool_id: ObjectId,
    pub offset: i32,
    pub width: i32,
    pub height: i32,
    pub stride: i32,
    pub format: u32,
    pub released: bool,
}

impl Buffer {
    pub fn new(
        id: ObjectId,
        pool_id: ObjectId,
        offset: i32,
        width: i32,
        height: i32,
        stride: i32,
        format: u32,
    ) -> Self {
        Self {
            id,
            pool_id,
            offset,
            width,
            height,
            stride,
            format,
            released: false,
        }
    }
}

// ── SHM Pool ────────────────────────────────────────────────────────────

/// Shared memory pool for wl_shm.
#[derive(Debug)]
pub struct ShmPool {
    pub id: ObjectId,
    pub size: i32,
    pub data: Vec<u8>,
    pub ref_count: u32,
}

impl ShmPool {
    pub fn new(id: ObjectId, size: i32) -> Self {
        Self {
            id,
            size,
            data: vec![0u8; size as usize],
            ref_count: 1,
        }
    }

    pub fn resize(&mut self, new_size: i32) {
        if new_size > self.size {
            self.data.resize(new_size as usize, 0);
            self.size = new_size;
        }
    }
}

// ── Output ──────────────────────────────────────────────────────────────

/// Display output (monitor).
#[derive(Debug, Clone)]
pub struct Output {
    pub id: ObjectId,
    pub x: i32,
    pub y: i32,
    pub physical_width: i32,
    pub physical_height: i32,
    pub subpixel: u32,
    pub make: String,
    pub model: String,
    pub transform: u32,
    pub scale: i32,
    pub mode: OutputMode,
}

#[derive(Debug, Clone)]
pub struct OutputMode {
    pub width: i32,
    pub height: i32,
    pub refresh: i32, // mHz
    pub preferred: bool,
    pub current: bool,
}

impl Output {
    pub fn new(id: ObjectId, width: i32, height: i32) -> Self {
        Self {
            id,
            x: 0,
            y: 0,
            physical_width: 510,
            physical_height: 287,
            subpixel: 0, // unknown
            make: "RustOS".to_string(),
            model: "Virtual Display".to_string(),
            transform: 0, // normal
            scale: 1,
            mode: OutputMode {
                width,
                height,
                refresh: 60000, // 60 Hz in mHz
                preferred: true,
                current: true,
            },
        }
    }
}

// ── Client Connection ───────────────────────────────────────────────────

// ── Seat / input role tracking ──────────────────────────────────────────

/// Per-client wl_seat state.
#[derive(Debug, Clone)]
pub struct SeatRole {
    pub seat_id: ObjectId,
    pub pointer_id: Option<ObjectId>,
    pub keyboard_id: Option<ObjectId>,
    pub focused_surface: Option<ObjectId>,
    pub serial: u32,
}

/// Wayland client connection state.
#[derive(Debug)]
pub struct ClientConnection {
    pub id: u32,
    pub objects: BTreeMap<ObjectId, ProtocolObject>,
    pub surfaces: BTreeMap<ObjectId, Surface>,
    pub buffers: BTreeMap<ObjectId, Buffer>,
    pub shm_pools: BTreeMap<ObjectId, ShmPool>,
    pub seats: BTreeMap<ObjectId, SeatRole>,
    pub pointers: BTreeMap<ObjectId, ObjectId>,
    pub keyboards: BTreeMap<ObjectId, ObjectId>,
    pub keyboard_keymap_pipes: BTreeMap<ObjectId, u32>,
    pub xdg_surface_to_surface: BTreeMap<ObjectId, ObjectId>,
    pub next_object_id: AtomicU32,
    pub display_serial: AtomicU32,
    pub input_serial: AtomicU32,
}

impl ClientConnection {
    pub fn new(id: u32) -> Self {
        let mut objects = BTreeMap::new();
        // wl_display is always object 1
        objects.insert(
            DISPLAY_OBJECT_ID,
            ProtocolObject {
                id: DISPLAY_OBJECT_ID,
                interface: interfaces::WL_DISPLAY,
                version: 1,
            },
        );
        Self {
            id,
            objects,
            surfaces: BTreeMap::new(),
            buffers: BTreeMap::new(),
            shm_pools: BTreeMap::new(),
            seats: BTreeMap::new(),
            pointers: BTreeMap::new(),
            keyboards: BTreeMap::new(),
            keyboard_keymap_pipes: BTreeMap::new(),
            xdg_surface_to_surface: BTreeMap::new(),
            next_object_id: AtomicU32::new(2), // Start after wl_display
            display_serial: AtomicU32::new(1),
            input_serial: AtomicU32::new(1),
        }
    }

    pub fn alloc_object_id(&self) -> ObjectId {
        self.next_object_id.fetch_add(1, Ordering::Relaxed)
    }

    pub fn create_surface(&mut self) -> ObjectId {
        let id = self.alloc_object_id();
        self.objects.insert(
            id,
            ProtocolObject {
                id,
                interface: interfaces::WL_SURFACE,
                version: 4,
            },
        );
        self.surfaces.insert(id, Surface::new(id));
        id
    }

    pub fn create_buffer(
        &mut self,
        pool_id: ObjectId,
        offset: i32,
        width: i32,
        height: i32,
        stride: i32,
        format: u32,
    ) -> ObjectId {
        let id = self.alloc_object_id();
        self.objects.insert(
            id,
            ProtocolObject {
                id,
                interface: interfaces::WL_BUFFER,
                version: 1,
            },
        );
        self.buffers.insert(
            id,
            Buffer::new(id, pool_id, offset, width, height, stride, format),
        );
        id
    }

    pub fn create_shm_pool(&mut self, size: i32) -> ObjectId {
        let id = self.alloc_object_id();
        self.objects.insert(
            id,
            ProtocolObject {
                id,
                interface: interfaces::WL_SHM_POOL,
                version: 1,
            },
        );
        self.shm_pools.insert(id, ShmPool::new(id, size));
        id
    }

    pub fn destroy_object(&mut self, id: ObjectId) {
        self.objects.remove(&id);
        self.surfaces.remove(&id);
        self.buffers.remove(&id);
        self.shm_pools.remove(&id);
        self.seats.remove(&id);
        self.pointers.remove(&id);
        self.keyboards.remove(&id);
    }

    pub fn next_input_serial(&self) -> u32 {
        self.input_serial.fetch_add(1, Ordering::Relaxed)
    }

    pub fn next_serial(&self) -> u32 {
        self.display_serial.fetch_add(1, Ordering::Relaxed)
    }
}

// ── Compositor ──────────────────────────────────────────────────────────

/// The Wayland compositor — manages clients, surfaces, and renders to the
/// framebuffer.
pub struct Compositor {
    clients: BTreeMap<u32, ClientConnection>,
    outputs: BTreeMap<ObjectId, Output>,
    next_client_id: AtomicU32,
    initialized: bool,
}

impl Compositor {
    pub const fn new() -> Self {
        Self {
            clients: BTreeMap::new(),
            outputs: BTreeMap::new(),
            next_client_id: AtomicU32::new(1),
            initialized: false,
        }
    }

    pub fn init(&mut self) -> Result<(), &'static str> {
        if self.initialized {
            return Ok(());
        }

        // Create a default output based on the current framebuffer dimensions
        let (width, height) = if let Some((w, h)) = crate::graphics::get_screen_dimensions() {
            (w as i32, h as i32)
        } else {
            (1024, 768)
        };

        let output = Output::new(2, width, height);
        self.outputs.insert(output.id, output);

        self.initialized = true;
        Ok(())
    }

    /// Accept a new client connection.
    pub fn connect_client(&mut self) -> u32 {
        let id = self.next_client_id.fetch_add(1, Ordering::Relaxed);
        let conn = ClientConnection::new(id);
        self.clients.insert(id, conn);
        id
    }

    /// Disconnect a client.
    pub fn disconnect_client(&mut self, client_id: u32) {
        self.clients.remove(&client_id);
    }

    /// Get a client connection.
    pub fn get_client(&self, client_id: u32) -> Option<&ClientConnection> {
        self.clients.get(&client_id)
    }

    /// Get a mutable client connection.
    pub fn get_client_mut(&mut self, client_id: u32) -> Option<&mut ClientConnection> {
        self.clients.get_mut(&client_id)
    }

    /// List all outputs.
    pub fn list_outputs(&self) -> Vec<&Output> {
        self.outputs.values().collect()
    }

    /// Check if compositor is initialized.
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Get the list of global objects for wl_registry.
    pub fn global_list(&self) -> Vec<GlobalEntry> {
        let mut globals = Vec::new();

        // wl_compositor (always present)
        globals.push(GlobalEntry {
            name: 1,
            interface: interfaces::WL_COMPOSITOR,
            version: 4,
        });

        // wl_shm
        globals.push(GlobalEntry {
            name: 2,
            interface: interfaces::WL_SHM,
            version: 1,
        });

        // wl_output
        globals.push(GlobalEntry {
            name: 3,
            interface: interfaces::WL_OUTPUT,
            version: 3,
        });

        // wl_seat
        globals.push(GlobalEntry {
            name: 4,
            interface: interfaces::WL_SEAT,
            version: 7,
        });

        // wl_data_device_manager
        globals.push(GlobalEntry {
            name: 5,
            interface: interfaces::WL_DATA_DEVICE_MANAGER,
            version: 3,
        });

        // wl_subcompositor
        globals.push(GlobalEntry {
            name: 6,
            interface: interfaces::WL_SUBCOMPOSITOR,
            version: 1,
        });

        // xdg_wm_base (required by Mutter for window management)
        globals.push(GlobalEntry {
            name: 7,
            interface: interfaces::XDG_WM_BASE,
            version: 6,
        });

        globals
    }
}

/// A global registry entry.
#[derive(Debug, Clone)]
pub struct GlobalEntry {
    pub name: u32,
    pub interface: &'static str,
    pub version: u32,
}

// ── Global Compositor Instance ──────────────────────────────────────────

static COMPOSITOR: RwLock<Compositor> = RwLock::new(Compositor::new());

/// Initialize the Wayland compositor.
pub fn init() -> Result<(), &'static str> {
    crate::interrupts::without_interrupts(|| {
        {
            let mut comp = COMPOSITOR.write();
            comp.init()?;
        }

        if let Err(e) = server::smoke_check() {
            unsafe {
                crate::early_serial_write_str("RustOS: Wayland handshake smoke FAILED: ");
                crate::early_serial_write_str(e);
                crate::early_serial_write_str("\r\n");
            }
        } else {
            server::mark_handshake_ready();
            unsafe {
                crate::early_serial_write_str("RustOS: Wayland wire handshake ready\r\n");
            }
        }

        unsafe {
            crate::early_serial_write_str("RustOS: Wayland compositor initialized\r\n");
        }
        Ok(())
    })
}

/// Get a read reference to the global compositor.
pub fn compositor() -> spin::rwlock::RwLockReadGuard<'static, Compositor> {
    COMPOSITOR.read()
}

/// Get a write reference to the global compositor.
pub fn compositor_mut() -> spin::rwlock::RwLockWriteGuard<'static, Compositor> {
    COMPOSITOR.write()
}

/// Try to acquire the compositor write lock without blocking (IRQ-safe).
pub fn try_compositor_mut() -> Option<spin::rwlock::RwLockWriteGuard<'static, Compositor>> {
    COMPOSITOR.try_write()
}

/// Check if the Wayland compositor is initialized.
pub fn is_ready() -> bool {
    COMPOSITOR.read().is_initialized()
}

/// Drain kernel input events and forward them to Wayland clients.
pub fn poll_input() {
    if is_ready() {
        server::poll_kernel_input();
    }
}

/// Composite all connected clients' committed surfaces to the framebuffer.
/// Called from the desktop main loop after the desktop render pass.
pub fn render_clients() {
    if !is_ready() {
        return;
    }
    let comp = COMPOSITOR.read();
    for (_id, client) in &comp.clients {
        for (_surface_id, surface) in &client.surfaces {
            if surface.buffer.is_some() {
                render::render_surface(client, surface);
            }
        }
    }
}

/// Count connected Wayland clients (excluding smoke-test pipe).
pub fn client_count() -> usize {
    if !is_ready() {
        return 0;
    }
    let comp = COMPOSITOR.read();
    comp.clients.len()
}

// ── wl_display Event Constructors ───────────────────────────────────────

/// Build a wl_display.error event.
pub fn event_error(object_id: ObjectId, code: u32, message: &str) -> Message {
    Message::new(
        DISPLAY_OBJECT_ID,
        0, // WL_DISPLAY_ERROR
        vec![
            Arg::Object(Some(object_id)),
            Arg::UInt(code),
            Arg::String(message.to_string()),
        ],
    )
}

/// Build a wl_display.delete_id event.
pub fn event_delete_id(id: ObjectId) -> Message {
    Message::new(
        DISPLAY_OBJECT_ID,
        1, // WL_DISPLAY_DELETE_ID
        vec![Arg::UInt(id)],
    )
}

// ── wl_registry Event Constructors ──────────────────────────────────────

/// Build a wl_registry.global event.
pub fn event_global(registry_id: ObjectId, name: u32, interface: &str, version: u32) -> Message {
    Message::new(
        registry_id,
        0, // WL_REGISTRY_GLOBAL
        vec![
            Arg::UInt(name),
            Arg::String(interface.to_string()),
            Arg::UInt(version),
        ],
    )
}

/// Build a wl_registry.global_remove event.
pub fn event_global_remove(registry_id: ObjectId, name: u32) -> Message {
    Message::new(
        registry_id,
        1, // WL_REGISTRY_GLOBAL_REMOVE
        vec![Arg::UInt(name)],
    )
}

// ── wl_output Event Constructors ────────────────────────────────────────

/// Build a wl_output.geometry event.
pub fn event_output_geometry(
    output_id: ObjectId,
    x: i32,
    y: i32,
    physical_width: i32,
    physical_height: i32,
    subpixel: u32,
    make: &str,
    model: &str,
    transform: u32,
) -> Message {
    Message::new(
        output_id,
        0, // WL_OUTPUT_GEOMETRY
        vec![
            Arg::Int(x),
            Arg::Int(y),
            Arg::Int(physical_width),
            Arg::Int(physical_height),
            Arg::UInt(subpixel),
            Arg::String(make.to_string()),
            Arg::String(model.to_string()),
            Arg::UInt(transform),
        ],
    )
}

/// Build a wl_output.mode event.
pub fn event_output_mode(
    output_id: ObjectId,
    flags: u32,
    width: i32,
    height: i32,
    refresh: i32,
) -> Message {
    Message::new(
        output_id,
        1, // WL_OUTPUT_MODE
        vec![
            Arg::UInt(flags),
            Arg::Int(width),
            Arg::Int(height),
            Arg::Int(refresh),
        ],
    )
}

// ── wl_surface Event Constructors ───────────────────────────────────────

/// Build a wl_surface.enter event (surface entered an output).
pub fn event_surface_enter(surface_id: ObjectId, output_id: ObjectId) -> Message {
    Message::new(
        surface_id,
        0, // WL_SURFACE_ENTER
        vec![Arg::Object(Some(output_id))],
    )
}

/// Build a wl_surface.leave event.
pub fn event_surface_leave(surface_id: ObjectId, output_id: ObjectId) -> Message {
    Message::new(
        surface_id,
        1, // WL_SURFACE_LEAVE
        vec![Arg::Object(Some(output_id))],
    )
}

// ── Smoke Test ──────────────────────────────────────────────────────────

/// Verify Wayland wire protocol round-trip works.
pub fn smoke_check() -> Result<(), &'static str> {
    // Test message encode/decode round-trip
    let msg = Message::new(
        DISPLAY_OBJECT_ID,
        0,
        vec![Arg::UInt(42), Arg::String("test".to_string()), Arg::Int(-7)],
    );

    let encoded = msg.encode();
    if encoded.len() < MessageHeader::SIZE {
        return Err("Encoded message too short");
    }

    let decoded = Message::decode(&encoded, &[ArgType::UInt, ArgType::String, ArgType::Int])
        .map_err(|_| "Failed to decode message")?;

    if decoded.header.object_id != DISPLAY_OBJECT_ID {
        return Err("Object ID mismatch after round-trip");
    }
    if decoded.header.opcode != 0 {
        return Err("Opcode mismatch after round-trip");
    }

    // Verify args
    match &decoded.args[0] {
        Arg::UInt(v) if *v == 42 => {}
        _ => return Err("First argument mismatch"),
    }
    match &decoded.args[1] {
        Arg::String(s) if s == "test" => {}
        _ => return Err("String argument mismatch"),
    }
    match &decoded.args[2] {
        Arg::Int(v) if *v == -7 => {}
        _ => return Err("Int argument mismatch"),
    }

    // Test compositor initialization
    let mut comp = Compositor::new();
    comp.init()?;
    if !comp.is_initialized() {
        return Err("Compositor failed to initialize");
    }

    // Test client connection
    let client_id = comp.connect_client();
    let client = comp.get_client_mut(client_id).ok_or("Client not found")?;

    // Test surface creation
    let surface_id = client.create_surface();
    if surface_id == 0 {
        return Err("Surface ID should not be 0");
    }

    // Test global list
    let globals = comp.global_list();
    if globals.is_empty() {
        return Err("Global list should not be empty");
    }

    // Verify wl_compositor is in the global list
    if !globals
        .iter()
        .any(|g| g.interface == interfaces::WL_COMPOSITOR)
    {
        return Err("wl_compositor should be in global list");
    }

    server::smoke_check()?;

    Ok(())
}
