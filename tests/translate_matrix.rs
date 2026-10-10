//! The cross product `--render` answers for (admin ADR-27, plan item
//! P3.2): every document of every corpus, read with its format's grammar,
//! written by aless in every format it writes, read back with that
//! format's grammar, and compared with the document's own value under the
//! target's declared conventions (its loss list). The writes run aless's
//! own routes, the native JSON and CSV ones and the composed ones alike,
//! from the source aless's plan takes for the format: JSON Lines and CSV a
//! record at a time, the verified grammars as the parse proceeds, the rest
//! whole. A pair that fails to write, that writes a document its own
//! grammar refuses, or that reads back as anything but the conventions say
//! is a failure, and so is a corpus that shrinks. Two refusals are a
//! target's declared outcome, and counted as such: a schema-only target's
//! (CSS, proto, PGN, which write only the tree their own documents read
//! as) of a document of another tree, before anything is read, with the
//! usage error naming the program route; and a version's (semver's embed)
//! of a tree that is not a version, `TARGET_VALUE_UNREPRESENTABLE` before
//! anything is written.
//!
//! The corpora are the grammar repositories' own, at the tags of the
//! versions Cargo.lock pins, which `scripts/fixtures.sh` clones and names
//! in `TABNAS_FIXTURES_DIR`: aless's own fixtures and the documents of
//! JSONTestSuite every JSON parser must accept (jsonc's) in the default
//! run, and every repository's `test/spec/*.tsv`, thousands of documents,
//! in the ignored run CI makes in the `matrix` profile. A document its own
//! grammar refuses (ZON's repeated fields) is no document, and is counted
//! as one refused. The conventions are those of tabnas/alchemy-cli's
//! matrix (`rs/tests/translate_test.rs`), which writes the same pairs
//! through the composed routes alone.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, Cursor, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use aless::export::{self, ExportError, Input, Job, Plan, Records, Renderer, What};
use aless::load::Format;
use aless::translate::{self, Options, Part, Shape, Translation};
use tabnas::Tabnas;
use tabnas_alchemy::{Output, Source};
use tabnas_transduce::{
    capability, Code, Datum, DatumBuilder, Duplicates, Fail, Flow, JsonEvent, Limits, Metrics,
    ParserSource, Prune, Sink, SourceMode,
};

/// What a run wrote, shared with the test.
#[derive(Clone, Default)]
struct Buffer(Arc<Mutex<Vec<u8>>>);

impl Write for Buffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The version of a crate Cargo.lock pins.
fn locked(name: &str) -> String {
    let lock = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.lock"))
        .expect("Cargo.lock");
    let heading = format!("name = \"{name}\"");
    let mut lines = lock.lines();
    while let Some(line) = lines.next() {
        if line == heading {
            if let Some(v) = lines.next().and_then(|l| l.strip_prefix("version = \"")) {
                return v.trim_end_matches('"').to_string();
            }
        }
    }
    panic!("Cargo.lock pins no {name}")
}

/// The grammar repository a format's id is from: PGN's is tabnas/chess,
/// whose manifest's `languageId` is `pgn`; every other is its id's.
fn repository(id: &str) -> &str {
    match id {
        "pgn" => "chess",
        id => id,
    }
}

/// The checkout of a grammar repository at the tag of the version
/// Cargo.lock pins: `TABNAS_FIXTURES_DIR/<repository>/<version>`, a
/// relative directory taken from this crate's root. Without the variable,
/// or without that checkout, this fails rather than reading no fixtures.
fn checkout(repository: &str) -> PathBuf {
    let dir = std::env::var_os("TABNAS_FIXTURES_DIR").unwrap_or_else(|| {
        panic!(
            "TABNAS_FIXTURES_DIR is not set: it names the grammar repositories' checkouts at the \
             tags of the versions Cargo.lock pins; `eval \"$(scripts/fixtures.sh)\"` clones them \
             and sets it"
        )
    });
    let version = locked(&format!("tabnas-{repository}"));
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(dir)
        .join(repository)
        .join(&version);
    assert!(
        dir.join("test/spec").is_dir(),
        "no tabnas/{repository} {version} checkout at {}: run scripts/fixtures.sh",
        dir.display()
    );
    dir
}

/// The eighteen formats aless writes, by id, and the format aless reads a
/// document of each with.
const FORMATS: [&str; 18] = [
    "css", "csv", "expr", "feed", "ini", "json", "json5", "jsonc", "jsonic", "jsonl", "markdown",
    "pgn", "proto", "semver", "toml", "xml", "yaml", "zon",
];

fn format(id: &str) -> Format {
    Format::from_name(id).unwrap_or_else(|| panic!("{id} is a format"))
}

fn target(id: &str) -> &'static Part {
    translate::part(id).unwrap_or_else(|| panic!("{id} has a render"))
}

/// The format a fixture's extension names, by its id.
fn format_of(extension: &str) -> Option<&'static str> {
    Some(match extension {
        "json" => "json",
        "json5" => "json5",
        "jsonc" => "jsonc",
        "jsonic" => "jsonic",
        "jsonl" => "jsonl",
        "csv" => "csv",
        "md" => "markdown",
        "toml" => "toml",
        "ini" => "ini",
        "xml" => "xml",
        "yaml" => "yaml",
        "zon" => "zon",
        "rss" | "atom" => "feed",
        "css" => "css",
        "proto" => "proto",
        "pgn" => "pgn",
        // Formats that declare no extension: aless's fixtures name them.
        "expr" => "expr",
        "semver" => "semver",
        _ => return None,
    })
}

/// The documents of `dir` whose extension names one of the formats, and
/// whose name starts with `prefix` when one is given.
fn documents(
    corpus: &str,
    dir: &Path,
    prefix: Option<&str>,
    docs: &mut Vec<(String, &'static str, String)>,
) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{corpus}: cannot read {}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if prefix.is_some_and(|p| !name.starts_with(p)) {
            continue;
        }
        let Some(id) = path
            .extension()
            .and_then(|e| e.to_str())
            .and_then(format_of)
        else {
            continue;
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        docs.push((format!("{corpus}/{name}"), id, text));
    }
}

/// aless's own fixtures, and the documents of JSONTestSuite every JSON
/// parser must accept.
fn corpus() -> Vec<(String, &'static str, String)> {
    let mut docs = Vec::new();
    documents(
        "aless",
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures"),
        None,
        &mut docs,
    );
    documents(
        "JSONTestSuite",
        &checkout("jsonc").join("test/JSONTestSuite/test_parsing"),
        Some("y_"),
        &mut docs,
    );
    docs
}

/// A fixture cell decoded as the fleet's fixture runner decodes it:
/// `\n`, `\r`, `\t` and `\\`, every other character as it is.
fn unescape(cell: &str) -> String {
    let mut out = String::with_capacity(cell.len());
    let mut chars = cell.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            let decoded = match chars.peek() {
                Some('n') => Some('\n'),
                Some('r') => Some('\r'),
                Some('t') => Some('\t'),
                Some('\\') => Some('\\'),
                _ => None,
            };
            if let Some(decoded) = decoded {
                chars.next();
                out.push(decoded);
                continue;
            }
        }
        out.push(c);
    }
    out
}

