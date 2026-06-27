//! Regular expressions matching `gregex.h` / `gregex.c`.
//!
//! Implements a backtracking regex engine supporting:
//! - Literals, `.`, `*`, `+`, `?`, `{n,m}`
//! - Character classes `[abc]`, `[^abc]`, ranges `[a-z]`
//! - Anchors `^`, `$`
//! - Alternation `|`
//! - Groups `()` with captures
//! - Escapes: `\d`, `\D`, `\w`, `\W`, `\s`, `\S`, `\b`, `\B`
//! - Case-insensitive flag
//! - Greedy and lazy quantifiers
//! - Split and replace operations
//!
//! Fully `no_std` compatible using `alloc`.

use crate::prelude::*;

/// Regex error codes (`GRegexError`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RegexError {
    Compile,
    Optimize,
    Replace,
    Match,
    Internal,
    StrayBackslash,
    MissingControlChar,
    UnrecognizedEscape,
    QuantifiersOutOfOrder,
    QuantifierTooBig,
    UnterminatedCharacterClass,
    InvalidEscapeInCharacterClass,
    RangeOutOfOrder,
    NothingToRepeat,
    UnrecognizedCharacter,
    PosixNamedClassOutsideClass,
    UnmatchedParenthesis,
    InexistentSubpatternReference,
    UnterminatedComment,
    ExpressionTooLarge,
    MemoryError,
    VariableLengthLookbehind,
    MalformedCondition,
    TooManyConditionalBranches,
    AssertionExpected,
    UnknownPosixClassName,
    PosixCollatingElementsNotSupported,
    HexCodeTooLarge,
    InvalidCondition,
    SingleByteMatchInLookbehind,
    InfiniteLoop,
    MissingSubpatternNameTerminator,
    DuplicateSubpatternName,
    MalformedProperty,
    UnknownProperty,
    SubpatternNameTooLong,
    TooManySubpatterns,
    InvalidOctalValue,
    TooManyBranchesInDefine,
    DefineRepetion,
    InconsistentNewlineOptions,
    MissingBackReference,
    InvalidRelativeReference,
    BacktrackingControlVerbArgumentForbidden,
    UnknownBacktrackingControlVerb,
    NumberTooBig,
    MissingSubpatternName,
    MissingDigit,
    InvalidDataCharacter,
    ExtraSubpatternName,
    BacktrackingControlVerbArgumentRequired,
    InvalidControlChar,
    MissingName,
    NotSupportedInClass,
    TooManyForwardReferences,
    NameTooLong,
    CharacterValueTooLarge,
}

impl RegexError {
    pub fn to_code(self) -> i32 {
        match self {
            RegexError::Compile => 0,
            RegexError::Optimize => 1,
            RegexError::Replace => 2,
            RegexError::Match => 3,
            RegexError::Internal => 4,
            RegexError::StrayBackslash => 101,
            RegexError::MissingControlChar => 102,
            RegexError::UnrecognizedEscape => 103,
            RegexError::QuantifiersOutOfOrder => 104,
            RegexError::QuantifierTooBig => 105,
            RegexError::UnterminatedCharacterClass => 106,
            RegexError::InvalidEscapeInCharacterClass => 107,
            RegexError::RangeOutOfOrder => 108,
            RegexError::NothingToRepeat => 109,
            RegexError::UnrecognizedCharacter => 112,
            RegexError::PosixNamedClassOutsideClass => 113,
            RegexError::UnmatchedParenthesis => 114,
            RegexError::InexistentSubpatternReference => 115,
            RegexError::UnterminatedComment => 118,
            RegexError::ExpressionTooLarge => 120,
            RegexError::MemoryError => 121,
            RegexError::VariableLengthLookbehind => 125,
            RegexError::MalformedCondition => 126,
            RegexError::TooManyConditionalBranches => 127,
            RegexError::AssertionExpected => 128,
            RegexError::UnknownPosixClassName => 130,
            RegexError::PosixCollatingElementsNotSupported => 131,
            RegexError::HexCodeTooLarge => 134,
            RegexError::InvalidCondition => 135,
            RegexError::SingleByteMatchInLookbehind => 136,
            RegexError::InfiniteLoop => 140,
            RegexError::MissingSubpatternNameTerminator => 142,
            RegexError::DuplicateSubpatternName => 143,
            RegexError::MalformedProperty => 146,
            RegexError::UnknownProperty => 147,
            RegexError::SubpatternNameTooLong => 148,
            RegexError::TooManySubpatterns => 149,
            RegexError::InvalidOctalValue => 151,
            RegexError::TooManyBranchesInDefine => 154,
            RegexError::DefineRepetion => 155,
            RegexError::InconsistentNewlineOptions => 156,
            RegexError::MissingBackReference => 157,
            RegexError::InvalidRelativeReference => 158,
            RegexError::BacktrackingControlVerbArgumentForbidden => 159,
            RegexError::UnknownBacktrackingControlVerb => 160,
            RegexError::NumberTooBig => 161,
            RegexError::MissingSubpatternName => 162,
            RegexError::MissingDigit => 163,
            RegexError::InvalidDataCharacter => 164,
            RegexError::ExtraSubpatternName => 165,
            RegexError::BacktrackingControlVerbArgumentRequired => 166,
            RegexError::InvalidControlChar => 168,
            RegexError::MissingName => 169,
            RegexError::NotSupportedInClass => 171,
            RegexError::TooManyForwardReferences => 172,
            RegexError::NameTooLong => 175,
            RegexError::CharacterValueTooLarge => 176,
        }
    }
}

/// Regex compile flags (`GRegexCompileFlags`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct RegexCompileFlags(pub u32);

