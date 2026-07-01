//! Stream parser helper (mirrors Linux `net/strparser/`)

pub struct StrParser {
    pub needle: u8,
}

impl StrParser {
    pub fn find_frame(&self, data: &[u8]) -> Option<usize> {
        data.iter().position(|&b| b == self.needle)
    }
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("strparser: TCP stream parser initialized");
    Ok(())
}