/// Every format's own fixture corpus: the input of every row of its
/// repository's `test/spec/*.tsv`, decoded as the fleet's fixture runner
/// reads it (a header row; blank lines and `#` lines with no tab skipped;
/// a CR before a line's end dropped), once each. A row that sets options
/// of its own (`opts`) is read by another reader than the format's
/// default, and an error row's input is one the format refuses, so
/// neither is a document of the format here.
fn spec_corpus() -> Vec<(String, &'static str, String)> {
    let mut docs = Vec::new();
    for id in FORMATS {
        let dir = checkout(repository(id)).join("test/spec");
        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("{id}: cannot read {}: {e}", dir.display()))
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e == "tsv") && p.is_file())
            .collect();
        files.sort();
        let mut seen = HashSet::new();
        for file in files {
            let text = std::fs::read_to_string(&file).unwrap();
            let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
            let mut lines = text.split('\n').enumerate();
            let Some((_, header)) = lines.next() else {
                continue;
            };
            let header: Vec<&str> = header.trim_end_matches('\r').split('\t').collect();
            let column = |name: &str| header.iter().position(|h| *h == name);
            let (Some(input), expected, opts) =
                (column("input"), column("expected"), column("opts"))
            else {
                continue;
            };
            let name = file.file_name().unwrap().to_string_lossy().to_string();
            for (i, line) in lines {
                let line = line.strip_suffix('\r').unwrap_or(line);
                if line.is_empty() || (line.starts_with('#') && !line.contains('\t')) {
                    continue;
                }
                let cols: Vec<&str> = line.split('\t').collect();
                let cell =
                    |at: Option<usize>| at.and_then(|at| cols.get(at)).copied().unwrap_or("");
                let expect = cell(expected);
                if !cell(opts).trim().is_empty()
                    || expect == "ERROR"
                    || expect.starts_with("ERROR:")
                {
                    continue;
                }
                let text = unescape(cell(Some(input)));
                if seen.insert(text.clone()) {
                    docs.push((format!("{id}/{name}:{}", i + 1), id, text));
                }
            }
        }
    }
    docs
}

/// The parser aless reads a format with: each grammar's default, JSONC
/// with the trailing comma accepted, as aless's loader has it.
fn parser(id: &str) -> Tabnas {
    match id {
        "csv" => tabnas_csv::make(),
        "ini" => tabnas_ini::make(),
        "json" => tabnas_json::make(),
        "json5" => tabnas_json5::make(),
        "jsonc" => tabnas_jsonc::make_with(
            tabnas_jsonc::JsoncOptions::new().with_allow_trailing_comma(true),
        ),
        "jsonic" => tabnas_jsonic::make(),
        "jsonl" => tabnas_jsonl::make(),
        "markdown" => tabnas_markdown::make(),
        "toml" => tabnas_toml::make(),
        "xml" => tabnas_xml::make(),
        "yaml" => tabnas_yaml::make(),
        "zon" => tabnas_zon::make(),
        "css" => tabnas_css::make(),
        "pgn" => tabnas_chess::make(&tabnas_chess::ChessOptions::default())
            .expect("the chess grammar installs"),
        "feed" => tabnas_feed::make(),
        other => panic!("no parser for {other}"),
    }
}

/// The events of a document, collected into a value.
struct Collect(DatumBuilder);

impl Sink for Collect {
    fn event(&mut self, ev: JsonEvent<'_>) -> Result<Flow, Fail> {
        if !matches!(ev, JsonEvent::End) {
            self.0.event(ev)?;
        }
        Ok(Flow::Continue)
    }
}

/// Whether aless reads a format's document as more than its grammar's
/// value: proto's as its descriptor, expr's as its S-expressions, a version
/// without the line break a file ends with (`load::read_as`,
/// `load::parse_text`).
fn read_by_aless(id: &str) -> bool {
    matches!(id, "proto" | "expr" | "semver")
}

/// A document read whole with a format's grammar, as a value: its events,
/// incrementally where the grammar is verified (so a number keeps the
/// lexeme the document spelled it with), collected, or the grammar's value
/// where the incremental source cannot follow it. A repeated member keeps
/// its last value. A document aless reads as more than its grammar's value
/// ([`read_by_aless`]) is read as aless's own JSON export writes it, which
/// holds every value those three formats' trees can: no number in them is
/// one JSON cannot spell (proto's descriptor holds a NaN as null).
fn read(id: &str, text: &str) -> Result<Datum, Fail> {
    if read_by_aless(id) {
        let out = Buffer::default();
        export::export(
            &job(id, format(id), What::Render(Renderer::Json)),
            input(format(id), text),
            Box::new(out.clone()),
        )
        .map_err(|e| Fail::input(format!("aless does not read it: {e:?}")))?;
        let json = String::from_utf8(out.0.lock().unwrap().clone())
            .map_err(|e| Fail::input(format!("aless wrote no UTF-8: {e}")))?;
        return read("json", &json);
    }
    let once = |mode: SourceMode| {
        let collect = Collect(DatumBuilder::new(
            usize::MAX,
            "max_capture_bytes",
            Duplicates::LastWins,
        ));
        let (outcome, mut collect) = ParserSource::new(parser(id), text)
            .grammar(id)
            .mode(mode)
            .limits(Limits::default())
            .run_owned(collect);
        outcome?;
        collect
            .0
            .take()
            .ok_or_else(|| Fail::input("the document holds no value"))
    };
    if capability::incremental(id) {
        match once(SourceMode::Incremental {
            prune: Prune::Never,
        }) {
            Err(f) if matches!(f.code, Code::StreamabilityUnknown | Code::DuplicateMember) => {}
            outcome => return outcome,
        }
    }
    once(SourceMode::Materialize)
}

thread_local! {
    /// The translations decided so far, by source and target: one serves
    /// every document of a pair.
    static TRANSLATIONS: RefCell<HashMap<(String, String), Rc<Translation>>> =
        RefCell::new(HashMap::new());
}

/// The job aless runs a write under.
fn job(name: &str, from: Format, what: What) -> Job {
    Job {
        name: name.to_string(),
        origin: name.to_string(),
        format: from,
        what,
        path: Vec::new(),
        compact: false,
        indent: 2,
        timeout: None,
        started: None,
        max_output: None,
    }
}

/// The input as aless's plan for the format reads it.
fn input(from: Format, text: &str) -> Input<'_> {
    match export::plan(from).expect("a format with a grammar") {
        Plan::Lines => {
            let reader: Box<dyn BufRead + Send> = Box::new(Cursor::new(text.as_bytes().to_vec()));
            Input::Lines(reader)
        }
        _ => Input::Text(text),
    }
}

/// A write that wrote no document.
enum Unwritten {
    /// A schema-only target (css, proto, pgn) refused a source of another
    /// tree before reading it, with the usage error naming the program
    /// route: that target's declared outcome for such a source.
    SchemaOnly,
    /// The target's embed refused the tree, with
    /// `TARGET_VALUE_UNREPRESENTABLE` and nothing written: a version's
    /// (semver's) declared outcome for a tree that is not a version
    /// (admin's log of the plan, 2026-10-10).
    NotAVersion,
    /// Anything else, a failure.
    Failed(String),
}

/// The schema a format's documents carry, when it has one.
fn schema(id: &str) -> Option<String> {
    translate::source_part(format(id)).and_then(|p| p.schema.clone())
}

