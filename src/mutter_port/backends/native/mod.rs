//! Native backend module for Mutter on RustOS.
//!
//! This module contains a mechanical port of GNOME Mutter's native backend (DRM/KMS/GBM-based
//! rendering and input) from C/GObject to Rust. It provides hardware abstraction for display
//! output, graphics device management, and input event handling.
//!
//! The native backend is used when running on bare metal or in environments without X11.
//! It interfaces directly with the Linux kernel's DRM (Direct Rendering Manager) and KMS
//! (Kernel Mode Setting) subsystems for hardware access.
//!
//! ## Module Organization
//!
//! ### CRTC Management (Cathode Ray Tube Controllers)
//! - `crtc_native` - Abstract base for hardware CRTCs
//! - `crtc_virtual` - Virtual CRTC for headless/nested modes
//! - `crtc_kms` - KMS-based hardware CRTC
//! - `crtc_mode_*` - Display mode representations
//!
//! ### Display Output Management
//! - `output_native` - Abstract base for outputs (connectors)
//! - `output_virtual` - Virtual outputs for nested environments
//! - `output_kms` - KMS-based hardware outputs
//!
//! ### GPU/Device Management
//! - `gpu_kms` - GPU device representation
//! - `kms_device` - KMS device wrapper
//! - `kms` - Main KMS subsystem manager
//!
//! ### KMS Primitives and Utilities
//! - `kms_utils` - Display mode calculations (refresh rate, vblank)
//! - `kms_mode` - Display mode object
//! - `kms_plane` - Display plane (primary, overlay, cursor)
//! - `kms_connector` - Display connector (HDMI, DP, etc.)
//! - `kms_crtc` - Low-level CRTC object
//!
//! ### Display Pipeline Management
//! - `kms_update` - Atomic update batching
//! - `kms_page_flip` - Page flip event handling
//! - `kms_cursor_manager` - Hardware cursor management
//!
//! ### Graphics Buffer Management
//! - `drm_buffer` - DRM framebuffer abstraction
//! - `drm_lease` - DRM lease support
//!
//! ### Top-Level Backend
//! - `backend_native` - Main native backend interface
//!
//! ## Implementation Notes
//!
//! This is a **structural port**, not a complete behavioral replication:
//! - GObject ref-counting becomes simple Rust ownership and borrowing
//! - Virtual methods become trait methods or explicit function pointers
//! - Property systems become simple fields with accessor methods
//! - Hardware-specific operations (atomic KMS commits, DRM ioctls) are
//!   documented at each call site with the kernel API they would invoke;
//!   local state is tracked so the data-structure lifecycle ports faithfully
//! - Math operations use `libm::` functions where needed for `no_std` compatibility
//!
//! ## KMS/DRM Integration Points
//!
//! The following DRM ioctls and operations are referenced but require actual implementation:
//! - `DRM_IOCTL_MODE_GETRESOURCES` - Query available CRTCs/connectors/planes
//! - `DRM_IOCTL_ATOMIC_COMMIT` - Atomic display mode changes
//! - `drmModeGetProperty()` / `drmModeGetPropertyBlob()` - Read DRM properties (EDID, etc.)
//! - `drmModeAtomicAddProperty()` - Build atomic requests
//! - Various plane/cursor/connector property management operations

pub mod crtc_kms;
pub mod crtc_mode_kms;
pub mod crtc_mode_virtual;
pub mod crtc_native;
pub mod crtc_virtual;
pub mod onscreen_native;
pub mod renderer_native;

pub mod output_kms;
pub mod output_native;
pub mod output_virtual;

pub mod gpu_kms;

pub mod kms_connector;
pub mod kms_crtc;
pub mod kms_device;
pub mod kms_mode;
pub mod kms_plane;
pub mod kms_utils;

pub mod kms_cursor_manager;
pub mod kms_page_flip;
pub mod kms_update;

pub mod drm_buffer;
pub mod drm_lease;

pub mod kms;

pub mod backend_native;
pub mod monitor_manager_native;

pub mod bezier;
pub mod device_pool;

// Re-export commonly used types at module level
pub use backend_native::BackendNative;
pub use crtc_kms::CrtcKms;
pub use crtc_native::CrtcNative;
pub use device_pool::DevicePool;
pub use gpu_kms::GpuKms;
pub use kms::Kms;
pub use kms_device::KmsDevice;
pub use monitor_manager_native::MonitorManagerNative;
pub use output_kms::OutputKms;
pub use output_native::OutputNative;

// --- Wired-in ported modules (previously undeclared) ---
pub mod barrier_native;
pub mod clutter_backend_native;
pub mod cursor_renderer_native;
pub mod drm_buffer_dumb;
pub mod drm_buffer_gbm;
pub mod drm_buffer_import;
pub mod egl_gbm;
pub mod frame_native;
pub mod input_device_native;
pub mod input_device_tool_native;
pub mod input_settings_native;
pub mod keyboard_a11y;
pub mod keymap_native;
pub mod kms_connector_private;
pub mod kms_crtc_private;
pub mod kms_impl;
pub mod kms_impl_device;
pub mod kms_impl_device_atomic;
pub mod kms_impl_device_dummy;
pub mod kms_impl_device_simple;
pub mod kms_mode_private;
pub mod kms_page_flip_private;
pub mod kms_plane_private;
pub mod kms_private;
pub mod kms_types;
pub mod kms_types_private;
pub mod kms_update_private;
pub mod pointer_constraint_native;
pub mod render_device;
pub mod render_device_gbm;
pub mod render_device_surfaceless;
pub mod renderer_context_egl;
pub mod renderer_display_egl;
pub mod renderer_egl;
pub mod renderer_native_gles3;
pub mod renderer_view_native;
pub mod seat_impl;
pub mod seat_native;
pub mod sprite_native;
pub mod stage_native;
pub mod thread;
pub mod thread_impl;
pub mod virtual_input_device_native;
pub mod virtual_monitor_native;
pub mod xkb_utils;
