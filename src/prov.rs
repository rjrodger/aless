//! Source positions for a parsed tree, by aligning its keys and leaf values
//! with the lexed tokens (docs/explanation/positions.md). Records keyed by a
//! header row align by column instead ([`align_records`]).

use std::collections::HashMap;
use std::ops::Range;
use std::sync::{Arc, Mutex};

use tabnas::{Tabnas, Tin, Token, Value};

use crate::doc::{Doc, Key, Kind};

/// The part of a token that alignment needs.
#[derive(Clone, Debug, PartialEq)]
pub struct Tok {
    pub line: u32,
    pub col: u32,
    pub val: TokVal,
    /// A number token with no digit in its text (YAML's `: `) is no number.
    pub digit: bool,
    /// The end-of-source token has no text, and a `null` value.
    pub has_src: bool,
    /// `Some(true)` for an opening `[`, `Some(false)` for `{` or ZON's `.{`.
    pub src_open: Option<bool>,
    /// Lone punctuation standing for itself, not a quoted one-character string.
    pub bare_punct: bool,
    /// The engine's word token (`#TX`): under a plain-text grammar `*` is a word.
    pub text: bool,
    /// Line breaks the text spans; a line end standing for blank lines counts each.
    pub rows: u32,
    /// tabnas-csv's field separator, a comma or a tab, bound to `#CA`.
    pub sep: bool,
    /// `#LN`, which the csv grammar takes out of its ignore set to see row breaks.
    pub line_end: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TokVal {
    Str(String),
    /// Boxed spelling ([`crate::fmt::spelling`]): few numbers have one.
    Num(f64, Option<Box<String>>),
    Bool(bool),
    Null,
    Other,
}

impl Tok {
    /// [`Tok::from_token_with`] with no fixed text.
    pub fn from_token(t: &Token) -> Tok {
        Tok::from_token_with(t, None)
    }

    /// `fixed` is the text of the fixed token `t`'s identity names. The lexer
    /// gives a fixed token no value, so it stands for its `src` (each character
    /// of a semver version); ZON's `.{`, made under `#OB`, is not the fixed `{`.
    pub fn from_token_with(t: &Token, fixed: Option<&str>) -> Tok {
        let src = t.src.as_str();
        let stands_for = match &t.val {
            Value::String(v) => Some(v.as_str()),
            Value::Undefined if fixed == Some(src) => Some(src),
            _ => None,
        };
        let val = match &t.val {
            Value::Text(s) => TokVal::Str(s.string.clone()),
            Value::Number(n) => TokVal::Num(
                *n,
                crate::fmt::spelling(src, *n).map(|s| Box::new(s.to_string())),
            ),
            Value::Bool(b) => TokVal::Bool(*b),
            Value::Null => TokVal::Null,
            _ => stands_for.map_or(TokVal::Other, |s| TokVal::Str(s.to_string())),
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
            bare_punct: single_punct && stands_for == Some(bare),
            text: t.name.as_str() == "#TX",
            rows: src.matches('\n').count() as u32,
            sep: t.name.as_str() == "#CA",
            line_end: t.name.as_str() == "#LN",
            val,
        }
    }
}

/// A token sink shared with a parser's token subscriber.
pub type Capture = Arc<Mutex<Vec<Tok>>>;

/// Punctuation standing for itself, whitespace and the end-of-source token
/// never place a node, and are about half a JSON document's tokens.
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
        TokVal::Num(..) => tok.digit,
        TokVal::Bool(_) | TokVal::Null => true,
    }
}

/// Subscribe to the parser's token stream, returning the sink it fills.
pub fn capture(parser: &mut Tabnas) -> Capture {
    capture_with(parser, false, false, &[])
}

/// [`capture`] without the tokens `names` identify: number tokens whose values
/// the tree also holds as counts of its own, which would match out of place.
pub fn capture_except(parser: &mut Tabnas, names: &'static [&'static str]) -> Capture {
    capture_with(parser, false, false, names)
}

