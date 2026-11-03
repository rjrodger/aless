//! Text forms of keys, values, paths and subtrees: what the tree pane
//! shows, what search runs over, and what yank puts on the clipboard.

use crate::doc::{Doc, Key, Kind, NodeId};

/// A number the way JavaScript would print it: integers without a
/// fraction, everything else in the shortest round-trip form.
pub fn number(n: f64) -> String {
    if n.is_nan() {
        return "NaN".to_string();
    }
    if n.is_infinite() {
        return if n > 0.0 { "Infinity" } else { "-Infinity" }.to_string();
    }
    if n.fract() == 0.0 && n.abs() < 1e16 {
        return format!("{}", n as i64);
    }
    match serde_json::Number::from_f64(n) {
        Some(x) => x.to_string(),
        None => n.to_string(),
    }
}

/// The source's spelling of the number `n`, when the viewer keeps it: a
/// number in JSON's syntax that denotes `n` exactly and that [`number`]
/// writes otherwise, as it does `1.0`, `-0`, `1e21` and an integer past
/// 2^53. `None` for the spelling [`number`] writes (`55`, `0.5`), and for
/// one JSON has no syntax for (`0x1F`, `1_000`, `+1`, `.5`): such a number
/// is shown as [`number`] writes its value, and the source view has the
/// spelling.
pub fn spelling(text: &str, n: f64) -> Option<&str> {
    let text = text.trim();
    if !is_json_number(text) {
        return None;
    }
    // Most numbers are short integers, which `number` writes as spelled:
    // up to 15 digits is below 2^53, and JSON has no leading zero.
    let digits = text.strip_prefix('-').unwrap_or(text);
    if digits.len() <= 15 && digits.bytes().all(|b| b.is_ascii_digit()) && text != "-0" {
        return None;
    }
    // Most of the rest (`0.5`, `42.25`) are spelled as `number` writes
    // them too, which saves reading the text back.
    if number(n) == text {
        return None;
    }
    text.parse::<f64>()
        .is_ok_and(|v| v.to_bits() == n.to_bits())
        .then_some(text)
}

/// Whether `text` is a number in JSON's syntax (RFC 8259, section 6): a
/// minus sign or none, an integer part with no leading zero, then a
/// fraction and an exponent, each optional.
fn is_json_number(text: &str) -> bool {
    fn digits(b: &[u8], i: &mut usize) -> usize {
        let start = *i;
        while b.get(*i).is_some_and(u8::is_ascii_digit) {
            *i += 1;
        }
        *i - start
    }
    let b = text.as_bytes();
    let mut i = usize::from(b.first() == Some(&b'-'));
    match b.get(i) {
        Some(b'0') => i += 1,
        Some(c) if c.is_ascii_digit() => {
            digits(b, &mut i);
        }
        _ => return false,
    }
    if b.get(i) == Some(&b'.') {
        i += 1;
        if digits(b, &mut i) == 0 {
            return false;
        }
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(b.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        if digits(b, &mut i) == 0 {
            return false;
        }
    }
    i == b.len()
}

/// A JSON string literal.
pub fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 || c == '\u{7f}' => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Would this key be a valid JavaScript identifier? (jless shows such keys
/// unquoted in data mode.)
pub fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' || c == '$' => {}
        _ => return false,
    }
    chars.all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}

/// The display form of a key. Object keys are bare in data mode when they
/// are identifiers and quoted otherwise; line mode always quotes. Array
/// indices and the root have no key text.
pub fn key_text(key: &Key, line_mode: bool) -> Option<String> {
    match key {
        Key::Name(n) => Some(if !line_mode && is_identifier(n) {
            n.to_string()
        } else {
            quote(n)
        }),
        _ => None,
    }
}

/// The display form of a scalar.
pub fn leaf_text(kind: &Kind) -> String {
    match kind {
        Kind::Null => "null".to_string(),
        Kind::Bool(b) => b.to_string(),
        Kind::Number(n) => number(*n),
        Kind::Str(s) => quote(s),
        Kind::Object => "{".to_string(),
        Kind::Array => "[".to_string(),
    }
}

