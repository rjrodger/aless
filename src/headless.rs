//! aless without a screen, for scripts and agents.
//!
//! A run reads one document (`--check` reads any number), picks where to
//! start (`--path`, `--at`, else the root) and writes one JSON value to
//! standard output. A failure writes `{"error": {…}}` to standard error
//! instead, and the exit status says what kind of failure it was (see
//! [`status`]). The output shapes are a contract: fields may be added, but
//! none is renamed, removed or given a new meaning.
//!
//! Nothing here touches the terminal. `main.rs` decides when to come here,
//! and prints what [`run`] returns.

use std::io;
use std::path::PathBuf;

use serde_json::{json, Map, Value};

use crate::doc::{Doc, Key, Kind, NodeId};
use crate::fmt;
use crate::load::{self, Format, LoadError, Loaded};
use crate::search;

/// Exit statuses.
pub mod status {
    /// Standard output holds the answer.
    pub const OK: i32 = 0;
    /// The input did not parse. With `--check`: an input failed, and
    /// standard output says which and why.
    pub const PARSE: i32 = 1;
    /// The command line asked for something aless cannot do: a bad option
    /// or path syntax, no input, a directory, or the viewer without a
    /// terminal.
    pub const USAGE: i32 = 2;
    /// An input could not be read.
    pub const IO: i32 = 3;
    /// `--path` or `--at` names nothing in the document.
    pub const NOT_FOUND: i32 = 4;
}

/// How many entries `--paths` and `--find` print unless `--limit` says.
pub const DEFAULT_LIMIT: usize = 200;

/// A string value in an entry is cut to this many characters.
pub const VALUE_CHARS: usize = 200;

/// How many keys a not-found error lists.
const KEYS_LISTED: usize = 20;

/// What to print.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    /// The value at the start, as JSON.
    Json,
    /// The start and the nodes below it, one entry each.
    Paths,
    /// The nodes whose row text matches a search pattern.
    Find(String),
    /// The start's entry: its path and source position.
    Where,
    /// Whether each input parses.
    Check,
}

impl Op {
    /// The option that asks for this.
    pub fn flag(&self) -> &'static str {
        match self {
            Op::Json => "--json",
            Op::Paths => "--paths",
            Op::Find(_) => "--find",
            Op::Where => "--where",
            Op::Check => "--check",
        }
    }
}

/// Where in the document an operation starts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Start {
    #[default]
    Root,
    /// A path as given to `--path`; see [`parse_path`].
    Path(String),
    /// A 1-based source line, and optionally a column, as given to `--at`.
    At(u32, Option<u32>),
}

impl Start {
    /// `--at`'s argument: `LINE` or `LINE:COL`, both from 1.
    pub fn parse_at(text: &str) -> Result<Start, String> {
        let bad = || format!("--at needs LINE or LINE:COL, counted from 1, not {text:?}");
        let (line, col) = match text.split_once(':') {
            Some((l, c)) => (l, Some(c)),
            None => (text, None),
        };
        let line: u32 = line.trim().parse().map_err(|_| bad())?;
        let col: Option<u32> = match col {
            Some(c) => Some(c.trim().parse().map_err(|_| bad())?),
            None => None,
        };
        if line == 0 || col == Some(0) {
            return Err(bad());
        }
        Ok(Start::At(line, col))
    }
}

#[derive(Clone, Debug)]
pub struct Request {
    pub op: Op,
    /// The inputs; `-` is standard input. None at all means standard input.
    pub files: Vec<PathBuf>,
    /// Parse every input as this, rather than by extension.
    pub kind: Option<Format>,
    pub start: Start,
    /// How many levels below the start `--paths` and `--find` go.
    pub depth: Option<u32>,
    /// At most this many entries from `--paths` and `--find`; 0 for all.
    pub limit: usize,
    /// Print JSON on one line.
    pub compact: bool,
    /// Indentation per level of `--json` output.
    pub indent: usize,
}

impl Request {
    pub fn new(op: Op) -> Request {
        Request {
            op,
            files: Vec::new(),
            kind: None,
            start: Start::Root,
            depth: None,
            limit: DEFAULT_LIMIT,
            compact: false,
            indent: 2,
        }
    }
}

/// What a run prints, and the status to exit with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Output {
    pub stdout: String,
    pub stderr: String,
    pub status: i32,
}

/// Reads all of standard input; the caller passes none when standard
/// input is a terminal, which nobody is going to type a document into.
pub type Stdin<'a> = Option<&'a mut dyn FnMut() -> io::Result<Vec<u8>>>;

/// Carry out a request.
pub fn run(req: &Request, mut stdin: Stdin<'_>) -> Output {
    let result = match req.op {
        Op::Check => check(req, &mut stdin),
        _ => single(req, &mut stdin),
    };
    match result {
        Ok((stdout, status)) => Output {
            stdout,
            stderr: String::new(),
            status,
        },
        Err(f) => Output {
            stdout: String::new(),
            stderr: render(&json!({ "error": f.error }), req.compact),
            status: f.status,
        },
    }
}

fn single(req: &Request, stdin: &mut Stdin<'_>) -> Result<(String, i32), Failure> {
    // A bad pattern is a mistake in the command; say so before reading.
    let pattern = match &req.op {
        Op::Find(p) => Some(search::compile(p).map_err(Failure::usage)?),
        _ => None,
    };
    let sources = sources(req);
    if sources.len() > 1 {
        return Err(Failure::usage(format!(
            "{} reads one input, and was given {}: run aless once per file, or use --check to check several",
            req.op.flag(),
            sources.len()
        )));
    }
    let (name, loaded) = load(&sources[0], req.kind, stdin, req.files.is_empty())?;
    let doc = &loaded.doc;
    let start = select(doc, &req.start, &name, loaded.format)?;
    let head = |m: &mut Map<String, Value>| {
        m.insert("file".into(), name.clone().into());
        m.insert("format".into(), loaded.format.name().into());
        m.insert("path".into(), fmt::path_jq(&doc.path(start)).into());
    };
    let out = match &req.op {
        Op::Json => {
            let mut s = if req.compact {
                fmt::to_json_compact(doc, start)
            } else {
                fmt::to_json_pretty(doc, start, req.indent)
            };
            s.push('\n');
            s
        }
        Op::Paths => {
            let (entries, total) = walk(doc, start, req.depth, req.limit, |_| true);
            let mut m = Map::new();
            head(&mut m);
            m.insert("entries".into(), Value::Array(entries));
            listed(&mut m, total, req.limit);
            render(&Value::Object(m), req.compact)
        }
        Op::Find(p) => {
            let regex = &pattern.as_ref().expect("compiled above").regex;
            let (matches, total) = walk(doc, start, req.depth, req.limit, |id| {
                regex.is_match(&fmt::search_text(doc, id))
            });
            let mut m = Map::new();
            head(&mut m);
            m.insert("pattern".into(), p.clone().into());
            m.insert("matches".into(), Value::Array(matches));
            listed(&mut m, total, req.limit);
            render(&Value::Object(m), req.compact)
        }
        Op::Where => {
            let mut m = Map::new();
            m.insert("file".into(), name.clone().into());
            m.insert("format".into(), loaded.format.name().into());
            m.extend(entry(doc, start));
            render(&Value::Object(m), req.compact)
        }
        Op::Check => unreachable!("--check is run by check()"),
    };
    Ok((out, status::OK))
}

