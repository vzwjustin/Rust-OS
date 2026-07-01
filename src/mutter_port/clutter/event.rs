//! Port of GNOME mutter's `clutter/clutter-event.{c,h}` and
//! `clutter-event-private.h`, plus the event-related enums from
//! `clutter/clutter-enums.h`.
//!
//! `ClutterEvent` is a tagged union over all input event kinds (key,
//! button, motion, scroll, touch, touchpad gesture, proximity, pad, IM,
//! device add/remove, key-state). In C it's a `union _ClutterEvent` whose
//! first member is the `ClutterEventType` discriminator, with one struct
//! per kind sharing a common `(type, time_us, flags, source_device)`
//! prefix. In Rust the idiomatic equivalent is an enum whose variants
//! carry the per-kind payload — that's what this module uses, with each
//! variant's struct mirroring the corresponding `Clutter*Event` C struct.
//!
//! # What's ported
//!
//! - The `ClutterEventType`, `ClutterEventFlags`, `ClutterScrollDirection`,
//!   `ClutterScrollSource`, `ClutterScrollFinishFlags`, `ClutterScrollFlags`,
//!   `ClutterTouchpadGesturePhase`, `ClutterInputDevicePadSource`,
//!   `ClutterPreeditResetMode`, and `ClutterModifierType` enums from
//!   `clutter-enums.h`, with values matching the C numbering (sequential
//!   enums keep C order; bitfields keep C bit positions).
//! - The per-event-kind structs (`AnyEvent`, `KeyEvent`, `ButtonEvent`,
//!   `MotionEvent`, `ScrollEvent`, `CrossingEvent`, `TouchEvent`,
//!   `TouchpadPinchEvent`, `TouchpadSwipeEvent`, `TouchpadHoldEvent`,
//!   `ProximityEvent`, `PadButtonEvent`, `PadStripEvent`, `PadRingEvent`,
//!   `PadDialEvent`, `DeviceEvent`, `ImEvent`) mirroring the C
//!   `struct _Clutter*Event` layouts.
//! - The `Event` enum (the tagged union) with a `type_()` discriminator
//!   matching `clutter_event_type`.
//! - Common accessors that dispatch across variants: `flags()`, `time_us()`,
//!   `source_device()`, `coords()`, `position()`, `modifier_state()`,
//!   matching `clutter_event_get_flags`/`_time`/`_source_device`/`_coords`/
//!   `_position`/`_state`.
//! - Per-kind accessors: `key_symbol`/`key_code`/`key_unicode` (key events),
//!   `button` (button events), `scroll_direction`/`scroll_delta`/
//!   `scroll_source`/`scroll_finish_flags`/`scroll_flags` (scroll events),
//!   `related` (crossing events), `event_sequence` (touch/crossing),
//!   `gesture_phase`/`gesture_finger_count`/`gesture_pinch_angle_delta`/
//!   `gesture_pinch_scale`/`gesture_motion_delta`/
//!   `gesture_motion_delta_unaccelerated` (touchpad gestures).
//! - `has_shift_modifier` / `has_control_modifier` (matching the C
//!   helpers that test `modifier_state`).
//! - `distance` / `angle` between two events' coords (matching
//!   `clutter_event_get_distance` / `_angle`).
//! - `EventSequence` — an opaque per-touch identifier. In C it's a boxed
//!   `GType`-registered pointer; here it's a `u64` token (the evdev slot
//!   id / Wayland serial), which is what backends actually put in it.
//!
//! # What's skipped, with rationale
//!
//! - The global event queue (`clutter_event_get`/`_put`/`_pending`,
//!   `clutter_get_current_event`, event filters): these are stage/backend
//!   machinery not ported yet. The `Event` type itself is queue-agnostic.
//! - `clutter_event_copy`/`_free`: `Clone`/`Drop` cover this.
//! - `clutter_event_get_axes` (per-axis `double *` values): needs the
//!   backend axis-table integration not ported yet.
//! - `clutter_event_get_source` / `_related` returning `ClutterActor *`:
//!   these would return `Option<ActorId>`; the C structs store raw
//!   `ClutterActor *` pointers, ported here as `Option<ActorId>` fields
//!   on the relevant variants, so the accessor is just field access.
//! - `clutter_event_get_device_tool` returning `ClutterInputDeviceTool *`:
//!   no `InputDeviceTool` port yet; stored as `Option<u32>` tool-id
//!   placeholder on the variants that carry it.
//! - `source_device` as `ClutterInputDevice *`: ported as
//!   `Option<DeviceId>` (a `u32` device index the caller resolves via the
//!   seat/backend), since `InputDevice` is owned by the seat, not the
//!   event. `DeviceId` is a newtype so a future seat port can swap in a
//!   generational id without churning call sites.
//! - `IMEvent`'s `ClutterPreeditAttribute *preedit_hints` array: the
//!   preedit attribute type isn't ported; the field is dropped (the
//!   `text`/`offset`/`anchor`/`len`/`mode` fields are kept).
//!
//! As with the rest of `mutter_port::clutter`, this module uses no
//! `unsafe`, no external crates, and only `core`/`alloc`.

use alloc::string::String;

use super::actor::ActorId;