/// `text`, read as `from`, written by `--render` into `to`: the route
/// [`translate::translation`] decides, on aless's own renderer or
/// composed.
fn render(name: &str, from: &str, to: &str, text: &str) -> Result<String, Unwritten> {
    let key = (from.to_string(), to.to_string());
    let translation = match TRANSLATIONS.with(|t| t.borrow().get(&key).cloned()) {
        Some(t) => t,
        None => {
            let t = translate::translation(
                translate::source_part(format(from)),
                target(to),
                &Options::default(),
            )
            .map_err(|f| {
                // Only a source of another schema is refused so.
                if translate::schema_only(&f, target(to), name).is_some()
                    && schema(from) != target(to).schema
                {
                    Unwritten::SchemaOnly
                } else {
                    Unwritten::Failed(format!("does not compose: {f}"))
                }
            })?;
            let t = Rc::new(t);
            TRANSLATIONS.with(|c| c.borrow_mut().insert(key, t.clone()));
            t
        }
    };
    let out = Buffer::default();
    let result = match &*translation {
        Translation::Native { renderer, .. } => export::export(
            &job(name, format(from), What::Render(*renderer)),
            input(format(from), text),
            Box::new(out.clone()),
        ),
        Translation::Composed(c) => translate::run(
            &job(name, format(from), What::Part),
            c,
            translate::records(target(to)),
            input(format(from), text),
            Box::new(out.clone()),
            Metrics::new(),
        ),
    };
    let bytes = out.0.lock().unwrap().clone();
    match result {
        Ok(()) => {}
        // A version's embed refuses a tree of another schema that is not a
        // version before the render writes anything.
        Err(ExportError::Program(fail))
            if to == "semver"
                && schema(from) != target(to).schema
                && fail.code == Code::TargetValueUnrepresentable
                && bytes.is_empty() =>
        {
            return Err(Unwritten::NotAVersion)
        }
        Err(e) => return Err(Unwritten::Failed(format!("does not write: {e:?}"))),
    }
    String::from_utf8(bytes).map_err(|e| Unwritten::Failed(format!("writes no UTF-8: {e}")))
}

/// `text`, read as `from`, through a program `program` whose export
/// answers JSON events, written as YAML, where a number that is not finite
/// has a spelling: how the matrix reads a lifted table and an embedding
/// back through their reverses.
fn through_program(file: &str, program: &str, from: &str, text: &str) -> Result<String, String> {
    let c = translate::compose_program(
        Source::new(file, program),
        Output::JsonEvents,
        target("yaml"),
        &Options::default(),
    )
    .map_err(|f| format!("{file} does not compose: {f}"))?;
    let out = Buffer::default();
    translate::run(
        &job(file, format(from), What::Part),
        &c,
        Records::Any,
        input(format(from), text),
        Box::new(out.clone()),
        Metrics::new(),
    )
    .map_err(|e| format!("{file} does not run: {e:?}"))?;
    let bytes = out.0.lock().unwrap().clone();
    String::from_utf8(bytes).map_err(|e| format!("writes no UTF-8: {e}"))
}

// ---------------------------------------------------------------------
// Values compared as the conventions compare them
// ---------------------------------------------------------------------

/// Two values the same: numbers by value (NaN is NaN), objects by their
/// members whatever their order, arrays in order.
fn same(a: &Datum, b: &Datum) -> bool {
    match (a, b) {
        (Datum::Number { value: x, .. }, Datum::Number { value: y, .. }) => {
            x == y || (x.is_nan() && y.is_nan())
        }
        (Datum::Array(x), Datum::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| same(a, b))
        }
        (Datum::Object(x), Datum::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w)))
        }
        (a, b) => a == b,
    }
}

/// The value with every number that is not finite replaced.
fn map_non_finite(d: &Datum, f: &dyn Fn(f64) -> Datum) -> Datum {
    match d {
        Datum::Number { value, .. } if !value.is_finite() => f(*value),
        Datum::Array(items) => Datum::Array(items.iter().map(|i| map_non_finite(i, f)).collect()),
        Datum::Object(members) => Datum::Object(
            members
                .iter()
                .map(|(k, v)| (k.clone(), map_non_finite(v, f)))
                .collect(),
        ),
        d => d.clone(),
    }
}

fn wrap_object(d: &Datum, key: &str) -> Datum {
    match d {
        Datum::Object(_) => d.clone(),
        d => Datum::Object([(key.into(), d.clone())].into_iter().collect()),
    }
}

fn wrap_array(d: &Datum) -> Datum {
    match d {
        Datum::Array(_) => d.clone(),
        d => Datum::Array(vec![d.clone()]),
    }
}

/// TOML's conventions: no null (a member whose value is null is not
/// written, a null element is skipped).
fn without_nulls(d: &Datum) -> Datum {
    match d {
        Datum::Array(items) => Datum::Array(
            items
                .iter()
                .filter(|i| !matches!(i, Datum::Null))
                .map(without_nulls)
                .collect(),
        ),
        Datum::Object(members) => Datum::Object(
            members
                .iter()
                .filter(|(_, v)| !matches!(v, Datum::Null))
                .map(|(k, v)| (k.clone(), without_nulls(v)))
                .collect(),
        ),
        d => d.clone(),
    }
}

/// The text a cell is written as in CSV and Markdown: a string as it is,
/// a number by its value (compared as one), a non-finite one by its word,
/// a boolean by its name, null and an absent member as the empty field, a
/// container as its compact JSON text.
enum Cell {
    Text(String),
    Number(f64),
}

fn cell(d: Option<&Datum>) -> Cell {
    match d {
        None | Some(Datum::Null) => Cell::Text(String::new()),
        Some(Datum::Bool(b)) => Cell::Text(b.to_string()),
        Some(Datum::Number { value, .. }) if value.is_nan() => Cell::Text("NaN".into()),
        Some(Datum::Number { value, .. }) if value.is_infinite() => Cell::Text(
            if *value > 0.0 {
                "Infinity"
            } else {
                "-Infinity"
            }
            .into(),
        ),
        Some(Datum::Number { value, .. }) => Cell::Number(*value),
        Some(Datum::String(s)) => Cell::Text(s.to_string()),
        Some(d) => Cell::Text(d.to_string()),
    }
}

fn cell_is(expected: &Cell, got: &str) -> bool {
    match expected {
        Cell::Text(t) => t == got,
        Cell::Number(n) => got.parse::<f64>().is_ok_and(|g| g == *n),
    }
}

/// The table the inferred binding makes of a value: the rows are the
/// root array's elements (a root of another kind is one row), the columns
/// the first row's (an object's keys, an array's positions, or one
/// `value` column for a scalar), each row's cell found by the column's
/// path.
fn table(d: &Datum) -> (Vec<String>, Vec<Vec<Cell>>) {
    let rows = match wrap_array(d) {
        Datum::Array(rows) => rows,
        _ => unreachable!(),
    };
    enum Path {
        Key(String),
        Index(usize),
        Itself,
    }
    let columns: Vec<(String, Path)> = match rows.first() {
        None => Vec::new(),
        Some(Datum::Object(m)) => m
            .keys()
            .map(|k| (k.to_string(), Path::Key(k.to_string())))
            .collect(),
        Some(Datum::Array(items)) => (0..items.len())
            .map(|i| (i.to_string(), Path::Index(i)))
            .collect(),
        Some(_) => vec![("value".to_string(), Path::Itself)],
    };
    let cells = rows
        .iter()
        .map(|row| {
            columns
                .iter()
                .map(|(_, path)| match path {
                    Path::Key(k) => cell(row.as_object().and_then(|m| m.get(k.as_str()))),
                    Path::Index(i) => cell(row.as_array().and_then(|a| a.get(*i))),
                    Path::Itself => cell(Some(row)),
                })
                .collect()
        })
        .collect();
    (columns.into_iter().map(|(l, _)| l).collect(), cells)
}