/// `--check`: every input is read and parsed, and each gets a verdict.
fn check(req: &Request, stdin: &mut Stdin<'_>) -> Result<(String, i32), Failure> {
    let mut files = Vec::new();
    let mut all_ok = true;
    for source in sources(req) {
        let (name, format) = match &source {
            Source::Stdin => ("-".to_string(), req.kind.unwrap_or(Format::Json)),
            Source::File(p) => (
                p.display().to_string(),
                req.kind.unwrap_or_else(|| Format::detect(p)),
            ),
        };
        let verdict = match load(&source, req.kind, stdin, req.files.is_empty()) {
            Ok(_) => Value::Null,
            // Nothing to check at all is a mistake in the command.
            Err(f) if f.status == status::USAGE && req.files.is_empty() => return Err(f),
            Err(f) => Value::Object(f.error),
        };
        all_ok &= verdict.is_null();
        files.push(json!({
            "file": name,
            "format": format.name(),
            "ok": verdict.is_null(),
            "error": verdict,
        }));
    }
    let out = render(&json!({ "ok": all_ok, "files": files }), req.compact);
    Ok((out, if all_ok { status::OK } else { status::PARSE }))
}

/// Record how much of a listing was printed.
fn listed(m: &mut Map<String, Value>, total: usize, limit: usize) {
    m.insert("total".into(), total.into());
    m.insert("limit".into(), limit.into());
    m.insert("truncated".into(), (limit != 0 && total > limit).into());
}

// ----- inputs --------------------------------------------------------------

enum Source {
    File(PathBuf),
    Stdin,
}

fn sources(req: &Request) -> Vec<Source> {
    if req.files.is_empty() {
        return vec![Source::Stdin];
    }
    req.files
        .iter()
        .map(|f| {
            if f.as_os_str() == "-" {
                Source::Stdin
            } else {
                Source::File(f.clone())
            }
        })
        .collect()
}

/// Read and parse one input. Returns the name outputs call it by: the
/// path as given, or `-` for standard input.
fn load(
    source: &Source,
    kind: Option<Format>,
    stdin: &mut Stdin<'_>,
    implicit: bool,
) -> Result<(String, Loaded), Failure> {
    match source {
        Source::Stdin => {
            let format = kind.unwrap_or(Format::Json);
            let Some(read) = stdin.as_mut() else {
                return Err(Failure::usage(if implicit {
                    "no input: name a FILE, or pipe a document into aless"
                } else {
                    "standard input is a terminal: pipe a document into aless, or name a FILE"
                }));
            };
            let bytes = read().map_err(|e| {
                Failure::load(
                    "-",
                    format,
                    &LoadError::new(e.to_string()).with_origin("(stdin)"),
                )
            })?;
            if implicit && bytes.is_empty() {
                return Err(Failure::usage(
                    "no input: standard input is empty; name a FILE, or pipe a document into aless",
                ));
            }
            let text = match String::from_utf8(bytes) {
                Ok(s) => s,
                Err(e) => String::from_utf8_lossy(e.as_bytes()).into_owned(),
            };
            let loaded = load::load_str(text, format)
                .map_err(|e| Failure::load("-", format, &e.with_origin("(stdin)")))?;
            Ok(("-".to_string(), loaded))
        }
        Source::File(path) => {
            let name = path.display().to_string();
            if path.is_dir() {
                return Err(Failure::usage(format!(
                    "{name} is a directory: aless reads files (list a directory with ls or find)"
                )));
            }
            let format = kind.unwrap_or_else(|| Format::detect(path));
            let loaded =
                load::load_path(path, kind).map_err(|e| Failure::load(&name, format, &e))?;
            Ok((name, loaded))
        }
    }
}

// ----- paths ---------------------------------------------------------------

/// One step of a path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Seg {
    /// `.name`, `."name"`, `["name"]`, `['name']` or a JSON Pointer token:
    /// a key of an object; on an array, an index written in decimal (`.0`,
    /// `/0`).
    Name(String),
    /// `[3]`, counting from the end when negative (`[-1]` is the last
    /// item), as in jq; on an object, the key with that decimal text.
    Index(i64),
}

