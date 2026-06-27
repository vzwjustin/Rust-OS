//! Unicode types matching `gunicode.h`.
//!
//! Defines Unicode general categories, break types, normalization modes,
//! and script codes. These are pure type definitions with no OS dependency.
//! Fully `no_std` compatible.

/// Unicode general category (`GUnicodeType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UnicodeType {
    Control,
    Format,
    Unassigned,
    PrivateUse,
    Surrogate,
    LowercaseLetter,
    ModifierLetter,
    OtherLetter,
    TitlecaseLetter,
    UppercaseLetter,
    SpacingMark,
    EnclosingMark,
    NonSpacingMark,
    DecimalNumber,
    LetterNumber,
    OtherNumber,
    ConnectPunctuation,
    DashPunctuation,
    ClosePunctuation,
    FinalPunctuation,
    InitialPunctuation,
    OtherPunctuation,
    OpenPunctuation,
    CurrencySymbol,
    ModifierSymbol,
    MathSymbol,
    OtherSymbol,
    LineSeparator,
    ParagraphSeparator,
    SpaceSeparator,
}

/// Unicode line break type (`GUnicodeBreakType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UnicodeBreakType {
    Mandatory,
    CarriageReturn,
    LineFeed,
    CombiningMark,
    Surrogate,
    ZeroWidthSpace,
    Inseparable,
    NonBreakingGlue,
    Contingent,
    Space,
    After,
    Before,
    BeforeAndAfter,
    Hyphen,
    NonStarter,
    OpenPunctuation,
    ClosePunctuation,
    Quotation,
    Exclamation,
    Ideographic,
    Numeric,
    InfixSeparator,
    Symbol,
    Alphabetic,
    Prefix,
    Postfix,
    ComplexContext,
    Ambiguous,
    Unknown,
    NextLine,
    WordJoiner,
    HangulLJamo,
    HangulVJamo,
    HangulTJamo,
    HangulLVSyllable,
    HangulLVTSyllable,
    CloseParenthesis,
    ConditionalJapaneseStarter,
    HebrewLetter,
    RegionalIndicator,
    EmojiBase,
    EmojiModifier,
    ZeroWidthJoiner,
    Aksara,
    AksaraPreBase,
    AksaraStart,
    ViramaFinal,
    Virama,
    UnambiguousHyphen,
}

/// Unicode normalization mode (`GNormalizeMode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NormalizeMode {
    Default,
    DefaultCompose,
    All,
    AllCompose,
}