impl RegexCompileFlags {
    pub const DEFAULT: RegexCompileFlags = RegexCompileFlags(0);
    pub const CASELESS: RegexCompileFlags = RegexCompileFlags(1 << 0);
    pub const MULTILINE: RegexCompileFlags = RegexCompileFlags(1 << 1);
    pub const DOTALL: RegexCompileFlags = RegexCompileFlags(1 << 2);
    pub const EXTENDED: RegexCompileFlags = RegexCompileFlags(1 << 3);
    pub const ANCHORED: RegexCompileFlags = RegexCompileFlags(1 << 4);
    pub const DOLLAR_ENDONLY: RegexCompileFlags = RegexCompileFlags(1 << 5);
    pub const UNGREEDY: RegexCompileFlags = RegexCompileFlags(1 << 9);
    pub const RAW: RegexCompileFlags = RegexCompileFlags(1 << 11);
    pub const NO_AUTO_CAPTURE: RegexCompileFlags = RegexCompileFlags(1 << 12);
    pub const OPTIMIZE: RegexCompileFlags = RegexCompileFlags(1 << 13);
    pub const FIRSTLINE: RegexCompileFlags = RegexCompileFlags(1 << 18);
    pub const DUPNAMES: RegexCompileFlags = RegexCompileFlags(1 << 19);
    pub const NEWLINE_CR: RegexCompileFlags = RegexCompileFlags(1 << 20);
    pub const NEWLINE_LF: RegexCompileFlags = RegexCompileFlags(1 << 21);
    pub const NEWLINE_CRLF: RegexCompileFlags = RegexCompileFlags((1 << 20) | (1 << 21));
    pub const NEWLINE_ANYCRLF: RegexCompileFlags = RegexCompileFlags((1 << 20) | (1 << 22));
    pub const BSR_ANYCRLF: RegexCompileFlags = RegexCompileFlags(1 << 23);

    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl core::ops::BitOr for RegexCompileFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        RegexCompileFlags(self.0 | rhs.0)
    }
}

/// Regex match flags (`GRegexMatchFlags`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct RegexMatchFlags(pub u32);

impl RegexMatchFlags {
    pub const DEFAULT: RegexMatchFlags = RegexMatchFlags(0);
    pub const ANCHORED: RegexMatchFlags = RegexMatchFlags(1 << 4);
    pub const NOTBOL: RegexMatchFlags = RegexMatchFlags(1 << 7);
    pub const NOTEOL: RegexMatchFlags = RegexMatchFlags(1 << 8);
    pub const NOTEMPTY: RegexMatchFlags = RegexMatchFlags(1 << 10);
    pub const PARTIAL: RegexMatchFlags = RegexMatchFlags(1 << 15);
    pub const PARTIAL_SOFT: RegexMatchFlags = RegexMatchFlags(1 << 15);
    pub const PARTIAL_HARD: RegexMatchFlags = RegexMatchFlags(1 << 27);
    pub const NOTEMPTY_ATSTART: RegexMatchFlags = RegexMatchFlags(1 << 28);

    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl core::ops::BitOr for RegexMatchFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        RegexMatchFlags(self.0 | rhs.0)
    }
}

pub fn regex_error_quark() -> u32 {
    crate::quark::quark_from_string(Some("g-regex-error-quark"))
}

// ── AST ──────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
enum Node {
    Char(char),
    Any,
    Class { negated: bool, items: Vec<ClassItem> },
    StartAnchor,
    EndAnchor,
    WordBoundary,
    NonWordBoundary,
    Group { capture: bool, child: Box<Node> },
    Alternation(Vec<Node>),
    Sequence(Vec<Node>),
    Quantifier { child: Box<Node>, min: u32, max: Option<u32>, greedy: bool },
}

#[derive(Clone, Debug)]
enum ClassItem {
    Single(char),
    Range(char, char),
    Digit,
    NonDigit,
    Word,
    NonWord,
    Space,
    NonSpace,
}

// ── Parser ───────────────────────────────────────────────────────────

struct Parser<'a> {
    chars: core::iter::Peekable<core::str::Chars<'a>>,
    caseless: bool,
    capture_count: i32,
    max_backref: i32,
    has_cr_or_lf: bool,
}

impl<'a> Parser<'a> {
    fn new(pattern: &'a str, caseless: bool) -> Self {
        Self {
            chars: pattern.chars().peekable(),
            caseless,
            capture_count: 0,
            max_backref: 0,
            has_cr_or_lf: pattern.contains('\r') || pattern.contains('\n'),
        }
    }

    fn peek(&mut self) -> Option<char> { self.chars.peek().copied() }
    fn next(&mut self) -> Option<char> { self.chars.next() }

    fn parse(&mut self) -> Result<Node, RegexError> {
        let node = self.parse_alternation()?;
        if self.peek().is_some() { return Err(RegexError::UnmatchedParenthesis); }
        Ok(node)
    }

    fn parse_alternation(&mut self) -> Result<Node, RegexError> {
        let mut alts = Vec::new();
        alts.push(self.parse_sequence()?);
        while self.peek() == Some('|') {
            self.next();
            alts.push(self.parse_sequence()?);
        }
        if alts.len() == 1 { Ok(alts.into_iter().next().unwrap()) }
        else { Ok(Node::Alternation(alts)) }
    }

    fn parse_sequence(&mut self) -> Result<Node, RegexError> {
        let mut items = Vec::new();
        loop {
            match self.peek() {
                None | Some('|') | Some(')') => break,
                _ => {}
            }
            let atom = self.parse_atom()?;
            items.push(self.parse_quantifier(atom)?);
        }
        if items.len() == 1 { Ok(items.into_iter().next().unwrap()) }
        else { Ok(Node::Sequence(items)) }
    }

    fn parse_atom(&mut self) -> Result<Node, RegexError> {
        let c = self.next().ok_or(RegexError::Compile)?;
        match c {
            '.' => Ok(Node::Any),
            '^' => Ok(Node::StartAnchor),
            '$' => Ok(Node::EndAnchor),
            '(' => self.parse_group(),
            '[' => self.parse_class(),
            '\\' => self.parse_escape(),
            _ => Ok(Node::Char(if self.caseless { c.to_ascii_lowercase() } else { c })),
        }
    }

