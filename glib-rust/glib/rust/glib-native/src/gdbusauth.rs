//! GDBusAuth matching `gio/gdbusauth.h` / `gio/gdbusauth.c`.
//!
//! D-Bus SASL authentication profile: line-oriented negotiation between
//! client and server using `AUTH`, `DATA`, `OK`, `REJECTED`, `BEGIN`, and
//! related commands. Mechanisms are delegated to [`DBusAuthMechanism`]
//! implementations (`ANONYMOUS`, `EXTERNAL`, `DBUS_COOKIE_SHA1`, …).
//!
//! Fully `no_std` compatible using `alloc`.

use crate::error::Error;
use crate::gdbusauthmechanism::{AuthMechanismState, DBusAuthMechanism};
use crate::gdbusauthmechanismanon::DBusAuthMechanismAnon;
use crate::gdbusauthmechanismexternal::DBusAuthMechanismExternal;
use crate::gdbusauthobserver::DBusAuthObserver;
use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// SASL auth session state (`GDBusAuth` internal state).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DBusAuthState {
    /// Waiting for the first client line (`AUTH` / `NEGOTIATE-UNIX-FD`).
    WaitingForBegin,
    /// Mechanism negotiation in progress.
    WaitingForAuth,
    /// Waiting for a `DATA` line from the peer.
    WaitingForData,
    /// Authentication succeeded; server waits for `BEGIN`, client waits for `OK`.
    Authenticated,
    /// Session cancelled or rejected.
    Cancelled,
}

/// Role of this auth session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DBusAuthRole {
    Server,
    Client,
}

/// D-Bus SASL authentication (`GDBusAuth`).
pub struct DBusAuth {
    role: DBusAuthRole,
    state: Mutex<DBusAuthState>,
    guid: Mutex<String>,
    allow_anonymous: bool,
    require_same_user: bool,
    mechanisms: Mutex<Vec<Box<dyn DBusAuthMechanism + Send + Sync>>>,
    active_mechanism: Mutex<Option<usize>>,
    authenticated: Mutex<bool>,
    pending_outgoing: Mutex<Vec<String>>,
}

impl DBusAuth {
    /// Creates a server-side auth context (`_g_dbus_auth_run_server` setup).
    pub fn new_server(guid: &str) -> Self {
        Self::new_server_with_flags(guid, true, false)
    }

    /// Creates a server with explicit policy flags.
    pub fn new_server_with_flags(
        guid: &str,
        allow_anonymous: bool,
        require_same_user: bool,
    ) -> Self {
        let mut mechanisms: Vec<Box<dyn DBusAuthMechanism + Send + Sync>> = Vec::new();
        if allow_anonymous {
            mechanisms.push(Box::new(DBusAuthMechanismAnon::new()));
        }
        mechanisms.push(Box::new(DBusAuthMechanismExternal::new("0")));
        Self {
            role: DBusAuthRole::Server,
            state: Mutex::new(DBusAuthState::WaitingForBegin),
            guid: Mutex::new(guid.to_string()),
            allow_anonymous,
            require_same_user,
            mechanisms: Mutex::new(mechanisms),
            active_mechanism: Mutex::new(None),
            authenticated: Mutex::new(false),
            pending_outgoing: Mutex::new(Vec::new()),
        }
    }

    /// Creates a client-side auth context (`_g_dbus_auth_run_client` setup).
    pub fn new_client() -> Self {
        Self {
            role: DBusAuthRole::Client,
            state: Mutex::new(DBusAuthState::WaitingForBegin),
            guid: Mutex::new(String::new()),
            allow_anonymous: true,
            require_same_user: false,
            mechanisms: Mutex::new(vec![
                Box::new(DBusAuthMechanismAnon::new()),
                Box::new(DBusAuthMechanismExternal::new("0")),
            ]),
            active_mechanism: Mutex::new(None),
            authenticated: Mutex::new(false),
            pending_outgoing: Mutex::new(Vec::new()),
        }
    }

    /// Back-compat constructor — equivalent to [`DBusAuth::new_server`].
    pub fn new(guid: &str) -> Self {
        Self::new_server(guid)
    }

