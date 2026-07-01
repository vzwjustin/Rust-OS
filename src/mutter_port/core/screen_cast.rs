//! MetaScreenCast ported from GNOME Mutter's src/core/meta-screen-cast.c
//!
//! MetaScreenCast manages screen casting sessions: it allows clients to
//! capture the screen (or a portion of it) and stream it to a remote viewer
//! or recording application. In Mutter this is exposed over D-Bus as the
//! org.gnome.Mutter.ScreenCast service.
//!
//! In the kernel, D-Bus is not available. The screen cast manager is modeled
//! as a plain struct that tracks active sessions and their stream
//! configurations. The actual frame capture is performed by the compositor's
//! framebuffer readback path.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/core/meta-screen-cast.c

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

/// Screen cast cursor mode, mirrors MetaScreenCastCursorMode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorMode {
    /// Cursor is hidden in the stream.
    Hidden,
    /// Cursor is embedded in the stream pixels.
    Embedded,
    /// Cursor is sent as a separate metadata stream.
    Metadata,
}

impl Default for CursorMode {
    fn default() -> Self {
        CursorMode::Embedded
    }
}

/// What to capture, mirrors MetaScreenCastStreamSourceType.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamSourceType {
    /// Capture the entire monitor.
    Monitor,
    /// Capture a specific window.
    Window,
    /// Capture a virtual monitor (for remote desktop).
    Virtual,
}

/// A screen cast stream configuration.
#[derive(Debug, Clone)]
pub struct StreamConfig {
    /// Unique stream id.
    pub id: u32,
    /// What to capture.
    pub source_type: StreamSourceType,
    /// Monitor index (for Monitor source type).
    pub monitor_index: Option<usize>,
    /// Window id (for Window source type).
    pub window_id: Option<u32>,
    /// Capture width in pixels.
    pub width: u32,
    /// Capture height in pixels.
    pub height: u32,
    /// Frame rate in fps.
    pub fps: u32,
    /// Cursor mode.
    pub cursor_mode: CursorMode,
}

impl StreamConfig {
    pub fn new_monitor(monitor_index: usize, width: u32, height: u32) -> Self {
        StreamConfig {
            id: 0,
            source_type: StreamSourceType::Monitor,
            monitor_index: Some(monitor_index),
            window_id: None,
            width,
            height,
            fps: 30,
            cursor_mode: CursorMode::default(),
        }
    }

    pub fn new_window(window_id: u32, width: u32, height: u32) -> Self {
        StreamConfig {
            id: 0,
            source_type: StreamSourceType::Window,
            monitor_index: None,
            window_id: Some(window_id),
            width,
            height,
            fps: 30,
            cursor_mode: CursorMode::default(),
        }
    }
}

/// A screen cast session. Mirrors MetaScreenCastSession.
#[derive(Debug)]
pub struct ScreenCastSession {
    /// Unique session id.
    pub id: u32,
    /// Client bus name (stubbed — no D-Bus in kernel).
    pub client: String,
    /// Active streams in this session.
    pub streams: Vec<StreamConfig>,
    /// Whether the session is active (streaming).
    pub active: bool,
    /// Number of frames captured.
    pub frame_count: u64,
}

impl ScreenCastSession {
    pub fn new(id: u32, client: &str) -> Self {
        ScreenCastSession {
            id,
            client: String::from(client),
            streams: Vec::new(),
            active: false,
            frame_count: 0,
        }
    }

    /// Add a stream to the session. Returns the stream id.
    pub fn add_stream(&mut self, mut config: StreamConfig) -> u32 {
        config.id = next_stream_id();
        let stream_id = config.id;
        self.streams.push(config);
        stream_id
    }

    /// Remove a stream by id.
    pub fn remove_stream(&mut self, stream_id: u32) -> bool {
        let before = self.streams.len();
        self.streams.retain(|s| s.id != stream_id);
        self.streams.len() != before
    }

    /// Start the session (begin capturing).
    pub fn start(&mut self) -> Result<(), &'static str> {
        if self.streams.is_empty() {
            return Err("No streams in session");
        }
        if self.active {
            return Err("Session already active");
        }
        self.active = true;
        Ok(())
    }

    /// Stop the session.
    pub fn stop(&mut self) {
        self.active = false;
    }

    /// Record a captured frame.
    pub fn on_frame_captured(&mut self) {
        if self.active {
            self.frame_count += 1;
        }
    }

    /// Number of streams.
    pub fn stream_count(&self) -> usize {
        self.streams.len()
    }
}

static SESSION_ID: AtomicU32 = AtomicU32::new(0);
static STREAM_ID: AtomicU32 = AtomicU32::new(0);

fn next_session_id() -> u32 {
    SESSION_ID.fetch_add(1, Ordering::Relaxed) + 1
}

fn next_stream_id() -> u32 {
    STREAM_ID.fetch_add(1, Ordering::Relaxed) + 1
}