/// Whether a read-back table (an array of objects keyed by label, every
/// value a string) is the table the inferred binding makes of `source`,
/// with each cell's text passed through `cell_text` first (Markdown's
/// normalisation of what it writes).
fn check_records(
    source: &Datum,
    back: &Datum,
    cell_text: &dyn Fn(&str) -> String,
) -> Result<(), String> {
    let (labels, rows) = table(source);
    let back_rows = back
        .as_array()
        .ok_or_else(|| format!("read back as {back}, not an array of records"))?;
    if labels.is_empty() {
        return if back_rows.is_empty() {
            Ok(())
        } else {
            Err(format!("a table of no columns read back as {back}"))
        };
    }
    if back_rows.len() != rows.len() {
        return Err(format!(
            "{} rows read back, {} written: {back}",
            back_rows.len(),
            rows.len()
        ));
    }
    for (i, (row, got)) in rows.iter().zip(back_rows).enumerate() {
        let got = got
            .as_object()
            .ok_or_else(|| format!("row {i} read back as {got}"))?;
        for (label, expected) in labels.iter().zip(row) {
            // A label is a header cell, written and read back as any cell.
            let text = match got.get(cell_text(label).as_str()) {
                Some(Datum::String(s)) => s.to_string(),
                Some(Datum::Null) | None => String::new(),
                Some(other) => other.to_string(),
            };
            let expected = match expected {
                Cell::Text(t) => Cell::Text(cell_text(t)),
                Cell::Number(n) => Cell::Number(*n),
            };
            if !cell_is(&expected, &text) {
                return Err(format!(
                    "row {i}, column {label:?}: read back {text:?}, where {} was written",
                    match expected {
                        Cell::Text(t) => format!("{t:?}"),
                        Cell::Number(n) => n.to_string(),
                    }
                ));
            }
        }
    }
    Ok(())
}

/// Markdown's normalisation of a written cell: a line break is a space,
/// a U+0000 is U+FFFD, and the whitespace at either end is not kept, as
/// the reader trims it: what JavaScript's `trim` takes, which is Unicode's
/// whitespace without U+0085 and with U+FEFF.
fn markdown_cell(text: &str) -> String {
    text.replace("\r\n", " ")
        .replace(['\n', '\r'], " ")
        .replace('\0', "\u{fffd}")
        .trim_matches(|c: char| (c.is_whitespace() && c != '\u{85}') || c == '\u{feff}')
        .to_string()
}

// ---------------------------------------------------------------------
// The cross product
// ---------------------------------------------------------------------

/// Whether an INI document read back (`back`) is what INI's conventions
/// make of `expected`: an object is a section (or the root) and an array
/// of scalars is `key[]` lines, each read back as itself; a number reads
/// back as its text, and one that is not finite as its word; a container
/// INI has no place for (inside an array, an empty array, or under a key
/// no header can spell) reads back as its compact JSON text, a string;
/// true, false and null read back as themselves, and a string as itself.
fn ini_same(expected: &Datum, back: &Datum) -> bool {
    match (expected, back) {
        (Datum::Object(e), Datum::Object(b)) => {
            e.len() == b.len()
                && e.iter().all(|(k, v)| {
                    b.get(k)
                        .or_else(|| b.get(k.trim()))
                        .is_some_and(|w| ini_same(v, w))
                })
        }
        (Datum::Array(e), Datum::Array(b)) if !e.is_empty() => {
            e.len() == b.len() && e.iter().zip(b).all(|(x, y)| ini_item(x, y))
        }
        (Datum::Number { value, .. }, Datum::String(s)) => number_text_is(*value, s),
        (container @ (Datum::Object(_) | Datum::Array(_)), Datum::String(s)) => {
            json_text_is(container, s)
        }
        (e, b) => same(e, b),
    }
}

/// An array item: a scalar as `ini_same` reads it, a container as its
/// JSON text.
fn ini_item(expected: &Datum, back: &Datum) -> bool {
    match (expected, back) {
        (container @ (Datum::Object(_) | Datum::Array(_)), Datum::String(s)) => {
            json_text_is(container, s)
        }
        (e, b) => ini_same(e, b),
    }
}

/// Whether `text` spells the number `value`: its digits, or the word of
/// one that is not finite.
fn number_text_is(value: f64, text: &str) -> bool {
    match text {
        "Infinity" => value == f64::INFINITY,
        "-Infinity" => value == f64::NEG_INFINITY,
        "NaN" => value.is_nan(),
        t => t.parse::<f64>().is_ok_and(|n| n == value),
    }
}

