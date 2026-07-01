//! MetaRemoteDesktop ported from GNOME Mutter's src/core/meta-remote-desktop.c
//!
//! MetaRemoteDesktop manages remote desktop control sessions: it allows a
//! remote client to inject input events (keyboard, pointer, touch) and
//! optionally capture the screen. In Mutter this is exposed over D-Bus as
//! the org.gnome.Mutter.RemoteDesktop service.
//!
//! In the kernel, D-Bus is not available. The remote desktop manager is
//! modeled as a plain struct that tracks sessions and provides event
//! injection helpers. The compositor polls for injected events and
//! processes them through the normal input path.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/core/meta-remote-desktop.c

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

/// Remote desktop session. Mirrors MetaRemoteDesktopSession.
#[derive(Debug)]
pub struct RemoteDesktopSession {
    /// Unique session id.
    pub id: u32,
    /// Client bus name (stubbed — no D-Bus in kernel).
    pub client: String,
    /// Whether the session is active.
    pub active: bool,
    /// Whether screen cast is attached to this session.
    pub has_screen_cast: bool,
    /// Screen cast session id, if attached.
    pub screen_cast_session_id: Option<u32>,
    /// Queued input events to inject.
    pub injected_events: Vec<InjectedEvent>,
    /// Number of events injected so far.
    pub event_count: u64,
}

/// An input event injected by a remote desktop client.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InjectedEvent {
    /// Key event. (keycode, pressed)
    Key { keycode: u32, pressed: bool },
    /// Pointer motion (relative). (dx, dy)
    PointerMotion { dx: f64, dy: f64 },
    /// Pointer absolute position. (x, y)
    PointerAbsolute { x: f64, y: f64 },
    /// Button event. (button, pressed)
    Button { button: u32, pressed: bool },
    /// Scroll event. (dx, dy)
    Scroll { dx: f64, dy: f64 },
    /// Touch down. (slot, x, y)
    TouchDown { slot: u32, x: f64, y: f64 },
    /// Touch motion. (slot, x, y)
    TouchMotion { slot: u32, x: f64, y: f64 },
    /// Touch up. (slot)
    TouchUp { slot: u32 },
}

static SESSION_ID: AtomicU32 = AtomicU32::new(0);

fn next_session_id() -> u32 {
    SESSION_ID.fetch_add(1, Ordering::Relaxed) + 1
}

impl RemoteDesktopSession {
    pub fn new(id: u32, client: &str) -> Self {
        RemoteDesktopSession {
            id,
            client: String::from(client),
            active: false,
            has_screen_cast: false,
            screen_cast_session_id: None,
            injected_events: Vec::new(),
            event_count: 0,
        }
    }

    /// Start the session. Mirrors meta_remote_desktop_session_start().
    pub fn start(&mut self) -> Result<(), &'static str> {
        if self.active {
            return Err("Session already active");
        }
        self.active = true;
        Ok(())
    }

    /// Stop the session. Mirrors meta_remote_desktop_session_stop().
    pub fn stop(&mut self) {
        self.active = false;
        self.injected_events.clear();
    }

    /// Inject an input event. Mirrors the D-Bus method handlers
    /// (NotifyKeyboardKeycode, NotifyPointerMotion, etc.).
    pub fn inject(&mut self, event: InjectedEvent) {
        if self.active {
            self.injected_events.push(event);
            self.event_count += 1;
        }
    }

    /// Drain injected events. The compositor processes these through
    /// the normal input path.
    pub fn take_injected_events(&mut self) -> Vec<InjectedEvent> {
        core::mem::take(&mut self.injected_events)
    }

    /// Number of pending injected events.
    pub fn pending_event_count(&self) -> usize {
        self.injected_events.len()
    }

    /// Attach a screen cast session. Mirrors
    /// meta_remote_desktop_session_attach_screen_cast().
    pub fn attach_screen_cast(&mut self, screen_cast_session_id: u32) {
        self.has_screen_cast = true;
        self.screen_cast_session_id = Some(screen_cast_session_id);
    }

    /// Detach the screen cast session.
    pub fn detach_screen_cast(&mut self) {
        self.has_screen_cast = false;
        self.screen_cast_session_id = None;
    }
}

/// The remote desktop manager. Mirrors MetaRemoteDesktop.
#[derive(Debug)]
pub struct MetaRemoteDesktop {
    sessions: Vec<RemoteDesktopSession>,
}

impl MetaRemoteDesktop {
    /// Create a new remote desktop manager. Mirrors meta_remote_desktop_new().
    pub fn new() -> Self {
        MetaRemoteDesktop {
            sessions: Vec::new(),
        }
    }

    /// Create a new session. Mirrors meta_remote_desktop_create_session().
    pub fn create_session(&mut self, client: &str) -> u32 {
        let id = next_session_id();
        self.sessions.push(RemoteDesktopSession::new(id, client));
        id
    }

    /// Get a session by id.
    pub fn get_session(&self, id: u32) -> Option<&RemoteDesktopSession> {
        self.sessions.iter().find(|s| s.id == id)
    }

