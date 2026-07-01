//! XKB keyboard layout info — ported from gnome-xkb-info.c
//!
//! Provides information about XKB keyboard layouts and option groups.
//! The upstream parses XKB rule files from the filesystem using xkbcommon.
//! We embed a static table of common layouts for no_std use.

use alloc::string::String;
use alloc::vec::Vec;

/// A keyboard layout descriptor.
#[derive(Debug, Clone, Copy)]
pub struct Layout {
    /// Unique layout ID (e.g. "us", "de+nodeadkeys").
    pub id: &'static str,
    /// XKB layout name (e.g. "us", "de").
    pub xkb_layout: &'static str,
    /// XKB variant (e.g. "nodeadkeys"), or empty if none.
    pub xkb_variant: &'static str,
    /// Short description (e.g. "US", "DE").
    pub short_name: &'static str,
    /// Full display name (e.g. "English (US)").
    pub display_name: &'static str,
    /// Whether this is a variant of a main layout.
    pub is_variant: bool,
    /// ISO 639 language codes associated with this layout.
    pub iso639_ids: &'static [&'static str],
    /// ISO 3166 country codes associated with this layout.
    pub iso3166_ids: &'static [&'static str],
}

/// An XKB option group (e.g. "ctrl", "caps", "alt").
#[derive(Debug, Clone, Copy)]
pub struct OptionGroup {
    pub id: &'static str,
    pub description: &'static str,
    pub allow_multiple: bool,
    pub options: &'static [(&'static str, &'static str)],
}

/// XKB info — provides layout and option lookups.
pub struct XkbInfo;

impl XkbInfo {
    /// Create a new XkbInfo instance.
    pub fn new() -> Self {
        Self
    }

    /// Get all known layouts.
    pub fn get_all_layouts(&self) -> Vec<&'static str> {
        LAYOUTS.iter().map(|l| l.id).collect()
    }

    /// Get info for a specific layout by ID.
    /// Matches `gnome_xkb_info_get_layout_info()`.
    pub fn get_layout_info(&self, id: &str) -> Option<&'static Layout> {
        LAYOUTS.iter().find(|l| l.id == id)
    }

    /// Get all option groups.
    pub fn get_all_option_groups(&self) -> Vec<&'static OptionGroup> {
        OPTION_GROUPS.iter().collect()
    }

    /// Get the description for an option group.
    pub fn description_for_group(&self, group_id: &str) -> Option<&'static str> {
        OPTION_GROUPS
            .iter()
            .find(|g| g.id == group_id)
            .map(|g| g.description)
    }

    /// Get options for a given group.
    pub fn get_options_for_group(
        &self,
        group_id: &str,
    ) -> Option<&'static [(&'static str, &'static str)]> {
        OPTION_GROUPS
            .iter()
            .find(|g| g.id == group_id)
            .map(|g| g.options)
    }

    /// Get the description for a specific option within a group.
    pub fn description_for_option(&self, group_id: &str, option_id: &str) -> Option<&'static str> {
        self.get_options_for_group(group_id)?
            .iter()
            .find(|(id, _)| *id == option_id)
            .map(|(_, desc)| *desc)
    }

    /// Get layouts for a given ISO 639 language code.
    pub fn get_layouts_for_language(&self, language_code: &str) -> Vec<&'static Layout> {
        LAYOUTS
            .iter()
            .filter(|l| l.iso639_ids.iter().any(|c| *c == language_code))
            .collect()
    }

    /// Get layouts for a given ISO 3166 country code.
    pub fn get_layouts_for_country(&self, country_code: &str) -> Vec<&'static Layout> {
        LAYOUTS
            .iter()
            .filter(|l| l.iso3166_ids.iter().any(|c| *c == country_code))
            .collect()
    }

    /// Get language codes for a given layout ID.
    pub fn get_languages_for_layout(&self, layout_id: &str) -> Vec<&'static str> {
        self.get_layout_info(layout_id)
            .map(|l| l.iso639_ids.iter().copied().collect())
            .unwrap_or_default()
    }
}

impl Default for XkbInfo {
    fn default() -> Self {
        Self::new()
    }
}

