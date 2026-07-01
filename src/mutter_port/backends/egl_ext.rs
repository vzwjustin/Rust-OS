//! EGL Extensions ported from GNOME Mutter's src/backends/
//!
//! Defines EGL extension constants for Wayland buffer support, including
//! target IDs for eglCreateImageKHR and texture format specifiers. These
//! are fallback definitions for EGL implementations that don't fully
//! advertise the extensions.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-egl-ext.h

/// Wayland buffer target for eglCreateImageKHR.
pub const EGL_WAYLAND_BUFFER_WL: u32 = 0x31D5;

/// Wayland plane target for eglCreateImageKHR.
pub const EGL_WAYLAND_PLANE_WL: u32 = 0x31D6;

/// Texture Y, U, V format for Wayland.
pub const EGL_TEXTURE_Y_U_V_WL: u32 = 0x31D7;

/// Texture Y, UV format for Wayland.
pub const EGL_TEXTURE_Y_UV_WL: u32 = 0x31D8;

/// Texture Y, XUXV format for Wayland.
pub const EGL_TEXTURE_Y_XUXV_WL: u32 = 0x31D9;

/// External texture format for Wayland.
pub const EGL_TEXTURE_EXTERNAL_WL: u32 = 0x31DA;
