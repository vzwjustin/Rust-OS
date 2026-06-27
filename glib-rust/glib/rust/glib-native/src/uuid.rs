//! UUID matching `guuid.h` / `guuid.c`.
//!
//! UUID validation and random generation (RFC 4122 v4).
//! Fully `no_std` compatible using `alloc`.

use crate::prelude::*;
use crate::rand::Rand;
use spin::mutex::Mutex;
use spin::once::Once;

/// Global RNG used by `uuid_string_random` so consecutive calls produce
/// different UUIDs. Seeded with a fixed value (no `/dev/urandom` in
/// `no_std`) but advances state between calls — matching upstream
/// behaviour of producing fresh UUIDs per call. Initialized lazily via
/// `spin::Once` because `Rand::with_seed` is not `const`.
static GLOBAL_RNG: Once<Mutex<Rand>> = Once::new();

/// Validate a UUID string (`g_uuid_string_is_valid`).
///
/// Checks format: `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`
/// where each `x` is a hex digit.
pub fn uuid_string_is_valid(str: &str) -> bool {
    if str.len() != 36 {
        return false;
    }
    let bytes = str.as_bytes();
    // Check hyphen positions
    if bytes[8] != b'-' || bytes[13] != b'-' || bytes[18] != b'-' || bytes[23] != b'-' {
        return false;
    }
    // Check all other positions are hex digits
    for (i, &b) in bytes.iter().enumerate() {
        if i == 8 || i == 13 || i == 18 || i == 23 {
            continue;
        }
        if !b.is_ascii_hexdigit() {
            return false;
        }
    }
    true
}

/// Generate a random UUID string (RFC 4122 v4) (`g_uuid_string_random`).
///
/// Uses a global RNG that persists state between calls so consecutive
/// invocations return different UUIDs (matching upstream behaviour).
pub fn uuid_string_random() -> String {
    let mut rng = GLOBAL_RNG
        .call_once(|| Mutex::new(Rand::with_seed(0x6d4f_3a2b)))
        .lock();
    uuid_string_random_with(&mut rng)
}

/// Generate a random UUID string using a provided RNG.
pub fn uuid_string_random_with(rng: &mut Rand) -> String {
    let mut bytes = [0u8; 16];
    for chunk in bytes.chunks_mut(4) {
        let val = rng.int();
        for (i, b) in chunk.iter_mut().enumerate() {
            *b = ((val >> (i * 8)) & 0xff) as u8;
        }
    }

    // Set version (v4) and variant (RFC 4122)
    bytes[6] = (bytes[6] & 0x0f) | 0x40; // version 4
    bytes[8] = (bytes[8] & 0x3f) | 0x80; // variant RFC 4122

    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5],
        bytes[6], bytes[7],
        bytes[8], bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_uuid() {
        assert!(uuid_string_is_valid("550e8400-e29b-41d4-a716-446655440000"));
        assert!(uuid_string_is_valid("00000000-0000-0000-0000-000000000000"));
        assert!(uuid_string_is_valid("FFFFFFFF-FFFF-FFFF-FFFF-FFFFFFFFFFFF"));
    }

    #[test]
    fn invalid_uuid() {
        assert!(!uuid_string_is_valid("not-a-uuid"));
        assert!(!uuid_string_is_valid("550e8400-e29b-41d4-a716-44665544000")); // too short
        assert!(!uuid_string_is_valid(
            "550e8400-e29b-41d4-a716-4466554400000"
        )); // too long
        assert!(!uuid_string_is_valid("550e8400e29b41d4a716446655440000")); // no hyphens
        assert!(!uuid_string_is_valid(
            "gggggggg-gggg-gggg-gggg-gggggggggggg"
        )); // non-hex
    }

    #[test]
    fn random_uuid_valid() {
        let uuid = uuid_string_random();
        assert!(uuid_string_is_valid(&uuid));
    }

    #[test]
    fn random_uuid_unique() {
        let a = uuid_string_random();
        let b = uuid_string_random();
        assert_ne!(a, b);
    }

    #[test]
    fn random_uuid_v4() {
        let uuid = uuid_string_random();
        // Version 4: 13th char (index 14) should be '4'
        let bytes = uuid.as_bytes();
        assert_eq!(bytes[14], b'4');
        // Variant: 17th char (index 19) should be 8, 9, a, or b
        assert!(bytes[19] == b'8' || bytes[19] == b'9' || bytes[19] == b'a' || bytes[19] == b'b');
    }
}