/// [`capture`], keeping a lone punctuation word (`#TX`) too: under a grammar
/// from the command line `*` (a passwd password) is a value.
pub fn capture_words(parser: &mut Tabnas) -> Capture {
    capture_with(parser, true, false, &[])
}

/// [`capture`], keeping every field separator and line end for [`align_records`].
pub fn capture_cells(parser: &mut Tabnas) -> Capture {
    capture_with(parser, false, true, &[])
}

fn capture_with(
    parser: &mut Tabnas,
    words: bool,
    cells: bool,
    except: &'static [&'static str],
) -> Capture {
    let sink: Capture = Arc::new(Mutex::new(Vec::new()));
    let s2 = sink.clone();
    let fixed: HashMap<Tin, String> = parser
        .config()
        .fixed
        .tokens
        .values()
        .map(|t| (t.tin, t.source.clone()))
        .collect();
    parser.subscribe_tokens(move |t: &Token| {
        if except.contains(&t.name.as_str()) {
            return;
        }
        let tok = Tok::from_token_with(t, fixed.get(&t.tin).map(String::as_str));
        let kept = worth_keeping(&tok)
            || (words && tok.text && tok.has_src)
            || (cells && (tok.sep || tok.line_end));
        if !kept {
            return;
        }
        if let Ok(mut v) = s2.lock() {
            v.push(tok);
        }
    });
    sink
}

/// Bounds the damage an accidental match of a synthesised value can do.
const WINDOW: usize = 64;

/// Punctuation is not kept, so an opening bracket is the next token; one more
/// allows a keyword token before it.
const OPEN_WINDOW: usize = 2;

/// Shortest string matched inside a longer token, as a Markdown run in its line.
const CONTAIN_MIN: usize = 3;

fn matches_str(tok: &Tok, s: &str) -> bool {
    match &tok.val {
        TokVal::Str(t) => t == s,
        // A key spelled as a number in the source (`{1: "a"}`).
        TokVal::Num(n, _) if tok.digit => crate::fmt::number(*n) == s,
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
        (Kind::Number(a), TokVal::Num(b, _)) => tok.digit && (a == b || (a.is_nan() && b.is_nan())),
        (Kind::Number(a), TokVal::Str(s)) => tok.digit && crate::fmt::number(*a) == *s,
        (Kind::Bool(a), TokVal::Bool(b)) => a == b,
        (Kind::Null, TokVal::Null) => tok.has_src,
        _ => false,
    }
}

/// Assign source positions to the nodes of `doc` from its token stream.
pub fn align(doc: &mut Doc, toks: &[Tok]) {
    let all = 0..doc.nodes.len();
    align_nodes(
        doc,
        all,
        toks,
        Look {
            keys: true,
            openers: true,
        },
    );
    inherit_positions(doc);
}

/// [`align`] for records keyed by a header row (CSV, TSV), over [`capture_cells`]:
/// the first line with a token is the header, and each field sits at the first
/// token of its column's cell. No key or bracket is looked for, as the source
/// has none; a line that does not spell its record is aligned by value, alone.
pub fn align_records(doc: &mut Doc, toks: &[Tok]) {
    let mut lines = split_runs(toks).skip_while(|run| run.is_empty());
    if let Some(header) = lines.next() {
        let columns = header_columns(header);
        let records: Vec<u32> = doc.children(0).collect();
        for record in records {
            // Skip a line with no token unless it spells the record: the plugin
            // makes a record of empty fields from one (`record.empty`, spaces).
            let mut found = None;
            for run in lines.by_ref() {
                let agrees = record_agrees(doc, record, run, &columns);
                if run.is_empty() && !agrees {
                    continue;
                }
                found = Some((run, agrees));
                break;
            }
            let Some((run, agrees)) = found else {
                break;
            };
            if agrees {
                place_record(doc, record, run, &columns);
            } else {
                let start = record as usize;
                let end = start + doc.nodes[start].size as usize;
                align_nodes(
                    doc,
                    start..end,
                    run,
                    Look {
                        keys: false,
                        openers: false,
                    },
                );
            }
        }
    }
    inherit_positions(doc);
}

