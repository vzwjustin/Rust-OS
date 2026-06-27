//! Lexical scanner matching `gscanner.h` / `gscanner.c`.
//!
//! A flexible general-purpose lexical scanner. Supports identifiers, numbers
//! (binary, octal, hex, int, float), strings (single/double quoted), comments
//! (single/multi line), symbols, and scope-based symbol tables.
//! Fully `no_std` compatible using `alloc`.

use crate::prelude::*;
use alloc::collections::BTreeMap;

/// Character sets.
pub const CSET_A_2_Z: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
pub const CSET_a_2_z: &str = "abcdefghijklmnopqrstuvwxyz";
pub const CSET_DIGITS: &str = "0123456789";

/// Error types (`GErrorType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ErrorType {
    Unknown,
    UnexpEof,
    UnexpEofInString,
    UnexpEofInComment,
    NonDigitInConst,
    DigitRadix,
    FloatRadix,
    FloatMalformed,
}

/// Token types (`GTokenType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TokenType {
    Eof,
    LeftParen,
    RightParen,
    LeftCurly,
    RightCurly,
    LeftBrace,
    RightBrace,
    EqualSign,
    Comma,
    None,
    Error,
    Char,
    Binary,
    Octal,
    Int,
    Hex,
    Float,
    String,
    Symbol,
    Identifier,
    IdentifierNull,
    CommentSingle,
    CommentMulti,
}

/// Token value (`GTokenValue`).
#[derive(Clone, Debug, PartialEq)]
pub enum TokenValue {
    Symbol(usize),
    Identifier(String),
    Binary(u64),
    Octal(u64),
    Int(u64),
    Int64(u64),
    Float(f64),
    Hex(u64),
    String(String),
    Comment(String),
    Char(u8),
    Error(ErrorType),
    None,
}

/// Scanner configuration (`GScannerConfig`).
#[derive(Clone, Debug)]
pub struct ScannerConfig {
    pub cset_skip_characters: String,
    pub cset_identifier_first: String,
    pub cset_identifier_nth: String,
    pub cpair_comment_single: String,
    pub case_sensitive: bool,
    pub skip_comment_multi: bool,
    pub skip_comment_single: bool,
    pub scan_comment_multi: bool,
    pub scan_identifier: bool,
    pub scan_identifier_1char: bool,
    pub scan_identifier_null: bool,
    pub scan_symbols: bool,
    pub scan_binary: bool,
    pub scan_octal: bool,
    pub scan_float: bool,
    pub scan_hex: bool,
    pub scan_hex_dollar: bool,
    pub scan_string_sq: bool,
    pub scan_string_dq: bool,
    pub numbers_2_int: bool,
    pub int_2_float: bool,
    pub identifier_2_string: bool,
    pub char_2_token: bool,
    pub symbol_2_token: bool,
    pub scope_0_fallback: bool,
    pub store_int64: bool,
}

impl Default for ScannerConfig {
    fn default() -> Self {
        Self {
            cset_skip_characters: " \t\n".to_owned(),
            cset_identifier_first: format!("{}{}_", CSET_A_2_Z, CSET_a_2_z),
            cset_identifier_nth: format!("{}{}{}_", CSET_A_2_Z, CSET_a_2_z, CSET_DIGITS),
            cpair_comment_single: "#\n".to_owned(),
            case_sensitive: true,
            skip_comment_multi: true,
            skip_comment_single: true,
            scan_comment_multi: true,
            scan_identifier: true,
            scan_identifier_1char: false,
            scan_identifier_null: false,
            scan_symbols: true,
            scan_binary: true,
            scan_octal: true,
            scan_float: true,
            scan_hex: true,
            scan_hex_dollar: false,
            scan_string_sq: true,
            scan_string_dq: true,
            numbers_2_int: true,
            int_2_float: false,
            identifier_2_string: false,
            char_2_token: true,
            symbol_2_token: true,
            scope_0_fallback: false,
            store_int64: false,
        }
    }
}

type SymbolTable = BTreeMap<(u32, String), usize>;