/// Static table of common XKB layouts.
static LAYOUTS: &[Layout] = &[
    Layout {
        id: "us",
        xkb_layout: "us",
        xkb_variant: "",
        short_name: "US",
        display_name: "English (US)",
        is_variant: false,
        iso639_ids: &["en"],
        iso3166_ids: &["US"],
    },
    Layout {
        id: "us+intl",
        xkb_layout: "us",
        xkb_variant: "intl",
        short_name: "US",
        display_name: "English (US, intl.)",
        is_variant: true,
        iso639_ids: &["en"],
        iso3166_ids: &["US"],
    },
    Layout {
        id: "us+dvorak",
        xkb_layout: "us",
        xkb_variant: "dvorak",
        short_name: "DV",
        display_name: "English (Dvorak)",
        is_variant: true,
        iso639_ids: &["en"],
        iso3166_ids: &["US"],
    },
    Layout {
        id: "gb",
        xkb_layout: "gb",
        xkb_variant: "",
        short_name: "GB",
        display_name: "English (UK)",
        is_variant: false,
        iso639_ids: &["en"],
        iso3166_ids: &["GB"],
    },
    Layout {
        id: "de",
        xkb_layout: "de",
        xkb_variant: "",
        short_name: "DE",
        display_name: "German",
        is_variant: false,
        iso639_ids: &["de"],
        iso3166_ids: &["DE", "AT", "CH"],
    },
    Layout {
        id: "de+nodeadkeys",
        xkb_layout: "de",
        xkb_variant: "nodeadkeys",
        short_name: "DE",
        display_name: "German (no dead keys)",
        is_variant: true,
        iso639_ids: &["de"],
        iso3166_ids: &["DE"],
    },
    Layout {
        id: "fr",
        xkb_layout: "fr",
        xkb_variant: "",
        short_name: "FR",
        display_name: "French",
        is_variant: false,
        iso639_ids: &["fr"],
        iso3166_ids: &["FR"],
    },
    Layout {
        id: "es",
        xkb_layout: "es",
        xkb_variant: "",
        short_name: "ES",
        display_name: "Spanish",
        is_variant: false,
        iso639_ids: &["es"],
        iso3166_ids: &["ES"],
    },
    Layout {
        id: "it",
        xkb_layout: "it",
        xkb_variant: "",
        short_name: "IT",
        display_name: "Italian",
        is_variant: false,
        iso639_ids: &["it"],
        iso3166_ids: &["IT"],
    },
    Layout {
        id: "pt",
        xkb_layout: "pt",
        xkb_variant: "",
        short_name: "PT",
        display_name: "Portuguese",
        is_variant: false,
        iso639_ids: &["pt"],
        iso3166_ids: &["PT"],
    },
    Layout {
        id: "br",
        xkb_layout: "br",
        xkb_variant: "",
        short_name: "BR",
        display_name: "Portuguese (Brazil)",
        is_variant: false,
        iso639_ids: &["pt"],
        iso3166_ids: &["BR"],
    },
    Layout {
        id: "ru",
        xkb_layout: "ru",
        xkb_variant: "",
        short_name: "RU",
        display_name: "Russian",
        is_variant: false,
        iso639_ids: &["ru"],
        iso3166_ids: &["RU"],
    },
    Layout {
        id: "jp",
        xkb_layout: "jp",
        xkb_variant: "",
        short_name: "JP",
        display_name: "Japanese",
        is_variant: false,
        iso639_ids: &["ja"],
        iso3166_ids: &["JP"],
    },
    Layout {
        id: "cn",
        xkb_layout: "cn",
        xkb_variant: "",
        short_name: "CN",
        display_name: "Chinese",
        is_variant: false,
        iso639_ids: &["zh"],
        iso3166_ids: &["CN"],
    },
    Layout {
        id: "kr",
        xkb_layout: "kr",
        xkb_variant: "",
        short_name: "KR",
        display_name: "Korean",
        is_variant: false,
        iso639_ids: &["ko"],
        iso3166_ids: &["KR"],
    },
    Layout {
        id: "ara",
        xkb_layout: "ara",
        xkb_variant: "",
        short_name: "AR",
        display_name: "Arabic",
        is_variant: false,
        iso639_ids: &["ar"],
        iso3166_ids: &["SA", "EG", "MA"],
    },
    Layout {
        id: "tr",
        xkb_layout: "tr",
        xkb_variant: "",
        short_name: "TR",
        display_name: "Turkish",
        is_variant: false,
        iso639_ids: &["tr"],
        iso3166_ids: &["TR"],
    },
    Layout {
        id: "gr",
        xkb_layout: "gr",
        xkb_variant: "",
        short_name: "GR",
        display_name: "Greek",
        is_variant: false,
        iso639_ids: &["el"],
        iso3166_ids: &["GR"],
    },
    Layout {
        id: "il",
        xkb_layout: "il",
        xkb_variant: "",
        short_name: "IL",
        display_name: "Hebrew",
        is_variant: false,
        iso639_ids: &["he"],
        iso3166_ids: &["IL"],
    },
    Layout {
        id: "th",
        xkb_layout: "th",
        xkb_variant: "",
        short_name: "TH",
        display_name: "Thai",
        is_variant: false,
        iso639_ids: &["th"],
        iso3166_ids: &["TH"],
    },
    Layout {
        id: "se",
        xkb_layout: "se",
        xkb_variant: "",
        short_name: "SE",
        display_name: "Swedish",
        is_variant: false,
        iso639_ids: &["sv"],
        iso3166_ids: &["SE"],
    },
    Layout {
        id: "no",
        xkb_layout: "no",
        xkb_variant: "",
        short_name: "NO",
        display_name: "Norwegian",
        is_variant: false,
        iso639_ids: &["no"],
        iso3166_ids: &["NO"],
    },
    Layout {
        id: "dk",
        xkb_layout: "dk",
        xkb_variant: "",
        short_name: "DK",
        display_name: "Danish",
        is_variant: false,
        iso639_ids: &["da"],
        iso3166_ids: &["DK"],
    },
    Layout {
        id: "fi",
        xkb_layout: "fi",
        xkb_variant: "",
        short_name: "FI",
        display_name: "Finnish",
        is_variant: false,
        iso639_ids: &["fi"],
        iso3166_ids: &["FI"],
    },
    Layout {
        id: "pl",
        xkb_layout: "pl",
        xkb_variant: "",
        short_name: "PL",
        display_name: "Polish",
        is_variant: false,
        iso639_ids: &["pl"],
        iso3166_ids: &["PL"],
    },
    Layout {
        id: "ua",
        xkb_layout: "ua",
        xkb_variant: "",
        short_name: "UA",
        display_name: "Ukrainian",
        is_variant: false,
        iso639_ids: &["uk"],
        iso3166_ids: &["UA"],
    },
    Layout {
        id: "cz",
        xkb_layout: "cz",
        xkb_variant: "",
        short_name: "CZ",
        display_name: "Czech",
        is_variant: false,
        iso639_ids: &["cs"],
        iso3166_ids: &["CZ"],
    },
    Layout {
        id: "hu",
        xkb_layout: "hu",
        xkb_variant: "",
        short_name: "HU",
        display_name: "Hungarian",
        is_variant: false,
        iso639_ids: &["hu"],
        iso3166_ids: &["HU"],
    },
    Layout {
        id: "ch",
        xkb_layout: "ch",
        xkb_variant: "",
        short_name: "CH",
        display_name: "Swiss",
        is_variant: false,
        iso639_ids: &["de", "fr"],
        iso3166_ids: &["CH"],
    },
    Layout {
        id: "ca",
        xkb_layout: "ca",
        xkb_variant: "",
        short_name: "CA",
        display_name: "Canadian",
        is_variant: false,
        iso639_ids: &["en", "fr"],
        iso3166_ids: &["CA"],
    },
    Layout {
        id: "ir",
        xkb_layout: "ir",
        xkb_variant: "",
        short_name: "IR",
        display_name: "Persian",
        is_variant: false,
        iso639_ids: &["fa"],
        iso3166_ids: &["IR"],
    },
    Layout {
        id: "pk",
        xkb_layout: "pk",
        xkb_variant: "",
        short_name: "PK",
        display_name: "Urdu",
        is_variant: false,
        iso639_ids: &["ur"],
        iso3166_ids: &["PK"],
    },
    Layout {
        id: "ie",
        xkb_layout: "ie",
        xkb_variant: "",
        short_name: "IE",
        display_name: "Irish",
        is_variant: false,
        iso639_ids: &["ga"],
        iso3166_ids: &["IE"],
    },
    Layout {
        id: "hr",
        xkb_layout: "hr",
        xkb_variant: "",
        short_name: "HR",
        display_name: "Croatian",
        is_variant: false,
        iso639_ids: &["hr"],
        iso3166_ids: &["HR"],
    },
    Layout {
        id: "rs",
        xkb_layout: "rs",
        xkb_variant: "",
        short_name: "RS",
        display_name: "Serbian",
        is_variant: false,
        iso639_ids: &["sr"],
        iso3166_ids: &["RS"],
    },
    Layout {
        id: "bg",
        xkb_layout: "bg",
        xkb_variant: "",
        short_name: "BG",
        display_name: "Bulgarian",
        is_variant: false,
        iso639_ids: &["bg"],
        iso3166_ids: &["BG"],
    },
    Layout {
        id: "ro",
        xkb_layout: "ro",
        xkb_variant: "",
        short_name: "RO",
        display_name: "Romanian",
        is_variant: false,
        iso639_ids: &["ro"],
        iso3166_ids: &["RO"],
    },
    Layout {
        id: "sk",
        xkb_layout: "sk",
        xkb_variant: "",
        short_name: "SK",
        display_name: "Slovak",
        is_variant: false,
        iso639_ids: &["sk"],
        iso3166_ids: &["SK"],
    },
    Layout {
        id: "nl",
        xkb_layout: "nl",
        xkb_variant: "",
        short_name: "NL",
        display_name: "Dutch",
        is_variant: false,
        iso639_ids: &["nl"],
        iso3166_ids: &["NL"],
    },
    Layout {
        id: "lv",
        xkb_layout: "lv",
        xkb_variant: "",
        short_name: "LV",
        display_name: "Latvian",
        is_variant: false,
        iso639_ids: &["lv"],
        iso3166_ids: &["LV"],
    },
    Layout {
        id: "lt",
        xkb_layout: "lt",
        xkb_variant: "",
        short_name: "LT",
        display_name: "Lithuanian",
        is_variant: false,
        iso639_ids: &["lt"],
        iso3166_ids: &["LT"],
    },
    Layout {
        id: "ee",
        xkb_layout: "ee",
        xkb_variant: "",
        short_name: "EE",
        display_name: "Estonian",
        is_variant: false,
        iso639_ids: &["et"],
        iso3166_ids: &["EE"],
    },
    Layout {
        id: "is",
        xkb_layout: "is",
        xkb_variant: "",
        short_name: "IS",
        display_name: "Icelandic",
        is_variant: false,
        iso639_ids: &["is"],
        iso3166_ids: &["IS"],
    },
];

