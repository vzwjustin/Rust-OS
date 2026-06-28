// Generated from upstream wayland/protocol/wayland.xml
// Source commit: 165504a90edd7d6e51dd42d11f9dd0e8c9384609
// Keep this no_std: it is protocol metadata, not libwayland C runtime.

use super::ArgType;

pub const WL_DISPLAY: &str = "wl_display";
pub const WL_DISPLAY_VERSION: u32 = 1;
pub const WL_REGISTRY: &str = "wl_registry";
pub const WL_REGISTRY_VERSION: u32 = 1;
pub const WL_CALLBACK: &str = "wl_callback";
pub const WL_CALLBACK_VERSION: u32 = 1;
pub const WL_COMPOSITOR: &str = "wl_compositor";
pub const WL_COMPOSITOR_VERSION: u32 = 7;
pub const WL_SHM_POOL: &str = "wl_shm_pool";
pub const WL_SHM_POOL_VERSION: u32 = 2;
pub const WL_SHM: &str = "wl_shm";
pub const WL_SHM_VERSION: u32 = 2;
pub const WL_BUFFER: &str = "wl_buffer";
pub const WL_BUFFER_VERSION: u32 = 1;
pub const WL_DATA_OFFER: &str = "wl_data_offer";
pub const WL_DATA_OFFER_VERSION: u32 = 4;
pub const WL_DATA_SOURCE: &str = "wl_data_source";
pub const WL_DATA_SOURCE_VERSION: u32 = 4;
pub const WL_DATA_DEVICE: &str = "wl_data_device";
pub const WL_DATA_DEVICE_VERSION: u32 = 4;
pub const WL_DATA_DEVICE_MANAGER: &str = "wl_data_device_manager";
pub const WL_DATA_DEVICE_MANAGER_VERSION: u32 = 4;
pub const WL_SHELL: &str = "wl_shell";
pub const WL_SHELL_VERSION: u32 = 1;
pub const WL_SHELL_SURFACE: &str = "wl_shell_surface";
pub const WL_SHELL_SURFACE_VERSION: u32 = 1;
pub const WL_SURFACE: &str = "wl_surface";
pub const WL_SURFACE_VERSION: u32 = 7;
pub const WL_SEAT: &str = "wl_seat";
pub const WL_SEAT_VERSION: u32 = 11;
pub const WL_POINTER: &str = "wl_pointer";
pub const WL_POINTER_VERSION: u32 = 11;
pub const WL_KEYBOARD: &str = "wl_keyboard";
pub const WL_KEYBOARD_VERSION: u32 = 11;
pub const WL_TOUCH: &str = "wl_touch";
pub const WL_TOUCH_VERSION: u32 = 11;
pub const WL_OUTPUT: &str = "wl_output";
pub const WL_OUTPUT_VERSION: u32 = 4;
pub const WL_REGION: &str = "wl_region";
pub const WL_REGION_VERSION: u32 = 7;
pub const WL_SUBCOMPOSITOR: &str = "wl_subcompositor";
pub const WL_SUBCOMPOSITOR_VERSION: u32 = 1;
pub const WL_SUBSURFACE: &str = "wl_subsurface";
pub const WL_SUBSURFACE_VERSION: u32 = 1;
pub const WL_FIXES: &str = "wl_fixes";
pub const WL_FIXES_VERSION: u32 = 2;

