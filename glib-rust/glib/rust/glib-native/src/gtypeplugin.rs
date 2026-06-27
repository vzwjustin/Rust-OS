//! `gtypeplugin.c` compatibility facade.

use crate::gtype::{GType, GTypeInfo, InterfaceInfo};

/// Minimal type-plugin trait used by dynamic type registration facades.
pub trait TypePlugin {
    /// Called when the plugin is put in use.
    fn use_plugin(&self) {}

    /// Called when the plugin is no longer used.
    fn unuse_plugin(&self) {}

    /// Complete type information for `type_id`.
    fn complete_type_info(&self, _type_id: GType) -> Option<GTypeInfo> {
        None
    }

    /// Complete interface information for an instance/interface pair.
    fn complete_interface_info(
        &self,
        _instance_type: GType,
        _interface_type: GType,
    ) -> Option<InterfaceInfo> {
        None
    }
}

/// Adapter used when a plugin only needs use/unuse notifications.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoopTypePlugin;

impl TypePlugin for NoopTypePlugin {}