    /// Registers an additional auth mechanism.
    pub fn add_mechanism(&self, mechanism: Box<dyn DBusAuthMechanism + Send + Sync>) {
        self.mechanisms.lock().push(mechanism);
    }

    /// Returns supported mechanism names.
    pub fn get_mechanisms(&self) -> Vec<String> {
        self.mechanisms
            .lock()
            .iter()
            .filter(|m| m.is_supported())
            .map(|m| m.name().to_string())
            .collect()
    }

    pub fn get_guid(&self) -> String {
        self.guid.lock().clone()
    }

    pub fn get_state(&self) -> DBusAuthState {
        *self.state.lock()
    }

    pub fn role(&self) -> DBusAuthRole {
        self.role
    }

    pub fn is_authenticated(&self) -> bool {
        *self.authenticated.lock()
    }

    /// Marks the session authenticated (testing / loopback shortcut).
    pub fn authenticate(&self) {
        *self.state.lock() = DBusAuthState::Authenticated;
        *self.authenticated.lock() = true;
    }

    pub fn cancel(&self) {
        *self.state.lock() = DBusAuthState::Cancelled;
        *self.authenticated.lock() = false;
        self.active_mechanism.lock().take();
    }

    /// Client: first SASL lines to send after connecting.
    pub fn client_start(&self) -> Vec<String> {
        if self.role != DBusAuthRole::Client {
            return Vec::new();
        }
        let mechs = self.mechanisms.lock();
        if let Some(m) = mechs
            .iter()
            .find(|m| m.is_supported() && m.name() == "ANONYMOUS")
        {
            if let Some(data) = m.initiate() {
                return vec![format!("AUTH ANONYMOUS {}", hex_encode(data.as_bytes()))];
            }
            return vec!["AUTH ANONYMOUS".to_string()];
        }
        if let Some(m) = mechs
            .iter()
            .find(|m| m.is_supported() && m.name() == "EXTERNAL")
        {
            if let Some(data) = m.initiate() {
                return vec![format!("AUTH EXTERNAL {}", hex_encode(data.as_bytes()))];
            }
        }
        Vec::new()
    }

    /// Server: process one incoming SASL line and return zero or more reply lines.
    pub fn server_feed_line(
        &self,
        line: &str,
        observer: Option<&DBusAuthObserver>,
    ) -> Result<Vec<String>, Error> {
        if self.role != DBusAuthRole::Server {
            return Ok(Vec::new());
        }
        let line = line.trim();
        if line.is_empty() {
            return Ok(Vec::new());
        }

        let mut state = self.state.lock();
        match *state {
            DBusAuthState::WaitingForBegin | DBusAuthState::WaitingForAuth => {
                if let Some(rest) = line.strip_prefix("AUTH ") {
                    return self.server_handle_auth(rest, observer, &mut *state);
                }
                if line == "BEGIN" {
                    if *self.authenticated.lock() {
                        *state = DBusAuthState::Authenticated;
                        return Ok(Vec::new());
                    }
                    return Ok(vec!["REJECTED".to_string()]);
                }
                if line.starts_with("NEGOTIATE-UNIX-FD") {
                    return Ok(vec!["ERROR Unsupported command".to_string()]);
                }
                Ok(vec!["REJECTED".to_string()])
            }
            DBusAuthState::WaitingForData => {
                if let Some(data) = line.strip_prefix("DATA ") {
                    return self.server_handle_data(data, &mut *state);
                }
                if line == "BEGIN" {
                    if *self.authenticated.lock() {
                        *state = DBusAuthState::Authenticated;
                        return Ok(Vec::new());
                    }
                }
                Ok(vec!["REJECTED".to_string()])
            }
            DBusAuthState::Authenticated => {
                if line == "BEGIN" {
                    return Ok(Vec::new());
                }
                Ok(Vec::new())
            }
            DBusAuthState::Cancelled => Ok(vec!["REJECTED".to_string()]),
        }
    }