pub fn request_arg_types(interface: &str, opcode: u16) -> Option<&'static [ArgType]> {
    match (interface, opcode) {
        (WL_DISPLAY, 0) => Some(&[ArgType::NewId]), // wl_display.sync
        (WL_DISPLAY, 1) => Some(&[ArgType::NewId]), // wl_display.get_registry
        (WL_REGISTRY, 0) => Some(&[
            ArgType::UInt,
            ArgType::String,
            ArgType::UInt,
            ArgType::NewId,
        ]), // wl_registry.bind
        (WL_COMPOSITOR, 0) => Some(&[ArgType::NewId]), // wl_compositor.create_surface
        (WL_COMPOSITOR, 1) => Some(&[ArgType::NewId]), // wl_compositor.create_region
        (WL_COMPOSITOR, 2) => Some(&[]),            // wl_compositor.release
        (WL_SHM_POOL, 0) => Some(&[
            ArgType::NewId,
            ArgType::Int,
            ArgType::Int,
            ArgType::Int,
            ArgType::Int,
            ArgType::UInt,
        ]), // wl_shm_pool.create_buffer
        (WL_SHM_POOL, 1) => Some(&[]),              // wl_shm_pool.destroy
        (WL_SHM_POOL, 2) => Some(&[ArgType::Int]),  // wl_shm_pool.resize
        (WL_SHM, 0) => Some(&[ArgType::NewId, ArgType::Fd, ArgType::Int]), // wl_shm.create_pool
        (WL_SHM, 1) => Some(&[]),                   // wl_shm.release
        (WL_BUFFER, 0) => Some(&[]),                // wl_buffer.destroy
        (WL_DATA_OFFER, 0) => Some(&[ArgType::UInt, ArgType::String]), // wl_data_offer.accept
        (WL_DATA_OFFER, 1) => Some(&[ArgType::String, ArgType::Fd]), // wl_data_offer.receive
        (WL_DATA_OFFER, 2) => Some(&[]),            // wl_data_offer.destroy
        (WL_DATA_OFFER, 3) => Some(&[]),            // wl_data_offer.finish
        (WL_DATA_OFFER, 4) => Some(&[ArgType::UInt, ArgType::UInt]), // wl_data_offer.set_actions
        (WL_DATA_SOURCE, 0) => Some(&[ArgType::String]), // wl_data_source.offer
        (WL_DATA_SOURCE, 1) => Some(&[]),           // wl_data_source.destroy
        (WL_DATA_SOURCE, 2) => Some(&[ArgType::UInt]), // wl_data_source.set_actions
        (WL_DATA_DEVICE, 0) => Some(&[
            ArgType::Object,
            ArgType::Object,
            ArgType::Object,
            ArgType::UInt,
        ]), // wl_data_device.start_drag
        (WL_DATA_DEVICE, 1) => Some(&[ArgType::Object, ArgType::UInt]), // wl_data_device.set_selection
        (WL_DATA_DEVICE, 2) => Some(&[]),                               // wl_data_device.release
        (WL_DATA_DEVICE_MANAGER, 0) => Some(&[ArgType::NewId]), // wl_data_device_manager.create_data_source
        (WL_DATA_DEVICE_MANAGER, 1) => Some(&[ArgType::NewId, ArgType::Object]), // wl_data_device_manager.get_data_device
        (WL_DATA_DEVICE_MANAGER, 2) => Some(&[]), // wl_data_device_manager.release
        (WL_SHELL, 0) => Some(&[ArgType::NewId, ArgType::Object]), // wl_shell.get_shell_surface
        (WL_SHELL_SURFACE, 0) => Some(&[ArgType::UInt]), // wl_shell_surface.pong
        (WL_SHELL_SURFACE, 1) => Some(&[ArgType::Object, ArgType::UInt]), // wl_shell_surface.move
        (WL_SHELL_SURFACE, 2) => Some(&[ArgType::Object, ArgType::UInt, ArgType::UInt]), // wl_shell_surface.resize
        (WL_SHELL_SURFACE, 3) => Some(&[]), // wl_shell_surface.set_toplevel
        (WL_SHELL_SURFACE, 4) => {
            Some(&[ArgType::Object, ArgType::Int, ArgType::Int, ArgType::UInt])
        } // wl_shell_surface.set_transient
        (WL_SHELL_SURFACE, 5) => Some(&[ArgType::UInt, ArgType::UInt, ArgType::Object]), // wl_shell_surface.set_fullscreen
        (WL_SHELL_SURFACE, 6) => Some(&[
            ArgType::Object,
            ArgType::UInt,
            ArgType::Object,
            ArgType::Int,
            ArgType::Int,
            ArgType::UInt,
        ]), // wl_shell_surface.set_popup
        (WL_SHELL_SURFACE, 7) => Some(&[ArgType::Object]), // wl_shell_surface.set_maximized
        (WL_SHELL_SURFACE, 8) => Some(&[ArgType::String]), // wl_shell_surface.set_title
        (WL_SHELL_SURFACE, 9) => Some(&[ArgType::String]), // wl_shell_surface.set_class
        (WL_SURFACE, 0) => Some(&[]),                      // wl_surface.destroy
        (WL_SURFACE, 1) => Some(&[ArgType::Object, ArgType::Int, ArgType::Int]), // wl_surface.attach
        (WL_SURFACE, 2) => Some(&[ArgType::Int, ArgType::Int, ArgType::Int, ArgType::Int]), // wl_surface.damage
        (WL_SURFACE, 3) => Some(&[ArgType::NewId]), // wl_surface.frame
        (WL_SURFACE, 4) => Some(&[ArgType::Object]), // wl_surface.set_opaque_region
        (WL_SURFACE, 5) => Some(&[ArgType::Object]), // wl_surface.set_input_region
        (WL_SURFACE, 6) => Some(&[]),               // wl_surface.commit
        (WL_SURFACE, 7) => Some(&[ArgType::Int]),   // wl_surface.set_buffer_transform
        (WL_SURFACE, 8) => Some(&[ArgType::Int]),   // wl_surface.set_buffer_scale
        (WL_SURFACE, 9) => Some(&[ArgType::Int, ArgType::Int, ArgType::Int, ArgType::Int]), // wl_surface.damage_buffer
        (WL_SURFACE, 10) => Some(&[ArgType::Int, ArgType::Int]), // wl_surface.offset
        (WL_SURFACE, 11) => Some(&[ArgType::NewId]),             // wl_surface.get_release
        (WL_SEAT, 0) => Some(&[ArgType::NewId]),                 // wl_seat.get_pointer
        (WL_SEAT, 1) => Some(&[ArgType::NewId]),                 // wl_seat.get_keyboard
        (WL_SEAT, 2) => Some(&[ArgType::NewId]),                 // wl_seat.get_touch
        (WL_SEAT, 3) => Some(&[]),                               // wl_seat.release
        (WL_POINTER, 0) => Some(&[ArgType::UInt, ArgType::Object, ArgType::Int, ArgType::Int]), // wl_pointer.set_cursor
        (WL_POINTER, 1) => Some(&[]),  // wl_pointer.release
        (WL_KEYBOARD, 0) => Some(&[]), // wl_keyboard.release
        (WL_TOUCH, 0) => Some(&[]),    // wl_touch.release
        (WL_OUTPUT, 0) => Some(&[]),   // wl_output.release
        (WL_REGION, 0) => Some(&[]),   // wl_region.destroy
        (WL_REGION, 1) => Some(&[ArgType::Int, ArgType::Int, ArgType::Int, ArgType::Int]), // wl_region.add
        (WL_REGION, 2) => Some(&[ArgType::Int, ArgType::Int, ArgType::Int, ArgType::Int]), // wl_region.subtract
        (WL_SUBCOMPOSITOR, 0) => Some(&[]), // wl_subcompositor.destroy
        (WL_SUBCOMPOSITOR, 1) => Some(&[ArgType::NewId, ArgType::Object, ArgType::Object]), // wl_subcompositor.get_subsurface
        (WL_SUBSURFACE, 0) => Some(&[]), // wl_subsurface.destroy
        (WL_SUBSURFACE, 1) => Some(&[ArgType::Int, ArgType::Int]), // wl_subsurface.set_position
        (WL_SUBSURFACE, 2) => Some(&[ArgType::Object]), // wl_subsurface.place_above
        (WL_SUBSURFACE, 3) => Some(&[ArgType::Object]), // wl_subsurface.place_below
        (WL_SUBSURFACE, 4) => Some(&[]), // wl_subsurface.set_sync
        (WL_SUBSURFACE, 5) => Some(&[]), // wl_subsurface.set_desync
        (WL_FIXES, 0) => Some(&[]),      // wl_fixes.destroy
        (WL_FIXES, 1) => Some(&[ArgType::Object]), // wl_fixes.destroy_registry
        (WL_FIXES, 2) => Some(&[ArgType::Object, ArgType::UInt]), // wl_fixes.ack_global_remove
        _ => None,
    }
}
