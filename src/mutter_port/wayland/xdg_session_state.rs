//! xdg-session-management state: lets a client tag an xdg_toplevel with a
//! restore token so its position/size can be recalled across restarts.

use alloc::string::String;

#[derive(Debug, Clone, Default)]
pub struct XdgSessionState {
    pub session_id: Option<String>,
    pub restore_token: Option<String>,
}

impl XdgSessionState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_session_id(&mut self, session_id: String) {
        self.session_id = Some(session_id);
    }

    pub fn add_toplevel(&mut self, restore_token: String) {
        self.restore_token = Some(restore_token);
    }

    pub fn remove(&mut self) {
        self.session_id = None;
        self.restore_token = None;
    }
}