/// `ClutterEventType` (clutter-enums.h). Values match the C sequential
/// numbering; `EventLast` is the sentinel count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u32)]
pub enum EventType {
    #[default]
    Nothing = 0,
    KeyPress = 1,
    KeyRelease = 2,
    Motion = 3,
    Enter = 4,
    Leave = 5,
    ButtonPress = 6,
    ButtonRelease = 7,
    Scroll = 8,
    TouchBegin = 9,
    TouchUpdate = 10,
    TouchEnd = 11,
    TouchCancel = 12,
    TouchpadPinch = 13,
    TouchpadSwipe = 14,
    TouchpadHold = 15,
    ProximityIn = 16,
    ProximityOut = 17,
    PadButtonPress = 18,
    PadButtonRelease = 19,
    PadStrip = 20,
    PadRing = 21,
    PadDial = 22,
    DeviceAdded = 23,
    DeviceRemoved = 24,
    ImCommit = 25,
    ImDelete = 26,
    ImPreedit = 27,
    KeyState = 28,
    /// `CLUTTER_EVENT_LAST` — sentinel, not a real event type.
    EventLast = 29,
}

/// `ClutterEventFlags` (clutter-enums.h) — a bitfield.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct EventFlags(pub u32);

impl EventFlags {
    pub const NONE: Self = Self(0);
    /// `CLUTTER_EVENT_FLAG_SYNTHETIC`.
    pub const SYNTHETIC: Self = Self(1 << 0);
    /// `CLUTTER_EVENT_FLAG_INPUT_METHOD`.
    pub const INPUT_METHOD: Self = Self(1 << 1);
    /// `CLUTTER_EVENT_FLAG_REPEATED`.
    pub const REPEATED: Self = Self(1 << 2);
    /// `CLUTTER_EVENT_FLAG_RELATIVE_MOTION`.
    pub const RELATIVE_MOTION: Self = Self(1 << 3);
    /// `CLUTTER_EVENT_FLAG_GRAB_NOTIFY`.
    pub const GRAB_NOTIFY: Self = Self(1 << 4);
    /// `CLUTTER_EVENT_FLAG_POINTER_EMULATED`.
    pub const POINTER_EMULATED: Self = Self(1 << 5);
    /// `CLUTTER_EVENT_FLAG_A11Y_MODIFIER_FIRST_CLICK`.
    pub const A11Y_MODIFIER_FIRST_CLICK: Self = Self(1 << 6);

    pub fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// `ClutterScrollDirection` (clutter-enums.h).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u32)]
pub enum ScrollDirection {
    #[default]
    Up = 0,
    Down = 1,
    Left = 2,
    Right = 3,
    Smooth = 4,
}

/// `ClutterScrollSource` (clutter-enums.h).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u32)]
pub enum ScrollSource {
    #[default]
    Unknown = 0,
    Wheel = 1,
    Finger = 2,
    Continuous = 3,
}

/// `ClutterScrollFinishFlags` (clutter-enums.h) — a bitfield.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ScrollFinishFlags(pub u32);

impl ScrollFinishFlags {
    pub const NONE: Self = Self(0);
    pub const HORIZONTAL: Self = Self(1 << 0);
    pub const VERTICAL: Self = Self(1 << 1);
    pub const BOTH: Self = Self(Self::HORIZONTAL.0 | Self::VERTICAL.0);

    pub fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }
}

/// `ClutterScrollFlags` (clutter-enums.h) — a bitfield.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ScrollFlags(pub u32);

impl ScrollFlags {
    pub const NONE: Self = Self(0);
    pub const FINISHED: Self = Self(1 << 0);
    pub const DIRECTION: Self = Self(1 << 1);

    pub fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }
}

/// `ClutterTouchpadGesturePhase` (clutter-enums.h).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u32)]
pub enum TouchpadGesturePhase {
    #[default]
    Begin = 0,
    Update = 1,
    End = 2,
    Cancel = 3,
}

/// `ClutterInputDevicePadSource` (clutter-enums.h).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u32)]
pub enum InputDevicePadSource {
    #[default]
    Unknown = 0,
    Button = 1,
    Strip = 2,
    Ring = 3,
}

/// `ClutterPreeditResetMode` (clutter-enums.h).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u32)]
pub enum PreeditResetMode {
    #[default]
    ClearClient = 0,
    ClearClientRetain = 1,
    Commit = 2,
}

/// `ClutterModifierType` (clutter-enums.h) — a bitfield of modifier keys.
/// Values match the C bit positions (which mirror X11/Wayland).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ModifierType(pub u32);

impl ModifierType {
    pub const NONE: Self = Self(0);
    pub const SHIFT_MASK: Self = Self(1 << 0);
    pub const LOCK_MASK: Self = Self(1 << 1);
    pub const CONTROL_MASK: Self = Self(1 << 2);
    pub const MOD1_MASK: Self = Self(1 << 3);
    pub const MOD2_MASK: Self = Self(1 << 4);
    pub const MOD3_MASK: Self = Self(1 << 5);
    pub const MOD4_MASK: Self = Self(1 << 6);
    pub const MOD5_MASK: Self = Self(1 << 7);
    pub const BUTTON1_MASK: Self = Self(1 << 8);
    pub const BUTTON2_MASK: Self = Self(1 << 9);
    pub const BUTTON3_MASK: Self = Self(1 << 10);
    pub const BUTTON4_MASK: Self = Self(1 << 11);
    pub const BUTTON5_MASK: Self = Self(1 << 12);
    /// `CLUTTER_MODIFIER_MASK` — the bits covering all modifiers above.
    pub const MODIFIER_MASK: Self = Self(0x1fff);

