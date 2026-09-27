//! `--render`: a document's records as CSV, or the document as JSON text,
//! streamed through the tabnas transducer and renderers rather than built
//! into the viewer's tree.
//!
//! The viewer's path (`load`) reads an input whole, parses it whole and
//! only then has a value to print. An export need not: the transducer
//! (`tabnas-transduce`) turns a parse into a stream of `JsonEvents/1`, a
//! table transducer projects the selected rows into `TableRows/1`, and a
//! renderer (`tabnas-render`) writes each row as it completes. What the
//! run holds is one row, or one chunk of a line-delimited file, rather
//! than the document. Where the events come from is [`plan`]'s decision,
//! and the one place the input format matters:
//!
//! - JSON Lines, CSV and TSV with the rows at the root are read a record
//!   (or a chunk of records) at a time from a reader, so the file is never
//!   in memory whole and `--max-size` does not apply to it;
//! - a grammar the transducer has verified for incremental streaming
//!   (`capability::incremental`: the JSON family, YAML and ZON) is parsed
//!   whole, the events leaving as the parse proceeds, and the exported
//!   array is emptied behind the stream so it is not held twice — except
//!   for YAML, whose aliases copy their anchor's value when the alias is
//!   met, and would copy an emptied array;
//! - every other grammar is parsed whole and its value walked afterwards.
//!
//! Everything here is terminal-free: the output is any writer, and the
//! result is a value `headless` turns into JSON. aless's own caps hold as
//! they do for a parse: the depth cap and `--timeout` through [`load::guard`]
//! on the grammar's parser, and the deadline's alarm through the
//! transducer's abort flag, so a line-by-line read stops on time too.

use std::io::{self, BufRead, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tabnas_render::{CsvOptions, CsvRenderer, JsonOptions, JsonRenderer, MissingText, WriteOut};
use tabnas_transduce::source::{LineFormat, LinesSource};
use tabnas_transduce::{
    capability, AbortFlag, Code, Duplicates, Fail, Flow, JsonEvent, Limits, Metrics, ParserSource,
    Prune, Schema, Selector, Sink, SourceMode, TableBinding, TableEvent, TableFromJson, TableSink,
};

use crate::fmt;
use crate::headless::{kind_of_event, Seg, KEYS_LISTED};
use crate::load::{self, Deadline, Format, LoadError, Stop, MAX_RULE_DEPTH, STOP_DEPTH, STOP_TIME};

/// The output `--render` writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Renderer {
    /// The records as the spec's always-quoted CSV: every field quoted,
    /// CRLF, a header row.
    Csv,
    /// The value as JSON text.
    Json,
}

impl Renderer {
    /// `--render`'s argument.
    pub fn from_name(name: &str) -> Option<Renderer> {
        match name.trim().to_ascii_lowercase().as_str() {
            "csv" => Some(Renderer::Csv),
            "json" => Some(Renderer::Json),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Renderer::Csv => "csv",
            Renderer::Json => "json",
        }
    }
}

/// How an input reaches the transducer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Plan {
    /// JSON Lines, CSV or TSV with the rows at the root: a record at a
    /// time from a reader, never the file whole.
    Lines,
    /// The text whole, its grammar's events leaving as the parse proceeds;
    /// `prune` says whether the exported array is emptied behind them.
    Incremental { prune: bool },
    /// The text whole, parsed and then walked.
    Materialize,
}

/// The plan for a format, given whether the export starts at the root.
/// `None` for plain text, which has no grammar and so no events.
pub fn plan(format: Format, at_root: bool) -> Option<Plan> {
    Some(match format {
        Format::Text => return None,
        Format::Jsonl | Format::Csv | Format::Tsv if at_root => Plan::Lines,
        f if capability::incremental(f.name()) => Plan::Incremental {
            prune: f != Format::Yaml,
        },
        _ => Plan::Materialize,
    })
}

/// One export.
#[derive(Clone, Debug)]
pub struct Job {
    /// The input as errors name it: the path as given, or `-`.
    pub name: String,
    /// The input as a report names it (`load::origin_of`, or `(stdin)`).
    pub origin: String,
    pub format: Format,
    pub renderer: Renderer,
    /// The value exported: the root when empty.
    pub path: Vec<Seg>,
    /// JSON on one line.
    pub compact: bool,
    /// Indentation per level of JSON output otherwise.
    pub indent: usize,
    pub timeout: Option<Duration>,
}

