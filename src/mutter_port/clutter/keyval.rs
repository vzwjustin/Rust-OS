//! Port of GNOME mutter's `clutter/clutter-keyval.{c,h}`.
//!
//! Keyval conversion and lookup. Handles case conversion for various
//! character sets (Latin 1-4, Cyrillic, Greek) and key name lookup.
//! External lookup tables (`clutter_keys_by_keyval`, `keynames`) are
//! not ported; this module provides the core algorithms.

/// Convert a key symbol to its lowercase and uppercase variants.
///
/// Handles 24-bit UCS characters (encoded as 0x01xxxxxx), Latin 1-4,
/// Cyrillic, and Greek character sets. Returns `(lower, upper)`; for
/// unmapped symbols, returns the input symbol for both.
pub fn keyval_convert_case(symbol: u32) -> (u32, u32) {
    let mut xlower = symbol;
    let mut xupper = symbol;

    if (symbol & 0xff000000) == 0x01000000 {
        return (xlower, xupper);
    }

    match symbol >> 8 {
        0 => {
            if (symbol >= 0x41) && (symbol <= 0x5a) {
                xlower = symbol + 0x20;
            } else if (symbol >= 0x61) && (symbol <= 0x7a) {
                xupper = symbol - 0x20;
            } else if (symbol >= 0xc0) && (symbol <= 0xd6) {
                xlower += 0x20;
            } else if (symbol >= 0xd8) && (symbol <= 0xde) {
                xlower += 0x20;
            } else if (symbol >= 0xdf) && (symbol <= 0xf6) {
                xupper -= 0x20;
            } else if (symbol >= 0xf8) && (symbol <= 0xfe) {
                xupper -= 0x20;
            }
        }
        1 => {
            if symbol == 0x0104 {
                xlower = 0x0105;
            } else if (symbol >= 0x0139) && (symbol <= 0x0146) {
                xlower += 0x01;
            } else if (symbol >= 0x0147) && (symbol <= 0x0148) {
                xlower += 0x01;
            } else if (symbol >= 0x0150) && (symbol <= 0x0176) {
                xlower += 0x01;
            } else if (symbol >= 0x0177) && (symbol <= 0x0178) {
                xupper -= 0x01;
            } else if (symbol >= 0x0179) && (symbol <= 0x017c) {
                xlower += 0x01;
            } else if (symbol >= 0x017d) && (symbol <= 0x0182) {
                xupper -= 0x01;
            } else if symbol == 0x0105 {
                xupper = 0x0104;
            } else if (symbol >= 0x0140) && (symbol <= 0x0148) {
                xupper -= 0x01;
            } else if (symbol >= 0x0149) && (symbol <= 0x0176) {
                xupper -= 0x01;
            } else if (symbol >= 0x0178) && (symbol <= 0x0179) {
                xlower += 0x01;
            } else if (symbol >= 0x017a) && (symbol <= 0x017c) {
                xupper -= 0x01;
            } else if (symbol >= 0x017e) && (symbol <= 0x0182) {
                xlower += 0x01;
            }
        }
        2 => {
            if (symbol >= 0x0243) && (symbol <= 0x0246) {
                xlower += 0x01;
            } else if (symbol >= 0x0247) && (symbol <= 0x0250) {
                xlower += 0x01;
            } else if (symbol >= 0x0251) && (symbol <= 0x0254) {
                xupper -= 0x01;
            } else if (symbol >= 0x0255) && (symbol <= 0x0260) {
                xupper -= 0x01;
            } else if (symbol >= 0x0261) && (symbol <= 0x0266) {
                xlower += 0x01;
            } else if (symbol >= 0x0267) && (symbol <= 0x0270) {
                xupper -= 0x01;
            }
        }
        3 => {
            if (symbol >= 0x0343) && (symbol <= 0x0374) {
                xlower += 0x01;
            } else if (symbol >= 0x0375) && (symbol <= 0x0376) {
                xupper -= 0x01;
            } else if (symbol >= 0x0377) && (symbol <= 0x0398) {
                xlower += 0x01;
            } else if (symbol >= 0x0399) && (symbol <= 0x03a1) {
                xupper -= 0x01;
            } else if (symbol >= 0x03a2) && (symbol <= 0x03ab) {
                xlower += 0x01;
            } else if symbol == 0x03ac {
                xupper = 0x0386;
            } else if (symbol >= 0x03ad) && (symbol <= 0x03af) {
                xupper -= 0x01;
            } else if (symbol >= 0x03b0) && (symbol <= 0x03ce) {
                xupper -= 0x20;
            }
        }
        6 => {
            if (symbol >= 0x0643) && (symbol <= 0x0644) {
                xlower -= 0x01;
            } else if (symbol >= 0x0645) && (symbol <= 0x0648) {
                xupper += 0x01;
            } else if (symbol >= 0x0649) && (symbol <= 0x0680) {
                xlower -= 0x02;
            } else if (symbol >= 0x0681) && (symbol <= 0x0682) {
                xupper += 0x02;
            }
        }
        7 => {
            if (symbol >= 0x0745) && (symbol <= 0x0748) {
                xlower += 0x01;
            } else if (symbol >= 0x0749) && (symbol <= 0x074a) {
                xupper -= 0x01;
            } else if (symbol >= 0x074b) && (symbol <= 0x076a) {
                xlower += 0x01;
            } else if (symbol >= 0x076b) && (symbol <= 0x076c) {
                xupper -= 0x01;
            } else if (symbol >= 0x076d) && (symbol <= 0x0770) {
                xlower += 0x01;
            } else if (symbol >= 0x0771) && (symbol <= 0x0774) {
                xupper -= 0x01;
            }
        }
        _ => {}
    }

    (xlower, xupper)
}

/// Returns the symbolic name of a key symbol.
///
/// For directly encoded 24-bit UCS characters (0x01xxxxxx), would
/// return "U+XXXX" format. For known keysyms, would return their
/// symbolic name. External lookup tables required for this function
/// are not ported.
pub fn keyval_name(_keyval: u32) -> Option<&'static str> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latin1_lowercase() {
        let (lower, upper) = keyval_convert_case(0x41);
        assert_eq!(lower, 0x61);
        assert_eq!(upper, 0x41);
    }

    #[test]
    fn latin1_uppercase() {
        let (lower, upper) = keyval_convert_case(0x61);
        assert_eq!(lower, 0x61);
        assert_eq!(upper, 0x41);
    }

    #[test]
    fn ucs_pass_through() {
        let sym = 0x01000041;
        let (lower, upper) = keyval_convert_case(sym);
        assert_eq!(lower, sym);
        assert_eq!(upper, sym);
    }

    #[test]
    fn keyval_name_unimplemented() {
        assert_eq!(keyval_name(0x41), None);
    }
}