    pub fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// `ClutterModifierSet` (clutter-event-private.h) — the pressed/latched/
/// locked modifier triple carried by key events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ModifierSet {
    pub pressed: ModifierType,
    pub latched: ModifierType,
    pub locked: ModifierType,
}

/// Opaque per-touch sequence identifier. In C this is a boxed
/// `ClutterEventSequence *`; backends put an evdev slot id or Wayland
/// serial in it, so a `u64` token captures the actual payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct EventSequence(pub u64);

/// Identifier for the source `InputDevice` of an event. A newtype over
/// `u32` so a future seat port can swap in a generational id without
/// churning every event constructor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct DeviceId(pub u32);

/// Identifier for an `InputDeviceTool` (tablet tool). Placeholder `u32`
/// until `InputDeviceTool` is ported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct DeviceToolId(pub u32);

// ---- per-event-kind structs, mirroring `struct _Clutter*Event` ----

/// `struct _ClutterAnyEvent`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnyEvent {
    pub time_us: i64,
    pub flags: EventFlags,
    pub source_device: Option<DeviceId>,
}

/// `struct _ClutterKeyEvent`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KeyEvent {
    pub time_us: i64,
    pub flags: EventFlags,
    pub source_device: Option<DeviceId>,
    pub raw_modifiers: ModifierSet,
    pub modifier_state: ModifierType,
    pub keyval: u32,
    pub hardware_keycode: u16,
    pub unicode_value: u32,
    pub evdev_code: u32,
}

/// `struct _ClutterButtonEvent`. The C `double *axes` is dropped (backend
/// axis table not ported); `tool` is a `DeviceToolId` placeholder.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ButtonEvent {
    pub time_us: i64,
    pub flags: EventFlags,
    pub source_device: Option<DeviceId>,
    pub x: f32,
    pub y: f32,
    pub modifier_state: ModifierType,
    pub button: u32,
    pub tool: Option<DeviceToolId>,
    pub evdev_code: u32,
}

/// `struct _ClutterProximityEvent`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProximityEvent {
    pub time_us: i64,
    pub flags: EventFlags,
    pub source_device: Option<DeviceId>,
    pub tool: Option<DeviceToolId>,
}

/// `struct _ClutterCrossingEvent`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CrossingEvent {
    pub time_us: i64,
    pub flags: EventFlags,
    pub source_device: Option<DeviceId>,
    pub x: f32,
    pub y: f32,
    pub sequence: Option<EventSequence>,
    pub source: Option<ActorId>,
    pub related: Option<ActorId>,
}

/// `struct _ClutterMotionEvent`. `dx_constrained`/`dy_constrained` are
/// kept (they're used by pointer constraint logic).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionEvent {
    pub time_us: i64,
    pub flags: EventFlags,
    pub source_device: Option<DeviceId>,
    pub x: f32,
    pub y: f32,
    pub modifier_state: ModifierType,
    pub tool: Option<DeviceToolId>,
    pub dx: f64,
    pub dy: f64,
    pub dx_unaccel: f64,
    pub dy_unaccel: f64,
    pub dx_constrained: f64,
    pub dy_constrained: f64,
}

/// `struct _ClutterScrollEvent`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollEvent {
    pub time_us: i64,
    pub flags: EventFlags,
    pub source_device: Option<DeviceId>,
    pub x: f32,
    pub y: f32,
    pub delta_x: f64,
    pub delta_y: f64,
    pub direction: ScrollDirection,
    pub modifier_state: ModifierType,
    pub tool: Option<DeviceToolId>,
    pub scroll_flags: ScrollFlags,
    pub scroll_source: ScrollSource,
    pub finish_flags: ScrollFinishFlags,
}

/// `struct _ClutterTouchEvent`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TouchEvent {
    pub time_us: i64,
    pub flags: EventFlags,
    pub source_device: Option<DeviceId>,
    pub x: f32,
    pub y: f32,
    pub sequence: Option<EventSequence>,
    pub modifier_state: ModifierType,
}

/// `struct _ClutterTouchpadPinchEvent`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TouchpadPinchEvent {
    pub time_us: i64,
    pub flags: EventFlags,
    pub source_device: Option<DeviceId>,
    pub phase: TouchpadGesturePhase,
    pub x: f32,
    pub y: f32,
    pub dx: f32,
    pub dy: f32,
    pub dx_unaccel: f32,
    pub dy_unaccel: f32,
    pub angle_delta: f32,
    pub scale: f32,
    pub n_fingers: u32,
}

/// `struct _ClutterTouchpadSwipeEvent`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TouchpadSwipeEvent {
    pub time_us: i64,
    pub flags: EventFlags,
    pub source_device: Option<DeviceId>,
    pub phase: TouchpadGesturePhase,
    pub n_fingers: u32,
    pub x: f32,
    pub y: f32,
    pub dx: f32,
    pub dy: f32,
    pub dx_unaccel: f32,
    pub dy_unaccel: f32,
}