    fn parse_group(&mut self) -> Result<Node, RegexError> {
        let capture = if self.peek() == Some('?') {
            self.next();
            match self.peek() {
                Some(':') => { self.next(); false }
                Some('=') | Some('!') | Some('<') | Some('#') => {
                    let mut d = 1i32;
                    while d > 0 {
                        match self.next() {
                            Some('(') => d += 1,
                            Some(')') => d -= 1,
                            None => return Err(RegexError::UnmatchedParenthesis),
                            _ => {}
                        }
                    }
                    return Ok(Node::Sequence(Vec::new()));
                }
                _ => return Err(RegexError::Compile),
            }
        } else { true };
        if capture { self.capture_count += 1; }
        let child = self.parse_alternation()?;
        match self.next() {
            Some(')') => {}
            _ => return Err(RegexError::UnmatchedParenthesis),
        }
        Ok(Node::Group { capture, child: Box::new(child) })
    }

    fn parse_class(&mut self) -> Result<Node, RegexError> {
        let negated = self.peek() == Some('^');
        if negated { self.next(); }
        let mut items = Vec::new();
        if self.peek() == Some(']') { self.next(); items.push(ClassItem::Single(']')); }
        while let Some(c) = self.peek() {
            if c == ']' { self.next(); return Ok(Node::Class { negated, items }); }
            self.next();
            let item = if c == '\\' {
                self.parse_class_escape()?
            } else if self.peek() == Some('-') {
                let mut peek_iter = self.chars.clone();
                peek_iter.next();
                if let Some(&next) = peek_iter.peek() {
                    if next != ']' {
                        self.next();
                        let end = self.next().ok_or(RegexError::UnterminatedCharacterClass)?;
                        if self.caseless {
                            items.push(ClassItem::Range(c.to_ascii_lowercase(), end.to_ascii_lowercase()));
                        } else {
                            items.push(ClassItem::Range(c, end));
                        }
                        continue;
                    }
                }
                ClassItem::Single(if self.caseless { c.to_ascii_lowercase() } else { c })
            } else {
                ClassItem::Single(if self.caseless { c.to_ascii_lowercase() } else { c })
            };
            items.push(item);
        }
        Err(RegexError::UnterminatedCharacterClass)
    }

    fn parse_class_escape(&mut self) -> Result<ClassItem, RegexError> {
        let c = self.next().ok_or(RegexError::StrayBackslash)?;
        match c {
            'd' => Ok(ClassItem::Digit), 'D' => Ok(ClassItem::NonDigit),
            'w' => Ok(ClassItem::Word), 'W' => Ok(ClassItem::NonWord),
            's' => Ok(ClassItem::Space), 'S' => Ok(ClassItem::NonSpace),
            'n' => Ok(ClassItem::Single('\n')), 'r' => Ok(ClassItem::Single('\r')),
            't' => Ok(ClassItem::Single('\t')), '\\' => Ok(ClassItem::Single('\\')),
            ']' => Ok(ClassItem::Single(']')), '-' => Ok(ClassItem::Single('-')),
            _ => Ok(ClassItem::Single(if self.caseless { c.to_ascii_lowercase() } else { c })),
        }
    }

    fn parse_escape(&mut self) -> Result<Node, RegexError> {
        let c = self.next().ok_or(RegexError::StrayBackslash)?;
        match c {
            'd' => Ok(Node::Class { negated: true, items: vec![ClassItem::NonDigit] }),
            'D' => Ok(Node::Class { negated: true, items: vec![ClassItem::Digit] }),
            'w' => Ok(Node::Class { negated: true, items: vec![ClassItem::NonWord] }),
            'W' => Ok(Node::Class { negated: true, items: vec![ClassItem::Word] }),
            's' => Ok(Node::Class { negated: true, items: vec![ClassItem::NonSpace] }),
            'S' => Ok(Node::Class { negated: true, items: vec![ClassItem::Space] }),
            'b' => Ok(Node::WordBoundary), 'B' => Ok(Node::NonWordBoundary),
            'n' => Ok(Node::Char('\n')), 'r' => Ok(Node::Char('\r')),
            't' => Ok(Node::Char('\t')), '0' => Ok(Node::Char('\0')),
            _ => {
                if c.is_ascii_digit() {
                    let mut num = (c as u32) - ('0' as u32);
                    while let Some(&d) = self.chars.peek() {
                        if d.is_ascii_digit() { num = num * 10 + (d as u32 - '0' as u32); self.next(); }
                        else { break; }
                    }
                    if num > self.capture_count as u32 {
                        Ok(Node::Char(char::from_u32(num).unwrap_or('\0')))
                    } else {
                        if (num as i32) > self.max_backref { self.max_backref = num as i32; }
                        Ok(Node::Sequence(Vec::new()))
                    }
                } else {
                    Ok(Node::Char(if self.caseless { c.to_ascii_lowercase() } else { c }))
                }
            }
        }
    }

    fn parse_quantifier(&mut self, atom: Node) -> Result<Node, RegexError> {
        let (min, max) = match self.peek() {
            Some('*') => { self.next(); (0u32, None) }
            Some('+') => { self.next(); (1u32, None) }
            Some('?') => { self.next(); (0u32, Some(1u32)) }
            Some('{') => { self.next(); self.parse_brace_quantifier()? }
            _ => return Ok(atom),
        };
        let greedy = if self.peek() == Some('?') { self.next(); false } else { true };
        Ok(Node::Quantifier { child: Box::new(atom), min, max, greedy })
    }