/// Whether `text` is the compact JSON text of `container`, read back as
/// JSON and compared as values, a number that is not finite matching
/// null or its word.
fn json_text_is(container: &Datum, text: &str) -> bool {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(text) else {
        return false;
    };
    fn matches(d: &Datum, j: &serde_json::Value) -> bool {
        use serde_json::Value as J;
        match (d, j) {
            (Datum::Null, J::Null) | (Datum::Number { .. }, J::Null) => true,
            (Datum::Bool(a), J::Bool(b)) => a == b,
            (Datum::Number { value, .. }, J::Number(n)) => n.as_f64() == Some(*value),
            (Datum::Number { value, .. }, J::String(s)) => number_text_is(*value, s),
            (Datum::String(a), J::String(b)) => **a == **b,
            (Datum::Array(a), J::Array(b)) => {
                a.len() == b.len() && a.iter().zip(b).all(|(x, y)| matches(x, y))
            }
            (Datum::Object(a), J::Object(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .all(|(k, v)| b.get(&**k).is_some_and(|w| matches(v, w)))
            }
            _ => false,
        }
    }
    matches(container, &json)
}

/// Whether a string spells an integer as ZON's reader writes a big
/// integer's digits, which is when the render writes a lone `$big` as the
/// integer itself: a minus sign at most, and first, then `0` or digits
/// that do not begin with `0`, but not `-0`.
fn zon_big_digits(s: &str) -> bool {
    let digits = s.strip_prefix('-').unwrap_or(s);
    !digits.is_empty()
        && digits.bytes().all(|b| b.is_ascii_digit())
        && (digits == "0" || !digits.starts_with('0'))
        && s != "-0"
}

/// The integer an object whose only member is `$big` spells, when its
/// value is a big integer's digits: ZON's render writes that object as
/// the integer, and its reader builds an integer no double holds exactly
/// as that object.
fn zon_big(d: &Datum) -> Option<Datum> {
    let members = d.as_object().filter(|m| m.len() == 1)?;
    match members.get("$big") {
        Some(Datum::String(s)) if zon_big_digits(s) => Some(Datum::Number {
            value: s.parse().ok()?,
            lexeme: Some(s.clone()),
        }),
        _ => None,
    }
}

/// What ZON's conventions make of a value it is given: an empty struct
/// reads back as an empty tuple, and a lone `$big` holding a big
/// integer's digits as that integer.
fn zon_reading(d: &Datum) -> Datum {
    match d {
        Datum::Object(m) if m.is_empty() => Datum::Array(Vec::new()),
        Datum::Object(_) if zon_big(d).is_some() => zon_big(d).unwrap(),
        Datum::Array(items) => Datum::Array(items.iter().map(zon_reading).collect()),
        Datum::Object(members) => Datum::Object(
            members
                .iter()
                .map(|(k, v)| (k.clone(), zon_reading(v)))
                .collect(),
        ),
        d => d.clone(),
    }
}

/// A field name as ZON's render wrote it, read back by the declared
/// reverse of its convention: `$empty` is the empty name; `$$` and a rest
/// is `$` and the rest; `$json:` and a text is the string the text spells
/// as a double-quoted JSON string; any other name is as it is.
fn zon_name(written: &str) -> Box<str> {
    if written == "$empty" {
        "".into()
    } else if let Some(rest) = written.strip_prefix("$$") {
        format!("${rest}").into()
    } else if let Some(json) = written.strip_prefix("$json:") {
        serde_json::from_str::<String>(json)
            .map(Into::into)
            .unwrap_or_else(|_| written.into())
    } else {
        written.into()
    }
}

/// What ZON's reader made of a document its render wrote, as the value it
/// was: a lone `$big` (the reader's big integer) is the integer, and every
/// field name reads back by the reverse of the convention that wrote it.
fn zon_back(d: &Datum) -> Datum {
    match d {
        Datum::Object(_) if zon_big(d).is_some() => zon_big(d).unwrap(),
        Datum::Array(items) => Datum::Array(items.iter().map(zon_back).collect()),
        Datum::Object(members) => Datum::Object(
            members
                .iter()
                .map(|(k, v)| (zon_name(k), zon_back(v)))
                .collect(),
        ),
        d => d.clone(),
    }
}

/// The default operators' source texts, which expr's render writes in
/// infix (`(` a group): an operation is a list whose first element is one.
const EXPR_OPERATORS: [&str; 6] = ["+", "-", "*", "/", "%", "("];

/// What expr's conventions make of a value its render is given, as its
/// reader reads the document back: a negative number is the operator `-`
/// applied to its magnitude; a number that is not finite is the string
/// `Infinity`, `-Infinity` or `NaN`; and a list whose first element is an
/// object whose `src` member is a default operator's source text has that
/// text in its place, the object's other members not kept.
fn expr_reading(d: &Datum) -> Datum {
    match d {
        Datum::Number { value, .. } if value.is_nan() => Datum::String("NaN".into()),
        Datum::Number { value, .. } if value.is_infinite() => Datum::String(
            if *value > 0.0 {
                "Infinity"
            } else {
                "-Infinity"
            }
            .into(),
        ),
        Datum::Number { value, lexeme } if value.is_sign_negative() => Datum::Array(vec![
            Datum::String("-".into()),
            Datum::Number {
                value: -value,
                lexeme: lexeme
                    .as_deref()
                    .and_then(|l| l.strip_prefix('-'))
                    .map(Into::into),
            },
        ]),
        Datum::Array(items) => {
            let mut items: Vec<Datum> = items.iter().map(expr_reading).collect();
            let operator = match items.first() {
                Some(Datum::Object(m)) => match m.get("src") {
                    Some(Datum::String(src)) if EXPR_OPERATORS.contains(&&**src) => {
                        Some(src.clone())
                    }
                    _ => None,
                },
                _ => None,
            };
            if let Some(src) = operator {
                items[0] = Datum::String(src);
            }
            Datum::Array(items)
        }
        Datum::Object(members) => Datum::Object(
            members
                .iter()
                .map(|(k, v)| (k.clone(), expr_reading(v)))
                .collect(),
        ),
        d => d.clone(),
    }
}

/// What a version's conventions make of a version its render is given, as
/// its reader reads the text back: only major, minor, patch, prerelease
/// and build are kept; a prerelease or build absent, null, empty or an
/// empty list is an empty list, and one given as one string the list of
/// its dot-separated identifiers; a major, minor, patch or prerelease
/// identifier of digits is a number, and a build identifier a string.
fn semver_reading(d: &Datum) -> Datum {
    let Some(members) = d.as_object() else {
        return d.clone();
    };
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let number = |d: &Datum| match d {
        Datum::String(s) if digits(s) && s.len() < 16 => Datum::Number {
            value: s.parse().unwrap_or(0.0),
            lexeme: Some(s.clone()),
        },
        d => d.clone(),
    };
    let text = |d: &Datum| match d {
        Datum::Number { value, lexeme } => {
            Datum::String(lexeme.clone().unwrap_or_else(|| value.to_string().into()))
        }
        d => d.clone(),
    };
    let ids = |d: Option<&Datum>, each: &dyn Fn(&Datum) -> Datum| match d {
        Some(Datum::Array(items)) => Datum::Array(items.iter().map(each).collect()),
        Some(Datum::String(s)) if !s.is_empty() => Datum::Array(
            s.split('.')
                .map(|id| each(&Datum::String(id.into())))
                .collect(),
        ),
        _ => Datum::Array(Vec::new()),
    };
    let mut out: Vec<(Box<str>, Datum)> = ["major", "minor", "patch"]
        .into_iter()
        .filter_map(|part| members.get(part).map(|v| (part.into(), number(v))))
        .collect();
    out.push(("prerelease".into(), ids(members.get("prerelease"), &number)));
    out.push(("build".into(), ids(members.get("build"), &text)));
    Datum::Object(out.into_iter().collect())
}

/// The tag feed's render names what it supplies with.
const FEED_RENDER: &str = "tag:tabnas.dev,2026:feed-render";

/// The date feed's render gives a feed or an entry with none.
const FEED_EPOCH: &str = "1970-01-01T00:00:00Z";

/// Whether `s` is `n` ASCII digits.
fn digits(s: &str, n: usize) -> bool {
    s.len() == n && s.bytes().all(|b| b.is_ascii_digit())
}

/// Whether `s` is two digits from `lo` to `hi`.
fn two(s: &str, lo: u32, hi: u32) -> bool {
    digits(s, 2) && s.parse::<u32>().is_ok_and(|n| (lo..=hi).contains(&n))
}

/// Whether `s` is an RFC 3339 date-time with the upper-case `T` and `Z`
/// Atom requires, as feed's render tells one: a full date; a time, its
/// seconds perhaps with a fraction; `Z` or an offset.
fn feed_is_3339(s: &str) -> bool {
    fn time(t: &str) -> bool {
        let p: Vec<&str> = t.split(':').collect();
        let second = |sec: &str| {
            let q: Vec<&str> = sec.split('.').collect();
            match q.as_slice() {
                [s] => two(s, 0, 60),
                [s, f] => two(s, 0, 60) && !f.is_empty() && f.bytes().all(|b| b.is_ascii_digit()),
                _ => false,
            }
        };
        p.len() == 3 && two(p[0], 0, 23) && two(p[1], 0, 59) && second(p[2])
    }
    fn offset(o: &str) -> bool {
        let p: Vec<&str> = o.split(':').collect();
        p.len() == 2 && two(p[0], 0, 23) && two(p[1], 0, 59)
    }
    let halves: Vec<&str> = s.split('T').collect();
    let [date, rest] = halves.as_slice() else {
        return false;
    };
    let d: Vec<&str> = date.split('-').collect();
    if !(d.len() == 3 && digits(d[0], 4) && two(d[1], 1, 12) && two(d[2], 1, 31)) {
        return false;
    }
    let z: Vec<&str> = rest.split('Z').collect();
    let plus: Vec<&str> = rest.split('+').collect();
    let minus: Vec<&str> = rest.split('-').collect();
    match (z.as_slice(), plus.as_slice(), minus.as_slice()) {
        ([t, ""], _, _) => time(t),
        ([_], [t, o], _) => time(t) && offset(o),
        ([_], [_], [t, o]) => time(t) && offset(o),
        _ => false,
    }
}

/// An RSS date, RFC 822's date-time with a four-digit year allowed and its
/// names in any case, as the same instant in RFC 3339's form, as feed's
/// render writes one: an optional day of the week and a comma, then the
/// day, the month, the year (two digits before 50 in the 2000s, else in
/// the 1900s, as RFC 2822 reads them), the time (seconds `00` where it
/// has none) and the zone (`Z` for UT, GMT and Z, the US zones and
/// `+HHMM` as offsets). `None` for any other text, a comment or another
/// military zone among them.
fn feed_822(s: &str) -> Option<String> {
    const DAYS: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];
    const MONTHS: [&str; 12] = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    let words = |t: &str| -> Vec<String> {
        t.replace(['\n', '\r', '\t'], " ")
            .split(' ')
            .filter(|w| !w.is_empty())
            .map(String::from)
            .collect()
    };
    let parts: Vec<&str> = s.split(',').collect();
    let w = match parts.as_slice() {
        [_] => words(s),
        [day, rest] => match words(day).as_slice() {
            [d] if DAYS.contains(&d.to_ascii_lowercase().as_str()) => words(rest),
            _ => return None,
        },
        _ => return None,
    };
    let [day, month, year, time, zone] = <[String; 5]>::try_from(w).ok()?;
    let day = if digits(&day, 1) {
        format!("0{day}")
    } else {
        day
    };
    if !two(&day, 1, 31) {
        return None;
    }
    let month = MONTHS
        .iter()
        .position(|m| *m == month.to_ascii_lowercase())?
        + 1;
    let year = if digits(&year, 4) {
        year
    } else if digits(&year, 2) {
        format!("{}{year}", if year.as_str() < "50" { "20" } else { "19" })
    } else {
        return None;
    };
    let mut hms: Vec<&str> = time.split(':').collect();
    if hms.len() == 2 {
        hms.push("00");
    }
    if !(hms.len() == 3 && two(hms[0], 0, 23) && two(hms[1], 0, 59) && two(hms[2], 0, 60)) {
        return None;
    }
    let numeric = |sign: char, d: &str| {
        (digits(d, 4) && two(&d[..2], 0, 23) && two(&d[2..], 0, 59))
            .then(|| format!("{sign}{}:{}", &d[..2], &d[2..]))
    };
    let offset = match zone.to_ascii_lowercase().as_str() {
        "ut" | "gmt" | "z" => "Z".to_string(),
        "est" | "cdt" => "-05:00".to_string(),
        "edt" => "-04:00".to_string(),
        "cst" => "-06:00".to_string(),
        "mst" => "-07:00".to_string(),
        "mdt" => "-06:00".to_string(),
        "pst" => "-08:00".to_string(),
        "pdt" => "-07:00".to_string(),
        _ => match (zone.strip_prefix('+'), zone.strip_prefix('-')) {
            (Some(d), _) => numeric('+', d)?,
            (_, Some(d)) => numeric('-', d)?,
            _ => return None,
        },
    };
    Some(format!("{year}-{month:02}-{day}T{}{offset}", hms.join(":")))
}