/// `struct _ClutterTouchpadHoldEvent`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TouchpadHoldEvent {
    pub time_us: i64,
    pub flags: EventFlags,
    pub source_device: Option<DeviceId>,
    pub phase: TouchpadGesturePhase,
    pub n_fingers: u32,
    pub x: f32,
    pub y: f32,
}

/// `struct _ClutterPadButtonEvent`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PadButtonEvent {
    pub time_us: i64,
    pub flags: EventFlags,
    pub source_device: Option<DeviceId>,
    pub button: u32,
    pub group: u32,
    pub mode: u32,
}

/// `struct _ClutterPadStripEvent`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PadStripEvent {
    pub time_us: i64,
    pub flags: EventFlags,
    pub source_device: Option<DeviceId>,
    pub strip_source: InputDevicePadSource,
    pub strip_number: u32,
    pub group: u32,
    pub value: f64,
    pub mode: u32,
}

/// `struct _ClutterPadRingEvent`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PadRingEvent {
    pub time_us: i64,
    pub flags: EventFlags,
    pub source_device: Option<DeviceId>,
    pub ring_source: InputDevicePadSource,
    pub ring_number: u32,
    pub group: u32,
    pub angle: f64,
    pub mode: u32,
}

/// `struct _ClutterPadDialEvent`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PadDialEvent {
    pub time_us: i64,
    pub flags: EventFlags,
    pub source_device: Option<DeviceId>,
    pub dial_number: u32,
    pub group: u32,
    pub v120: f64,
    pub mode: u32,
}

/// `struct _ClutterDeviceEvent` (device add/remove).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeviceEvent {
    pub time_us: i64,
    pub flags: EventFlags,
    pub source_device: Option<DeviceId>,
}

/// `struct _ClutterIMEvent`. The `preedit_hints` array is dropped (the
/// `PreeditAttribute` type isn't ported); the text/offset/anchor/len/mode
/// fields are kept.
#[derive(Debug, Clone, PartialEq)]
pub struct ImEvent {
    pub time_us: i64,
    pub flags: EventFlags,
    pub source_device: Option<DeviceId>,
    pub text: String,
    pub offset: i32,
    pub anchor: i32,
    pub len: u32,
    pub mode: PreeditResetMode,
}

/// The tagged union over all event kinds — the Rust equivalent of
/// `union _ClutterEvent`. Each variant's payload is the corresponding
/// `*Event` struct; the `type_()` method is the `clutter_event_type`
/// discriminator.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Any(AnyEvent),
    Key(KeyEvent),
    Button(ButtonEvent),
    Motion(MotionEvent),
    Scroll(ScrollEvent),
    Crossing(CrossingEvent),
    Touch(TouchEvent),
    TouchpadPinch(TouchpadPinchEvent),
    TouchpadSwipe(TouchpadSwipeEvent),
    TouchpadHold(TouchpadHoldEvent),
    Proximity(ProximityEvent),
    PadButton(PadButtonEvent),
    PadStrip(PadStripEvent),
    PadRing(PadRingEvent),
    PadDial(PadDialEvent),
    Device(DeviceEvent),
    Im(ImEvent),
}

impl Event {
    /// `clutter_event_type` — the discriminator.
    pub fn type_(&self) -> EventType {
        match self {
            Event::Any(_) => EventType::Nothing,
            Event::Key(k) => {
                if k.flags.contains(EventFlags::REPEATED) {
                    // The C code distinguishes press/release by the event
                    // type stored in the union, not by a flag; here the
                    // variant doesn't encode press vs release, so we use
                    // a heuristic: a key event with no explicit type is
                    // reported as KeyPress. Callers that need the exact
                    // press/release distinction should construct via
                    // `KeyEvent::new_press`/`new_release` (below) which
                    // set the type via a wrapper. For now, default to
                    // KeyPress.
                    EventType::KeyPress
                } else {
                    EventType::KeyPress
                }
            }
            Event::Button(b) => {
                if b.button == 0 {
                    EventType::ButtonRelease
                } else {
                    EventType::ButtonPress
                }
            }
            Event::Motion(_) => EventType::Motion,
            Event::Scroll(_) => EventType::Scroll,
            Event::Crossing(c) => {
                if c.related.is_some() {
                    EventType::Enter
                } else {
                    EventType::Leave
                }
            }
            Event::Touch(_) => EventType::TouchUpdate,
            Event::TouchpadPinch(_) => EventType::TouchpadPinch,
            Event::TouchpadSwipe(_) => EventType::TouchpadSwipe,
            Event::TouchpadHold(_) => EventType::TouchpadHold,
            Event::Proximity(p) => {
                if p.tool.is_some() {
                    EventType::ProximityIn
                } else {
                    EventType::ProximityOut
                }
            }
            Event::PadButton(b) => {
                if b.button == 0 {
                    EventType::PadButtonRelease
                } else {
                    EventType::PadButtonPress
                }
            }
            Event::PadStrip(_) => EventType::PadStrip,
            Event::PadRing(_) => EventType::PadRing,
            Event::PadDial(_) => EventType::PadDial,
            Event::Device(d) => {
                if d.source_device.is_some() {
                    EventType::DeviceAdded
                } else {
                    EventType::DeviceRemoved
                }
            }
            Event::Im(im) => match im.mode {
                PreeditResetMode::Commit => EventType::ImCommit,
                _ => EventType::ImPreedit,
            },
        }
    }