/// The input, as [`plan`] chose to read it.
pub enum Input<'a> {
    /// The whole text, already read within `--max-size`.
    Text(&'a str),
    /// A reader over a line-delimited file.
    Lines(Box<dyn BufRead + Send + 'a>),
}

/// The node `--path` did reach when it named nothing, for a `not_found`
/// report: its path, jq's name for its kind, a container's item count so
/// far or a scalar's value, and an object's first keys.
#[derive(Clone, Debug, PartialEq)]
pub struct Nearest {
    pub path: String,
    pub kind: &'static str,
    pub length: Option<usize>,
    pub value: Option<Value>,
    pub keys: Option<Vec<String>>,
    /// How many of the path's segments it is at.
    depth: usize,
}

/// Why an export failed.
#[derive(Debug)]
pub enum ExportError {
    /// The command asked for what cannot be done.
    Usage(String),
    /// aless's own limits, or a grammar's: `too_deep` or `timeout`, in the
    /// shape of a load error so it reports as one; `partial` says whether
    /// output had been written when the parse stopped.
    Load {
        error: Box<LoadError>,
        partial: bool,
    },
    /// A transducer stage failed; the code says which.
    Transduce(Box<Fail>),
    /// `--path` names nothing.
    NotFound {
        message: String,
        nearest: Box<Nearest>,
    },
    /// The reader of the output went away (`| head`): not an error.
    ReaderGone,
}

/// The tabnas transducer's limits: its defaults, which are generous for a
/// document and are named in a `RESOURCE_LIMIT_EXCEEDED` failure.
fn limits() -> Limits {
    Limits::default()
}

/// Run an export: `input` in, rendered text out through `out`.
pub fn export(job: &Job, input: Input<'_>, out: Box<dyn Write + Send>) -> Result<(), ExportError> {
    if let Some(Seg::Index(i)) = job
        .path
        .iter()
        .find(|s| matches!(s, Seg::Index(i) if *i < 0))
    {
        return Err(ExportError::Usage(format!(
            "--render reads the input once, front to back, so a path cannot count from the \
             end: [{i}] in {}; use --json --path, which reads the whole document",
            path_text(&job.path)
        )));
    }
    let written = Arc::new(AtomicU64::new(0));
    let broken = Arc::new(AtomicBool::new(false));
    let out = WriteOut::new(Pipe {
        inner: out,
        written: written.clone(),
        broken: broken.clone(),
    });
    let target = job.path.clone();
    let outcome = match job.renderer {
        Renderer::Json => {
            let options = JsonOptions {
                indent: (!job.compact).then_some(job.indent),
                trailing_newline: true,
            };
            run(
                job,
                input,
                Scope::new(target, JsonRenderer::new(out, options)),
            )
        }
        Renderer::Csv => {
            let options = CsvOptions {
                // The default export is lossy where the standard profile
                // is: an absent member and a null both read as an empty
                // field. Failing a whole export for a member one row lacks
                // would serve nobody.
                missing: MissingText::Text("".into()),
                ..CsvOptions::default()
            };
            let csv =
                CsvRenderer::new(out, options).map_err(|f| ExportError::Transduce(Box::new(f)))?;
            // The rows are the elements of the array the scope re-roots
            // the stream at, so the binding selects the root's elements.
            let binding = TableBinding {
                schema: Schema::Infer,
                rows: Selector::root().each_index(),
            };
            let table = TableFromJson::new(
                binding,
                &limits(),
                // The grammars keep a repeated member's last value; the
                // export agrees with `--json` rather than failing.
                Duplicates::LastWins,
                Metrics::new(),
                EmptyOk::new(csv),
            )
            .map_err(|f| ExportError::Transduce(Box::new(f)))?;
            run(job, input, Scope::new(target, Rows::new(table)))
        }
    };
    outcome.map_err(|failed| classify(job, *failed, &written, &broken))
}

/// What a run ends with: the source's outcome and, when aless's guard was
/// on the parser, its record of why the parse stopped.
struct Ran {
    outcome: Result<Flow, Fail>,
    stop: Option<Arc<Stop>>,
    missing: Option<Nearest>,
}

/// A failure with what the run knew: the guard's record and how far the
/// parse got.
struct Failed {
    fail: Fail,
    stop: Option<Arc<Stop>>,
    missing: Option<Nearest>,
}

/// Drive `input` into `scope` on the parse thread, under `--timeout`.
fn run<S: Sink + Send + 'static>(
    job: &Job,
    input: Input<'_>,
    scope: Scope<S>,
) -> Result<(), Box<Failed>> {
    let format = job.format;
    let abort = AbortFlag::new();
    let alarm = abort.clone();
    let mode = mode_for(job);
    // A grammar's parser carries aless's guard, which reads the deadline
    // between steps and records where the parse was before it raises the
    // abort flag; the alarm raises it directly only for a line-by-line
    // read, which has no such guard. (The transducer's own guard runs
    // ahead of the budget, so an alarm raising the flag itself would stop
    // the parse first, and lose the position.)
    let lines = matches!(input, Input::Lines(_));
    let ran = load::on_parse_thread(
        job.timeout,
        move || {
            if lines {
                alarm.abort();
            }
        },
        move |deadline: Option<Deadline>| match input {
            Input::Lines(reader) => {
                let source = LinesSource::new(reader, line_format(format))
                    .limits(limits())
                    .abort(abort);
                let (outcome, scope) = source.run_owned(scope);
                Ran {
                    outcome,
                    stop: None,
                    missing: scope.missing(),
                }
            }
            Input::Text(text) => {
                let mut parser =
                    load::make_parser(format).expect("plain text is refused before a run");
                let notify = abort.clone();
                let stop = load::guard(&mut parser, deadline, move || notify.abort());
                let text = text.strip_prefix('\u{feff}').unwrap_or(text);
                let source = ParserSource::new(parser, text)
                    .mode(mode)
                    .limits(limits())
                    .abort(abort);
                let (outcome, scope) = source.run_owned(scope);
                Ran {
                    outcome,
                    stop: Some(stop),
                    missing: scope.missing(),
                }
            }
        },
    );
    match ran.outcome {
        // The scope never stops the source, so a stop is a whole run.
        Ok(_) => Ok(()),
        Err(fail) => Err(Box::new(Failed {
            fail,
            stop: ran.stop,
            missing: ran.missing,
        })),
    }
}

/// The source mode for a text input: incremental where the grammar is
/// verified, pruning the exported array as it streams (see the module
/// doc for why YAML is not pruned).
fn mode_for(job: &Job) -> SourceMode {
    if !capability::incremental(job.format.name()) {
        return SourceMode::Materialize;
    }
    let prune = if job.format == Format::Yaml {
        Prune::Never
    } else {
        // The array exported: named by its elements for CSV, as the rows
        // selector does, and by itself for JSON; the source prunes the
        // same array either way.
        let array = selector_of(&job.path);
        Prune::Under(match job.renderer {
            Renderer::Csv => array.each_index(),
            Renderer::Json => array,
        })
    };
    SourceMode::Incremental { prune }
}

