//! Glob pattern matching matching `gpattern.h` / `gpattern.c`.
//!
//! Supports `*` (match any sequence) and `?` (match any single character).
//! Fully `no_std` compatible.

use crate::prelude::*;

/// A compiled glob pattern (`GPatternSpec`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatternSpec {
    pattern: String,
    /// Pattern parts: prefix, middle, suffix (for optimized matching).
    match_type: MatchType,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MatchType {
    /// No wildcards - exact match.
    Exact,
    /// Only `*` at the end: prefix*.
    Prefix,
    /// Only `*` at the beginning: *suffix.
    Suffix,
    /// General glob pattern.
    Glob,
}

impl PatternSpec {
    /// Create a new pattern spec (`g_pattern_spec_new`).
    pub fn new(pattern: &str) -> Self {
        let has_wildcards = pattern.contains('*') || pattern.contains('?');

        let match_type = if !has_wildcards {
            MatchType::Exact
        } else if pattern.starts_with('*')
            && !pattern[1..].contains('*')
            && !pattern[1..].contains('?')
        {
            MatchType::Suffix
        } else if pattern.ends_with('*')
            && !pattern[..pattern.len() - 1].contains('*')
            && !pattern[..pattern.len() - 1].contains('?')
        {
            MatchType::Prefix
        } else {
            MatchType::Glob
        };

        Self {
            pattern: pattern.to_owned(),
            match_type,
        }
    }

    /// Check if this pattern matches `string` (`g_pattern_spec_match_string`).
    pub fn match_string(&self, string: &str) -> bool {
        match self.match_type {
            MatchType::Exact => string == self.pattern,
            MatchType::Prefix => string.starts_with(&self.pattern[..self.pattern.len() - 1]),
            MatchType::Suffix => string.ends_with(&self.pattern[1..]),
            MatchType::Glob => glob_match(&self.pattern, string),
        }
    }

    /// Check if this pattern matches a string of known length
    /// (`g_pattern_spec_match`).
    pub fn matches(
        &self,
        string_length: usize,
        string: &str,
        string_reversed: Option<&str>,
    ) -> bool {
        let _ = string_length;
        let _ = string_reversed;
        self.match_string(string)
    }

    /// Compare two pattern specs for equality (`g_pattern_spec_equal`).
    pub fn equal(&self, other: &PatternSpec) -> bool {
        self.pattern == other.pattern
    }

    /// Copy this pattern spec (`g_pattern_spec_copy`).
    pub fn copy(&self) -> PatternSpec {
        self.clone()
    }
}

/// Match a glob pattern with `*` and `?` against a string.
fn glob_match(pattern: &str, string: &str) -> bool {
    let p = pattern.as_bytes();
    let s = string.as_bytes();
    glob_match_bytes(p, s)
}

fn glob_match_bytes(p: &[u8], s: &[u8]) -> bool {
    let mut pi = 0;
    let mut si = 0;
    let mut star_pi: Option<usize> = None;
    let mut star_si = 0;

    while si < s.len() {
        if pi < p.len() && (p[pi] == s[si] || p[pi] == b'?') {
            pi += 1;
            si += 1;
        } else if pi < p.len() && p[pi] == b'*' {
            star_pi = Some(pi);
            star_si = si;
            pi += 1;
        } else if let Some(spi) = star_pi {
            pi = spi + 1;
            star_si += 1;
            si = star_si;
        } else {
            return false;
        }
    }

    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }

    pi == p.len()
}

/// Simple one-shot pattern match (`g_pattern_match_simple`).
pub fn pattern_match_simple(pattern: &str, string: &str) -> bool {
    let pspec = PatternSpec::new(pattern);
    pspec.match_string(string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match() {
        let p = PatternSpec::new("hello");
        assert!(p.match_string("hello"));
        assert!(!p.match_string("hell"));
        assert!(!p.match_string("helloo"));
    }

    #[test]
    fn prefix_match() {
        let p = PatternSpec::new("hel*");
        assert!(p.match_string("hello"));
        assert!(p.match_string("help"));
        assert!(p.match_string("hel"));
        assert!(!p.match_string("he"));
    }

    #[test]
    fn suffix_match() {
        let p = PatternSpec::new("*lo");
        assert!(p.match_string("hello"));
        assert!(p.match_string("lo"));
        assert!(!p.match_string("helloo"));
    }

    #[test]
    fn glob_match() {
        let p = PatternSpec::new("h*o");
        assert!(p.match_string("hello"));
        assert!(p.match_string("ho"));
        assert!(!p.match_string("hell"));

        let p = PatternSpec::new("?ello");
        assert!(p.match_string("hello"));
        assert!(p.match_string("jello"));
        assert!(!p.match_string("ello"));
        assert!(!p.match_string("helloo"));
    }

    #[test]
    fn multiple_stars() {
        let p = PatternSpec::new("*.*");
        assert!(p.match_string("file.txt"));
        assert!(p.match_string("a.b"));
        assert!(p.match_string("."));
        assert!(!p.match_string("file"));

        let p = PatternSpec::new("a*b*c");
        assert!(p.match_string("abc"));
        assert!(p.match_string("aXXbYYc"));
        assert!(!p.match_string("ac"));
    }

    #[test]
    fn question_marks() {
        let p = PatternSpec::new("??");
        assert!(p.match_string("ab"));
        assert!(p.match_string("xy"));
        assert!(!p.match_string("a"));
        assert!(!p.match_string("abc"));
    }

    #[test]
    fn simple_match() {
        assert!(pattern_match_simple("*.txt", "file.txt"));
        assert!(!pattern_match_simple("*.txt", "file.rs"));
        assert!(pattern_match_simple("hello", "hello"));
    }

    #[test]
    fn pattern_equal() {
        let p1 = PatternSpec::new("*.txt");
        let p2 = PatternSpec::new("*.txt");
        let p3 = PatternSpec::new("*.rs");
        assert!(p1.equal(&p2));
        assert!(!p1.equal(&p3));
    }

    #[test]
    fn pattern_copy() {
        let p1 = PatternSpec::new("test*");
        let p2 = p1.copy();
        assert!(p1.equal(&p2));
    }
}
