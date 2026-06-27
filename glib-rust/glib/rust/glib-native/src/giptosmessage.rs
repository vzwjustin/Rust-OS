//! GIPtosMessage matching `gio/giptosmessage.h`.
//! IP TOS (Type of Service) socket control message. In this no_std port
//! we model it with a TOS value.
//! Fully `no_std` compatible.

/// An IP TOS socket control message (`GIPTosMessage`).
pub struct IPTosMessage {
    tos: u8,
}

impl IPTosMessage {
    pub fn new(tos: u8) -> Self {
        Self { tos }
    }
    pub fn get_tos(&self) -> u8 {
        self.tos
    }
    pub fn set_tos(&mut self, tos: u8) {
        self.tos = tos;
    }
    pub fn get_level(&self) -> i32 {
        0
    } // IPPROTO_IP
    pub fn get_type(&self) -> i32 {
        1
    } // IP_TOS
    pub fn get_size(&self) -> usize {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let m = IPTosMessage::new(0x10);
        assert_eq!(m.get_tos(), 0x10);
        assert_eq!(m.get_size(), 1);
    }
}
