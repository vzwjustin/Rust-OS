//! Accelerator string parsing — ported from GNOME Mutter's meta-accel-parse.c.
//! Parses keybinding strings like "<Control><Alt>Tab" into modifier + keycode representation.
//! See: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/core/meta-accel-parse.c

use core::fmt;

/// Bitflag for accelerator modifiers (Control, Shift, Alt, Super, etc.)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Modifiers(u32);

impl Modifiers {
    const SHIFT: u32 = 1 << 0;
    const CONTROL: u32 = 1 << 1;
    const ALT: u32 = 1 << 2;
    const SUPER: u32 = 1 << 3;
    const HYPER: u32 = 1 << 4;
    const META: u32 = 1 << 5;
    const MOD1: u32 = 1 << 6;
    const MOD2: u32 = 1 << 7;
    const MOD3: u32 = 1 << 8;
    const MOD4: u32 = 1 << 9;
    const MOD5: u32 = 1 << 10;

    pub fn new() -> Self {
        Modifiers(0)
    }

    pub fn with_shift(mut self) -> Self {
        self.0 |= Self::SHIFT;
        self
    }

    pub fn with_control(mut self) -> Self {
        self.0 |= Self::CONTROL;
        self
    }

    pub fn with_alt(mut self) -> Self {
        self.0 |= Self::ALT;
        self
    }

    pub fn with_super(mut self) -> Self {
        self.0 |= Self::SUPER;
        self
    }

    pub fn has_shift(&self) -> bool {
        self.0 & Self::SHIFT != 0
    }

    pub fn has_control(&self) -> bool {
        self.0 & Self::CONTROL != 0
    }

    pub fn has_alt(&self) -> bool {
        self.0 & Self::ALT != 0
    }

    pub fn has_super(&self) -> bool {
        self.0 & Self::SUPER != 0
    }

    pub fn inner(&self) -> u32 {
        self.0
    }
}

/// Simple keysym representation (xkbcommon-compatible keysym values).
pub type KeyCode = u32;

/// Parse an accelerator string into modifiers and keycode.
///
/// Accepts strings like:
/// - `"<Control><Alt>Tab"` → (Modifiers with Control+Alt, keysym for Tab)
/// - `"<Shift>a"` → (Modifiers with Shift, keysym for 'a')
/// - `"<Super>q"` → (Modifiers with Super, keysym for 'q')
/// - `"0xff09"` → (no modifiers, keycode 0xff09 as hex)
///
/// Returns None if parsing fails (unknown key name).
pub fn parse_accelerator(s: &str) -> Option<(Modifiers, KeyCode)> {
    let bytes = s.as_bytes();
    let mut mods = Modifiers::new();
    let mut pos = 0;

    // Parse all leading modifier tokens.
    loop {
        if pos >= bytes.len() {
            return None;
        }

        if bytes[pos] != b'<' {
            break;
        }

        let consumed = match_modifier(&bytes[pos..], &mut mods);
        if consumed == 0 {
            break;
        }
        pos += consumed;
    }

    let remaining = &s[pos..];

    // Parse remaining as key name or hex keycode.
    if remaining.starts_with("0x") && remaining.len() == 6 {
        if let Ok(keycode) = u32::from_str_radix(&remaining[2..], 16) {
            return Some((mods, keycode));
        }
    }

    keysym_from_name(remaining).map(|ks| (mods, ks))
}

/// Format a modifiers + keycode pair back into an accelerator string.
pub fn format_accelerator(mods: Modifiers, keycode: KeyCode) -> alloc::string::String {
    use alloc::string::String;

    let mut s = String::new();

    if mods.has_control() {
        s.push_str("<Control>");
    }
    if mods.has_shift() {
        s.push_str("<Shift>");
    }
    if mods.has_alt() {
        s.push_str("<Alt>");
    }
    if mods.has_super() {
        s.push_str("<Super>");
    }

    s.push_str(&keyname_from_keysym(keycode));
    s
}

