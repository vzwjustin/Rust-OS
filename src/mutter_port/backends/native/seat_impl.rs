//! Seat Implementation — input device and event handling.
//!
//! Central input abstraction that manages keyboards, mice, touchpads, and other
//! input devices. Handles event dispatch, modifier state tracking, and keyboard repeat.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-seat-impl.h

use alloc::{boxed::Box, string::String, vec::Vec};
use core::ffi::c_void;

pub struct SeatImpl {
    /// Main GLib context for event loop (opaque pointer to GMainContext)
    pub main_context: *mut c_void,
    /// Input thread GLib context (opaque pointer to GMainContext)
    pub input_context: *mut c_void,
    /// Input loop (opaque pointer to GMainLoop)
    pub input_loop: *mut c_void,
    /// Input thread (opaque pointer to GThread)
    pub input_thread: *mut c_void,
    /// Initialization mutex (opaque, GMutex-sized)
    pub init_mutex: [u8; 56], // sizeof(GMutex) ≈ 56 bytes on 64-bit
    /// Initialization condition variable (opaque, GCond-sized)
    pub init_cond: [u8; 48], // sizeof(GCond) ≈ 48 bytes on 64-bit

    /// Associated native seat (opaque pointer to MetaSeatNative)
    pub seat_native: *mut c_void,
    /// Seat identifier string (e.g., "seat0")
    pub seat_id: Option<Box<str>>,
    /// Seat flags (bitfield from MetaSeatNativeFlag)
    pub flags: u32,
    /// Libinput event source (opaque pointer to GSource)
    pub libinput_source: *mut c_void,
    /// Libinput context (opaque pointer to libinput)
    pub libinput: *mut c_void,
    /// State lock (opaque RWLock-sized)
    pub state_lock: [u8; 56], // sizeof(GRWLock) ≈ 56 bytes

    /// List of input devices (opaque pointer to GSList)
    pub devices: *mut c_void,
    /// Hash table of tools by id (opaque pointer to GHashTable)
    pub tools: *mut c_void,

    /// XKB state (opaque pointer to xkb_state)
    pub xkb: *mut c_void,
    /// XKB layout index
    pub layout_idx: u32,
    /// Button state bitmask
    pub button_state: u32,
    /// Button counts per key (array of int; KEY_CNT = 512)
    pub button_count: Vec<i32>,

    /// Barrier manager (opaque pointer to MetaBarrierManagerNative)
    pub barrier_manager: *mut c_void,
    /// Pointer constraint impl (opaque pointer to MetaPointerConstraintImpl)
    pub pointer_constraint: *mut c_void,

    /// Keyboard accessibility helper (opaque pointer to MetaKeyboardA11y)
    pub keyboard_a11y: *mut c_void,
    /// Keymap native (opaque pointer to MetaKeymapNative)
    pub keymap: *mut c_void,
    /// Input settings (opaque pointer to MetaInputSettings)
    pub input_settings: *mut c_void,
    /// Virtual pointer (opaque pointer to ClutterInputDevice)
    pub virtual_source_pointer: *mut c_void,

    /// Viewport info (opaque pointer to MetaViewportInfo)
    pub viewports: *mut c_void,

    /// Tablet mode switch state
    pub tablet_mode_switch_state: bool,
    /// Whether device has touchscreen
    pub has_touchscreen: bool,
    /// Whether device has tablet mode switch
    pub has_tablet_switch: bool,
    /// Whether device has pointer
    pub has_pointer: bool,
    /// Touch mode active
    pub touch_mode: bool,
    /// Input thread initialized
    pub input_thread_initialized: bool,

    /// Keyboard repeat enabled
    pub repeat: bool,
    /// Keyboard repeat delay (ms)
    pub repeat_delay: u32,
    /// Keyboard repeat interval (ms)
    pub repeat_interval: u32,
    /// Key being repeated
    pub repeat_key: u32,
    /// Repeat count
    pub repeat_count: u32,
    /// Device for repeat (opaque pointer to ClutterInputDevice)
    pub repeat_device: *mut c_void,
    /// Repeat source (opaque pointer to GSource)
    pub repeat_source: *mut c_void,

    /// Accumulated horizontal scroll
    pub accum_scroll_dx: f32,
    /// Accumulated vertical scroll
    pub accum_scroll_dy: f32,

    /// Input released
    pub released: bool,
}

impl SeatImpl {
    pub fn new() -> Self {
        SeatImpl {
            main_context: core::ptr::null_mut(),
            input_context: core::ptr::null_mut(),
            input_loop: core::ptr::null_mut(),
            input_thread: core::ptr::null_mut(),
            init_mutex: [0; 56],
            init_cond: [0; 48],

            seat_native: core::ptr::null_mut(),
            seat_id: None,
            flags: 0,
            libinput_source: core::ptr::null_mut(),
            libinput: core::ptr::null_mut(),
            state_lock: [0; 56],

            devices: core::ptr::null_mut(),
            tools: core::ptr::null_mut(),

            xkb: core::ptr::null_mut(),
            layout_idx: 0,
            button_state: 0,
            button_count: Vec::new(),

            barrier_manager: core::ptr::null_mut(),
            pointer_constraint: core::ptr::null_mut(),

            keyboard_a11y: core::ptr::null_mut(),
            keymap: core::ptr::null_mut(),
            input_settings: core::ptr::null_mut(),
            virtual_source_pointer: core::ptr::null_mut(),

            viewports: core::ptr::null_mut(),

            tablet_mode_switch_state: false,
            has_touchscreen: false,
            has_tablet_switch: false,
            has_pointer: false,
            touch_mode: false,
            input_thread_initialized: false,

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