/// Parse a path. Accepted, so that whatever syntax a caller reaches for
/// works:
///
/// - jq's: `.`, `.a.b[0]`, `."odd key"`, `.["odd key"]`, `.[0]` — the form
///   aless prints, so a path from any output can be passed back as it is;
/// - the same without the leading dot (`a.b[0]`), or JSONPath's `$` in its
///   place (`$.a['odd key'][0]`);
/// - a JSON Pointer (RFC 6901): `/a/b/0`, with `~1` for `/` and `~0` for
///   `~`.
///
/// The empty path, `.` and `$` are the root. Wildcards, slices and
/// recursive descent are refused: that is jq's work, and `--json` pipes
/// into jq.
pub fn parse_path(text: &str) -> Result<Vec<Seg>, String> {
    let t = text.trim();
    if let Some(pointer) = t.strip_prefix('/') {
        return Ok(pointer
            .split('/')
            .map(|tok| Seg::Name(tok.replace("~1", "/").replace("~0", "~")))
            .collect());
    }
    let t = t.strip_prefix('$').unwrap_or(t);
    let chars: Vec<char> = t.chars().collect();
    let err = |at: usize, what: &str| format!("bad path {text:?} at character {}: {what}", at + 1);
    let mut segs = Vec::new();
    let mut i = 0;
    if i < chars.len() && chars[i] != '.' && chars[i] != '[' {
        // A bare first key: `a.b` for `.a.b`.
        let (name, next) = bare(&chars, i);
        segs.push(Seg::Name(name));
        i = next;
    }
    while i < chars.len() {
        match chars[i] {
            '.' => {
                i += 1;
                match chars.get(i) {
                    None if segs.is_empty() => {}
                    None => return Err(err(i - 1, "a key is missing after the last '.'")),
                    Some('.') => {
                        return Err(err(
                            i - 1,
                            "'..' (recursive descent) is not supported: use --find, or pipe --json into jq",
                        ))
                    }
                    // jq's `.[0]`: the bracket is read next time round.
                    Some('[') => {}
                    Some('"') => {
                        let (name, next) = quoted(&chars, i).map_err(|w| err(i, &w))?;
                        segs.push(Seg::Name(name));
                        i = next;
                    }
                    Some(_) => {
                        let (name, next) = bare(&chars, i);
                        segs.push(Seg::Name(name));
                        i = next;
                    }
                }
            }
            '[' => {
                let open = i;
                i = skip_space(&chars, i + 1);
                match chars.get(i) {
                    Some('"') | Some('\'') => {
                        let (name, next) = quoted(&chars, i).map_err(|w| err(i, &w))?;
                        segs.push(Seg::Name(name));
                        i = next;
                    }
                    Some(c) if c.is_ascii_digit() || *c == '-' => {
                        let from = i;
                        i += 1;
                        while chars.get(i).is_some_and(|c| c.is_ascii_digit()) {
                            i += 1;
                        }
                        let digits: String = chars[from..i].iter().collect();
                        let n: i64 = digits
                            .parse()
                            .map_err(|_| err(from, "expected an index such as [0] or [-1]"))?;
                        segs.push(Seg::Index(n));
                    }
                    Some(']') | Some('*') => {
                        return Err(err(
                            open,
                            "wildcards are not supported: name one item, or pipe --json into jq",
                        ))
                    }
                    _ => {
                        return Err(err(
                            i,
                            "expected an index such as [0], or a quoted key such as [\"a b\"]",
                        ))
                    }
                }
                i = skip_space(&chars, i);
                match chars.get(i) {
                    Some(']') => i += 1,
                    Some(':') => {
                        return Err(err(
                            i,
                            "slices are not supported: name one item, or pipe --json into jq",
                        ))
                    }
                    _ => return Err(err(i, "expected ']'")),
                }
            }
            _ => return Err(err(i, "expected '.' or '['")),
        }
    }
    Ok(segs)
}

/// A key written bare: everything up to the next `.` or `[`.
fn bare(chars: &[char], from: usize) -> (String, usize) {
    let mut i = from;
    while i < chars.len() && chars[i] != '.' && chars[i] != '[' {
        i += 1;
    }
    (chars[from..i].iter().collect(), i)
}

fn skip_space(chars: &[char], mut i: usize) -> usize {
    while chars.get(i).is_some_and(|c| c.is_whitespace()) {
        i += 1;
    }
    i
}

/// A quoted key starting at `from`: a JSON string, or a single-quoted one
/// in which `\'` and `\\` are escapes. Returns the key and where it ends.
fn quoted(chars: &[char], from: usize) -> Result<(String, usize), String> {
    let q = chars[from];
    let mut i = from + 1;
    while i < chars.len() && chars[i] != q {
        i += if chars[i] == '\\' { 2 } else { 1 };
    }
    if i >= chars.len() {
        return Err(format!("the key quoted with {q} is not closed"));
    }
    let body: String = chars[from + 1..i].iter().collect();
    let key = if q == '"' {
        serde_json::from_str::<String>(&format!("\"{body}\""))
            .map_err(|e| format!("bad quoted key: {e}"))?
    } else {
        let mut out = String::new();
        let mut it = body.chars();
        while let Some(c) = it.next() {
            match (c, c == '\\') {
                (_, true) => match it.next() {
                    Some(e @ ('\'' | '\\')) => out.push(e),
                    Some(e) => {
                        out.push('\\');
                        out.push(e);
                    }
                    None => out.push('\\'),
                },
                (c, false) => out.push(c),
            }
        }
        out
    };
    Ok((key, i + 1))
}

/// The child `seg` names, if there is one.
fn step(doc: &Doc, id: NodeId, seg: &Seg) -> Option<NodeId> {
    let node = doc.node(id);
    match (&node.kind, seg) {
        (Kind::Array, Seg::Index(i)) => {
            let n = i64::from(node.children);
            let i = if *i < 0 { n + i } else { *i };
            if !(0..n).contains(&i) {
                return None;
            }
            doc.child_by_key(id, &Key::Index(i as u32))
        }
        (Kind::Array, Seg::Name(s)) => {
            let canonical = s == "0" || (!s.starts_with('0') && !s.is_empty());
            let i: u32 = s
                .parse()
                .ok()
                .filter(|_| canonical && s.bytes().all(|b| b.is_ascii_digit()))?;
            doc.child_by_key(id, &Key::Index(i))
        }
        (Kind::Object, Seg::Name(s)) => doc.child_by_key(id, &Key::Name(s.as_str().into())),
        (Kind::Object, Seg::Index(i)) => {
            doc.child_by_key(id, &Key::Name(i.to_string().as_str().into()))
        }
        _ => None,
    }
}

/// Follow a path from the root. On a miss, the deepest node reached and
/// the index of the step that failed from it.
pub fn resolve(doc: &Doc, segs: &[Seg]) -> Result<NodeId, (NodeId, usize)> {
    let mut cur: NodeId = 0;
    for (i, seg) in segs.iter().enumerate() {
        cur = step(doc, cur, seg).ok_or((cur, i))?;
    }
    Ok(cur)
}

/// The node at a source position. On a line with nodes on it: the one
/// starting furthest right at or before `col`, or when no column is given
/// the one the line starts with; of nodes starting at the same place, the
/// innermost. On a line with none (a comment, a closing bracket, the inside
/// of a long string): the node that starts last before it. `None` when no
/// node has a known position.
pub fn node_at(doc: &Doc, line: u32, col: Option<u32>) -> Option<NodeId> {
    let known = || (0..doc.len() as NodeId).filter(|&i| doc.node(i).line != 0);
    let on_line: Vec<NodeId> = known().filter(|&i| doc.node(i).line == line).collect();
    if let Some(first) = on_line.iter().map(|&i| doc.node(i).col).min() {
        let target = col.unwrap_or(first).max(first);
        let best = on_line
            .iter()
            .map(|&i| doc.node(i).col)
            .filter(|&c| c <= target)
            .max()
            .unwrap_or(first);
        return on_line
            .iter()
            .rev()
            .find(|&&i| doc.node(i).col == best)
            .copied();
    }
    known()
        .filter(|&i| doc.node(i).line < line)
        .max_by_key(|&i| (doc.node(i).line, doc.node(i).col, i))
        .or_else(|| known().next())
}

