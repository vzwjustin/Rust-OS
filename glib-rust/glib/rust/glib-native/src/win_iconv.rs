//! Win32 iconv compatibility (`win_iconv.c`).

use alloc::string::String;
use alloc::vec::Vec;
use core::char::decode_utf16;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IconvError {
    InvalidUtf8,
    InvalidUtf16,
    UnsupportedEncoding,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Iconv {
    from: String,
    to: String,
}

impl Iconv {
    #[must_use]
    pub fn new(to: &str, from: &str) -> Self {
        Self {
            from: normalize_encoding(from),
            to: normalize_encoding(to),
        }
    }

    pub fn convert(&self, input: &[u8]) -> Result<Vec<u8>, IconvError> {
        match (self.from.as_str(), self.to.as_str()) {
            (from, to) if from == to => Ok(input.to_vec()),
            ("utf8", "utf16le") => {
                let s = core::str::from_utf8(input).map_err(|_| IconvError::InvalidUtf8)?;
                let mut out = Vec::with_capacity(s.len() * 2);
                for unit in s.encode_utf16() {
                    out.extend_from_slice(&unit.to_le_bytes());
                }
                Ok(out)
            }
            ("utf16le", "utf8") => {
                if input.len() % 2 != 0 {
                    return Err(IconvError::InvalidUtf16);
                }
                let mut units = Vec::with_capacity(input.len() / 2);
                for chunk in input.chunks_exact(2) {
                    units.push(u16::from_le_bytes([chunk[0], chunk[1]]));
                }
                let mut out = String::new();
                for ch in decode_utf16(units) {
                    out.push(ch.map_err(|_| IconvError::InvalidUtf16)?);
                }
                Ok(out.into_bytes())
            }
            _ => Err(IconvError::UnsupportedEncoding),
        }
    }
}

fn normalize_encoding(name: &str) -> String {
    name.chars()
        .filter(|&c| c != '-' && c != '_')
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_utf8_and_utf16le() {
        let to_wide = Iconv::new("UTF-16LE", "UTF-8");
        let wide = to_wide.convert("hi".as_bytes()).unwrap();
        assert_eq!(wide, [b'h', 0, b'i', 0]);

        let to_utf8 = Iconv::new("utf-8", "utf_16le");
        assert_eq!(to_utf8.convert(&wide).unwrap(), b"hi");
    }
}