    fn parse_brace_quantifier(&mut self) -> Result<(u32, Option<u32>), RegexError> {
        let mut min_str = String::new();
        while let Some(&c) = self.chars.peek() {
            if c.is_ascii_digit() { min_str.push(c); self.next(); } else { break; }
        }
        let min: u32 = min_str.parse().map_err(|_| RegexError::QuantifierTooBig)?;
        let max = if self.peek() == Some(',') {
            self.next();
            let mut max_str = String::new();
            while let Some(&c) = self.chars.peek() {
                if c.is_ascii_digit() { max_str.push(c); self.next(); } else { break; }
            }
            if max_str.is_empty() { None }
            else { Some(max_str.parse().map_err(|_| RegexError::QuantifierTooBig)?) }
        } else { Some(min) };
        match self.next() { Some('}') => {} _ => return Err(RegexError::QuantifiersOutOfOrder) }
        Ok((min, max))
    }
}

// ── Matcher ──────────────────────────────────────────────────────────

struct Matcher<'a> {
    input: &'a [char],
    caseless: bool,
    captures: Vec<Option<(usize, usize)>>,
    num_captures: usize,
}

impl<'a> Matcher<'a> {
    fn new(input: &'a [char], caseless: bool, num_captures: usize) -> Self {
        Self { input, caseless, captures: vec![None; num_captures + 1], num_captures }
    }

    fn char_eq(&self, a: char, b: char) -> bool {
        if self.caseless { a.to_ascii_lowercase() == b.to_ascii_lowercase() } else { a == b }
    }

    fn is_word_char(c: char) -> bool { c.is_ascii_alphanumeric() || c == '_' }

    fn match_class_item(&self, item: &ClassItem, c: char) -> bool {
        match item {
            ClassItem::Single(ch) => self.char_eq(*ch, c),
            ClassItem::Range(lo, hi) => {
                let c2 = if self.caseless { c.to_ascii_lowercase() } else { c };
                c2 >= *lo && c2 <= *hi
            }
            ClassItem::Digit => c.is_ascii_digit(),
            ClassItem::NonDigit => !c.is_ascii_digit(),
            ClassItem::Word => Self::is_word_char(c),
            ClassItem::NonWord => !Self::is_word_char(c),
            ClassItem::Space => c.is_whitespace(),
            ClassItem::NonSpace => !c.is_whitespace(),
        }
    }

    fn match_class(&self, negated: bool, items: &[ClassItem], c: char) -> bool {
        let matched = items.iter().any(|item| self.match_class_item(item, c));
        if negated { !matched } else { matched }
    }

    fn try_match(&mut self, node: &Node, pos: usize, captures: &mut Vec<Option<(usize, usize)>>) -> Vec<usize> {
        match node {
            Node::Char(ch) => if pos < self.input.len() && self.char_eq(*ch, self.input[pos]) { vec![pos + 1] } else { vec![] },
            Node::Any => if pos < self.input.len() && self.input[pos] != '\n' { vec![pos + 1] } else { vec![] },
            Node::Class { negated, items } => if pos < self.input.len() && self.match_class(*negated, items, self.input[pos]) { vec![pos + 1] } else { vec![] },
            Node::StartAnchor => if pos == 0 { vec![pos] } else { vec![] },
            Node::EndAnchor => if pos == self.input.len() { vec![pos] } else { vec![] },
            Node::WordBoundary => {
                let before = pos > 0 && Self::is_word_char(self.input[pos - 1]);
                let after = pos < self.input.len() && Self::is_word_char(self.input[pos]);
                if before != after { vec![pos] } else { vec![] }
            }
            Node::NonWordBoundary => {
                let before = pos > 0 && Self::is_word_char(self.input[pos - 1]);
                let after = pos < self.input.len() && Self::is_word_char(self.input[pos]);
                if before == after { vec![pos] } else { vec![] }
            }
            Node::Group { capture, child } => {
                let cap_idx = if *capture { let idx = captures.len(); captures.push(None); Some(idx) } else { None };
                let mut results = Vec::new();
                for end in self.try_match(child, pos, captures) {
                    if let Some(idx) = cap_idx { captures[idx] = Some((pos, end)); }
                    results.push(end);
                }
                results
            }
            Node::Alternation(alts) => {
                let mut results = Vec::new();
                for alt in alts {
                    let mut caps_copy = captures.clone();
                    let r = self.try_match(alt, pos, &mut caps_copy);
                    if !r.is_empty() { *captures = caps_copy; results.extend(r); }
                }
                results
            }
            Node::Sequence(items) => self.match_sequence(items, 0, pos, captures),
            Node::Quantifier { child, min, max, greedy } => self.match_quant_rec(child, *min, *max, *greedy, pos, 0, captures),
        }
    }

    fn match_sequence(&mut self, items: &[Node], idx: usize, pos: usize, captures: &mut Vec<Option<(usize, usize)>>) -> Vec<usize> {
        if idx >= items.len() { return vec![pos]; }
        let mut results = Vec::new();
        for end in self.try_match(&items[idx], pos, captures) {
            let mut caps_copy = captures.clone();
            let rest = self.match_sequence(items, idx + 1, end, &mut caps_copy);
            if !rest.is_empty() { *captures = caps_copy; results.extend(rest); }
        }
        results
    }

    fn match_quant_rec(&mut self, child: &Node, min: u32, max: Option<u32>, greedy: bool, pos: usize, count: u32, captures: &mut Vec<Option<(usize, usize)>>) -> Vec<usize> {
        let mut results = Vec::new();
        let can_more = max.map_or(true, |m| count < m);
        let min_ok = count >= min;
        if can_more {
            for end in self.try_match(child, pos, captures) {
                if end == pos && count >= min { continue; }
                let mut caps_copy = captures.clone();
                let rest = self.match_quant_rec(child, min, max, greedy, end, count + 1, &mut caps_copy);
                if !rest.is_empty() { *captures = caps_copy; results.extend(rest); }
            }
        }
        if min_ok { results.push(pos); }
        if !greedy { results.reverse(); }
        results
    }