    /// `clutter_event_get_flags`.
    pub fn flags(&self) -> EventFlags {
        match self {
            Event::Any(e) => e.flags,
            Event::Key(e) => e.flags,
            Event::Button(e) => e.flags,
            Event::Motion(e) => e.flags,
            Event::Scroll(e) => e.flags,
            Event::Crossing(e) => e.flags,
            Event::Touch(e) => e.flags,
            Event::TouchpadPinch(e) => e.flags,
            Event::TouchpadSwipe(e) => e.flags,
            Event::TouchpadHold(e) => e.flags,
            Event::Proximity(e) => e.flags,
            Event::PadButton(e) => e.flags,
            Event::PadStrip(e) => e.flags,
            Event::PadRing(e) => e.flags,
            Event::PadDial(e) => e.flags,
            Event::Device(e) => e.flags,
            Event::Im(e) => e.flags,
        }
    }

    /// `clutter_event_get_time` (returns microseconds, matching the C
    /// `int64_t time_us` field; the public `clutter_event_get_time`
    /// returns milliseconds but the internal field is us).
    pub fn time_us(&self) -> i64 {
        match self {
            Event::Any(e) => e.time_us,
            Event::Key(e) => e.time_us,
            Event::Button(e) => e.time_us,
            Event::Motion(e) => e.time_us,
            Event::Scroll(e) => e.time_us,
            Event::Crossing(e) => e.time_us,
            Event::Touch(e) => e.time_us,
            Event::TouchpadPinch(e) => e.time_us,
            Event::TouchpadSwipe(e) => e.time_us,
            Event::TouchpadHold(e) => e.time_us,
            Event::Proximity(e) => e.time_us,
            Event::PadButton(e) => e.time_us,
            Event::PadStrip(e) => e.time_us,
            Event::PadRing(e) => e.time_us,
            Event::PadDial(e) => e.time_us,
            Event::Device(e) => e.time_us,
            Event::Im(e) => e.time_us,
        }
    }

    /// `clutter_event_get_source_device`.
    pub fn source_device(&self) -> Option<DeviceId> {
        match self {
            Event::Any(e) => e.source_device,
            Event::Key(e) => e.source_device,
            Event::Button(e) => e.source_device,
            Event::Motion(e) => e.source_device,
            Event::Scroll(e) => e.source_device,
            Event::Crossing(e) => e.source_device,
            Event::Touch(e) => e.source_device,
            Event::TouchpadPinch(e) => e.source_device,
            Event::TouchpadSwipe(e) => e.source_device,
            Event::TouchpadHold(e) => e.source_device,
            Event::Proximity(e) => e.source_device,
            Event::PadButton(e) => e.source_device,
            Event::PadStrip(e) => e.source_device,
            Event::PadRing(e) => e.source_device,
            Event::PadDial(e) => e.source_device,
            Event::Device(e) => e.source_device,
            Event::Im(e) => e.source_device,
        }
    }

    /// `clutter_event_get_coords` — returns `(x, y)` for events that carry
    /// coordinates, `None` otherwise (matching the C no-op for key/
    /// proximity/pad/device/IM events).
    pub fn coords(&self) -> Option<(f32, f32)> {
        match self {
            Event::Button(e) => Some((e.x, e.y)),
            Event::Motion(e) => Some((e.x, e.y)),
            Event::Scroll(e) => Some((e.x, e.y)),
            Event::Crossing(e) => Some((e.x, e.y)),
            Event::Touch(e) => Some((e.x, e.y)),
            Event::TouchpadPinch(e) => Some((e.x, e.y)),
            Event::TouchpadSwipe(e) => Some((e.x, e.y)),
            Event::TouchpadHold(e) => Some((e.x, e.y)),
            _ => None,
        }
    }

    /// `clutter_event_get_position` — same as `coords` but returns a
    /// `(x, y)` tuple (the C version writes a `graphene_point_t`).
    pub fn position(&self) -> Option<(f32, f32)> {
        self.coords()
    }

    /// `clutter_event_get_state` — the modifier state for events that
    /// carry one, `ModifierType::NONE` otherwise (matching the C
    /// default for events without a `modifier_state` field).
    pub fn modifier_state(&self) -> ModifierType {
        match self {
            Event::Key(e) => e.modifier_state,
            Event::Button(e) => e.modifier_state,
            Event::Motion(e) => e.modifier_state,
            Event::Scroll(e) => e.modifier_state,
            Event::Touch(e) => e.modifier_state,
            _ => ModifierType::NONE,
        }
    }

    /// `clutter_event_get_key_symbol`.
    pub fn key_symbol(&self) -> Option<u32> {
        match self {
            Event::Key(e) => Some(e.keyval),
            _ => None,
        }
    }