    /// Get a mutable session by id.
    pub fn get_session_mut(&mut self, id: u32) -> Option<&mut RemoteDesktopSession> {
        self.sessions.iter_mut().find(|s| s.id == id)
    }

    /// Destroy a session. Mirrors meta_remote_desktop_destroy_session().
    pub fn destroy_session(&mut self, id: u32) -> bool {
        let before = self.sessions.len();
        self.sessions.retain(|s| s.id != id);
        self.sessions.len() != before
    }

    /// Number of sessions.
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Number of active sessions.
    pub fn active_session_count(&self) -> usize {
        self.sessions.iter().filter(|s| s.active).count()
    }
}

impl Default for MetaRemoteDesktop {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_manager() {
        let rd = MetaRemoteDesktop::new();
        assert_eq!(rd.session_count(), 0);
    }

    #[test]
    fn test_create_session() {
        let mut rd = MetaRemoteDesktop::new();
        let id = rd.create_session("test");
        assert!(rd.get_session(id).is_some());
        assert_eq!(rd.session_count(), 1);
    }

    #[test]
    fn test_start_stop() {
        let mut rd = MetaRemoteDesktop::new();
        let id = rd.create_session("test");

        let session = rd.get_session_mut(id).unwrap();
        assert!(session.start().is_ok());
        assert!(session.active);
        assert!(session.start().is_err()); // double start fails

        session.stop();
        assert!(!session.active);
    }

    #[test]
    fn test_inject_events() {
        let mut rd = MetaRemoteDesktop::new();
        let id = rd.create_session("test");
        let session = rd.get_session_mut(id).unwrap();
        session.start().unwrap();

        session.inject(InjectedEvent::Key {
            keycode: 30,
            pressed: true,
        });
        session.inject(InjectedEvent::PointerMotion { dx: 10.0, dy: 5.0 });
        session.inject(InjectedEvent::Button {
            button: 1,
            pressed: true,
        });

        assert_eq!(session.pending_event_count(), 3);
        assert_eq!(session.event_count, 3);

        let events = session.take_injected_events();
        assert_eq!(events.len(), 3);
        assert_eq!(session.pending_event_count(), 0);
    }

    #[test]
    fn test_inject_when_inactive_ignored() {
        let mut rd = MetaRemoteDesktop::new();
        let id = rd.create_session("test");
        let session = rd.get_session_mut(id).unwrap();
        // Not started.
        session.inject(InjectedEvent::Key {
            keycode: 30,
            pressed: true,
        });
        assert_eq!(session.pending_event_count(), 0);
    }

    #[test]
    fn test_screen_cast_attach() {
        let mut rd = MetaRemoteDesktop::new();
        let id = rd.create_session("test");
        let session = rd.get_session_mut(id).unwrap();

        session.attach_screen_cast(42);
        assert!(session.has_screen_cast);
        assert_eq!(session.screen_cast_session_id, Some(42));

        session.detach_screen_cast();
        assert!(!session.has_screen_cast);
    }

    #[test]
    fn test_destroy_session() {
        let mut rd = MetaRemoteDesktop::new();
        let id = rd.create_session("test");
        assert!(rd.destroy_session(id));
        assert_eq!(rd.session_count(), 0);
        assert!(!rd.destroy_session(id));
    }

    #[test]
    fn test_active_session_count() {
        let mut rd = MetaRemoteDesktop::new();
        let s1 = rd.create_session("a");
        let s2 = rd.create_session("b");

        rd.get_session_mut(s1).unwrap().start().unwrap();
        assert_eq!(rd.active_session_count(), 1);

        rd.get_session_mut(s2).unwrap().start().unwrap();
        assert_eq!(rd.active_session_count(), 2);

        rd.get_session_mut(s1).unwrap().stop();
        assert_eq!(rd.active_session_count(), 1);
    }

    #[test]
    fn test_all_event_types() {
        let mut rd = MetaRemoteDesktop::new();
        let id = rd.create_session("test");
        let session = rd.get_session_mut(id).unwrap();
        session.start().unwrap();

        session.inject(InjectedEvent::Key {
            keycode: 1,
            pressed: true,
        });
        session.inject(InjectedEvent::PointerMotion { dx: 1.0, dy: 1.0 });
        session.inject(InjectedEvent::PointerAbsolute { x: 100.0, y: 200.0 });
        session.inject(InjectedEvent::Button {
            button: 0,
            pressed: true,
        });
        session.inject(InjectedEvent::Scroll { dx: 0.0, dy: -1.0 });
        session.inject(InjectedEvent::TouchDown {
            slot: 0,
            x: 10.0,
            y: 10.0,
        });
        session.inject(InjectedEvent::TouchMotion {
            slot: 0,
            x: 20.0,
            y: 20.0,
        });
        session.inject(InjectedEvent::TouchUp { slot: 0 });

        assert_eq!(session.pending_event_count(), 8);
    }
}
