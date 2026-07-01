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
//! - Complex hardware-specific logic (atomic KMS commits, DRM ioctls) is stubbed with TODO comments
//!   indicating what kernel calls are needed
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
pub mod onscreen_native;
pub mod renderer_native;
pub mod crtc_native;
pub mod crtc_virtual;

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