/// A date as feed's render writes it and its reader reads it back: `Ok`
/// with its RFC 3339 form (a date with no text is written as a missing one
/// is), or `Err` with the text of a date in neither form, which the render
/// writes as the epoch followed by a category that holds the text.
fn feed_date(v: &Datum) -> Result<String, String> {
    match v {
        Datum::String(s) if s.is_empty() => Ok(FEED_EPOCH.to_string()),
        Datum::String(s) => {
            let upper = s.replace('t', "T").replace('z', "Z");
            if feed_is_3339(&upper) {
                Ok(upper)
            } else {
                feed_822(s).ok_or_else(|| s.to_string())
            }
        }
        other => Err(other.to_string()),
    }
}

/// A value as feed's render writes it, read back: a member whose value is
/// null not written, and a character XML 1.0 cannot carry written as
/// U+FFFD.
fn feed_clean(d: &Datum) -> Datum {
    let carried = |c: char| {
        matches!(c, '\t' | '\n' | '\r') || (c >= ' ' && c != '\u{fffe}' && c != '\u{ffff}')
    };
    match d {
        Datum::String(s) if !s.chars().all(carried) => Datum::String(
            s.chars()
                .map(|c| if carried(c) { c } else { '\u{fffd}' })
                .collect::<String>()
                .into(),
        ),
        Datum::Array(items) => Datum::Array(items.iter().map(feed_clean).collect()),
        Datum::Object(members) => Datum::Object(
            members
                .iter()
                .filter(|(_, v)| !matches!(v, Datum::Null))
                .map(|(k, v)| (k.clone(), feed_clean(v)))
                .collect(),
        ),
        d => d.clone(),
    }
}

/// A feed's or an entry's members as feed's render writes them, read back
/// (`entry` the entry's position; `None` for the feed): Atom 1.0; a text
/// construct of type xhtml as html, its value trimmed; each date in RFC
/// 3339's form, or the epoch and a category holding the text of one in
/// neither form, written after it, so among the categories in the order
/// of the members; an entry's RSS source not read back; and the id, the
/// title and the updated Atom requires, where they are missing.
fn feed_members(members: &[(Box<str>, Datum)], entry: Option<usize>) -> Vec<(Box<str>, Datum)> {
    let text = |name: &str| ["title", "subtitle", "rights", "summary", "content"].contains(&name);
    let trimmed = |s: &str| {
        s.trim_matches(|c: char| (c.is_whitespace() && c != '\u{85}') || c == '\u{feff}')
            .to_string()
    };
    let mut out: Vec<(Box<str>, Datum)> = Vec::new();
    let mut categories: Vec<Datum> = Vec::new();
    let mut has_categories = false;
    for (k, v) in members {
        let v = match (k.as_ref(), v) {
            ("version", _) => Datum::String("1.0".into()),
            ("format", _) => Datum::String("atom".into()),
            ("source", _) if entry.is_some() => continue,
            ("updated" | "published", date) => match feed_date(date) {
                Ok(date) => Datum::String(date.into()),
                Err(kept) => {
                    categories.push(Datum::Object(
                        [
                            ("term".into(), Datum::String(k.clone())),
                            (
                                "scheme".into(),
                                Datum::String(format!("{FEED_RENDER}/date").into()),
                            ),
                            ("label".into(), Datum::String(kept.into())),
                        ]
                        .into_iter()
                        .collect(),
                    ));
                    Datum::String(FEED_EPOCH.into())
                }
            },
            ("categories", Datum::Array(own)) => {
                has_categories = true;
                categories.extend(own.iter().cloned());
                continue;
            }
            ("entries", Datum::Array(entries)) => Datum::Array(
                entries
                    .iter()
                    .enumerate()
                    .map(|(i, e)| match e.as_object() {
                        Some(m) => {
                            let m: Vec<(Box<str>, Datum)> =
                                m.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                            Datum::Object(feed_members(&m, Some(i)).into_iter().collect())
                        }
                        None => e.clone(),
                    })
                    .collect(),
            ),
            (name, Datum::Object(construct)) if text(name) => {
                let xhtml =
                    matches!(construct.get("type"), Some(Datum::String(t)) if &**t == "xhtml");
                if xhtml {
                    Datum::Object(
                        construct
                            .iter()
                            .map(|(ck, cv)| match (ck.as_ref(), cv) {
                                ("type", _) => (ck.clone(), Datum::String("html".into())),
                                ("value", Datum::String(s)) => {
                                    (ck.clone(), Datum::String(trimmed(s).into()))
                                }
                                _ => (ck.clone(), cv.clone()),
                            })
                            .collect(),
                    )
                } else {
                    v.clone()
                }
            }
            _ => v.clone(),
        };
        out.push((k.clone(), v));
    }
    if has_categories || !categories.is_empty() {
        out.push(("categories".into(), Datum::Array(categories)));
    }
    let has = |out: &[(Box<str>, Datum)], name: &str| out.iter().any(|(k, _)| &**k == name);
    if !has(&out, "id") {
        let id = match entry {
            Some(i) => format!("{FEED_RENDER}/{i}"),
            None => FEED_RENDER.to_string(),
        };
        out.push(("id".into(), Datum::String(id.into())));
    }
    if !has(&out, "title") {
        let empty = [
            ("type".into(), Datum::String("text".into())),
            ("value".into(), Datum::String("".into())),
        ];
        out.push(("title".into(), Datum::Object(empty.into_iter().collect())));
    }
    if !has(&out, "updated") {
        out.push(("updated".into(), Datum::String(FEED_EPOCH.into())));
    }
    out
}

