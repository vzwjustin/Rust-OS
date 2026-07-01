//! XWayland drag-and-drop bridge: forwards an X11 client's drag-and-drop
//! selection to Wayland clients via a synthesized wl_data_source.

#[derive(Debug, Clone, Copy, Default)]
pub struct XWaylandDataSource {
    pub owner_window: Option<u32>,
    pub drag_active: bool,
}

impl XWaylandDataSource {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn begin(&mut self, owner_window: u32) {
        self.owner_window = Some(owner_window);
        self.drag_active = true;
    }

    pub fn end(&mut self) {
        self.owner_window = None;
        self.drag_active = false;
    }

    pub fn is_active(&self) -> bool {
        self.drag_active
    }
}
