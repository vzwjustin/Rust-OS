//! Connector types ported from GNOME Mutter's src/backends/meta-connector.c
//!
//! Defines the DRM/KMS connector type enumeration and its human-readable names.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-connector.c

/// DRM/KMS connector type.
///
/// Values mirror the DRM connector type numbering used by Mutter
/// (see `drm_mode.h`), with `Meta` reserved for virtual/internal connectors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaConnectorType {
    Unknown = 0,
    Vga = 1,
    Dvii = 2,
    Dvid = 3,
    Dvia = 4,
    Composite = 5,
    SVideo = 6,
    Lvds = 7,
    Component = 8,
    NinePinDin = 9,
    DisplayPort = 10,
    HdmiA = 11,
    HdmiB = 12,
    Tv = 13,
    Edp = 14,
    Virtual = 15,
    Dsi = 16,
    Dpi = 17,
    Writeback = 18,
    Spi = 19,
    Usb = 20,

    Meta = 1000,
}

impl MetaConnectorType {
    /// Return the human-readable name of the connector type.
    ///
    /// Returns `None` for unrecognized values (mirrors the C fall-through).
    pub fn get_name(self) -> Option<&'static str> {
        let name = match self {
            MetaConnectorType::Unknown => "None",
            MetaConnectorType::Vga => "VGA",
            MetaConnectorType::Dvii => "DVI-I",
            MetaConnectorType::Dvid => "DVI-D",
            MetaConnectorType::Dvia => "DVI-A",
            MetaConnectorType::Composite => "Composite",
            MetaConnectorType::SVideo => "SVIDEO",
            MetaConnectorType::Lvds => "LVDS",
            MetaConnectorType::Component => "Component",
            MetaConnectorType::NinePinDin => "DIN",
            MetaConnectorType::DisplayPort => "DP",
            MetaConnectorType::HdmiA => "HDMI",
            MetaConnectorType::HdmiB => "HDMI-B",
            MetaConnectorType::Tv => "TV",
            MetaConnectorType::Edp => "eDP",
            MetaConnectorType::Virtual => "Virtual",
            MetaConnectorType::Dsi => "DSI",
            MetaConnectorType::Dpi => "DPI",
            MetaConnectorType::Writeback => "WRITEBACK",
            MetaConnectorType::Spi => "SPI",
            MetaConnectorType::Usb => "USB",
            MetaConnectorType::Meta => "Meta",
        };
        Some(name)
    }
}
