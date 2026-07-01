//! Wayland surface role assigned to the icon a client drags around during
//! a drag-and-drop operation (the "drag icon" surface).

#[derive(Debug, Clone, Copy, Default)]
pub struct DndSurfaceRole {
    pub surface_id: Option<u32>,
    pub offset_x: i32,
    pub offset_y: i32,
}

impl DndSurfaceRole {
    pub fn new(surface_id: u32) -> Self {
        Self {
            surface_id: Some(surface_id),
            offset_x: 0,
            offset_y: 0,
        }
    }

    pub fn set_offset(&mut self, x: i32, y: i32) {
        self.offset_x = x;
        self.offset_y = y;
    }
}
