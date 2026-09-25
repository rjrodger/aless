//! Provenance: which source line and column each node came from.
//!
//! The tabnas engine reports every lexed token with its position, but a
//! parsed value carries no offsets. The two are aligned here: the tree's
//! keys and leaf values, in document order, are matched against the
//! value-bearing tokens, in source order, with a bounded lookahead. What
//! matches gets a position; what does not (a value a grammar synthesised,
//! a CSV column name that only appears in the header) is left at 0 and
//! inherits nothing. Containers take the position of their key when it
//! matched, else of their first positioned descendant.
//!
//! This is grammar-agnostic and best-effort by design: it is exact for the
//! JSON family, TOML, INI, CSV and ZON values, and degrades gracefully for
//! YAML, XML and Markdown.

use std::sync::{Arc, Mutex};

use tabnas::{Tabnas, Token, Value};

use crate::doc::{Doc, Key, Kind};

/// The part of a token that alignment needs.
#[derive(Clone, Debug, PartialEq)]
pub struct Tok {
    pub line: u32,
    pub col: u32,
    pub val: TokVal,
    /// Whether the token's source text contains a digit: a number token
    /// whose text has none (YAML's `: ` carries a numeric value) is not a
    /// number in the document.
    pub digit: bool,
    /// Whether the token has any source text at all (the end-of-source
    /// token has none and a `null` value).
    pub has_src: bool,
    /// `Some(true)` when the token text is an opening `[`, `Some(false)`
    /// for an opening `{` (a ZON `.{` included), else `None`.
    pub src_open: Option<bool>,
    /// A single punctuation character standing for itself (`,` `:` `]`),
    /// as opposed to a quoted one-character string value.
    pub bare_punct: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TokVal {
    Str(String),
    Num(f64),
    Bool(bool),
    Null,
    Other,
}

impl Tok {
    pub fn from_token(t: &Token) -> Tok {
        let src = t.src.as_str();
        let val = match &t.val {
            Value::String(s) => TokVal::Str(s.clone()),
            Value::Text(s) => TokVal::Str(s.string.clone()),
            Value::Number(n) => TokVal::Num(*n),
            Value::Bool(b) => TokVal::Bool(*b),
            Value::Null => TokVal::Null,
            _ => TokVal::Other,
        };
        let bare = src.trim();
        let mut chars = bare.chars();
        let single_punct =
            matches!((chars.next(), chars.next()), (Some(c), None) if c.is_ascii_punctuation());
        Tok {
            line: t.site.ri as u32,
            col: t.site.ci as u32,
            digit: src.chars().any(|c| c.is_ascii_digit()),
            has_src: !src.is_empty(),
            src_open: match bare {
                "[" | ".[" => Some(true),
                "{" | ".{" => Some(false),
                _ => None,
            },
            bare_punct: single_punct && matches!(&t.val, Value::String(v) if v == bare),
            val,
        }
    }
}

/// A token sink shared with a parser's token subscriber.
pub type Capture = Arc<Mutex<Vec<Tok>>>;

/// Could this token ever place a node? Punctuation standing for itself
/// (`:` `,` `]`), bare whitespace and newlines, and the end-of-source
/// token never match a key or a value, and roughly half of a JSON
/// document's tokens are exactly those, so they are not kept.
pub fn worth_keeping(tok: &Tok) -> bool {
    if !tok.has_src {
        return false;
    }
    if tok.src_open.is_some() {
        return true;
    }
    match &tok.val {
        TokVal::Other => false,
        TokVal::Str(s) => !s.trim().is_empty() && !tok.bare_punct,
        TokVal::Num(_) => tok.digit,
        TokVal::Bool(_) | TokVal::Null => true,
    }
}

/// Subscribe to the parser's token stream, returning the sink it fills.
pub fn capture(parser: &mut Tabnas) -> Capture {
    let sink: Capture = Arc::new(Mutex::new(Vec::new()));
    let s2 = sink.clone();
    parser.subscribe_tokens(move |t: &Token| {
        let tok = Tok::from_token(t);
        if !worth_keeping(&tok) {
            return;
        }
        if let Ok(mut v) = s2.lock() {
            v.push(tok);
        }
    });
    sink
}

/// How far ahead of the cursor a match may be found. Bounds the damage an
/// accidental match of a synthesised value can do.
const WINDOW: usize = 64;

/// How far an opening bracket may sit past the cursor. Punctuation is not
/// kept (see [`worth_keeping`]), so the bracket is the very next token; one
/// more covers a grammar that emits a keyword token before it.
const OPEN_WINDOW: usize = 2;

/// Shortest string for which a containment match (the item found inside a
/// longer token, as a Markdown text run sits inside its line) is accepted.
const CONTAIN_MIN: usize = 3;

fn matches_str(tok: &Tok, s: &str) -> bool {
    match &tok.val {
        TokVal::Str(t) => t == s,
        // A key spelled as a number in the source (`{1: "a"}`).
        TokVal::Num(n) if tok.digit => crate::fmt::number(*n) == s,
        _ => false,
    }
}

fn contains_str(tok: &Tok, s: &str) -> bool {
    matches!(&tok.val, TokVal::Str(t) if t.len() > s.len() && t.contains(s))
}

fn matches_open(tok: &Tok, kind: &Kind) -> bool {
    let want = crate::fmt::open_bracket(kind);
    match &tok.val {
        TokVal::Str(t) => t == want,
        // ZON's `.{` carries no value; its text still opens the container.
        TokVal::Null | TokVal::Other => tok.has_src && tok.src_open == Some(want == "["),
        _ => false,
    }
}

fn matches_kind(tok: &Tok, kind: &Kind) -> bool {
    match (kind, &tok.val) {
        (Kind::Str(s), _) => matches_str(tok, s),
        (Kind::Number(a), TokVal::Num(b)) => tok.digit && (a == b || (a.is_nan() && b.is_nan())),
        (Kind::Number(a), TokVal::Str(s)) => tok.digit && crate::fmt::number(*a) == *s,
        (Kind::Bool(a), TokVal::Bool(b)) => a == b,
        (Kind::Null, TokVal::Null) => tok.has_src,
        _ => false,
    }
}

/// Assign source positions to the nodes of `doc` from its token stream.
pub fn align(doc: &mut Doc, toks: &[Tok]) {
    let mut cursor = 0usize;
    let find = |cursor: usize, window: usize, pred: &dyn Fn(&Tok) -> bool| -> Option<usize> {
        let end = (cursor + window).min(toks.len());
        (cursor..end).find(|&j| pred(&toks[j]))
    };
    for i in 0..doc.nodes.len() {
        let (key, kind) = {
            let n = &doc.nodes[i];
            (n.key.clone(), n.kind.clone())
        };
        let mut placed = false;
        if let Key::Name(name) = &key {
            if let Some(j) = find(cursor, WINDOW, &|t| matches_str(t, name)) {
                doc.nodes[i].line = toks[j].line;
                doc.nodes[i].col = toks[j].col;
                cursor = j + 1;
                placed = true;
            }
        }
        if kind.is_container() {
            // A container with no key of its own (the root, an array
            // element) sits where its opening bracket was lexed. The
            // bracket must be right at hand: a container a grammar
            // synthesised, like the JSON Lines root array, has no bracket,
            // and a wider look would seize the next nested container's and
            // drag every later match off by one.
            if !placed {
                if let Some(j) = find(cursor, OPEN_WINDOW, &|t| matches_open(t, &kind)) {
                    doc.nodes[i].line = toks[j].line;
                    doc.nodes[i].col = toks[j].col;
                    cursor = j + 1;
                }
            }
        } else {
            if let Some(j) = find(cursor, WINDOW, &|t| matches_kind(t, &kind)) {
                if !placed {
                    doc.nodes[i].line = toks[j].line;
                    doc.nodes[i].col = toks[j].col;
                }
                cursor = j + 1;
            } else if let Kind::Str(s) = &kind {
                if s.len() >= CONTAIN_MIN {
                    if let Some(j) = find(cursor, WINDOW, &|t| contains_str(t, s)) {
                        if !placed {
                            doc.nodes[i].line = toks[j].line;
                            doc.nodes[i].col = toks[j].col;
                        }
                        // The same token may host later items too.
                        cursor = j;
                    }
                }
            }
        }
    }
    // Containers without a key position take their first positioned
    // child's; children have larger indices, so a reverse sweep sees them
    // final.
    for i in (0..doc.nodes.len()).rev() {
        if doc.nodes[i].line != 0 || !doc.nodes[i].is_container() {
            continue;
        }
        let first = doc
            .children(i as u32)
            .map(|c| (doc.nodes[c as usize].line, doc.nodes[c as usize].col))
            .find(|(l, _)| *l != 0);
        if let Some((l, c)) = first {
            doc.nodes[i].line = l;
            doc.nodes[i].col = c;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> Doc {
        let mut p = tabnas_json::make();
        let sink = capture(&mut p);
        let v = p.parse(src).unwrap();
        let mut doc = Doc::from_value(&v);
        let toks = sink.lock().unwrap();
        align(&mut doc, &toks);
        doc
    }

    #[test]
    fn json_positions_are_exact() {
        let doc = parse("{\n  \"a\": 1,\n  \"b\": [true, null, \"x\"],\n  \"c\": {\"d\": 2}\n}\n");
        let pos: Vec<(u32, u32)> = doc.nodes.iter().map(|n| (n.line, n.col)).collect();
        assert_eq!(
            pos,
            vec![
                (1, 1),  // root: its opening brace
                (2, 3),  // a
                (3, 3),  // b
                (3, 9),  // true
                (3, 15), // null
                (3, 21), // "x"
                (4, 3),  // c
                (4, 9),  // d
            ]
        );
    }

    #[test]
    fn repeated_values_keep_order() {
        let doc = parse(r#"{"a": "a", "b": "a", "c": 1, "d": 1}"#);
        let cols: Vec<u32> = doc.nodes.iter().skip(1).map(|n| n.col).collect();
        assert_eq!(cols, vec![2, 12, 22, 30]);
    }

    #[test]
    fn synthesised_values_are_skipped() {
        let mut p = tabnas_json::make();
        let sink = capture(&mut p);
        let v = p.parse(r#"{"a": 1, "b": 2}"#).unwrap();
        let mut doc = Doc::from_value(&v);
        // Pretend the grammar added a key the source never had.
        doc.nodes.insert(
            2,
            crate::doc::Node {
                parent: 0,
                key: Key::Name("ghost".into()),
                kind: Kind::Str("nowhere".into()),
                depth: 1,
                size: 1,
                children: 0,
                expanded: true,
                line: 0,
                col: 0,
            },
        );
        doc.nodes[0].children = 3;
        doc.nodes[0].size = 4;
        align(&mut doc, &sink.lock().unwrap());
        assert_eq!(doc.nodes[1].col, 2);
        assert_eq!(doc.nodes[2].line, 0);
        assert_eq!(doc.nodes[3].col, 10);
    }

    #[test]
    fn synthesised_root_does_not_steal_a_nested_bracket() {
        let mut p = tabnas_jsonl::make();
        let sink = capture(&mut p);
        let v = p
            .parse("{\"id\": 1, \"tags\": [\"a\"]}\n{\"id\": 2, \"tags\": []}\n{\"id\": 3, \"tags\": [\"c\"]}\n")
            .unwrap();
        let mut doc = Doc::from_value(&v);
        align(&mut doc, &sink.lock().unwrap());
        let lines: Vec<u32> = doc.nodes.iter().map(|n| n.line).collect();
        // root (from its first child), then each record's nodes on its own line
        assert_eq!(lines, vec![1, 1, 1, 1, 1, 2, 2, 2, 3, 3, 3, 3]);
    }

    #[test]
    fn punctuation_is_not_kept() {
        let mut p = tabnas_json::make();
        let sink = capture(&mut p);
        p.parse(r#"{"a": [1, true, null], "b": "x", "c": "-"}"#)
            .unwrap();
        let toks = sink.lock().unwrap();
        let vals: Vec<String> = toks
            .iter()
            .map(|t| match &t.val {
                TokVal::Str(s) => s.clone(),
                TokVal::Num(n) => n.to_string(),
                TokVal::Bool(b) => b.to_string(),
                TokVal::Null => "null".to_string(),
                TokVal::Other => "?".to_string(),
            })
            .collect();
        assert_eq!(
            vals,
            vec!["{", "a", "[", "1", "true", "null", "b", "x", "c", "-"]
        );
    }

    #[test]
    fn end_token_null_is_not_a_value() {
        let doc = parse("[null]");
        assert_eq!((doc.nodes[1].line, doc.nodes[1].col), (1, 2));
        let doc = parse("[1]");
        assert_eq!(doc.nodes[1].col, 2);
    }
}