/// The node an operation starts at.
fn select(doc: &Doc, start: &Start, file: &str, format: Format) -> Result<NodeId, Failure> {
    match start {
        Start::Root => Ok(0),
        Start::Path(text) => {
            let segs = parse_path(text).map_err(Failure::usage)?;
            resolve(doc, &segs).map_err(|(near, at)| {
                let why = match &doc.node(near).kind {
                    Kind::Object => match &segs[at] {
                        Seg::Name(k) => format!("has no key {}", fmt::quote(k)),
                        Seg::Index(i) => format!("has no key \"{i}\""),
                    },
                    Kind::Array => {
                        let n = doc.node(near).children;
                        match n {
                            0 => "is an empty array".to_string(),
                            1 => "has 1 item, [0]".to_string(),
                            n => format!("has {n} items, [0] to [{}]", n - 1),
                        }
                    }
                    k => format!("is {}, which has no keys", with_article(kind_name(k))),
                };
                let near_path = match near {
                    0 => "the root (.)".to_string(),
                    _ => fmt::path_jq(&doc.path(near)),
                };
                let message = format!("no {} in {file}: {near_path} {why}", text.trim());
                Failure::not_found(doc, file, format, ("path", text.trim()), near, message)
            })
        }
        Start::At(line, col) => {
            let at = match col {
                Some(c) => format!("{line}:{c}"),
                None => line.to_string(),
            };
            node_at(doc, *line, *col).ok_or_else(|| {
                let message = format!("no node in {file} has a known source position");
                Failure::not_found(doc, file, format, ("at", &at), NodeId::MAX, message)
            })
        }
    }
}

// ----- entries -------------------------------------------------------------

/// A node's kind, in jq's words.
pub fn kind_name(kind: &Kind) -> &'static str {
    match kind {
        Kind::Object => "object",
        Kind::Array => "array",
        Kind::Str(_) => "string",
        Kind::Number(_) => "number",
        Kind::Bool(_) => "boolean",
        Kind::Null => "null",
    }
}

fn with_article(word: &str) -> String {
    match word.chars().next() {
        Some('a' | 'e' | 'i' | 'o' | 'u') => format!("an {word}"),
        _ => format!("a {word}"),
    }
}

/// A number as JSON: integers without a fraction, as [`fmt::number`]
/// prints them; `NaN` and the infinities, which JSON cannot hold, as the
/// strings `"NaN"`, `"Infinity"` and `"-Infinity"`.
fn number_value(n: f64) -> Value {
    if !n.is_finite() {
        return Value::String(fmt::number(n));
    }
    if n.fract() == 0.0 && n.abs() < 1e16 {
        return Value::from(n as i64);
    }
    serde_json::Number::from_f64(n).map_or(Value::Null, Value::Number)
}

/// What every listing prints for a node: its `path` (jq syntax, which
/// `--path` takes back), `kind`, 1-based source `line` and `col` (`null`
/// when unknown), and either `length` (a container's item count) or
/// `value` (a scalar's value; a string longer than [`VALUE_CHARS`] is cut
/// to that many characters, and `truncated` and `length` say so).
pub fn entry(doc: &Doc, id: NodeId) -> Map<String, Value> {
    let node = doc.node(id);
    let mut m = Map::new();
    m.insert("path".into(), fmt::path_jq(&doc.path(id)).into());
    m.insert("kind".into(), kind_name(&node.kind).into());
    let pos = |n: u32| if n == 0 { Value::Null } else { n.into() };
    m.insert("line".into(), pos(node.line));
    m.insert("col".into(), pos(node.col));
    match &node.kind {
        Kind::Object | Kind::Array => {
            m.insert("length".into(), node.children.into());
        }
        Kind::Str(s) => {
            let n = s.chars().count();
            if n > VALUE_CHARS {
                let cut: String = s.chars().take(VALUE_CHARS).collect();
                m.insert("value".into(), cut.into());
                m.insert("truncated".into(), true.into());
                m.insert("length".into(), n.into());
            } else {
                m.insert("value".into(), s.to_string().into());
            }
        }
        Kind::Number(n) => {
            m.insert("value".into(), number_value(*n));
        }
        Kind::Bool(b) => {
            m.insert("value".into(), (*b).into());
        }
        Kind::Null => {
            m.insert("value".into(), Value::Null);
        }
    }
    m
}

/// The entries of the nodes at and below `start` (to `depth` levels) that
/// `keep` accepts, in document order, at most `limit` of them (0: all);
/// and how many were accepted in all.
fn walk(
    doc: &Doc,
    start: NodeId,
    depth: Option<u32>,
    limit: usize,
    keep: impl Fn(NodeId) -> bool,
) -> (Vec<Value>, usize) {
    let base = doc.node(start).depth;
    let end = doc.subtree_end(start);
    let mut out = Vec::new();
    let mut total = 0;
    let mut id = start;
    while id < end {
        if keep(id) {
            total += 1;
            if limit == 0 || out.len() < limit {
                out.push(Value::Object(entry(doc, id)));
            }
        }
        id = if depth.is_some_and(|d| doc.node(id).depth - base >= d) {
            doc.subtree_end(id)
        } else {
            id + 1
        };
    }
    (out, total)
}

// ----- failures ------------------------------------------------------------

/// Why a run failed: the status to exit with and the error object.
struct Failure {
    status: i32,
    error: Map<String, Value>,
}

impl Failure {
    /// `{"kind": "usage", "message"}`.
    fn usage(message: impl Into<String>) -> Failure {
        let mut error = Map::new();
        error.insert("kind".into(), "usage".into());
        error.insert("message".into(), message.into().into());
        Failure {
            status: status::USAGE,
            error,
        }
    }