/// The line-delimited format of a [`Plan::Lines`] input.
fn line_format(format: Format) -> LineFormat {
    match format {
        Format::Tsv => {
            let mut options = tabnas_csv::CsvOptions::default();
            options.field.separation = Some("\t".to_string());
            LineFormat::csv_with(true, options)
        }
        Format::Csv => LineFormat::csv(),
        _ => LineFormat::Jsonl,
    }
}

/// A path as a transducer selector, strictly: a name selects a member, an
/// index an element. (The lenient reading `--path` has, a decimal name for
/// an index, is the scope's; this only decides what is pruned.)
fn selector_of(path: &[Seg]) -> Selector {
    let mut s = Selector::root();
    for seg in path {
        s = match seg {
            Seg::Name(n) => s.property(n.as_str()),
            Seg::Index(i) => s.index(usize::try_from(*i).unwrap_or(usize::MAX)),
        };
    }
    s
}

/// A path in jq's syntax, as the outputs print one.
pub fn path_text(path: &[Seg]) -> String {
    let keys: Vec<crate::doc::Key> = path
        .iter()
        .map(|s| match s {
            Seg::Name(n) => crate::doc::Key::Name(n.as_str().into()),
            Seg::Index(i) => crate::doc::Key::Index(u32::try_from(*i).unwrap_or(u32::MAX)),
        })
        .collect();
    fmt::path_jq(&keys)
}

/// Sort a failure into what it means for aless.
fn classify(job: &Job, failed: Failed, written: &AtomicU64, broken: &AtomicBool) -> ExportError {
    let Failed {
        fail,
        stop,
        missing,
    } = failed;
    if let Some(nearest) = missing {
        return ExportError::NotFound {
            message: not_found_message(job, &nearest),
            nearest: Box::new(nearest),
        };
    }
    let partial = written.load(Ordering::Relaxed) > 0 || fail.committed_output;
    let position = |fail: &Fail| -> (u32, u32) {
        let at = |n: Option<u64>| n.and_then(|n| u32::try_from(n).ok()).unwrap_or(0);
        (at(fail.row), at(fail.column))
    };
    let load = |error: LoadError| ExportError::Load {
        error: Box::new(error),
        partial,
    };
    match stop.as_ref().map(|s| s.why()) {
        Some(STOP_DEPTH) => {
            let (line, col) = stop.as_ref().map_or((0, 0), |s| s.position());
            return load(too_deep(job, line, col, None));
        }
        Some(STOP_TIME) => {
            let (line, col) = stop.as_ref().map_or((0, 0), |s| s.position());
            return load(timed_out(job, line, col));
        }
        _ => {}
    }
    match fail.code {
        // Only the deadline's alarm raises the abort flag ahead of the
        // guard: on a line-by-line read, or between two of a grammar's
        // parses.
        Code::Aborted => {
            let (line, col) = position(&fail);
            load(timed_out(job, line, col))
        }
        // A grammar's own depth limit stops the parse with the engine's
        // `cancel`, as the viewer's loader reads it too.
        Code::InputInvalid if fail.message.starts_with("cancel") => {
            let (line, col) = position(&fail);
            load(too_deep(job, line, col, Some(job.format)))
        }
        Code::OutputFailed if broken.load(Ordering::Relaxed) => ExportError::ReaderGone,
        _ => {
            let mut fail = absolute(fail, &job.path);
            if partial {
                fail = fail.committed();
            }
            ExportError::Transduce(Box::new(fail))
        }
    }
}

/// A failure's path, relative to the exported value, made absolute; a
/// failure that names no path, or something that is not one (a renderer's
/// `column "x", row 3`), is left alone.
fn absolute(mut fail: Fail, base: &[Seg]) -> Fail {
    if base.is_empty() {
        return fail;
    }
    if let Some(p) = &fail.path {
        let base = path_text(base);
        fail.path = Some(match p.as_str() {
            "." => base,
            p if p.starts_with('.') || p.starts_with('[') => format!("{base}{p}"),
            _ => return fail,
        });
    }
    fail
}

/// A load error of aless's own with a position, named for the report:
/// `[aless/<code>]: message` over `--> origin:line:col`.
fn positioned(
    job: &Job,
    code: &str,
    message: String,
    hint: &str,
    line: u32,
    col: u32,
) -> LoadError {
    let mut e = LoadError::tagged(code, message).with_hint(hint);
    let origin = if line > 0 {
        format!("{}:{line}:{col}", job.origin)
    } else {
        job.origin.clone()
    };
    e = e.with_origin(&origin);
    e.line = line;
    e.col = col;
    e
}

/// `too_deep`, worded as the loader words it: aless's cap, or the
/// grammar's own when `grammar` says which.
fn too_deep(job: &Job, line: u32, col: u32, grammar: Option<Format>) -> LoadError {
    let (message, hint) = match grammar {
        None => (
            format!(
                "too_deep: nested deeper than aless reads (about {} levels)",
                MAX_RULE_DEPTH / 3
            ),
            "Nesting this deep would overflow the parser's stack, so the parse \
             stopped here.\nReal documents nest a few dozen levels at most."
                .to_string(),
        ),
        Some(format) => (
            format!("too_deep: nested deeper than the {format} grammar reads"),
            format!(
                "The {format} grammar has a nesting limit of its own, and the parse stopped \
                 where the document passed it.\nReal documents nest a few dozen levels at most."
            ),
        ),
    };
    positioned(job, "too_deep", message, &hint, line, col)
}

/// `timeout`, worded as the loader words a parse stopped at its deadline.
fn timed_out(job: &Job, line: u32, col: u32) -> LoadError {
    let limit = job.timeout.map_or(0.0, |t| t.as_secs_f64()).to_string();
    let message = format!("timeout: the parse ran longer than {limit} s");
    let hint = format!(
        "The parse had got this far when --timeout {limit} stopped it; what was written \
         before that stays written.\nPass a larger --timeout to let it finish, or --timeout 0 \
         for no limit."
    );
    positioned(job, "timeout", message, &hint, line, col)
}