    /// Client: process one incoming SASL line from the server.
    pub fn client_feed_line(&self, line: &str) -> Result<Vec<String>, Error> {
        if self.role != DBusAuthRole::Client {
            return Ok(Vec::new());
        }
        let line = line.trim();
        if line.is_empty() {
            return Ok(Vec::new());
        }

        let mut state = self.state.lock();
        if let Some(guid) = line.strip_prefix("OK ") {
            *self.guid.lock() = guid.to_string();
            *self.authenticated.lock() = true;
            *state = DBusAuthState::Authenticated;
            return Ok(vec!["BEGIN".to_string()]);
        }
        if line == "OK" {
            *self.authenticated.lock() = true;
            *state = DBusAuthState::Authenticated;
            return Ok(vec!["BEGIN".to_string()]);
        }
        if line == "REJECTED" {
            *state = DBusAuthState::Cancelled;
            return Ok(Vec::new());
        }
        if let Some(data) = line.strip_prefix("DATA ") {
            let idx = self.active_mechanism.lock().unwrap_or(0);
            let mechs = self.mechanisms.lock();
            if let Some(m) = mechs.get(idx) {
                let decoded = hex_decode(data).unwrap_or_else(|| data.to_string());
                match m.process_data(&decoded) {
                    AuthMechanismState::Accepted => {
                        *self.authenticated.lock() = true;
                        *state = DBusAuthState::Authenticated;
                        return Ok(Vec::new());
                    }
                    AuthMechanismState::HaveDataToSend => {
                        if let Some(out) = m.initiate() {
                            return Ok(vec![format!("DATA {}", hex_encode(out.as_bytes()))]);
                        }
                    }
                    AuthMechanismState::Rejected => {
                        *state = DBusAuthState::Cancelled;
                    }
                    _ => {}
                }
            }
        }
        Ok(Vec::new())
    }

    /// Runs a full in-memory server exchange (for tests and loopback transports).
    pub fn run_server_sync(
        &self,
        client_lines: &[&str],
        observer: Option<&DBusAuthObserver>,
    ) -> Result<Vec<String>, Error> {
        let mut replies = Vec::new();
        for line in client_lines {
            let mut out = self.server_feed_line(line, observer)?;
            replies.append(&mut out);
            if self.get_state() == DBusAuthState::Cancelled {
                break;
            }
        }
        Ok(replies)
    }

    /// Runs a full in-memory client exchange (for tests).
    pub fn run_client_sync(&self, server_lines: &[&str]) -> Result<Vec<String>, Error> {
        let mut all = self.client_start();
        for line in server_lines {
            let mut out = self.client_feed_line(line)?;
            all.append(&mut out);
        }
        Ok(all)
    }

    fn server_handle_auth(
        &self,
        rest: &str,
        observer: Option<&DBusAuthObserver>,
        state: &mut DBusAuthState,
    ) -> Result<Vec<String>, Error> {
        let mut parts = rest.splitn(2, ' ');
        let mech_name = parts.next().unwrap_or("");
        let initial = parts.next().unwrap_or("");
        let initial_data = hex_decode(initial).unwrap_or_else(|| initial.to_string());

        if let Some(obs) = observer {
            if !obs.allow_mechanism(mech_name) {
                *state = DBusAuthState::Cancelled;
                return Ok(vec!["REJECTED".to_string()]);
            }
        }

        if mech_name == "ANONYMOUS" && !self.allow_anonymous {
            *state = DBusAuthState::Cancelled;
            return Ok(vec!["REJECTED".to_string()]);
        }

        let mechs = self.mechanisms.lock();
        let idx = mechs
            .iter()
            .position(|m| m.is_supported() && m.name() == mech_name);
        let Some(idx) = idx else {
            *state = DBusAuthState::Cancelled;
            return Ok(vec!["REJECTED".to_string()]);
        };

        let mech = &mechs[idx];
        *self.active_mechanism.lock() = Some(idx);

        let result = if initial_data.is_empty() {
            mech.initiate().map(|s| s).unwrap_or_default()
        } else {
            match mech.process_data(&initial_data) {
                AuthMechanismState::Accepted => String::new(),
                AuthMechanismState::HaveDataToSend => mech.initiate().unwrap_or_default(),
                AuthMechanismState::Rejected => {
                    *state = DBusAuthState::Cancelled;
                    return Ok(vec!["REJECTED".to_string()]);
                }
                _ => initial_data.clone(),
            }
        };

        match mech.process_data(if result.is_empty() {
            &initial_data
        } else {
            &result
        }) {
            AuthMechanismState::Accepted | AuthMechanismState::Initial => {
                *self.authenticated.lock() = true;
                *state = DBusAuthState::Authenticated;
                let guid = self.guid.lock().clone();
                Ok(vec![format!("OK {guid}")])
            }
            AuthMechanismState::HaveDataToSend => {
                *state = DBusAuthState::WaitingForData;
                Ok(vec![format!("DATA {}", hex_encode(result.as_bytes()))])
            }
            AuthMechanismState::Rejected => {
                *state = DBusAuthState::Cancelled;
                Ok(vec!["REJECTED".to_string()])
            }
            AuthMechanismState::WaitingForData => {
                *state = DBusAuthState::WaitingForData;
                Ok(Vec::new())
            }
        }
    }

