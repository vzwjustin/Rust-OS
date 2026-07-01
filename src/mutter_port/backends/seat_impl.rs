//! Seat Impl — Input seat implementation from GNOME Mutter
//!
//! Manages input devices (keyboard, mouse, touchscreen) for a single seat.
//! Coordinates input event processing, device state, and keyboard layout management.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-seat-impl.h

use alloc::{string::String, vec::Vec};
use core::ffi::c_void;

/// Input seat implementation for handling keyboard/mouse/touch input.
/// Maintains device lists, input context, XKB state, and input constraints.
#[derive(Debug, Clone)]
pub struct SeatImpl {
    /// GMainContext for main thread coordination (opaque pointer)
    pub main_context: *mut c_void,
    /// GMainContext for input thread (opaque pointer)
    pub input_context: *mut c_void,
    /// GMainLoop for input thread (opaque pointer)
    pub input_loop: *mut c_void,
    /// GThread handle for input processing thread (opaque pointer)
    pub input_thread: *mut c_void,
    /// Mutex for input thread initialization (opaque pointer)
    pub init_mutex: *mut c_void,
    /// Condition variable for initialization (opaque pointer)
    pub init_cond: *mut c_void,

    /// Parent MetaSeatNative (opaque pointer)
    pub seat_native: *mut c_void,
    /// Seat identifier string (e.g., "seat0")
    pub seat_id: String,
    /// Flags controlling seat behavior
    pub flags: u32,
    /// GSource for libinput events (opaque pointer)
    pub libinput_source: *mut c_void,
    /// libinput context handle (opaque pointer)
    pub libinput: *mut c_void,
    /// Read-write lock for device state (opaque pointer)
    pub state_lock: *mut c_void,

    /// Connected input devices (opaque pointers)
    pub devices: Vec<*mut c_void>,
    /// Tools hash table: device tools by id (opaque pointer)
    pub tools: *mut c_void,

    /// XKB keyboard state (opaque pointer)
    pub xkb: *mut c_void,
    /// Current XKB layout index
    pub layout_idx: u32,
    /// Button state bitmask
    pub button_state: u32,
    /// Button repeat state
    pub button_count: [i32; 768], // KEY_CNT = 768 from linux/input-event-codes.h

    /// Pointer constraint barrier manager (opaque pointer)
    pub barrier_manager: *mut c_void,
    /// Pointer constraint implementation (opaque pointer)
    pub pointer_constraint: *mut c_void,

    /// Keyboard accessibility features (opaque pointer)
    pub keyboard_a11y: *mut c_void,
    /// Keymap native implementation (opaque pointer)
    pub keymap: *mut c_void,
    /// Input settings manager (opaque pointer)
    pub input_settings: *mut c_void,
    /// Virtual source pointer device (opaque pointer)
    pub virtual_source_pointer: *mut c_void,

    /// Key repeat enabled flag
    pub repeat: bool,
    /// Key repeat delay in milliseconds
    pub repeat_delay: u32,
    /// Key repeat interval in milliseconds
    pub repeat_interval: u32,
    /// Currently repeating key code
    pub repeat_key: u32,
    /// Number of repeat events generated
    pub repeat_count: u32,
    /// Device generating repeat event (opaque pointer)
    pub repeat_device: *mut c_void,
    /// Repeat event source (opaque pointer)
    pub repeat_source: *mut c_void,

    /// Accumulated horizontal scroll (smooth scroll emulation)
    pub accum_scroll_dx: f32,
    /// Accumulated vertical scroll (smooth scroll emulation)
    pub accum_scroll_dy: f32,

    /// Seat released flag
    pub released: bool,
}

impl SeatImpl {
    pub fn new() -> Self {
        SeatImpl {
            main_context: core::ptr::null_mut(),
            input_context: core::ptr::null_mut(),
            input_loop: core::ptr::null_mut(),
            input_thread: core::ptr::null_mut(),
            init_mutex: core::ptr::null_mut(),
            init_cond: core::ptr::null_mut(),
            seat_native: core::ptr::null_mut(),
            seat_id: String::new(),
            flags: 0,
            libinput_source: core::ptr::null_mut(),
            libinput: core::ptr::null_mut(),
            state_lock: core::ptr::null_mut(),
            devices: Vec::new(),
            tools: core::ptr::null_mut(),
            xkb: core::ptr::null_mut(),
            layout_idx: 0,
            button_state: 0,
            button_count: [0; 768],
            barrier_manager: core::ptr::null_mut(),
            pointer_constraint: core::ptr::null_mut(),
            keyboard_a11y: core::ptr::null_mut(),
            keymap: core::ptr::null_mut(),
            input_settings: core::ptr::null_mut(),
            virtual_source_pointer: core::ptr::null_mut(),
            repeat: false,
            repeat_delay: 0,
            repeat_interval: 0,
            repeat_key: 0,
            repeat_count: 0,
            repeat_device: core::ptr::null_mut(),
            repeat_source: core::ptr::null_mut(),
            accum_scroll_dx: 0.0,
            accum_scroll_dy: 0.0,
            released: false,
        }
    }
}

impl Default for SeatImpl {
    fn default() -> Self {
        Self::new()
    }
}
