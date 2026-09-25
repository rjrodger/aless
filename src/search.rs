//! Search patterns, jless style: regular expressions with smart case, a
//! `/s` suffix to force case sensitivity, and square/curly brackets taken
//! literally unless escaped.

use regex::{Regex, RegexBuilder};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Forward,
    Backward,
}

impl Direction {
    pub fn prompt(self) -> char {
        match self {
            Direction::Forward => '/',
            Direction::Backward => '?',
        }
    }

    pub fn reverse(self) -> Direction {
        match self {
            Direction::Forward => Direction::Backward,
            Direction::Backward => Direction::Forward,
        }
    }
}

/// A compiled search: the pattern as typed, and the regex it became.
#[derive(Clone, Debug)]
pub struct Pattern {
    pub input: String,
    pub regex: Regex,
}

/// Compile a search line as typed after `/` or `?`.
///
/// - `pat/s` forces case sensitivity; a single trailing `/` is dropped and
///   `//` stands for a literal `/`.
/// - Smart case: the search is case-insensitive unless the pattern has an
///   uppercase letter.
/// - Unescaped `[ ] { }` match themselves; write `\[` for a regex class.
pub fn compile(input: &str) -> Result<Pattern, String> {
    let (body, forced) = strip_suffix(input);
    let sensitive = forced || body.chars().any(char::is_uppercase);
    let regex = RegexBuilder::new(&escape_brackets(&body))
        .case_insensitive(!sensitive)
        .size_limit(1 << 24)
        .build()
        .map_err(|e| {
            let msg = e.to_string();
            let msg = msg.lines().last().unwrap_or("").trim();
            format!("Invalid regex: {msg}")
        })?;
    Ok(Pattern {
        input: input.to_string(),
        regex,
    })
}

/// A pattern that matches the object key `key` exactly, in the line-mode
/// text search runs over (`"key": …`). Used by `*` and `#`.
pub fn key_pattern(key: &str) -> Pattern {
    let body = format!("\"{}\":", regex::escape(key));
    let regex = RegexBuilder::new(&body)
        .case_insensitive(false)
        .build()
        .expect("escaped key is a valid regex");
    Pattern {
        input: format!("{}/s", crate::fmt::quote(key)),
        regex,
    }
}

fn strip_suffix(input: &str) -> (String, bool) {
    if let Some(body) = input.strip_suffix("/s") {
        return (body.to_string(), true);
    }
    if let Some(body) = input.strip_suffix("//") {
        return (format!("{body}/"), false);
    }
    if let Some(body) = input.strip_suffix('/') {
        return (body.to_string(), false);
    }
    (input.to_string(), false)
}

fn escape_brackets(pat: &str) -> String {
    let mut out = String::with_capacity(pat.len() + 8);
    let mut chars = pat.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.peek() {
                Some(&n) if "[]{}".contains(n) => {
                    chars.next();
                    out.push(n);
                }
                Some(&n) => {
                    chars.next();
                    out.push('\\');
                    out.push(n);
                }
                None => out.push('\\'),
            },
            '[' | ']' | '{' | '}' => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smart_case() {
        assert!(compile("abc").unwrap().regex.is_match("xABCx"));
        assert!(!compile("Abc").unwrap().regex.is_match("xabcx"));
        assert!(!compile("abc/s").unwrap().regex.is_match("xABCx"));
        assert!(compile("abc/").unwrap().regex.is_match("ABC"));
    }

    #[test]
    fn slashes() {
        assert_eq!(strip_suffix("a//"), ("a/".to_string(), false));
        assert_eq!(strip_suffix("a/"), ("a".to_string(), false));
        assert_eq!(strip_suffix("a/s"), ("a".to_string(), true));
        assert!(compile("a//").unwrap().regex.is_match("a/"));
    }

    #[test]
    fn brackets_are_literal_unless_escaped() {
        assert_eq!(escape_brackets("[1]"), r"\[1\]");
        assert_eq!(escape_brackets(r"\[a-z\]"), "[a-z]");
        assert_eq!(escape_brackets(r"\d{2}"), r"\d\{2\}");
        assert_eq!(escape_brackets(r"\\"), r"\\");
        assert!(compile("[0]").unwrap().regex.is_match("x[0]"));
        assert!(compile(r"\[abc\]").unwrap().regex.is_match("b"));
    }

    #[test]
    fn errors_are_one_line() {
        let e = compile("(").unwrap_err();
        assert!(e.starts_with("Invalid regex:"), "{e}");
        assert!(!e.contains('\n'));
    }

    #[test]
    fn key_patterns() {
        let p = key_pattern("a.b");
        assert!(p.regex.is_match("\"a.b\": 1"));
        assert!(!p.regex.is_match("\"axb\": 1"));
        assert!(!p.regex.is_match("\"A.b\": 1"));
        assert!(!p.regex.is_match("\"x\": \"a.b\""));
    }
}
