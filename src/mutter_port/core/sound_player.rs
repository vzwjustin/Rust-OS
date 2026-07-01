//! Sound player ported from GNOME Mutter's src/core/meta-sound-player.c
//!
//! Manages event sound playback using Canberra sound library.
//! Handles sound caching and asynchronous playback.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/core/meta-sound-player.c

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Sound events that can be cached for quick playback
const CACHE_ALLOW_LIST: &[&str] = &[
    "bell-window-system",
    "desktop-switch-left",
    "desktop-switch-right",
    "desktop-switch-up",
    "desktop-switch-down",
];

/// Configuration keys for sound settings
const EVENT_SOUNDS_KEY: &str = "event-sounds";
const THEME_NAME_KEY: &str = "theme-name";

/// Sound play request tracking
#[derive(Debug, Clone)]
struct PlayRequest {
    id: u32,
    event_name: String,
    properties: BTreeMap<String, String>,
}

/// Sound player for compositor events
#[derive(Debug)]
pub struct SoundPlayer {
    pub id: u32,
    /// Queue of pending play requests
    queue: Vec<PlayRequest>,
    /// Cached sound data
    cache: BTreeMap<String, Vec<u8>>,
    /// Next request ID
    id_pool: u32,
    /// Whether event sounds are enabled
    sounds_enabled: bool,
    /// Current sound theme name
    theme_name: String,
}

impl SoundPlayer {
    /// Create new sound player
    pub fn new() -> Self {
        SoundPlayer {
            id: 0,
            queue: Vec::new(),
            cache: BTreeMap::new(),
            id_pool: 0,
            sounds_enabled: true,
            theme_name: "freedesktop".to_string(),
        }
    }

    /// Play a sound event asynchronously
    /// Stub: requires Canberra library integration
    pub fn play_async(&mut self, event_name: &str) -> u32 {
        let req_id = self.id_pool;
        self.id_pool = self.id_pool.wrapping_add(1);

        let request = PlayRequest {
            id: req_id,
            event_name: event_name.to_string(),
            properties: BTreeMap::new(),
        };

        self.queue.push(request);
        // Stub: would invoke Canberra context to play sound
        req_id
    }

    /// Play sound with custom properties
    /// Stub: requires Canberra property list handling
    pub fn play_with_properties(
        &mut self,
        event_name: &str,
        properties: BTreeMap<String, String>,
    ) -> u32 {
        let req_id = self.id_pool;
        self.id_pool = self.id_pool.wrapping_add(1);

        let request = PlayRequest {
            id: req_id,
            event_name: event_name.to_string(),
            properties,
        };

        self.queue.push(request);
        req_id
    }

    /// Cancel pending sound playback
    pub fn cancel(&mut self, request_id: u32) -> bool {
        if let Some(pos) = self.queue.iter().position(|r| r.id == request_id) {
            self.queue.remove(pos);
            return true;
        }
        false
    }

    /// Set whether event sounds are enabled
    pub fn set_sounds_enabled(&mut self, enabled: bool) {
        self.sounds_enabled = enabled;
    }

    /// Check if event sounds are enabled
    pub fn is_sounds_enabled(&self) -> bool {
        self.sounds_enabled
    }

    /// Set sound theme name
    pub fn set_theme_name(&mut self, theme_name: String) {
        self.theme_name = theme_name;
    }

    /// Get current sound theme name
    pub fn get_theme_name(&self) -> &str {
        &self.theme_name
    }

    /// Cache a sound for quick playback
    pub fn cache_sound(&mut self, event_name: String, data: Vec<u8>) {
        if CACHE_ALLOW_LIST.contains(&event_name.as_str()) {
            self.cache.insert(event_name, data);
        }
    }

    /// Check if sound is cached
    pub fn is_cached(&self, event_name: &str) -> bool {
        self.cache.contains_key(event_name)
    }

    /// Get cached sound data
    pub fn get_cached(&self, event_name: &str) -> Option<&[u8]> {
        self.cache.get(event_name).map(|v| v.as_slice())
    }
}

impl Default for SoundPlayer {
    fn default() -> Self {
        Self::new()
    }
}