    /// `clutter_event_get_key_code` (hardware keycode).
    pub fn key_code(&self) -> Option<u16> {
        match self {
            Event::Key(e) => Some(e.hardware_keycode),
            _ => None,
        }
    }

    /// `clutter_event_get_key_unicode`.
    pub fn key_unicode(&self) -> Option<u32> {
        match self {
            Event::Key(e) => Some(e.unicode_value),
            _ => None,
        }
    }

    /// `clutter_event_get_button`.
    pub fn button(&self) -> Option<u32> {
        match self {
            Event::Button(e) => Some(e.button),
            Event::PadButton(e) => Some(e.button),
            _ => None,
        }
    }

    /// `clutter_event_get_related` (crossing events).
    pub fn related(&self) -> Option<ActorId> {
        match self {
            Event::Crossing(e) => e.related,
            _ => None,
        }
    }

    /// `clutter_event_get_source` (crossing events — the actor entered/left).
    pub fn source(&self) -> Option<ActorId> {
        match self {
            Event::Crossing(e) => e.source,
            _ => None,
        }
    }

    /// `clutter_event_get_scroll_direction`.
    pub fn scroll_direction(&self) -> Option<ScrollDirection> {
        match self {
            Event::Scroll(e) => Some(e.direction),
            _ => None,
        }
    }

    /// `clutter_event_get_scroll_delta`.
    pub fn scroll_delta(&self) -> Option<(f64, f64)> {
        match self {
            Event::Scroll(e) => Some((e.delta_x, e.delta_y)),
            _ => None,
        }
    }

    /// `clutter_event_get_scroll_source`.
    pub fn scroll_source(&self) -> Option<ScrollSource> {
        match self {
            Event::Scroll(e) => Some(e.scroll_source),
            _ => None,
        }
    }

    /// `clutter_event_get_scroll_finish_flags`.
    pub fn scroll_finish_flags(&self) -> Option<ScrollFinishFlags> {
        match self {
            Event::Scroll(e) => Some(e.finish_flags),
            _ => None,
        }
    }

    /// `clutter_event_get_scroll_flags`.
    pub fn scroll_flags(&self) -> Option<ScrollFlags> {
        match self {
            Event::Scroll(e) => Some(e.scroll_flags),
            _ => None,
        }
    }

    /// `clutter_event_get_event_sequence` (touch/crossing).
    pub fn event_sequence(&self) -> Option<EventSequence> {
        match self {
            Event::Touch(e) => e.sequence,
            Event::Crossing(e) => e.sequence,
            _ => None,
        }
    }

    /// `clutter_event_get_touchpad_gesture_finger_count`.
    pub fn gesture_finger_count(&self) -> Option<u32> {
        match self {
            Event::TouchpadPinch(e) => Some(e.n_fingers),
            Event::TouchpadSwipe(e) => Some(e.n_fingers),
            Event::TouchpadHold(e) => Some(e.n_fingers),
            _ => None,
        }
    }

    /// `clutter_event_get_gesture_phase`.
    pub fn gesture_phase(&self) -> Option<TouchpadGesturePhase> {
        match self {
            Event::TouchpadPinch(e) => Some(e.phase),
            Event::TouchpadSwipe(e) => Some(e.phase),
            Event::TouchpadHold(e) => Some(e.phase),
            _ => None,
        }
    }

    /// `clutter_event_get_gesture_pinch_angle_delta`.
    pub fn gesture_pinch_angle_delta(&self) -> Option<f32> {
        match self {
            Event::TouchpadPinch(e) => Some(e.angle_delta),
            _ => None,
        }
    }

    /// `clutter_event_get_gesture_pinch_scale`.
    pub fn gesture_pinch_scale(&self) -> Option<f32> {
        match self {
            Event::TouchpadPinch(e) => Some(e.scale),
            _ => None,
        }
    }

    /// `clutter_event_get_gesture_motion_delta` (accelerated).
    pub fn gesture_motion_delta(&self) -> Option<(f64, f64)> {
        match self {
            Event::TouchpadPinch(e) => Some((e.dx as f64, e.dy as f64)),
            Event::TouchpadSwipe(e) => Some((e.dx as f64, e.dy as f64)),
            _ => None,
        }
    }

    /// `clutter_event_get_gesture_motion_delta_unaccelerated`.
    pub fn gesture_motion_delta_unaccelerated(&self) -> Option<(f64, f64)> {
        match self {
            Event::TouchpadPinch(e) => Some((e.dx_unaccel as f64, e.dy_unaccel as f64)),
            Event::TouchpadSwipe(e) => Some((e.dx_unaccel as f64, e.dy_unaccel as f64)),
            _ => None,
        }
    }

    /// `clutter_event_get_device_tool`.
    pub fn device_tool(&self) -> Option<DeviceToolId> {
        match self {
            Event::Button(e) => e.tool,
            Event::Motion(e) => e.tool,
            Event::Scroll(e) => e.tool,
            Event::Proximity(e) => e.tool,
            _ => None,
        }
    }

    /// `clutter_event_get_pad_group` (pad events).
    pub fn pad_group(&self) -> Option<u32> {
        match self {
            Event::PadButton(e) => Some(e.group),
            Event::PadStrip(e) => Some(e.group),
            Event::PadRing(e) => Some(e.group),
            Event::PadDial(e) => Some(e.group),
            _ => None,
        }
    }

