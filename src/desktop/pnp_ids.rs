//! PNP (Plug and Play) vendor ID lookup — ported from gnome-pnp-ids.c
//!
//! The upstream uses udev/hwdb to resolve ACPI/PNP vendor IDs to manufacturer
//! names.  Since RustOS has no udev, we embed a static table of the most common
//! display vendor IDs and look them up directly.

/// Lookup the manufacturer name for a given PNP/ACPI vendor ID.
///
/// Returns `Some(&'static str)` if found, `None` if the ID is not in the table.
/// This replaces the upstream `gnome_pnp_ids_get_pnp_id()` which queries udev hwdb.
pub fn get_pnp_id(pnp_id: &str) -> Option<&'static str> {
    TABLE
        .iter()
        .find(|(id, _)| id_eq_ignore_case(id, pnp_id))
        .map(|(_, name)| *name)
}

/// Resolve a PNP ID, returning "Unknown" if not found (matching the upstream
/// fallback behavior when udev returns no result).
pub fn get_pnp_id_or_unknown(pnp_id: &str) -> &'static str {
    get_pnp_id(pnp_id).unwrap_or("Unknown")
}

/// Case-insensitive ASCII comparison for two str slices (no alloc needed).
fn id_eq_ignore_case(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.chars()
        .zip(b.chars())
        .all(|(x, y)| x.to_ascii_uppercase() == y.to_ascii_uppercase())
}

/// Static PNP ID → manufacturer table.  Covers the most common monitor and
/// display controller vendors.  Sourced from the Linux hwdb pnp.id database.
static TABLE: &[(&str, &str)] = &[
    ("ACI", "Asus Computer International"),
    ("ACR", "Acer"),
    ("ACT", "Actebis"),
    ("ADI", "ADI Corporation"),
    ("AMW", "AMW"),
    ("AOC", "AOC"),
    ("API", "America Online"),
    ("APP", "Apple Computer"),
    ("ART", "ArtMedia"),
    ("AST", "AST Research"),
    ("AUO", "AsusTek"),
    ("BMM", "BMM"),
    ("BNQ", "BenQ"),
    ("BOE", "BOE Display Technology"),
    ("CMO", "Acer"),
    ("CPL", "Compal"),
    ("CPQ", "Compaq"),
    ("CPT", "Chunghwa Picture Tubes"),
    ("CTX", "CTX"),
    ("DEC", "DEC"),
    ("DEL", "Dell"),
    ("DPC", "Delta"),
    ("DWE", "Daewoo"),
    ("EIZ", "EIZO"),
    ("ELS", "ELSA"),
    ("ENC", "EIZO"),
    ("EPI", "Envision"),
    ("FCM", "Funai"),
    ("FUJ", "Fujitsu"),
    ("FUS", "Fujitsu-Siemens"),
    ("GSM", "Goldstar Company"),
    ("GWY", "Gateway 2000"),
    ("GBT", "Gigabyte"),
    ("HEI", "Hyundai"),
    ("HIQ", "Hyundai"),
    ("HIT", "Hyundai"),
    ("HSD", "Hannspree Inc"),
    ("HSL", "Hansol"),
    ("HTC", "Hitachi"),
    ("HWP", "HP"),
    ("IBM", "IBM"),
    ("ICL", "Fujitsu ICL"),
    ("IFS", "InFocus"),
    ("IQT", "Hyundai"),
    ("IVM", "Iiyama"),
    ("KDS", "Korea Data Systems"),
    ("KFC", "KFC Computek"),
    ("LEN", "Lenovo"),
    ("LGD", "LG Display"),
    ("LKM", "ADLAS / AKIA"),
    ("LNK", "LINK Technologies"),
    ("LPL", "LG Philips"),
    ("LTN", "Lite-On"),
    ("MAG", "MAG InnoVision"),
    ("MAX", "Belinea"),
    ("MEI", "Panasonic"),
    ("MEL", "Mitsubishi Electronics"),
    ("MIR", "miro Computer Products"),
    ("MSI", "MSI"),
    ("MS_928", "Panasonic"),
    ("MTC", "MITAC"),
    ("NAN", "Nanao"),
    ("NEC", "NEC"),
    ("NOK", "Nokia Data"),
    ("NVD", "Nvidia"),
    ("OPT", "Optoma"),
    ("OQI", "OPTIQUEST"),
    ("PBN", "Packard Bell"),
    ("PCK", "Daewoo"),
    ("PDC", "Polaroid"),
    ("PGS", "Princeton Graphic Systems"),
    ("PHL", "Philips"),
    ("PIO", "Pioneer"),
    ("PNR", "Planar"),
    ("PRT", "Princeton"),
    ("REL", "Relisys"),
    ("SAM", "Samsung"),
    ("SAN", "Samsung"),
    ("SBI", "Smarttech"),
    ("SEC", "Hewlett-Packard"),
    ("SGI", "SGI"),
    ("SMC", "Samtron"),
    ("SMI", "Smile"),
    ("SNI", "Siemens Nixdorf"),
    ("SNY", "Sony"),
    ("SPT", "Sceptre"),
    ("SRC", "Shamrock"),
    ("STN", "Samtron"),
    ("STP", "Sceptre"),
    ("SUN", "Sun Microsystems"),
    ("TAT", "Tatung"),
    ("TOS", "Toshiba"),
    ("TRL", "Royal Information"),
    ("TSB", "Toshiba"),
    ("UNK", "Unknown"),
    ("UNM", "Unisys"),
    ("VES", "Vestel"),
    ("VIZ", "Vizio"),
    ("VSC", "ViewSonic"),
    ("WAC", "Wacom"),
    ("WDE", "Westinghouse"),
    ("YMH", "Yamaha"),
    ("ZCM", "Zenith"),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn test_known_id() {
        assert_eq!(get_pnp_id("DEL"), Some("Dell"));
    }

    fn test_case_insensitive() {
        assert_eq!(get_pnp_id("del"), Some("Dell"));
        assert_eq!(get_pnp_id("Del"), Some("Dell"));
    }

    fn test_unknown_id() {
        assert_eq!(get_pnp_id("XYZ"), None);
    }

    fn test_unknown_fallback() {
        assert_eq!(get_pnp_id_or_unknown("XYZ"), "Unknown");
    }
}
