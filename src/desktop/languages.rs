//! Language/locale utilities — ported from gnome-languages.c
//!
//! Parses locale strings (e.g. "en_US.UTF-8") into their components,
//! normalizes locales, and provides a static lookup of ISO 639 language
//! codes and ISO 3166 country codes to human-readable names.
//!
//! The upstream reads from `/usr/share/xml/iso-codes` and the filesystem
//! locale directory.  We embed a static table of common language/country
//! codes for no_std use.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Parsed locale components.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocaleParts {
    pub language_code: Option<String>,
    pub country_code: Option<String>,
    pub codeset: Option<String>,
    pub modifier: Option<String>,
}

/// Parse a locale string into its components.
/// Matches `gnome_parse_locale()`.
///
/// Format: `language[_country][.codeset][@modifier]`
pub fn parse_locale(locale: &str) -> LocaleParts {
    let mut parts = LocaleParts {
        language_code: None,
        country_code: None,
        codeset: None,
        modifier: None,
    };

    // Split off modifier (@)
    let (main, modifier) = match locale.find('@') {
        Some(pos) => (&locale[..pos], Some(locale[pos + 1..].to_string())),
        None => (locale, None),
    };

    // Split off codeset (.)
    let (lang_country, codeset) = match main.find('.') {
        Some(pos) => (&main[..pos], Some(main[pos + 1..].to_string())),
        None => (main, None),
    };

    // Split off country (_)
    let (language, country) = match lang_country.find('_') {
        Some(pos) => (
            &lang_country[..pos],
            Some(lang_country[pos + 1..].to_string()),
        ),
        None => (lang_country, None),
    };

    if !language.is_empty() {
        parts.language_code = Some(language.to_string());
    }
    parts.country_code = country;
    parts.codeset = codeset;
    parts.modifier = modifier;

    parts
}

/// Normalize a locale string by reassembling its parsed components.
/// Matches `gnome_normalize_locale()`.
pub fn normalize_locale(locale: &str) -> Option<String> {
    let parts = parse_locale(locale);
    let lang = parts.language_code.as_ref()?;
    let mut result = lang.clone();
    if let Some(ref country) = parts.country_code {
        result.push('_');
        result.push_str(country);
    }
    if let Some(ref codeset) = parts.codeset {
        result.push('.');
        result.push_str(codeset);
    }
    if let Some(ref modifier) = parts.modifier {
        result.push('@');
        result.push_str(modifier);
    }
    Some(result)
}

/// Construct a locale name from components.
/// Matches `construct_language_name()`.
pub fn construct_language_name(
    language: &str,
    territory: Option<&str>,
    codeset: Option<&str>,
    modifier: Option<&str>,
) -> String {
    let mut result = language.to_string();
    if let Some(t) = territory {
        result.push('_');
        result.push_str(t);
    }
    if let Some(c) = codeset {
        result.push('.');
        result.push_str(c);
    }
    if let Some(m) = modifier {
        result.push('@');
        result.push_str(m);
    }
    result
}

/// Get a human-readable language name from a locale string.
/// Matches `gnome_get_language_from_locale()`.
pub fn get_language_from_locale(locale: &str) -> Option<String> {
    let parts = parse_locale(locale);
    let lang_code = parts.language_code.as_ref()?;
    let lang_name = get_language_from_code(lang_code)?;
    let country_name = parts
        .country_code
        .as_ref()
        .and_then(|c| get_country_from_code(c));
    match country_name {
        Some(country) => Some(format!("{} ({})", lang_name, country)),
        None => Some(lang_name.to_string()),
    }
}

/// Get a human-readable country name from a locale string.
/// Matches `gnome_get_country_from_locale()`.
pub fn get_country_from_locale(locale: &str) -> Option<String> {
    let parts = parse_locale(locale);
    let country_code = parts.country_code.as_ref()?;
    get_country_from_code(country_code).map(|s| s.to_string())
}

/// Get all known locales (static list of common ones).
/// Matches `gnome_get_all_locales()`.
pub fn get_all_locales() -> Vec<String> {
    COMMON_LOCALES.iter().map(|s| s.to_string()).collect()
}

/// Check if a language code has translations (static check).
/// Matches `gnome_language_has_translations()`.
pub fn language_has_translations(code: &str) -> bool {
    LANGUAGE_TABLE.iter().any(|(c, _)| *c == code)
}

/// Get the human-readable name for an ISO 639 language code.
/// Matches `gnome_get_language_from_code()`.
pub fn get_language_from_code(code: &str) -> Option<&'static str> {
    LANGUAGE_TABLE
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, name)| *name)
}