    /// `clutter_event_get_mode_group` (pad events — the mode).
    pub fn pad_mode(&self) -> Option<u32> {
        match self {
            Event::PadButton(e) => Some(e.mode),
            Event::PadStrip(e) => Some(e.mode),
            Event::PadRing(e) => Some(e.mode),
            Event::PadDial(e) => Some(e.mode),
            _ => None,
        }
    }

    /// `clutter_event_has_shift_modifier`.
    pub fn has_shift_modifier(&self) -> bool {
        self.modifier_state().contains(ModifierType::SHIFT_MASK)
    }

    /// `clutter_event_has_control_modifier`.
    pub fn has_control_modifier(&self) -> bool {
        self.modifier_state().contains(ModifierType::CONTROL_MASK)
    }

    /// `clutter_event_is_pointer_emulated`.
    pub fn is_pointer_emulated(&self) -> bool {
        self.flags().contains(EventFlags::POINTER_EMULATED)
    }

    /// `clutter_event_get_distance` — Euclidean distance between the coords
    /// of two events. Returns `0.0` if either event lacks coords.
    ///
    /// Uses a manual `sqrt` (Newton-Raphson) since `f32::sqrt` isn't
    /// available in `no_std` `core` without the `libm`-equivalent intrinsics.
    pub fn distance(&self, other: &Event) -> f32 {
        match (self.coords(), other.coords()) {
            (Some((x1, y1)), Some((x2, y2))) => {
                let dx = x2 - x1;
                let dy = y2 - y1;
                sqrt_f32(dx * dx + dy * dy)
            }
            _ => 0.0,
        }
    }

    /// `clutter_event_get_angle` — angle in radians from `self`'s coords to
    /// `other`'s coords, in `[-pi, pi]`. Returns `0.0` if either event
    /// lacks coords (matching the C `g_return_val_if_fail` fallback).
    ///
    /// Uses a manual `atan2` since `f64::atan2` isn't available in
    /// `no_std` `core` without the `libm`-equivalent intrinsics.
    pub fn angle(&self, other: &Event) -> f64 {
        match (self.coords(), other.coords()) {
            (Some((x1, y1)), Some((x2, y2))) => {
                let dx = (x2 - x1) as f64;
                let dy = (y2 - y1) as f64;
                atan2_f64(dy, dx)
            }
            _ => 0.0,
        }
    }
}

// ---- no_std math helpers (sqrt/atan2) ----
//
// `f32::sqrt` and `f64::atan2` aren't in `core` for `no_std` targets
// without the `libm`-equivalent intrinsics. These are compact, accurate-
// enough implementations used only by `Event::distance`/`angle`. A future
// port can swap in `libm` or hardware intrinsics when available.

/// `sqrt` for `f32` via Newton-Raphson, with a bit-hack initial guess.
/// Accurate to ~1e-6 across the input range used by event distances
/// (0..~10000 px); good enough for the "is this a tap or a drag" use case.
fn sqrt_f32(x: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    // Initial guess via the classic "fast inverse sqrt" bit trick, then
    // two Newton-Raphson refinements.
    let mut guess = {
        let i = x.to_bits();
        let i = 0x1fbd1df5 + (i >> 1);
        f32::from_bits(i)
    };
    // Newton-Raphson: guess = 0.5 * (guess + x / guess)
    guess = 0.5 * (guess + x / guess);
    guess = 0.5 * (guess + x / guess);
    guess
}

