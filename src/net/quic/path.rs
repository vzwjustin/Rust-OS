//! QUIC path validation and migration (RFC 9000 §8, §9).
//!
//! Mirrors `net/quic/path.{c,h}`. A path is the 4-tuple a connection currently
//! uses. Before migrating to a new path an endpoint validates it with a
//! PATH_CHALLENGE / PATH_RESPONSE exchange of an 8-byte random token.

use super::super::NetworkAddress;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathState {
    /// Path in active use and validated.
    Active,
    /// PATH_CHALLENGE sent, awaiting matching PATH_RESPONSE.
    Validating,
    /// Validation failed or path abandoned.
    Failed,
}

#[derive(Debug, Clone)]
pub struct Path {
    pub local: NetworkAddress,
    pub local_port: u16,
    pub remote: NetworkAddress,
    pub remote_port: u16,
    pub state: PathState,
    /// The challenge token we sent (matched against the peer's response).
    pub challenge: Option<[u8; 8]>,
}

impl Path {
    pub fn new(
        local: NetworkAddress,
        local_port: u16,
        remote: NetworkAddress,
        remote_port: u16,
    ) -> Self {
        Self {
            local,
            local_port,
            remote,
            remote_port,
            state: PathState::Validating,
            challenge: None,
        }
    }

    /// Begin validation with a freshly generated 8-byte challenge token.
    pub fn start_validation(&mut self, token: [u8; 8]) {
        self.challenge = Some(token);
        self.state = PathState::Validating;
    }

    /// Process a PATH_RESPONSE; the path becomes Active iff the echoed token
    /// matches the outstanding challenge (RFC 9000 §8.2.2).
    pub fn on_response(&mut self, token: &[u8; 8]) -> bool {
        match self.challenge {
            Some(expected) if &expected == token => {
                self.state = PathState::Active;
                self.challenge = None;
                true
            }
            _ => false,
        }
    }
}