    /// An input that could not be read (`"kind": "io"`) or parsed
    /// (`"parse"`), with every field of the engine's report; those that do
    /// not apply are `null`.
    fn load(file: &str, format: Format, e: &LoadError) -> Failure {
        let (kind, status) = if e.is_io() {
            ("io", status::IO)
        } else {
            ("parse", status::PARSE)
        };
        let some = |s: &str| {
            if s.is_empty() {
                Value::Null
            } else {
                s.into()
            }
        };
        let pos = |n: u32| if n == 0 { Value::Null } else { n.into() };
        let mut error = Map::new();
        error.insert("kind".into(), kind.into());
        error.insert("file".into(), file.into());
        error.insert("format".into(), format.name().into());
        error.insert("code".into(), e.code.clone().into());
        error.insert("message".into(), e.message.clone().into());
        error.insert("line".into(), pos(e.line));
        error.insert("col".into(), pos(e.col));
        error.insert("hint".into(), some(&e.hint));
        error.insert(
            "source_line".into(),
            e.source_line.as_deref().map_or(Value::Null, Value::from),
        );
        error.insert("report".into(), e.plain_report().into());
        Failure { status, error }
    }

    /// `--path` or `--at` named nothing: `{"kind": "not_found", "file",
    /// "format", "path" or "at", "message", "nearest", "keys"}`, where
    /// `nearest` is the entry of the deepest node the path did reach, and
    /// `keys` the first of that node's keys when it is an object.
    fn not_found(
        doc: &Doc,
        file: &str,
        format: Format,
        asked: (&str, &str),
        near: NodeId,
        message: String,
    ) -> Failure {
        let mut error = Map::new();
        error.insert("kind".into(), "not_found".into());
        error.insert("file".into(), file.into());
        error.insert("format".into(), format.name().into());
        error.insert(asked.0.into(), asked.1.into());
        error.insert("message".into(), message.into());
        let (nearest, keys) = if (near as usize) < doc.len() {
            let keys = match doc.node(near).kind {
                Kind::Object => Value::Array(
                    doc.children(near)
                        .take(KEYS_LISTED)
                        .filter_map(|c| doc.node(c).key.name().map(|k| k.into()))
                        .collect(),
                ),
                _ => Value::Null,
            };
            (Value::Object(entry(doc, near)), keys)
        } else {
            (Value::Null, Value::Null)
        };
        error.insert("nearest".into(), nearest);
        error.insert("keys".into(), keys);
        Failure {
            status: status::NOT_FOUND,
            error,
        }
    }
}

/// The error for a result that could not be written to standard output
/// (a full disk, say): an `io` error, exit status 3, whose `file` is null
/// because no input is at fault.
pub fn write_failure(e: &io::Error, compact: bool) -> String {
    let error = json!({ "error": {
        "kind": "io",
        "file": null,
        "format": null,
        "code": "io",
        "message": format!("cannot write standard output: {e}"),
        "line": null,
        "col": null,
        "hint": null,
        "source_line": null,
        "report": null,
    }});
    render(&error, compact)
}

// ----- printing ------------------------------------------------------------

/// How many levels of a result are laid out one item per line.
const OPEN_LEVELS: usize = 2;

/// A result as aless prints it, newline included. `compact` puts it all on
/// one line. Otherwise the top two levels take one item per line and
/// anything deeper stays on one line, so a list of entries reads, and
/// greps, one entry per line.
pub fn render(v: &Value, compact: bool) -> String {
    let mut out = String::new();
    if compact {
        out.push_str(&v.to_string());
    } else {
        write(&mut out, v, 0);
    }
    out.push('\n');
    out
}