/// Get the human-readable name for an ISO 3166 country code.
/// Matches `gnome_get_country_from_code()`.
pub fn get_country_from_code(code: &str) -> Option<&'static str> {
    COUNTRY_TABLE
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, name)| *name)
}

/// Get a translated modifier name (e.g. "cyrillic" → "Cyrillic").
/// Matches `gnome_get_translated_modifier()`.
pub fn get_translated_modifier<'a>(modifier: &'a str) -> &'a str {
    MODIFIER_TABLE
        .iter()
        .find(|(m, _)| *m == modifier)
        .map(|(_, name)| *name)
        .unwrap_or(modifier)
}

/// Get the default input source for a locale.
/// Matches `gnome_get_input_source_from_locale()`.
/// Looks up by full locale (e.g. "en_US"), falling back to language-only.
pub fn get_input_source_from_locale(locale: &str) -> Option<(&'static str, &'static str)> {
    let parts = parse_locale(locale);
    let lang = parts.language_code.as_ref()?;
    let country = parts.country_code.as_deref().unwrap_or("");

    // Try full locale match first (e.g. "en_US")
    if !country.is_empty() {
        let full = format!("{}_{}", lang, country);
        if let Some((_, t, id)) = INPUT_SOURCE_TABLE
            .iter()
            .find(|(l, _, _)| *l == full.as_str())
        {
            return Some((*t, *id));
        }
    }

    // Fall back to language-only match (e.g. "en" → "en_US")
    INPUT_SOURCE_TABLE
        .iter()
        .find(|(l, _, _)| l.starts_with(&format!("{}_", lang)))
        .map(|(_, t, id)| (*t, *id))
}

/// Common locale strings.
static COMMON_LOCALES: &[&str] = &[
    "en_US.UTF-8",
    "en_GB.UTF-8",
    "de_DE.UTF-8",
    "fr_FR.UTF-8",
    "es_ES.UTF-8",
    "it_IT.UTF-8",
    "pt_BR.UTF-8",
    "ru_RU.UTF-8",
    "ja_JP.UTF-8",
    "zh_CN.UTF-8",
    "zh_TW.UTF-8",
    "ko_KR.UTF-8",
    "ar_SA.UTF-8",
    "hi_IN.UTF-8",
    "tr_TR.UTF-8",
    "pl_PL.UTF-8",
    "nl_NL.UTF-8",
    "sv_SE.UTF-8",
    "da_DK.UTF-8",
    "fi_FI.UTF-8",
    "no_NO.UTF-8",
    "cs_CZ.UTF-8",
    "el_GR.UTF-8",
    "he_IL.UTF-8",
    "th_TH.UTF-8",
    "vi_VN.UTF-8",
    "id_ID.UTF-8",
    "ms_MY.UTF-8",
    "uk_UA.UTF-8",
    "ro_RO.UTF-8",
    "hu_HU.UTF-8",
    "sk_SK.UTF-8",
    "bg_BG.UTF-8",
    "hr_HR.UTF-8",
    "sr_RS.UTF-8",
    "sl_SI.UTF-8",
    "et_EE.UTF-8",
    "lv_LV.UTF-8",
    "lt_LT.UTF-8",
    "is_IS.UTF-8",
    "ga_IE.UTF-8",
    "cy_GB.UTF-8",
    "eu_ES.UTF-8",
    "ca_ES.UTF-8",
    "gl_ES.UTF-8",
    "fa_IR.UTF-8",
    "ur_PK.UTF-8",
    "bn_IN.UTF-8",
    "ta_IN.UTF-8",
    "te_IN.UTF-8",
    "ml_IN.UTF-8",
];