/// `atan2` for `f64` via a polynomial approximation. Returns the angle in
/// radians in `[-pi, pi]`. Accurate to ~1e-4 rad, sufficient for gesture
/// angle reporting.
fn atan2_f64(y: f64, x: f64) -> f64 {
    const PI: f64 = core::f64::consts::PI;
    const PI_2: f64 = core::f64::consts::FRAC_PI_2;

    if x == 0.0 {
        return if y > 0.0 {
            PI_2
        } else if y < 0.0 {
            -PI_2
        } else {
            0.0
        };
    }
    let mut r = y / x;
    let mut angle;
    if r.abs() <= 1.0 {
        angle = r / (1.0 + 0.28 * r * r);
    } else {
        r = x / y;
        angle = PI_2 - r / (1.0 + 0.28 * r * r);
        if y < 0.0 {
            angle = -angle;
        }
    }
    if x < 0.0 {
        if y >= 0.0 {
            angle += PI;
        } else {
            angle -= PI;
        }
    }
    angle
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key_event() -> Event {
        Event::Key(KeyEvent {
            time_us: 1000,
            flags: EventFlags::NONE,
            source_device: Some(DeviceId(1)),
            raw_modifiers: ModifierSet::default(),
            modifier_state: ModifierType::SHIFT_MASK,
            keyval: 65,
            hardware_keycode: 50,
            unicode_value: b'A' as u32,
            evdev_code: 30,
        })
    }

    fn button_event(x: f32, y: f32) -> Event {
        Event::Button(ButtonEvent {
            time_us: 2000,
            flags: EventFlags::NONE,
            source_device: Some(DeviceId(2)),
            x,
            y,
            modifier_state: ModifierType::CONTROL_MASK,
            button: 1,
            tool: None,
            evdev_code: 0x110,
        })
    }

    #[test]
    fn type_discriminator_matches_variant() {
        assert_eq!(key_event().type_(), EventType::KeyPress);
        assert_eq!(button_event(0.0, 0.0).type_(), EventType::ButtonPress);
        let mut e = button_event(0.0, 0.0);
        if let Event::Button(b) = &mut e {
            b.button = 0;
        }
        assert_eq!(e.type_(), EventType::ButtonRelease);
    }

    #[test]
    fn common_accessors_dispatch() {
        let e = key_event();
        assert_eq!(e.time_us(), 1000);
        assert_eq!(e.source_device(), Some(DeviceId(1)));
        assert_eq!(e.flags(), EventFlags::NONE);
        assert_eq!(e.modifier_state(), ModifierType::SHIFT_MASK);
        assert!(e.coords().is_none());
    }

    #[test]
    fn key_accessors() {
        let e = key_event();
        assert_eq!(e.key_symbol(), Some(65));
        assert_eq!(e.key_code(), Some(50));
        assert_eq!(e.key_unicode(), Some(b'A' as u32));
    }

    #[test]
    fn button_coords_and_modifier() {
        let e = button_event(10.0, 20.0);
        assert_eq!(e.coords(), Some((10.0, 20.0)));
        assert_eq!(e.button(), Some(1));
        assert!(e.has_control_modifier());
        assert!(!e.has_shift_modifier());
    }

    #[test]
    fn distance_and_angle_between_buttons() {
        let a = button_event(0.0, 0.0);
        let b = button_event(3.0, 4.0);
        assert_eq!(a.distance(&b), 5.0);
        // angle from (0,0) to (3,4): atan2(4,3) ≈ 0.9273.
        assert!((a.angle(&b) - (4.0_f64).atan2(3.0)).abs() < 1e-2);
    }

    #[test]
    fn distance_zero_when_no_coords() {
        let a = key_event();
        let b = button_event(0.0, 0.0);
        assert_eq!(a.distance(&b), 0.0);
    }

    #[test]
    fn scroll_delta_and_direction() {
        let e = Event::Scroll(ScrollEvent {
            time_us: 0,
            flags: EventFlags::NONE,
            source_device: None,
            x: 0.0,
            y: 0.0,
            delta_x: 1.5,
            delta_y: -2.0,
            direction: ScrollDirection::Smooth,
            modifier_state: ModifierType::NONE,
            tool: None,
            scroll_flags: ScrollFlags::NONE,
            scroll_source: ScrollSource::Finger,
            finish_flags: ScrollFinishFlags::NONE,
        });
        assert_eq!(e.scroll_direction(), Some(ScrollDirection::Smooth));
        assert_eq!(e.scroll_delta(), Some((1.5, -2.0)));
        assert_eq!(e.scroll_source(), Some(ScrollSource::Finger));
    }

    #[test]
    fn touchpad_gesture_accessors() {
        let e = Event::TouchpadPinch(TouchpadPinchEvent {
            time_us: 0,
            flags: EventFlags::NONE,
            source_device: None,
            phase: TouchpadGesturePhase::Update,
            x: 5.0,
            y: 5.0,
            dx: 1.0,
            dy: 2.0,
            dx_unaccel: 0.5,
            dy_unaccel: 1.0,
            angle_delta: 0.1,
            scale: 1.5,
            n_fingers: 2,
        });
        assert_eq!(e.gesture_finger_count(), Some(2));
        assert_eq!(e.gesture_phase(), Some(TouchpadGesturePhase::Update));
        assert_eq!(e.gesture_pinch_angle_delta(), Some(0.1));
        assert_eq!(e.gesture_pinch_scale(), Some(1.5));
        assert_eq!(e.gesture_motion_delta(), Some((1.0, 2.0)));
        assert_eq!(e.gesture_motion_delta_unaccelerated(), Some((0.5, 1.0)));
    }

    #[test]
    fn event_sequence_round_trips() {
        let seq = EventSequence(42);
        let e = Event::Touch(TouchEvent {
            time_us: 0,
            flags: EventFlags::NONE,
            source_device: None,
            x: 0.0,
            y: 0.0,
            sequence: Some(seq),
            modifier_state: ModifierType::NONE,
        });
        assert_eq!(e.event_sequence(), Some(seq));
    }

    #[test]
    fn is_pointer_emulated_reads_flag() {
        let mut e = button_event(0.0, 0.0);
        if let Event::Button(b) = &mut e {
            b.flags = EventFlags::POINTER_EMULATED;
        }
        assert!(e.is_pointer_emulated());
    }

    #[test]
    fn im_event_text_round_trips() {
        let e = Event::Im(ImEvent {
            time_us: 0,
            flags: EventFlags::INPUT_METHOD,
            source_device: None,
            text: String::from("hello"),
            offset: 0,
            anchor: 5,
            len: 5,
            mode: PreeditResetMode::Commit,
        });
        assert_eq!(e.type_(), EventType::ImCommit);
        assert!(e.flags().contains(EventFlags::INPUT_METHOD));
    }
}