fn write(out: &mut String, v: &Value, depth: usize) {
    let pad = |out: &mut String, d: usize| out.extend(std::iter::repeat_n(' ', d * 2));
    match v {
        Value::Object(m) if depth < OPEN_LEVELS && !m.is_empty() => {
            out.push_str("{\n");
            for (i, (k, v)) in m.iter().enumerate() {
                pad(out, depth + 1);
                out.push_str(&Value::String(k.clone()).to_string());
                out.push_str(": ");
                write(out, v, depth + 1);
                if i + 1 < m.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            pad(out, depth);
            out.push('}');
        }
        Value::Array(a) if depth < OPEN_LEVELS && !a.is_empty() => {
            out.push_str("[\n");
            for (i, v) in a.iter().enumerate() {
                pad(out, depth + 1);
                write(out, v, depth + 1);
                if i + 1 < a.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            pad(out, depth);
            out.push(']');
        }
        v => out.push_str(&v.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "{\n  \"a\": 1,\n  \"b\": [true, null, \"x\"],\n  \"c\": {\"d\": 2.5, \"odd key\": \"v\"}\n}\n";

    fn doc(src: &str) -> Doc {
        load::parse(src, Format::Json).unwrap()
    }

    /// Run `req` with `src` on standard input.
    fn with_stdin(req: &Request, src: &str) -> Output {
        let bytes = src.as_bytes().to_vec();
        let mut read = move || Ok(bytes.clone());
        run(req, Some(&mut read))
    }

    fn json_of(text: &str) -> Value {
        serde_json::from_str(text).unwrap_or_else(|e| panic!("{e}: {text}"))
    }

    fn req(op: Op) -> Request {
        Request::new(op)
    }

    #[test]
    fn path_syntaxes() {
        use Seg::{Index as I, Name as N};
        let n = |s: &str| N(s.to_string());
        for root in ["", ".", "$", " . ", "$."] {
            assert_eq!(parse_path(root), Ok(vec![]), "{root:?}");
        }
        let want = vec![n("a"), n("b"), I(0), n("c")];
        for p in [
            ".a.b[0].c",
            "a.b[0].c",
            "$.a.b[0].c",
            ".a.b.[0].c",
            "$['a'][\"b\"][ 0 ].c",
        ] {
            assert_eq!(parse_path(p), Ok(want.clone()), "{p:?}");
        }
        assert_eq!(
            parse_path("/a/b/0/c"),
            Ok(vec![n("a"), n("b"), n("0"), n("c")])
        );
        assert_eq!(parse_path("/"), Ok(vec![n("")]));
        assert_eq!(parse_path("/a~1b/c~0d"), Ok(vec![n("a/b"), n("c~d")]));
        assert_eq!(
            parse_path(r#"."odd key"."q\"t""#),
            Ok(vec![n("odd key"), n("q\"t")])
        );
        assert_eq!(parse_path(r#".["a.b"]"#), Ok(vec![n("a.b")]));
        assert_eq!(parse_path(r"['it\'s']"), Ok(vec![n("it's")]));
        assert_eq!(parse_path(".x[-1]"), Ok(vec![n("x"), I(-1)]));
        assert_eq!(parse_path(".0"), Ok(vec![n("0")]));
        assert_eq!(parse_path(r#"."é""#), Ok(vec![n("é")]));
        for (bad, why) in [
            (".a.", "missing after the last '.'"),
            ("..a", "recursive descent"),
            (".a[*]", "wildcards"),
            (".a[]", "wildcards"),
            (".a[1:2]", "slices"),
            (".a[x]", "expected an index"),
            (".a[0", "expected ']'"),
            (r#"."open"#, "not closed"),
            (".a]b[", "expected an index"),
        ] {
            let e = parse_path(bad).unwrap_err();
            assert!(e.contains(why), "{bad:?}: {e}");
        }
    }

    #[test]
    fn resolution_is_lenient_about_names_and_indices() {
        let d = doc(r#"{"a": [10, 20, {"0": "zero", "k": [1]}], "1": "one"}"#);
        let at = |p: &str| resolve(&d, &parse_path(p).unwrap()).ok();
        let path = |id: Option<NodeId>| id.map(|i| fmt::path_jq(&d.path(i)));
        assert_eq!(path(at(".a[1]")), Some(".a[1]".into()));
        assert_eq!(
            path(at(".a.1")),
            Some(".a[1]".into()),
            "a decimal name indexes an array"
        );
        assert_eq!(path(at("/a/1")), Some(".a[1]".into()));
        assert_eq!(path(at(".a[-1]")), Some(".a[2]".into()));
        assert_eq!(path(at(".a[-3]")), Some(".a[0]".into()));
        assert_eq!(at(".a[-4]"), None);
        assert_eq!(at(".a[3]"), None);
        assert_eq!(at(".a.01"), None, "only the canonical decimal is an index");
        assert_eq!(
            path(at("[1]")),
            Some(r#"."1""#.into()),
            "an index on an object is its key"
        );
        assert_eq!(path(at(".a[2][0]")), Some(r#".a[2]."0""#.into()));
        // The miss: `.a[2].k` (node 6) has no item 5, the fourth step.
        assert_eq!(resolve(&d, &parse_path(".a[2].k[5]").unwrap()), Err((6, 3)));
    }

    #[test]
    fn every_printed_path_resolves_back() {
        let src =
            r#"{"plain": 1, "odd key": {"é": [0, {"a.b": 2, "q\"t": 3, "": 4, "$x": 5, "0": 6}]}}"#;
        let d = doc(src);
        for id in 0..d.len() as NodeId {
            let printed = fmt::path_jq(&d.path(id));
            let segs = parse_path(&printed).unwrap_or_else(|e| panic!("{printed}: {e}"));
            assert_eq!(resolve(&d, &segs), Ok(id), "{printed}");
        }
    }

    #[test]
    fn positions_pick_the_node_a_tool_means() {
        // 1 {
        // 2   "a": 1,
        // 3   "b": [true, null, "x"],
        // 4   "c": {"d": 2.5, "odd key": "v"}
        // 5 }
        let d = doc(DOC);
        let at = |l, c| node_at(&d, l, c).map(|i| fmt::path_jq(&d.path(i)));
        assert_eq!(at(1, None).as_deref(), Some("."));
        assert_eq!(at(2, None).as_deref(), Some(".a"));
        assert_eq!(at(3, None).as_deref(), Some(".b"), "the line's own node");
        assert_eq!(
            at(3, Some(1)).as_deref(),
            Some(".b"),
            "before the first node"
        );
        assert_eq!(at(3, Some(16)).as_deref(), Some(".b[1]"), "inside `null`");
        assert_eq!(at(3, Some(99)).as_deref(), Some(".b[2]"));
        assert_eq!(at(4, Some(12)).as_deref(), Some(".c.d"));
        assert_eq!(
            at(5, None).as_deref(),
            Some(r#".c."odd key""#),
            "the node before"
        );
        assert_eq!(at(99, None).as_deref(), Some(r#".c."odd key""#));
        // Nested nodes starting at the same place: the innermost.
        let y = load::parse("items:\n  - name: web\n    port: 80\n", Format::Yaml).unwrap();
        let path = node_at(&y, 2, None).map(|i| fmt::path_jq(&y.path(i)));
        assert_eq!(path.as_deref(), Some(".items[0].name"));
        let none = Doc::from_lines(&[]);
        assert_eq!(node_at(&none, 1, None), None);
    }

    #[test]
    fn at_parses_line_and_column() {
        assert_eq!(Start::parse_at("42"), Ok(Start::At(42, None)));
        assert_eq!(Start::parse_at("42:7"), Ok(Start::At(42, Some(7))));
        for bad in ["0", "4:0", "x", "4:", ":4", "-1", "1:2:3"] {
            assert!(Start::parse_at(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn entries_bound_their_values() {
        let long = "é".repeat(VALUE_CHARS + 5);
        let d = doc(&format!(
            r#"[1, 2.5, 1e300, "{long}", "short", false, null, {{}}, [1, 2]]"#
        ));
        let e = |i: NodeId| Value::Object(entry(&d, i));
        assert_eq!(
            e(0),
            json!({"path": ".", "kind": "array", "line": 1, "col": 1, "length": 9})
        );
        assert_eq!(e(1)["value"], json!(1));
        assert_eq!(e(2)["value"], json!(2.5));
        assert_eq!(e(3)["value"], json!(1e300));
        let s = e(4);
        assert_eq!(s["truncated"], json!(true));
        assert_eq!(s["length"], json!(VALUE_CHARS + 5));
        assert_eq!(s["value"].as_str().unwrap().chars().count(), VALUE_CHARS);
        assert_eq!(e(5)["value"], json!("short"));
        assert!(e(5).get("truncated").is_none());
        assert_eq!(e(6)["value"], json!(false));
        assert_eq!(e(7)["value"], Value::Null);
        assert_eq!(e(8)["length"], json!(0));
        assert_eq!(e(9)["kind"], json!("array"));
        // JSON has no NaN: the value is its name, and the kind still says
        // number.
        let nan = load::parse("[NaN, -Infinity]", Format::Json5).unwrap();
        assert_eq!(entry(&nan, 1)["value"], json!("NaN"));
        assert_eq!(entry(&nan, 2)["value"], json!("-Infinity"));
        assert_eq!(entry(&nan, 1)["kind"], json!("number"));
        // A position nobody knows is null, not 0.
        let mut unknown = doc("[1]");
        unknown.nodes[1].line = 0;
        unknown.nodes[1].col = 0;
        assert_eq!(entry(&unknown, 1)["line"], Value::Null);
    }

    #[test]
    fn json_prints_the_start() {
        let mut r = req(Op::Json);
        let out = with_stdin(&r, DOC);
        assert_eq!(out.status, status::OK);
        assert_eq!(out.stderr, "");
        assert_eq!(json_of(&out.stdout)["c"]["odd key"], json!("v"));
        assert!(out.stdout.ends_with("}\n"), "{}", out.stdout);
        r.start = Start::Path(".c".into());
        r.compact = true;
        let out = with_stdin(&r, DOC);
        assert_eq!(out.stdout, "{\"d\":2.5,\"odd key\":\"v\"}\n");
        r.start = Start::At(3, Some(16));
        assert_eq!(with_stdin(&r, DOC).stdout, "null\n");
    }

    #[test]
    fn paths_list_to_a_depth_and_a_limit() {
        let mut r = req(Op::Paths);
        let all = json_of(&with_stdin(&r, DOC).stdout);
        assert_eq!(all["file"], json!("-"));
        assert_eq!(all["format"], json!("json"));
        assert_eq!(all["path"], json!("."));
        assert_eq!(all["total"], json!(9));
        assert_eq!(all["truncated"], json!(false));
        let paths: Vec<&str> = all["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["path"].as_str().unwrap())
            .collect();
        assert_eq!(
            paths,
            [
                ".",
                ".a",
                ".b",
                ".b[0]",
                ".b[1]",
                ".b[2]",
                ".c",
                ".c.d",
                r#".c."odd key""#
            ]
        );
        assert_eq!(
            all["entries"][2],
            json!({"path": ".b", "kind": "array", "line": 3, "col": 3, "length": 3})
        );

        r.depth = Some(1);
        let top = json_of(&with_stdin(&r, DOC).stdout);
        assert_eq!(top["total"], json!(4));
        r.depth = Some(0);
        r.start = Start::Path(".c".into());
        let one = json_of(&with_stdin(&r, DOC).stdout);
        assert_eq!(one["path"], json!(".c"));
        assert_eq!(one["total"], json!(1));

        r.depth = None;
        r.start = Start::Root;
        r.limit = 2;
        let cut = json_of(&with_stdin(&r, DOC).stdout);
        assert_eq!(cut["entries"].as_array().unwrap().len(), 2);
        assert_eq!(cut["total"], json!(9));
        assert_eq!(cut["limit"], json!(2));
        assert_eq!(cut["truncated"], json!(true));
        r.limit = 0;
        let whole = json_of(&with_stdin(&r, DOC).stdout);
        assert_eq!(whole["entries"].as_array().unwrap().len(), 9);
        assert_eq!(whole["truncated"], json!(false));
    }

    #[test]
    fn find_uses_the_viewers_search() {
        let mut r = req(Op::Find("\"d\":".into()));
        let out = json_of(&with_stdin(&r, DOC).stdout);
        assert_eq!(out["pattern"], json!("\"d\":"));
        assert_eq!(out["total"], json!(1));
        assert_eq!(
            out["matches"][0],
            json!({"path": ".c.d", "kind": "number", "line": 4, "col": 9, "value": 2.5})
        );
        // Smart case, and `/s` to match case.
        r.op = Op::Find("NULL".into());
        assert_eq!(json_of(&with_stdin(&r, DOC).stdout)["total"], json!(0));
        r.op = Op::Find("null".into());
        assert_eq!(json_of(&with_stdin(&r, DOC).stdout)["total"], json!(1));
        r.op = Op::Find("ODD/s".into());
        assert_eq!(json_of(&with_stdin(&r, DOC).stdout)["total"], json!(0));
        // Within the start. Brackets match themselves, as in the viewer.
        r.op = Op::Find("[0-9]".into());
        assert_eq!(json_of(&with_stdin(&r, DOC).stdout)["total"], json!(0));
        r.op = Op::Find(r"\d".into());
        r.start = Start::Path(".c".into());
        let under = json_of(&with_stdin(&r, DOC).stdout);
        assert_eq!(under["path"], json!(".c"));
        assert_eq!(under["total"], json!(1));
        // A pattern that is not a regex is a usage error, found before
        // any input is read.
        r.op = Op::Find("(".into());
        let mut read = || -> io::Result<Vec<u8>> { panic!("read input for a bad pattern") };
        let bad = run(&r, Some(&mut read));
        assert_eq!(bad.status, status::USAGE);
        assert!(json_of(&bad.stderr)["error"]["message"]
            .as_str()
            .unwrap()
            .starts_with("Invalid regex"));
    }

    #[test]
    fn where_gives_the_entry_and_its_file() {
        let mut r = req(Op::Where);
        r.start = Start::Path("/c/odd key".into());
        let out = json_of(&with_stdin(&r, DOC).stdout);
        assert_eq!(
            out,
            json!({"file": "-", "format": "json", "path": ".c.\"odd key\"", "kind": "string", "line": 4, "col": 19, "value": "v"})
        );
    }

    #[test]
    fn parse_errors_carry_everything_the_report_does() {
        let out = with_stdin(&req(Op::Json), "{\"a\": 1,\n  \"b\" 2\n}\n");
        assert_eq!(out.status, status::PARSE);
        assert_eq!(out.stdout, "");
        let e = &json_of(&out.stderr)["error"];
        assert_eq!(e["kind"], json!("parse"));
        assert_eq!(e["file"], json!("-"));
        assert_eq!(e["format"], json!("json"));
        assert_eq!(e["code"], json!("unexpected"));
        // The engine reports the pair it could not match, at its key.
        assert_eq!((e["line"].clone(), e["col"].clone()), (json!(2), json!(3)));
        assert_eq!(e["source_line"], json!("  \"b\" 2"));
        assert!(e["hint"].as_str().unwrap().contains("do not match"), "{e}");
        let report = e["report"].as_str().unwrap();
        assert!(report.contains("--> (stdin):2:3"), "{report}");
        assert!(!report.contains('\u{1b}'), "no colour codes: {report}");
    }

    #[test]
    fn failures_say_what_kind_they_are() {
        let usage = |out: Output| {
            assert_eq!(out.status, status::USAGE, "{}", out.stderr);
            assert_eq!(out.stdout, "");
            let e = json_of(&out.stderr);
            assert_eq!(e["error"]["kind"], json!("usage"));
            e["error"]["message"].as_str().unwrap().to_string()
        };
        // Nothing to read.
        assert!(usage(run(&req(Op::Json), None)).starts_with("no input"));
        assert!(usage(with_stdin(&req(Op::Json), "")).contains("standard input is empty"));
        let mut dash = req(Op::Json);
        dash.files = vec!["-".into()];
        assert!(usage(run(&dash, None)).contains("standard input is a terminal"));
        // One input at a time, except for --check.
        let mut two = req(Op::Paths);
        two.files = vec!["a.json".into(), "b.json".into()];
        assert!(usage(run(&two, None)).contains("--paths reads one input, and was given 2"));
        // A directory.
        let mut dir = req(Op::Json);
        dir.files = vec![std::env::temp_dir()];
        assert!(usage(run(&dir, None)).contains("is a directory"));
        // Bad path syntax.
        let mut bad = req(Op::Json);
        bad.start = Start::Path(".a[".into());
        assert!(usage(with_stdin(&bad, DOC)).starts_with("bad path"));

        // A file that is not there.
        let mut missing = req(Op::Json);
        missing.files = vec!["no/such/file.yaml".into()];
        let out = run(&missing, None);
        assert_eq!(out.status, status::IO);
        let e = &json_of(&out.stderr)["error"];
        assert_eq!(e["kind"], json!("io"));
        assert_eq!(e["code"], json!("io"));
        assert_eq!(e["file"], json!("no/such/file.yaml"));
        assert_eq!(e["format"], json!("yaml"));
        assert_eq!(e["line"], Value::Null);

        // A path the document does not have.
        let mut absent = req(Op::Json);
        absent.start = Start::Path(".c.nope".into());
        let out = with_stdin(&absent, DOC);
        assert_eq!(out.status, status::NOT_FOUND);
        let e = &json_of(&out.stderr)["error"];
        assert_eq!(e["kind"], json!("not_found"));
        assert_eq!(e["path"], json!(".c.nope"));
        assert_eq!(e["nearest"]["path"], json!(".c"));
        assert_eq!(e["keys"], json!(["d", "odd key"]));
        assert_eq!(
            e["message"],
            json!("no .c.nope in -: .c has no key \"nope\"")
        );
        absent.start = Start::Path(".b[7]".into());
        let e = json_of(&with_stdin(&absent, DOC).stderr);
        assert_eq!(
            e["error"]["message"],
            json!("no .b[7] in -: .b has 3 items, [0] to [2]")
        );
        assert_eq!(e["error"]["keys"], Value::Null);
        absent.start = Start::Path(".a.x".into());
        let e = json_of(&with_stdin(&absent, DOC).stderr);
        assert_eq!(
            e["error"]["message"],
            json!("no .a.x in -: .a is a number, which has no keys")
        );
    }

    #[test]
    fn check_reports_every_input() {
        let dir = std::env::temp_dir().join(format!("aless-check-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let good = dir.join("good.toml");
        let bad = dir.join("bad.json");
        std::fs::write(&good, "a = 1\n").unwrap();
        std::fs::write(&bad, "{\"a\": }\n").unwrap();
        let mut r = req(Op::Check);
        r.files = vec![good.clone()];
        let out = run(&r, None);
        assert_eq!(out.status, status::OK);
        let v = json_of(&out.stdout);
        assert_eq!(v["ok"], json!(true));
        assert_eq!(
            v["files"][0],
            json!({"file": good.display().to_string(), "format": "toml", "ok": true, "error": null})
        );
        r.files = vec![good, bad.clone(), dir.join("gone.yaml"), dir.clone()];
        let out = run(&r, None);
        assert_eq!(out.status, status::PARSE);
        assert_eq!(out.stderr, "");
        let v = json_of(&out.stdout);
        assert_eq!(v["ok"], json!(false));
        let files = v["files"].as_array().unwrap();
        assert_eq!(files.len(), 4);
        assert_eq!(files[1]["ok"], json!(false));
        assert_eq!(files[1]["error"]["kind"], json!("parse"));
        assert_eq!(files[1]["error"]["line"], json!(1));
        assert_eq!(files[2]["error"]["kind"], json!("io"));
        assert_eq!(files[3]["error"]["kind"], json!("usage"));
        // One report line per input.
        assert_eq!(out.stdout.lines().count(), 4 + 5);
        // Standard input when no file is named.
        let r = req(Op::Check);
        let v = json_of(&with_stdin(&r, "[1").stdout);
        assert_eq!(v["files"][0]["file"], json!("-"));
        assert_eq!(v["files"][0]["ok"], json!(false));
        assert_eq!(run(&r, None).status, status::USAGE);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn kind_overrides_detection() {
        let mut r = req(Op::Json);
        r.kind = Some(Format::Yaml);
        r.compact = true;
        let out = with_stdin(&r, "a: [1, 2]\n");
        assert_eq!(out.stdout, "{\"a\":[1,2]}\n");
        // Standard input is JSON unless told otherwise.
        let out = with_stdin(&req(Op::Json), "a: [1, 2]\n");
        assert_eq!(out.status, status::PARSE);
    }

    #[test]
    fn an_unwritable_output_is_an_io_error_with_no_file() {
        let e = io::Error::new(io::ErrorKind::StorageFull, "no space left");
        let v = json_of(&write_failure(&e, false));
        assert_eq!(v["error"]["kind"], json!("io"));
        assert_eq!(v["error"]["code"], json!("io"));
        assert_eq!(v["error"]["file"], Value::Null);
        assert_eq!(
            v["error"]["message"],
            json!("cannot write standard output: no space left")
        );
        // The same fields as any io error, in the same order.
        let missing = run(
            &{
                let mut r = req(Op::Json);
                r.files = vec!["no/such.json".into()];
                r
            },
            None,
        );
        let keys = |v: &Value| -> Vec<String> {
            v["error"].as_object().unwrap().keys().cloned().collect()
        };
        assert_eq!(keys(&v), keys(&json_of(&missing.stderr)));
        assert!(!write_failure(&e, true).trim_end().contains('\n'));
    }

    #[test]
    fn rendering_keeps_entries_on_one_line_each() {
        let v =
            json!({"a": 1, "list": [{"x": [1, 2]}, {"y": {}}], "empty": [], "o": {"p": {"q": 1}}});
        assert_eq!(
            render(&v, false),
            "{\n  \"a\": 1,\n  \"list\": [\n    {\"x\":[1,2]},\n    {\"y\":{}}\n  ],\n  \"empty\": [],\n  \"o\": {\n    \"p\": {\"q\":1}\n  }\n}\n"
        );
        assert_eq!(
            render(&v, true),
            "{\"a\":1,\"list\":[{\"x\":[1,2]},{\"y\":{}}],\"empty\":[],\"o\":{\"p\":{\"q\":1}}}\n"
        );
        assert_eq!(json_of(&render(&v, false)), v);
    }
}