/// ISO 639 language codes → names (subset of most common).
static LANGUAGE_TABLE: &[(&str, &str)] = &[
    ("en", "English"),
    ("de", "German"),
    ("fr", "French"),
    ("es", "Spanish"),
    ("it", "Italian"),
    ("pt", "Portuguese"),
    ("ru", "Russian"),
    ("ja", "Japanese"),
    ("zh", "Chinese"),
    ("ko", "Korean"),
    ("ar", "Arabic"),
    ("hi", "Hindi"),
    ("tr", "Turkish"),
    ("pl", "Polish"),
    ("nl", "Dutch"),
    ("sv", "Swedish"),
    ("da", "Danish"),
    ("fi", "Finnish"),
    ("no", "Norwegian"),
    ("cs", "Czech"),
    ("el", "Greek"),
    ("he", "Hebrew"),
    ("th", "Thai"),
    ("vi", "Vietnamese"),
    ("id", "Indonesian"),
    ("ms", "Malay"),
    ("uk", "Ukrainian"),
    ("ro", "Romanian"),
    ("hu", "Hungarian"),
    ("sk", "Slovak"),
    ("bg", "Bulgarian"),
    ("hr", "Croatian"),
    ("sr", "Serbian"),
    ("sl", "Slovenian"),
    ("et", "Estonian"),
    ("lv", "Latvian"),
    ("lt", "Lithuanian"),
    ("is", "Icelandic"),
    ("ga", "Irish"),
    ("cy", "Welsh"),
    ("eu", "Basque"),
    ("ca", "Catalan"),
    ("gl", "Galician"),
    ("fa", "Persian"),
    ("ur", "Urdu"),
    ("bn", "Bengali"),
    ("ta", "Tamil"),
    ("te", "Telugu"),
    ("ml", "Malayalam"),
];

/// ISO 3166 country codes → names (subset of most common).
static COUNTRY_TABLE: &[(&str, &str)] = &[
    ("US", "United States"),
    ("GB", "United Kingdom"),
    ("DE", "Germany"),
    ("FR", "France"),
    ("ES", "Spain"),
    ("IT", "Italy"),
    ("BR", "Brazil"),
    ("RU", "Russia"),
    ("JP", "Japan"),
    ("CN", "China"),
    ("TW", "Taiwan"),
    ("KR", "South Korea"),
    ("SA", "Saudi Arabia"),
    ("IN", "India"),
    ("TR", "Turkey"),
    ("PL", "Poland"),
    ("NL", "Netherlands"),
    ("SE", "Sweden"),
    ("DK", "Denmark"),
    ("FI", "Finland"),
    ("NO", "Norway"),
    ("CZ", "Czech Republic"),
    ("GR", "Greece"),
    ("IL", "Israel"),
    ("TH", "Thailand"),
    ("VN", "Vietnam"),
    ("ID", "Indonesia"),
    ("MY", "Malaysia"),
    ("UA", "Ukraine"),
    ("RO", "Romania"),
    ("HU", "Hungary"),
    ("SK", "Slovakia"),
    ("BG", "Bulgaria"),
    ("HR", "Croatia"),
    ("RS", "Serbia"),
    ("SI", "Slovenia"),
    ("EE", "Estonia"),
    ("LV", "Latvia"),
    ("LT", "Lithuania"),
    ("IS", "Iceland"),
    ("IE", "Ireland"),
    ("PT", "Portugal"),
    ("AT", "Austria"),
    ("CH", "Switzerland"),
    ("BE", "Belgium"),
    ("CA", "Canada"),
    ("AU", "Australia"),
    ("NZ", "New Zealand"),
    ("MX", "Mexico"),
    ("AR", "Argentina"),
    ("CL", "Chile"),
    ("CO", "Colombia"),
    ("ZA", "South Africa"),
    ("EG", "Egypt"),
    ("MA", "Morocco"),
    ("IR", "Iran"),
    ("PK", "Pakistan"),
    ("BD", "Bangladesh"),
    ("PH", "Philippines"),
    ("SG", "Singapore"),
    ("HK", "Hong Kong"),
];

/// Locale modifier → display name.
static MODIFIER_TABLE: &[(&str, &str)] = &[
    ("cyrillic", "Cyrillic"),
    ("latin", "Latin"),
    ("euro", "Euro"),
    ("abnt2", "ABNT2"),
    ("pinyin", "Pinyin"),
    ("stroke", "Stroke"),
    ("phonetic", "Phonetic"),
    ("valencia", "Valencian"),
    ("latn", "Latin"),
];

