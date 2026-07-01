//! Port of GNOME mutter's `clutter/clutter-input-focus.{c,h}`.
//!
//! `ClutterInputFocus` is the abstract base class for IME/text-input focus
//! management: it receives preedit/commit/delete events from input methods
//! and exposes properties (surrounding text, cursor location) to IMEs.
//! Subclasses implement concrete text widgets.
//!
//! # What's ported
//!
//! - The `ClutterInputFocusClass` vtable as an `InputFocus` trait with
//!   eight virtuals: `focus_in`, `focus_out`, `request_surrounding`,
//!   `delete_surrounding`, `commit_text`, `set_preedit_text`, and `action`.
//!   Default implementations are no-op (matching the C null-vtable guards).
//! - `PreeditStyleHint` enum and `PreeditAttribute` struct (preedit styling).
//! - `PreeditResetMode` enum (preedit reset behavior on commit).
//! - Wrapper functions dispatching through the trait (`focus_in`, `focus_out`,
//!   `request_surrounding`, etc.).
//!
//! # What's skipped, with rationale
//!
//! - GObject machinery and `ClutterInputMethod` storage: the C version
//!   holds `priv->im` and calls input-method functions; the trait is
//!   input-method agnostic. A future InputMethod port can extend this.
//! - `is_focused`, `reset`, `set_surrounding`, `set_content_hints`,
//!   `set_content_purpose`, `filter_event`, `process_event`,
//!   `set_can_show_preedit`, `set_input_panel_state`, `set_handled_actions`,
//!   `trigger_action`: these delegate to InputMethod or manage internal
//!   state (preedit, surrounding text). Implementers can add them as needed.
//!
//! As with the rest of `mutter_port::clutter`, this module uses no
//! `unsafe`, no external crates, and only `core`/`alloc`.

use alloc::string::String;

/// `ClutterPreeditStyleHint` (clutter-enums.h). Style hint for a preedit span.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum PreeditStyleHint {
    /// `CLUTTER_PREEDIT_STYLE_NONE`: no styling.
    None = 0,
    /// `CLUTTER_PREEDIT_STYLE_DEFAULT`: default styling.
    Default = 1,
    /// `CLUTTER_PREEDIT_STYLE_HIGHLIGHT`: highlight.
    Highlight = 2,
    /// `CLUTTER_PREEDIT_STYLE_UNDERLINE`: underline.
    Underline = 3,
    /// `CLUTTER_PREEDIT_STYLE_ACTIVE`: active/cursor underline.
    Active = 4,
}

/// `ClutterPreeditAttribute`: a span of preedit text with a style hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreeditAttribute {
    /// Style hint for this span.
    pub hint: PreeditStyleHint,
    /// Start index in characters.
    pub start: u32,
    /// End index in characters.
    pub end: u32,
}

/// `ClutterPreeditResetMode`: how to handle preedit on reset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum PreeditResetMode {
    /// `CLUTTER_PREEDIT_RESET_CLEAR`: clear preedit on reset.
    Clear = 0,
    /// `CLUTTER_PREEDIT_RESET_COMMIT`: commit preedit on reset.
    Commit = 1,
}

/// `ClutterInputAction` (clutter-enums.h): input-method action (Enter, etc).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum InputAction {
    /// `CLUTTER_INPUT_ACTION_COMMIT`: commit the preedit.
    Commit = 0,
    /// `CLUTTER_INPUT_ACTION_DELETE`: delete surrounding.
    Delete = 1,
    /// `CLUTTER_INPUT_ACTION_SWITCH_GROUP`: switch input group.
    SwitchGroup = 2,
}

/// Port of `ClutterInputFocusClass` vtable. Implement this per text widget.
pub trait InputFocus {
    /// `ClutterInputFocusClass::focus_in`: gained focus, input method set.
    fn focus_in(&mut self) {}

    /// `ClutterInputFocusClass::focus_out`: lost focus, input method cleared.
    fn focus_out(&mut self) {}

    /// `ClutterInputFocusClass::request_surrounding`: request surrounding
    /// text from the input method.
    fn request_surrounding(&mut self) {}

