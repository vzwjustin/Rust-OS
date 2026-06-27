//! GIPv6TClassMessage matching `gio/gipv6tclassmessage.h`.
//! IPv6 Traffic Class socket control message. In this no_std port
//! we model it with a traffic class value.
//! Fully `no_std` compatible.

/// An IPv6 Traffic Class socket control message (`GIPv6TClassMessage`).
pub struct IPv6TClassMessage {
    tclass: u8,
}

impl IPv6TClassMessage {
    pub fn new(tclass: u8) -> Self {
        Self { tclass }
    }
    pub fn get_tclass(&self) -> u8 {
        self.tclass
    }
    pub fn set_tclass(&mut self, tclass: u8) {
        self.tclass = tclass;
    }
    pub fn get_level(&self) -> i32 {
        41
    } // IPPROTO_IPV6
    pub fn get_type(&self) -> i32 {
        67
    } // IPV6_TCLASS
    pub fn get_size(&self) -> usize {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let m = IPv6TClassMessage::new(0x20);
        assert_eq!(m.get_tclass(), 0x20);
        assert_eq!(m.get_level(), 41);
        assert_eq!(m.get_type(), 67);
    }
}