/// Try to match and consume a modifier token at the start of bytes.
/// Returns bytes consumed (0 if no match), and sets the corresponding bit in mods.
fn match_modifier(bytes: &[u8], mods: &mut Modifiers) -> usize {
    if bytes.len() < 3 {
        return 0;
    }

    let b1 = bytes[0];
    if b1 != b'<' {
        return 0;
    }

    // Case-insensitive match for each modifier tag.
    // Ordered by length (longest first) to avoid prefix collisions.

    // <Control> (9 bytes)
    if bytes.len() >= 9 && match_bytes_ci(&bytes[..9], b"<Control>") {
        mods.0 |= Modifiers::CONTROL;
        return 9;
    }

    // <Primary> (9 bytes) — treated as Control
    if bytes.len() >= 9 && match_bytes_ci(&bytes[..9], b"<Primary>") {
        mods.0 |= Modifiers::CONTROL;
        return 9;
    }

    // <Shift> (7 bytes)
    if bytes.len() >= 7 && match_bytes_ci(&bytes[..7], b"<Shift>") {
        mods.0 |= Modifiers::SHIFT;
        return 7;
    }

    // <Super> (7 bytes)
    if bytes.len() >= 7 && match_bytes_ci(&bytes[..7], b"<Super>") {
        mods.0 |= Modifiers::SUPER;
        return 7;
    }

    // <Hyper> (7 bytes)
    if bytes.len() >= 7 && match_bytes_ci(&bytes[..7], b"<Hyper>") {
        mods.0 |= Modifiers::HYPER;
        return 7;
    }

    // <Meta> (6 bytes)
    if bytes.len() >= 6 && match_bytes_ci(&bytes[..6], b"<Meta>") {
        mods.0 |= Modifiers::META;
        return 6;
    }

    // <Alt> (5 bytes)
    if bytes.len() >= 5 && match_bytes_ci(&bytes[..5], b"<Alt>") {
        mods.0 |= Modifiers::ALT;
        return 5;
    }

    // <Ctl> (5 bytes)
    if bytes.len() >= 5 && match_bytes_ci(&bytes[..5], b"<Ctl>") {
        mods.0 |= Modifiers::CONTROL;
        return 5;
    }

    // <Mod1> through <Mod5> (6 bytes each)
    if bytes.len() >= 6
        && bytes[1..4] == *b"Mod"
        && bytes[4] >= b'1'
        && bytes[4] <= b'5'
        && bytes[5] == b'>'
    {
        let bits = match bytes[4] {
            b'1' => Modifiers::MOD1,
            b'2' => Modifiers::MOD2,
            b'3' => Modifiers::MOD3,
            b'4' => Modifiers::MOD4,
            b'5' => Modifiers::MOD5,
            _ => return 0,
        };
        mods.0 |= bits;
        return 6;
    }

    0
}

/// Case-insensitive byte sequence match.
fn match_bytes_ci(bytes: &[u8], pattern: &[u8]) -> bool {
    if bytes.len() != pattern.len() {
        return false;
    }
    bytes
        .iter()
        .zip(pattern.iter())
        .all(|(a, b)| a.eq_ignore_ascii_case(b))
}