/// A quoted cell holding a newline is one token, and never splits a line.
fn split_runs(toks: &[Tok]) -> impl Iterator<Item = &[Tok]> {
    toks.split(|t| t.line_end)
}

fn cells(run: &[Tok]) -> impl Iterator<Item = &[Tok]> {
    run.split(|t| t.sep)
}

fn squeeze(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

/// The lexer cuts `first name` into words and reads ` 36` as a space and a
/// word, so a cell's text is compared with all whitespace left out.
fn cell_text(cell: &[Tok]) -> Option<String> {
    let mut text = String::new();
    for t in cell {
        match &t.val {
            TokVal::Str(s) => text.push_str(&squeeze(s)),
            TokVal::Num(n, _) if t.digit => text.push_str(&crate::fmt::number(*n)),
            _ => return None,
        }
    }
    Some(text)
}

/// A repeated name maps to its last column: tabnas-csv keeps one member under
/// it, in the first column's place, holding the last column's value.
fn header_columns(header: &[Tok]) -> HashMap<String, usize> {
    let mut columns = HashMap::new();
    for (k, cell) in cells(header).enumerate() {
        if let Some(name) = cell_text(cell) {
            columns.insert(name, k);
        }
    }
    columns
}

fn cell_holds(cell: &[Tok], kind: &Kind) -> bool {
    if let [tok] = cell {
        if matches_kind(tok, kind) {
            return true;
        }
    }
    match (kind, cell_text(cell)) {
        (Kind::Str(s), Some(text)) => squeeze(s) == text,
        _ => false,
    }
}

fn record_agrees(doc: &Doc, record: u32, run: &[Tok], columns: &HashMap<String, usize>) -> bool {
    if !matches!(doc.nodes[record as usize].kind, Kind::Object) {
        return false;
    }
    let cells: Vec<&[Tok]> = cells(run).collect();
    doc.children(record).all(|f| {
        let node = &doc.nodes[f as usize];
        match node.key.name().and_then(|name| columns.get(&squeeze(name))) {
            Some(&col) => cell_holds(cells.get(col).copied().unwrap_or(&[]), &node.kind),
            None => false,
        }
    })
}

fn place_record(doc: &mut Doc, record: u32, run: &[Tok], columns: &HashMap<String, usize>) {
    let cells: Vec<&[Tok]> = cells(run).collect();
    let fields: Vec<u32> = doc.children(record).collect();
    for f in fields {
        let node = &doc.nodes[f as usize];
        let at = node
            .key
            .name()
            .and_then(|name| columns.get(&squeeze(name)))
            .and_then(|&col| cells.get(col))
            .and_then(|cell| cell.first())
            .map(|t| (t.line, t.col));
        if let Some((line, col)) = at {
            doc.nodes[f as usize].line = line;
            doc.nodes[f as usize].col = col;
        }
    }
}

/// Bit equality, as a token is matched by value and `-0` equals `0`.
fn take_spelling(doc: &mut Doc, i: usize, tok: &Tok) {
    if let (Kind::Number(a), TokVal::Num(b, Some(spelled))) = (&doc.nodes[i].kind, &tok.val) {
        if a.to_bits() == b.to_bits() {
            doc.spellings.insert(i as u32, spelled.as_str().into());
        }
    }
}

/// What [`align_nodes`] looks for in the stream besides a node's value.
#[derive(Clone, Copy)]
struct Look {
    keys: bool,
    openers: bool,
}

/// The alignment proper: a lookahead of [`WINDOW`] from a cursor past each match.
fn align_nodes(doc: &mut Doc, range: Range<usize>, toks: &[Tok], look: Look) {
    let mut cursor = 0usize;
    let find = |cursor: usize, window: usize, pred: &dyn Fn(&Tok) -> bool| -> Option<usize> {
        let end = (cursor + window).min(toks.len());
        (cursor..end).find(|&j| pred(&toks[j]))
    };
    for i in range {
        let (key, kind) = {
            let n = &doc.nodes[i];
            (n.key.clone(), n.kind.clone())
        };
        let mut placed = false;
        if let (true, Key::Name(name)) = (look.keys, &key) {
            if let Some(j) = find(cursor, WINDOW, &|t| matches_str(t, name)) {
                doc.nodes[i].line = toks[j].line;
                doc.nodes[i].col = toks[j].col;
                cursor = j + 1;
                placed = true;
            }
        }
        if kind.is_container() {
            // A keyless container sits at its opening bracket, sought only at
            // hand: a synthesised one (the JSON Lines root) has none, and would
            // seize a nested container's and shift every later match.
            if !placed && look.openers {
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
                take_spelling(doc, i, &toks[j]);
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
}

/// Positions for a C syntax tree, whose token nodes carry their `src` and
/// `span`: alignment by value would misplace its repeated subtrees and span
/// numbers. A token node and its text are placed; a token with no text is not.
pub fn align_spans(doc: &mut Doc) {
    for i in 0..doc.nodes.len() {
        if doc.nodes[i].kind != Kind::Object {
            continue;
        }
        let (mut token, mut text, mut place) = (false, None, None);
        for c in doc.children(i as u32) {
            let node = &doc.nodes[c as usize];
            match (node.key.name(), &node.kind) {
                (Some("kind"), Kind::Str(kind)) => token = &**kind == "token",
                (Some("src"), Kind::Str(src)) if !src.is_empty() => text = Some(c),
                (Some("span"), Kind::Object) => place = span_place(doc, c),
                _ => {}
            }
        }
        if let (true, Some(text), Some((line, col))) = (token, text, place) {
            for at in [i, text as usize] {
                doc.nodes[at].line = line;
                doc.nodes[at].col = col;
            }
        }
    }
    inherit_positions(doc);
}

fn span_place(doc: &Doc, span: u32) -> Option<(u32, u32)> {
    let (mut line, mut col) = (None, None);
    for c in doc.children(span) {
        let node = &doc.nodes[c as usize];
        let number = match node.kind {
            Kind::Number(n) if n >= 1.0 && n <= f64::from(u32::MAX) && n.fract() == 0.0 => n as u32,
            _ => continue,
        };
        match node.key.name() {
            Some("line") => line = Some(number),
            Some("col") => col = Some(number),
            _ => {}
        }
    }
    Some((line?, col?))
}

/// A container without a position takes its first positioned child's;
/// children have larger indices, so a reverse sweep sees them final.
fn inherit_positions(doc: &mut Doc) {
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
                TokVal::Num(n, _) => n.to_string(),
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
    fn a_fixed_token_stands_for_its_text() {
        let mut p = tabnas_semver::make();
        let sink = capture(&mut p);
        let v = p.parse("1.0.2").unwrap();
        let toks = sink.lock().unwrap();
        let kept: Vec<(u32, TokVal)> = toks.iter().map(|t| (t.col, t.val.clone())).collect();
        let str = |s: &str| TokVal::Str(s.to_string());
        assert_eq!(kept, vec![(1, str("1")), (3, str("0")), (5, str("2"))]);
        let mut doc = Doc::from_value(&v);
        align(&mut doc, &toks);
        let cols: Vec<u32> = doc.nodes.iter().map(|n| n.col).collect();
        // The empty pre-release and build lists have nothing in the source.
        assert_eq!(&cols[..4], &[1, 1, 3, 5]);
    }

    #[test]
    fn a_word_of_one_punctuation_character_is_placed_for_plain_text() {
        let grammar = crate::grammar::compile("doc = *word   ; @array\nword = ( TX )\n").unwrap();
        let mut p = grammar.parser().unwrap();
        let sink = capture_words(&mut p);
        let v = p.parse("a * b\n- x").unwrap();
        let mut doc = Doc::from_value(&v);
        align(&mut doc, &sink.lock().unwrap());
        let pos: Vec<(u32, u32)> = doc.nodes.iter().skip(1).map(|n| (n.line, n.col)).collect();
        assert_eq!(pos, vec![(1, 1), (1, 3), (1, 5), (2, 1), (2, 3)]);
        let mut p = grammar.parser().unwrap();
        let sink = capture(&mut p);
        p.parse("a * b").unwrap();
        let kept: Vec<bool> = sink.lock().unwrap().iter().map(|t| t.text).collect();
        assert_eq!(kept.len(), 2, "the `*` is not kept for the JSON family");
    }

    #[test]
    fn a_number_takes_its_tokens_spelling() {
        let doc =
            parse(r#"{"a": 1.0, "b": [-0, 55, 1e21], "1.0": 12345678901234567890, "c": 0.5}"#);
        let spelled: Vec<(u32, &str)> = (0..doc.len() as u32)
            .filter_map(|i| doc.spelling(i).map(|s| (i, s)))
            .collect();
        assert_eq!(
            spelled,
            vec![
                (1, "1.0"),
                (3, "-0"),
                (5, "1e21"),
                (6, "12345678901234567890")
            ]
        );
    }

    #[test]
    fn a_spelling_goes_only_to_its_own_number() {
        let mut p = tabnas_json::make();
        let sink = capture(&mut p);
        let v = p.parse("[-0]").unwrap();
        let mut doc = Doc::from_value(&v);
        doc.nodes[1].kind = Kind::Number(0.0);
        align(&mut doc, &sink.lock().unwrap());
        assert_eq!(doc.nodes[1].col, 2, "placed at the token");
        assert_eq!(doc.spelling(1), None, "but not spelled `-0`");
    }

    #[test]
    fn end_token_null_is_not_a_value() {
        let doc = parse("[null]");
        assert_eq!((doc.nodes[1].line, doc.nodes[1].col), (1, 2));
        let doc = parse("[1]");
        assert_eq!(doc.nodes[1].col, 2);
    }

    fn parse_records(mut p: Tabnas, src: &str) -> Doc {
        let sink = capture_cells(&mut p);
        let v = p.parse(src).unwrap();
        let mut doc = Doc::from_value(&v);
        align_records(&mut doc, &sink.lock().unwrap());
        doc
    }

    fn parse_csv(src: &str) -> Doc {
        parse_records(tabnas_csv::make(), src)
    }

    /// The parser `load` builds for `Format::Tsv`.
    fn parse_tsv(src: &str) -> Doc {
        let mut options = tabnas_csv::CsvOptions::default();
        options.field.separation = Some("\t".to_string());
        parse_records(tabnas_csv::make_with(options), src)
    }

    /// A record's fields, as `(key, value, line, col)`.
    fn fields(doc: &Doc, record: u32) -> Vec<(String, String, u32, u32)> {
        doc.children(record)
            .map(|f| {
                let n = &doc.nodes[f as usize];
                let value = match &n.kind {
                    Kind::Str(s) => s.to_string(),
                    other => format!("{other:?}"),
                };
                (n.key.name().unwrap_or("").to_string(), value, n.line, n.col)
            })
            .collect()
    }

    fn positions(doc: &Doc) -> Vec<(u32, u32)> {
        doc.nodes.iter().map(|n| (n.line, n.col)).collect()
    }

    #[test]
    fn csv_fields_sit_at_their_values_and_never_on_the_header() {
        let doc = parse_csv("name,age,city\nada,36,london\nlin,28,helsinki\n");
        assert_eq!(
            positions(&doc),
            vec![
                (2, 1), // the root array: its first record
                (2, 1), // [0]
                (2, 1), // [0].name
                (2, 5), // [0].age
                (2, 8), // [0].city
                (3, 1), // [1]
                (3, 1), // [1].name
                (3, 5), // [1].age
                (3, 8), // [1].city
            ]
        );
    }

    #[test]
    fn csv_values_that_spell_a_column_name_do_not_shift_the_record() {
        let doc = parse_csv("a,b\nb,a\n");
        assert_eq!(positions(&doc)[2..], [(2, 1), (2, 3)]);
    }

    #[test]
    fn csv_empty_cell_is_unplaced_and_the_rest_stay_put() {
        let doc = parse_csv("name,age,city\nada,36\nlin,28,helsinki\n");
        assert_eq!(
            positions(&doc)[2..],
            [(2, 1), (2, 5), (0, 0), (3, 1), (3, 1), (3, 5), (3, 8)]
        );
    }

    #[test]
    fn csv_header_is_found_by_its_first_cell() {
        let doc = parse_csv("\n\nname,age\nada,36\n");
        assert_eq!(positions(&doc)[2..], [(4, 1), (4, 5)]);
        let doc = parse_tsv("\n\nname\tage\nada\t36\n");
        assert_eq!(positions(&doc)[2..], [(4, 1), (4, 5)]);
        let doc = parse_tsv("name\tage\nada\t36\nlin\t28\n");
        assert_eq!(
            positions(&doc),
            vec![(2, 1), (2, 1), (2, 1), (2, 5), (3, 1), (3, 1), (3, 5)]
        );
    }

    #[test]
    fn csv_header_cell_across_lines_is_still_the_header() {
        let doc = parse_csv("a,\"b\nx\",c\nc,2,3\n");
        assert_eq!(
            fields(&doc, 1),
            vec![
                ("a".into(), "c".into(), 3, 1),
                ("b\nx".into(), "2".into(), 3, 3),
                ("c".into(), "3".into(), 3, 5),
            ]
        );
        assert!(
            doc.nodes.iter().all(|n| n.line == 0 || n.line == 3),
            "nothing on the header's lines: {:?}",
            positions(&doc)
        );
    }

    #[test]
    fn csv_bracket_cell_is_a_value_not_the_roots_opener() {
        for (src, open) in [("a,b\n[,x\n", "["), ("a,b\n{,x\n", "{")] {
            let doc = parse_csv(src);
            assert_eq!(
                positions(&doc),
                vec![(2, 1), (2, 1), (2, 1), (2, 3)],
                "{src:?}"
            );
            assert_eq!(fields(&doc, 1)[0].1, open);
        }
    }

    #[test]
    fn csv_empty_first_header_cell_names_a_column() {
        let doc = parse_csv(",age\nage,36\n");
        assert_eq!(
            fields(&doc, 1),
            vec![
                ("".into(), "age".into(), 2, 1),
                ("age".into(), "36".into(), 2, 5),
            ]
        );
        assert!(
            doc.nodes.iter().all(|n| n.line != 1),
            "{:?}",
            positions(&doc)
        );
    }

    #[test]
    fn csv_repeated_header_name_places_the_kept_column() {
        let doc = parse_csv("a,a\nx,x\n");
        assert_eq!(fields(&doc, 1), vec![("a".into(), "x".into(), 2, 3)]);
        let doc = parse_csv("a,b,a\ny,y,y\n");
        assert_eq!(
            fields(&doc, 1),
            vec![
                ("a".into(), "y".into(), 2, 5),
                ("b".into(), "y".into(), 2, 3)
            ]
        );
    }

    #[test]
    fn csv_crlf_records_sit_on_their_lines() {
        let doc = parse_csv("a,b\r\n1,2\r\n3,4\r\n");
        assert_eq!(
            positions(&doc),
            vec![(2, 1), (2, 1), (2, 1), (2, 3), (3, 1), (3, 1), (3, 3)]
        );
    }

    #[test]
    fn csv_blank_line_between_records_makes_no_record() {
        let doc = parse_csv("a,b\n1,2\n\n3,4\n");
        assert_eq!(doc.nodes[0].children, 2, "the plugin makes two records");
        assert_eq!(
            fields(&doc, 1),
            vec![
                ("a".into(), "1".into(), 2, 1),
                ("b".into(), "2".into(), 2, 3)
            ]
        );
        assert_eq!(
            fields(&doc, 4),
            vec![
                ("a".into(), "3".into(), 4, 1),
                ("b".into(), "4".into(), 4, 3)
            ]
        );
    }

    #[test]
    fn csv_line_of_spaces_is_a_record_of_its_own() {
        let doc = parse_csv("a,b\n1,2\n  \n3,4\n");
        assert_eq!(doc.nodes[0].children, 3);
        assert_eq!(
            fields(&doc, 4),
            vec![
                ("a".into(), "  ".into(), 0, 0),
                ("b".into(), "".into(), 0, 0)
            ]
        );
        assert_eq!(
            fields(&doc, 7),
            vec![
                ("a".into(), "3".into(), 4, 1),
                ("b".into(), "4".into(), 4, 3)
            ]
        );
    }

    #[test]
    fn csv_last_record_without_a_newline_is_placed() {
        let doc = parse_csv("a,b\n1,2");
        assert_eq!(positions(&doc), vec![(2, 1), (2, 1), (2, 1), (2, 3)]);
    }

    #[test]
    fn csv_quoted_cell_holding_the_separator_is_one_cell() {
        let doc = parse_csv("a,b\n\"x,y\",2\n");
        assert_eq!(
            fields(&doc, 1),
            vec![
                ("a".into(), "x,y".into(), 2, 1),
                ("b".into(), "2".into(), 2, 7)
            ]
        );
    }

    #[test]
    fn csv_short_record_leaves_the_missing_field_unplaced() {
        let doc = parse_csv("a,b,c\n1,2\n");
        assert_eq!(
            fields(&doc, 1),
            vec![
                ("a".into(), "1".into(), 2, 1),
                ("b".into(), "2".into(), 2, 3),
                ("c".into(), "".into(), 0, 0),
            ]
        );
    }

    #[test]
    fn csv_extra_cell_falls_back_to_values_on_the_records_line() {
        let doc = parse_csv("a,b\n1,2,3\n");
        assert_eq!(
            fields(&doc, 1),
            vec![
                ("a".into(), "1".into(), 2, 1),
                ("b".into(), "2".into(), 2, 3),
                ("field~2".into(), "3".into(), 2, 5),
            ]
        );
    }

    #[test]
    fn csv_cell_of_several_words_is_placed_at_its_first() {
        let doc = parse_csv("first name,age\nada smith, 36\n");
        assert_eq!(
            fields(&doc, 1),
            vec![
                ("first name".into(), "ada smith".into(), 2, 1),
                ("age".into(), " 36".into(), 2, 12),
            ]
        );
    }

    #[test]
    fn capture_cells_keeps_separators_and_line_ends() {
        let mut p = tabnas_csv::make();
        let sink = capture_cells(&mut p);
        p.parse("a,\"b\nx\"\n1,,3\n").unwrap();
        let toks = sink.lock().unwrap();
        let shape: Vec<(bool, bool, u32)> =
            toks.iter().map(|t| (t.sep, t.line_end, t.rows)).collect();
        assert_eq!(
            shape,
            vec![
                (false, false, 0),
                (true, false, 0),
                (false, false, 1),
                (false, true, 1),
                (false, false, 0),
                (true, false, 0),
                (true, false, 0),
                (false, false, 0),
                (false, true, 1),
            ]
        );
    }
}
