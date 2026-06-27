//! GSocketAddressEnumerator matching `gio/gsocketaddressenumerator.h`.
//!
//! The canonical implementation lives in
//! [`crate::gsocketconnectable::SocketAddressEnumerator`], which is also used by
//! [`crate::gsocketconnectable::SocketConnectable`]. This module re-exports that
//! type for the dedicated header import path.

pub use crate::gsocketconnectable::SocketAddressEnumerator;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ginetaddress::{InetAddress, SocketFamily};
    use crate::ginetsocketaddress::InetSocketAddress;

    #[test]
    fn reexport_next_path() {
        let addr = InetAddress::new_from_bytes(&[127, 0, 0, 1], SocketFamily::Ipv4).unwrap();
        let enumerator = SocketAddressEnumerator::new(vec![InetSocketAddress::new(addr, 8080)]);

        assert_eq!(enumerator.next(None).unwrap().unwrap().port(), 8080);
        assert!(enumerator.next(None).unwrap().is_none());
    }
}
