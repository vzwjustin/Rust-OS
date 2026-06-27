//! GTlsInteraction matching `gio/gtlsinteraction.h`.
//!
//! A `TlsInteraction` object is queried during TLS handshakes to supply
//! passwords or certificates. In this no_std port it acts as a configurable
//! stub backed by `spin::Mutex`.

use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// Result of a TLS interaction request.
///
/// Mirrors the `GTlsInteractionResult` enum from `gio/gtlsinteraction.h`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsInteractionResult {
    /// The interaction was not handled.
    Unhandled,
    /// The interaction was handled and provided a value.
    Handled,
    /// The interaction failed.
    Failed,
}

/// Password prompt result (`GTlsPassword`).
///
/// Holds a byte-level password value and a human-readable description.
pub struct TlsPassword {
    value: Vec<u8>,
    description: String,
}

impl TlsPassword {
    /// Creates a new, empty `TlsPassword` with the given description.
    ///
    /// Mirrors `g_tls_password_new`.
    pub fn new(description: &str) -> Self {
        Self {
            value: Vec::new(),
            description: description.into(),
        }
    }

    /// Sets the raw password bytes.
    ///
    /// Mirrors `g_tls_password_set_value`.
    pub fn set_value(&mut self, value: &[u8]) {
        self.value = value.to_vec();
    }

    /// Returns the raw password bytes.
    ///
    /// Mirrors `g_tls_password_get_value`.
    pub fn get_value(&self) -> &[u8] {
        &self.value
    }

    /// Returns the human-readable description.
    ///
    /// Mirrors `g_tls_password_get_description`.
    pub fn get_description(&self) -> &str {
        &self.description
    }
}

/// TLS interaction handler (`GTlsInteraction`).
///
/// Holds an optional pre-configured password and certificate name that are
/// returned during a simulated TLS handshake. Suitable for use in bare-metal
/// and test contexts where real user interaction is unavailable.
pub struct TlsInteraction {
    /// Pre-configured password to return on `ask_password`.
    password: Mutex<Option<Vec<u8>>>,
    /// Pre-configured certificate name to return on `request_certificate`.
    certificate: Mutex<Option<String>>,
}

impl TlsInteraction {
    /// Creates a new `TlsInteraction` with no pre-configured values.
    pub fn new() -> Self {
        Self {
            password: Mutex::new(None),
            certificate: Mutex::new(None),
        }
    }

    /// Pre-configures the password that will be supplied to the next
    /// `ask_password` call.
    pub fn set_password(&self, password: &[u8]) {
        *self.password.lock() = Some(password.to_vec());
    }

    /// Attempts to fill `prompt` with the pre-configured password.
    ///
    /// Returns [`TlsInteractionResult::Handled`] and writes the password into
    /// `prompt` when one is configured; otherwise returns
    /// [`TlsInteractionResult::Unhandled`].
    ///
    /// Mirrors `g_tls_interaction_ask_password`.
    pub fn ask_password(&self, prompt: &mut TlsPassword) -> TlsInteractionResult {
        let guard = self.password.lock();
        match guard.as_deref() {
            Some(pw) => {
                prompt.set_value(pw);
                TlsInteractionResult::Handled
            }
            None => TlsInteractionResult::Unhandled,
        }
    }

    /// Pre-configures the certificate name that will be returned by the next
    /// `request_certificate` call.
    pub fn set_certificate_name(&self, name: &str) {
        *self.certificate.lock() = Some(name.into());
    }

    /// Returns the pre-configured certificate name, if any.
    ///
    /// Returns `(Handled, Some(name))` when a certificate name is configured;
    /// otherwise `(Unhandled, None)`.
    ///
    /// Mirrors `g_tls_interaction_request_certificate`.
    pub fn request_certificate(&self) -> (TlsInteractionResult, Option<String>) {
        let guard = self.certificate.lock();
        match guard.clone() {
            Some(name) => (TlsInteractionResult::Handled, Some(name)),
            None => (TlsInteractionResult::Unhandled, None),
        }
    }
}

impl Default for TlsInteraction {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- TlsPassword tests ---

    #[test]
    fn password_new_empty_value() {
        let p = TlsPassword::new("Enter passphrase");
        assert_eq!(p.get_value(), b"");
        assert_eq!(p.get_description(), "Enter passphrase");
    }

    #[test]
    fn password_set_and_get_value() {
        let mut p = TlsPassword::new("desc");
        p.set_value(b"s3cr3t");
        assert_eq!(p.get_value(), b"s3cr3t");
    }

    #[test]
    fn password_overwrite_value() {
        let mut p = TlsPassword::new("desc");
        p.set_value(b"first");
        p.set_value(b"second");
        assert_eq!(p.get_value(), b"second");
    }

    // --- TlsInteraction ask_password tests ---

    #[test]
    fn ask_password_unhandled_when_not_configured() {
        let interaction = TlsInteraction::new();
        let mut prompt = TlsPassword::new("Enter TLS password");
        let result = interaction.ask_password(&mut prompt);
        assert_eq!(result, TlsInteractionResult::Unhandled);
        assert_eq!(prompt.get_value(), b"");
    }

    #[test]
    fn ask_password_handled_when_configured() {
        let interaction = TlsInteraction::new();
        interaction.set_password(b"my_password");
        let mut prompt = TlsPassword::new("Enter TLS password");
        let result = interaction.ask_password(&mut prompt);
        assert_eq!(result, TlsInteractionResult::Handled);
        assert_eq!(prompt.get_value(), b"my_password");
    }

    // --- TlsInteraction request_certificate tests ---

    #[test]
    fn request_certificate_unhandled_when_not_configured() {
        let interaction = TlsInteraction::new();
        let (result, cert) = interaction.request_certificate();
        assert_eq!(result, TlsInteractionResult::Unhandled);
        assert!(cert.is_none());
    }

    #[test]
    fn request_certificate_handled_when_configured() {
        let interaction = TlsInteraction::new();
        interaction.set_certificate_name("client.example.com");
        let (result, cert) = interaction.request_certificate();
        assert_eq!(result, TlsInteractionResult::Handled);
        assert_eq!(cert.as_deref(), Some("client.example.com"));
    }

    #[test]
    fn default_is_same_as_new() {
        let interaction = TlsInteraction::default();
        let mut prompt = TlsPassword::new("p");
        assert_eq!(
            interaction.ask_password(&mut prompt),
            TlsInteractionResult::Unhandled
        );
        let (r, c) = interaction.request_certificate();
        assert_eq!(r, TlsInteractionResult::Unhandled);
        assert!(c.is_none());
    }
}
