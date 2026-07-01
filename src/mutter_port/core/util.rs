//! Mutter utility functions — ported from mutter-main/src/core/util.c
//! Skipped: environment variables, file I/O, process spawning, GLib/GObject async, locale encoding.
//! Pure logic: debug topic/paint flag bitmasks, gravity/hash utilities, random ID generation, hex encoding.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt::Write as FmtWrite;

/// Debug topic bitmask for verbose logging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DebugTopic(u32);

impl DebugTopic {
    pub const FOCUS: Self = Self(1 << 0);
    pub const WORKAREA: Self = Self(1 << 1);
    pub const STACK: Self = Self(1 << 2);
    pub const EVENTS: Self = Self(1 << 3);
    pub const WINDOW_STATE: Self = Self(1 << 4);
    pub const WINDOW_OPS: Self = Self(1 << 5);
    pub const GEOMETRY: Self = Self(1 << 6);
    pub const PLACEMENT: Self = Self(1 << 7);
    pub const DISPLAY: Self = Self(1 << 8);
    pub const KEYBINDINGS: Self = Self(1 << 9);
    pub const SYNC: Self = Self(1 << 10);
    pub const STARTUP: Self = Self(1 << 11);
    pub const PREFS: Self = Self(1 << 12);
    pub const EDGE_RESISTANCE: Self = Self(1 << 13);
    pub const DBUS: Self = Self(1 << 14);
    pub const INPUT: Self = Self(1 << 15);
    pub const WAYLAND: Self = Self(1 << 16);
    pub const KMS: Self = Self(1 << 17);
    pub const SCREEN_CAST: Self = Self(1 << 18);
    pub const REMOTE_DESKTOP: Self = Self(1 << 19);
    pub const BACKEND: Self = Self(1 << 20);
    pub const RENDER: Self = Self(1 << 21);
    pub const COLOR: Self = Self(1 << 22);
    pub const INPUT_EVENTS: Self = Self(1 << 23);
    pub const EIS: Self = Self(1 << 24);
    pub const KMS_DEADLINE: Self = Self(1 << 25);
    pub const SESSION_MANAGEMENT: Self = Self(1 << 26);
    pub const X11: Self = Self(1 << 27);
    pub const WORKSPACES: Self = Self(1 << 28);
    pub const VERBOSE: Self = Self(1 << 29);

    pub fn to_string(&self) -> &'static str {
        match self.0 {
            b if b == Self::FOCUS.0 => "FOCUS",
            b if b == Self::WORKAREA.0 => "WORKAREA",
            b if b == Self::STACK.0 => "STACK",
            b if b == Self::EVENTS.0 => "EVENTS",
            b if b == Self::WINDOW_STATE.0 => "WINDOW_STATE",
            b if b == Self::WINDOW_OPS.0 => "WINDOW_OPS",
            b if b == Self::GEOMETRY.0 => "GEOMETRY",
            b if b == Self::PLACEMENT.0 => "PLACEMENT",
            b if b == Self::DISPLAY.0 => "DISPLAY",
            b if b == Self::KEYBINDINGS.0 => "KEYBINDINGS",
            b if b == Self::SYNC.0 => "SYNC",
            b if b == Self::STARTUP.0 => "STARTUP",
            b if b == Self::PREFS.0 => "PREFS",
            b if b == Self::EDGE_RESISTANCE.0 => "EDGE_RESISTANCE",
            b if b == Self::DBUS.0 => "DBUS",
            b if b == Self::INPUT.0 => "INPUT",
            b if b == Self::WAYLAND.0 => "WAYLAND",
            b if b == Self::KMS.0 => "KMS",
            b if b == Self::SCREEN_CAST.0 => "SCREEN_CAST",
            b if b == Self::REMOTE_DESKTOP.0 => "REMOTE_DESKTOP",
            b if b == Self::BACKEND.0 => "BACKEND",
            b if b == Self::RENDER.0 => "RENDER",
            b if b == Self::COLOR.0 => "COLOR",
            b if b == Self::INPUT_EVENTS.0 => "INPUT_EVENTS",
            b if b == Self::EIS.0 => "EIS",
            b if b == Self::KMS_DEADLINE.0 => "KMS_DEADLINE",
            b if b == Self::SESSION_MANAGEMENT.0 => "SESSION_MANAGEMENT",
            b if b == Self::X11.0 => "X11",
            b if b == Self::WORKSPACES.0 => "WORKSPACES",
            b if b == Self::VERBOSE.0 => "VERBOSE",
            _ => "WM",
        }
    }
}