/// `no PATH in FILE: NEAREST has no key "x"`, as the loader's path
/// resolution words a miss.
fn not_found_message(job: &Job, nearest: &Nearest) -> String {
    let seg = &job.path[nearest.depth];
    let why = match nearest.kind {
        "object" => match seg {
            Seg::Name(k) => format!("has no key {}", fmt::quote(k)),
            Seg::Index(i) => format!("has no key \"{i}\""),
        },
        "array" => match nearest.length.unwrap_or(0) {
            0 => "is an empty array".to_string(),
            1 => "has 1 item, [0]".to_string(),
            n => format!("has {n} items, [0] to [{}]", n - 1),
        },
        kind => format!("is {}, which has no keys", with_article(kind)),
    };
    let near = if nearest.depth == 0 {
        "the root (.)".to_string()
    } else {
        nearest.path.clone()
    };
    format!("no {} in {}: {near} {why}", path_text(&job.path), job.name)
}

fn with_article(word: &str) -> String {
    match word.chars().next() {
        Some('a' | 'e' | 'i' | 'o' | 'u') => format!("an {word}"),
        _ => format!("a {word}"),
    }
}

// ----- the writer ------------------------------------------------------------

/// The writer under the renderer's `WriteOut`: counts what reached the
/// output, so a failure can say whether output is partial, and notices a
/// reader that went away, which is not a failure.
struct Pipe {
    inner: Box<dyn Write + Send>,
    written: Arc<AtomicU64>,
    broken: Arc<AtomicBool>,
}

impl Pipe {
    fn note(&self, r: io::Result<()>) -> io::Result<()> {
        if let Err(e) = &r {
            if e.kind() == io::ErrorKind::BrokenPipe {
                self.broken.store(true, Ordering::Relaxed);
            }
        }
        r
    }
}

impl Write for Pipe {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let r = self.inner.write_all(buf);
        self.note(r)?;
        self.written.fetch_add(buf.len() as u64, Ordering::Relaxed);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        let r = self.inner.flush();
        self.note(r)
    }
}

// ----- the scope -------------------------------------------------------------

/// The value at `--path`, as a document of its own.
///
/// Every event of the input passes through; only those of the value at
/// the target path go on to the sink, followed by the document's `End`,
/// so the sink sees one whole document rooted there. The rest is tracked,
/// to know where the target begins and to describe the nearest node when
/// it is never found, and the input is read to its end, so it is validated
/// whole as `--json` validates it. The path is read as `--path` reads it
/// everywhere: a decimal name indexes an array, an index on an object is
/// its key.
struct Scope<S> {
    inner: S,
    target: Vec<Seg>,
    frames: Vec<Frame>,
    state: State,
    /// The deepest node on the target's path seen so far.
    nearest: Option<Nearest>,
    /// The document ended without the target.
    missing: bool,
}

struct Frame {
    kind: Container,
    /// Members or elements begun so far; for an array, the next index.
    count: usize,
    /// Whether this container's own path is a prefix of the target's, so
    /// that a child of it may be on the path.
    on_path: bool,
}

enum Container {
    Object { key: Option<Box<str>> },
    Array,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Before,
    /// Inside the target, which began with `depth` containers open.
    Inside {
        depth: usize,
    },
    After,
}

impl<S: Sink> Scope<S> {
    fn new(target: Vec<Seg>, inner: S) -> Scope<S> {
        Scope {
            inner,
            target,
            frames: Vec::new(),
            state: State::Before,
            nearest: None,
            missing: false,
        }
    }

    /// The nearest node, when the document ended without the target.
    fn missing(&self) -> Option<Nearest> {
        self.missing.then(|| self.nearest.clone()).flatten()
    }

    /// Whether a value beginning now is on the target's path, and whether
    /// it is the target.
    fn locate(&self) -> (bool, bool) {
        let depth = self.frames.len();
        let Some(parent) = self.frames.last() else {
            return (true, self.target.is_empty());
        };
        if !parent.on_path || depth > self.target.len() {
            return (false, false);
        }
        let seg = &self.target[depth - 1];
        let matches = match &parent.kind {
            Container::Object { key: Some(k) } => seg_names_key(seg, k),
            Container::Object { key: None } => false,
            Container::Array => seg_names_index(seg, parent.count),
        };
        (matches, matches && depth == self.target.len())
    }

    /// A child landed in the frame on top; the nearest node's description
    /// grows when that frame is it.
    fn count_child(&mut self) {
        let depth = self.frames.len();
        let Some(parent) = self.frames.last_mut() else {
            return;
        };
        let key = match &parent.kind {
            Container::Object { key } => key.clone(),
            Container::Array => None,
        };
        parent.count += 1;
        if let Some(near) = &mut self.nearest {
            if near.depth + 1 == depth && near.length.is_some() {
                near.length = Some(parent.count);
                if let (Some(keys), Some(k)) = (&mut near.keys, key) {
                    if keys.len() < KEYS_LISTED {
                        keys.push(k.to_string());
                    }
                }
            }
        }
    }

    fn forward(&mut self, ev: JsonEvent<'_>) -> Result<Flow, Fail> {
        self.inner.event(ev)
    }
}

