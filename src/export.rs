//! `--render`: a document's records as CSV, or the document as JSON text,
//! streamed through the tabnas transducer and renderers rather than built
//! into the viewer's tree; a format written by a render its crate
//! carries runs through the same plumbing from [`crate::translate`].
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
//!   (`capability::incremental`: the JSON family, jsonic, YAML, ZON and
//!   Markdown) is parsed whole, the events leaving as the parse proceeds,
//!   and the exported array is emptied behind the stream so it is not held
//!   twice — except where the grammar may read a container back after it
//!   was streamed (see [`plan`]);
//! - every other grammar is parsed whole and its value walked afterwards.
//!
//! A verified grammar may still refuse to stream a particular document
//! part-way (a jsonic implicit list whose first element is a container, a
//! YAML stream of several documents or a `<<` merge key, a repeated member
//! the grammar merges): the transducer stops with `STREAMABILITY_UNKNOWN`
//! or `DUPLICATE_MEMBER` before a wrong stream can complete. When nothing
//! has reached the output yet, the export falls back once to the other
//! path, parsing the document whole and streaming its value; when output
//! had been written, the failure is reported with `output: "partial"`.
//!
//! Everything here is terminal-free: the output is any writer, and the
//! result is a value `headless` turns into JSON. aless's own caps hold as
//! they do for a parse: the depth cap and `--timeout` through [`load::guard`]
//! on the grammar's parser while it parses, and the deadline's alarm
//! through the transducer's abort flag for the rest of the run (a
//! line-by-line read, and the walk of a value after its parse) and
//! through a program's own flag in every mode, so the whole operation is
//! under the deadline; a grammar that panics is caught at the same
//! boundary the loader has.