    /// `ClutterInputFocusClass::delete_surrounding`: delete text relative
    /// to cursor (offset in chars, len in chars).
    fn delete_surrounding(&mut self, _offset: i32, _len: u32) {}

    /// `ClutterInputFocusClass::commit_text`: commit text from IME.
    fn commit_text(&mut self, _text: &str) {}

    /// `ClutterInputFocusClass::set_preedit_text`: set preedit with cursor
    /// position and style hints.
    fn set_preedit_text(
        &mut self,
        _preedit: Option<&str>,
        _cursor: u32,
        _anchor: u32,
        _style_hints: &[PreeditAttribute],
    ) {
    }

    /// `ClutterInputFocusClass::action`: trigger an input action.
    fn action(&mut self, _action: InputAction) {}
}

// ---- wrapper functions matching the C `clutter_input_focus_*` API ----

/// `clutter_input_focus_focus_in`.
pub fn focus_in<F: InputFocus + ?Sized>(focus: &mut F) {
    focus.focus_in();
}

/// `clutter_input_focus_focus_out`.
pub fn focus_out<F: InputFocus + ?Sized>(focus: &mut F) {
    focus.focus_out();
}

/// `clutter_input_focus_request_surrounding`.
pub fn request_surrounding<F: InputFocus + ?Sized>(focus: &mut F) {
    focus.request_surrounding();
}

/// `clutter_input_focus_delete_surrounding`.
pub fn delete_surrounding<F: InputFocus + ?Sized>(focus: &mut F, offset: i32, len: u32) {
    focus.delete_surrounding(offset, len);
}

/// `clutter_input_focus_commit_text`.
pub fn commit_text<F: InputFocus + ?Sized>(focus: &mut F, text: &str) {
    focus.commit_text(text);
}

/// `clutter_input_focus_set_preedit_text`.
pub fn set_preedit_text<F: InputFocus + ?Sized>(
    focus: &mut F,
    preedit: Option<&str>,
    cursor: u32,
    anchor: u32,
    style_hints: &[PreeditAttribute],
) {
    focus.set_preedit_text(preedit, cursor, anchor, style_hints);
}

/// `clutter_input_focus_trigger_action`.
pub fn trigger_action<F: InputFocus + ?Sized>(focus: &mut F, action: InputAction) {
    focus.action(action);
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestFocus {
        preedit: Option<String>,
    }

    impl InputFocus for TestFocus {
        fn commit_text(&mut self, text: &str) {
            self.preedit = Some(text.into());
        }
        fn set_preedit_text(
            &mut self,
            preedit: Option<&str>,
            _cursor: u32,
            _anchor: u32,
            _style_hints: &[PreeditAttribute],
        ) {
            self.preedit = preedit.map(|s| s.into());
        }
    }

    #[test]
    fn commit_text_sets_preedit() {
        let mut f = TestFocus { preedit: None };
        commit_text(&mut f, "hello");
        assert_eq!(f.preedit.as_deref(), Some("hello"));
    }

    #[test]
    fn set_preedit_text_with_none_clears() {
        let mut f = TestFocus {
            preedit: Some("hello".into()),
        };
        set_preedit_text(&mut f, None, 0, 0, &[]);
        assert_eq!(f.preedit, None);
    }

    #[test]
    fn preedit_attribute_fields() {
        let attr = PreeditAttribute {
            hint: PreeditStyleHint::Underline,
            start: 0,
            end: 5,
        };
        assert_eq!(attr.hint, PreeditStyleHint::Underline);
        assert_eq!(attr.start, 0);
        assert_eq!(attr.end, 5);
    }

    #[test]
    fn default_methods_are_noop() {
        struct Bare;
        impl InputFocus for Bare {}
        let mut f = Bare;
        focus_in(&mut f);
        focus_out(&mut f);
        request_surrounding(&mut f);
        delete_surrounding(&mut f, -1, 5);
        commit_text(&mut f, "text");
        set_preedit_text(&mut f, Some("pre"), 0, 1, &[]);
        trigger_action(&mut f, InputAction::Commit);
    }
}