impl<S: Sink> Sink for Scope<S> {
    fn event(&mut self, ev: JsonEvent<'_>) -> Result<Flow, Fail> {
        match ev {
            JsonEvent::Key(k) => {
                if let Some(Frame {
                    kind: Container::Object { key },
                    ..
                }) = self.frames.last_mut()
                {
                    *key = Some(k.into());
                }
                match self.state {
                    State::Inside { .. } => self.forward(ev),
                    _ => Ok(Flow::Continue),
                }
            }
            JsonEvent::ObjectEnd | JsonEvent::ArrayEnd => {
                self.frames.pop();
                match self.state {
                    State::Inside { depth } => {
                        let flow = self.forward(ev)?;
                        if self.frames.len() == depth {
                            self.state = State::After;
                        }
                        Ok(flow)
                    }
                    _ => Ok(Flow::Continue),
                }
            }
            JsonEvent::End => match self.state {
                State::After => self.forward(ev),
                State::Before => {
                    self.missing = true;
                    // The caller reads the nearest node off the scope; the
                    // code here only ends the run.
                    Err(Fail::input(format!(
                        "the document has no {}",
                        path_text(&self.target)
                    )))
                }
                State::Inside { .. } => Err(Fail::protocol(
                    "the document ended inside the value being exported",
                )),
            },
            _ => {
                let (on_path, is_target) = self.locate();
                let depth = self.frames.len();
                self.count_child();
                if self.state == State::Before && is_target {
                    self.state = State::Inside { depth };
                }
                let inside = matches!(self.state, State::Inside { .. });
                let path = || path_text(&self.target[..depth]);
                if ev.is_start() {
                    let kind = if ev == JsonEvent::ObjectStart {
                        Container::Object { key: None }
                    } else {
                        Container::Array
                    };
                    if on_path && !is_target {
                        let object = matches!(kind, Container::Object { .. });
                        self.nearest = Some(Nearest {
                            path: path(),
                            kind: kind_of_event(&ev),
                            length: Some(0),
                            value: None,
                            keys: object.then(Vec::new),
                            depth,
                        });
                    }
                    self.frames.push(Frame {
                        kind,
                        count: 0,
                        on_path,
                    });
                } else if on_path && !is_target {
                    self.nearest = Some(Nearest {
                        path: path(),
                        kind: kind_of_event(&ev),
                        length: None,
                        value: Some(scalar_value(&ev)),
                        keys: None,
                        depth,
                    });
                }
                if !inside {
                    return Ok(Flow::Continue);
                }
                let flow = self.forward(ev)?;
                if let State::Inside { depth: entered } = self.state {
                    // A scalar target is whole at once.
                    if ev.is_scalar() && self.frames.len() == entered {
                        self.state = State::After;
                    }
                }
                Ok(flow)
            }
        }
    }
}

/// Whether a path step names an object's member `key`.
fn seg_names_key(seg: &Seg, key: &str) -> bool {
    match seg {
        Seg::Name(s) => s == key,
        Seg::Index(i) => i.to_string() == key,
    }
}

/// Whether a path step names an array's element `index`: an index, or a
/// name that is the canonical decimal of one.
fn seg_names_index(seg: &Seg, index: usize) -> bool {
    match seg {
        Seg::Index(i) => usize::try_from(*i).is_ok_and(|i| i == index),
        Seg::Name(s) => {
            let canonical = s == "0" || (!s.starts_with('0') && !s.is_empty());
            canonical && s.bytes().all(|b| b.is_ascii_digit()) && s.parse() == Ok(index)
        }
    }
}

/// A scalar event's value, as an entry prints one.
fn scalar_value(ev: &JsonEvent<'_>) -> Value {
    match ev {
        JsonEvent::Null => Value::Null,
        JsonEvent::Bool(b) => Value::Bool(*b),
        JsonEvent::Number(n) => {
            if !n.value.is_finite() {
                Value::String(fmt::number(n.value))
            } else if n.value.fract() == 0.0 && n.value.abs() < 1e16 {
                Value::from(n.value as i64)
            } else {
                serde_json::Number::from_f64(n.value).map_or(Value::Null, Value::Number)
            }
        }
        JsonEvent::String(s) => Value::String(s.to_string()),
        _ => Value::Null,
    }
}

// ----- the rows --------------------------------------------------------------

/// The rows of a CSV export: the elements of the array the stream is
/// rooted at, each an object (its members are the columns) or each a
/// scalar (one column, `value`).
///
/// Anything else is the input's fault, said with its path: a root that is
/// not an array (the message names `--path`), a row that is an array, and
/// a row of the other kind than the first. A scalar row is wrapped as
/// `{"value": …}` so the table transducer's inferred schema has its one
/// column.
struct Rows<S> {
    inner: S,
    /// Containers open in the re-rooted stream.
    depth: usize,
    started: bool,
    kind: Option<RowKind>,
    /// Rows begun.
    rows: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RowKind {
    Objects,
    Scalars,
}

impl<S: Sink> Rows<S> {
    fn new(inner: S) -> Rows<S> {
        Rows {
            inner,
            depth: 0,
            started: false,
            kind: None,
            rows: 0,
        }
    }

