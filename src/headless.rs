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

use std::io::{self, BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};
use tabnas_transduce::{Code, Fail, JsonEvent, Metrics};

use crate::alchemy::{self, ProgramArg};
use crate::doc::{Doc, Key, Kind, NodeId, NO_NODE};
use crate::export::{self, ExportError, Input, Job, Nearest, Plan, Records, Renderer, What};
use crate::fmt;
use crate::grammar::GrammarError;
use crate::load::{self, Format, Limits, LoadError, Loaded};
use crate::search;
use crate::translate::{self, Part};

/// Exit statuses.
///
/// `--render` reports through the same numbers. A failure of its own
/// (`"kind": "transduce"`) carries the transducer's code, and the code
/// picks the status: `RESOURCE_LIMIT_EXCEEDED` is [`TOO_LARGE`],
/// `OUTPUT_FAILED` is [`IO`], `ABORTED` is [`TIMEOUT`], and every other
/// code (`INPUT_INVALID`, the protocol and target codes) is [`PARSE`].
/// aless's own limits report as they do for a parse: nesting past its cap
/// is a `parse` error with the code `too_deep` ([`PARSE`]), and the
/// deadline a `timeout` error ([`TIMEOUT`]). A `--path` that names
/// nothing is [`NOT_FOUND`] as ever.
pub mod status {
    /// Standard output holds the answer.
    pub const OK: i32 = 0;
    /// The input did not parse. With `--check`: an input failed, and
    /// standard output says which and why.
    pub const PARSE: i32 = 1;
    /// The command line asked for something aless cannot do: a bad option
    /// or path syntax, no input, a directory, a `--grammar` that does not
    /// compile, or the viewer without a terminal.
    pub const USAGE: i32 = 2;
    /// An input could not be read, or the output not written.
    pub const IO: i32 = 3;
    /// `--path` or `--at` names nothing in the document.
    pub const NOT_FOUND: i32 = 4;
    /// An input is larger than `--max-size` allows, or an export passed
    /// one of the transducer's limits.
    pub const TOO_LARGE: i32 = 5;
    /// A parse ran longer than `--timeout` allows; under `--render` or
    /// `--alchemy`, the run, which the deadline covers whole.
    pub const TIMEOUT: i32 = 6;
}

/// How many entries `--paths` and `--find` print unless `--limit` says.
pub const DEFAULT_LIMIT: usize = 200;

/// A string value in an entry is cut to this many characters.
pub const VALUE_CHARS: usize = 200;