/// Unicode script (`GUnicodeScript`).
///
/// Subset of the most common scripts. Full list would be very large.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UnicodeScript {
    Common,
    Inherited,
    Arabic,
    Armenian,
    Bengali,
    Cyrillic,
    Devanagari,
    Georgian,
    Greek,
    Gujarati,
    Gurmukhi,
    Hangul,
    Han,
    Hebrew,
    Hiragana,
    Kannada,
    Katakana,
    Lao,
    Latin,
    Malayalam,
    Oriya,
    Tamil,
    Telugu,
    Thai,
    Tibetan,
    Bopomofo,
    Braille,
    CanadianAboriginal,
    Cherokee,
    Ethiopic,
    Khmer,
    Mongolian,
    Myanmar,
    Ogham,
    Runic,
    Sinhala,
    Syriac,
    Thaana,
    Yi,
    Deseret,
    Gothic,
    OldItalic,
    Buhid,
    Hanunoo,
    Tagalog,
    Tagbanwa,
    Cypriot,
    Limbu,
    LinearB,
    Osmanya,
    Shavian,
    TaiLe,
    Ugaritic,
    Buginese,
    Coptic,
    Glagolitic,
    Kharoshthi,
    NewTaiLue,
    OldPersian,
    SylotiNagri,
    Tifinagh,
    Balinese,
    Cuneiform,
    Nko,
    PhagsPa,
    Phoenician,
    Carian,
    Cham,
    KayahLi,
    Lepcha,
    Lycian,
    Lydian,
    OlChiki,
    Rejang,
    Saurashtra,
    Sundanese,
    Vai,
    Avestan,
    Bamum,
    EgyptianHieroglyphs,
    ImperialAramaic,
    InscriptionalPahlavi,
    InscriptionalParthian,
    Javanese,
    Kaithi,
    Lisu,
    MeeteiMayek,
    OldSouthArabian,
    OldTurkic,
    Samaritan,
    TaiTham,
    TaiViet,
    Batak,
    Brahmi,
    Mandaic,
    Chakma,
    MeroiticCursive,
    MeroiticHieroglyphs,
    Miao,
    Sharada,
    SoraSompeng,
    Takri,
    BassaVah,
    CaucasianAlbanian,
    Duployan,
    Elbasan,
    Grantha,
    PahawhHmong,
    Khojki,
    LinearA,
    Mahajani,
    Manichaean,
    MendeKikakui,
    Modi,
    Mro,
    Nabataean,
    OldNorthArabian,
    OldPermic,
    PauCinHau,
    Palmyrene,
    PauCinHauLogograms,
    PsalterPahlavi,
    Siddham,
    Tirhuta,
    WarangCiti,
    Ahom,
    AnatolianHieroglyphs,
    Hatran,
    Multani,
    OldHungarian,
    SignWriting,
    Adlam,
    Bhaiksuki,
    Marchen,
    Newa,
    Osage,
    Tangut,
    MasaramGondi,
    Nushu,
    Soyombo,
    Bhaiksuki2,
    Dogra,
    GunjalaGondi,
    HanifiRohingya,
    Makasar,
    Medefaidrin,
    OldSogdian,
    Sogdian,
    Elymaic,
    Nandinagari,
    NyiakengPuachueHmong,
    Wancho,
    Chorasmian,
    DivesAkuru,
    KhitanSmallScript,
    Yezidi,
    CyproMinoan,
    OldUyghur,
    Tangsa,
    Toto,
    Vithkuqi,
    Unknown,
}

/// Combining class for a Unicode character.
///
/// This is a simplified version. Full Unicode data tables would be needed
/// for complete coverage.
pub fn combining_class(ch: u32) -> i32 {
    // Most characters have combining class 0
    if ch < 0x0300 {
        return 0;
    }
    // Combining diacritical marks (0x0300-0x036F) have various classes
    if ch >= 0x0300 && ch <= 0x036F {
        return match ch {
            0x0316..=0x0319 | 0x031C..=0x031E | 0x0323..=0x0326 |
            0x032A | 0x032C | 0x0333..=0x0339 | 0x034C => 220,
            0x0300..=0x0304 | 0x0306..=0x030C | 0x030F | 0x0311..=0x0312 |
            0x0315 | 0x031A | 0x0322 | 0x032B | 0x0327..=0x0328 |
            0x032F | 0x0331 | 0x0338 | 0x0342..=0x0344 | 0x0346 |
            0x0350..=0x0352 | 0x0357 => 230,
            0x0313..=0x0314 | 0x033D..=0x033E | 0x0340..=0x0341 |
            0x0343 | 0x0345 | 0x0353..=0x0356 | 0x0359..=0x035A => 240,
            0x0347..=0x0349 | 0x034D..=0x034E | 0x0358 => 232,
            _ => 0,
        };
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_type_variants() {
        let t = UnicodeType::UppercaseLetter;
        assert_eq!(t, UnicodeType::UppercaseLetter);
        assert_ne!(t, UnicodeType::LowercaseLetter);
    }

    #[test]
    fn break_type_variants() {
        let b = UnicodeBreakType::Mandatory;
        assert_eq!(b, UnicodeBreakType::Mandatory);
    }

    #[test]
    fn normalize_mode() {
        assert_eq!(NormalizeMode::Default, NormalizeMode::Default);
        assert_ne!(NormalizeMode::All, NormalizeMode::Default);
    }

    #[test]
    fn script_variants() {
        assert_eq!(UnicodeScript::Latin, UnicodeScript::Latin);
        assert_ne!(UnicodeScript::Latin, UnicodeScript::Greek);
    }

    #[test]
    fn combining_class_basic() {
        assert_eq!(combining_class(0x0041), 0); // 'A'
        assert_eq!(combining_class(0x0300), 230); // combining grave
        assert_eq!(combining_class(0x0316), 220); // combining grave below
    }
}