/// The display form of the scalar at `id`: [`leaf_text`], with a number
/// its source spelled otherwise in JSON's syntax kept as spelled
/// ([`Doc::spelling`]).
pub fn value_text(doc: &Doc, id: NodeId) -> String {
    match doc.spelling(id) {
        Some(s) => s.to_string(),
        None => leaf_text(&doc.node(id).kind),
    }
}

pub fn open_bracket(kind: &Kind) -> &'static str {
    match kind {
        Kind::Array => "[",
        _ => "{",
    }
}

pub fn close_bracket(kind: &Kind) -> &'static str {
    match kind {
        Kind::Array => "]",
        _ => "}",
    }
}

/// The jless-style preview of a collapsed container: its item count, then
/// as many leading items as fit in `budget` characters, then `…`.
pub fn preview(doc: &Doc, id: NodeId, budget: usize) -> String {
    let node = doc.node(id);
    let (open, close) = (open_bracket(&node.kind), close_bracket(&node.kind));
    if node.children == 0 {
        return format!("{open}{close}");
    }
    let mut out = format!("({}) {}", node.children, open);
    let budget = budget.max(out.len() + 3);
    let mut first = true;
    for c in doc.children(id) {
        let child = doc.node(c);
        let mut item = String::new();
        if let Key::Name(n) = &child.key {
            if is_identifier(n) {
                item.push_str(n);
            } else {
                item.push_str(&quote(n));
            }
            item.push_str(": ");
        }
        match &child.kind {
            Kind::Object => item.push_str(if child.children == 0 { "{}" } else { "{…}" }),
            Kind::Array => item.push_str(if child.children == 0 { "[]" } else { "[…]" }),
            _ => item.push_str(&value_text(doc, c)),
        }
        let sep = if first { "" } else { ", " };
        // Keep room for ", …" and the closing bracket.
        if out.chars().count() + sep.len() + item.chars().count() + 3 > budget {
            out.push_str(if first { "…" } else { ", …" });
            out.push_str(close);
            return out;
        }
        out.push_str(sep);
        out.push_str(&item);
        first = false;
    }
    out.push_str(close);
    out
}

/// The text search runs over for one node: the line-mode rendering of its
/// own row (`"key": value`, or `"key": {` for a container).
pub fn search_text(doc: &Doc, id: NodeId) -> String {
    let node = doc.node(id);
    let mut out = String::new();
    if let Some(k) = key_text(&node.key, true) {
        out.push_str(&k);
        out.push_str(": ");
    }
    match &node.kind {
        Kind::Object => out.push_str(if node.children == 0 { "{}" } else { "{" }),
        Kind::Array => out.push_str(if node.children == 0 { "[]" } else { "[" }),
        _ => out.push_str(&value_text(doc, id)),
    }
    out
}

/// Serialise the subtree at `id` as pretty-printed JSON.
pub fn to_json_pretty(doc: &Doc, id: NodeId, indent: usize) -> String {
    serialize(doc, id, Layout::Pretty(indent))
}

/// Serialise the subtree at `id` as one line of JSON, `, ` and `: ` spaced.
pub fn to_json_line(doc: &Doc, id: NodeId) -> String {
    serialize(doc, id, Layout::Line)
}

/// Serialise the subtree at `id` as one line of JSON with no spaces.
pub fn to_json_compact(doc: &Doc, id: NodeId) -> String {
    serialize(doc, id, Layout::Compact)
}

#[derive(Clone, Copy, PartialEq)]
enum Layout {
    /// One item per line, indented this many spaces a level.
    Pretty(usize),
    /// One line, `, ` and `: ` spaced.
    Line,
    /// One line, no spaces.
    Compact,
}