/// A lexical scanner (`GScanner`).
pub struct Scanner {
    pub config: ScannerConfig,
    pub max_parse_errors: u32,
    pub parse_errors: u32,
    pub input_name: String,
    text: Vec<u8>,
    pos: usize,
    line: u32,
    column: u32,
    token: TokenType,
    value: TokenValue,
    next_token: TokenType,
    next_value: TokenValue,
    next_line: u32,
    next_position: u32,
    has_next: bool,
    symbols: SymbolTable,
    scope_id: u32,
}

impl Scanner {
    /// Create a new scanner (`g_scanner_new`).
    pub fn new(config: ScannerConfig) -> Self {
        Self {
            config,
            max_parse_errors: 0,
            parse_errors: 0,
            input_name: String::new(),
            text: Vec::new(),
            pos: 0,
            line: 1,
            column: 1,
            token: TokenType::None,
            value: TokenValue::None,
            next_token: TokenType::None,
            next_value: TokenValue::None,
            next_line: 1,
            next_position: 1,
            has_next: false,
            symbols: BTreeMap::new(),
            scope_id: 0,
        }
    }

    /// Set input text (`g_scanner_input_text`).
    pub fn input_text(&mut self, text: &str) {
        self.text = text.as_bytes().to_vec();
        self.pos = 0;
        self.line = 1;
        self.column = 1;
        self.token = TokenType::None;
        self.value = TokenValue::None;
        self.has_next = false;
    }

    /// Get the current token (`g_scanner_cur_token`).
    pub fn cur_token(&self) -> TokenType {
        self.token
    }

    /// Get the current value (`g_scanner_cur_value`).
    pub fn cur_value(&self) -> &TokenValue {
        &self.value
    }

    /// Get the current line (`g_scanner_cur_line`).
    pub fn cur_line(&self) -> u32 {
        self.line
    }

    /// Get the current position (`g_scanner_cur_position`).
    pub fn cur_position(&self) -> u32 {
        self.column
    }

    /// Check if at EOF (`g_scanner_eof`).
    pub fn eof(&self) -> bool {
        self.token == TokenType::Eof
    }

    /// Set the current scope (`g_scanner_set_scope`).
    pub fn set_scope(&mut self, scope_id: u32) -> u32 {
        let old = self.scope_id;
        self.scope_id = scope_id;
        old
    }

    /// Add a symbol to a scope (`g_scanner_scope_add_symbol`).
    pub fn scope_add_symbol(&mut self, scope_id: u32, symbol: &str, value: usize) {
        let key = if self.config.case_sensitive {
            (scope_id, symbol.to_owned())
        } else {
            (scope_id, symbol.to_lowercase())
        };
        self.symbols.insert(key, value);
    }

    /// Remove a symbol from a scope (`g_scanner_scope_remove_symbol`).
    pub fn scope_remove_symbol(&mut self, scope_id: u32, symbol: &str) {
        let key = if self.config.case_sensitive {
            (scope_id, symbol.to_owned())
        } else {
            (scope_id, symbol.to_lowercase())
        };
        self.symbols.remove(&key);
    }

    /// Look up a symbol in a scope (`g_scanner_scope_lookup_symbol`).
    pub fn scope_lookup_symbol(&self, scope_id: u32, symbol: &str) -> Option<usize> {
        let key = if self.config.case_sensitive {
            (scope_id, symbol.to_owned())
        } else {
            (scope_id, symbol.to_lowercase())
        };
        if let Some(v) = self.symbols.get(&key) {
            return Some(*v);
        }
        if self.config.scope_0_fallback && scope_id != 0 {
            let key0 = if self.config.case_sensitive {
                (0, symbol.to_owned())
            } else {
                (0, symbol.to_lowercase())
            };
            return self.symbols.get(&key0).copied();
        }
        None
    }

    /// Look up a symbol in the current scope (`g_scanner_lookup_symbol`).
    pub fn lookup_symbol(&self, symbol: &str) -> Option<usize> {
        self.scope_lookup_symbol(self.scope_id, symbol)
    }

