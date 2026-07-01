//! Kms Update Private — ported from GNOME Mutter
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-kms-update-private.h









use crate::mutter_port::backends::common_types::*;
/// gatomicrefcount — atomic reference count (using u32)
pub type gatomicrefcount = u32;

/// CoglPixelFormat — pixel format codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum CoglPixelFormat {
    Rgba = 0,
    Rgb = 1,
}

use alloc::string::String;

/// MetaKmsFeedback
#[derive(Debug, Clone)]
pub struct MetaKmsFeedback {
    pub ref_count: u32,
    pub result: MetaKmsFeedbackResult,
    pub ready_time_us: i32,
}

impl MetaKmsFeedback {
    /// TODO: port logic from meta_kms_plane_feedback_free
    pub fn kms_plane_feedback_free(&self) {
        todo!()
    }

    /// TODO: port logic from meta_kms_plane_feedback_new_take_error
    pub fn kms_plane_feedback_new_take_error(&self) {
        todo!()
    }

}

/// MetaKmsFbDamage
#[derive(Debug, Clone)]
pub struct MetaKmsFbDamage {
    pub n_rects: i32,
}

impl MetaKmsFbDamage {
    /// TODO: port logic from meta_kms_plane_feedback_free
    pub fn kms_plane_feedback_free(&self) {
        todo!()
    }

    /// TODO: port logic from meta_kms_plane_feedback_new_take_error
    pub fn kms_plane_feedback_new_take_error(&self) {
        todo!()
    }

}

/// MetaKmsModeSet
#[derive(Debug, Clone)]
pub struct MetaKmsModeSet {
    // TODO: Add fields from C struct
}

impl MetaKmsModeSet {
    /// TODO: port logic from meta_kms_plane_feedback_free
    pub fn kms_plane_feedback_free(&self) {
        todo!()
    }

    /// TODO: port logic from meta_kms_plane_feedback_new_take_error
    pub fn kms_plane_feedback_new_take_error(&self) {
        todo!()
    }

}