/// The screen cast manager. Mirrors MetaScreenCast.
#[derive(Debug)]
pub struct MetaScreenCast {
    sessions: Vec<ScreenCastSession>,
}

impl MetaScreenCast {
    /// Create a new screen cast manager. Mirrors meta_screen_cast_new().
    pub fn new() -> Self {
        MetaScreenCast {
            sessions: Vec::new(),
        }
    }

    /// Create a new session. Mirrors meta_screen_cast_create_session().
    pub fn create_session(&mut self, client: &str) -> u32 {
        let id = next_session_id();
        let session = ScreenCastSession::new(id, client);
        self.sessions.push(session);
        id
    }

    /// Get a session by id.
    pub fn get_session(&self, id: u32) -> Option<&ScreenCastSession> {
        self.sessions.iter().find(|s| s.id == id)
    }

    /// Get a mutable session by id.
    pub fn get_session_mut(&mut self, id: u32) -> Option<&mut ScreenCastSession> {
        self.sessions.iter_mut().find(|s| s.id == id)
    }

    /// Destroy a session. Mirrors meta_screen_cast_destroy_session().
    pub fn destroy_session(&mut self, id: u32) -> bool {
        let before = self.sessions.len();
        self.sessions.retain(|s| s.id != id);
        self.sessions.len() != before
    }

    /// Number of active sessions.
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Number of currently streaming sessions.
    pub fn active_session_count(&self) -> usize {
        self.sessions.iter().filter(|s| s.active).count()
    }
}

impl Default for MetaScreenCast {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_manager() {
        let sc = MetaScreenCast::new();
        assert_eq!(sc.session_count(), 0);
    }

    #[test]
    fn test_create_session() {
        let mut sc = MetaScreenCast::new();
        let id = sc.create_session("test-client");
        assert!(sc.get_session(id).is_some());
        assert_eq!(sc.session_count(), 1);
    }

    #[test]
    fn test_add_stream_and_start() {
        let mut sc = MetaScreenCast::new();
        let sid = sc.create_session("test");
        let session = sc.get_session_mut(sid).unwrap();
        let stream_id = session.add_stream(StreamConfig::new_monitor(0, 1920, 1080));
        assert!(stream_id > 0);
        assert_eq!(session.stream_count(), 1);

        assert!(session.start().is_ok());
        assert!(session.active);
    }

    #[test]
    fn test_start_without_streams_fails() {
        let mut sc = MetaScreenCast::new();
        let sid = sc.create_session("test");
        let session = sc.get_session_mut(sid).unwrap();
        assert!(session.start().is_err());
    }

    #[test]
    fn test_double_start_fails() {
        let mut sc = MetaScreenCast::new();
        let sid = sc.create_session("test");
        let session = sc.get_session_mut(sid).unwrap();
        session.add_stream(StreamConfig::new_monitor(0, 1920, 1080));
        session.start().unwrap();
        assert!(session.start().is_err());
    }

    #[test]
    fn test_stop_and_destroy() {
        let mut sc = MetaScreenCast::new();
        let sid = sc.create_session("test");
        let session = sc.get_session_mut(sid).unwrap();
        session.add_stream(StreamConfig::new_monitor(0, 1920, 1080));
        session.start().unwrap();

        session.stop();
        assert!(!session.active);

        assert!(sc.destroy_session(sid));
        assert_eq!(sc.session_count(), 0);
    }

    #[test]
    fn test_frame_count() {
        let mut sc = MetaScreenCast::new();
        let sid = sc.create_session("test");
        let session = sc.get_session_mut(sid).unwrap();
        session.add_stream(StreamConfig::new_monitor(0, 1920, 1080));
        session.start().unwrap();

        session.on_frame_captured();
        session.on_frame_captured();
        assert_eq!(session.frame_count, 2);
    }

    #[test]
    fn test_frame_count_not_incremented_when_inactive() {
        let mut sc = MetaScreenCast::new();
        let sid = sc.create_session("test");
        let session = sc.get_session_mut(sid).unwrap();
        session.add_stream(StreamConfig::new_monitor(0, 1920, 1080));
        // Not started.
        session.on_frame_captured();
        assert_eq!(session.frame_count, 0);
    }

    #[test]
    fn test_remove_stream() {
        let mut sc = MetaScreenCast::new();
        let sid = sc.create_session("test");
        let session = sc.get_session_mut(sid).unwrap();
        let stream_id = session.add_stream(StreamConfig::new_monitor(0, 1920, 1080));

        assert!(session.remove_stream(stream_id));
        assert_eq!(session.stream_count(), 0);
    }

    #[test]
    fn test_active_session_count() {
        let mut sc = MetaScreenCast::new();
        let s1 = sc.create_session("a");
        let s2 = sc.create_session("b");

        let session = sc.get_session_mut(s1).unwrap();
        session.add_stream(StreamConfig::new_monitor(0, 1920, 1080));
        session.start().unwrap();

        assert_eq!(sc.active_session_count(), 1);
        assert_eq!(sc.session_count(), 2);

        let _ = s2;
    }
}