/// Paint debug flags bitmask.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DebugPaintFlag(u32);

impl DebugPaintFlag {
    pub const OPAQUE_REGION: Self = Self(1 << 0);
    pub const SYNC_CURSOR_PRIMARY: Self = Self(1 << 1);
    pub const DISABLE_DIRECT_SCANOUT: Self = Self(1 << 2);
    pub const IGNORE_COLOR_STATE_FOR_DIRECT_SCANOUT: Self = Self(1 << 3);

    pub fn has(&self, flag: DebugPaintFlag) -> bool {
        self.0 & flag.0 != 0
    }

    pub fn add(&mut self, flag: DebugPaintFlag) {
        self.0 |= flag.0;
    }

    pub fn remove(&mut self, flag: DebugPaintFlag) {
        self.0 &= !flag.0;
    }
}

/// Window gravity enum for alignment calculations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gravity {
    NorthWest,
    North,
    NorthEast,
    West,
    Center,
    East,
    SouthWest,
    South,
    SouthEast,
    Static,
}

impl Gravity {
    pub fn to_string(&self) -> &'static str {
        match self {
            Gravity::NorthWest => "META_GRAVITY_NORTH_WEST",
            Gravity::North => "META_GRAVITY_NORTH",
            Gravity::NorthEast => "META_GRAVITY_NORTH_EAST",
            Gravity::West => "META_GRAVITY_WEST",
            Gravity::Center => "META_GRAVITY_CENTER",
            Gravity::East => "META_GRAVITY_EAST",
            Gravity::SouthWest => "META_GRAVITY_SOUTH_WEST",
            Gravity::South => "META_GRAVITY_SOUTH",
            Gravity::SouthEast => "META_GRAVITY_SOUTH_EAST",
            Gravity::Static => "META_GRAVITY_STATIC",
        }
    }
}

/// Hash function for unsigned long (u64).
pub fn unsigned_long_hash(v: u64) -> u32 {
    #[cfg(target_pointer_width = "64")]
    {
        (v ^ (v >> 32)) as u32
    }
    #[cfg(target_pointer_width = "32")]
    {
        v as u32
    }
}

/// Generate a random printable ASCII ID of given length.
pub fn generate_random_id(len: usize, seed: u64) -> String {
    let mut id = String::with_capacity(len);
    let mut rng = seed;
    for _ in 0..len {
        rng = rng.wrapping_mul(1103515245).wrapping_add(12345);
        let byte = ((rng / 65536) % 95) as u8 + 32;
        id.push(byte as char);
    }
    id
}

/// Encode binary data as hex string.
pub fn encode_hex(data: &[u8]) -> String {
    let mut hex = String::with_capacity(data.len() * 2);
    for byte in data {
        let _ = write!(hex, "{:02x}", byte);
    }
    hex
}

/// Format external binding name for keybinding action.
pub fn external_binding_name_for_action(action: u32) -> String {
    alloc::format!("external-grab-{}", action)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_debug_topic_flags() {
        let mut topics = DebugTopic(0);
        assert_eq!(topics.0, 0);
    }

    #[test]
    fn test_gravity_to_string() {
        assert_eq!(Gravity::NorthWest.to_string(), "META_GRAVITY_NORTH_WEST");
        assert_eq!(Gravity::Center.to_string(), "META_GRAVITY_CENTER");
    }

    #[test]
    fn test_unsigned_long_hash() {
        let hash1 = unsigned_long_hash(0x123456789ABCDEF0);
        let hash2 = unsigned_long_hash(0x123456789ABCDEF0);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_encode_hex() {
        assert_eq!(encode_hex(&[0xAB, 0xCD, 0xEF]), "abcdef");
        assert_eq!(encode_hex(&[0x00, 0xFF]), "00ff");
    }

    #[test]
    fn test_external_binding_name() {
        assert_eq!(external_binding_name_for_action(42), "external-grab-42");
    }
}