/// Static table of XKB option groups.
static OPTION_GROUPS: &[OptionGroup] = &[
    OptionGroup {
        id: "ctrl",
        description: "Ctrl key position",
        allow_multiple: false,
        options: &[
            ("ctrl:nocaps", "Caps Lock as Ctrl"),
            ("ctrl:swapcaps", "Swap Ctrl and Caps Lock"),
            ("ctrl:ctrl_acme", "Ctrl at A position (left)"),
            ("ctrl:ctrl_aa", "Ctrl at A position"),
            ("ctrl:ctrl_ra", "Ctrl at right of A"),
        ],
    },
    OptionGroup {
        id: "caps",
        description: "Caps Lock key behavior",
        allow_multiple: false,
        options: &[
            ("caps:internal", "Caps Lock uses internal capitalization"),
            ("caps:shift", "Caps Lock acts as Shift"),
            ("caps:swapescape", "Swap ESC and Caps Lock"),
            ("caps:none", "Caps Lock disabled"),
        ],
    },
    OptionGroup {
        id: "alt",
        description: "Alt/Win key behavior",
        allow_multiple: false,
        options: &[
            ("altwin:left_meta_win", "Left Alt as Meta"),
            ("altwin:alt_super_win", "Alt as Super"),
            ("altwin:swap_lalt_lwin", "Swap Left Alt and Left Win"),
        ],
    },
    OptionGroup {
        id: "compose",
        description: "Compose key position",
        allow_multiple: false,
        options: &[
            ("compose:ralt", "Right Alt"),
            ("compose:caps", "Caps Lock"),
            ("compose:menu", "Menu key"),
            ("compose:rctrl", "Right Ctrl"),
        ],
    },
    OptionGroup {
        id: "grp",
        description: "Switching to another layout",
        allow_multiple: false,
        options: &[
            ("grp:alt_shift_toggle", "Alt+Shift"),
            ("grp:caps_toggle", "Caps Lock"),
            ("grp:shifts_toggle", "Both Shift keys together"),
            ("grp:alt_space_toggle", "Alt+Space"),
            ("grp:win_space_toggle", "Win+Space"),
        ],
    },
    OptionGroup {
        id: "lv3",
        description: "3rd level key",
        allow_multiple: false,
        options: &[
            ("lv3:ralt_switch", "Right Alt"),
            ("lv3:caps_switch", "Caps Lock"),
            ("lv3:switch", "Right Ctrl"),
        ],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    fn test_get_layout_info() {
        let info = XkbInfo::new();
        let layout = info.get_layout_info("us").unwrap();
        assert_eq!(layout.xkb_layout, "us");
        assert_eq!(layout.display_name, "English (US)");
    }

    fn test_get_layouts_for_language() {
        let info = XkbInfo::new();
        let layouts = info.get_layouts_for_language("en");
        assert!(!layouts.is_empty());
        assert!(layouts.iter().any(|l| l.id == "us"));
    }

    fn test_get_layouts_for_country() {
        let info = XkbInfo::new();
        let layouts = info.get_layouts_for_country("DE");
        assert!(!layouts.is_empty());
        assert!(layouts.iter().any(|l| l.id == "de"));
    }

    fn test_option_group() {
        let info = XkbInfo::new();
        let desc = info.description_for_group("ctrl").unwrap();
        assert_eq!(desc, "Ctrl key position");
        let opt = info.description_for_option("caps", "caps:none").unwrap();
        assert_eq!(opt, "Caps Lock disabled");
    }

    fn test_unknown_layout() {
        let info = XkbInfo::new();
        assert!(info.get_layout_info("xx").is_none());
    }
}