fn serialize(doc: &Doc, root: NodeId, layout: Layout) -> String {
    enum Step {
        Open(NodeId),
        Close(NodeId),
    }
    let indent = match layout {
        Layout::Pretty(w) => Some(w),
        _ => None,
    };
    let colon = if layout == Layout::Compact { ":" } else { ": " };
    let comma = if layout == Layout::Line { ", " } else { "," };
    let base_depth = doc.node(root).depth;
    let mut out = String::new();
    let mut stack = vec![Step::Open(root)];
    let pad = |out: &mut String, depth: u32| {
        if let Some(w) = indent {
            for _ in 0..((depth - base_depth) as usize * w) {
                out.push(' ');
            }
        }
    };
    let nl = |out: &mut String| {
        if indent.is_some() {
            out.push('\n');
        }
    };
    while let Some(step) = stack.pop() {
        match step {
            Step::Open(id) => {
                let node = doc.node(id);
                pad(&mut out, node.depth);
                if id != root {
                    if let Key::Name(n) = &node.key {
                        out.push_str(&quote(n));
                        out.push_str(colon);
                    }
                }
                let more = id != root && doc.next_sibling(id).is_some();
                match &node.kind {
                    Kind::Object | Kind::Array if node.children > 0 => {
                        out.push_str(open_bracket(&node.kind));
                        nl(&mut out);
                        stack.push(Step::Close(id));
                        let kids: Vec<NodeId> = doc.children(id).collect();
                        for c in kids.into_iter().rev() {
                            stack.push(Step::Open(c));
                        }
                    }
                    Kind::Object | Kind::Array => {
                        out.push_str(open_bracket(&node.kind));
                        out.push_str(close_bracket(&node.kind));
                        if more {
                            out.push_str(comma);
                        }
                        nl(&mut out);
                    }
                    k => {
                        match k {
                            Kind::Number(n) if !n.is_finite() => out.push_str("null"),
                            _ => out.push_str(&value_text(doc, id)),
                        }
                        if more {
                            out.push_str(comma);
                        }
                        nl(&mut out);
                    }
                }
            }
            Step::Close(id) => {
                let node = doc.node(id);
                pad(&mut out, node.depth);
                out.push_str(close_bracket(&node.kind));
                if id != root && doc.next_sibling(id).is_some() {
                    out.push_str(comma);
                }
                nl(&mut out);
            }
        }
    }
    if indent.is_some() && out.ends_with('\n') {
        out.pop();
    }
    out
}

/// `.foo[3].bar` — jless's dot path. Keys that are not identifiers are
/// written as `["weird key"]`.
pub fn path_dot(path: &[Key]) -> String {
    let mut out = String::new();
    for k in path {
        match k {
            Key::Root => {}
            Key::Index(i) => out.push_str(&format!("[{i}]")),
            Key::Name(n) => {
                if is_identifier(n) {
                    out.push('.');
                    out.push_str(n);
                } else {
                    out.push('[');
                    out.push_str(&quote(n));
                    out.push(']');
                }
            }
        }
    }
    out
}

/// `["foo"][3]["bar"]` — every segment in brackets.
pub fn path_bracket(path: &[Key]) -> String {
    let mut out = String::new();
    for k in path {
        match k {
            Key::Root => {}
            Key::Index(i) => out.push_str(&format!("[{i}]")),
            Key::Name(n) => {
                out.push('[');
                out.push_str(&quote(n));
                out.push(']');
            }
        }
    }
    out
}

/// Can jq's `.field` shorthand take this key bare? jq is stricter than
/// JavaScript: ASCII letters, digits and underscores only.
pub fn is_jq_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `.foo[3].bar` in jq's syntax; keys jq cannot take bare become
/// `."weird key"`, and an index first in the path follows a dot, `.[0]`,
/// since jq reads a bare `[0]` as an array of its own.
pub fn path_jq(path: &[Key]) -> String {
    let mut out = String::new();
    for k in path {
        match k {
            Key::Root => {}
            Key::Index(i) => push_jq_index(&mut out, i64::from(*i)),
            Key::Name(n) => push_jq_name(&mut out, n),
        }
    }
    if out.is_empty() {
        out.push('.');
    }
    out
}

/// Add an index step to a jq path being written: `[3]`, or `.[3]` when it
/// is the path's first step. A negative index counts from the end, as jq's
/// does: `.[-1]`.
pub fn push_jq_index(out: &mut String, i: i64) {
    if out.is_empty() {
        out.push('.');
    }
    out.push_str(&format!("[{i}]"));
}

