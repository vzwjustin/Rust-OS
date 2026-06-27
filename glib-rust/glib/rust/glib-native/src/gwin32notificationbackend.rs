//! gwin32notificationbackend matching `gio/gwin32notificationbackend.c`.
//!
//! Windows notification backend using `Shell_NotifyIcon` to display balloon
//! notifications in the system tray. Manages a hidden window lifecycle and
//! tray icon add/update/remove state.
//!
//! In this no_std port, we model the notification state machine abstractly.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gnotification::Notification;
use crate::gnotificationbackend::NotificationBackend;
use crate::prelude::*;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// Maximum title length in UTF-16 code units (`NOTIFYICONDATA.szInfoTitle`).
pub const MAX_TITLE_COUNT: usize = 63;

/// Maximum body length in UTF-16 code units (`NOTIFYICONDATA.szInfo`).
pub const MAX_BODY_COUNT: usize = 255;

/// HWND initialization state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HwndState {
    Ready,
    Failed,
    Uninitialized,
    Initializing,
    Destroying,
    InitializingNotifyIcon,
}

/// Tray icon notify operation (`NIM_ADD`, `NIM_MODIFY`, `NIM_DELETE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyIconOp {
    Add,
    Modify,
    Delete,
}

/// Record of a tray icon operation (stub for `Shell_NotifyIcon`).
#[derive(Debug, Clone)]
pub struct TrayIconEvent {
    pub op: NotifyIconOp,
    pub id: String,
    pub title: String,
    pub body: String,
}

/// Windows notification backend (`GWin32NotificationBackend`).
pub struct Win32NotificationBackend {
    inner: NotificationBackend,
    hwnd_state: Mutex<HwndState>,
    hwnd_refcount: Mutex<u32>,
    tray_icon_added: Mutex<bool>,
    tray_events: Mutex<Vec<TrayIconEvent>>,
    sent_ids: Mutex<Vec<String>>,
}

impl Win32NotificationBackend {
    pub fn new() -> Self {
        Self {
            inner: NotificationBackend::new(),
            hwnd_state: Mutex::new(HwndState::Uninitialized),
            hwnd_refcount: Mutex::new(0),
            tray_icon_added: Mutex::new(false),
            tray_events: Mutex::new(Vec::new()),
            sent_ids: Mutex::new(Vec::new()),
        }
    }

    /// Mirrors `g_win32_notification_backend_is_supported`.
    pub fn is_supported() -> bool {
        true
    }

    /// Initializes the hidden window and tray icon (`NIM_ADD`).
    pub fn init_hwnd(&self) -> bool {
        let mut state = self.hwnd_state.lock();
        match *state {
            HwndState::Uninitialized => {
                *state = HwndState::Initializing;
                *self.hwnd_refcount.lock() = 1;
                *state = HwndState::Ready;
                self.shell_notify_icon(NotifyIconOp::Add, "", "", "");
                *self.tray_icon_added.lock() = true;
                true
            }
            HwndState::Ready | HwndState::InitializingNotifyIcon => {
                *self.hwnd_refcount.lock() += 1;
                true
            }
            _ => false,
        }
    }

    pub fn hwnd_state(&self) -> HwndState {
        *self.hwnd_state.lock()
    }

    pub fn tray_icon_added(&self) -> bool {
        *self.tray_icon_added.lock()
    }

    /// Mirrors `g_win32_notification_backend_send_notification`.
    pub fn send_notification(&self, id: &str, notification: Notification) -> Result<(), String> {
        let state = *self.hwnd_state.lock();
        if state != HwndState::Ready && state != HwndState::InitializingNotifyIcon {
            return Err("notification backend not ready".to_string());
        }

        let title = notification.title();
        if title.is_empty() {
            return Err("notification title is empty".to_string());
        }

        let body = notification.body();
        let display_body = if body.is_empty() {
            " ".to_string()
        } else {
            truncate_utf16(body, MAX_BODY_COUNT)
        };

        let display_title = truncate_utf16(title, MAX_TITLE_COUNT);

        if !*self.tray_icon_added.lock() {
            self.shell_notify_icon(NotifyIconOp::Add, id, &display_title, &display_body);
            *self.tray_icon_added.lock() = true;
        } else {
            self.shell_notify_icon(NotifyIconOp::Modify, id, &display_title, &display_body);
        }

        self.sent_ids.lock().push(id.to_string());
        self.inner.send_notification(id, notification);
        Ok(())
    }