    fn row(&mut self, ev: JsonEvent<'_>) -> Result<Flow, Fail> {
        let index = self.rows;
        self.rows += 1;
        let at = format!("[{index}]");
        let kind = kind_of_event(&ev);
        let wanted = if ev == JsonEvent::ObjectStart {
            RowKind::Objects
        } else if ev == JsonEvent::ArrayStart {
            return Err(Fail::input(format!(
                "row {at} is an array; a row is an object (its members are the columns) or a \
                 scalar (one column, \"value\")"
            ))
            .at_path(at));
        } else {
            RowKind::Scalars
        };
        match self.kind {
            None => self.kind = Some(wanted),
            Some(first) if first != wanted => {
                let (this, that) = match wanted {
                    RowKind::Objects => ("an object".to_string(), "a scalar"),
                    RowKind::Scalars => (with_article(kind), "an object"),
                };
                return Err(Fail::input(format!(
                    "row {at} is {this}, but the first row was {that}; every row is an object \
                     with the columns as its members, or every row is a scalar"
                ))
                .at_path(at));
            }
            Some(_) => {}
        }
        match wanted {
            RowKind::Objects => {
                self.depth += 1;
                self.inner.event(ev)
            }
            RowKind::Scalars => {
                for wrapped in [JsonEvent::ObjectStart, JsonEvent::Key("value"), ev] {
                    if self.inner.event(wrapped)? == Flow::Stop {
                        return Ok(Flow::Stop);
                    }
                }
                self.inner.event(JsonEvent::ObjectEnd)
            }
        }
    }
}

impl<S: Sink> Sink for Rows<S> {
    fn event(&mut self, ev: JsonEvent<'_>) -> Result<Flow, Fail> {
        if !self.started {
            self.started = true;
            if ev == JsonEvent::ArrayStart {
                self.depth = 1;
                return self.inner.event(ev);
            }
            return Err(Fail::input(format!(
                "the value to export is {}, not an array of records; give --path to an array \
                 whose elements are the rows (--paths --depth 2 shows the arrays)",
                with_article(kind_of_event(&ev))
            ))
            .at_path("."));
        }
        match self.depth {
            0 => self.inner.event(ev),
            1 => match ev {
                JsonEvent::ArrayEnd => {
                    self.depth = 0;
                    self.inner.event(ev)
                }
                JsonEvent::End | JsonEvent::Key(_) | JsonEvent::ObjectEnd => {
                    // The scope hands over one whole value, so these do
                    // not arrive at a row boundary.
                    Err(Fail::protocol("an event out of place between rows"))
                }
                _ => self.row(ev),
            },
            _ => {
                if ev.is_start() {
                    self.depth += 1;
                } else if ev.is_end() {
                    self.depth -= 1;
                }
                self.inner.event(ev)
            }
        }
    }
}

// ----- the empty table -------------------------------------------------------

/// A table with no columns and no rows (an empty array of records) is an
/// empty export, not a failure: the renderer never sees its schema. One
/// with rows but no columns (`[{}]`) has no CSV form, and the renderer
/// says so.
struct EmptyOk<S> {
    inner: S,
    held: bool,
}

impl<S: TableSink> EmptyOk<S> {
    fn new(inner: S) -> EmptyOk<S> {
        EmptyOk { inner, held: false }
    }
}

impl<S: TableSink> TableSink for EmptyOk<S> {
    fn table_event(&mut self, ev: TableEvent<'_>) -> Result<Flow, Fail> {
        match ev {
            TableEvent::Schema([]) => {
                self.held = true;
                Ok(Flow::Continue)
            }
            TableEvent::Row(_) if self.held => {
                self.held = false;
                self.inner.table_event(TableEvent::Schema(&[]))?;
                self.inner.table_event(ev)
            }
            TableEvent::End if self.held => Ok(Flow::Continue),
            _ => self.inner.table_event(ev),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tabnas_transduce::sink::replay;
    use tabnas_transduce::{OwnedJsonEvent, Source};

    /// A writer the test can read back after the run took it.
    #[derive(Clone, Default)]
    struct Shared(Arc<Mutex<Vec<u8>>>);

    impl Write for Shared {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Shared {
        fn text(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
        }
    }

    fn job(format: Format, renderer: Renderer, path: &str) -> Job {
        Job {
            name: "-".into(),
            origin: "(stdin)".into(),
            format,
            renderer,
            path: crate::headless::parse_path(path).unwrap(),
            compact: true,
            indent: 2,
            timeout: None,
        }
    }

    fn run_text(job: &Job, text: &str) -> (Result<(), ExportError>, String) {
        let out = Shared::default();
        let r = export(job, Input::Text(text), Box::new(out.clone()));
        (r, out.text())
    }

    fn run_lines(job: &Job, text: &str) -> (Result<(), ExportError>, String) {
        let out = Shared::default();
        let reader = Box::new(io::BufReader::new(text.as_bytes()));
        let r = export(job, Input::Lines(reader), Box::new(out.clone()));
        (r, out.text())
    }

    fn events(json: &str) -> Vec<OwnedJsonEvent> {
        let value = tabnas_json::parse(json).unwrap();
        let mut rec = Vec::new();
        tabnas_transduce::ValueSource(&value).run(&mut rec).unwrap();
        rec
    }

    #[test]
    fn the_plan_follows_the_format_and_the_start() {
        assert_eq!(plan(Format::Jsonl, true), Some(Plan::Lines));
        assert_eq!(plan(Format::Csv, true), Some(Plan::Lines));
        assert_eq!(plan(Format::Tsv, true), Some(Plan::Lines));
        // A path into a line-delimited file needs the document.
        assert_eq!(
            plan(Format::Jsonl, false),
            Some(Plan::Incremental { prune: true })
        );
        assert_eq!(plan(Format::Csv, false), Some(Plan::Materialize));
        for f in [Format::Json, Format::Json5, Format::Jsonc, Format::Zon] {
            assert_eq!(
                plan(f, true),
                Some(Plan::Incremental { prune: true }),
                "{f}"
            );
            assert_eq!(
                plan(f, false),
                Some(Plan::Incremental { prune: true }),
                "{f}"
            );
        }
        assert_eq!(
            plan(Format::Yaml, true),
            Some(Plan::Incremental { prune: false }),
            "aliases copy their anchor when met"
        );
        for f in [
            Format::Jsonic,
            Format::Toml,
            Format::Ini,
            Format::Xml,
            Format::Markdown,
            Format::Feed,
        ] {
            assert_eq!(plan(f, true), Some(Plan::Materialize), "{f}");
        }
        assert_eq!(plan(Format::Text, true), None);
        assert_eq!(Renderer::from_name(" CSV "), Some(Renderer::Csv));
        assert_eq!(Renderer::from_name("json"), Some(Renderer::Json));
        assert_eq!(Renderer::from_name("xml"), None);
    }

    #[test]
    fn the_scope_re_roots_the_stream_at_the_path() {
        let doc = events(r#"{"a": [1, {"b": [true, "x"]}], "c": null}"#);
        let scoped = |path: &str| {
            let mut scope = Scope::new(crate::headless::parse_path(path).unwrap(), Vec::new());
            let r = replay(&doc, &mut scope);
            let missing = scope.missing();
            (r, scope.inner, missing)
        };
        let (r, got, missing) = scoped(".a[1].b");
        assert_eq!(r.unwrap(), Flow::Continue);
        assert_eq!(got, events(r#"[true, "x"]"#));
        assert_eq!(missing, None);
        // The root, a scalar, and the lenient readings of a step.
        assert_eq!(scoped(".").1, doc);
        assert_eq!(scoped(".a[1].b[1]").1, events(r#""x""#));
        assert_eq!(scoped(".a.1.b").1, events(r#"[true, "x"]"#));
        assert_eq!(scoped("/a/1/b/0").1, events("true"));
        assert_eq!(scoped(".c").1, events("null"));

        // A miss: the run fails at the end, and the nearest node is known.
        let (r, got, missing) = scoped(".a[1].nope");
        assert_eq!(r.unwrap_err().code, Code::InputInvalid);
        assert!(got.is_empty());
        let near = missing.unwrap();
        assert_eq!(near.path, ".a[1]");
        assert_eq!(near.kind, "object");
        assert_eq!(near.length, Some(1));
        assert_eq!(near.keys, Some(vec!["b".to_string()]));
        assert_eq!(near.depth, 2);
        let near = scoped(".a[5]").2.unwrap();
        assert_eq!((near.kind, near.length), ("array", Some(2)));
        let near = scoped(".c.d").2.unwrap();
        assert_eq!((near.kind, near.value.clone()), ("null", Some(Value::Null)));
        let near = scoped(".zz").2.unwrap();
        assert_eq!(near.depth, 0);
        assert_eq!(near.keys, Some(vec!["a".to_string(), "c".to_string()]));
        assert_eq!(near.length, Some(2), "counted to the end, past the miss");
    }

    #[test]
    fn rows_wrap_scalars_and_refuse_the_rest() {
        let through = |json: &str| {
            let mut rows = Rows::new(Vec::new());
            let r = replay(&events(json), &mut rows);
            r.map(|_| rows.inner)
        };
        assert_eq!(
            through(r#"[1, "x", null]"#).unwrap(),
            events(r#"[{"value": 1}, {"value": "x"}, {"value": null}]"#)
        );
        assert_eq!(
            through(r#"[{"a": [1]}, {"b": {}}]"#).unwrap(),
            events(r#"[{"a": [1]}, {"b": {}}]"#)
        );
        assert_eq!(through("[]").unwrap(), events("[]"));
        let e = through(r#"{"a": 1}"#).unwrap_err();
        assert_eq!(e.code, Code::InputInvalid);
        assert!(e.message.contains("--path"), "{}", e.message);
        assert_eq!(e.path.as_deref(), Some("."));
        let e = through(r#"[{"a": 1}, [2]]"#).unwrap_err();
        assert_eq!(e.path.as_deref(), Some("[1]"));
        assert!(e.message.contains("is an array"), "{}", e.message);
        let e = through(r#"[{"a": 1}, 2]"#).unwrap_err();
        assert!(
            e.message
                .contains("is a number, but the first row was an object"),
            "{}",
            e.message
        );
        let e = through(r#"[2, {"a": 1}]"#).unwrap_err();
        assert!(
            e.message
                .contains("is an object, but the first row was a scalar"),
            "{}",
            e.message
        );
    }

    #[test]
    fn csv_from_json_text_and_from_lines() {
        let j = job(Format::Json, Renderer::Csv, ".");
        let (r, out) = run_text(&j, r#"[{"a": 1.50, "b": "x"}, {"b": "y,z", "c": true}]"#);
        r.unwrap();
        assert_eq!(out, "\"a\",\"b\"\r\n\"1.50\",\"x\"\r\n\"\",\"y,z\"\r\n");
        // Scalars: one column.
        let (r, out) = run_text(&j, "[1, 2]");
        r.unwrap();
        assert_eq!(out, "\"value\"\r\n\"1\"\r\n\"2\"\r\n");
        // An empty array exports as nothing.
        let (r, out) = run_text(&j, "[]");
        r.unwrap();
        assert_eq!(out, "");
        // Rows with no columns have no CSV form.
        let (r, _) = run_text(&j, "[{}]");
        match r.unwrap_err() {
            ExportError::Transduce(f) => assert_eq!(f.code, Code::TargetValueUnrepresentable),
            other => panic!("{other:?}"),
        }
        // JSON Lines, a record at a time, lexemes kept.
        let j = job(Format::Jsonl, Renderer::Csv, ".");
        let (r, out) = run_lines(&j, "{\"n\": 1e2}\n\n{\"n\": 2}\n");
        r.unwrap();
        assert_eq!(out, "\"n\"\r\n\"1e2\"\r\n\"2\"\r\n");
        // CSV in, CSV out: the records keyed by the header.
        let j = job(Format::Csv, Renderer::Csv, ".");
        let (r, out) = run_lines(&j, "a,b\n1,\"x\"\"y\"\n");
        r.unwrap();
        assert_eq!(out, "\"a\",\"b\"\r\n\"1\",\"x\"\"y\"\r\n");
        // TSV: the grammar's records, whose fields are text as in the viewer.
        let j = job(Format::Tsv, Renderer::Json, ".");
        let (r, out) = run_lines(&j, "a\tb\n1\t2\n");
        r.unwrap();
        assert_eq!(out, "[{\"a\":\"1\",\"b\":\"2\"}]\n");
    }

    #[test]
    fn json_streams_the_value_at_the_path() {
        let mut j = job(Format::Json, Renderer::Json, ".a");
        let (r, out) = run_text(&j, r#"{"a": {"b": [1, 2.0, "x"]}, "c": 1}"#);
        r.unwrap();
        assert_eq!(out, "{\"b\":[1,2.0,\"x\"]}\n");
        j.compact = false;
        let (_, out) = run_text(&j, r#"{"a": {"b": [1]}}"#);
        assert_eq!(out, "{\n  \"b\": [\n    1\n  ]\n}\n");
        // Other formats walk their value.
        let j = job(Format::Toml, Renderer::Json, ".");
        let (r, out) = run_text(&j, "a = 1\nb = [1.5, \"x\"]\n");
        r.unwrap();
        assert_eq!(out, "{\"a\":1,\"b\":[1.5,\"x\"]}\n");
    }

    #[test]
    fn failures_are_sorted_by_what_they_mean() {
        // The input did not parse: the transducer's code, with the position.
        let j = job(Format::Json, Renderer::Json, ".");
        match run_text(&j, "{\"a\": 1,\n \"b\": }").0.unwrap_err() {
            ExportError::Transduce(f) => {
                assert_eq!(f.code, Code::InputInvalid);
                assert_eq!((f.row, f.column), (Some(2), Some(7)));
                assert!(!f.committed_output);
            }
            other => panic!("{other:?}"),
        }
        // A path that names nothing.
        let j = job(Format::Json, Renderer::Csv, ".rows");
        match run_text(&j, r#"{"items": []}"#).0.unwrap_err() {
            ExportError::NotFound { message, nearest } => {
                assert_eq!(message, "no .rows in -: the root (.) has no key \"rows\"");
                assert_eq!(nearest.keys, Some(vec!["items".to_string()]));
            }
            other => panic!("{other:?}"),
        }
        // A path counting from the end.
        let j = job(Format::Json, Renderer::Csv, ".a[-1]");
        assert!(matches!(
            run_text(&j, "[]").0.unwrap_err(),
            ExportError::Usage(m) if m.contains("[-1]")
        ));
        // A row failure's path is made absolute. The header and a row
        // had been rendered, but the renderer flushes at the end only, so
        // nothing left it: the output is none, and stays a whole table or
        // nothing.
        let j = job(Format::Json, Renderer::Csv, ".a.b");
        let (r, out) = run_text(&j, r#"{"a": {"b": [{"x": 1}, [2]]}}"#);
        match r.unwrap_err() {
            ExportError::Transduce(f) => {
                assert_eq!(f.path.as_deref(), Some(".a.b[1]"));
                assert!(!f.committed_output);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(out, "");
        assert_eq!(
            absolute(
                Fail::input("x").at_path("column \"a\", row 1"),
                &[Seg::Name("a".into())]
            )
            .path
            .as_deref(),
            Some("column \"a\", row 1")
        );
        // A grammar's own depth limit reads as too_deep, as the loader says.
        let deep = format!("{}1{}", "[".repeat(300), "]".repeat(300));
        match run_text(&j, &deep).0.unwrap_err() {
            ExportError::Load { error, partial } => {
                assert_eq!(error.code, "too_deep");
                assert!(error.message.contains("json grammar"), "{}", error.message);
                assert!(error.line > 0);
                assert!(!partial);
                assert!(
                    error.plain_report().contains("(stdin):1:"),
                    "{}",
                    error.report
                );
            }
            other => panic!("{other:?}"),
        }
        // A reader that went away is no failure.
        struct Gone;
        impl Write for Gone {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::from(io::ErrorKind::BrokenPipe))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let j = job(Format::Json, Renderer::Json, ".");
        assert!(matches!(
            export(&j, Input::Text("[1]"), Box::new(Gone)).unwrap_err(),
            ExportError::ReaderGone
        ));
        // Any other writer failure is the output's.
        struct Full;
        impl Write for Full {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::from(io::ErrorKind::StorageFull))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        match export(&j, Input::Text("[1]"), Box::new(Full)).unwrap_err() {
            ExportError::Transduce(f) => assert_eq!(f.code, Code::OutputFailed),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_timeout_stops_both_kinds_of_source() {
        let mut j = job(Format::Toml, Renderer::Json, ".");
        j.timeout = Some(Duration::from_millis(1));
        let toml: String = (0..3_000)
            .map(|i| format!("[[item]]\nid = {i}\nname = \"item {i}\"\n\n"))
            .collect();
        match run_text(&j, &toml).0.unwrap_err() {
            ExportError::Load { error, partial } => {
                assert_eq!(error.code, "timeout");
                assert!(error.line > 0, "how far it got");
                assert!(!partial, "a walk writes nothing before the parse ends");
            }
            other => panic!("{other:?}"),
        }
        let mut j = job(Format::Jsonl, Renderer::Csv, ".");
        j.timeout = Some(Duration::from_millis(1));
        let lines: String = (0..40_000)
            .map(|i| format!("{{\"id\": {i}, \"name\": \"item {i}\"}}\n"))
            .collect();
        match run_lines(&j, &lines).0.unwrap_err() {
            ExportError::Load { error, .. } => assert_eq!(error.code, "timeout"),
            other => panic!("{other:?}"),
        }
        // Time enough: the same inputs export.
        j.timeout = Some(Duration::from_secs(120));
        run_lines(&j, &lines).0.unwrap();
    }

    #[test]
    fn an_empty_schema_is_held_back() {
        use tabnas_transduce::Table;
        let mut t = EmptyOk::new(Table::default());
        t.table_event(TableEvent::Schema(&[])).unwrap();
        t.table_event(TableEvent::End).unwrap();
        assert!(!t.inner.ended, "nothing reached the renderer");
        let mut t = EmptyOk::new(Table::default());
        t.table_event(TableEvent::Schema(&[])).unwrap();
        t.table_event(TableEvent::Row(&[])).unwrap();
        assert!(t.inner.columns.is_empty() && t.inner.rows.len() == 1);
    }
}