    fn find(&mut self, node: &Node, start_pos: usize) -> Option<(usize, usize)> {
        for pos in start_pos..=self.input.len() {
            let mut captures = vec![None; self.num_captures + 1];
            if let Some(&end) = self.try_match(node, pos, &mut captures).first() {
                self.captures = captures;
                return Some((pos, end));
            }
        }
        None
    }
}

// ── Public API ───────────────────────────────────────────────────────

/// A compiled regular expression (`GRegex`).
pub struct Regex {
    pattern: String,
    ast: Node,
    compile_flags: RegexCompileFlags,
    match_flags: RegexMatchFlags,
    capture_count: i32,
    max_backref: i32,
    has_cr_or_lf: bool,
}

impl Regex {
    pub fn new(pattern: &str, compile_flags: RegexCompileFlags, match_flags: RegexMatchFlags) -> Result<Self, RegexError> {
        if pattern.is_empty() { return Err(RegexError::Compile); }
        let caseless = compile_flags.contains(RegexCompileFlags::CASELESS);
        let mut parser = Parser::new(pattern, caseless);
        let ast = parser.parse()?;
        Ok(Self {
            pattern: pattern.to_owned(), ast, compile_flags, match_flags,
            capture_count: parser.capture_count, max_backref: parser.max_backref,
            has_cr_or_lf: parser.has_cr_or_lf,
        })
    }

    pub fn get_pattern(&self) -> &str { &self.pattern }
    pub fn get_max_backref(&self) -> i32 { self.max_backref }
    pub fn get_capture_count(&self) -> i32 { self.capture_count }
    pub fn get_has_cr_or_lf(&self) -> bool { self.has_cr_or_lf }
    pub fn get_max_lookbehind(&self) -> i32 { 0 }
    pub fn get_compile_flags(&self) -> RegexCompileFlags { self.compile_flags }
    pub fn get_match_flags(&self) -> RegexMatchFlags { self.match_flags }
    pub fn get_string_number(&self, _name: &str) -> i32 { -1 }

    pub fn match_simple(pattern: &str, string: &str, compile_flags: RegexCompileFlags, match_flags: RegexMatchFlags) -> bool {
        match Regex::new(pattern, compile_flags, match_flags) {
            Ok(re) => re.r#match(string, match_flags).matches(),
            Err(_) => false,
        }
    }

    pub fn r#match(&self, string: &str, _match_flags: RegexMatchFlags) -> MatchInfo {
        let input: Vec<char> = string.chars().collect();
        let caseless = self.compile_flags.contains(RegexCompileFlags::CASELESS);
        let mut matcher = Matcher::new(&input, caseless, self.capture_count as usize);
        match matcher.find(&self.ast, 0) {
            Some((start, end)) => MatchInfo {
                regex_pattern: self.pattern.clone(), string: string.to_owned(), matched: true,
                match_count: self.capture_count + 1, start_pos: start, end_pos: end,
                is_partial: false, captures: matcher.captures, input_chars: input,
            },
            None => MatchInfo {
                regex_pattern: self.pattern.clone(), string: string.to_owned(), matched: false,
                match_count: 0, start_pos: 0, end_pos: 0, is_partial: false,
                captures: vec![None; self.capture_count as usize + 1], input_chars: input,
            },
        }
    }

    pub fn match_full(&self, string: &str, string_len: i64, start_position: i32, _match_flags: RegexMatchFlags) -> Result<MatchInfo, RegexError> {
        let s = if string_len >= 0 { &string[..(string_len as usize).min(string.len())] } else { string };
        let input: Vec<char> = s.chars().collect();
        let caseless = self.compile_flags.contains(RegexCompileFlags::CASELESS);
        let mut matcher = Matcher::new(&input, caseless, self.capture_count as usize);
        let start = (start_position as usize).min(input.len());
        match matcher.find(&self.ast, start) {
            Some((s_pos, e_pos)) => Ok(MatchInfo {
                regex_pattern: self.pattern.clone(), string: string.to_owned(), matched: true,
                match_count: self.capture_count + 1, start_pos: s_pos, end_pos: e_pos,
                is_partial: false, captures: matcher.captures, input_chars: input,
            }),
            None => Ok(MatchInfo {
                regex_pattern: self.pattern.clone(), string: string.to_owned(), matched: false,
                match_count: 0, start_pos: 0, end_pos: 0, is_partial: false,
                captures: vec![None; self.capture_count as usize + 1], input_chars: input,
            }),
        }
    }

    pub fn split(&self, string: &str, _match_flags: RegexMatchFlags) -> Vec<String> {
        let mut result = Vec::new();
        let input: Vec<char> = string.chars().collect();
        let caseless = self.compile_flags.contains(RegexCompileFlags::CASELESS);
        let mut matcher = Matcher::new(&input, caseless, self.capture_count as usize);
        let mut pos = 0;
        while pos < input.len() {
            if let Some((start, end)) = matcher.find(&self.ast, pos) {
                if start == end { result.push(input[pos..start].iter().collect()); pos = start + 1; }
                else { result.push(input[pos..start].iter().collect()); pos = end; }
            } else { break; }
        }
        result.push(input[pos..].iter().collect());
        result
    }

    pub fn replace(&self, string: &str, replacement: &str, _match_flags: RegexMatchFlags) -> String {
        let input: Vec<char> = string.chars().collect();
        let caseless = self.compile_flags.contains(RegexCompileFlags::CASELESS);
        let mut matcher = Matcher::new(&input, caseless, self.capture_count as usize);
        let mut result = String::new();
        let mut pos = 0;
        while pos < input.len() {
            if let Some((start, end)) = matcher.find(&self.ast, pos) {
                result.extend(input[pos..start].iter());
                let mut repl = replacement.chars().peekable();
                while let Some(c) = repl.next() {
                    if c == '$' || c == '\\' {
                        if let Some(&n) = repl.peek() {
                            if n.is_ascii_digit() {
                                repl.next();
                                let idx = (n as usize) - ('0' as usize);
                                if idx < matcher.captures.len() {
                                    if let Some((cs, ce)) = matcher.captures[idx] { result.extend(input[cs..ce].iter()); }
                                }
                                continue;
                            }
                        }
                    }
                    result.push(c);
                }
                if end == start { if start < input.len() { result.push(input[start]); } pos = end + 1; }
                else { pos = end; }
            } else { break; }
        }
        result.extend(input[pos..].iter());
        result
    }

    pub fn escape_string(string: &str, length: i32) -> String {
        let s = if length < 0 { string } else { &string[..(length as usize).min(string.len())] };
        let mut result = String::new();
        for c in s.chars() {
            match c {
                '\\' | '.' | '*' | '+' | '?' | '(' | ')' | '|' | '[' | ']' | '{' | '}' | '^' | '$' | '/' => { result.push('\\'); result.push(c); }
                _ => result.push(c),
            }
        }
        result
    }

    pub fn escape_nul(string: &str, length: i32) -> String {
        let s = if length < 0 { string } else { &string[..(length as usize).min(string.len())] };
        let mut result = String::new();
        for c in s.chars() { if c == '\0' { result.push_str("\\0"); } else { result.push(c); } }
        result
    }
}

