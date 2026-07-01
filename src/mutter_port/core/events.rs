//! Event routing logic ported from GNOME Mutter (src/core/events.c).
//!
//! Implements core event dispatch to determine which window/grab receives an event.
//! Ref: https://github.com/GNOME/mutter/blob/main/src/core/events.c

use crate::desktop::window_manager::{DesktopEvent, WindowId};

/// Event routing decision: where an event should be dispatched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventRoute {
    /// Event handled by a grab or system-level handler; stop propagation.
    Stop,
    /// Event should propagate to the window; may be consumed.
    ToWindow(WindowId),
    /// Event not handled; propagate through normal channels.
    Propagate,
}

/// Core event routing logic.
///
/// Determines which window (if any) should receive an event based on focus state,
/// grabs, and hit-test results. Mirrors Mutter's `get_window_for_event()` flow.
///
/// # Arguments
/// * `event` - The incoming event
/// * `focused_window` - Currently focused window (for key events)
/// * `grabbed_window` - Window with an active grab (takes precedence)
/// * `hit_test` - Closure to determine window at pointer coordinates (x, y -> Option<WindowId>)
pub fn route_event<F>(
    event: &DesktopEvent,
    focused_window: Option<WindowId>,
    grabbed_window: Option<WindowId>,
    hit_test: F,
) -> EventRoute
where
    F: Fn(usize, usize) -> Option<WindowId>,
{
    // If there's an active grab, it consumes the event (Mutter: stage_has_grab check).
    if grabbed_window.is_some() {
        return EventRoute::Propagate;
    }

    match event {
        // Key events always route to focused window (Mutter: IS_KEY_EVENT check).
        DesktopEvent::KeyDown { .. } | DesktopEvent::KeyUp { .. } => {
            if let Some(window) = focused_window {
                EventRoute::ToWindow(window)
            } else {
                EventRoute::Propagate
            }
        }

        // Pointer events route via hit-test.
        DesktopEvent::MouseMove { x, y }
        | DesktopEvent::MouseDown { x, y, .. }
        | DesktopEvent::MouseUp { x, y, .. }
        | DesktopEvent::Scroll { x, y, .. } => {
            if let Some(window) = hit_test(*x, *y) {
                EventRoute::ToWindow(window)
            } else {
                EventRoute::Propagate
            }
        }

        // Window management events bypass routing (destined for the window manager).
        DesktopEvent::WindowClose { window_id }
        | DesktopEvent::WindowFocus { window_id }
        | DesktopEvent::WindowResize { window_id, .. }
        | DesktopEvent::WindowMove { window_id, .. } => {
            // These are intrinsic to their window; route directly.
            EventRoute::ToWindow(*window_id)
        }
    }
}

/// Determines if a key event should be delivered to the focused window.
///
/// Key events require explicit focus to be delivered (Mutter: stage_has_key_focus).
/// Returns true if delivery should proceed.
pub fn should_deliver_key_event(focused_window: Option<WindowId>) -> bool {
    focused_window.is_some()
}

/// Determines if an event should update user activity time.
///
/// Synthetic events, enter/leave, and idle-time events are excluded (Mutter: handle_idletime_for_event).
pub fn should_update_idle_time(event: &DesktopEvent) -> bool {
    match event {
        // Real user input resets idle time.
        DesktopEvent::MouseMove { .. }
        | DesktopEvent::MouseDown { .. }
        | DesktopEvent::MouseUp { .. }
        | DesktopEvent::KeyDown { .. }
        | DesktopEvent::KeyUp { .. }
        | DesktopEvent::Scroll { .. } => true,

        // Window events don't affect idle time.
        DesktopEvent::WindowClose { .. }
        | DesktopEvent::WindowFocus { .. }
        | DesktopEvent::WindowResize { .. }
        | DesktopEvent::WindowMove { .. } => false,
    }
}