/// Add a key step to a jq path being written: `.foo`, or `."weird key"`
/// for a key jq cannot take bare.
pub fn push_jq_name(out: &mut String, n: &str) {
    out.push('.');
    if is_jq_identifier(n) {
        out.push_str(n);
    } else {
        out.push_str(&quote(n));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(src: &str) -> Doc {
        Doc::from_value(&tabnas_json::parse(src).unwrap())
    }

    #[test]
    fn numbers() {
        assert_eq!(number(3.0), "3");
        assert_eq!(number(-0.0), "0");
        assert_eq!(number(42.5), "42.5");
        assert_eq!(number(1e21), "1e+21");
        assert_eq!(number(1.5e-7), "1.5e-7");
        assert_eq!(number(f64::NAN), "NaN");
        assert_eq!(number(f64::NEG_INFINITY), "-Infinity");
    }

    #[test]
    fn spellings_kept_and_not() {
        let kept = spelling;
        // JSON's spellings that `number` writes otherwise are kept.
        assert_eq!(kept("1.0", 1.0), Some("1.0"));
        assert_eq!(kept("-0", -0.0), Some("-0"));
        assert_eq!(kept("1e21", 1e21), Some("1e21"));
        assert_eq!(kept("1E3", 1000.0), Some("1E3"));
        assert_eq!(kept("2.50", 2.5), Some("2.50"));
        assert_eq!(kept(" 1.0 ", 1.0), Some("1.0"));
        let big = "12345678901234567890";
        assert_eq!(kept(big, 12345678901234567890.0), Some(big));
        assert_eq!(
            kept("9007199254740993", 9007199254740992.0),
            Some("9007199254740993")
        );
        // What `number` writes needs no spelling.
        for (text, n) in [
            ("55", 55.0),
            ("0", 0.0),
            ("-7", -7.0),
            ("0.5", 0.5),
            ("1e+21", 1e21),
        ] {
            assert_eq!(kept(text, n), None, "{text}");
        }
        assert_eq!(kept("123456789012345", 123456789012345.0), None);
        // A spelling JSON has no syntax for is shown as the value.
        for (text, n) in [
            ("0x1F", 31.0),
            ("1_000", 1000.0),
            ("+1", 1.0),
            (".5", 0.5),
            ("5.", 5.0),
            ("007", 7.0),
            ("1e", 1.0),
            ("Infinity", f64::INFINITY),
            ("", 0.0),
        ] {
            assert_eq!(kept(text, n), None, "{text}");
        }
        // A spelling of another number is not this one's.
        assert_eq!(kept("1.5", 1.0), None);
        assert_eq!(kept("-0", 0.0), None);
    }

    #[test]
    fn a_kept_spelling_is_shown_searched_and_written() {
        let mut d = doc(r#"{"a": 1, "b": [2, 3]}"#);
        d.spellings.insert(1, "1.0".into());
        d.spellings.insert(3, "12345678901234567890".into());
        assert_eq!(value_text(&d, 1), "1.0");
        assert_eq!(value_text(&d, 4), "3");
        assert_eq!(search_text(&d, 1), "\"a\": 1.0");
        assert_eq!(preview(&d, 2, 50), "(2) [12345678901234567890, 3]");
        assert_eq!(
            to_json_line(&d, 0),
            r#"{"a": 1.0, "b": [12345678901234567890, 3]}"#
        );
        assert_eq!(to_json_pretty(&d, 3, 2), "12345678901234567890");
    }

    #[test]
    fn quoting_and_identifiers() {
        assert_eq!(quote("a\"b\\c\n\u{1}"), r#""a\"b\\c\n\u0001""#);
        assert!(is_identifier("foo_bar$1"));
        assert!(!is_identifier("1abc"));
        assert!(!is_identifier("a-b"));
        assert!(!is_identifier(""));
        assert_eq!(
            key_text(&Key::Name("abc".into()), false),
            Some("abc".into())
        );
        assert_eq!(
            key_text(&Key::Name("a b".into()), false),
            Some("\"a b\"".into())
        );
        assert_eq!(
            key_text(&Key::Name("abc".into()), true),
            Some("\"abc\"".into())
        );
        assert_eq!(key_text(&Key::Index(2), false), None);
    }

    #[test]
    fn previews() {
        let d = doc(r#"{"a": 1, "b": "x", "c": [1, 2], "d": {}}"#);
        assert_eq!(preview(&d, 0, 200), r#"(4) {a: 1, b: "x", c: […], d: {}}"#);
        assert_eq!(preview(&d, 0, 22), r#"(4) {a: 1, b: "x", …}"#);
        assert_eq!(preview(&d, 0, 18), r#"(4) {a: 1, …}"#);
        assert_eq!(preview(&d, 0, 1), "(4) {…}");
        assert_eq!(preview(&d, 6, 50), "{}");
        assert_eq!(preview(&d, 3, 50), "(2) [1, 2]");
    }

    #[test]
    fn serialisation() {
        let d = doc(r#"{"a": 1, "b": [true, {"c": "x"}], "d": {}, "e": []}"#);
        let pretty = to_json_pretty(&d, 0, 2);
        let expect = "{\n  \"a\": 1,\n  \"b\": [\n    true,\n    {\n      \"c\": \"x\"\n    }\n  ],\n  \"d\": {},\n  \"e\": []\n}";
        assert_eq!(pretty, expect);
        assert_eq!(
            to_json_line(&d, 0),
            r#"{"a": 1, "b": [true, {"c": "x"}], "d": {}, "e": []}"#
        );
        assert_eq!(
            to_json_compact(&d, 0),
            r#"{"a":1,"b":[true,{"c":"x"}],"d":{},"e":[]}"#
        );
        // A subtree root takes no trailing comma even with a next sibling.
        assert_eq!(to_json_line(&d, 2), r#"[true, {"c": "x"}]"#);
        assert_eq!(to_json_pretty(&d, 1, 2), "1");
        let round: serde_json::Value = serde_json::from_str(&pretty).unwrap();
        assert_eq!(round["b"][1]["c"], "x");
    }

    #[test]
    fn paths() {
        let p = vec![
            Key::Name("foo".into()),
            Key::Index(3),
            Key::Name("bar baz".into()),
        ];
        assert_eq!(path_dot(&p), r#".foo[3]["bar baz"]"#);
        assert_eq!(path_bracket(&p), r#"["foo"][3]["bar baz"]"#);
        assert_eq!(path_jq(&p), r#".foo[3]."bar baz""#);
        assert_eq!(path_jq(&[]), ".");
        // A root array's items: jq takes `.[0]`, and reads `[0]` as an
        // array literal. Only the first step needs the dot.
        assert_eq!(path_jq(&[Key::Index(0)]), ".[0]");
        assert_eq!(
            path_jq(&[Key::Index(1), Key::Name("name".into())]),
            ".[1].name"
        );
        assert_eq!(path_jq(&[Key::Index(1), Key::Index(2)]), ".[1][2]");
        // jless's dot path keeps its own form.
        assert_eq!(path_dot(&[Key::Index(0)]), "[0]");
        let mut negative = String::new();
        push_jq_index(&mut negative, -1);
        assert_eq!(negative, ".[-1]");
        // jq takes fewer keys bare than JavaScript does.
        let jq = |k: &str| path_jq(&[Key::Name(k.into())]);
        assert_eq!(jq("a_b1"), ".a_b1");
        assert_eq!(jq("$foo"), r#"."$foo""#);
        assert_eq!(jq("é"), r#"."é""#);
        assert_eq!(jq("1a"), r#"."1a""#);
        assert_eq!(path_dot(&[Key::Name("$foo".into())]), ".$foo");
        assert_eq!(path_dot(&[]), "");
    }

    #[test]
    fn search_texts() {
        let d = doc(r#"{"a": 1, "b": [true], "c d": {}}"#);
        assert_eq!(search_text(&d, 0), "{");
        assert_eq!(search_text(&d, 1), "\"a\": 1");
        assert_eq!(search_text(&d, 2), "\"b\": [");
        assert_eq!(search_text(&d, 3), "true");
        assert_eq!(search_text(&d, 4), "\"c d\": {}");
    }
}