/// Match info (`GMatchInfo`).
pub struct MatchInfo {
    regex_pattern: String,
    string: String,
    matched: bool,
    match_count: i32,
    start_pos: usize,
    end_pos: usize,
    is_partial: bool,
    captures: Vec<Option<(usize, usize)>>,
    input_chars: Vec<char>,
}

impl MatchInfo {
    pub fn matches(&self) -> bool { self.matched }
    pub fn get_match_count(&self) -> i32 { if self.matched { self.match_count } else { 0 } }
    pub fn is_partial_match(&self) -> bool { self.is_partial }
    pub fn get_regex_pattern(&self) -> &str { &self.regex_pattern }
    pub fn get_string(&self) -> &str { &self.string }

    pub fn fetch(&self, match_num: i32) -> Option<String> {
        let idx = match_num as usize;
        if idx >= self.captures.len() { return None; }
        self.captures[idx].map(|(s, e)| self.input_chars[s..e].iter().collect())
    }

    pub fn fetch_pos(&self, match_num: i32) -> Option<(i32, i32)> {
        let idx = match_num as usize;
        if idx >= self.captures.len() { return None; }
        self.captures[idx].map(|(s, e)| (s as i32, e as i32))
    }

    pub fn fetch_all(&self) -> Vec<String> {
        let mut result = Vec::new();
        for cap in &self.captures {
            if let Some((s, e)) = cap { result.push(self.input_chars[*s..*e].iter().collect()); }
        }
        result
    }