/// What feed's render makes of a feed's own tree, read back by its reader
/// (feed's loss list): its members as [`feed_members`] writes them, and,
/// for a feed without an author, unless it has entries and every one has
/// one, an author named by its title where that is text and not empty,
/// and `unknown` otherwise, with the render's uri.
fn feed_reading(d: &Datum) -> Datum {
    let d = feed_clean(d);
    let Some(feed) = d.as_object() else {
        return d;
    };
    let members: Vec<(Box<str>, Datum)> =
        feed.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    let mut out = feed_members(&members, None);
    let get = |out: &[(Box<str>, Datum)], name: &str| {
        out.iter()
            .find(|(k, _)| &**k == name)
            .map(|(_, v)| v.clone())
    };
    let authored = |d: Option<Datum>| matches!(d, Some(Datum::Array(a)) if !a.is_empty());
    if !authored(get(&out, "authors")) {
        let entries = match get(&out, "entries") {
            Some(Datum::Array(entries)) => entries,
            _ => Vec::new(),
        };
        let every = !entries.is_empty()
            && entries
                .iter()
                .all(|e| authored(e.as_object().and_then(|m| m.get("authors")).cloned()));
        if !every {
            let name = match get(&out, "title") {
                Some(Datum::Object(t)) if matches!(t.get("type"), Some(Datum::String(k)) if &**k == "text") => {
                    match t.get("value") {
                        Some(Datum::String(v)) if !v.is_empty() => v.clone(),
                        _ => "unknown".into(),
                    }
                }
                _ => "unknown".into(),
            };
            let author: Datum = Datum::Object(
                [
                    ("name".into(), Datum::String(name)),
                    ("uri".into(), Datum::String(FEED_RENDER.into())),
                ]
                .into_iter()
                .collect(),
            );
            out.retain(|(k, _)| &**k != "authors");
            out.push(("authors".into(), Datum::Array(vec![author])));
        }
    }
    Datum::Object(out.into_iter().collect())
}

/// Whether `written`, the document aless wrote into `to` from a source of
/// `from` whose value is `source`, reads back as the conventions say.
fn check(from: &str, to: &str, source: &Datum, written: &str) -> Result<(), String> {
    let part = target(to);
    let from_schema = translate::source_part(format(from)).and_then(|p| p.schema.clone());
    // A source whose events carry the target's own schema (XML's element
    // tree into XML) is written as it is, and reads back as it is.
    let embedded = part.schema.is_some() && from_schema != part.schema;
    let back = if to == "markdown" {
        // A Markdown table reads back as records through the format's
        // lift, as a host reads it for a records target.
        let lift = part.lift.as_ref().expect("markdown has a lift");
        let program = format!(
            "{}\ndef export [input] (records ({} input))\n",
            lift.text, lift.entry
        );
        let yaml = through_program("markdown-records.alc", &program, "markdown", written)
            .map_err(|f| format!("the written table does not read back: {f}"))?;
        read("yaml", &yaml).map_err(|f| format!("its records do not read back: {f}"))?
    } else if (to == "xml" || to == "feed") && embedded {
        // An embedding reads back through its reverse: the element tree, or
        // the feed, as JSON, unembedded, and written where a non-finite
        // number has a spelling. (expr's and semver's reverses are the
        // identity, so their documents read back as they are.)
        let tree = read(to, written)
            .map_err(|f| format!("the written document does not read back: {f}"))?;
        let embed = part
            .embed
            .as_ref()
            .expect("an embedding format has an embed");
        let program = format!("{}\ndef export [input] ({to}-unembed input)\n", embed.text);
        let yaml = through_program(
            &format!("{to}-unembed.alc"),
            &program,
            "json",
            &tree.to_string(),
        )
        .map_err(|f| format!("the tree does not unembed: {f}"))?;
        read("yaml", &yaml).map_err(|f| format!("the unembedded tree does not read back: {f}"))?
    } else {
        read(to, written).map_err(|f| format!("the written document does not read back: {f}"))?
    };
    let key = Options::default().key;
    let expected = match to {
        "csv" => return check_records(source, &back, &|t| t.to_string()),
        "markdown" => return check_records(source, &back, &markdown_cell),
        "ini" => {
            let expected = wrap_object(source, &key);
            return if ini_same(&expected, &back) {
                Ok(())
            } else {
                Err(format!("read back as {back}, where {expected} was written"))
            };
        }
        "json" | "jsonc" | "jsonic" => map_non_finite(source, &|_| Datum::Null),
        "jsonl" => map_non_finite(&wrap_array(source), &|_| Datum::Null),
        "toml" => without_nulls(&wrap_object(source, &key)),
        "zon" => zon_reading(source),
        "expr" => expr_reading(source),
        // Only a version reaches its render: the embed refuses any other
        // tree, and a version's own documents are versions.
        "semver" => semver_reading(source),
        // A feed's own tree; a plain one read back through the reverse
        // above, exactly.
        "feed" if !embedded => feed_reading(source),
        _ => source.clone(),
    };
    let back = if to == "zon" { zon_back(&back) } else { back };
    if same(&expected, &back) {
        Ok(())
    } else {
        Err(format!("read back as {back}, where {expected} was written"))
    }
}

/// How deep a value nests: a scalar is 0, a container one more than its
/// deepest member.
fn depth(d: &Datum) -> usize {
    match d {
        Datum::Array(items) => 1 + items.iter().map(depth).max().unwrap_or(0),
        Datum::Object(members) => 1 + members.values().map(depth).max().unwrap_or(0),
        _ => 0,
    }
}

/// The nesting every format reads: the readers guard nesting at different
/// depths (tabnas-json past 128 levels, YAML's and ZON's near it, the
/// transducer at 256 events deep, which XML's embedding reaches at about
/// 127 levels, two elements a level), and a root adapter or an embedding
/// adds a level or two. A document nested deeper than this is at one
/// format's guard and past another's, a limit and not a shape, so the
/// matrix leaves it out, and pins how few such documents there are.
const DEPTH_BOUND: usize = 100;

