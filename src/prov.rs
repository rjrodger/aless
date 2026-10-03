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
//!
//! Records keyed by a header row (CSV, TSV) are the one shape whose keys
//! are not in the stream where the values are: every record's keys are
//! the header's cells, lexed once, before any value, and a value is told
//! from the next only by the column it sits in. For these formats the
//! capture keeps the field separator and line-end tokens too, and
//! [`align_records`] aligns by column: the first line with a token is the
//! header, each later line is a record, and each field is placed at the
//! token of the cell its column name denotes, with no key ever looked for
//! by name and no bracket looked for at all (the root array and the
//! records have none in the source). A record whose line does not spell
//! its fields, a shape the plugin made that this does not foresee, falls
//! back to matching its values in order along that line alone, so nothing
//! is placed off its line.

use std::collections::HashMap;
use std::ops::Range;
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
    /// The engine's word token (`#TX`), whatever its text: under a grammar
    /// of plain text a lone `*` is a word, and a value.
    pub text: bool,
    /// How many line breaks the token's text spans: a quoted CSV cell
    /// holding a newline ends on `line + rows`, and a line-end token
    /// standing for a run of blank lines counts each of them.
    pub rows: u32,
    /// The field separator of a format keyed by a header row (`#CA`:
    /// tabnas-csv binds its separator, a comma or a tab, to that fixed
    /// token).
    pub sep: bool,
    /// A line end (`#LN`), which the csv grammar takes out of its ignore
    /// set so that a row break reaches its rules and its subscribers.
    pub line_end: bool,
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
    capture_with(parser, false, false)
}

/// [`capture`], keeping a word token (`#TX`) too when it is a single
/// punctuation character: under a grammar from the command line the text
/// is plain, so `*` (a passwd password) is a word and a value, where under
/// the JSON family punctuation stands for itself and is not kept.
pub fn capture_words(parser: &mut Tabnas) -> Capture {
    capture_with(parser, true, false)
}

/// [`capture`], keeping every field separator and line end too, for a
/// format of records keyed by a header row (CSV, TSV): they carry the
/// column structure [`align_records`] aligns by, where a value's text
/// alone cannot tell which cell it is.
pub fn capture_cells(parser: &mut Tabnas) -> Capture {
    capture_with(parser, false, true)
}

