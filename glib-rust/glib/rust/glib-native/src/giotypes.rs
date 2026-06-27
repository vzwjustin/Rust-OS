//! GIoTypes matching `gio/giotypes.h`.
//! GIO type definitions. In this no_std port we re-export key types
//! from other modules and define type aliases.
//! Fully `no_std` compatible using `alloc`.

// Re-export key GIO types for convenience
pub use crate::gaction::Action;
pub use crate::gactiongroup::ActionGroup;
pub use crate::gappinfo::AppInfo;
pub use crate::gapplication::Application;
pub use crate::gcancellable::GCancellable;
pub use crate::gdbusconnection::DBusConnection;
pub use crate::gdbusproxy::DBusProxy;
pub use crate::gdrive::SimpleDrive;
pub use crate::gfile::File;
pub use crate::gfileinfo::FileInfo;
pub use crate::gicon::Icon;
pub use crate::ginputstream::InputStream;
pub use crate::giostream::IOStream;
pub use crate::gmenu::Menu;
pub use crate::gmenumodel::MenuModel;
pub use crate::gmount::SimpleMount;
pub use crate::gnotification::Notification;
pub use crate::goutputstream::OutputStream;
pub use crate::gresource::Resource;
pub use crate::gsettings::Settings;
pub use crate::gsocket::Socket;
pub use crate::gsocketconnection::SocketConnection;
pub use crate::gvfs::Vfs;
pub use crate::gvolume::SimpleVolume;
pub use crate::gvolumemonitor::VolumeMonitor;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_re_exports_compile() {
        let _ = GCancellable::new();
        let _ = crate::gvfs::LocalVfs::new();
    }
}
