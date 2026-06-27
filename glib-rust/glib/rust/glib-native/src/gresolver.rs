//! GIO resolver matching `gio/gresolver.h` / `gio/gresolver.c`.
//!
//! Upstream `GResolver` is an abstract `GObject` subclass that performs
//! hostname-to-address and address-to-hostname resolution. We port it as a
//! plain `Resolver` trait plus a `NoopResolver` concrete implementation that
//! always returns `ResolverError::NotFound`, mirroring the behaviour of a
//! resolver with no backend wired up.
//!
//! Provides:
//! - `ResolverError` enum with `to_code` and `resolver_error_quark`.
//! - `Resolver` trait: `lookup_by_name`, `lookup_by_address`, `lookup_service`.
//! - `NoopResolver` struct implementing `Resolver`.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::error::Error;
use crate::gcancellable::GCancellable;
use crate::ginetaddress::InetAddress;
use crate::prelude::*;
use alloc::string::String;
use alloc::vec::Vec;

// ──────────────────────────── ResolverError ───────────────────────────────

/// Error codes for resolver operations (`GResolverError`).
///
/// Mirrors the upstream enum values from `gio/gresolver.h`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResolverError {
    /// The requested name/address/service was not found.
    NotFound = 0,
    /// A temporary error occurred; the lookup may succeed later.
    Temporary = 1,
    /// An internal resolver error occurred.
    Internal = 2,
}

impl ResolverError {
    /// Returns the numeric error code for this variant.
    ///
    /// Mirrors `GResolverError` enum integer values.
    pub fn to_code(&self) -> i32 {
        *self as i32
    }

    /// Returns the quark for the resolver error domain.
    ///
    /// Mirrors `g_resolver_error_quark`. Hard-coded to 9 in this port.
    pub fn resolver_error_quark() -> u32 {
        9
    }
}

impl core::fmt::Display for ResolverError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ResolverError::NotFound => f.write_str("Resolver: name/address not found"),
            ResolverError::Temporary => f.write_str("Resolver: temporary failure"),
            ResolverError::Internal => f.write_str("Resolver: internal error"),
        }
    }
}

// ──────────────────────────── Resolver trait ──────────────────────────────

/// Trait representing a DNS resolver (`GResolver`).
///
/// Implementors provide synchronous name/address/service lookup. The
/// `cancellable` argument is accepted but whether it is honoured is
/// implementation-defined (the `NoopResolver` ignores it).
pub trait Resolver {
    /// Looks up all addresses for `hostname`.
    ///
    /// Mirrors `g_resolver_lookup_by_name`.
    fn lookup_by_name(
        &self,
        hostname: &str,
        cancellable: Option<&GCancellable>,
    ) -> Result<Vec<InetAddress>, Error>;

    /// Performs a reverse lookup for `address`, returning the hostname.
    ///
    /// Mirrors `g_resolver_lookup_by_address`.
    fn lookup_by_address(
        &self,
        address: &InetAddress,
        cancellable: Option<&GCancellable>,
    ) -> Result<String, Error>;

    /// Looks up the SRV record for `(service, protocol, domain)`.
    ///
    /// Mirrors `g_resolver_lookup_service`.
    fn lookup_service(
        &self,
        service: &str,
        protocol: &str,
        domain: &str,
        cancellable: Option<&GCancellable>,
    ) -> Result<Vec<crate::gsrvtarget::SrvTarget>, Error>;
}

// ──────────────────────────── NoopResolver ────────────────────────────────

/// A resolver that always returns `ResolverError::NotFound`.
///
/// Useful as a stub when no real DNS backend is available (e.g. in the
/// RustOS kernel where there is no libc resolver).
#[derive(Clone, Debug, Default)]
pub struct NoopResolver;

impl NoopResolver {
    /// Creates a new `NoopResolver`.
    pub fn new() -> Self {
        NoopResolver
    }
}

fn not_found_error() -> Error {
    Error::new(
        ResolverError::resolver_error_quark(),
        ResolverError::NotFound.to_code(),
        "name not found",
    )
}

impl Resolver for NoopResolver {
    fn lookup_by_name(
        &self,
        _hostname: &str,
        _cancellable: Option<&GCancellable>,
    ) -> Result<Vec<InetAddress>, Error> {
        Err(not_found_error())
    }

    fn lookup_by_address(
        &self,
        _address: &InetAddress,
        _cancellable: Option<&GCancellable>,
    ) -> Result<String, Error> {
        Err(not_found_error())
    }

    fn lookup_service(
        &self,
        _service: &str,
        _protocol: &str,
        _domain: &str,
        _cancellable: Option<&GCancellable>,
    ) -> Result<Vec<crate::gsrvtarget::SrvTarget>, Error> {
        Err(not_found_error())
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolver_error_codes() {
        assert_eq!(ResolverError::NotFound.to_code(), 0);
        assert_eq!(ResolverError::Temporary.to_code(), 1);
        assert_eq!(ResolverError::Internal.to_code(), 2);
    }

    #[test]
    fn test_resolver_error_quark() {
        assert_eq!(ResolverError::resolver_error_quark(), 9);
    }

    #[test]
    fn test_noop_lookup_by_name_fails() {
        let r = NoopResolver::new();
        let result = r.lookup_by_name("example.com", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_noop_lookup_by_address_fails() {
        use crate::ginetaddress::SocketFamily;
        let addr = InetAddress::new_loopback(SocketFamily::Ipv4).unwrap();
        let r = NoopResolver::new();
        let result = r.lookup_by_address(&addr, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_noop_lookup_service_fails() {
        let r = NoopResolver::new();
        let result = r.lookup_service("ldap", "tcp", "example.com", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_resolver_error_display() {
        let msg = alloc::format!("{}", ResolverError::NotFound);
        assert!(!msg.is_empty());
        let msg2 = alloc::format!("{}", ResolverError::Temporary);
        assert!(!msg2.is_empty());
    }
}