/// The cross product of `docs` and every format: each document read with
/// its format's grammar, written by aless in every format, and read back
/// under the target's conventions. At least `floor` documents, and at most
/// `too_deep_at_most` of them deeper than every format reads.
fn matrix(docs: Vec<(String, &'static str, String)>, floor: usize, too_deep_at_most: usize) {
    assert_eq!(
        translate::render_names(),
        FORMATS.to_vec(),
        "the formats --render writes"
    );
    let total = docs.len() * FORMATS.len();
    let mut failures: Vec<String> = Vec::new();
    let mut refused_sources = Vec::new();
    let mut too_deep = Vec::new();
    let mut pairs = 0;
    // The declared refusals, by class: a schema-only target's of a source
    // of another tree (by target), and a version's embed's of a tree that
    // is not a version.
    let mut schema_only: HashMap<&str, usize> = HashMap::new();
    let mut not_a_version = 0;
    for (n, (name, from, text)) in docs.iter().enumerate() {
        let source = match read(from, text) {
            Ok(d) => d,
            Err(f) => {
                refused_sources.push(format!("{name}: {f}"));
                continue;
            }
        };
        if depth(&source) > DEPTH_BOUND {
            too_deep.push(format!("{name}: {} levels", depth(&source)));
            continue;
        }
        for to in FORMATS {
            pairs += 1;
            let written = match render(name, from, to, text) {
                Ok(written) => written,
                Err(Unwritten::SchemaOnly) => {
                    *schema_only.entry(to).or_default() += 1;
                    continue;
                }
                Err(Unwritten::NotAVersion) => {
                    not_a_version += 1;
                    continue;
                }
                Err(Unwritten::Failed(why)) => {
                    failures.push(format!("{name} ({from}) -> {to}: {why}"));
                    continue;
                }
            };
            // A records source read through its lift writes its table, not
            // its tree: it is held to reading back.
            let lifted = translate::source_part(format(from))
                .is_some_and(|p| p.reads.first() == Some(&Shape::Records))
                && target(to).writes == Shape::Records;
            let outcome = if lifted {
                read(to, &written)
                    .map(|_| ())
                    .map_err(|f| format!("the written document does not read back: {f}"))
            } else {
                check(from, to, &source, &written)
            };
            if let Err(why) = outcome {
                failures.push(format!("{name} ({from}) -> {to}: {why}"));
            }
        }
        if (n + 1) % 25 == 0 || n + 1 == docs.len() {
            eprintln!(
                "matrix: {} of {} documents ({}%), {} failures",
                n + 1,
                docs.len(),
                (n + 1) * 100 / docs.len(),
                failures.len()
            );
        }
    }
    for line in &refused_sources {
        eprintln!("refused source: {line}");
    }
    for line in &too_deep {
        eprintln!("deeper than every format reads: {line}");
    }
    for line in &failures {
        eprintln!("FAIL {line}");
    }
    let mut refusals: Vec<(&str, usize)> = schema_only.into_iter().collect();
    refusals.sort_unstable();
    let declared = refusals.iter().map(|(_, n)| n).sum::<usize>() + not_a_version;
    eprintln!(
        "matrix: {pairs} pairs of {} documents; {} written and read back, {declared} refused as \
         declared: by a schema-only target {refusals:?}, by semver as not a version \
         {not_a_version}; {} documents refused by their own reader, {} too deep",
        docs.len(),
        pairs - declared - failures.len(),
        refused_sources.len(),
        too_deep.len()
    );
    assert!(
        pairs + (refused_sources.len() + too_deep.len()) * FORMATS.len() == total
            && docs.len() >= floor,
        "the corpora shrank: {} documents",
        docs.len()
    );
    assert!(
        too_deep.len() <= too_deep_at_most,
        "{} documents are deeper than every format reads (above)",
        too_deep.len()
    );
    assert!(
        failures.is_empty(),
        "{} of {pairs} pairs failed (above)",
        failures.len()
    );
}

/// aless's own fixtures, one document per format at least, and the
/// documents of JSONTestSuite every JSON parser must accept.
#[test]
fn every_document_translates_into_every_format() {
    matrix(corpus(), FLOOR, 0);
}

/// The documents [`corpus`] reads, at least.
const FLOOR: usize = 117;

/// Every format's own fixture corpus, into every format: thousands of
/// documents, run by CI with `--ignored` in the `matrix` profile, where
/// they take minutes rather than the hour a debug build would.
#[test]
#[ignore = "the cross product of every format's fixtures: CI runs it in the matrix profile"]
fn every_fixture_of_every_format_translates_into_every_format() {
    matrix(spec_corpus(), 3850, 1);
}

/// Ten times the records leave what the composed render retains where it
/// was, for every source aless streams a record at a time or as the parse
/// proceeds: the render keeps one marker per open container, never the
/// rows behind it.
#[test]
fn ten_times_the_records_leave_what_a_render_retains_flat() {
    type Doc = fn(usize) -> String;
    let sources: [(&str, Doc); 11] = [
        ("jsonl", |n| {
            (0..n)
                .map(|i| format!("{{\"n\": {i}, \"s\": \"row {i}\"}}\n"))
                .collect()
        }),
        ("csv", |n| {
            let rows: String = (0..n).map(|i| format!("{i},row {i}\n")).collect();
            format!("n,s\n{rows}")
        }),
        ("tsv", |n| {
            let rows: String = (0..n).map(|i| format!("{i}\trow {i}\n")).collect();
            format!("n\ts\n{rows}")
        }),
        ("json", |n| {
            let rows: Vec<String> = (0..n)
                .map(|i| format!("{{\"n\": {i}, \"s\": \"row {i}\"}}"))
                .collect();
            format!("[{}]", rows.join(",\n"))
        }),
        ("json5", |n| {
            let rows: Vec<String> = (0..n)
                .map(|i| format!("{{n: {i}, s: 'row {i}'}}"))
                .collect();
            format!("[{}]", rows.join(",\n"))
        }),
        ("jsonc", |n| {
            let rows: Vec<String> = (0..n)
                .map(|i| format!("// {i}\n{{\"n\": {i}, \"s\": \"row {i}\"}}"))
                .collect();
            format!("[{}]", rows.join(",\n"))
        }),
        ("jsonic", |n| {
            let rows: Vec<String> = (0..n)
                .map(|i| format!("{{n: {i}, s: \"row {i}\"}}"))
                .collect();
            format!("[{}]", rows.join(",\n"))
        }),
        ("yaml", |n| {
            (0..n)
                .map(|i| format!("- n: {i}\n  s: row {i}\n"))
                .collect()
        }),
        ("zon", |n| {
            let rows: Vec<String> = (0..n)
                .map(|i| format!(".{{ .n = {i}, .s = \"row {i}\" }}"))
                .collect();
            format!(".{{ {} }}", rows.join(", "))
        }),
        ("markdown", |n| {
            let rows: String = (0..n).map(|i| format!("| {i} | row {i} |\n")).collect();
            format!("| n | s |\n| --- | --- |\n{rows}")
        }),
        ("xml", |n| {
            let rows: String = (0..n)
                .map(|i| format!("<row><n>{i}</n><s>row {i}</s></row>"))
                .collect();
            format!("<rows>{rows}</rows>")
        }),
    ];
    let yaml = target("yaml");
    for (id, doc) in sources {
        let from = format(id);
        assert!(
            !matches!(export::plan(from), Some(Plan::Materialize)),
            "{id} streams"
        );
        let composition =
            translate::compose(translate::source_part(from), yaml, &Options::default())
                .unwrap_or_else(|f| panic!("{id}: {f}"));
        let retained = |rows: usize| {
            let text = doc(rows);
            let metrics = Metrics::new();
            translate::run(
                &job(id, from, What::Part),
                &composition,
                Records::Any,
                input(from, &text),
                Box::new(std::io::sink()),
                metrics.clone(),
            )
            .unwrap_or_else(|e| panic!("{id}: {e:?}"));
            (
                Metrics::get(&metrics.retained_bytes_high),
                Metrics::get(&metrics.output_bytes),
            )
        };
        let (one, written_one) = retained(200);
        let (ten, written_ten) = retained(2000);
        eprintln!("retention: {id}, 200 records retain at most {one} bytes, 2000 {ten}");
        assert!(
            written_ten > 9 * written_one,
            "{id}: {written_one} {written_ten}"
        );
        assert_eq!(ten, one, "{id}: the peak is one record's, not the count's");
    }
}