/// Map common key names to xkbcommon keysym values.
fn keysym_from_name(name: &str) -> Option<KeyCode> {
    Some(match name {
        "Tab" | "tab" => 0xff09,
        "Return" | "Enter" | "return" => 0xff0d,
        "Escape" | "Esc" | "escape" => 0xff1b,
        "BackSpace" | "BackSpace" | "backspace" => 0xff08,
        "space" | " " => 0x20,
        "Delete" | "delete" => 0xffff,
        "Home" | "home" => 0xff50,
        "End" | "end" => 0xff57,
        "Page_Up" | "Page_Up" | "Prior" => 0xff55,
        "Page_Down" | "Page_Down" | "Next" => 0xff56,
        "Left" | "left" => 0xff51,
        "Right" | "right" => 0xff53,
        "Up" | "up" => 0xff52,
        "Down" | "down" => 0xff54,
        "F1" | "f1" => 0xffbe,
        "F2" | "f2" => 0xffbf,
        "F3" | "f3" => 0xffc0,
        "F4" | "f4" => 0xffc1,
        "F5" | "f5" => 0xffc2,
        "F6" | "f6" => 0xffc3,
        "F7" | "f7" => 0xffc4,
        "F8" | "f8" => 0xffc5,
        "F9" | "f9" => 0xffc6,
        "F10" | "f10" => 0xffc7,
        "F11" | "f11" => 0xffc8,
        "F12" | "f12" => 0xffc9,
        "Above_Tab" => 0xff12, // Special Mutter key
        // Single ASCII characters
        c if c.len() == 1 => {
            let b = c.as_bytes()[0];
            if (b'a'..=b'z').contains(&b)
                || (b'A'..=b'Z').contains(&b)
                || (b'0'..=b'9').contains(&b)
            {
                b as u32
            } else {
                return None;
            }
        }
        _ => return None,
    })
}

/// Map xkbcommon keysym back to a readable key name.
fn keyname_from_keysym(sym: KeyCode) -> &'static str {
    match sym {
        0xff09 => "Tab",
        0xff0d => "Return",
        0xff1b => "Escape",
        0xff08 => "BackSpace",
        0x20 => "space",
        0xffff => "Delete",
        0xff50 => "Home",
        0xff57 => "End",
        0xff55 => "Page_Up",
        0xff56 => "Page_Down",
        0xff51 => "Left",
        0xff53 => "Right",
        0xff52 => "Up",
        0xff54 => "Down",
        0xffbe => "F1",
        0xffbf => "F2",
        0xffc0 => "F3",
        0xffc1 => "F4",
        0xffc2 => "F5",
        0xffc3 => "F6",
        0xffc4 => "F7",
        0xffc5 => "F8",
        0xffc6 => "F9",
        0xffc7 => "F10",
        0xffc8 => "F11",
        0xffc9 => "F12",
        0xff12 => "Above_Tab",
        // ASCII characters
        c if (c as u8 as char).is_ascii() => {
            // HACK: can't return owned string in static context, use debug display
            "<non-ASCII keysym>"
        }
        _ => "<unknown>",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_key() {
        let (mods, key) = parse_accelerator("Tab").expect("failed to parse Tab");
        assert_eq!(mods, Modifiers::new());
        assert_eq!(key, 0xff09);
    }

    #[test]
    fn test_parse_with_control() {
        let (mods, key) = parse_accelerator("<Control>a").expect("failed to parse <Control>a");
        assert!(mods.has_control());
        assert_eq!(key, b'a' as u32);
    }

    #[test]
    fn test_parse_multiple_mods() {
        let (mods, key) = parse_accelerator("<Control><Shift>Tab").expect("failed");
        assert!(mods.has_control());
        assert!(mods.has_shift());
        assert_eq!(key, 0xff09);
    }

    #[test]
    fn test_parse_case_insensitive() {
        let (mods1, _) = parse_accelerator("<control>a").expect("lowercase");
        let (mods2, _) = parse_accelerator("<Control>a").expect("titlecase");
        assert_eq!(mods1, mods2);
    }

    #[test]
    fn test_parse_hex_keycode() {
        let (mods, key) = parse_accelerator("0xff09").expect("hex");
        assert_eq!(mods, Modifiers::new());
        assert_eq!(key, 0xff09);
    }

    #[test]
    fn test_format_simple() {
        let s = format_accelerator(Modifiers::new(), 0xff09);
        assert_eq!(s, "Tab");
    }

    #[test]
    fn test_format_with_mods() {
        let mods = Modifiers::new().with_control();
        let s = format_accelerator(mods, b'a' as u32);
        assert_eq!(s, "<Control>a");
    }
}