    pub fn next(&mut self) -> bool {
        if !self.matched { return false; }
        let start = if self.end_pos == self.start_pos { self.end_pos + 1 } else { self.end_pos };
        if start > self.input_chars.len() { self.matched = false; return false; }
        let re = match Regex::new(&self.regex_pattern, RegexCompileFlags::DEFAULT, RegexMatchFlags::DEFAULT) {
            Ok(r) => r, Err(_) => { self.matched = false; return false; }
        };
        let mut matcher = Matcher::new(&self.input_chars, false, re.capture_count as usize);
        match matcher.find(&re.ast, start) {
            Some((s, e)) => { self.start_pos = s; self.end_pos = e; self.captures = matcher.captures; true }
            None => { self.matched = false; false }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_match() {
        let re = Regex::new("hello", RegexCompileFlags::DEFAULT, RegexMatchFlags::DEFAULT).unwrap();
        let info = re.r#match("hello world", RegexMatchFlags::DEFAULT);
        assert!(info.matches());
        assert_eq!(info.fetch(0), Some("hello".to_owned()));
    }

    #[test]
    fn literal_no_match() {
        let re = Regex::new("xyz", RegexCompileFlags::DEFAULT, RegexMatchFlags::DEFAULT).unwrap();
        assert!(!re.r#match("hello world", RegexMatchFlags::DEFAULT).matches());
    }

    #[test]
    fn dot_match() {
        let re = Regex::new("h.llo", RegexCompileFlags::DEFAULT, RegexMatchFlags::DEFAULT).unwrap();
        assert!(re.r#match("hello", RegexMatchFlags::DEFAULT).matches());
    }

    #[test]
    fn star_match() {
        let re = Regex::new("ab*c", RegexCompileFlags::DEFAULT, RegexMatchFlags::DEFAULT).unwrap();
        assert!(re.r#match("ac", RegexMatchFlags::DEFAULT).matches());
        assert!(re.r#match("abc", RegexMatchFlags::DEFAULT).matches());
        assert!(re.r#match("abbbc", RegexMatchFlags::DEFAULT).matches());
        assert!(!re.r#match("adc", RegexMatchFlags::DEFAULT).matches());
    }

    #[test]
    fn plus_match() {
        let re = Regex::new("ab+c", RegexCompileFlags::DEFAULT, RegexMatchFlags::DEFAULT).unwrap();
        assert!(!re.r#match("ac", RegexMatchFlags::DEFAULT).matches());
        assert!(re.r#match("abc", RegexMatchFlags::DEFAULT).matches());
        assert!(re.r#match("abbbc", RegexMatchFlags::DEFAULT).matches());
    }

    #[test]
    fn question_match() {
        let re = Regex::new("colou?r", RegexCompileFlags::DEFAULT, RegexMatchFlags::DEFAULT).unwrap();
        assert!(re.r#match("color", RegexMatchFlags::DEFAULT).matches());
        assert!(re.r#match("colour", RegexMatchFlags::DEFAULT).matches());
        assert!(!re.r#match("colouur", RegexMatchFlags::DEFAULT).matches());
    }

    #[test]
    fn char_class() {
        let re = Regex::new("[abc]", RegexCompileFlags::DEFAULT, RegexMatchFlags::DEFAULT).unwrap();
        assert!(re.r#match("a", RegexMatchFlags::DEFAULT).matches());
        assert!(re.r#match("b", RegexMatchFlags::DEFAULT).matches());
        assert!(re.r#match("c", RegexMatchFlags::DEFAULT).matches());
        assert!(!re.r#match("d", RegexMatchFlags::DEFAULT).matches());
    }

    #[test]
    fn char_class_range() {
        let re = Regex::new("[a-z]", RegexCompileFlags::DEFAULT, RegexMatchFlags::DEFAULT).unwrap();
        assert!(re.r#match("m", RegexMatchFlags::DEFAULT).matches());
        assert!(!re.r#match("M", RegexMatchFlags::DEFAULT).matches());
    }

    #[test]
    fn negated_class() {
        let re = Regex::new("[^abc]", RegexCompileFlags::DEFAULT, RegexMatchFlags::DEFAULT).unwrap();
        assert!(!re.r#match("a", RegexMatchFlags::DEFAULT).matches());
        assert!(re.r#match("d", RegexMatchFlags::DEFAULT).matches());
    }

    #[test]
    fn alternation() {
        let re = Regex::new("cat|dog|bird", RegexCompileFlags::DEFAULT, RegexMatchFlags::DEFAULT).unwrap();
        assert!(re.r#match("cat", RegexMatchFlags::DEFAULT).matches());
        assert!(re.r#match("dog", RegexMatchFlags::DEFAULT).matches());
        assert!(re.r#match("bird", RegexMatchFlags::DEFAULT).matches());
        assert!(!re.r#match("fish", RegexMatchFlags::DEFAULT).matches());
    }

    #[test]
    fn anchors() {
        let re = Regex::new("^hello", RegexCompileFlags::DEFAULT, RegexMatchFlags::DEFAULT).unwrap();
        assert!(re.r#match("hello world", RegexMatchFlags::DEFAULT).matches());
        let re2 = Regex::new("world$", RegexCompileFlags::DEFAULT, RegexMatchFlags::DEFAULT).unwrap();
        assert!(re2.r#match("hello world", RegexMatchFlags::DEFAULT).matches());
        assert!(!re2.r#match("world hello", RegexMatchFlags::DEFAULT).matches());
    }

    #[test]
    fn digit_class() {
        let re = Regex::new("\\d+", RegexCompileFlags::DEFAULT, RegexMatchFlags::DEFAULT).unwrap();
        assert!(re.r#match("123", RegexMatchFlags::DEFAULT).matches());
        assert!(re.r#match("abc123def", RegexMatchFlags::DEFAULT).matches());
        assert!(!re.r#match("abc", RegexMatchFlags::DEFAULT).matches());
    }

    #[test]
    fn word_class() {
        let re = Regex::new("\\w+", RegexCompileFlags::DEFAULT, RegexMatchFlags::DEFAULT).unwrap();
        assert!(re.r#match("hello_123", RegexMatchFlags::DEFAULT).matches());
        assert!(!re.r#match("!!!", RegexMatchFlags::DEFAULT).matches());
    }

    #[test]
    fn space_class() {
        let re = Regex::new("\\s+", RegexCompileFlags::DEFAULT, RegexMatchFlags::DEFAULT).unwrap();
        assert!(re.r#match("  \t\n", RegexMatchFlags::DEFAULT).matches());
        assert!(!re.r#match("abc", RegexMatchFlags::DEFAULT).matches());
    }

    #[test]
    fn capture_group() {
        let re = Regex::new("(\\d+)-(\\d+)", RegexCompileFlags::DEFAULT, RegexMatchFlags::DEFAULT).unwrap();
        let info = re.r#match("123-456", RegexMatchFlags::DEFAULT);
        assert!(info.matches());
        assert_eq!(info.fetch(0), Some("123-456".to_owned()));
        assert_eq!(info.fetch(1), Some("123".to_owned()));
        assert_eq!(info.fetch(2), Some("456".to_owned()));
    }

    #[test]
    fn non_capturing_group() {
        let re = Regex::new("(?:ab)+c", RegexCompileFlags::DEFAULT, RegexMatchFlags::DEFAULT).unwrap();
        assert!(re.r#match("abc", RegexMatchFlags::DEFAULT).matches());
        assert!(re.r#match("ababc", RegexMatchFlags::DEFAULT).matches());
        assert!(!re.r#match("ac", RegexMatchFlags::DEFAULT).matches());
    }

    #[test]
    fn caseless() {
        let re = Regex::new("hello", RegexCompileFlags::CASELESS, RegexMatchFlags::DEFAULT).unwrap();
        assert!(re.r#match("HELLO", RegexMatchFlags::DEFAULT).matches());
        assert!(re.r#match("Hello", RegexMatchFlags::DEFAULT).matches());
        assert!(re.r#match("hello", RegexMatchFlags::DEFAULT).matches());
    }

    #[test]
    fn brace_quantifier() {
        let re = Regex::new("a{2,4}", RegexCompileFlags::DEFAULT, RegexMatchFlags::DEFAULT).unwrap();
        assert!(!re.r#match("a", RegexMatchFlags::DEFAULT).matches());
        assert!(re.r#match("aa", RegexMatchFlags::DEFAULT).matches());
        assert!(re.r#match("aaa", RegexMatchFlags::DEFAULT).matches());
        assert!(re.r#match("aaaa", RegexMatchFlags::DEFAULT).matches());
    }

    #[test]
    fn split() {
        let re = Regex::new("\\s+", RegexCompileFlags::DEFAULT, RegexMatchFlags::DEFAULT).unwrap();
        let parts = re.split("hello  world\tfoo", RegexMatchFlags::DEFAULT);
        assert_eq!(parts, vec!["hello", "world", "foo"]);
    }

    #[test]
    fn replace() {
        let re = Regex::new("\\d+", RegexCompileFlags::DEFAULT, RegexMatchFlags::DEFAULT).unwrap();
        let result = re.replace("abc123def456", "X", RegexMatchFlags::DEFAULT);
        assert_eq!(result, "abcXdefX");
    }

    #[test]
    fn replace_with_capture() {
        let re = Regex::new("(\\w+)", RegexCompileFlags::DEFAULT, RegexMatchFlags::DEFAULT).unwrap();
        let result = re.replace("hello world", "[$1]", RegexMatchFlags::DEFAULT);
        assert_eq!(result, "[hello] [world]");
    }

    #[test]
    fn word_boundary() {
        let re = Regex::new("\\bword\\b", RegexCompileFlags::DEFAULT, RegexMatchFlags::DEFAULT).unwrap();
        assert!(re.r#match("a word here", RegexMatchFlags::DEFAULT).matches());
        assert!(!re.r#match("awordhere", RegexMatchFlags::DEFAULT).matches());
    }

    #[test]
    fn match_simple() {
        assert!(Regex::match_simple("hello", "hello world", RegexCompileFlags::DEFAULT, RegexMatchFlags::DEFAULT));
        assert!(!Regex::match_simple("xyz", "hello world", RegexCompileFlags::DEFAULT, RegexMatchFlags::DEFAULT));
        assert!(Regex::match_simple("HELLO", "hello", RegexCompileFlags::CASELESS, RegexMatchFlags::DEFAULT));
    }

    #[test]
    fn escape_string() {
        assert_eq!(Regex::escape_string("a.b*c", -1), "a\\.b\\*c");
    }

    #[test]
    fn escape_nul() {
        assert_eq!(Regex::escape_nul("a\0b", -1), "a\\0b");
    }

    #[test]
    fn complex_pattern() {
        let re = Regex::new("(\\w+)@(\\w+)\\.(\\w+)", RegexCompileFlags::DEFAULT, RegexMatchFlags::DEFAULT).unwrap();
        let info = re.r#match("user@example.com", RegexMatchFlags::DEFAULT);
        assert!(info.matches());
        assert_eq!(info.fetch(1), Some("user".to_owned()));
        assert_eq!(info.fetch(2), Some("example".to_owned()));
        assert_eq!(info.fetch(3), Some("com".to_owned()));
    }

    #[test]
    fn lazy_quantifier() {
        let re = Regex::new("a.+?b", RegexCompileFlags::DEFAULT, RegexMatchFlags::DEFAULT).unwrap();
        let info = re.r#match("axbxb", RegexMatchFlags::DEFAULT);
        assert!(info.matches());
        assert_eq!(info.fetch(0), Some("axb".to_owned()));
    }

    #[test]
    fn greedy_quantifier() {
        let re = Regex::new("a.+b", RegexCompileFlags::DEFAULT, RegexMatchFlags::DEFAULT).unwrap();
        let info = re.r#match("axbxb", RegexMatchFlags::DEFAULT);
        assert!(info.matches());
        assert_eq!(info.fetch(0), Some("axbxb".to_owned()));
    }

    #[test]
    fn regex_error_to_code() {
        assert_eq!(RegexError::Compile.to_code(), 0);
        assert_eq!(RegexError::StrayBackslash.to_code(), 101);
        assert_eq!(RegexError::CharacterValueTooLarge.to_code(), 176);
    }

    #[test]
    fn flags_bitor() {
        let flags = RegexCompileFlags::CASELESS | RegexCompileFlags::MULTILINE;
        assert!(flags.contains(RegexCompileFlags::CASELESS));
        assert!(flags.contains(RegexCompileFlags::MULTILINE));
    }

    #[test]
    fn capture_count() {
        let re = Regex::new("(a)(b)(c)", RegexCompileFlags::DEFAULT, RegexMatchFlags::DEFAULT).unwrap();
        assert_eq!(re.get_capture_count(), 3);
    }

    #[test]
    fn non_capturing_count() {
        let re = Regex::new("(?:a)(b)", RegexCompileFlags::DEFAULT, RegexMatchFlags::DEFAULT).unwrap();
        assert_eq!(re.get_capture_count(), 1);
    }

    #[test]
    fn empty_pattern_fails() {
        assert!(Regex::new("", RegexCompileFlags::DEFAULT, RegexMatchFlags::DEFAULT).is_err());
    }

    #[test]
    fn unmatched_paren_fails() {
        assert!(Regex::new("(abc", RegexCompileFlags::DEFAULT, RegexMatchFlags::DEFAULT).is_err());
    }

    #[test]
    fn unterminated_class_fails() {
        assert!(Regex::new("[abc", RegexCompileFlags::DEFAULT, RegexMatchFlags::DEFAULT).is_err());
    }

    #[test]
    fn fetch_all() {
        let re = Regex::new("(\\d+)-(\\d+)", RegexCompileFlags::DEFAULT, RegexMatchFlags::DEFAULT).unwrap();
        let info = re.r#match("12-34", RegexMatchFlags::DEFAULT);
        let all = info.fetch_all();
        assert_eq!(all, vec!["12-34", "12", "34"]);
    }

    #[test]
    fn fetch_pos() {
        let re = Regex::new("world", RegexCompileFlags::DEFAULT, RegexMatchFlags::DEFAULT).unwrap();
        let info = re.r#match("hello world", RegexMatchFlags::DEFAULT);
        let pos = info.fetch_pos(0).unwrap();
        assert_eq!(pos.0, 6);
        assert_eq!(pos.1, 11);
    }
}