/// Default input sources per locale (type, id).
/// Matches the `default-input-sources.h` mapping from upstream.
static INPUT_SOURCE_TABLE: &[(&str, &str, &str)] = &[
    ("ar_DZ", "xkb", "ara+azerty"),
    ("as_IN", "ibus", "m17n:as:inscript2"),
    ("ast_ES", "xkb", "es+ast"),
    ("az_AZ", "xkb", "az"),
    ("be_BY", "xkb", "by"),
    ("bg_BG", "xkb", "bg+phonetic"),
    ("bn_IN", "ibus", "m17n:bn:inscript2"),
    ("ca_ES", "xkb", "es+cat"),
    ("cs_CZ", "xkb", "cz"),
    ("de_CH", "xkb", "ch"),
    ("de_DE", "xkb", "de"),
    ("el_CY", "xkb", "gr"),
    ("el_GR", "xkb", "gr"),
    ("en_GB", "xkb", "gb"),
    ("en_US", "xkb", "us"),
    ("en_ZA", "xkb", "za"),
    ("es_ES", "xkb", "es"),
    ("es_GT", "xkb", "latam"),
    ("es_MX", "xkb", "latam"),
    ("es_US", "xkb", "us+intl"),
    ("fr_BE", "xkb", "be"),
    ("fr_CH", "xkb", "ch+fr"),
    ("fr_FR", "xkb", "fr+oss"),
    ("gl_ES", "xkb", "es"),
    ("gu_IN", "ibus", "m17n:gu:inscript2"),
    ("he_IL", "xkb", "il"),
    ("hi_IN", "ibus", "m17n:hi:inscript2"),
    ("id_ID", "xkb", "us"),
    ("it_IT", "xkb", "it"),
    ("ja_JP", "ibus", "anthy"),
    ("kn_IN", "ibus", "m17n:kn:inscript2"),
    ("ko_KR", "ibus", "hangul"),
    ("mai_IN", "ibus", "m17n:mai:inscript2"),
    ("ml_IN", "ibus", "m17n:ml:inscript2"),
    ("mr_IN", "ibus", "m17n:mr:inscript2"),
    ("nl_NL", "xkb", "us+altgr-intl"),
    ("or_IN", "ibus", "m17n:or:inscript2"),
    ("pa_IN", "ibus", "m17n:pa:inscript2-guru"),
    ("pl_PL", "xkb", "pl"),
    ("pt_BR", "xkb", "br"),
    ("pt_PT", "xkb", "pt"),
    ("ru_RU", "xkb", "ru"),
    ("sd_IN", "ibus", "m17n:sd:inscript2-deva"),
    ("sk_SK", "xkb", "sk"),
    ("ta_IN", "ibus", "m17n:ta:inscript2"),
    ("te_IN", "ibus", "m17n:te:inscript2"),
    ("tr_TR", "xkb", "tr"),
    ("ur_IN", "ibus", "m17n:ur:phonetic"),
    ("zh_CN", "ibus", "libpinyin"),
    ("zh_HK", "ibus", "cangjie"),
    ("zh_TW", "ibus", "chewing"),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn test_parse_locale_full() {
        let parts = parse_locale("en_US.UTF-8@latin");
        assert_eq!(parts.language_code.as_deref(), Some("en"));
        assert_eq!(parts.country_code.as_deref(), Some("US"));
        assert_eq!(parts.codeset.as_deref(), Some("UTF-8"));
        assert_eq!(parts.modifier.as_deref(), Some("latin"));
    }

    fn test_parse_locale_simple() {
        let parts = parse_locale("de");
        assert_eq!(parts.language_code.as_deref(), Some("de"));
        assert_eq!(parts.country_code, None);
        assert_eq!(parts.codeset, None);
        assert_eq!(parts.modifier, None);
    }

    fn test_normalize_locale() {
        let n = normalize_locale("en_US.UTF-8").unwrap();
        assert_eq!(n, "en_US.UTF-8");
    }

    fn test_get_language_from_code() {
        assert_eq!(get_language_from_code("en"), Some("English"));
        assert_eq!(get_language_from_code("de"), Some("German"));
        assert_eq!(get_language_from_code("xx"), None);
    }

    fn test_get_country_from_code() {
        assert_eq!(get_country_from_code("US"), Some("United States"));
        assert_eq!(get_country_from_code("DE"), Some("Germany"));
        assert_eq!(get_country_from_code("XX"), None);
    }

    fn test_get_language_from_locale() {
        let name = get_language_from_locale("en_US.UTF-8").unwrap();
        assert_eq!(name, "English (United States)");
    }

    fn test_construct_language_name() {
        let name = construct_language_name("en", Some("US"), Some("UTF-8"), None);
        assert_eq!(name, "en_US.UTF-8");
    }

    fn test_input_source() {
        let (t, id) = get_input_source_from_locale("en_US.UTF-8").unwrap();
        assert_eq!(t, "xkb");
        assert_eq!(id, "us");
    }

    fn test_input_source_full_locale() {
        let (t, id) = get_input_source_from_locale("ja_JP.UTF-8").unwrap();
        assert_eq!(t, "ibus");
        assert_eq!(id, "anthy");
    }

    fn test_input_source_fallback() {
        // fr_BE should find Belgian layout
        let (t, id) = get_input_source_from_locale("fr_BE.UTF-8").unwrap();
        assert_eq!(t, "xkb");
        assert_eq!(id, "be");
    }
}