use std::io::{self, BufRead, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;
use tabnas::Tabnas;
use tabnas_render::{CsvOptions, CsvRenderer, JsonOptions, JsonRenderer, MissingText, WriteOut};
use tabnas_transduce::source::{LineFormat, LinesSource};
use tabnas_transduce::{
    capability, AbortFlag, Code, Duplicates, Fail, Flow, Guarded, JsonEvent, Limits, Metrics,
    ParserSource, Prune, Schema, Selector, Sink, Source, SourceMode, TableBinding, TableEvent,
    TableFromJson, TableSink, ValueSource,
};

use crate::fmt;
use crate::headless::{kind_of_event, Seg, KEYS_LISTED};
use crate::load::{
    self, Deadline, Deep, Format, LoadError, Stop, STOP_DEPTH, STOP_TIME, STOP_VALUE_DEPTH,
};

/// The output `--render` writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Renderer {
    /// The records as the spec's always-quoted CSV: every field quoted,
    /// CRLF, a header row.
    Csv,
    /// The value as JSON text.
    Json,
    /// The value in a format written by its own render, an alchemy part
    /// its crate hands over, named by the manifest's `languageId`
    /// (`yaml`): see [`crate::translate`].
    Part(&'static str),
}

impl Renderer {
    /// `--render`'s argument: a part the registry has, by its id, which
    /// names the built-ins too (`csv` and `json` are formats whose
    /// manifests name aless's own renderers), so that the source decides
    /// how its events reach the renderer; the built-in itself only where
    /// no manifest names it.
    pub fn from_name(name: &str) -> Option<Renderer> {
        let name = name.trim().to_ascii_lowercase();
        if let Some(id) = crate::translate::id_of(&name) {
            return Some(Renderer::Part(id));
        }
        match name.as_str() {
            "csv" => Some(Renderer::Csv),
            "json" => Some(Renderer::Json),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Renderer::Csv => "csv",
            Renderer::Json => "json",
            Renderer::Part(id) => id,
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
///
/// Pruning empties the exported array in the engine's tree as its
/// elements are streamed, which is sound only while the grammar never
/// reads that array back. YAML does: an alias copies its anchor's value
/// when the alias is met. jsonic rewrites containers in place after they
/// are built (a value promoted into an implicit list, repeated members
/// merged), and Markdown builds its tree imperatively, appending to nodes
/// it already inserted; whether either reads a streamed array back is not
/// established, so neither is pruned. The rest keep the memory saving.
pub fn plan(format: Format, at_root: bool) -> Option<Plan> {
    Some(match format {
        Format::Text => return None,
        Format::Jsonl | Format::Csv | Format::Tsv if at_root => Plan::Lines,
        // A grammar from the command line is nobody's verified grammar,
        // whatever it is named (it may take a built-in's name).
        Format::Custom(_) => Plan::Materialize,
        f if capability::incremental(f.name()) => Plan::Incremental {
            prune: !matches!(f, Format::Yaml | Format::Jsonic | Format::Markdown),
        },
        _ => Plan::Materialize,
    })
}

/// What a run produces.
#[derive(Clone, Debug)]
pub enum What {
    /// `--render`: the records at the path as CSV, or the value there as
    /// JSON text.
    Render(Renderer),
    /// `--alchemy`: a program's output, whatever it is; `rows` is the
    /// selector the program reads its rows under, when its plan names
    /// one, so the parse can be pruned under it as an export's is under
    /// the exported array (see [`crate::alchemy`]).
    Program { rows: Option<Selector> },
    /// `--render` through a format's own render, or a program's output
    /// through one: a program over the value at the path, pruned behind
    /// it as JSON's render is (see [`crate::translate`]).
    Part,
}

/// One export, or one program's run.
#[derive(Clone, Debug)]
pub struct Job {
    /// The input as errors name it: the path as given, or `-`.
    pub name: String,
    /// The input as a report names it (`load::origin_of`, or `(stdin)`).
    pub origin: String,
    pub format: Format,
    pub what: What,
    /// The value exported: the root when empty.
    pub path: Vec<Seg>,
    /// JSON on one line.
    pub compact: bool,
    /// Indentation per level of JSON output otherwise.
    pub indent: usize,
    pub timeout: Option<Duration>,
    /// When the time limit started running: the run's start, so that the
    /// time spent reading its input counts (see [`load::Limits::started`]);
    /// `None` for the start of the parse.
    pub started: Option<Instant>,
    /// The most a program may write (the transducer's `max_output_bytes`,
    /// `--max-output`); `None` for no limit, and for a render, whose
    /// output follows its input.
    pub max_output: Option<u64>,
}

impl Job {
    /// Whether the run's deadline has passed: its time limit, counted from
    /// its start; false when either is unknown.
    fn past_deadline(&self) -> bool {
        self.started
            .zip(self.timeout)
            .and_then(|(started, limit)| started.checked_add(limit))
            .is_some_and(|at| Instant::now() >= at)
    }
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
    /// aless's own limits, or a grammar's (`too_deep`, `timeout`), or a
    /// grammar that panicked (`grammar`), in the shape of a load error so
    /// it reports as one; `partial` says whether output had been written
    /// when the run stopped.
    Load {
        error: Box<LoadError>,
        partial: bool,
    },
    /// A transducer stage failed; the code says which. Under a program
    /// ([`What::Program`]), the source's failures come here and the
    /// program's own in [`Program`](Self::Program).
    Transduce(Box<Fail>),
    /// The program's sink failed, as against the source that fed it: a
    /// failure of the program's own, or one it met in the data, which
    /// `headless` tells apart by code and position
    /// (`alchemy::is_placed`: the events a program reads carry no
    /// positions, so a position on a failure that came this way is in the
    /// program). A code the language shares with the source
    /// (`STREAMABILITY_UNKNOWN`: the checker's or the evaluator's, and a
    /// verified grammar's refusal to stream part-way) is the program's
    /// only when it came this way. A `Code::Aborted` that came this way
    /// is the deadline's and is classified as a `timeout` instead, with
    /// no position.
    Program(Box<Fail>),
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
    let renderer = match &job.what {
        What::Render(Renderer::Part(_)) | What::Part | What::Program { .. } => {
            return Err(ExportError::Usage(
                "a program's run, a part's among them, goes through run_program".to_string(),
            ))
        }
        What::Render(renderer) => renderer,
    };
    let written = Arc::new(AtomicU64::new(0));
    let broken = Arc::new(AtomicBool::new(false));
    // Shared, so that a second chain of sinks (the fallback) can write to
    // the same output without the first chain having to hand it back.
    let out = Arc::new(Mutex::new(out));
    let records = match renderer {
        Renderer::Csv => Records::Csv,
        _ => Records::Json,
    };
    let pipe = || WriteOut::new(Pipe::new(out.clone(), &written, &broken, records));
    match renderer {
        Renderer::Part(_) => unreachable!("refused above"),
        Renderer::Json => {
            let options = JsonOptions {
                indent: (!job.compact).then_some(job.indent),
                trailing_newline: true,
            };
            attempt(job, input, &written, &broken, |_| {
                Ok(Scope::new(
                    job.path.clone(),
                    JsonRenderer::new(pipe(), options.clone()),
                ))
            })
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
            attempt(job, input, &written, &broken, |_| {
                let csv = CsvRenderer::new(pipe(), options.clone())
                    .map_err(|f| ExportError::Transduce(Box::new(f)))?;
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
                Ok(Scope::new(job.path.clone(), Rows::new(table)))
            })
        }
    }
}

/// Run a program's sink over `input`, through the source plumbing an
/// export runs on: the input read as [`plan`] says, the parse pruned under
/// the program's rows, aless's caps and `--timeout` on the parse and the
/// deadline's alarm on the rest, the one fallback to the whole value when
/// a grammar refuses to stream before anything was written, and the same
/// failures. The program reads the value at the job's path, re-rooted
/// there as an export's renderer reads it: a program's own job has none,
/// and reads the whole document; a part's may. `chain` builds the sink
/// over the writer it is given and the program's abort flag, which its
/// functions read between steps and the deadline's alarm raises in every
/// mode; it is called once per attempt. `records` says where the records
/// of what it writes end, when a run that fails has to stop at one.
pub fn run_program(
    job: &Job,
    input: Input<'_>,
    out: Box<dyn Write + Send>,
    records: Records,
    chain: impl Fn(Box<dyn Write + Send>, AbortFlag) -> Result<Box<dyn Sink + Send>, ExportError>,
) -> Result<(), ExportError> {
    let written = Arc::new(AtomicU64::new(0));
    let broken = Arc::new(AtomicBool::new(false));
    let out = Arc::new(Mutex::new(out));
    attempt(job, input, &written, &broken, |abort| {
        let pipe: Box<dyn Write + Send> =
            Box::new(Pipe::new(out.clone(), &written, &broken, records));
        // A scope at the root passes every event through and never stops
        // the run: the program does the selecting. A part's scope re-roots
        // the stream at `--path`, as an export's does.
        Ok(Scope::new(job.path.clone(), chain(pipe, abort)?))
    })
}

/// Run the export through a fresh chain of sinks from `chain`, and once
/// more through the whole-value path when the incremental stream was
/// refused part-way before anything reached the output (see the module
/// doc); a refusal after output was written is reported as it is.
fn attempt<S: Sink + Send + 'static>(
    job: &Job,
    input: Input<'_>,
    written: &AtomicU64,
    broken: &AtomicBool,
    chain: impl Fn(AbortFlag) -> Result<Scope<S>, ExportError>,
) -> Result<(), ExportError> {
    let mode = mode_for(job);
    let text = match &input {
        Input::Text(text) => Some(*text),
        Input::Lines(_) => None,
    };
    // Two flags per attempt, both fresh: the source polls one, raised by
    // the parser's guard with the position, or by the alarm where no
    // guard runs; a program's sink reads the other between steps, which
    // the alarm raises in every mode (see [`Alarm`]), so the deadline
    // stops a slow parse and a program slow on one item alike.
    let abort = AbortFlag::new();
    let program = AbortFlag::new();
    let failed = match run(
        job,
        input,
        chain(program.clone())?,
        mode.clone(),
        abort,
        program,
    ) {
        Ok(()) => return Ok(()),
        Err(failed) => failed,
    };
    // A program's own `STREAMABILITY_UNKNOWN`, its sink's, is the
    // program's, as `classify` reports it, and the whole value would meet
    // the same program again: only the source's refusal falls back. A
    // repeated member is the stream's whoever meets it first, and the
    // whole value, which keeps one, has none.
    let programs = failed.from_sink && matches!(job.what, What::Program { .. } | What::Part);
    let refused = failed.verdict.is_none()
        && match &failed.why {
            Outcome::Failed(f) => {
                (f.code == Code::StreamabilityUnknown && !programs)
                    || f.code == Code::DuplicateMember
                    // The rows sink refused a root that is not an array at
                    // its first event. For a grammar that may wrap the root
                    // in a list after streaming it (jsonic's implicit list,
                    // a YAML stream of documents), the whole value may yet
                    // be one; every other grammar's root is final.
                    || (f.code == Code::InputInvalid
                        && f.path.as_deref() == Some(".")
                        && matches!(job.format, Format::Jsonic | Format::Yaml))
            }
            _ => false,
        };
    match text {
        Some(text)
            if refused
                && mode != SourceMode::Materialize
                && written.load(Ordering::Relaxed) == 0 =>
        {
            let abort = AbortFlag::new();
            let program = AbortFlag::new();
            run(
                job,
                Input::Text(text),
                chain(program.clone())?,
                SourceMode::Materialize,
                abort,
                program,
            )
            .map_err(|f| classify(job, *f, written, broken))
        }
        _ => Err(classify(job, *failed, written, broken)),
    }
}

/// How a run ended.
#[derive(Debug)]
enum Outcome {
    /// The source ran to the document's end (the scope never stops it, so
    /// a stop is a whole run too).
    Done,
    /// A stage failed.
    Failed(Fail),
    /// The grammar panicked; this is what it said.
    Panicked(String),
}

impl Outcome {
    fn of(result: Result<Flow, Fail>) -> Outcome {
        match result {
            Ok(_) => Outcome::Done,
            Err(fail) => Outcome::Failed(fail),
        }
    }
}

/// What a run ends with: how, the scope's verdict on the path when it has
/// one, and, when aless's guard was on the parser, its record of why the
/// parse stopped.
struct Ran {
    outcome: Outcome,
    stop: Option<Arc<Stop>>,
    verdict: Option<Verdict>,
    /// The scope's sink failed (a renderer's chain, or a program's), as
    /// against the source that fed it.
    from_sink: bool,
}

/// A failure with what the run knew: the guard's record, how far the
/// parse got, whether the sink raised it, and whether the input was read
/// a record at a time as the run went.
#[derive(Debug)]
struct Failed {
    why: Outcome,
    stop: Option<Arc<Stop>>,
    verdict: Option<Verdict>,
    from_sink: bool,
    lines: bool,
}

/// What the deadline's alarm does when it fires, on the waiting thread.
///
/// While a grammar parses a whole text, the guard on its parser sees the
/// deadline first, between two steps, and records where the parse was
/// before it raises the abort flag; the alarm leaves the flag alone then,
/// since the transducer's own guard runs ahead of the budget and would
/// stop the parse without the position. Everywhere else nothing but the
/// abort flag stops the run: a line-by-line read, which parses a record at
/// a time under no guard of aless's, and the walk of a parsed value, which
/// the source polls the flag on at every event. The alarm raises the flag
/// in those cases, so the deadline covers the whole operation.
///
/// A program's sink has a flag of its own, which the alarm raises in
/// every mode: while the sink works on one item the parser is inside an
/// event, where no guard of aless's runs, so nothing but that flag would
/// stop a program slow per item, and the parse's position is not at
/// stake, since the parser's guard sees the deadline first the moment
/// the sink returns.
struct Alarm {
    lines: bool,
    /// The parse of a whole text has returned, and its value is being
    /// walked.
    parsed: Arc<AtomicBool>,
    abort: AbortFlag,
    /// The program's flag ([`run_program`]'s `chain` builds the sink over
    /// it); a render's chain has one too, which nothing reads.
    program: AbortFlag,
}

impl Alarm {
    fn fire(&self) {
        self.program.abort();
        if self.lines || self.parsed.load(Ordering::Relaxed) {
            self.abort.abort();
        }
    }
}

/// Drive `input` into `scope` on the parse thread, under `--timeout`; a
/// text input is read in `mode`. `abort` is the source's flag and
/// `program` the sink's (see [`Alarm`]).
fn run<S: Sink + Send + 'static>(
    job: &Job,
    input: Input<'_>,
    scope: Scope<S>,
    mode: SourceMode,
    abort: AbortFlag,
    program: AbortFlag,
) -> Result<(), Box<Failed>> {
    let format = job.format;
    let lines = matches!(input, Input::Lines(_));
    let alarm = Alarm {
        lines,
        parsed: Arc::new(AtomicBool::new(false)),
        abort: abort.clone(),
        program,
    };
    let parsed = alarm.parsed.clone();
    let ran = load::on_parse_thread(
        job.timeout,
        job.started,
        move || alarm.fire(),
        move |deadline: Option<Deadline>| match input {
            Input::Lines(reader) => {
                let source = LinesSource::new(reader, line_format(format))
                    .limits(limits())
                    .abort(abort);
                match load::catch_grammar(|| source.run_owned(scope)) {
                    Ok((outcome, scope)) => Ran {
                        outcome: Outcome::of(outcome),
                        stop: None,
                        verdict: scope.verdict(),
                        from_sink: scope.sink_failed,
                    },
                    Err(what) => Ran {
                        outcome: Outcome::Panicked(what),
                        stop: None,
                        verdict: None,
                        from_sink: false,
                    },
                }
            }
            Input::Text(text) => {
                let mut parser = match load::make_parser(format) {
                    Ok(parser) => parser.expect("plain text is refused before a run"),
                    // A custom grammar's engine would not take its spec:
                    // reported as the grammar's failure, which it is.
                    Err(e) => {
                        return Ran {
                            outcome: Outcome::Panicked(e.message),
                            stop: None,
                            verdict: None,
                            from_sink: false,
                        }
                    }
                };
                let notify = abort.clone();
                let stop = load::guard(
                    &mut parser,
                    deadline.clone(),
                    load::MAX_RULE_DEPTH,
                    move || notify.abort(),
                );
                // Without its byte-order mark, and as the loader prepares
                // it for the format: the same lines and columns.
                let text = load::parser_text(text, format);
                let text = text.as_ref();
                match mode {
                    SourceMode::Materialize => {
                        materialize(parser, format, text, deadline, &abort, &parsed, scope, stop)
                    }
                    mode => {
                        // The source checks the grammar's name against the
                        // verified list itself, and refuses an unlisted one
                        // before parsing.
                        let source = ParserSource::new(parser, text)
                            .mode(mode)
                            .grammar(format.name())
                            .limits(limits())
                            .abort(abort);
                        match load::catch_grammar(|| source.run_owned(scope)) {
                            Ok((outcome, scope)) => Ran {
                                outcome: Outcome::of(outcome),
                                stop: Some(stop),
                                verdict: scope.verdict(),
                                from_sink: scope.sink_failed,
                            },
                            Err(what) => Ran {
                                outcome: Outcome::Panicked(what),
                                stop: Some(stop),
                                verdict: None,
                                from_sink: false,
                            },
                        }
                    }
                }
            }
        },
    );
    match ran.outcome {
        Outcome::Done => Ok(()),
        why => Err(Box::new(Failed {
            why,
            stop: ran.stop,
            verdict: ran.verdict,
            from_sink: ran.from_sink,
            lines,
        })),
    }
}

/// Parse, then walk the value: the transducer's `Materialize` mode, done
/// here rather than by `ParserSource` so that the moment the parse returns
/// is known. From then on the deadline is the alarm's to enforce, through
/// the abort flag the walk polls; and a deadline that passed during the
/// parse's last step, which no guard saw, is caught here too.
#[allow(clippy::too_many_arguments)]
fn materialize<S: Sink>(
    parser: Tabnas,
    format: Format,
    text: &str,
    deadline: Option<Deadline>,
    abort: &AbortFlag,
    parsed: &AtomicBool,
    scope: Scope<S>,
    stop: Arc<Stop>,
) -> Ran {
    let value = load::catch_grammar(|| parser.parse(text).map_err(Box::new));
    parsed.store(true, Ordering::Relaxed);
    // A parse that came to its end after the deadline, with a value or an
    // error, is too late, as the loader has it: the time is its outcome,
    // whatever it returned (a stop the guard made is read first, from
    // `stop`, when the run is classified).
    let late = deadline.as_ref().is_some_and(Deadline::passed);
    if late {
        abort.abort();
    }
    let value = match value {
        Err(what) => {
            return Ran {
                outcome: Outcome::Panicked(what),
                stop: Some(stop),
                verdict: None,
                from_sink: false,
            }
        }
        Ok(Err(e)) => {
            // As the transducer's source maps an engine error: the abort
            // flag's cancel is aless's, anything else the input's, unless
            // it came too late to count.
            let fail = if late || (e.code == "cancel" && abort.is_aborted()) {
                Fail::aborted()
            } else {
                Fail::from_tabnas(&e)
            };
            return Ran {
                outcome: Outcome::Failed(fail),
                stop: Some(stop),
                verdict: None,
                from_sink: false,
            };
        }
        Ok(Ok(value)) => value,
    };
    drop(parser);
    // Before the value's depth is measured, which would report a late
    // parse as `too_deep`; the value is let go without recursing into it,
    // as it may nest too deep for that.
    if late {
        load::drop_deep(value);
        return Ran {
            outcome: Outcome::Failed(Fail::aborted()),
            stop: Some(stop),
            verdict: None,
            from_sink: false,
        };
    }
    // A custom grammar's nesting is measured on its value, as the loader
    // measures it; the value is let go without recursing into it.
    if format.is_custom() && load::nests_past(&value, load::MAX_VALUE_DEPTH) {
        stop.mark(STOP_VALUE_DEPTH);
        load::drop_deep(value);
        return Ran {
            outcome: Outcome::Failed(Fail::aborted()),
            stop: Some(stop),
            verdict: None,
            from_sink: false,
        };
    }
    let mut guarded = Guarded::new(scope, &limits(), abort.clone(), Metrics::new());
    let outcome = ValueSource(&value).run(&mut guarded);
    let scope = guarded.into_inner();
    Ran {
        outcome: Outcome::of(outcome),
        stop: Some(stop),
        verdict: scope.verdict(),
        from_sink: scope.sink_failed,
    }
}

/// The source mode for a text input: incremental where the grammar is
/// verified, pruning the exported array as it streams where [`plan`]
/// allows.
fn mode_for(job: &Job) -> SourceMode {
    // A custom grammar builds its value whole; nothing streams it.
    if job.format.is_custom() {
        return SourceMode::Materialize;
    }
    let prune = match plan(job.format, job.path.is_empty()) {
        Some(Plan::Incremental { prune }) => prune,
        // A line-delimited format handed over as one text: JSON Lines is
        // verified, CSV and TSV are not.
        Some(Plan::Lines) if capability::incremental(job.format.name()) => true,
        _ => return SourceMode::Materialize,
    };
    let prune = if prune {
        match &job.what {
            // The array exported: named by its elements for CSV, as the
            // rows selector does, and by itself for JSON; the source
            // prunes the same array either way.
            What::Render(Renderer::Csv) => Prune::Under(selector_of(&job.path).each_index()),
            What::Render(Renderer::Json) | What::Render(Renderer::Part(_)) | What::Part => {
                Prune::Under(selector_of(&job.path))
            }
            // The rows a program reads, when its plan names them; a
            // program that reads the document some other way keeps it.
            What::Program { rows: Some(rows) } => Prune::Under(rows.clone()),
            What::Program { rows: None } => Prune::Never,
        }
    } else {
        Prune::Never
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
        why,
        stop,
        verdict,
        from_sink,
        lines,
    } = failed;
    let partial = written.load(Ordering::Relaxed) > 0;
    let load = |error: LoadError| ExportError::Load {
        error: Box::new(error),
        partial,
    };
    let fail = match why {
        Outcome::Done => unreachable!("a run that ended well is no failure"),
        Outcome::Panicked(what) => {
            return load(load::grammar_failed(job.format, &what).with_origin(&job.origin))
        }
        Outcome::Failed(fail) => fail,
    };
    // The scope's word on the path comes first: it stopped the run itself,
    // and its failure names the input's paths, which need no re-rooting.
    match verdict {
        Some(Verdict::Missing(nearest)) => {
            return ExportError::NotFound {
                message: not_found_message(job, &nearest),
                nearest: Box::new(nearest),
            }
        }
        Some(Verdict::FromEnd { index, at }) => {
            return ExportError::Usage(format!(
                "--render reads the input once, front to back, so it cannot count from the end \
                 of an array: [{index}] in {} names an item of the array at {at}; use --json \
                 --path, which reads the whole document",
                path_text(&job.path)
            ))
        }
        Some(Verdict::Duplicate(mut fail)) => {
            fail.committed_output = partial;
            return ExportError::Transduce(Box::new(fail));
        }
        None => {}
    }
    // What reached the output is what the pipe wrote: a stage above it may
    // have handed over bytes the pipe holds back, a record not yet whole,
    // which a failure drops.
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
            return load(too_deep(job, line, col, Deep::Rules(load::MAX_RULE_DEPTH)));
        }
        Some(STOP_VALUE_DEPTH) => return load(too_deep(job, 0, 0, Deep::Value)),
        Some(STOP_TIME) => {
            let (line, col) = stop.as_ref().map_or((0, 0), |s| s.position());
            return load(timed_out(job, line, col));
        }
        _ => {}
    }
    // A line-delimited input read as the run goes fails the source with
    // the reader's error, not the abort flag's, when a read was still
    // waiting at the deadline (standard input whose writer sent nothing
    // more, see [`load::DeadlineReader`]): the input timed out, whatever
    // code the source gave the error.
    if lines && !from_sink && fail.code != Code::Aborted && job.past_deadline() {
        return load(input_timed_out(job));
    }
    match fail.code {
        // Only the deadline's alarm raises the abort flags ahead of the
        // guard: the source's on a line-by-line read, or once a parse has
        // returned and its value is being walked, where there is no
        // position to give; and a program's in every mode.
        Code::Aborted => {
            let (line, col) = match job.what {
                // The program's sink returns the deadline's Aborted with
                // the program's own span stamped on it (its evaluator
                // places every failure at the form it was in), which is
                // no position in the input. The parser's guard records
                // how far the parse got only when it stops the parse
                // itself, which the arm above reports with that position;
                // a run the sink stopped mid-item has none to give.
                What::Program { .. } | What::Part if from_sink => (0, 0),
                _ => position(&fail),
            };
            load(timed_out(job, line, col))
        }
        // A grammar's own depth limit stops the parse with the engine's
        // `cancel`, as the viewer's loader reads it too: the transducer's
        // source words it as the grammar's guard, the materialized path
        // here reports the engine's code as it is.
        Code::InputInvalid
            if fail.message.starts_with("cancel")
                || fail.message.starts_with("the grammar stopped the parse") =>
        {
            let (line, col) = position(&fail);
            load(too_deep(job, line, col, Deep::Grammar))
        }
        Code::OutputFailed if broken.load(Ordering::Relaxed) => ExportError::ReaderGone,
        _ => {
            let mut fail = absolute(fail, &job.path);
            fail.committed_output = partial;
            // Where it came from decides whose it is: a program's sink
            // raising a code the source also raises (a run-time
            // `STREAMABILITY_UNKNOWN`) is the program's, and the source
            // raising it (a grammar that refused to stream part-way, once
            // output had left) is the input's, whatever the code.
            match job.what {
                What::Program { .. } | What::Part if from_sink => {
                    ExportError::Program(Box::new(fail))
                }
                _ => ExportError::Transduce(Box::new(fail)),
            }
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

/// `timeout` for an input still being read at the deadline, worded as the
/// loader words it ([`load::input_timed_out_words`]).
fn input_timed_out(job: &Job) -> LoadError {
    let (message, hint) = load::input_timed_out_words(job.timeout.unwrap_or_default());
    positioned(job, "timeout", message, &hint, 0, 0)
}

/// `too_deep`, worded as the loader words it ([`load::too_deep_words`]).
fn too_deep(job: &Job, line: u32, col: u32, why: Deep) -> LoadError {
    let (message, hint) = load::too_deep_words(job.format, why);
    positioned(job, "too_deep", message, &hint, line, col)
}

/// `timeout`, worded as the loader words a parse stopped at its deadline;
/// under a program, worded for the whole run, since the deadline covers
/// the parse and the program together and either may have been running
/// when it passed: a stop in the parse carries how far the parse got, a
/// stop in the program's work on an item no position at all, so the
/// program's hint claims none.
fn timed_out(job: &Job, line: u32, col: u32) -> LoadError {
    let limit = job.timeout.map_or(0.0, |t| t.as_secs_f64()).to_string();
    let (message, hint) = match job.what {
        What::Render(_) => (
            format!("timeout: the parse ran longer than {limit} s"),
            format!(
                "The parse had got this far when --timeout {limit} stopped it; what was written \
                 before that stays written.\nPass a larger --timeout to let it finish, or \
                 --timeout 0 for no limit."
            ),
        ),
        What::Program { .. } | What::Part => (
            format!("timeout: the run (the parse and the program) ran longer than {limit} s"),
            format!(
                "--timeout {limit} stopped the run, in the parse or in the program's work on an \
                 item; what was written before that stays written.\nPass a larger --timeout to \
                 let it finish, or --timeout 0 for no limit."
            ),
        ),
    };
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

/// Where the records of a run's output end, so that a run that fails part
/// way leaves standard output at the end of one, every record there whole.
/// The writer above the pipe sends what it holds whenever its buffer
/// fills, wherever the renderer was; the pipe holds back what follows the
/// last record's end until the next one ends, or the run does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Records {
    /// No shape the pipe can see, and nothing is held: a program's own
    /// text, which `concat-map` writes an item at a time and each item
    /// whole, or a format's render.
    Any,
    /// CSV: a record ends with a line break outside quotes.
    Csv,
    /// JSON: a record is a value directly inside the root array or object,
    /// and ends before the comma that follows it; the root's end ends the
    /// last.
    Json,
    /// One record a line, as JSON Lines writes them.
    Lines,
}

/// The most the pipe holds back. A record longer than this is not held
/// whole, so that a root object of one huge member, say, does not sit in
/// memory: it is written up to the end of one of its own values where it
/// has them (a JSON record's), else as it comes, and a failure can then
/// cut it.
const HOLD_MAX: usize = 16 << 20;

/// The writer under the renderer's `WriteOut`: counts what reached the
/// output, so a failure can say whether output is partial, notices a
/// reader that went away, which is not a failure, and writes whole
/// records ([`Records`]). What it holds when the run fails is dropped with
/// it: only the run's end, the renderer's one flush, writes it.
struct Pipe {
    inner: Arc<Mutex<Box<dyn Write + Send>>>,
    written: Arc<AtomicU64>,
    broken: Arc<AtomicBool>,
    records: Records,
    /// What follows the last record's end.
    held: Vec<u8>,
    /// How much of `held` has been looked at, and what was open there.
    seen: usize,
    /// Inside a CSV field's quotes, or a JSON string.
    quoted: bool,
    /// After a backslash in a JSON string.
    escaped: bool,
    /// JSON containers open.
    depth: usize,
    /// Where in `held` the last value one level inside a JSON record ends:
    /// the cut for a record too long to hold.
    inner_end: usize,
    /// [`HOLD_MAX`], which a test lowers.
    hold_max: usize,
}

impl Pipe {
    fn new(
        inner: Arc<Mutex<Box<dyn Write + Send>>>,
        written: &Arc<AtomicU64>,
        broken: &Arc<AtomicBool>,
        records: Records,
    ) -> Pipe {
        Pipe {
            inner,
            written: written.clone(),
            broken: broken.clone(),
            records,
            held: Vec::new(),
            seen: 0,
            quoted: false,
            escaped: false,
            depth: 0,
            inner_end: 0,
            hold_max: HOLD_MAX,
        }
    }

    fn note(&self, r: io::Result<()>) -> io::Result<()> {
        if let Err(e) = &r {
            if e.kind() == io::ErrorKind::BrokenPipe {
                self.broken.store(true, Ordering::Relaxed);
            }
        }
        r
    }

    /// Write `bytes` to the output, counting them.
    fn send(&self, bytes: &[u8]) -> io::Result<()> {
        if bytes.is_empty() {
            return Ok(());
        }
        let r = self
            .inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .write_all(bytes);
        self.note(r)?;
        self.written
            .fetch_add(bytes.len() as u64, Ordering::Relaxed);
        Ok(())
    }

    /// Look at what `held` gained, and return where its last whole record
    /// ends (0 for none).
    fn last_end(&mut self) -> usize {
        let mut end = 0;
        for (i, &b) in self.held.iter().enumerate().skip(self.seen) {
            match self.records {
                Records::Any => end = i + 1,
                Records::Lines => {
                    if b == b'\n' {
                        end = i + 1;
                    }
                }
                Records::Csv => match b {
                    // A doubled quote inside quotes closes and reopens them.
                    b'"' => self.quoted = !self.quoted,
                    b'\n' if !self.quoted => end = i + 1,
                    _ => {}
                },
                Records::Json if self.quoted => match b {
                    _ if self.escaped => self.escaped = false,
                    b'\\' => self.escaped = true,
                    b'"' => self.quoted = false,
                    _ => {}
                },
                Records::Json => match b {
                    b'"' => self.quoted = true,
                    b'[' | b'{' => self.depth += 1,
                    b']' | b'}' => {
                        self.depth = self.depth.saturating_sub(1);
                        if self.depth == 0 {
                            end = i + 1;
                        }
                    }
                    b',' if self.depth == 1 => end = i,
                    b',' if self.depth == 2 => self.inner_end = i,
                    _ => {}
                },
            }
        }
        self.seen = self.held.len();
        end
    }
}

impl Write for Pipe {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.records == Records::Any {
            self.send(buf)?;
            return Ok(buf.len());
        }
        self.held.extend_from_slice(buf);
        let end = match self.last_end() {
            // A record too long to hold: up to the end of one of its own
            // values, where it has had one, else what there is.
            0 if self.held.len() > self.hold_max && self.inner_end > 0 => self.inner_end,
            0 if self.held.len() > self.hold_max => self.held.len(),
            end => end,
        };
        if end > 0 {
            let rest = self.held.split_off(end);
            let whole = std::mem::replace(&mut self.held, rest);
            self.seen -= end;
            self.inner_end = self.inner_end.saturating_sub(end);
            self.send(&whole)?;
        }
        Ok(buf.len())
    }

    /// The run's end, the one flush a renderer makes: what is held is
    /// whole now.
    fn flush(&mut self) -> io::Result<()> {
        let held = std::mem::take(&mut self.held);
        self.seen = 0;
        self.inner_end = 0;
        self.send(&held)?;
        let r = self.inner.lock().unwrap_or_else(|e| e.into_inner()).flush();
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
/// its key. Two things a stream cannot do are refused where they come
/// up, in a [`Verdict`]: count from the end of an array (`[-1]`), and
/// honour the last of a repeated key on the path as the grammars and
/// `--json` do, since the first may already have been written.
struct Scope<S> {
    inner: S,
    target: Vec<Seg>,
    frames: Vec<Frame>,
    state: State,
    /// The deepest node on the target's path seen so far.
    nearest: Option<Nearest>,
    /// Why the scope stopped the run, when it did.
    verdict: Option<Verdict>,
    /// The inner sink failed an event: the failure is the sink's, not the
    /// source's.
    sink_failed: bool,
}

/// The scope's own reasons to stop a run. Each is returned to the source
/// as a failure too, which ends the run; the caller reads the verdict off
/// the scope and reports it in its own shape.
#[derive(Clone, Debug, PartialEq)]
enum Verdict {
    /// The document ended without the target; this is the nearest node.
    Missing(Nearest),
    /// The path counts from the end of the array at `at`.
    FromEnd { index: i64, at: String },
    /// A key on the path was repeated after a value under it was taken.
    Duplicate(Fail),
}

struct Frame {
    kind: Container,
    /// Members or elements begun so far; for an array, the next index.
    count: usize,
    /// Whether this container's own path is a prefix of the target's, so
    /// that a child of it may be on the path.
    on_path: bool,
    /// A child on the path has begun: for an object, the key the path
    /// names has been seen once.
    matched: bool,
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
            verdict: None,
            sink_failed: false,
        }
    }

    /// Why the scope stopped the run, if it did.
    fn verdict(&self) -> Option<Verdict> {
        self.verdict.clone()
    }

    /// Stop the run with a verdict of the scope's own.
    fn stop(&mut self, verdict: Verdict, fail: Fail) -> Result<Flow, Fail> {
        self.verdict = Some(verdict);
        Err(fail)
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
        let r = self.inner.event(ev);
        if r.is_err() {
            self.sink_failed = true;
        }
        r
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
                    let fail =
                        Fail::input(format!("the document has no {}", path_text(&self.target)));
                    match self.nearest.take() {
                        Some(near) => self.stop(Verdict::Missing(near), fail),
                        // The root itself is the target when the path is
                        // empty, so a document has always begun a node on
                        // the path before it ends.
                        None => Err(fail),
                    }
                }
                State::Inside { .. } => Err(Fail::protocol(
                    "the document ended inside the value being exported",
                )),
            },
            _ => {
                let (on_path, is_target) = self.locate();
                let depth = self.frames.len();
                // On the path, the value's own path is the target's first
                // `depth` steps; off it, nothing below needs a path.
                let path = if on_path {
                    path_text(&self.target[..depth])
                } else {
                    String::new()
                };
                if on_path {
                    if let Some(parent) = self.frames.last_mut() {
                        if parent.matched {
                            // The path's key again: the grammars and --json
                            // keep the last value, and what this stream has
                            // taken from the first cannot be taken back.
                            let key = match &parent.kind {
                                Container::Object { key: Some(k) } => k.to_string(),
                                _ => String::new(),
                            };
                            let at = path_text(&self.target[..depth - 1]);
                            let fail = Fail::new(
                                Code::DuplicateMember,
                                format!(
                                    "the member {} of {at} is repeated, and --render has taken \
                                     the first: a stream cannot keep the last, as --json does",
                                    fmt::quote(&key)
                                ),
                            )
                            .at_path(at);
                            return self.stop(Verdict::Duplicate(fail.clone()), fail);
                        }
                        parent.matched = true;
                    }
                }
                self.count_child();
                if self.state == State::Before && is_target {
                    self.state = State::Inside { depth };
                }
                let inside = matches!(self.state, State::Inside { .. });
                if ev.is_start() {
                    let kind = if ev == JsonEvent::ObjectStart {
                        Container::Object { key: None }
                    } else {
                        Container::Array
                    };
                    if on_path && !is_target {
                        let object = matches!(kind, Container::Object { .. });
                        self.nearest = Some(Nearest {
                            path: path.clone(),
                            kind: kind_of_event(&ev),
                            length: Some(0),
                            value: None,
                            keys: object.then(Vec::new),
                            depth,
                        });
                        // Counting from the end needs the whole array, which
                        // a stream never has.
                        if let (false, Seg::Index(i)) = (object, &self.target[depth]) {
                            if *i < 0 {
                                let fail = Fail::input(format!(
                                    "[{i}] counts from the end of the array at {path}"
                                ));
                                return self.stop(
                                    Verdict::FromEnd {
                                        index: *i,
                                        at: path,
                                    },
                                    fail,
                                );
                            }
                        }
                    }
                    self.frames.push(Frame {
                        kind,
                        count: 0,
                        on_path,
                        matched: false,
                    });
                } else if on_path && !is_target {
                    self.nearest = Some(Nearest {
                        path,
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
pub(crate) struct Rows<S> {
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
    pub(crate) fn new(inner: S) -> Rows<S> {
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
            what: What::Render(renderer),
            path: crate::headless::parse_path(path).unwrap(),
            compact: true,
            indent: 2,
            timeout: None,
            started: None,
            max_output: None,
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

    /// What a renderer hands the pipe, in pieces cut wherever its buffer
    /// filled, reaches the output a whole record at a time: fed `text` up
    /// to any point, the pipe has written up to the last record's end it
    /// could see there, and the run's end, its flush, writes the rest
    /// (aless#17). A JSON element is seen to end at the comma after it.
    #[test]
    fn the_pipe_writes_whole_records() {
        let csv = "a,\"b\nc\",\"d\"\"e\"\r\n1,2,3\r\n4,\"5,6\",7\r\n8,9";
        let json = "[\n  {\"a\": \"x,]}\\\"\"},\n  [1, 2],\n  {\"b\": {}}\n]\n";
        let object = "{\"a\": [1, 2], \"b\": \"c,d\"}";
        let lines = "{\"a\":1}\n{\"a\":2}\n{\"a\"";
        let after = |text: &str, part: &str| text.find(part).unwrap() + part.len();
        // (where a record ends, how far the pipe must have read to know)
        let at = |end: usize| (end, end);
        let comma = |end: usize| (end, end + 1);
        let cases = [
            (
                Records::Csv,
                csv,
                vec![
                    at(after(csv, "\"e\"\r\n")),
                    at(after(csv, "3\r\n")),
                    at(after(csv, "7\r\n")),
                ],
            ),
            (
                Records::Json,
                json,
                vec![
                    comma(after(json, "\"\"}")),
                    comma(after(json, "[1, 2]")),
                    at(after(json, "{}}\n]")),
                ],
            ),
            (
                Records::Json,
                object,
                vec![comma(after(object, "[1, 2]")), at(object.len())],
            ),
            (
                Records::Lines,
                lines,
                vec![at(after(lines, ":1}\n")), at(after(lines, ":2}\n"))],
            ),
        ];
        for (records, text, ends) in cases {
            for piece in [1, 2, 3, 7, 64] {
                for upto in 0..=text.len() {
                    let out = Shared::default();
                    let inner: Box<dyn Write + Send> = Box::new(out.clone());
                    let written = Arc::new(AtomicU64::new(0));
                    let broken = Arc::new(AtomicBool::new(false));
                    let mut pipe =
                        Pipe::new(Arc::new(Mutex::new(inner)), &written, &broken, records);
                    for chunk in text.as_bytes()[..upto].chunks(piece) {
                        assert_eq!(pipe.write(chunk).unwrap(), chunk.len());
                    }
                    let whole = ends
                        .iter()
                        .filter(|&&(_, seen)| seen <= upto)
                        .map(|&(end, _)| end)
                        .max()
                        .unwrap_or(0);
                    assert_eq!(
                        out.text(),
                        text[..whole],
                        "{records:?} {text:?} to {upto} by {piece}"
                    );
                    assert_eq!(written.load(Ordering::Relaxed), whole as u64);
                    pipe.flush().unwrap();
                    assert_eq!(out.text(), text[..upto], "{records:?} flushed at {upto}");
                }
            }
        }
    }

    /// A record too long to hold is written up to the end of one of its
    /// own values where it has them (JSON), else as it comes (CSV), so the
    /// pipe never holds much more than its limit.
    #[test]
    fn a_record_too_long_to_hold_is_written_as_it_comes() {
        let json = "{\"a\": [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]";
        let csv = "x,\"0123456789abcdefghijklmnop";
        for (records, text) in [(Records::Json, json), (Records::Csv, csv)] {
            let out = Shared::default();
            let inner: Box<dyn Write + Send> = Box::new(out.clone());
            let w = Arc::new(AtomicU64::new(0));
            let b = Arc::new(AtomicBool::new(false));
            let mut pipe = Pipe::new(Arc::new(Mutex::new(inner)), &w, &b, records);
            pipe.hold_max = 8;
            for chunk in text.as_bytes().chunks(3) {
                pipe.write_all(chunk).unwrap();
            }
            let written = out.text();
            assert!(text.starts_with(&written), "{records:?}: {written:?}");
            assert!(written.len() > 8, "{records:?}: something was let go");
            assert!(
                text.len() - written.len() <= 8 + 3,
                "{records:?}: {written:?}"
            );
            if records == Records::Json {
                // Up to an element's end: the comma after it is still held.
                assert!(text[written.len()..].starts_with(','), "{written:?}");
            }
        }
    }

    /// A run that fails part way leaves standard output at the end of a
    /// record, past the renderer's buffer: every CSV row whole, every JSON
    /// element whole, and `output` says partial (aless#17).
    #[test]
    fn a_failure_part_way_leaves_whole_records() {
        let mut text: String = (0..3_000)
            .map(|i| format!("{{\"id\": {i}, \"name\": \"n{i}\", \"v\": {i}}}\n"))
            .collect();
        text.push_str("{\"id\": 3000, \"name\": oops}\n");
        let (r, out) = run_lines(&job(Format::Jsonl, Renderer::Csv, "."), &text);
        assert!(
            matches!(r, Err(ExportError::Transduce(ref f)) if f.committed_output),
            "{r:?}"
        );
        assert!(
            out.len() > 32 * 1024,
            "past the renderer's buffer: {}",
            out.len()
        );
        assert!(out.ends_with("\r\n"), "{:?}", &out[out.len() - 40..]);
        for row in out.lines() {
            assert_eq!(row.split(',').count(), 3, "{row:?}");
        }
        let (r, out) = run_lines(&job(Format::Jsonl, Renderer::Json, "."), &text);
        assert!(r.is_err());
        assert!(out.len() > 32 * 1024, "{}", out.len());
        let closed: Value = serde_json::from_str(&format!("{out}]"))
            .unwrap_or_else(|e| panic!("{e}: {:?}", &out[out.len() - 80..]));
        let rows = closed.as_array().unwrap();
        assert!(
            rows.iter().all(|r| r["v"].is_number()),
            "every element is whole"
        );
    }

    fn events(json: &str) -> Vec<OwnedJsonEvent> {
        let value = tabnas_json::parse(json).unwrap();
        let mut rec = Vec::new();
        tabnas_transduce::ValueSource(&value).run(&mut rec).unwrap();
        rec
    }

    /// The events as the parse emits them, a repeated key included: the
    /// grammar's value, which [`events`] walks, keeps only the last.
    fn streamed(json: &str) -> Vec<OwnedJsonEvent> {
        let (r, rec) = ParserSource::new(tabnas_json::make(), json)
            .mode(SourceMode::Incremental {
                prune: Prune::Never,
            })
            .grammar("json")
            .run_owned(Vec::new());
        r.unwrap();
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
        // Verified, but they may read a streamed container back.
        for f in [Format::Yaml, Format::Jsonic, Format::Markdown] {
            assert_eq!(
                plan(f, true),
                Some(Plan::Incremental { prune: false }),
                "{f}"
            );
        }
        for f in [Format::Toml, Format::Ini, Format::Xml, Format::Feed] {
            assert_eq!(plan(f, true), Some(Plan::Materialize), "{f}");
        }
        assert_eq!(plan(Format::Text, true), None);
        // Every name is a part's, the built-ins' among them, since their
        // manifests name aless's own renderers.
        assert_eq!(Renderer::from_name(" CSV "), Some(Renderer::Part("csv")));
        assert_eq!(Renderer::from_name("json"), Some(Renderer::Part("json")));
        assert_eq!(Renderer::from_name("rss"), None);
        assert_eq!(Renderer::from_name(" YAML "), Some(Renderer::Part("yaml")));
        assert_eq!(Renderer::Part("yaml").name(), "yaml");
        assert_eq!(Renderer::from_name("toml"), Some(Renderer::Part("toml")));
        assert_eq!(Renderer::from_name("tsv"), None, "TSV is written as csv");
    }

    #[test]
    fn the_scope_re_roots_the_stream_at_the_path() {
        let doc = events(r#"{"a": [1, {"b": [true, "x"]}], "c": null}"#);
        let scoped = |path: &str| {
            let mut scope = Scope::new(crate::headless::parse_path(path).unwrap(), Vec::new());
            let r = replay(&doc, &mut scope);
            let missing = match scope.verdict() {
                Some(Verdict::Missing(near)) => Some(near),
                Some(other) => panic!("{other:?}"),
                None => None,
            };
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
    fn the_scope_refuses_what_a_stream_cannot_do() {
        let scoped = |path: &str, json: &str| {
            let mut scope = Scope::new(crate::headless::parse_path(path).unwrap(), Vec::new());
            let r = replay(&streamed(json), &mut scope);
            let verdict = scope.verdict();
            (r, scope.inner, verdict)
        };
        // An index on an object is its key, as --path reads it everywhere;
        // on an array a negative one counts from the end, which needs the
        // whole array.
        let (r, got, verdict) = scoped("[-1]", r#"{"-1": 5, "x": 6}"#);
        assert_eq!(r.unwrap(), Flow::Continue);
        assert_eq!(got, streamed("5"));
        assert_eq!(verdict, None);
        let (r, got, verdict) = scoped(".a[-1].b", r#"{"a": [{"b": 1}, {"b": 2}]}"#);
        assert_eq!(r.unwrap_err().code, Code::InputInvalid);
        assert!(got.is_empty());
        assert_eq!(
            verdict,
            Some(Verdict::FromEnd {
                index: -1,
                at: ".a".into()
            })
        );
        // A key on the path repeated after a value under it was taken.
        let (r, got, verdict) = scoped(".rows", r#"{"rows": [1], "rows": [2]}"#);
        let fail = r.unwrap_err();
        assert_eq!(fail.code, Code::DuplicateMember);
        assert_eq!(fail.path.as_deref(), Some("."));
        assert!(fail.message.contains("\"rows\""), "{}", fail.message);
        let mut first = streamed("[1]");
        assert_eq!(first.pop(), Some(OwnedJsonEvent::End));
        assert_eq!(got, first, "the first was streamed, and no end");
        assert!(matches!(verdict, Some(Verdict::Duplicate(f)) if f == fail));
        let (r, _, _) = scoped(
            ".a.rows",
            r#"{"a": {"rows": [1]}, "b": 1, "a": {"rows": [2]}}"#,
        );
        let fail = r.unwrap_err();
        assert_eq!(fail.code, Code::DuplicateMember);
        assert!(fail.message.contains("\"a\" of ."), "{}", fail.message);
        // Repeated keys off the path, and inside the target, are the
        // stream's to pass on. (Built by hand: the incremental source
        // misplaces the values after a repeated key off the path, which is
        // its defect, not the scope's; see the agents guide.)
        use OwnedJsonEvent as E;
        let n = |v: f64| E::Number {
            value: v,
            lexeme: None,
        };
        let k = |s: &str| E::Key(s.into());
        let doc = vec![
            E::ObjectStart,
            k("x"),
            n(1.0),
            k("x"),
            n(2.0),
            k("rows"),
            E::ArrayStart,
            E::ObjectStart,
            k("k"),
            n(1.0),
            k("k"),
            n(2.0),
            E::ObjectEnd,
            E::ArrayEnd,
            E::ObjectEnd,
            E::End,
        ];
        let mut scope = Scope::new(crate::headless::parse_path(".rows").unwrap(), Vec::new());
        assert_eq!(replay(&doc, &mut scope).unwrap(), Flow::Continue);
        assert_eq!(
            scope.inner,
            doc[6..14]
                .iter()
                .cloned()
                .chain([E::End])
                .collect::<Vec<_>>()
        );
    }

    /// The source's flag only where no guard will raise it; the program's
    /// flag in every mode, since a program at work on one item is inside
    /// a parser event, where the guard cannot run.
    #[test]
    fn the_alarm_raises_the_sources_flag_only_where_no_guard_will() {
        let alarm = Alarm {
            lines: false,
            parsed: Arc::new(AtomicBool::new(false)),
            abort: AbortFlag::new(),
            program: AbortFlag::new(),
        };
        alarm.fire();
        assert!(!alarm.abort.is_aborted(), "the parser's guard has it");
        assert!(
            alarm.program.is_aborted(),
            "the program, mid-item, has nothing else"
        );
        alarm.parsed.store(true, Ordering::Relaxed);
        alarm.fire();
        assert!(alarm.abort.is_aborted(), "the walk after the parse");
        let alarm = Alarm {
            lines: true,
            parsed: Arc::new(AtomicBool::new(false)),
            abort: AbortFlag::new(),
            program: AbortFlag::new(),
        };
        alarm.fire();
        assert!(alarm.abort.is_aborted(), "a line-by-line read");
        assert!(alarm.program.is_aborted());
    }

    #[test]
    fn a_grammar_panic_reports_as_the_loader_reports_one() {
        let j = job(Format::Json, Renderer::Json, ".");
        let failed = Failed {
            why: Outcome::Panicked("boom".into()),
            stop: None,
            verdict: None,
            from_sink: false,
            lines: false,
        };
        match classify(&j, failed, &AtomicU64::new(0), &AtomicBool::new(false)) {
            ExportError::Load { error, partial } => {
                assert_eq!(error.code, "grammar");
                assert_eq!(error.message, "json grammar failed: boom");
                assert!(error.plain_report().contains("(stdin)"), "{}", error.report);
                assert!(!partial);
            }
            other => panic!("{other:?}"),
        }
        // A panic on the parse thread is caught, whichever source runs.
        assert_eq!(load::catch_grammar(|| panic!("x")).unwrap_err(), "x");
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
        // A path counting from the end of an array: known only when the
        // container turns out to be one.
        let j = job(Format::Json, Renderer::Csv, "[-1]");
        assert!(matches!(
            run_text(&j, "[[1], [2]]").0.unwrap_err(),
            ExportError::Usage(m) if m.contains("[-1]") && m.contains("the array at .")
        ));
        let j = job(Format::Json, Renderer::Json, "[-1]");
        let (r, out) = run_text(&j, r#"{"-1": [1]}"#);
        r.unwrap();
        assert_eq!(out, "[1]\n", "on an object, the key");
        // A repeated key on the path, after the first was taken.
        let j = job(Format::Json, Renderer::Csv, ".rows");
        let (r, out) = run_text(&j, r#"{"rows": [{"v": 1}], "rows": [{"v": 2}]}"#);
        match r.unwrap_err() {
            ExportError::Transduce(f) => {
                assert_eq!(f.code, Code::DuplicateMember);
                assert_eq!(f.path.as_deref(), Some("."));
                assert!(!f.committed_output, "held back by the renderer");
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(out, "");
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

    /// The document as `--json --compact` prints it, newline included.
    fn whole(text: &str, format: Format) -> String {
        let doc = load::parse(text, format).unwrap();
        format!("{}\n", fmt::to_json_compact(&doc, 0))
    }

    #[test]
    fn a_stream_refused_part_way_falls_back_to_the_whole_value() {
        // A jsonic top-level implicit list whose first element is a
        // container, and a YAML stream of documents: the incremental
        // source streams the first value as the root, then finds it
        // wrapped, and refuses. Nothing had reached the output, so the
        // export runs again from the whole value, and agrees with --json.
        for (format, text) in [
            (Format::Jsonic, "{a:1}\n{b:2}\n"),
            (Format::Yaml, "a: 1\n---\nb: 2\n"),
            // A YAML merge key rewrites the map after it was streamed.
            (
                Format::Yaml,
                "base: &b\n  x: 1\nderived:\n  <<: *b\n  y: 2\n",
            ),
            // A repeated member whose values the grammar merges.
            (Format::Json5, "{a: {x: 1}, a: {y: 2}}"),
        ] {
            let j = job(format, Renderer::Json, ".");
            let (r, out) = run_text(&j, text);
            r.unwrap_or_else(|e| panic!("{format}: {e:?}"));
            assert_eq!(out, whole(text, format), "{format}: {text:?}");
        }
        // Rows too: the CSV chain is rebuilt for the second run. Here the
        // rows sink refuses the streamed root (an object) before the
        // grammar wraps it, which for these two grammars is not final.
        for (format, text) in [
            (Format::Jsonic, "{a:1}\n{a:2}\n"),
            (Format::Yaml, "a: 1\n---\na: 2\n"),
        ] {
            let j = job(format, Renderer::Csv, ".");
            let (r, out) = run_text(&j, text);
            r.unwrap_or_else(|e| panic!("{format}: {e:?}"));
            assert_eq!(out, "\"a\"\r\n\"1\"\r\n\"2\"\r\n", "{format}");
        }
        // A root that really is an object still fails, once for a grammar
        // that could not have wrapped it and twice over for one that could.
        for format in [Format::Json, Format::Jsonic] {
            let j = job(format, Renderer::Csv, ".");
            match run_text(&j, r#"{"a": 1}"#).0.unwrap_err() {
                ExportError::Transduce(f) => {
                    assert_eq!(f.code, Code::InputInvalid, "{format}");
                    assert!(f.message.contains("--path"), "{format}: {}", f.message);
                }
                other => panic!("{format}: {other:?}"),
            }
        }
        // Once output has left, a refusal stays a refusal, and says so: the
        // first member is whole, and written, before the second ends.
        let long = "x".repeat(2 * tabnas_render::DEFAULT_BUDGET);
        let j = job(Format::Jsonic, Renderer::Json, ".");
        let (r, out) = run_text(&j, &format!("{{a:'{long}', b:'{long}'}}\n{{c:2}}\n"));
        match r.unwrap_err() {
            ExportError::Transduce(f) => {
                assert_eq!(f.code, Code::StreamabilityUnknown);
                assert!(f.committed_output);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(
            out,
            format!("{{\"a\":\"{long}\""),
            "the record that was whole"
        );
        // A record still being written when the refusal came was held back,
        // so nothing had left, and the whole value is written instead.
        let one = format!("{{a:'{long}'}}\n{{b:2}}\n");
        let (r, out) = run_text(&j, &one);
        r.unwrap();
        assert_eq!(out, whole(&one, Format::Jsonic));
    }

    /// A program's own refusal to stream, raised by its sink before
    /// anything was written, is the program's, as [`classify`] reports
    /// it: the whole value would meet the same program and the same
    /// refusal, so the run is not made a second time over it. The chain
    /// is built once. (A refusal of the source's, above, still falls
    /// back.)
    #[test]
    fn a_programs_own_refusal_does_not_run_it_again_over_the_whole_value() {
        struct Refuses;
        impl Sink for Refuses {
            fn event(&mut self, _ev: JsonEvent<'_>) -> Result<Flow, Fail> {
                Err(Fail::new(
                    Code::StreamabilityUnknown,
                    "the program's step cannot stream this",
                ))
            }
        }
        for what in [What::Program { rows: None }, What::Part] {
            let mut j = job(Format::Json, Renderer::Json, ".");
            j.what = what.clone();
            let builds = std::sync::atomic::AtomicUsize::new(0);
            let r = run_program(
                &j,
                Input::Text("[1, 2, 3]"),
                Box::new(Shared::default()),
                Records::Json,
                |_out, _abort| {
                    builds.fetch_add(1, Ordering::Relaxed);
                    Ok(Box::new(Refuses))
                },
            );
            match r.unwrap_err() {
                ExportError::Program(f) => {
                    assert_eq!(f.code, Code::StreamabilityUnknown, "{what:?}")
                }
                other => panic!("{what:?}: {other:?}"),
            }
            assert_eq!(builds.load(Ordering::Relaxed), 1, "{what:?}: chains built");
        }
    }

    /// A JSON5 string continued across a CRLF is exported as across an LF
    /// (#30): the export reads the text the loader reads.
    #[test]
    fn json5_exports_a_continuation_across_crlf() {
        let j = job(Format::Json5, Renderer::Json, ".");
        let lf = "['a\\\nb', {k: 'c\\\nd'}]\n";
        let (r, one) = run_text(&j, lf);
        r.unwrap();
        let (r, two) = run_text(&j, &lf.replace('\n', "\r\n"));
        r.unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(two, one);
        assert!(one.contains(r#""ab""#) && one.contains(r#""cd""#), "{one}");
    }

    #[test]
    fn the_deadline_covers_the_walk_after_a_parse() {
        // A sink slow enough that a small, quickly parsed TOML takes far
        // longer to walk than the deadline allows; the parse itself may
        // still run past a busy machine's deadline, and is then stopped
        // by the guard, which is a timeout too. Either way the run ends
        // as a timeout rather than a success.
        struct Slow;
        impl Sink for Slow {
            fn event(&mut self, _ev: JsonEvent<'_>) -> Result<Flow, Fail> {
                std::thread::sleep(Duration::from_millis(10));
                Ok(Flow::Continue)
            }
        }
        let toml: String = (0..60).map(|i| format!("k{i} = {i}\n")).collect();
        let mut j = job(Format::Toml, Renderer::Json, ".");
        j.timeout = Some(Duration::from_millis(100));
        let failed = run(
            &j,
            Input::Text(&toml),
            Scope::new(Vec::new(), Slow),
            SourceMode::Materialize,
            AbortFlag::new(),
            AbortFlag::new(),
        )
        .unwrap_err();
        match (&failed.why, failed.stop.as_ref().map(|s| s.why())) {
            (Outcome::Failed(fail), Some(0)) => {
                assert_eq!(fail.code, Code::Aborted, "the alarm stopped the walk");
            }
            (Outcome::Failed(_), Some(STOP_TIME)) => {}
            other => panic!("{:?}", other.1),
        }
        match classify(&j, *failed, &AtomicU64::new(0), &AtomicBool::new(false)) {
            ExportError::Load { error, .. } => assert_eq!(error.code, "timeout"),
            other => panic!("{other:?}"),
        }
        // With time enough the same walk finishes.
        j.timeout = Some(Duration::from_secs(120));
        run(
            &j,
            Input::Text("a = 1\n"),
            Scope::new(Vec::new(), Slow),
            SourceMode::Materialize,
            AbortFlag::new(),
            AbortFlag::new(),
        )
        .unwrap();
    }

    /// The deadline reaches a program's sink in every mode. A sink that
    /// works on one item until its flag is raised stands for a program
    /// slow per item: under the incremental source, where the parser is
    /// inside an event while the sink works and no guard of aless's runs,
    /// the alarm must raise the program's flag, or the run would last as
    /// long as the item's work. The run ends a moment after the deadline,
    /// as a timeout worded for the run and with no position: the sink's
    /// `Aborted` is not the parse's stop, and the program's own span, if
    /// a sink stamps one on it, is no position in the input.
    #[test]
    fn the_alarm_reaches_a_program_slow_on_one_item_in_every_mode() {
        struct UntilAborted {
            program: AbortFlag,
            waited: Arc<AtomicBool>,
        }
        impl Sink for UntilAborted {
            fn event(&mut self, _ev: JsonEvent<'_>) -> Result<Flow, Fail> {
                let started = std::time::Instant::now();
                // Work on the item, reading the flag as a program does
                // between steps; give up after a while so a failure of
                // this test ends.
                while !self.program.is_aborted() {
                    if started.elapsed() > Duration::from_secs(10) {
                        return Ok(Flow::Continue);
                    }
                    std::thread::sleep(Duration::from_millis(2));
                }
                self.waited.store(true, Ordering::Relaxed);
                // Stamped with a span of the program's, as alchemy's
                // evaluator stamps every failure it returns.
                let mut fail = Fail::aborted();
                fail.row = Some(3);
                fail.column = Some(25);
                Err(fail)
            }
        }
        for (format, mode) in [
            (
                Format::Json,
                SourceMode::Incremental {
                    prune: Prune::Never,
                },
            ),
            (Format::Toml, SourceMode::Materialize),
        ] {
            let text = match format {
                Format::Json => "[1, 2, 3]",
                _ => "a = 1\n",
            };
            let mut j = job(format, Renderer::Json, ".");
            j.what = What::Program { rows: None };
            // A deadline a debug build meets for a one-line document on a slow
            // runner too, with the tests running in parallel (a grammar is
            // built first); the sink then outlasts it by design.
            j.timeout = Some(Duration::from_millis(2000));
            let program = AbortFlag::new();
            let waited = Arc::new(AtomicBool::new(false));
            let started = std::time::Instant::now();
            let failed = run(
                &j,
                Input::Text(text),
                Scope::new(
                    Vec::new(),
                    UntilAborted {
                        program: program.clone(),
                        waited: waited.clone(),
                    },
                ),
                mode,
                AbortFlag::new(),
                program,
            )
            .unwrap_err();
            assert!(
                started.elapsed() < Duration::from_secs(6),
                "{format}: the run went on {:?} past the deadline",
                started.elapsed()
            );
            assert!(
                waited.load(Ordering::Relaxed),
                "{format}: the alarm raised the flag"
            );
            assert!(failed.from_sink, "{format}: the sink's failure");
            match classify(&j, *failed, &AtomicU64::new(0), &AtomicBool::new(false)) {
                ExportError::Load { error, partial } => {
                    assert_eq!(error.code, "timeout", "{format}");
                    assert!(!partial, "{format}");
                    assert!(
                        error
                            .message
                            .contains("the run (the parse and the program)"),
                        "{format}: {}",
                        error.message
                    );
                    assert_eq!(
                        (error.line, error.col),
                        (0, 0),
                        "{format}: the program's span is no position in the input"
                    );
                    let report = error.plain_report();
                    assert!(
                        report.contains("--> (stdin)\n") && !report.contains("--> (stdin):"),
                        "{format}: {report}"
                    );
                    assert!(
                        !error.hint.contains("got this far"),
                        "{format}: the hint claims no position: {}",
                        error.hint
                    );
                }
                other => panic!("{format}: {other:?}"),
            }
        }
        // The same sink, the same document, under a render: nothing reads
        // the program's flag, and the run ends with the parse and the walk.
        let mut j = job(Format::Json, Renderer::Json, ".");
        j.timeout = Some(Duration::from_millis(100));
        match run_text(&j, "[1, 2, 3]").0 {
            Ok(()) => {}
            Err(ExportError::Load { error, .. }) => {
                assert_eq!(error.code, "timeout", "a busy machine's deadline");
                assert!(error.message.starts_with("timeout: the parse ran longer"));
            }
            Err(other) => panic!("{other:?}"),
        }
    }

    /// [`materialize`] with a custom grammar's parse that comes to its end
    /// after the deadline, classified as the run would be. The deadline
    /// has passed before the parse starts, and the guard is set without
    /// it: that stands for a last step that ran past the time with no step
    /// left for the guard to look at it between (the engine reads trailing
    /// space after its last check).
    fn late_custom_parse(format: Format, text: &str) -> ExportError {
        let mut j = job(format, Renderer::Json, ".");
        j.timeout = Some(Duration::from_millis(1));
        let ran = load::on_parse_thread(
            j.timeout,
            None,
            || {},
            |deadline| {
                let d = deadline.clone().expect("a deadline");
                while !d.passed() {
                    std::thread::sleep(Duration::from_millis(1));
                }
                let mut parser = load::make_parser(format).unwrap().unwrap();
                let stop = load::guard(&mut parser, None, load::MAX_RULE_DEPTH, || {});
                materialize(
                    parser,
                    format,
                    text,
                    deadline,
                    &AbortFlag::new(),
                    &AtomicBool::new(false),
                    Scope::new(Vec::new(), Vec::<OwnedJsonEvent>::new()),
                    stop,
                )
            },
        );
        let failed = Failed {
            why: ran.outcome,
            stop: ran.stop,
            verdict: ran.verdict,
            from_sink: ran.from_sink,
            lines: false,
        };
        classify(&j, failed, &AtomicU64::new(0), &AtomicBool::new(false))
    }

    /// A parse that finishes late fails on the time, as the loader has
    /// it, with a value or an error: a custom grammar's value that also
    /// nests past the cap is a `timeout` (status 6), not `too_deep`, and
    /// neither is an input error found late the input's fault. On time,
    /// the same value is `too_deep`.
    #[test]
    fn a_late_custom_parse_is_a_timeout_before_its_depth() {
        use crate::grammar::{self, Definition};
        let def = Definition::parse(
            "--grammar-expr",
            "impl-late-nest=doc = \"(\" doc \")\" / \"x\"\n",
        )
        .unwrap();
        let format = Format::Custom(grammar::register(def, crate::load::Limits::NONE).unwrap());
        let deep = "(".repeat(600) + "x" + &")".repeat(600);
        for text in [deep.as_str(), "x)"] {
            match late_custom_parse(format, text) {
                ExportError::Load { error, partial } => {
                    assert_eq!(error.code, "timeout", "{text:.8}: {}", error.message);
                    assert!(!partial);
                }
                other => panic!("{text:.8}: {other:?}"),
            }
        }
        let mut j = job(format, Renderer::Json, ".");
        j.timeout = Some(Duration::from_secs(120));
        match run_text(&j, &deep).0.unwrap_err() {
            ExportError::Load { error, .. } => assert_eq!(error.code, "too_deep"),
            other => panic!("{other:?}"),
        }
        match run_text(&j, "x)").0.unwrap_err() {
            ExportError::Transduce(f) => assert_eq!(f.code, Code::InputInvalid),
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