/// How many keys a not-found error lists.
pub(crate) const KEYS_LISTED: usize = 20;

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
    /// The records at the start as CSV, the value there as JSON text, or
    /// the value there as any format a crate ships a render for, streamed
    /// (see [`crate::export`] and [`crate::translate`]).
    Render(Renderer),
    /// An alchemy program's output over the input, streamed, or with
    /// `explain` its plan report instead (see [`crate::alchemy`]).
    /// `render` is `--render`, the renderer for a table or JSON events.
    Alchemy {
        program: ProgramArg,
        render: Option<Renderer>,
        explain: bool,
    },
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
            Op::Render(_) => "--render",
            Op::Alchemy { .. } => "--alchemy",
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
    /// Refuse an input larger than this many bytes; `None` for no limit.
    pub max_size: Option<u64>,
    /// Stop a parse that runs longer than this; `None` for no limit.
    pub timeout: Option<Duration>,
    /// Stop a program that writes more than this many bytes; `None` for
    /// no limit.
    pub max_output: Option<u64>,
    /// When the run started, for a time limit that covers reading its
    /// input too (see [`load::Limits::started`]); `None` to count from the
    /// start of the parse.
    pub started: Option<Instant>,
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
            max_size: Limits::DEFAULT.max_size,
            timeout: Limits::DEFAULT.timeout,
            max_output: Some(load::DEFAULT_MAX_OUTPUT),
            started: None,
        }
    }

    fn limits(&self) -> Limits {
        Limits {
            max_size: self.max_size,
            timeout: self.timeout,
            started: self.started,
        }
    }

    /// The limits for reading and parsing `source`. On standard input the
    /// time runs from the start of the run, so that waiting on it counts;
    /// on a file, from the start of its parse, as it always has, so that
    /// each of the files `--check` reads gets the whole of it.
    fn limits_for(&self, source: &Source) -> Limits {
        Limits {
            started: match source {
                Source::Stdin => self.started,
                Source::File(_) => None,
            },
            ..self.limits()
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

/// Standard input; the caller passes none when it is a terminal, which
/// nobody is going to type a document into. It is read to its end, but
/// never held past the size limit; `--render` on a line-delimited format
/// reads it a record at a time.
pub type Stdin<'a> = Option<&'a mut (dyn Read + Send)>;

/// Where a streamed result (`--render`) goes; the other results come back
/// in the [`Output`]. Owned, since the parse's subscriber holds it.
pub type Stdout = Box<dyn Write + Send>;

/// Carry out a request. A `--render` result is collected into the output
/// like any other; [`run_to`] streams it instead.
pub fn run(req: &Request, stdin: Stdin<'_>) -> Output {
    let collected = Arc::new(Mutex::new(Vec::new()));
    let mut out = run_to(req, stdin, Box::new(Collect(collected.clone())));
    let bytes = std::mem::take(&mut *collected.lock().unwrap_or_else(|e| e.into_inner()));
    if !bytes.is_empty() {
        out.stdout = String::from_utf8_lossy(&bytes).into_owned();
    }
    out
}

/// A writer into a shared buffer, for [`run`].
struct Collect(Arc<Mutex<Vec<u8>>>);

impl Write for Collect {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Carry out a request, streaming a `--render` result to `out` as it is
/// produced; `Output::stdout` is then empty.
pub fn run_to(req: &Request, mut stdin: Stdin<'_>, out: Stdout) -> Output {
    // What a run that succeeds leaves on standard error: nothing, but for
    // a write through a format's own render, its loss declaration.
    let mut note = String::new();
    let result = match &req.op {
        Op::Check => check(req, &mut stdin),
        Op::Render(Renderer::Part(id)) => translate(req, id, &mut stdin, out).map(|loss| {
            note = loss;
            (String::new(), status::OK)
        }),
        Op::Render(renderer) => {
            export(req, *renderer, &mut stdin, out).map(|()| (String::new(), status::OK))
        }
        Op::Alchemy {
            program,
            render,
            explain,
        } => run_program(req, program, *render, *explain, &mut stdin, out).map(|(stdout, loss)| {
            note = loss;
            (stdout, status::OK)
        }),
        _ => single(req, &mut stdin),
    };
    match result {
        Ok((stdout, status)) => Output {
            stdout,
            stderr: note,
            status,
        },
        Err(f) => Output {
            stdout: String::new(),
            stderr: render(&json!({ "error": f.error }), req.compact),
            status: f.status,
        },
    }
}

/// The one input of an operation that reads one.
fn one_source(req: &Request) -> Result<Source, Failure> {
    let mut sources = sources(req);
    if sources.len() > 1 {
        return Err(Failure::usage(format!(
            "{} reads one input, and was given {}: run aless once per file, or use --check to check several",
            req.op.flag(),
            sources.len()
        )));
    }
    Ok(sources.remove(0))
}

fn single(req: &Request, stdin: &mut Stdin<'_>) -> Result<(String, i32), Failure> {
    // A bad pattern is a mistake in the command; say so before reading.
    let pattern = match &req.op {
        Op::Find(p) => Some(search::compile(p).map_err(Failure::usage)?),
        _ => None,
    };
    let source = one_source(req)?;
    let (name, loaded) = load(&source, req, stdin)?;
    let doc = &loaded.doc;
    let text = load::parser_text(&loaded.source);
    let start = select(doc, text, &req.start, &name, loaded.format)?;
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
        Op::Render(_) => unreachable!("--render is run by export()"),
        Op::Alchemy { .. } => unreachable!("--alchemy is run by run_program()"),
    };
    Ok((out, status::OK))
}

/// `--render`: the input streamed through the transducer to `out`.
fn export(
    req: &Request,
    renderer: Renderer,
    stdin: &mut Stdin<'_>,
    out: Stdout,
) -> Result<(), Failure> {
    let path = match &req.start {
        Start::Root => Vec::new(),
        Start::Path(text) => parse_path(text).map_err(Failure::usage)?,
        Start::At(..) => {
            return Err(Failure::usage(
                "--render reads the input once, front to back, and cannot find a source \
                 position in it: start it with --path, or use --where --at to find the path",
            ))
        }
    };
    let (source, name, origin, format) = streamed_source(req)?;
    let Some(plan) = export::plan(format, path.is_empty()) else {
        return Err(Failure::usage(format!(
            "{name} is plain text, which has no records to export: name its format with -k, \
             such as -k jsonl or -k csv"
        )));
    };
    let job = Job {
        name: name.clone(),
        origin,
        format,
        what: What::Render(renderer),
        path,
        compact: req.compact,
        indent: req.indent,
        timeout: req.timeout,
        started: req.limits_for(&source).started,
        max_output: None,
    };
    let result = match open_input(req, &source, &name, format, plan, stdin)? {
        Opened::Text(text) => export::export(&job, Input::Text(&text), out),
        Opened::Lines(reader) => export::export(&job, Input::Lines(reader), out),
    };
    let failure = match result {
        Ok(()) => return Ok(()),
        // The reader of the output went away (`| head`): nothing to report.
        Err(ExportError::ReaderGone) => return Ok(()),
        Err(e) => e,
    };
    Err(match failure {
        ExportError::Usage(m) => Failure::usage(m),
        ExportError::Load { error, partial } => {
            let mut f = Failure::load(&name, format, &error).limited(None, req);
            f.error.insert(
                "output".into(),
                if partial { "partial" } else { "none" }.into(),
            );
            f
        }
        ExportError::Transduce(fail) | ExportError::Program(fail) => {
            Failure::transduce(&name, format, &fail)
        }
        ExportError::NotFound { message, nearest } => {
            let path = match &req.start {
                Start::Path(text) => text.trim().to_string(),
                _ => ".".to_string(),
            };
            Failure::not_found_streamed(&name, format, &path, message, &nearest)
        }
        ExportError::ReaderGone => unreachable!("handled above"),
    })
}

/// `--render ID` for a format a crate ships a render for: the render
/// composed with the source's lift and the adapter the shapes need
/// ([`translate::compose`]) and run over the value at the start, as a
/// program runs ([`translate::run`]); or, for a format written by one of
/// aless's own renderers (`--render json5`), that renderer. A write that
/// succeeds returns the loss declaration for standard error
/// ([`loss_note`]); a failure met while it ran carries the same sentences
/// as `loss`.
fn translate(
    req: &Request,
    id: &str,
    stdin: &mut Stdin<'_>,
    out: Stdout,
) -> Result<String, Failure> {
    let part = translate::part(id).expect("--render names a part the registry has");
    let path = match &req.start {
        Start::Root => Vec::new(),
        Start::Path(text) => parse_path(text).map_err(Failure::usage)?,
        Start::At(..) => {
            return Err(Failure::usage(
                "--render reads the input once, front to back, and cannot find a source \
                 position in it: start it with --path, or use --where --at to find the path",
            ))
        }
    };
    let (source, name, origin, format) = streamed_source(req)?;
    // The source's parts stand at the root only: its lift reads a whole
    // document, so a value below the root is a tree whatever the document
    // was (a Markdown file's table is its rows at the root, and --path
    // into the file selects nodes of its tree, rows by their own shape).
    let source_part = if path.is_empty() {
        translate::source_part(format)
    } else {
        None
    };
    // The composition first: a part that does not compile is a failure
    // naming its file, said before the input is read.
    let translation = translate::translation(source_part, part)
        .map_err(|fail| Failure::alchemy(part_file(part, &fail), &fail))?;
    let loss = translation.loss(part);
    let composition = match translation {
        translate::Translation::Composed(composition) => composition,
        // One of aless's own renderers over the events as they are.
        translate::Translation::Native { renderer, adapter } => {
            return match export(req, renderer, stdin, out) {
                Ok(()) => Ok(loss_note(part, &loss, adapter, &name, req.compact)),
                Err(mut f) => {
                    f.error.insert("loss".into(), json!(loss));
                    Err(f)
                }
            };
        }
    };
    let Some(plan) = export::plan(format, path.is_empty()) else {
        return Err(Failure::usage(format!(
            "{name} is plain text, which has no values to write as {id}: name its format with \
             -k, such as -k jsonl or -k csv"
        )));
    };
    let job = Job {
        name: name.clone(),
        origin,
        format,
        what: What::Part,
        path,
        compact: req.compact,
        indent: req.indent,
        timeout: req.timeout,
        started: req.limits_for(&source).started,
        max_output: None,
    };
    let metrics = Metrics::new();
    let records = part.records();
    let result = match open_input(req, &source, &name, format, plan, stdin)? {
        Opened::Text(text) => translate::run(
            &job,
            &composition,
            records,
            Input::Text(&text),
            out,
            metrics,
        ),
        Opened::Lines(reader) => translate::run(
            &job,
            &composition,
            records,
            Input::Lines(reader),
            out,
            metrics,
        ),
    };
    let failure = match result {
        Ok(()) | Err(ExportError::ReaderGone) => {
            return Ok(loss_note(
                part,
                &loss,
                composition.adapter,
                &name,
                req.compact,
            ))
        }
        Err(e) => e,
    };
    let mut f = translation_failure(req, &name, format, part, failure)?;
    f.error.insert("loss".into(), json!(loss));
    Err(f)
}

/// The file a compile failure of a composition is placed in: the one the
/// failure names when it names one (a lift's, a render's, the program's),
/// else the render's.
fn part_file<'a>(part: &'a Part, fail: &'a tabnas_transduce::Fail) -> &'a str {
    match (&fail.file, &part.render) {
        (Some(file), _) => file,
        (None, translate::Render::Alc { file, .. }) => file,
        (None, _) => &part.id,
    }
}

/// A failure met while a translation ran, as the error object: a usage
/// error, a path that named nothing, aless's own limits, the render's or
/// the lift's own failure placed in its file with the input named beside
/// it, or the input's.
fn translation_failure(
    req: &Request,
    name: &str,
    format: Format,
    part: &Part,
    failure: ExportError,
) -> Result<Failure, Failure> {
    Ok(match failure {
        ExportError::Usage(m) => return Err(Failure::usage(m)),
        ExportError::NotFound { message, nearest } => {
            let path = match &req.start {
                Start::Path(text) => text.trim().to_string(),
                _ => ".".to_string(),
            };
            return Err(Failure::not_found_streamed(
                name, format, &path, message, &nearest,
            ));
        }
        ExportError::Load { error, partial } => {
            let mut f = Failure::load(name, format, &error).limited(None, req);
            f.error.insert(
                "output".into(),
                if partial { "partial" } else { "none" }.into(),
            );
            f
        }
        // A failure of a part's own, with a code of the language's or a
        // position in its text, is placed in the part, the input named
        // beside it, as a program's is.
        ExportError::Program(fail) if alchemy::is_placed(&fail) => {
            let mut f = Failure::alchemy(part_file(part, &fail), &fail);
            f.error.insert("input".into(), name.into());
            f
        }
        ExportError::Program(fail) | ExportError::Transduce(fail) => {
            Failure::transduce(name, format, &fail)
        }
        ExportError::ReaderGone => unreachable!("handled above"),
    })
}

/// What a write through a format's render leaves on standard error when
/// it succeeds: `{"warning": {"kind": "loss", "message", "file",
/// "render", "loss"}}`, the loss declaration, whose sentences say what a
/// document written that way does not keep, the adapter's among them when
/// one ran, named as `adapter`. Nothing, for a write that declares no
/// loss.
fn loss_note(
    part: &Part,
    loss: &[String],
    adapter: Option<translate::Adapter>,
    file: &str,
    compact: bool,
) -> String {
    if loss.is_empty() {
        return String::new();
    }
    let mut warning = json!({
        "kind": "loss",
        "message": format!(
            "the document was written as {}, which does not keep everything a document \
             can hold",
            part.id
        ),
        "file": file,
        "render": part.id,
        "loss": loss,
    });
    if let Some(adapter) = adapter {
        warning["adapter"] = json!(adapter.name());
    }
    render(&json!({ "warning": warning }), compact)
}

/// `--alchemy`: the program compiled, then the input streamed through it
/// to `out`; with `explain`, the program's plan report as JSON instead,
/// and no input read.
fn run_program(
    req: &Request,
    program: &ProgramArg,
    render: Option<Renderer>,
    explain: bool,
    stdin: &mut Stdin<'_>,
    out: Stdout,
) -> Result<(String, String), Failure> {
    // The program first: one that does not compile is the command's
    // mistake, whatever the input holds, and is said so before the input
    // is read.
    let file = program.name();
    let text = program.read(req.max_size).map_err(|e| {
        let size = program.path().and_then(file_size);
        Failure::program_read(&file, &e).limited(size, req)
    })?;
    let compiled = alchemy::compile(&text, &file).map_err(|fail| Failure::alchemy(&file, &fail))?;
    // --render is checked against the program before anything else,
    // --explain included: one the program refuses is the same usage error
    // either way. The plan's `renderer` is the program's own default,
    // whatever --render names.
    alchemy::check_render(&compiled, render).map_err(Failure::usage)?;
    if explain {
        if !req.files.is_empty() {
            return Err(Failure::usage(
                "--explain reports the program's plan and reads no input: give no FILE",
            ));
        }
        return Ok((
            self::render(&compiled.explain_json(), req.compact),
            String::new(),
        ));
    }
    // A format's render over the program's output, as --render chooses it
    // for a source ([`translate::program_translation`]): one of aless's own
    // renderers when the output reaches it as it is, with the adapter it
    // runs natively named, or the render composed over the output, the
    // program linked under another name, in one plan.
    let (part, translation) = match render {
        Some(Renderer::Part(id)) => {
            let part = translate::part(id).expect("--render names a part the registry has");
            let translation = translate::program_translation(
                tabnas_alchemy::Source::new(&file, &text),
                compiled.output(),
                part,
            )
            .map_err(|fail| {
                let in_program = fail.file.as_deref() == Some(file.as_str());
                let mut f = Failure::alchemy(
                    if in_program {
                        &file
                    } else {
                        part_file(part, &fail)
                    },
                    &fail,
                );
                if !in_program {
                    f.error.insert("program".into(), file.clone().into());
                }
                f
            })?;
            (Some(part), Some(translation))
        }
        _ => (None, None),
    };
    let (composition, render, adapter) = match &translation {
        Some(translate::Translation::Composed(c)) => (Some(c), None, c.adapter),
        Some(translate::Translation::Native { renderer, adapter }) => {
            (None, Some(*renderer), *adapter)
        }
        None => (None, render, None),
    };
    let (source, name, origin, format) = streamed_source(req)?;
    let Some(plan) = export::plan(format, true) else {
        return Err(Failure::usage(format!(
            "{name} is plain text, which has no values for a program to read: name its format \
             with -k, such as -k jsonl or -k csv"
        )));
    };
    let job = Job {
        name: name.clone(),
        origin,
        format,
        what: What::Program {
            rows: compiled.row_selector().cloned(),
        },
        path: Vec::new(),
        compact: req.compact,
        indent: req.indent,
        timeout: req.timeout,
        started: req.limits_for(&source).started,
        max_output: req.max_output,
    };
    let run = |input: Input<'_>| match composition {
        Some(composition) => {
            let records = part.map_or(Records::Any, Part::records);
            translate::run(&job, composition, records, input, out, Metrics::new())
        }
        None => alchemy::run(&job, &compiled, render, input, out),
    };
    let result = match open_input(req, &source, &name, format, plan, stdin)? {
        Opened::Text(text) => run(Input::Text(&text)),
        Opened::Lines(reader) => run(Input::Lines(reader)),
    };
    let loss = match (part, &translation) {
        (Some(part), Some(t)) => t.loss(part),
        _ => Vec::new(),
    };
    let failure = match result {
        Ok(()) | Err(ExportError::ReaderGone) => {
            let note = match part {
                Some(part) => loss_note(part, &loss, adapter, &name, req.compact),
                None => String::new(),
            };
            return Ok((String::new(), note));
        }
        Err(e) => e,
    };
    let mut failed = match failure {
        ExportError::Usage(m) => Failure::usage(m),
        // aless's own limits, the deadline among them: a timeout raised in
        // the program's work on an item carries no position (`export`
        // drops the program's span from it), one raised in the parse how
        // far the parse got.
        ExportError::Load { error, partial } => {
            let mut f = Failure::load(&name, format, &error).limited(None, req);
            f.error.insert(
                "output".into(),
                if partial { "partial" } else { "none" }.into(),
            );
            f
        }
        // Where a failure came from decides whose it is. One from the
        // program's sink with a code of the language's (a `match` no case
        // took, the evaluator's `recursion`) or with a position (a `fail`
        // at a record: the events a program reads carry no positions, so
        // the position is in the program) is the program's, placed in it,
        // with the input named beside it.
        ExportError::Program(fail) if alchemy::is_placed(&fail) => {
            let mut f = Failure::alchemy(&file, &fail);
            f.error.insert("input".into(), name.into());
            f
        }
        // One from the program's sink with neither (a renderer's, over the
        // rows the program built), and the source's whatever its code (a
        // verified grammar that refused to stream part-way, once output
        // had left: `STREAMABILITY_UNKNOWN` too), are the input's.
        ExportError::Program(fail) | ExportError::Transduce(fail) => {
            Failure::transduce(&name, format, &fail)
        }
        ExportError::NotFound { message, .. } => Failure::usage(message),
        ExportError::ReaderGone => unreachable!("handled above"),
    };
    if part.is_some() {
        failed.error.insert("loss".into(), json!(loss));
    }
    Err(failed)
}

/// The one input of a streamed run (`--render`, `--alchemy`): where it
/// is, the name outputs call it by, the origin a report names, and its
/// format.
fn streamed_source(req: &Request) -> Result<(Source, String, String, Format), Failure> {
    let source = one_source(req)?;
    let (name, origin, format) = match &source {
        Source::Stdin => (
            "-".to_string(),
            "(stdin)".to_string(),
            req.kind.unwrap_or(Format::Json),
        ),
        Source::File(p) => {
            let name = p.display().to_string();
            if p.is_dir() {
                return Err(Failure::usage(format!(
                    "{name} is a directory: aless reads files (list a directory with ls or find)"
                )));
            }
            (
                name,
                load::origin_of(p),
                req.kind.unwrap_or_else(|| Format::detect(p)),
            )
        }
    };
    Ok((source, name, origin, format))
}

/// A streamed run's input, opened the way its plan wants it read.
enum Opened<'a> {
    /// The whole text, read within `--max-size` as a parse reads it.
    Text(String),
    /// A reader over a line-delimited file, read a record at a time, so
    /// `--max-size` does not apply to it.
    Lines(Box<dyn BufRead + Send + 'a>),
}

/// Open a streamed run's input as `plan` wants it.
fn open_input<'a>(
    req: &Request,
    source: &Source,
    name: &str,
    format: Format,
    plan: Plan,
    stdin: &'a mut Stdin<'_>,
) -> Result<Opened<'a>, Failure> {
    let implicit = req.files.is_empty();
    let too_large = |e: &LoadError, size| Failure::load(name, format, e).limited(size, req);
    match (source, plan) {
        (Source::Stdin, plan) => {
            let Some(read) = stdin.as_mut() else {
                return Err(Failure::usage(if implicit {
                    "no input: name a FILE, or pipe a document into aless"
                } else {
                    "standard input is a terminal: pipe a document into aless, or name a FILE"
                }));
            };
            if plan == Plan::Lines {
                let mut reader = io::BufReader::new(read);
                let empty = match reader.fill_buf() {
                    Ok(bytes) => bytes.is_empty(),
                    // Nothing came before the deadline, not even an end.
                    Err(e) => match load::read_timed_out(&e) {
                        Some(limit) => {
                            let e = LoadError::input_timed_out(limit).with_origin("(stdin)");
                            return Err(too_large(&e, None));
                        }
                        None => false,
                    },
                };
                if implicit && empty {
                    return Err(Failure::usage(
                        "no input: standard input is empty; name a FILE, or pipe a document into aless",
                    ));
                }
                Ok(Opened::Lines(Box::new(reader)))
            } else {
                let bytes = load::read_within(read, req.max_size)
                    .map_err(|e| too_large(&e.with_origin("(stdin)"), None))?;
                if implicit && bytes.is_empty() {
                    return Err(Failure::usage(
                        "no input: standard input is empty; name a FILE, or pipe a document into aless",
                    ));
                }
                Ok(Opened::Text(String::from_utf8_lossy(&bytes).into_owned()))
            }
        }
        (Source::File(p), Plan::Lines) => {
            let file = std::fs::File::open(p).map_err(|e| {
                let e = LoadError::new(e.to_string()).with_origin(&load::origin_of(p));
                Failure::load(name, format, &e)
            })?;
            Ok(Opened::Lines(Box::new(io::BufReader::new(file))))
        }
        (Source::File(p), _) => {
            let text =
                load::read_path_within(p, req.max_size).map_err(|e| too_large(&e, file_size(p)))?;
            Ok(Opened::Text(text))
        }
    }
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
        let verdict = match load(&source, req, stdin) {
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
    req: &Request,
    stdin: &mut Stdin<'_>,
) -> Result<(String, Loaded), Failure> {
    let implicit = req.files.is_empty();
    match source {
        Source::Stdin => {
            let format = req.kind.unwrap_or(Format::Json);
            let Some(read) = stdin.as_mut() else {
                return Err(Failure::usage(if implicit {
                    "no input: name a FILE, or pipe a document into aless"
                } else {
                    "standard input is a terminal: pipe a document into aless, or name a FILE"
                }));
            };
            let bytes = load::read_within(read, req.max_size).map_err(|e| {
                Failure::load("-", format, &e.with_origin("(stdin)")).limited(None, req)
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
            let loaded = load::load_str_within(text, format, req.limits()).map_err(|e| {
                Failure::load("-", format, &e.with_origin("(stdin)")).limited(None, req)
            })?;
            Ok(("-".to_string(), loaded))
        }
        Source::File(path) => {
            let name = path.display().to_string();
            if path.is_dir() {
                return Err(Failure::usage(format!(
                    "{name} is a directory: aless reads files (list a directory with ls or find)"
                )));
            }
            let format = req.kind.unwrap_or_else(|| Format::detect(path));
            let loaded = load::load_path_within(path, req.kind, req.limits_for(source))
                .map_err(|e| Failure::load(&name, format, &e).limited(file_size(path), req))?;
            Ok((name, loaded))
        }
    }
}

/// A regular file's size; `None` for anything else (a FIFO, a device).
fn file_size(path: &Path) -> Option<u64> {
    std::fs::metadata(path)
        .ok()
        .filter(|m| m.is_file())
        .map(|m| m.len())
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
/// node has a known position. A position outside the text is the caller's
/// to refuse ([`outside`]); what this answers for it is its `nearest`.
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

/// Why a source position is outside the text, when it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outside {
    /// The line is past the last: the text has `lines` lines.
    Line { lines: usize },
    /// The column is past the end of its line, which has `len` characters.
    Col { col: u32, len: usize },
}

/// Whether a source position is outside `text`, and why when it is;
/// `None` when it is inside. Lines and columns count from 1, columns in
/// characters, as every position does. The lines are those of
/// [`load::line_count`]: a line's text excludes its terminator (LF or
/// CRLF), and a trailing terminator starts no line, so `"a\n"` has one. A
/// CR that no LF follows ends no line and is a character of its line, as
/// it is to the grammars, whose rows end at LF alone. A column is inside
/// its line when it is at most the line's length, so an empty line has no
/// column inside it. The lines are read up to the one asked for, never
/// collected: the text can be as long as `--max-size` allows.
pub fn outside(text: &str, line: u32, col: Option<u32>) -> Option<Outside> {
    let past = || Outside::Line {
        lines: load::line_count(text),
    };
    let Some(index) = (line as usize).checked_sub(1) else {
        return Some(past());
    };
    let mut pieces = text.split('\n').peekable();
    let Some(piece) = pieces.nth(index) else {
        return Some(past());
    };
    let text = match pieces.peek() {
        // Followed by an LF: the line's text, less the CR of a CRLF.
        Some(_) => piece.strip_suffix('\r').unwrap_or(piece),
        // After the last LF: a line only when there is text, whole.
        None if piece.is_empty() => return Some(past()),
        None => piece,
    };
    let len = text.chars().count();
    match col {
        Some(col) if col as usize > len => Some(Outside::Col { col, len }),
        _ => None,
    }
}

/// The node an operation starts at. `text` is what the document was parsed
/// from, which says whether a position is inside it.
fn select(
    doc: &Doc,
    text: &str,
    start: &Start,
    file: &str,
    format: Format,
) -> Result<NodeId, Failure> {
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
            // What a position inside the text answers; for one outside
            // it, what it would have answered: the error's `nearest`.
            let near = node_at(doc, *line, *col);
            if let Some(why) = outside(text, *line, *col) {
                let counted = |n: usize, what: &str| match n {
                    1 => format!("1 {what}"),
                    n => format!("{n} {what}s"),
                };
                let message = match why {
                    Outside::Line { lines: 0 } => format!("no line {line} in {file}: it is empty"),
                    Outside::Line { lines } => {
                        format!(
                            "no line {line} in {file}: it has {}",
                            counted(lines, "line")
                        )
                    }
                    Outside::Col { col, len: 0 } => {
                        format!("no column {col} on line {line} of {file}: the line is empty")
                    }
                    Outside::Col { col, len } => format!(
                        "no column {col} on line {line} of {file}: the line has {}",
                        counted(len, "character")
                    ),
                };
                return Err(Failure::not_found(
                    doc,
                    file,
                    format,
                    ("at", &at),
                    near.unwrap_or(NO_NODE),
                    message,
                ));
            }
            near.ok_or_else(|| {
                let message = format!("no node in {file} has a known source position");
                Failure::not_found(doc, file, format, ("at", &at), NO_NODE, message)
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

/// The kind of the value a source event begins, in the same words.
pub fn kind_of_event(ev: &JsonEvent<'_>) -> &'static str {
    match ev {
        JsonEvent::ObjectStart | JsonEvent::ObjectEnd => "object",
        JsonEvent::ArrayStart | JsonEvent::ArrayEnd => "array",
        JsonEvent::String(_) => "string",
        JsonEvent::Number(_) => "number",
        JsonEvent::Bool(_) => "boolean",
        JsonEvent::Null => "null",
        JsonEvent::Key(_) | JsonEvent::End => "none",
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

    /// A grammar the command line gave could not be registered. One that
    /// does not compile is the command's mistake: `{"kind": "usage",
    /// "message": "--grammar NAME: <the compiler's message>", "grammar":
    /// NAME, "file": FILE}`, `file` left out for `--grammar-expr`, status
    /// [`status::USAGE`]. A grammar file that cannot be read is the usual
    /// `io` ([`status::IO`]), one over `--max-size` `too_large`
    /// ([`status::TOO_LARGE`]), and a compile past `--timeout` `timeout`
    /// ([`status::TIMEOUT`]): each with the fields of that kind as
    /// [`load`](Self::load) and [`limited`](Self::limited) give them (the
    /// grammar file as `file`, `format` null) plus `grammar`.
    fn grammar(e: &GrammarError) -> Failure {
        let (kind, status) = if e.error.is_io() {
            ("io", status::IO)
        } else if e.error.is_too_large() {
            ("too_large", status::TOO_LARGE)
        } else if e.error.is_timeout() {
            ("timeout", status::TIMEOUT)
        } else {
            ("usage", status::USAGE)
        };
        let mut error = Map::new();
        error.insert("kind".into(), kind.into());
        error.insert("message".into(), e.to_string().into());
        error.insert("grammar".into(), e.grammar.clone().into());
        if let Some(file) = &e.file {
            error.insert("file".into(), file.clone().into());
        }
        if kind == "usage" {
            return Failure { status, error };
        }
        error.entry("file").or_insert(Value::Null);
        error.insert("format".into(), Value::Null);
        error.insert("code".into(), e.error.code.clone().into());
        error.insert("line".into(), Value::Null);
        error.insert("col".into(), Value::Null);
        error.insert(
            "hint".into(),
            if e.error.hint.is_empty() {
                Value::Null
            } else {
                e.error.hint.as_ref().into()
            },
        );
        error.insert("source_line".into(), Value::Null);
        error.insert("report".into(), e.error.plain_report().into());
        if status == status::TOO_LARGE {
            error.insert("size".into(), e.size.map_or(Value::Null, Value::from));
            error.insert(
                "limit".into(),
                e.limits.max_size.map_or(Value::Null, Value::from),
            );
        }
        if status == status::TIMEOUT {
            let secs = e
                .limits
                .timeout
                .map_or(Value::Null, |t| t.as_secs_f64().into());
            error.insert("seconds".into(), secs);
        }
        Failure { status, error }
    }

    /// An input that could not be read (`"kind": "io"`) or parsed
    /// (`"parse"`), with every field of the engine's report; those that do
    /// not apply are `null`.
    fn load(file: &str, format: Format, e: &LoadError) -> Failure {
        let (kind, status) = if e.is_io() {
            ("io", status::IO)
        } else if e.is_too_large() {
            ("too_large", status::TOO_LARGE)
        } else if e.is_timeout() {
            ("timeout", status::TIMEOUT)
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

    /// Say which limit an input broke. A `too_large` error gives how large
    /// the input is (`size`, `null` for a stream, which is read no further
    /// than the limit) and the limit (`limit`), both in bytes; a `timeout`
    /// error gives the time limit in `seconds`.
    fn limited(mut self, size: Option<u64>, req: &Request) -> Failure {
        if self.status == status::TOO_LARGE {
            let limit = req.max_size.map_or(Value::Null, Value::from);
            self.error
                .insert("size".into(), size.map_or(Value::Null, Value::from));
            self.error.insert("limit".into(), limit);
        }
        if self.status == status::TIMEOUT {
            let secs = req.timeout.map_or(Value::Null, |t| t.as_secs_f64().into());
            self.error.insert("seconds".into(), secs);
        }
        self
    }

    /// `--path` or `--at` named nothing: `{"kind": "not_found", "file",
    /// "format", "path" or "at", "message", "nearest", "keys"}`, where
    /// `nearest` is the entry of `near`, the deepest node the path did
    /// reach or the node a position outside the text would have answered
    /// inside it (`null` for [`NO_NODE`]), and `keys` the first of that
    /// node's keys when it is an object.
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

    /// [`Failure::not_found`] for a streamed run, which has no tree to
    /// take the nearest node's entry from: the entry is built from what
    /// the stream showed, with no source position.
    fn not_found_streamed(
        file: &str,
        format: Format,
        path: &str,
        message: String,
        near: &Nearest,
    ) -> Failure {
        let mut error = Map::new();
        error.insert("kind".into(), "not_found".into());
        error.insert("file".into(), file.into());
        error.insert("format".into(), format.name().into());
        error.insert("path".into(), path.into());
        error.insert("message".into(), message.into());
        let mut entry = Map::new();
        entry.insert("path".into(), near.path.clone().into());
        entry.insert("kind".into(), near.kind.into());
        entry.insert("line".into(), Value::Null);
        entry.insert("col".into(), Value::Null);
        match (&near.length, &near.value) {
            (Some(n), _) => {
                entry.insert("length".into(), (*n).into());
            }
            (None, Some(v)) => {
                entry.insert("value".into(), v.clone());
            }
            (None, None) => {}
        }
        error.insert("nearest".into(), Value::Object(entry));
        error.insert(
            "keys".into(),
            near.keys.as_ref().map_or(Value::Null, |k| {
                Value::Array(k.iter().map(|k| k.as_str().into()).collect())
            }),
        );
        Failure {
            status: status::NOT_FOUND,
            error,
        }
    }

    /// The program file of `--alchemy` could not be read (`"kind": "io"`)
    /// or is over `--max-size` (`"too_large"`, worded for the program:
    /// [`LoadError::program_too_large`]): the fields of that kind as
    /// [`load`](Self::load) and [`limited`](Self::limited) give them, with
    /// the program file as `file` and `format` null, since a program has
    /// no format.
    fn program_read(file: &str, e: &LoadError) -> Failure {
        let mut f = Failure::load(file, Format::Text, e);
        f.error.insert("format".into(), Value::Null);
        f
    }

    /// A failure of the program's own (`"kind": "alchemy"`: it does not
    /// parse, does not type check, uses a stream twice, or cannot be
    /// shown to stream; status [`status::USAGE`], as for a `--grammar`
    /// that does not compile), or one the program's build met (a
    /// transducer limit while the plan was built), or the program raised
    /// at a position of its own (`fail`), that is not the language's
    /// (`"kind": "transduce"`, the status following the code): the
    /// `code`, its `message` (a finer code leads the language's), `file`
    /// the program's, `format` null, `line` and `col` in the program when
    /// the failure has them, `path` and `limit` when it has those, and
    /// `output`. A failure met while the program ran adds `input`, the
    /// document's name; whether one met then is the program's is read
    /// from where it came from ([`ExportError::Program`]), its code and
    /// its position together ([`alchemy::is_placed`]), since
    /// `STREAMABILITY_UNKNOWN` is the source's code too and a `fail`'s
    /// `INPUT_INVALID` the transducer's.
    fn alchemy(file: &str, fail: &Fail) -> Failure {
        let programs = alchemy::is_programs(fail.code);
        let status = if programs {
            status::USAGE
        } else {
            match fail.code {
                Code::ResourceLimitExceeded => status::TOO_LARGE,
                Code::OutputFailed => status::IO,
                Code::Aborted => status::TIMEOUT,
                _ => status::PARSE,
            }
        };
        let mut error = Map::new();
        error.insert(
            "kind".into(),
            if programs { "alchemy" } else { "transduce" }.into(),
        );
        error.insert("file".into(), file.into());
        error.insert("format".into(), Value::Null);
        let Value::Object(detail) = fail.to_json() else {
            unreachable!("a failure is an object")
        };
        for (k, v) in detail {
            let k = if k == "row" { "line".to_string() } else { k };
            error.insert(k, v);
        }
        Failure { status, error }
    }

    /// A transducer stage failed (`"kind": "transduce"`): the transducer's
    /// `code`, its `message`, the `file` and `format`, then `path`,
    /// `limit` (`{name, value}`), `line` and `col` when the failure has
    /// them, and `output`, `"partial"` when some of the result had been
    /// written before the failure, else `"none"`. The status follows the
    /// code (see [`status`]).
    fn transduce(file: &str, format: Format, fail: &Fail) -> Failure {
        let status = match fail.code {
            Code::ResourceLimitExceeded => status::TOO_LARGE,
            Code::OutputFailed => status::IO,
            Code::Aborted => status::TIMEOUT,
            _ => status::PARSE,
        };
        let mut error = Map::new();
        error.insert("kind".into(), "transduce".into());
        error.insert("file".into(), file.into());
        error.insert("format".into(), format.name().into());
        let Value::Object(detail) = fail.to_json() else {
            unreachable!("a failure is an object")
        };
        for (k, v) in detail {
            // The transducer's `row` is aless's `line`.
            let k = if k == "row" { "line".to_string() } else { k };
            error.insert(k, v);
        }
        // The one limit of the transducer's that an option sets.
        if fail
            .limit
            .as_ref()
            .is_some_and(|l| l.name == "max_output_bytes")
        {
            error.insert(
                "hint".into(),
                "Pass --max-output with a larger size to let the program write more, or \
                 --max-output 0 for no limit."
                    .into(),
            );
        }
        Failure { status, error }
    }
}

/// A grammar the command line gave (`--grammar`, `--grammar-expr`) could
/// not be registered, as the text `main` prints on standard error and the
/// status to exit with: `{"error": {…}}` in the shape [`Failure::grammar`]
/// gives.
pub fn grammar_failure(e: &GrammarError, compact: bool) -> (String, i32) {
    let f = Failure::grammar(e);
    (render(&json!({ "error": f.error }), compact), f.status)
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
        let mut input = src.as_bytes();
        run(req, Some(&mut input))
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
        // Past the end of the text, which `select` refuses: its `nearest`.
        assert_eq!(at(99, None).as_deref(), Some(r#".c."odd key""#));
        // Nested nodes starting at the same place: the innermost.
        let y = load::parse("items:\n  - name: web\n    port: 80\n", Format::Yaml).unwrap();
        let path = node_at(&y, 2, None).map(|i| fmt::path_jq(&y.path(i)));
        assert_eq!(path.as_deref(), Some(".items[0].name"));
        let none = Doc::from_lines(&[]);
        assert_eq!(node_at(&none, 1, None), None);
    }

    #[test]
    fn a_position_is_inside_the_text_or_not() {
        use Outside::{Col, Line};
        // A line's text excludes its terminator, LF or CRLF; a trailing
        // terminator starts no line; an empty line has no column inside
        // it; columns count characters.
        assert_eq!(outside("a\n", 1, None), None);
        assert_eq!(outside("a\n", 1, Some(1)), None);
        assert_eq!(outside("a\n", 1, Some(2)), Some(Col { col: 2, len: 1 }));
        assert_eq!(outside("a\n", 2, None), Some(Line { lines: 1 }));
        assert_eq!(
            outside("a", 1, Some(1)),
            None,
            "no terminator: a line still"
        );
        assert_eq!(outside("a", 2, None), Some(Line { lines: 1 }));
        assert_eq!(outside("ab\r\ncd", 1, Some(2)), None);
        assert_eq!(
            outside("ab\r\ncd", 1, Some(3)),
            Some(Col { col: 3, len: 2 }),
            "the CR is not text"
        );
        assert_eq!(outside("ab\r\ncd", 2, Some(2)), None);
        assert_eq!(
            outside("ab\r\ncd", 2, Some(3)),
            Some(Col { col: 3, len: 2 })
        );
        assert_eq!(outside("ab\r\ncd", 3, None), Some(Line { lines: 2 }));
        // A CR that no LF follows is a character of its line, as it is
        // to the grammars, whose rows end at LF alone.
        assert_eq!(outside("{\"a\":1}\r", 1, Some(8)), None);
        assert_eq!(
            outside("{\"a\":1}\r", 1, Some(9)),
            Some(Col { col: 9, len: 8 })
        );
        assert_eq!(outside("{\"a\":1}\r", 2, None), Some(Line { lines: 1 }));
        assert_eq!(outside("a\rb\n", 1, Some(3)), None, "a CR inside a line");
        assert_eq!(outside("a\r\n", 1, Some(2)), Some(Col { col: 2, len: 1 }));
        assert_eq!(outside("a\n\r", 2, Some(1)), None, "a last line of one CR");
        assert_eq!(outside("a\n\r\n", 2, None), None, "an empty second line");
        assert_eq!(outside("a\n\r\n", 2, Some(1)), Some(Col { col: 1, len: 0 }));
        assert_eq!(
            outside("a\n\nb\n", 2, None),
            None,
            "an empty line is a line"
        );
        assert_eq!(
            outside("a\n\nb\n", 2, Some(1)),
            Some(Col { col: 1, len: 0 })
        );
        assert_eq!(outside("", 1, None), Some(Line { lines: 0 }));
        assert_eq!(outside("\n", 1, None), None, "one empty line");
        assert_eq!(outside("\n", 2, None), Some(Line { lines: 1 }));
        assert_eq!(
            outside("é1\n", 1, Some(2)),
            None,
            "columns count characters"
        );
        assert_eq!(outside("é1\n", 1, Some(3)), Some(Col { col: 3, len: 2 }));
    }

    #[test]
    fn positions_outside_the_text_are_not_found() {
        // DOC's lines: `{`; `  "a": 1,` (9 characters); `  "b": [true,
        // null, "x"],`; `  "c": {"d": 2.5, "odd key": "v"}` (33); `}`.
        let at = |op: Op, line, col, src: &str| {
            let mut r = req(op);
            r.start = Start::At(line, col);
            with_stdin(&r, src)
        };
        let path = |out: &Output| json_of(&out.stdout)["path"].clone();
        let error = |out: &Output| json_of(&out.stderr)["error"].clone();
        // The last position inside the text is the closing brace at 5:1,
        // and answers the node before it.
        let out = at(Op::Where, 5, Some(1), DOC);
        assert_eq!(out.status, status::OK, "{}", out.stderr);
        assert_eq!(path(&out), json!(r#".c."odd key""#));
        // Between two nodes on a line, the one before; at the end of a
        // line, the last on it.
        assert_eq!(path(&at(Op::Where, 4, Some(17), DOC)), json!(".c.d"));
        assert_eq!(
            path(&at(Op::Where, 4, Some(33), DOC)),
            json!(r#".c."odd key""#)
        );
        assert_eq!(path(&at(Op::Where, 2, Some(9), DOC)), json!(".a"));
        // A line past the last.
        let out = at(Op::Where, 6, None, DOC);
        assert_eq!(out.status, status::NOT_FOUND);
        assert!(out.stdout.is_empty());
        let e = error(&out);
        assert_eq!(e["kind"], json!("not_found"));
        assert_eq!(e["file"], json!("-"));
        assert_eq!(e["format"], json!("json"));
        assert_eq!(e["at"], json!("6"));
        assert_eq!(e["message"], json!("no line 6 in -: it has 5 lines"));
        assert_eq!(e["nearest"]["path"], json!(r#".c."odd key""#));
        assert_eq!(e["nearest"]["line"], json!(4));
        assert_eq!(e["keys"], Value::Null);
        // A column past the end of its line.
        for (line, col, nearest, why) in [
            (5, 2, r#".c."odd key""#, "the line has 1 character"),
            (2, 10, ".a", "the line has 9 characters"),
            (4, 34, r#".c."odd key""#, "the line has 33 characters"),
        ] {
            let out = at(Op::Where, line, Some(col), DOC);
            assert_eq!(out.status, status::NOT_FOUND, "{line}:{col}");
            let e = error(&out);
            assert_eq!(e["at"], json!(format!("{line}:{col}")));
            assert_eq!(
                e["message"],
                json!(format!("no column {col} on line {line} of -: {why}"))
            );
            assert_eq!(e["nearest"]["path"], json!(nearest));
        }
        // --json, --paths and --find answer the same way.
        for op in [Op::Json, Op::Paths, Op::Find("a".into())] {
            let flag = op.flag();
            let out = at(op, 6, None, DOC);
            assert_eq!(out.status, status::NOT_FOUND, "{flag}");
            assert_eq!(error(&out)["nearest"]["path"], json!(r#".c."odd key""#));
        }
        // The nearest node's keys, as for a path: the `{}` at 1:5 is an
        // object with none.
        let out = at(Op::Where, 1, Some(8), "[1, {}]\n");
        assert_eq!(out.status, status::NOT_FOUND);
        let e = error(&out);
        assert_eq!(e["nearest"]["path"], json!("[1]"));
        assert_eq!(e["keys"], json!([]));
        assert_eq!(path(&at(Op::Where, 1, Some(7), "[1, {}]\n")), json!("[1]"));
        // CRLF: the CR is not text, and a trailing CRLF starts no line.
        let crlf = "{\r\n  \"a\": 1\r\n}\r\n";
        assert_eq!(path(&at(Op::Where, 2, Some(8), crlf)), json!(".a"));
        let out = at(Op::Where, 2, Some(9), crlf);
        assert_eq!(out.status, status::NOT_FOUND);
        assert_eq!(
            error(&out)["message"],
            json!("no column 9 on line 2 of -: the line has 8 characters")
        );
        assert_eq!(path(&at(Op::Where, 3, Some(1), crlf)), json!(".a"));
        let out = at(Op::Where, 4, None, crlf);
        assert_eq!(out.status, status::NOT_FOUND);
        assert_eq!(
            error(&out)["message"],
            json!("no line 4 in -: it has 3 lines")
        );
        // A last line without a terminator is a line.
        let bare = "{\n  \"a\": 1\n}";
        assert_eq!(path(&at(Op::Where, 3, Some(1), bare)), json!(".a"));
        assert_eq!(at(Op::Where, 3, Some(2), bare).status, status::NOT_FOUND);
        let out = at(Op::Where, 4, None, bare);
        assert_eq!(out.status, status::NOT_FOUND);
        assert_eq!(error(&out)["nearest"]["path"], json!(".a"));
        // An empty text has no line, and no node with a position: nearest
        // is null.
        let mut r = req(Op::Where);
        r.files = vec!["-".into()];
        r.kind = Some(Format::Text);
        r.start = Start::At(1, None);
        let out = with_stdin(&r, "");
        assert_eq!(out.status, status::NOT_FOUND);
        let e = error(&out);
        assert_eq!(e["message"], json!("no line 1 in -: it is empty"));
        assert_eq!(e["nearest"], Value::Null);
        assert_eq!(e["keys"], Value::Null);
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
        struct Untouchable;
        impl Read for Untouchable {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                panic!("read input for a bad pattern")
            }
        }
        let bad = run(&r, Some(&mut Untouchable));
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
    fn inputs_over_the_size_limit_are_refused() {
        let mut r = req(Op::Json);
        r.max_size = Some(10);
        // Standard input: read no further than the limit, so no size.
        let out = with_stdin(&r, DOC);
        assert_eq!(out.status, status::TOO_LARGE);
        assert_eq!(out.stdout, "");
        let e = &json_of(&out.stderr)["error"];
        assert_eq!(e["kind"], json!("too_large"));
        assert_eq!(e["code"], json!("too_large"));
        assert_eq!(e["file"], json!("-"));
        assert_eq!(e["size"], Value::Null);
        assert_eq!(e["limit"], json!(10));
        assert!(e["hint"].as_str().unwrap().contains("--max-size"), "{e}");
        // A file: refused before it is read, with its size.
        let dir = std::env::temp_dir().join(format!("aless-limit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("doc.json");
        std::fs::write(&file, DOC).unwrap();
        r.files = vec![file.clone()];
        let out = run(&r, None);
        assert_eq!(out.status, status::TOO_LARGE);
        let e = &json_of(&out.stderr)["error"];
        assert_eq!(e["size"], json!(DOC.len()));
        assert_eq!(e["format"], json!("json"));
        // --check reports it with the rest.
        r.op = Op::Check;
        let v = json_of(&run(&r, None).stdout);
        assert_eq!(v["files"][0]["error"]["kind"], json!("too_large"));
        // No limit, or a large enough one, reads it.
        r.op = Op::Json;
        r.max_size = None;
        assert_eq!(run(&r, None).status, status::OK);
        r.max_size = Some(DOC.len() as u64);
        assert_eq!(run(&r, None).status, status::OK);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_parse_past_its_timeout_fails_with_status_6() {
        let toml: String = (0..2_000)
            .map(|i| format!("[[item]]\nid = {i}\nname = \"item {i}\"\n\n"))
            .collect();
        let mut r = req(Op::Paths);
        r.kind = Some(Format::Toml);
        r.timeout = Some(Duration::from_millis(1));
        let out = with_stdin(&r, &toml);
        assert_eq!(out.status, status::TIMEOUT, "{}", out.stderr);
        assert_eq!(out.stdout, "");
        let e = &json_of(&out.stderr)["error"];
        assert_eq!(e["kind"], json!("timeout"));
        assert_eq!(e["code"], json!("timeout"));
        assert_eq!(e["format"], json!("toml"));
        assert_eq!(e["seconds"], json!(0.001));
        assert!(e["line"].as_u64().unwrap() > 0, "{e}");
        // No limit: the same request succeeds.
        r.timeout = None;
        assert_eq!(with_stdin(&r, "[[item]]\nid = 1\n").status, status::OK);
    }

    /// `--timeout` ends a run whose standard input never sends a byte, or
    /// stops sending and never closes, in every mode, at the deadline that
    /// runs from the run's start (aless#16).
    #[test]
    fn a_run_on_quiet_stdin_stops_at_its_deadline() {
        let program = ProgramArg::Expr("def export [input] input".into());
        let modes = [
            (Op::Json, None),
            (Op::Paths, Some(Format::Yaml)),
            (Op::Render(Renderer::Json), None),
            (Op::Render(Renderer::Csv), Some(Format::Jsonl)),
            (Op::Render(Renderer::Csv), Some(Format::Csv)),
            (
                Op::Alchemy {
                    program,
                    render: None,
                    explain: false,
                },
                Some(Format::Jsonl),
            ),
        ];
        for (op, kind) in modes {
            for first in [&b""[..], b"{\"a\": 1}\n"] {
                let mut r = req(op.clone());
                r.kind = kind;
                r.timeout = Some(Duration::from_millis(300));
                let started = Instant::now();
                r.started = Some(started);
                let quiet = load::quiet(first);
                let mut input =
                    load::DeadlineReader::new(quiet, started, Duration::from_millis(300)).unwrap();
                let out = run(&r, Some(&mut input));
                let took = started.elapsed();
                let what = format!("{:?} {kind:?} after {} bytes", r.op, first.len());
                assert_eq!(out.status, status::TIMEOUT, "{what}: {}", out.stderr);
                let e = &json_of(&out.stderr)["error"];
                assert_eq!(e["kind"], json!("timeout"), "{what}");
                assert_eq!(
                    e["message"],
                    json!("timeout: the input was still being read after 0.3 s"),
                    "{what}"
                );
                assert_eq!(e["seconds"], json!(0.3), "{what}");
                assert!(took < Duration::from_secs(10), "{what}: {took:?}");
            }
        }
    }

    /// The time limit on standard input runs from the start of the run,
    /// so that waiting on it counts; on a file it runs from the start of
    /// its parse, so that every file `--check` reads gets the whole of it.
    #[test]
    fn only_standard_input_is_timed_from_the_start_of_the_run() {
        let mut r = req(Op::Check);
        r.timeout = Some(Duration::from_secs(5));
        let started = Instant::now();
        r.started = Some(started);
        assert_eq!(r.limits_for(&Source::Stdin).started, Some(started));
        let file = Source::File(PathBuf::from("a.json"));
        assert_eq!(r.limits_for(&file).started, None);
        assert_eq!(r.limits_for(&file).timeout, r.timeout);
    }

    /// A program that writes without end from a small input is stopped at
    /// `--max-output` (by default [`load::DEFAULT_MAX_OUTPUT`]), as the
    /// transducer's `RESOURCE_LIMIT_EXCEEDED` naming `max_output_bytes`,
    /// with status 5; a render, whose output follows its input, is not
    /// held to it (aless#18).
    #[test]
    fn a_programs_output_is_bounded() {
        let mut doubling = String::from("def a0 \"0123456789\"\n");
        for i in 1..=40 {
            doubling.push_str(&format!("def a{i} (concat a{} a{})\n", i - 1, i - 1));
        }
        doubling.push_str("def export [input] (concat a40 (json input))\n");
        let program = |text: &str| Op::Alchemy {
            program: ProgramArg::Expr(text.into()),
            render: None,
            explain: false,
        };
        assert_eq!(
            req(Op::Json).max_output,
            Some(load::DEFAULT_MAX_OUTPUT),
            "bounded unless asked otherwise"
        );
        let mut r = req(program(&doubling));
        r.max_output = Some(4096);
        let out = with_stdin(&r, "{\"a\": 1}");
        assert_eq!(out.status, status::TOO_LARGE, "{}", out.stderr);
        let e = &json_of(&out.stderr)["error"];
        assert_eq!(e["kind"], json!("transduce"));
        assert_eq!(e["code"], json!("RESOURCE_LIMIT_EXCEEDED"));
        assert_eq!(
            e["limit"],
            json!({"name": "max_output_bytes", "value": 4096})
        );
        assert!(
            e["hint"].as_str().unwrap().contains("--max-output 0"),
            "{e}"
        );
        assert!(out.stdout.len() <= 4096, "{}", out.stdout.len());
        // A program within the bound runs as ever.
        let echo = req(program("def export [input] input"));
        assert_eq!(with_stdin(&echo, "[1, 2]").stdout, "[1,2]\n");
        // A render is not a program's output.
        let mut render = req(Op::Render(Renderer::Json));
        render.max_output = Some(4);
        render.compact = true;
        let out = with_stdin(&render, "[1, 2, 3, 4, 5]");
        assert_eq!(out.status, status::OK, "{}", out.stderr);
        assert_eq!(out.stdout, "[1,2,3,4,5]\n");
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

    const RECORDS: &str = r#"{"rows": [{"a": 1.50, "b": "x"}, {"b": "y", "c": null}], "n": 2}"#;

    #[test]
    fn render_streams_csv_and_json() {
        let mut r = req(Op::Render(Renderer::Csv));
        r.start = Start::Path(".rows".into());
        let out = with_stdin(&r, RECORDS);
        assert_eq!(out.status, status::OK, "{}", out.stderr);
        assert_eq!(out.stderr, "");
        assert_eq!(
            out.stdout,
            "\"a\",\"b\"\r\n\"1.50\",\"x\"\r\n\"\",\"y\"\r\n"
        );
        // JSON: the value at the start, as `--json` prints it but streamed.
        r.op = Op::Render(Renderer::Json);
        r.compact = true;
        let out = with_stdin(&r, RECORDS);
        assert_eq!(
            out.stdout,
            "[{\"a\":1.50,\"b\":\"x\"},{\"b\":\"y\",\"c\":null}]\n"
        );
        r.start = Start::Root;
        r.compact = false;
        let out = with_stdin(&r, r#"{"a": [1]}"#);
        assert_eq!(out.stdout, "{\n  \"a\": [\n    1\n  ]\n}\n");
        // Standard input in a line-delimited format is read a record at a
        // time, and -k says which.
        let mut lines = req(Op::Render(Renderer::Csv));
        lines.kind = Some(Format::Jsonl);
        let out = with_stdin(&lines, "{\"id\": 1}\n{\"id\": 2, \"x\": true}\n");
        assert_eq!(out.status, status::OK, "{}", out.stderr);
        assert_eq!(out.stdout, "\"id\"\r\n\"1\"\r\n\"2\"\r\n");
        lines.kind = Some(Format::Csv);
        let out = with_stdin(&lines, "a,b\n1,2\n");
        assert_eq!(out.stdout, "\"a\",\"b\"\r\n\"1\",\"2\"\r\n");
        // A grammar that refuses to stream a document part-way (jsonic's
        // implicit list with a container first) is run again from the
        // whole value, and the answer is --json's.
        let mut jsonic = req(Op::Render(Renderer::Json));
        jsonic.kind = Some(Format::Jsonic);
        jsonic.compact = true;
        let out = with_stdin(&jsonic, "{a:1}\n{b:2}\n");
        assert_eq!(out.status, status::OK, "{}", out.stderr);
        assert_eq!(out.stdout, "[{\"a\":1},{\"b\":2}]\n");
        // The size limit does not apply to a line-by-line read.
        lines.max_size = Some(4);
        let out = with_stdin(&lines, "a,b\n1,2\n");
        assert_eq!(out.status, status::OK, "{}", out.stderr);
        // It does to the rest.
        r.max_size = Some(4);
        assert_eq!(with_stdin(&r, RECORDS).status, status::TOO_LARGE);
        assert_eq!(Op::Render(Renderer::Csv).flag(), "--render");
    }

    #[test]
    fn programs_run_over_stdin_and_report_their_own_failures() {
        let echo = || Op::Alchemy {
            program: ProgramArg::Expr("def export [input] input".into()),
            render: None,
            explain: false,
        };
        assert_eq!(echo().flag(), "--alchemy");
        let mut r = req(echo());
        r.compact = true;
        let out = with_stdin(&r, RECORDS);
        assert_eq!(out.status, status::OK, "{}", out.stderr);
        assert_eq!(out.stderr, "");
        assert_eq!(
            out.stdout,
            "{\"rows\":[{\"a\":1.50,\"b\":\"x\"},{\"b\":\"y\",\"c\":null}],\"n\":2}\n"
        );
        // A table bound by the document's own metadata, as CSV and as
        // JSON records keyed by the column labels.
        let doc = r#"{"cols": [{"title": "A", "path": ["a"]}, {"title": "B", "path": ["b"]}], "rows": [{"a": 1.50, "b": "x"}, {"a": 2, "b": "y"}]}"#;
        let table = "def col [c]\n  record\n    entry :label (get \"title\" c)\n    entry :source (as-path (get \"path\" c))\ndef export [input]\n  table-from-json (record (entry :columns (path \"cols\")) (entry :rows (path \"rows\" each-index)) (entry :column col)) input\n";
        let mut t = req(Op::Alchemy {
            program: ProgramArg::Expr(table.into()),
            render: None,
            explain: false,
        });
        let out = with_stdin(&t, doc);
        assert_eq!(out.status, status::OK, "{}", out.stderr);
        assert_eq!(
            out.stdout,
            "\"A\",\"B\"\r\n\"1.50\",\"x\"\r\n\"2\",\"y\"\r\n"
        );
        t.op = Op::Alchemy {
            program: ProgramArg::Expr(table.into()),
            render: Some(Renderer::Json),
            explain: false,
        };
        let out = with_stdin(&t, doc);
        assert_eq!(out.status, status::OK, "{}", out.stderr);
        assert_eq!(
            json_of(&out.stdout),
            json!([{"A": 1.50, "B": "x"}, {"A": 2, "B": "y"}])
        );
        // The plan, and no input read.
        let mut x = req(Op::Alchemy {
            program: ProgramArg::Expr("def export [input] input".into()),
            render: None,
            explain: true,
        });
        x.compact = true;
        let out = with_stdin(&x, RECORDS);
        assert_eq!(out.status, status::OK, "{}", out.stderr);
        let plan = json_of(&out.stdout);
        assert_eq!(plan["entry"], json!("export"));
        assert_eq!(plan["output"], json!("JsonEvents/1"));
        // The program's own failure: its code, its file, its position.
        let bad = req(Op::Alchemy {
            program: ProgramArg::Expr("def export [input] (nope input)".into()),
            render: None,
            explain: false,
        });
        let out = with_stdin(&bad, RECORDS);
        assert_eq!(out.status, status::USAGE);
        assert_eq!(out.stdout, "");
        let e = &json_of(&out.stderr)["error"];
        assert_eq!(e["kind"], json!("alchemy"));
        assert_eq!(e["file"], json!(alchemy::EXPR_NAME));
        assert_eq!(e["format"], json!(null));
        assert_eq!(e["code"], json!("DSL_TYPE_ERROR"));
        assert_eq!((e["line"].clone(), e["col"].clone()), (json!(1), json!(21)));
        assert_eq!(e["output"], json!("none"));
        // The input's failure keeps the transducer's shape.
        let out = with_stdin(&r, "[{\"a\": 1},\n {\"b\": }]");
        assert_eq!(out.status, status::PARSE);
        let e = &json_of(&out.stderr)["error"];
        assert_eq!(e["kind"], json!("transduce"));
        assert_eq!(e["code"], json!("INPUT_INVALID"));
        assert_eq!(e["file"], json!("-"));
        assert_eq!(e["format"], json!("json"));
        assert_eq!((e["line"].clone(), e["col"].clone()), (json!(2), json!(8)));
        // A failure the program raises at a record (`fail`) keeps the
        // transducer's code and status, but is placed in the program: it
        // came from the program's sink with a position, and the events a
        // program reads carry no positions, so `line` and `col` are in
        // the program (the input here has one line), `file` is the
        // program's, `format` null, and `input` names the document.
        let failing = req(Op::Alchemy {
            program: ProgramArg::Expr(
                "def check [r]\n  match (get \"b\" r)\n    case \"y\" (fail \"no y\")\n    case _ \"ok\"\n\
                 def export [input]\n  join \"\\n\" (map check (select (path \"rows\" each-index) input))"
                    .into(),
            ),
            render: None,
            explain: false,
        });
        let out = with_stdin(&failing, RECORDS);
        assert_eq!(out.status, status::PARSE, "{}", out.stderr);
        assert_eq!(out.stdout, "");
        let e = &json_of(&out.stderr)["error"];
        assert_eq!(e["kind"], json!("transduce"));
        assert_eq!(e["code"], json!("INPUT_INVALID"));
        assert_eq!(e["message"], json!("no y"));
        assert_eq!(e["file"], json!(alchemy::EXPR_NAME));
        assert_eq!(e["format"], json!(null));
        assert_eq!((e["line"].clone(), e["col"].clone()), (json!(3), json!(14)));
        assert_eq!(e["input"], json!("-"));
        assert_eq!(e["output"], json!("none"));
        // A line-delimited format from standard input is read a record at
        // a time, so the size limit does not apply to it.
        let mut lines = req(echo());
        lines.kind = Some(Format::Jsonl);
        lines.max_size = Some(4);
        let out = with_stdin(&lines, "{\"id\": 1}\n{\"id\": 2}\n");
        assert_eq!(out.status, status::OK, "{}", out.stderr);
        assert_eq!(out.stdout, "[{\"id\":1},{\"id\":2}]\n");
        r.max_size = Some(4);
        assert_eq!(with_stdin(&r, RECORDS).status, status::TOO_LARGE);
    }

    /// A failure from the program's sink with neither a code of the
    /// language's nor a position is reported as the input's: a renderer's
    /// over the rows the program built (a record without a value for a
    /// bound column, as CSV: `MISSING_VALUE`), in an export's shape, `file`
    /// and `format` the document's, no `input`, `line` or `col`, status 1.
    #[test]
    fn a_renderers_failure_over_the_programs_rows_is_reported_as_the_inputs() {
        let table = "def api-binding\n  record\n    entry :columns (path \"meta\" \"fields\")\n    \
                     entry :rows (path \"records\" each-index)\n    entry :column (fn [f] (record \
                     (entry :label (get \"title\" f)) (entry :source (as-path (get \"path\" f)))))\n\
                     def export [input]\n  table-from-json api-binding input\n";
        let mut r = req(Op::Alchemy {
            program: ProgramArg::Expr(table.into()),
            render: None,
            explain: false,
        });
        r.compact = true;
        let doc = r#"{"meta": {"fields": [{"title": "Id", "path": ["id"]}, {"title": "Name", "path": ["name"]}]}, "records": [{"id": 1, "name": "a"}, {"id": 2}]}"#;
        let out = with_stdin(&r, doc);
        assert_eq!(out.status, status::PARSE, "{}", out.stderr);
        assert_eq!(out.stdout, "");
        let e = &json_of(&out.stderr)["error"];
        assert_eq!(e["kind"], json!("transduce"));
        assert_eq!(e["code"], json!("MISSING_VALUE"));
        assert_eq!(e["file"], json!("-"));
        assert_eq!(e["format"], json!("json"));
        assert_eq!(e["output"], json!("none"));
        assert!(e.get("input").is_none(), "{e}");
        assert!(e.get("line").is_none() && e.get("col").is_none(), "{e}");
        // As JSON records the same rows render, a missing value left out.
        let mut r = req(Op::Alchemy {
            program: ProgramArg::Expr(table.into()),
            render: Some(Renderer::Json),
            explain: false,
        });
        r.compact = true;
        let out = with_stdin(&r, doc);
        assert_eq!(out.status, status::OK, "{}", out.stderr);
        assert_eq!(out.stdout, "[{\"Id\":1,\"Name\":\"a\"},{\"Id\":2}]\n");
    }

    /// An `--alchemy` program file over `--max-size` is `too_large` with
    /// the shape, fields and status of an input's, worded for the program:
    /// the message names it, and the hint claims no memory per byte, since
    /// no parse of it is coming.
    #[test]
    fn an_oversize_program_file_is_too_large_worded_for_the_program() {
        let dir = std::env::temp_dir().join(format!("aless-program-size-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("p.alc");
        std::fs::write(&path, "def export [input] input\n").unwrap();
        let mut r = req(Op::Alchemy {
            program: ProgramArg::File(path.clone()),
            render: None,
            explain: false,
        });
        r.max_size = Some(10);
        let out = with_stdin(&r, RECORDS);
        assert_eq!(out.status, status::TOO_LARGE, "{}", out.stderr);
        assert_eq!(out.stdout, "");
        let e = &json_of(&out.stderr)["error"];
        assert_eq!(e["kind"], json!("too_large"));
        assert_eq!(e["code"], json!("too_large"));
        assert_eq!(e["file"], json!(path.display().to_string()));
        assert_eq!(e["format"], json!(null));
        assert_eq!(e["message"], json!("program is 25 B, over the 10 B limit"));
        assert_eq!(e["size"], json!(25));
        assert_eq!(e["limit"], json!(10));
        let hint = e["hint"].as_str().unwrap();
        assert!(!hint.contains("bytes of memory"), "{hint}");
        assert!(
            hint.starts_with("A program file over --max-size is not read."),
            "{hint}"
        );
        assert!(hint.contains("--max-size 0"), "{hint}");
        assert!(
            e["report"].as_str().unwrap().contains("program is 25 B"),
            "{e}"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// `STREAMABILITY_UNKNOWN` is a code of both sides: the program's (the
    /// checker's, the evaluator's `recursion`) and the source's (a verified
    /// grammar that refuses to stream a document part-way). Once output has
    /// left, the source's refusal cannot fall back, and it is the input's
    /// failure, in the transducer's shape with status 1, never the
    /// program's; the code alone does not tell them apart, where the
    /// failure came from does.
    #[test]
    fn a_grammars_refusal_to_stream_under_a_program_is_the_inputs() {
        let mut r = req(Op::Alchemy {
            program: ProgramArg::Expr("def export [input] (json input)".into()),
            render: None,
            explain: false,
        });
        r.kind = Some(Format::Jsonic);
        // A jsonic top-level implicit list whose first element is a
        // container: the first value is streamed as the root, and written
        // (the string is longer than the writer holds back), before the
        // grammar wraps it in a list.
        let long = "x".repeat(2 * tabnas_render::DEFAULT_BUDGET);
        let out = with_stdin(&r, &format!("{{a:'{long}'}}\n{{b:2}}\n"));
        assert_eq!(out.status, status::PARSE, "{}", out.stderr);
        assert!(out.stdout.starts_with("{\"a\":\"xxx"), "output had left");
        let e = &json_of(&out.stderr)["error"];
        assert_eq!(e["kind"], json!("transduce"));
        assert_eq!(e["code"], json!("STREAMABILITY_UNKNOWN"));
        assert_eq!(e["file"], json!("-"));
        assert_eq!(e["format"], json!("jsonic"));
        assert_eq!(e["output"], json!("partial"));
        assert!(e.get("input").is_none(), "{e}");
        // Before anything has left, the same refusal falls back to the
        // whole value, as under --render.
        let out = with_stdin(&r, "{a:1}\n{b:2}\n");
        assert_eq!(out.status, status::OK, "{}", out.stderr);
        assert_eq!(out.stdout, "[{\"a\":1},{\"b\":2}]\n");
        // The program's own STREAMABILITY_UNKNOWN met at run time (the
        // evaluator's `recursion`, on an item) is still the program's:
        // status 2, its position in the program, and the input's name.
        let w = req(Op::Alchemy {
            program: ProgramArg::Expr(
                "def w [f] (f f)\ndef export [input]\n  concat-map (fn [row] (let [x (w w)] \".\")) \
                 (select (path each-index) input)"
                    .into(),
            ),
            render: None,
            explain: false,
        });
        let out = with_stdin(&w, "[1, 2]");
        assert_eq!(out.status, status::USAGE, "{}", out.stderr);
        assert_eq!(out.stdout, "");
        let e = &json_of(&out.stderr)["error"];
        assert_eq!(e["kind"], json!("alchemy"));
        assert_eq!(e["code"], json!("STREAMABILITY_UNKNOWN"));
        assert!(
            e["message"].as_str().unwrap().starts_with("recursion: "),
            "{e}"
        );
        assert_eq!(e["file"], json!(alchemy::EXPR_NAME));
        assert_eq!(e["format"], json!(null));
        assert_eq!((e["line"].clone(), e["col"].clone()), (json!(1), json!(12)));
        assert_eq!(e["input"], json!("-"));
        assert_eq!(e["output"], json!("none"));
    }

    /// A program slow on one item works inside one of the parser's events,
    /// where no guard of aless's runs: `--timeout` reaches the program's
    /// own flag, in the incremental mode JSON runs in, and the run ends a
    /// moment after the deadline, status 6, with a `timeout` that says the
    /// run (the parse and the program) ran too long, not the parse alone,
    /// and carries no input position (`line` and `col` null, the report's
    /// `-->` naming the input alone): the program's span, which its
    /// evaluator stamps on the failure, is none, and the parse was not
    /// what stopped. The deadline is one the parse comfortably meets (a
    /// few milliseconds for three hundred numbers) and the item's cubic
    /// work does not (seconds), so the parser's guard cannot be what ends
    /// the run, on a slow runner too:
    /// without the program's flag the run would last the item's work.
    #[test]
    fn a_program_slow_on_one_item_stops_at_the_deadline_as_a_run_timeout() {
        let slow =
            "def slow [row]\n  let [v (as-vector row)]\n    let [w (map (fn [x] (map (fn [y] (map (fn [z] z) v)) \
                    v)) v)]\n      \".\"\ndef export [input]\n  concat-map slow (select (path \
                    each-index) input)\n";
        let mut r = req(Op::Alchemy {
            program: ProgramArg::Expr(slow.into()),
            render: None,
            explain: false,
        });
        r.timeout = Some(Duration::from_millis(1500));
        // One row whose cubic work takes seconds; its parse milliseconds.
        let row: Vec<String> = (0..300).map(|i| i.to_string()).collect();
        let doc = format!("[[{}]]", row.join(","));
        let started = std::time::Instant::now();
        let out = with_stdin(&r, &doc);
        assert!(
            started.elapsed() < Duration::from_secs(4),
            "the run went on {:?} past a 1.5 s deadline",
            started.elapsed()
        );
        assert_eq!(out.status, status::TIMEOUT, "{}", out.stderr);
        assert_eq!(out.stdout, "");
        let e = &json_of(&out.stderr)["error"];
        assert_eq!(e["kind"], json!("timeout"));
        assert_eq!(e["code"], json!("timeout"));
        assert_eq!(e["seconds"], json!(1.5));
        assert_eq!(e["output"], json!("none"));
        assert_eq!(e["file"], json!("-"));
        assert_eq!(e["format"], json!("json"));
        assert_eq!(
            e["message"],
            json!("timeout: the run (the parse and the program) ran longer than 1.5 s")
        );
        let hint = e["hint"].as_str().unwrap();
        assert!(hint.contains("the program's work on an item"), "{e}");
        assert!(!hint.contains("got this far"), "{e}");
        // No input position: the program's, not the parse's, stop.
        assert_eq!(e["line"], Value::Null, "{e}");
        assert_eq!(e["col"], Value::Null, "{e}");
        let report = e["report"].as_str().unwrap();
        assert!(
            report.contains("--> (stdin)\n") && !report.contains("--> (stdin):"),
            "{report}"
        );
        // A parse stopped at the deadline under --render still names the
        // parse, as it did.
        let mut slow_parse = req(Op::Render(Renderer::Json));
        slow_parse.kind = Some(Format::Toml);
        slow_parse.timeout = Some(Duration::from_millis(1));
        let toml: String = (0..3_000)
            .map(|i| format!("[[item]]\nid = {i}\nname = \"item {i}\"\n\n"))
            .collect();
        let e = json_of(&with_stdin(&slow_parse, &toml).stderr);
        assert_eq!(
            e["error"]["message"],
            json!("timeout: the parse ran longer than 0.001 s")
        );
    }

    #[test]
    fn render_failures_keep_the_error_shape() {
        let r = req(Op::Render(Renderer::Csv));
        // The input did not parse: the transducer's code, and the position.
        let out = with_stdin(&r, "[{\"a\": 1},\n {\"b\": }]");
        assert_eq!(out.status, status::PARSE);
        assert_eq!(out.stdout, "");
        let e = &json_of(&out.stderr)["error"];
        assert_eq!(e["kind"], json!("transduce"));
        assert_eq!(e["file"], json!("-"));
        assert_eq!(e["format"], json!("json"));
        assert_eq!(e["code"], json!("INPUT_INVALID"));
        assert_eq!((e["line"].clone(), e["col"].clone()), (json!(2), json!(8)));
        assert_eq!(e["output"], json!("none"));
        assert!(
            e.get("row").is_none(),
            "the transducer's row is aless's line"
        );
        // Records that are not records.
        let e = json_of(&with_stdin(&r, r#"{"a": 1}"#).stderr);
        assert_eq!(e["error"]["code"], json!("INPUT_INVALID"));
        assert!(
            e["error"]["message"].as_str().unwrap().contains("--path"),
            "{e}"
        );
        assert_eq!(e["error"]["path"], json!("."));
        let mut deep = r.clone();
        deep.start = Start::Path(".rows".into());
        let e = json_of(&with_stdin(&deep, r#"{"rows": [{"a": 1}, 2]}"#).stderr);
        assert_eq!(e["error"]["path"], json!(".rows[1]"));
        // A limit of the transducer's is status 5, and says which limit.
        let wide: String = (0..10_001).map(|i| format!("\"k{i}\":1,")).collect();
        let e = json_of(&with_stdin(&r, &format!("[{{{}}}]", wide.trim_end_matches(','))).stderr);
        assert_eq!(e["error"]["code"], json!("RESOURCE_LIMIT_EXCEEDED"));
        assert_eq!(e["error"]["limit"]["name"], json!("max_columns"));
        let out = with_stdin(&r, &format!("[{{{}}}]", wide.trim_end_matches(',')));
        assert_eq!(out.status, status::TOO_LARGE);
        // A path that names nothing: not_found, with the nearest node.
        let mut absent = r.clone();
        absent.start = Start::Path(".rows.nope".into());
        let out = with_stdin(&absent, RECORDS);
        assert_eq!(out.status, status::NOT_FOUND);
        let e = &json_of(&out.stderr)["error"];
        assert_eq!(e["kind"], json!("not_found"));
        assert_eq!(e["path"], json!(".rows.nope"));
        assert_eq!(
            e["message"],
            json!("no .rows.nope in -: .rows has 2 items, [0] to [1]")
        );
        assert_eq!(
            e["nearest"],
            json!({"path": ".rows", "kind": "array", "line": null, "col": null, "length": 2})
        );
        assert_eq!(e["keys"], Value::Null);
        absent.start = Start::Path(".zz".into());
        let e = json_of(&with_stdin(&absent, RECORDS).stderr);
        assert_eq!(e["error"]["keys"], json!(["rows", "n"]));
        assert_eq!(e["error"]["nearest"]["kind"], json!("object"));
        // Mistakes in the command.
        let usage = |out: Output| {
            assert_eq!(out.status, status::USAGE, "{}", out.stderr);
            let e = json_of(&out.stderr);
            assert_eq!(e["error"]["kind"], json!("usage"));
            e["error"]["message"].as_str().unwrap().to_string()
        };
        let mut at = r.clone();
        at.start = Start::At(1, None);
        assert!(usage(with_stdin(&at, RECORDS)).contains("--path"));
        let mut last = r.clone();
        last.start = Start::Path(".rows[-1]".into());
        assert!(usage(with_stdin(&last, RECORDS)).contains("[-1]"));
        let mut text = r.clone();
        text.kind = Some(Format::Text);
        assert!(usage(with_stdin(&text, "hello\n")).contains("-k"));
        assert!(usage(run(&r, None)).starts_with("no input"));
        let mut lines = r.clone();
        lines.kind = Some(Format::Jsonl);
        assert!(usage(with_stdin(&lines, "")).contains("standard input is empty"));
        let mut two = r.clone();
        two.files = vec!["a.json".into(), "b.json".into()];
        assert!(usage(run(&two, None)).contains("--render reads one input"));
        // aless's own limits report as they do for a parse.
        let mut slow = req(Op::Render(Renderer::Json));
        slow.kind = Some(Format::Toml);
        slow.timeout = Some(Duration::from_millis(1));
        let toml: String = (0..3_000)
            .map(|i| format!("[[item]]\nid = {i}\nname = \"item {i}\"\n\n"))
            .collect();
        let out = with_stdin(&slow, &toml);
        assert_eq!(out.status, status::TIMEOUT, "{}", out.stderr);
        let e = &json_of(&out.stderr)["error"];
        assert_eq!(e["kind"], json!("timeout"));
        assert_eq!(e["code"], json!("timeout"));
        assert_eq!(e["seconds"], json!(0.001));
        assert_eq!(e["output"], json!("none"));
        assert!(e["line"].as_u64().unwrap() > 0, "{e}");
        assert!(e["report"].as_str().unwrap().contains("(stdin):"), "{e}");
        let deep = format!("{}1{}", "[".repeat(300), "]".repeat(300));
        let out = with_stdin(&req(Op::Render(Renderer::Json)), &deep);
        assert_eq!(out.status, status::PARSE);
        let e = &json_of(&out.stderr)["error"];
        assert_eq!(e["kind"], json!("parse"));
        assert_eq!(e["code"], json!("too_deep"));
    }

    /// A grammar from the command line is a format like any other here:
    /// `-k` and the extension select it, `format` is its name, and the
    /// value is what the grammar builds.
    #[test]
    fn custom_grammars_are_formats_like_any_other() {
        use crate::grammar::{self, Definition};
        let def = Definition::parse(
            "--grammar-expr",
            &format!("impl-kv-h={}", crate::grammar::tests::KV),
        )
        .unwrap();
        let custom = Format::Custom(grammar::register(def, Limits::NONE).unwrap());
        let src = "a=1\nb = two\n";
        let mut r = req(Op::Json);
        r.kind = Some(custom);
        r.compact = true;
        let out = with_stdin(&r, src);
        assert_eq!(out.status, status::OK, "{}", out.stderr);
        assert_eq!(
            out.stdout,
            "[{\"key\":\"a\",\"val\":\"1\"},{\"key\":\"b\",\"val\":\"two\"}]\n"
        );
        let mut r = req(Op::Paths);
        r.kind = Some(custom);
        r.depth = Some(1);
        let v = json_of(&with_stdin(&r, src).stdout);
        assert_eq!(v["format"], json!("impl-kv-h"));
        let paths: Vec<&str> = v["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["path"].as_str().unwrap())
            .collect();
        assert_eq!(paths, [".", "[0]", "[1]"]);
        // The values are the tokens, so they have positions.
        let mut r = req(Op::Where);
        r.kind = Some(custom);
        r.start = Start::At(2, Some(5));
        let v = json_of(&with_stdin(&r, src).stdout);
        assert_eq!(v["path"], json!("[1].val"));
        assert_eq!(v["value"], json!("two"));
        assert_eq!((v["line"].clone(), v["col"].clone()), (json!(2), json!(5)));
        // An input the grammar refuses: a parse error in the grammar's name.
        let mut r = req(Op::Json);
        r.kind = Some(custom);
        let out = with_stdin(&r, "a=1\nb 2\n");
        assert_eq!(out.status, status::PARSE);
        let e = &json_of(&out.stderr)["error"];
        assert_eq!(e["kind"], json!("parse"));
        assert_eq!(e["format"], json!("impl-kv-h"));
        assert_eq!(e["code"], json!("unexpected"));
        assert_eq!((e["line"].clone(), e["col"].clone()), (json!(2), json!(3)));
        // A file is read by its extension, --check included.
        let dir = std::env::temp_dir().join(format!("aless-headless-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("settings.impl-kv-h");
        std::fs::write(&file, src).unwrap();
        let mut r = req(Op::Check);
        r.files = vec![file.clone()];
        let v = json_of(&run(&r, None).stdout);
        assert_eq!(v["files"][0]["format"], json!("impl-kv-h"));
        assert_eq!(v["files"][0]["ok"], json!(true));
        let mut r = req(Op::Json);
        r.files = vec![file];
        r.compact = true;
        assert_eq!(
            run(&r, None).stdout,
            "[{\"key\":\"a\",\"val\":\"1\"},{\"key\":\"b\",\"val\":\"two\"}]\n"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn grammar_failures_have_their_shape() {
        use crate::grammar::{self, Definition};
        let e = grammar::register(
            Definition::parse("--grammar-expr", "impl-bad-h=doc = nope\n").unwrap(),
            Limits::NONE,
        )
        .unwrap_err();
        let (text, status) = grammar_failure(&e, true);
        assert_eq!(status, status::USAGE);
        let v = json_of(&text);
        let err = &v["error"];
        assert_eq!(err["kind"], json!("usage"));
        assert!(
            err["message"]
                .as_str()
                .unwrap()
                .starts_with("--grammar-expr impl-bad-h: abnf"),
            "{err}"
        );
        assert_eq!(err["grammar"], json!("impl-bad-h"));
        assert!(err.get("file").is_none(), "{err}");
        assert!(text.ends_with('\n') && text.lines().count() == 1);
        // A grammar file that cannot be read is an io error, with the file.
        let e = grammar::register(
            Definition::parse("--grammar", "impl-missing-h=/nonexistent/g.abnf").unwrap(),
            Limits::NONE,
        )
        .unwrap_err();
        let (text, status) = grammar_failure(&e, false);
        assert_eq!(status, status::IO);
        let err = &json_of(&text)["error"];
        assert_eq!(err["kind"], json!("io"));
        assert_eq!(err["grammar"], json!("impl-missing-h"));
        assert_eq!(err["file"], json!("/nonexistent/g.abnf"));
        assert_eq!(err["code"], json!("io"));
        assert!(
            err["message"]
                .as_str()
                .unwrap()
                .starts_with("--grammar impl-missing-h: /nonexistent/g.abnf: "),
            "{err}"
        );
        // With the fields of any io error: the grammar file is the file,
        // and there is no input format.
        for key in ["format", "line", "col", "hint", "source_line"] {
            assert_eq!(err[key], Value::Null, "{key}: {err}");
        }
        let report = err["report"].as_str().unwrap();
        assert!(
            report.starts_with("[aless/io]: ") && report.contains("--> /nonexistent/g.abnf"),
            "{report}"
        );
        // A grammar file over --max-size: too_large, with its size and the
        // limit, and a hint worded for a grammar file.
        let dir = std::env::temp_dir().join(format!("aless-grammar-h-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("big.abnf");
        std::fs::write(&file, crate::grammar::tests::KV).unwrap();
        let def =
            Definition::parse("--grammar", &format!("impl-big-h={}", file.display())).unwrap();
        let limits = Limits {
            max_size: Some(8),
            timeout: None,
            started: None,
        };
        let e = grammar::register(def, limits).unwrap_err();
        let (text, status) = grammar_failure(&e, true);
        assert_eq!(status, status::TOO_LARGE);
        let err = &json_of(&text)["error"];
        assert_eq!(err["kind"], json!("too_large"));
        assert_eq!(err["code"], json!("too_large"));
        assert_eq!(err["grammar"], json!("impl-big-h"));
        assert_eq!(err["file"], json!(file.display().to_string()));
        assert_eq!(err["format"], Value::Null);
        assert_eq!(err["size"], json!(crate::grammar::tests::KV.len()));
        assert_eq!(err["limit"], json!(8));
        assert!(
            err["hint"]
                .as_str()
                .unwrap()
                .starts_with("A grammar file over --max-size is not read. Pass --max-size "),
            "{err}"
        );
        assert!(
            err["report"]
                .as_str()
                .unwrap()
                .starts_with("[aless/too_large]: the grammar file is "),
            "{err}"
        );
        std::fs::remove_dir_all(&dir).unwrap();
        // A compile past --timeout: timeout, with the seconds; inline text
        // has no file.
        let def = Definition::parse("--grammar-expr", "impl-slow-h=doc = 1*300\"a\"\n").unwrap();
        let limits = Limits {
            max_size: None,
            timeout: Some(std::time::Duration::from_millis(1)),
            started: None,
        };
        let e = grammar::register(def, limits).unwrap_err();
        let (text, status) = grammar_failure(&e, true);
        assert_eq!(status, status::TIMEOUT);
        let err = &json_of(&text)["error"];
        assert_eq!(err["kind"], json!("timeout"));
        assert_eq!(err["code"], json!("timeout"));
        assert_eq!(err["grammar"], json!("impl-slow-h"));
        assert_eq!(err["file"], Value::Null);
        assert_eq!(err["format"], Value::Null);
        assert_eq!(err["seconds"], json!(0.001));
        assert_eq!(err["line"], Value::Null);
        assert!(
            err["message"]
                .as_str()
                .unwrap()
                .ends_with("took longer than 0.001 s to compile"),
            "{err}"
        );
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