    /// Mirrors `g_win32_notification_backend_withdraw_notification` (no-op upstream).
    pub fn withdraw_notification(&self, id: &str) -> bool {
        self.inner.withdraw_notification(id)
    }

    /// Removes the tray icon (`NIM_DELETE`).
    pub fn remove_tray_icon(&self) {
        if *self.tray_icon_added.lock() {
            self.shell_notify_icon(NotifyIconOp::Delete, "", "", "");
            *self.tray_icon_added.lock() = false;
        }
    }

    pub fn pending_count(&self) -> usize {
        self.inner.pending_count()
    }

    pub fn tray_events(&self) -> Vec<TrayIconEvent> {
        self.tray_events.lock().clone()
    }

    /// Disposes the backend and tears down the hidden window.
    pub fn dispose(&self) {
        self.remove_tray_icon();
        let mut refcount = self.hwnd_refcount.lock();
        if *refcount > 0 {
            *refcount -= 1;
            if *refcount == 0 {
                *self.hwnd_state.lock() = HwndState::Destroying;
                *self.hwnd_state.lock() = HwndState::Uninitialized;
            }
        }
    }

    fn shell_notify_icon(&self, op: NotifyIconOp, id: &str, title: &str, body: &str) {
        self.tray_events.lock().push(TrayIconEvent {
            op,
            id: id.to_string(),
            title: title.to_string(),
            body: body.to_string(),
        });
    }
}

impl Default for Win32NotificationBackend {
    fn default() -> Self {
        Self::new()
    }
}

fn truncate_utf16(s: &str, max: usize) -> String {
    let units: Vec<u16> = s.encode_utf16().collect();
    if units.len() <= max {
        return s.to_string();
    }
    let mut cut = max;
    if cut > 0 && (0xDC00..=0xDFFF).contains(&units[cut]) {
        cut -= 1;
    }
    String::from_utf16_lossy(&units[..cut])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_supported() {
        assert!(Win32NotificationBackend::is_supported());
    }

    #[test]
    fn test_init_adds_tray_icon() {
        let backend = Win32NotificationBackend::new();
        assert!(backend.init_hwnd());
        assert!(backend.tray_icon_added());
        let events = backend.tray_events();
        assert_eq!(events[0].op, NotifyIconOp::Add);
    }

    #[test]
    fn test_send_notification_modify() {
        let backend = Win32NotificationBackend::new();
        backend.init_hwnd();
        backend
            .send_notification("n1", Notification::new("Hello"))
            .unwrap();
        assert_eq!(backend.pending_count(), 1);
        let events = backend.tray_events();
        assert!(events.iter().any(|e| e.op == NotifyIconOp::Modify));
    }

    #[test]
    fn test_send_empty_title_fails() {
        let backend = Win32NotificationBackend::new();
        backend.init_hwnd();
        assert!(backend
            .send_notification("n1", Notification::new(""))
            .is_err());
    }

    #[test]
    fn test_empty_body_becomes_space() {
        let backend = Win32NotificationBackend::new();
        backend.init_hwnd();
        backend
            .send_notification("n1", Notification::new("Title"))
            .unwrap();
        let events = backend.tray_events();
        let modify = events
            .iter()
            .find(|e| e.op == NotifyIconOp::Modify)
            .unwrap();
        assert_eq!(modify.body, " ");
    }

    #[test]
    fn test_remove_tray_icon() {
        let backend = Win32NotificationBackend::new();
        backend.init_hwnd();
        backend.remove_tray_icon();
        assert!(!backend.tray_icon_added());
        assert!(backend
            .tray_events()
            .iter()
            .any(|e| e.op == NotifyIconOp::Delete));
    }

    #[test]
    fn test_dispose() {
        let backend = Win32NotificationBackend::new();
        backend.init_hwnd();
        backend.dispose();
        assert_eq!(backend.hwnd_state(), HwndState::Uninitialized);
    }
}