fn capture_with(parser: &mut Tabnas, words: bool, cells: bool) -> Capture {
    let sink: Capture = Arc::new(Mutex::new(Vec::new()));
    let s2 = sink.clone();
    parser.subscribe_tokens(move |t: &Token| {
        let tok = Tok::from_token(t);
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

/// [`align`] for a document of records keyed by a header row (CSV, TSV):
/// an array of objects whose keys are the header's cells, over a stream
/// from [`capture_cells`], which keeps the separators and line ends.
///
/// The lines are the token runs between line ends. The first with a token
/// is the header, and its cells name the columns; each record then takes
/// the next line with a token (a blank line makes no record), and when
/// that line spells the record, every field whose column holds a token is
/// placed at it and a field whose cell is empty is left unplaced. No key
/// is ever looked for by name, since no record carries its keys in the
/// source, and no bracket is looked for: the root array and the records
/// are the plugin's, with nothing in the source to stand for them, and a
/// cell that is a lone `[` is a value.
///
/// A line that does not spell its record (a shape the plugin made that
/// this does not foresee: a cell past the header's columns, a nested
/// value) is matched the way [`align`] matches values, in order, over that
/// line's tokens alone, so the record's fields stay on the record's line
/// and the next record starts fresh on the next.
pub fn align_records(doc: &mut Doc, toks: &[Tok]) {
    let mut lines = split_runs(toks).skip_while(|run| run.is_empty());
    if let Some(header) = lines.next() {
        let columns = header_columns(header);
        let records: Vec<u32> = doc.children(0).collect();
        for record in records {
            // The record's line is the next one with a token, unless the
            // plugin made a record of a line without one (`record.empty`,
            // or spaces alone), which only a record of empty fields spells.
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

/// The lines of a header-keyed document: the token runs between its line
/// ends, in order, a blank line an empty run. A quoted cell holding a
/// newline is one string token and never splits a line.
fn split_runs(toks: &[Tok]) -> impl Iterator<Item = &[Tok]> {
    toks.split(|t| t.line_end)
}

/// A line's cells: the token runs between its field separators, in order,
/// an empty cell an empty run.
fn cells(run: &[Tok]) -> impl Iterator<Item = &[Tok]> {
    run.split(|t| t.sep)
}

/// `s` with its whitespace left out.
fn squeeze(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

/// The text a cell's tokens spell, whitespace left out: the lexer cuts
/// `first name` into two words and keeps the space apart from both, and
/// reads ` 36` as a space and a word, so a cell is compared to a key or a
/// value with the whitespace of both set aside. `None` for a token that
/// has no text to compare (a cell with no token spells the empty string).
fn cell_text(cell: &[Tok]) -> Option<String> {
    let mut text = String::new();
    for t in cell {
        match &t.val {
            TokVal::Str(s) => text.push_str(&squeeze(s)),
            TokVal::Num(n) if t.digit => text.push_str(&crate::fmt::number(*n)),
            _ => return None,
        }
    }
    Some(text)
}

/// The header's column names, each mapped to its column. A name that
/// repeats maps to its last column: tabnas-csv keeps one member under the
/// name, in the first column's place, holding the last column's value.
fn header_columns(header: &[Tok]) -> HashMap<String, usize> {
    let mut columns = HashMap::new();
    for (k, cell) in cells(header).enumerate() {
        if let Some(name) = cell_text(cell) {
            columns.insert(name, k);
        }
    }
    columns
}

/// Whether a cell's tokens spell a field's value. One token is held to the
/// value as any token is ([`matches_kind`]); a value the lexer cut into
/// words, or read with its leading space, is their text with the
/// whitespace set aside. A cell with no token spells the plugin's empty
/// field, the empty string.
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

/// Whether `run` is the record's line: the record is an object, each of
/// its fields names a header column, and each holds what its cell spells.
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

/// Place each field of a record at the first token of its cell; a field
/// whose cell has none (an empty cell, a short record) stays unplaced.
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

/// What [`align_nodes`] looks for in the stream besides a node's value.
#[derive(Clone, Copy)]
struct Look {
    /// A node's name, for a node that has one.
    keys: bool,
    /// A container's opening bracket, for a container not placed by its
    /// key.
    openers: bool,
}

/// The alignment proper, over the nodes `range` names and the tokens
/// given, in order, with a bounded lookahead from a cursor that advances
/// past each match.
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
            // A container with no key of its own (the root, an array
            // element) sits where its opening bracket was lexed. The
            // bracket must be right at hand: a container a grammar
            // synthesised, like the JSON Lines root array, has no bracket,
            // and a wider look would seize the next nested container's and
            // drag every later match off by one.
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

/// Containers without a position of their own take their first positioned
/// child's; children have larger indices, so a reverse sweep sees them
/// final.
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

    /// Under a grammar of plain text a lone punctuation character is a
    /// word (`*` for a passwd password), and is placed like any word; the
    /// JSON family's capture leaves such a token out.
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

    /// The first record's fields, as `(key, value, line, col)`.
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

    /// A record's keys are the header's cells, so the first record's first
    /// field used to be placed on the header line, at the cell that spells
    /// its key; every field sits at its own value.
    #[test]
    fn csv_fields_sit_at_their_values_and_never_on_the_header() {
        let doc = parse_csv("name,age,city\nada,36,london\nlin,28,helsinki\n");
        assert_eq!(
            positions(&doc),
            vec![
                (2, 1), // the root array: its first record
                (2, 1), // [0]
                (2, 1), // [0].name  (was 1:1, the header cell)
                (2, 5), // [0].age
                (2, 8), // [0].city
                (3, 1), // [1]
                (3, 1), // [1].name
                (3, 5), // [1].age
                (3, 8), // [1].city
            ]
        );
    }

    /// Values that spell column names are values, not keys.
    #[test]
    fn csv_values_that_spell_a_column_name_do_not_shift_the_record() {
        let doc = parse_csv("a,b\nb,a\n");
        assert_eq!(positions(&doc)[2..], [(2, 1), (2, 3)]);
    }

    /// A short record's missing field has no value token and no position;
    /// the fields after it keep theirs, and so does the next record.
    #[test]
    fn csv_empty_cell_is_unplaced_and_the_rest_stay_put() {
        let doc = parse_csv("name,age,city\nada,36\nlin,28,helsinki\n");
        assert_eq!(
            positions(&doc)[2..],
            [(2, 1), (2, 5), (0, 0), (3, 1), (3, 1), (3, 5), (3, 8)]
        );
    }

    /// The header is the first line with a token, however many blank lines
    /// come before it, for CSV and for TSV alike.
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

    /// A quoted header cell may hold a newline, so the header's last cells
    /// sit on a later physical line than its first: they are still the
    /// header's, and the first record's `c` is not matched to one of them.
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

    /// The root array and the records have no bracket in the source, so a
    /// cell that is a lone `[` or `{` is a value, not the root's opener.
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

    /// A first header cell the capture drops (empty here) still names a
    /// column, `""`, and the header's other cells are not matched to the
    /// first record's values.
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

    /// Under a repeated header name the plugin keeps one member, in the
    /// first column's place, with the last column's value: it is placed at
    /// the column its value came from.
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

    /// Line ends of either kind end a record.
    #[test]
    fn csv_crlf_records_sit_on_their_lines() {
        let doc = parse_csv("a,b\r\n1,2\r\n3,4\r\n");
        assert_eq!(
            positions(&doc),
            vec![(2, 1), (2, 1), (2, 1), (2, 3), (3, 1), (3, 1), (3, 3)]
        );
    }

    /// A blank line between records makes no record; the record after it
    /// is on the line after it.
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

    /// A line of spaces alone is a record to the plugin, of a field of
    /// spaces and an empty one: it takes its own line, unplaced, and the
    /// next record is not shifted onto it.
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

    /// The last record needs no line end after it.
    #[test]
    fn csv_last_record_without_a_newline_is_placed() {
        let doc = parse_csv("a,b\n1,2");
        assert_eq!(positions(&doc), vec![(2, 1), (2, 1), (2, 1), (2, 3)]);
    }

    /// A quoted cell may hold the separator: it is one cell, and the next
    /// field sits past its closing quote.
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

    /// A short record's missing field is the empty string, unplaced; the
    /// fields before it are placed.
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

    /// A record with a cell past the header's columns gets a field the
    /// header does not name (`field~2`), a shape the column alignment does
    /// not foresee: the record falls back to value matching along its own
    /// line, and every field is still on line 2.
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

    /// A cell the lexer cuts into words, or reads with its leading space,
    /// is still its field's cell, placed at its first word.
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

    /// The tokens a header-keyed capture keeps beyond the values: every
    /// separator and line end, so the lines and cells can be told apart.
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
                (false, false, 0), // a
                (true, false, 0),  // ,
                (false, false, 1), // "b\nx": spans one line break
                (false, true, 1),  // \n
                (false, false, 0), // 1
                (true, false, 0),  // ,
                (true, false, 0),  // ,
                (false, false, 0), // 3
                (false, true, 1),  // \n
            ]
        );
    }
}