    /// Peek at the next token (`g_scanner_peek_next_token`).
    pub fn peek_next_token(&mut self) -> TokenType {
        if !self.has_next {
            let saved_pos = self.pos;
            let saved_line = self.line;
            let saved_column = self.column;
            let (tok, val, line, pos) = self.scan_token();
            self.next_token = tok;
            self.next_value = val;
            self.next_line = line;
            self.next_position = pos;
            self.pos = saved_pos;
            self.line = saved_line;
            self.column = saved_column;
            self.has_next = true;
        }
        self.next_token
    }

    /// Get the next token (`g_scanner_get_next_token`).
    pub fn get_next_token(&mut self) -> TokenType {
        if self.has_next {
            self.token = self.next_token;
            self.value = core::mem::replace(&mut self.next_value, TokenValue::None);
            self.line = self.next_line;
            self.column = self.next_position;
            self.has_next = false;
        } else {
            let (tok, val, line, pos) = self.scan_token();
            self.token = tok;
            self.value = val;
            self.line = line;
            self.column = pos;
        }
        self.token
    }

    fn peek_byte(&self) -> Option<u8> {
        self.text.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<u8> {
        let b = self.peek_byte()?;
        self.pos += 1;
        if b == b'\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(b)
    }

    fn skip_whitespace(&mut self) {
        while let Some(b) = self.peek_byte() {
            if self.config.cset_skip_characters.as_bytes().contains(&b) {
                self.advance();
            } else if b == b'/' && self.text.get(self.pos + 1) == Some(&b'*') {
                // Multi-line comment
                if self.config.skip_comment_multi {
                    self.advance();
                    self.advance();
                    while let Some(b2) = self.advance() {
                        if b2 == b'*' && self.peek_byte() == Some(b'/') {
                            self.advance();
                            break;
                        }
                    }
                } else {
                    break;
                }
            } else if self.config.cpair_comment_single.len() == 2
                && b == self.config.cpair_comment_single.as_bytes()[0]
            {
                // Single-line comment
                if self.config.skip_comment_single {
                    self.advance();
                    let end_char = self.config.cpair_comment_single.as_bytes()[1];
                    while let Some(b2) = self.advance() {
                        if b2 == end_char {
                            break;
                        }
                    }
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    }

    fn scan_token(&mut self) -> (TokenType, TokenValue, u32, u32) {
        self.skip_whitespace();

        let start_line = self.line;
        let start_col = self.column;

        let b = match self.peek_byte() {
            Some(b) => b,
            None => return (TokenType::Eof, TokenValue::None, start_line, start_col),
        };

        // Single-char tokens
        match b {
            b'(' => { self.advance(); return (TokenType::LeftParen, TokenValue::None, start_line, start_col); }
            b')' => { self.advance(); return (TokenType::RightParen, TokenValue::None, start_line, start_col); }
            b'{' => { self.advance(); return (TokenType::LeftCurly, TokenValue::None, start_line, start_col); }
            b'}' => { self.advance(); return (TokenType::RightCurly, TokenValue::None, start_line, start_col); }
            b'[' => { self.advance(); return (TokenType::LeftBrace, TokenValue::None, start_line, start_col); }
            b']' => { self.advance(); return (TokenType::RightBrace, TokenValue::None, start_line, start_col); }
            b'=' => { self.advance(); return (TokenType::EqualSign, TokenValue::None, start_line, start_col); }
            b',' => { self.advance(); return (TokenType::Comma, TokenValue::None, start_line, start_col); }
            _ => {}
        }

        // Strings
        if b == b'\'' && self.config.scan_string_sq {
            self.advance();
            let s = self.scan_string(b'\'');
            return (TokenType::String, TokenValue::String(s), start_line, start_col);
        }
        if b == b'"' && self.config.scan_string_dq {
            self.advance();
            let s = self.scan_string(b'"');
            return (TokenType::String, TokenValue::String(s), start_line, start_col);
        }

        // Numbers
        if b.is_ascii_digit() || (b == b'.' && self.text.get(self.pos + 1).map_or(false, |c| c.is_ascii_digit())) {
            return self.scan_number(start_line, start_col);
        }

        // Hex with dollar
        if b == b'$' && self.config.scan_hex_dollar {
            if self.text.get(self.pos + 1).map_or(false, |c| c.is_ascii_hexdigit()) {
                self.advance();
                return self.scan_hex(start_line, start_col);
            }
        }

        // Identifiers and symbols
        if self.config.scan_identifier && self.is_identifier_first(b) {
            return self.scan_identifier(start_line, start_col);
        }

        // Single character token
        if self.config.char_2_token {
            self.advance();
            return (TokenType::Char, TokenValue::Char(b), start_line, start_col);
        }

        self.advance();
        (TokenType::Char, TokenValue::Char(b), start_line, start_col)
    }

    fn is_identifier_first(&self, b: u8) -> bool {
        self.config.cset_identifier_first.as_bytes().contains(&b)
    }

    fn is_identifier_nth(&self, b: u8) -> bool {
        self.config.cset_identifier_nth.as_bytes().contains(&b)
    }

    fn scan_string(&mut self, quote: u8) -> String {
        let mut result = String::new();
        while let Some(b) = self.peek_byte() {
            if b == quote {
                self.advance();
                break;
            }
            if b == b'\\' && quote == b'"' {
                self.advance();
                if let Some(esc) = self.advance() {
                    match esc {
                        b'n' => result.push('\n'),
                        b't' => result.push('\t'),
                        b'r' => result.push('\r'),
                        b'\\' => result.push('\\'),
                        b'"' => result.push('"'),
                        b'\'' => result.push('\''),
                        b'0' => result.push('\0'),
                        c => result.push(c as char),
                    }
                }
            } else {
                self.advance();
                result.push(b as char);
            }
        }
        result
    }

    fn scan_number(&mut self, line: u32, col: u32) -> (TokenType, TokenValue, u32, u32) {
        let start = self.pos;
        let b = self.peek_byte().unwrap();

        // Check for 0x (hex), 0b (binary), 0 (octal)
        if b == b'0' {
            if let Some(b1) = self.text.get(self.pos + 1) {
                if *b1 == b'x' || *b1 == b'X' {
                    if self.config.scan_hex {
                        self.advance();
                        self.advance();
                        return self.scan_hex(line, col);
                    }
                }
                if *b1 == b'b' || *b1 == b'B' {
                    if self.config.scan_binary {
                        self.advance();
                        self.advance();
                        return self.scan_binary(line, col);
                    }
                }
            }
            // Octal: leading 0, next char is an octal digit (not float/exp)
            if self.config.scan_octal {
                if let Some(b1) = self.text.get(self.pos + 1) {
                    if b1 != &b'.' && b1 != &b'e' && b1 != &b'E' && (*b1 >= b'0' && *b1 <= b'7') {
                        return self.scan_octal(line, col);
                    }
                }
            }
        }

        // Decimal int or float
        let mut is_float = false;
        while let Some(b) = self.peek_byte() {
            if b.is_ascii_digit() {
                self.advance();
            } else if b == b'.' {
                is_float = true;
                self.advance();
            } else if b == b'e' || b == b'E' {
                is_float = true;
                self.advance();
                if let Some(b2) = self.peek_byte() {
                    if b2 == b'+' || b2 == b'-' {
                        self.advance();
                    }
                }
            } else {
                break;
            }
        }

        let s = core::str::from_utf8(&self.text[start..self.pos]).unwrap_or("");
        if is_float && self.config.scan_float {
            let v: f64 = s.parse().unwrap_or(0.0);
            (TokenType::Float, TokenValue::Float(v), line, col)
        } else if self.config.int_2_float {
            let v: f64 = s.parse().unwrap_or(0.0);
            (TokenType::Float, TokenValue::Float(v), line, col)
        } else {
            let v: u64 = s.parse().unwrap_or(0);
            (TokenType::Int, TokenValue::Int(v), line, col)
        }
    }

    fn scan_hex(&mut self, line: u32, col: u32) -> (TokenType, TokenValue, u32, u32) {
        let start = self.pos;
        while let Some(b) = self.peek_byte() {
            if b.is_ascii_hexdigit() {
                self.advance();
            } else {
                break;
            }
        }
        let s = core::str::from_utf8(&self.text[start..self.pos]).unwrap_or("");
        let v = u64::from_str_radix(s, 16).unwrap_or(0);
        (TokenType::Hex, TokenValue::Hex(v), line, col)
    }

    fn scan_binary(&mut self, line: u32, col: u32) -> (TokenType, TokenValue, u32, u32) {
        let start = self.pos;
        while let Some(b) = self.peek_byte() {
            if b == b'0' || b == b'1' {
                self.advance();
            } else {
                break;
            }
        }
        let s = core::str::from_utf8(&self.text[start..self.pos]).unwrap_or("");
        let v = u64::from_str_radix(s, 2).unwrap_or(0);
        (TokenType::Binary, TokenValue::Binary(v), line, col)
    }

    fn scan_octal(&mut self, line: u32, col: u32) -> (TokenType, TokenValue, u32, u32) {
        let start = self.pos;
        while let Some(b) = self.peek_byte() {
            if b >= b'0' && b <= b'7' {
                self.advance();
            } else {
                break;
            }
        }
        let s = core::str::from_utf8(&self.text[start..self.pos]).unwrap_or("");
        let v = u64::from_str_radix(s, 8).unwrap_or(0);
        (TokenType::Octal, TokenValue::Octal(v), line, col)
    }

    fn scan_identifier(&mut self, line: u32, col: u32) -> (TokenType, TokenValue, u32, u32) {
        let start = self.pos;
        self.advance();
        while let Some(b) = self.peek_byte() {
            if self.is_identifier_nth(b) {
                self.advance();
            } else {
                break;
            }
        }
        let s = core::str::from_utf8(&self.text[start..self.pos]).unwrap_or("").to_owned();

        // Check for NULL identifier
        if self.config.scan_identifier_null && s == "NULL" {
            return (TokenType::IdentifierNull, TokenValue::Identifier(s), line, col);
        }

        // Check for symbol
        if self.config.scan_symbols {
            if let Some(val) = self.scope_lookup_symbol(self.scope_id, &s) {
                if self.config.symbol_2_token {
                    return (TokenType::Symbol, TokenValue::Symbol(val), line, col);
                }
            }
        }

        if self.config.identifier_2_string {
            (TokenType::String, TokenValue::String(s), line, col)
        } else {
            (TokenType::Identifier, TokenValue::Identifier(s), line, col)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_integers() {
        let mut s = Scanner::new(ScannerConfig::default());
        s.input_text("42 0 255");
        assert_eq!(s.get_next_token(), TokenType::Int);
        assert_eq!(s.cur_value().clone(), TokenValue::Int(42));
        assert_eq!(s.get_next_token(), TokenType::Int);
        assert_eq!(s.get_next_token(), TokenType::Int);
        assert_eq!(s.cur_value().clone(), TokenValue::Int(255));
    }

    #[test]
    fn scan_hex() {
        let mut s = Scanner::new(ScannerConfig::default());
        s.input_text("0xFF 0xDEAD");
        assert_eq!(s.get_next_token(), TokenType::Hex);
        assert_eq!(s.cur_value().clone(), TokenValue::Hex(0xFF));
        assert_eq!(s.get_next_token(), TokenType::Hex);
        assert_eq!(s.cur_value().clone(), TokenValue::Hex(0xDEAD));
    }

    #[test]
    fn scan_float() {
        let mut s = Scanner::new(ScannerConfig::default());
        s.input_text("3.14 1e10");
        assert_eq!(s.get_next_token(), TokenType::Float);
        if let TokenValue::Float(v) = s.cur_value() {
            assert!((v - 3.14).abs() < 0.001);
        }
        assert_eq!(s.get_next_token(), TokenType::Float);
    }

    #[test]
    fn scan_strings() {
        let mut s = Scanner::new(ScannerConfig::default());
        s.input_text("'hello' \"world\"");
        assert_eq!(s.get_next_token(), TokenType::String);
        assert_eq!(s.cur_value().clone(), TokenValue::String("hello".to_owned()));
        assert_eq!(s.get_next_token(), TokenType::String);
        assert_eq!(s.cur_value().clone(), TokenValue::String("world".to_owned()));
    }

    #[test]
    fn scan_identifiers() {
        let mut s = Scanner::new(ScannerConfig::default());
        s.input_text("foo bar_123 _baz");
        assert_eq!(s.get_next_token(), TokenType::Identifier);
        assert_eq!(s.cur_value().clone(), TokenValue::Identifier("foo".to_owned()));
        assert_eq!(s.get_next_token(), TokenType::Identifier);
        assert_eq!(s.get_next_token(), TokenType::Identifier);
    }

    #[test]
    fn scan_symbols() {
        let mut s = Scanner::new(ScannerConfig::default());
        s.scope_add_symbol(0, "if", 1);
        s.scope_add_symbol(0, "else", 2);
        s.input_text("if else foo");
        assert_eq!(s.get_next_token(), TokenType::Symbol);
        assert_eq!(s.cur_value().clone(), TokenValue::Symbol(1));
        assert_eq!(s.get_next_token(), TokenType::Symbol);
        assert_eq!(s.cur_value().clone(), TokenValue::Symbol(2));
        assert_eq!(s.get_next_token(), TokenType::Identifier);
    }

    #[test]
    fn scan_punctuation() {
        let mut s = Scanner::new(ScannerConfig::default());
        s.input_text("(){}=[],");
        assert_eq!(s.get_next_token(), TokenType::LeftParen);
        assert_eq!(s.get_next_token(), TokenType::RightParen);
        assert_eq!(s.get_next_token(), TokenType::LeftCurly);
        assert_eq!(s.get_next_token(), TokenType::RightCurly);
        assert_eq!(s.get_next_token(), TokenType::EqualSign);
        assert_eq!(s.get_next_token(), TokenType::LeftBrace);
        assert_eq!(s.get_next_token(), TokenType::RightBrace);
        assert_eq!(s.get_next_token(), TokenType::Comma);
    }

    #[test]
    fn skip_comments() {
        let mut s = Scanner::new(ScannerConfig::default());
        s.input_text("/* comment */ 42");
        assert_eq!(s.get_next_token(), TokenType::Int);
        assert_eq!(s.cur_value().clone(), TokenValue::Int(42));
    }

    #[test]
    fn skip_single_comments() {
        let mut config = ScannerConfig::default();
        config.cpair_comment_single = "#\n".to_owned();
        let mut s = Scanner::new(config);
        s.input_text("# comment\n42");
        assert_eq!(s.get_next_token(), TokenType::Int);
        assert_eq!(s.cur_value().clone(), TokenValue::Int(42));
    }

    #[test]
    fn peek_next_token() {
        let mut s = Scanner::new(ScannerConfig::default());
        s.input_text("42 100");
        assert_eq!(s.peek_next_token(), TokenType::Int);
        assert_eq!(s.get_next_token(), TokenType::Int);
        assert_eq!(s.cur_value().clone(), TokenValue::Int(42));
        assert_eq!(s.get_next_token(), TokenType::Int);
        assert_eq!(s.cur_value().clone(), TokenValue::Int(100));
    }

    #[test]
    fn eof() {
        let mut s = Scanner::new(ScannerConfig::default());
        s.input_text("42");
        s.get_next_token();
        s.get_next_token();
        assert!(s.eof());
    }

    #[test]
    fn line_tracking() {
        let mut s = Scanner::new(ScannerConfig::default());
        s.input_text("foo\nbar");
        s.get_next_token();
        assert_eq!(s.cur_line(), 1);
        s.get_next_token();
        assert_eq!(s.cur_line(), 2);
    }

    #[test]
    fn binary_octal() {
        let mut s = Scanner::new(ScannerConfig::default());
        s.input_text("0b1010 0777");
        assert_eq!(s.get_next_token(), TokenType::Binary);
        assert_eq!(s.cur_value().clone(), TokenValue::Binary(10));
        assert_eq!(s.get_next_token(), TokenType::Octal);
        assert_eq!(s.cur_value().clone(), TokenValue::Octal(511));
    }

    #[test]
    fn scope_fallback() {
        let mut config = ScannerConfig::default();
        config.scope_0_fallback = true;
        let mut s = Scanner::new(config);
        s.scope_add_symbol(0, "global", 99);
        s.set_scope(1);
        assert_eq!(s.lookup_symbol("global"), Some(99));
    }
}