    fn server_handle_data(
        &self,
        data: &str,
        state: &mut DBusAuthState,
    ) -> Result<Vec<String>, Error> {
        let idx = match *self.active_mechanism.lock() {
            Some(i) => i,
            None => {
                *state = DBusAuthState::Cancelled;
                return Ok(vec!["REJECTED".to_string()]);
            }
        };
        let decoded = hex_decode(data).unwrap_or_else(|| data.to_string());
        let mechs = self.mechanisms.lock();
        let mech = &mechs[idx];
        match mech.process_data(&decoded) {
            AuthMechanismState::Accepted => {
                *self.authenticated.lock() = true;
                *state = DBusAuthState::Authenticated;
                let guid = self.guid.lock().clone();
                Ok(vec![format!("OK {guid}")])
            }
            AuthMechanismState::HaveDataToSend => {
                if let Some(out) = mech.initiate() {
                    Ok(vec![format!("DATA {}", hex_encode(out.as_bytes()))])
                } else {
                    Ok(Vec::new())
                }
            }
            AuthMechanismState::Rejected => {
                *state = DBusAuthState::Cancelled;
                Ok(vec!["REJECTED".to_string()])
            }
            _ => Ok(Vec::new()),
        }
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0xf) as usize] as char);
    }
    out
}

fn hex_decode(hex: &str) -> Option<String> {
    if hex.is_empty() {
        return Some(String::new());
    }
    if hex.len() % 2 != 0 {
        return None;
    }
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    let chars: Vec<char> = hex.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let hi = chars[i].to_digit(16)? as u8;
        let lo = chars[i + 1].to_digit(16)? as u8;
        bytes.push((hi << 4) | lo);
        i += 2;
    }
    String::from_utf8(bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_anonymous_auth() {
        let auth = DBusAuth::new_server("server-guid-1");
        let replies = auth.run_server_sync(&["AUTH ANONYMOUS"], None).unwrap();
        assert_eq!(replies.len(), 1);
        assert!(replies[0].starts_with("OK server-guid-1"));
        assert!(auth.is_authenticated());
    }

    #[test]
    fn test_server_rejects_unknown_mechanism() {
        let auth = DBusAuth::new_server("g");
        let replies = auth.run_server_sync(&["AUTH NOPE"], None).unwrap();
        assert_eq!(replies, vec!["REJECTED".to_string()]);
        assert_eq!(auth.get_state(), DBusAuthState::Cancelled);
    }

    #[test]
    fn test_client_server_roundtrip() {
        let server = DBusAuth::new_server("abc-guid");
        let client = DBusAuth::new_client();
        let client_first = client.client_start();
        assert!(!client_first.is_empty());
        let server_replies = server
            .run_server_sync(&[client_first[0].as_str()], None)
            .unwrap();
        assert!(server.is_authenticated());
        let client_replies = client.client_feed_line(&server_replies[0]).unwrap();
        assert_eq!(client_replies, vec!["BEGIN".to_string()]);
        assert!(client.is_authenticated());
        assert_eq!(client.get_guid(), "abc-guid");
    }

    #[test]
    fn test_cancel() {
        let a = DBusAuth::new_server("guid");
        a.cancel();
        assert_eq!(a.get_state(), DBusAuthState::Cancelled);
    }

    #[test]
    fn hex_roundtrip() {
        assert_eq!(hex_decode(&hex_encode(b"rust")).as_deref(), Some("rust"));
    }
}
