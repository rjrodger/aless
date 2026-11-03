//! Format detection and loading: a file or string goes in, a positioned [`Doc`] comes out,
//! parsed by its format's tabnas grammar or split into lines.

use std::cell::Cell;
use std::collections::HashMap;
use std::fmt;
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tabnas::Tabnas;

use crate::doc::Doc;
use crate::grammar::{self, CustomId};
use crate::prov;

/// A format aless reads: a built-in grammar, plain text, or a grammar from the command line.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Format {
    Json,
    Jsonl,
    Jsonic,
    Jsonc,
    Json5,
    Yaml,
    Toml,
    Ini,
    Csv,
    Tsv,
    Xml,
    Zon,
    Markdown,
    Feed,
    Css,
    Proto,
    /// Named by its manifest's `languageId`, not after the tabnas-chess crate.
    Pgn,
    C,
    Abnf,
    Ebnf,
    Gbnf,
    Expr,
    Semver,
    Text,
    Custom(CustomId),
}

impl Format {
    /// The built-in formats; custom grammars are in the [`grammar`] registry.
    pub const ALL: [Format; 24] = [
        Format::Json,
        Format::Jsonl,
        Format::Jsonic,
        Format::Jsonc,
        Format::Json5,
        Format::Yaml,
        Format::Toml,
        Format::Ini,
        Format::Csv,
        Format::Tsv,
        Format::Xml,
        Format::Zon,
        Format::Markdown,
        Format::Feed,
        Format::Css,
        Format::Proto,
        Format::Pgn,
        Format::C,
        Format::Abnf,
        Format::Ebnf,
        Format::Gbnf,
        Format::Expr,
        Format::Semver,
        Format::Text,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Format::Json => "json",
            Format::Jsonl => "jsonl",
            Format::Jsonic => "jsonic",
            Format::Jsonc => "jsonc",
            Format::Json5 => "json5",
            Format::Yaml => "yaml",
            Format::Toml => "toml",
            Format::Ini => "ini",
            Format::Csv => "csv",
            Format::Tsv => "tsv",
            Format::Xml => "xml",
            Format::Zon => "zon",
            Format::Markdown => "markdown",
            Format::Feed => "feed",
            Format::Css => "css",
            Format::Proto => "proto",
            Format::Pgn => "pgn",
            Format::C => "c",
            Format::Abnf => "abnf",
            Format::Ebnf => "ebnf",
            Format::Gbnf => "gbnf",
            Format::Expr => "expr",
            Format::Semver => "semver",
            Format::Text => "text",
            Format::Custom(id) => grammar::get(id).map_or("custom", |g| g.name),
        }
    }

    pub fn is_custom(self) -> bool {
        matches!(self, Format::Custom(_))
    }

    /// A grammar notation, read as the spec its compiler writes ([`compile_notation`]). The
    /// compiler uses its own parser, so the value has no source positions and no colours.
    pub fn is_notation(self) -> bool {
        matches!(self, Format::Abnf | Format::Ebnf | Format::Gbnf)
    }

    /// Records keyed by a header row, so no record carries its keys in the source.
    pub fn keyed_by_header(&self) -> bool {
        matches!(self, Format::Csv | Format::Tsv)
    }

    /// A format by name or alias (`yml`, `md`); a custom grammar's name wins over a built-in's.
    pub fn from_name(s: &str) -> Option<Format> {
        let s = s.trim().to_lowercase();
        grammar::lookup(&s)
            .map(Format::Custom)
            .or_else(|| Format::ALL.iter().copied().find(|f| f.name() == s))
            .or_else(|| Format::from_extension(&s))
    }

    /// Every name [`from_name`](Format::from_name) takes as a format's own, each once.
    pub fn known_names() -> Vec<&'static str> {
        let mut names: Vec<&'static str> = Format::ALL.iter().map(|f| f.name()).collect();
        for name in grammar::names() {
            if !names
                .iter()
                .any(|n| n.to_lowercase() == name.to_lowercase())
            {
                names.push(name);
            }
        }
        names
    }

    /// The extensions (lower case, no dot) the format's crate manifest declares; none for expr
    /// and semver, which `-k` names. The help's table of formats is this one.
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Format::Json => &["json", "geojson", "har", "jsonld", "webmanifest"],
            Format::Jsonl => &["jsonl", "ndjson"],
            Format::Jsonic => &["jsonic"],
            Format::Jsonc => &["jsonc"],
            Format::Json5 => &["json5"],
            Format::Yaml => &["yaml", "yml"],
            Format::Toml => &["toml"],
            Format::Ini => &["ini", "cfg", "conf", "cnf"],
            Format::Csv => &["csv"],
            Format::Tsv => &["tsv", "tab"],
            Format::Xml => &["xml", "svg", "xhtml", "xsd", "xsl", "xslt", "plist"],
            Format::Zon => &["zon"],
            Format::Markdown => &["md", "markdown"],
            Format::Feed => &["rss", "atom"],
            Format::Css => &["css"],
            Format::Proto => &["proto"],
            Format::Pgn => &["pgn"],
            Format::C => &["c", "h"],
            Format::Abnf => &["abnf"],
            Format::Ebnf => &["ebnf"],
            Format::Gbnf => &["gbnf"],
            Format::Expr | Format::Semver => &[],
            Format::Text => &["txt", "text", "log"],
            Format::Custom(_) => &[],
        }
    }

    /// The format an extension implies: a custom grammar of that name, else a built-in.
    pub fn from_extension(ext: &str) -> Option<Format> {
        if let Some(id) = grammar::lookup(ext) {
            return Some(Format::Custom(id));
        }
        let ext = ext.to_ascii_lowercase();
        Format::ALL
            .into_iter()
            .find(|f| f.extensions().contains(&ext.as_str()))
    }

    /// The format of a path: a custom grammar named by the whole file name (`/etc/hosts`), else
    /// the extension's; plain text when neither says.
    pub fn detect(path: &Path) -> Format {
        Format::detect_known(path).unwrap_or(Format::Text)
    }

    /// [`detect`](Format::detect), or `None` when nothing claims the path.
    pub fn detect_known(path: &Path) -> Option<Format> {
        let whole = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(grammar::lookup)
            .map(Format::Custom);
        whole.or_else(|| {
            path.extension()
                .and_then(|e| e.to_str())
                .and_then(Format::from_extension)
        })
    }
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Why a load failed: an I/O error (no position) or a parse error at a 1-based line and column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadError {
    /// One line, for the status bar.
    pub message: String,
    /// The grammar's error code, or aless's own for an error it raised itself.
    pub code: String,
    pub line: u32,
    pub col: u32,
    /// The grammar's advice, or empty. Boxed, as is `source_line`, to keep the error small.
    pub hint: Box<str>,
    /// The error's source line, cut to a window around the column when long.
    pub source_line: Option<Box<str>>,
    /// The report as the engine renders it, its ANSI colour codes styled when drawn.
    pub report: String,
}

/// The largest input read unless `--max-size` says otherwise: some 2.7 GB of parse memory.
pub const DEFAULT_MAX_SIZE: u64 = 64 << 20;

/// Roughly the bytes of memory a parse takes per byte of input, measured on JSON records.
pub const MEMORY_PER_BYTE: u64 = 40;

/// What a load may cost: how much input it reads, and how long it parses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Refuse an input larger than this many bytes (`--max-size`).
    pub max_size: Option<u64>,
    /// Stop a parse that runs longer than this (`--timeout`).
    pub timeout: Option<Duration>,
}

impl Limits {
    /// [`DEFAULT_MAX_SIZE`], and no time limit: a fair one depends on the machine.
    pub const DEFAULT: Limits = Limits {
        max_size: Some(DEFAULT_MAX_SIZE),
        timeout: None,
    };

    pub const NONE: Limits = Limits {
        max_size: None,
        timeout: None,
    };

    /// The limits [`set_limits`] last set for this process.
    pub fn current() -> Limits {
        let set = |n: u64| (n > 0).then_some(n);
        Limits {
            max_size: set(MAX_SIZE.load(Ordering::Relaxed)),
            timeout: set(TIMEOUT_MS.load(Ordering::Relaxed)).map(Duration::from_millis),
        }
    }
}

impl Default for Limits {
    fn default() -> Limits {
        Limits::DEFAULT
    }
}

/// The limits of [`Limits::current`]; 0 for none.
static MAX_SIZE: AtomicU64 = AtomicU64::new(DEFAULT_MAX_SIZE);
static TIMEOUT_MS: AtomicU64 = AtomicU64::new(0);

/// Set the limits [`load_path`], [`load_str`] and [`parse`] apply from now on.
pub fn set_limits(limits: Limits) {
    MAX_SIZE.store(limits.max_size.unwrap_or(0), Ordering::Relaxed);
    TIMEOUT_MS.store(timeout_ms(limits.timeout), Ordering::Relaxed);
}

/// A timeout as [`TIMEOUT_MS`] holds it: milliseconds, never under 1, as 0 is no limit.
fn timeout_ms(timeout: Option<Duration>) -> u64 {
    timeout.map_or(0, |t| t.as_millis().clamp(1, u128::from(u64::MAX)) as u64)
}

/// A time as `--timeout` takes it: seconds, fractions allowed, with an
/// optional `s`, or minutes with `m`; 0 for no limit.
pub fn parse_timeout(text: &str) -> Result<Option<Duration>, String> {
    let t = text.trim();
    let bad = || format!("--timeout needs seconds such as 30, 2.5 or 0, not {text:?}");
    let (number, scale) = match t.strip_suffix('m') {
        Some(n) => (n, 60.0),
        None => (t.strip_suffix('s').unwrap_or(t), 1.0),
    };
    let secs: f64 = number.trim().parse().map_err(|_| bad())?;
    if !secs.is_finite() || secs < 0.0 {
        return Err(bad());
    }
    if secs == 0.0 {
        return Ok(None);
    }
    Duration::try_from_secs_f64(secs * scale)
        .map(Some)
        .map_err(|_| bad())
}

/// Seconds as a report writes them: `30`, `2.5`.
fn seconds(t: Duration) -> String {
    t.as_secs_f64().to_string()
}

/// A size as `--max-size` takes it: bytes, or with a `K`, `M` or `G` suffix counted in 1024s
/// (`kb` and `mib` alike); 0 for no limit.
pub fn parse_size(text: &str) -> Result<Option<u64>, String> {
    parse_size_for("--max-size", text)
}

/// [`parse_size`] for the option `flag`, which its error names.
pub fn parse_size_for(flag: &str, text: &str) -> Result<Option<u64>, String> {
    let t = text.trim();
    let digits = t.find(|c: char| !c.is_ascii_digit()).unwrap_or(t.len());
    let bad = || format!("{flag} needs a size such as 64M, 1G or 0, not {text:?}");
    let n: u64 = t[..digits].parse().map_err(|_| bad())?;
    let shift = match t[digits..].trim().to_ascii_lowercase().as_str() {
        "" | "b" => 0,
        "k" | "kb" | "kib" => 10,
        "m" | "mb" | "mib" => 20,
        "g" | "gb" | "gib" => 30,
        _ => return Err(bad()),
    };
    let bytes = n.checked_mul(1 << shift).ok_or_else(bad)?;
    Ok((bytes > 0).then_some(bytes))
}

/// A size the way `--max-size` is written: `64M`, `1G`, `512K`, or bytes.
pub fn size_flag(bytes: u64) -> String {
    for (shift, unit) in [(30, "G"), (20, "M"), (10, "K")] {
        if bytes >= 1 << shift && bytes.is_multiple_of(1 << shift) {
            return format!("{}{unit}", bytes >> shift);
        }
    }
    bytes.to_string()
}

/// Read all of `input`, holding at most `max` bytes: a longer input is refused, the rest
/// unread.
pub fn read_within(mut input: impl Read, max: Option<u64>) -> Result<Vec<u8>, LoadError> {
    let mut buf = Vec::new();
    let read = match max {
        Some(max) => input.take(max.saturating_add(1)).read_to_end(&mut buf),
        None => input.read_to_end(&mut buf),
    };
    read.map_err(|e| LoadError::new(e.to_string()))?;
    match max {
        Some(max) if buf.len() as u64 > max => Err(LoadError::too_large(None, max)),
        _ => Ok(buf),
    }
}

/// How deep a parse's rule stack may grow before aless stops it as `too_deep`. It guards a
/// grammar with no nesting limit of its own: deeper, some recurse until the stack runs out,
/// which ends the process. Depth follows nesting, never a file's length (ADR-007).
pub const MAX_RULE_DEPTH: usize = 3_000;

/// How deep a custom grammar's value may nest, the levels [`MAX_RULE_DEPTH`] works out to.
/// Measured after the parse: the compiler builds an object and its `kids` per level, which the
/// rule stack does not count in the same units.
pub const MAX_VALUE_DEPTH: usize = MAX_RULE_DEPTH / 3;

/// How far back through `prev` a rule reaches, on every parser [`make_parser`] makes unless its
/// grammar bounds it tighter. Unbounded, parse memory grows with document length.
pub const RULE_HISTORY: usize = 3;

/// The stack a parse runs on: room for [`MAX_RULE_DEPTH`] with a wide margin, whatever the
/// calling thread has (a Windows main thread has 1 MB).
pub(crate) const PARSE_STACK: usize = 64 << 20;

/// Why aless stopped a parse itself: nesting past [`MAX_RULE_DEPTH`], or time.
pub(crate) const STOP_DEPTH: u8 = 1;
pub(crate) const STOP_TIME: u8 = 2;

/// When a parse must be done by.
#[derive(Clone)]
pub(crate) struct Deadline {
    at: Instant,
    limit: Duration,
    /// Raised by the thread waiting on the parse once `at` passes, so the parse reads a flag
    /// between steps, not the clock; `None` when no thread waits.
    alarm: Option<Arc<AtomicBool>>,
    /// Raised by whoever calls the parse off: the painter, once its text is out of view.
    cancel: Option<Arc<AtomicBool>>,
}

impl Deadline {
    /// A clock-read deadline `timeout` from now; `None` for no limit, or one out of reach.
    pub(crate) fn after(timeout: Option<Duration>) -> Option<Deadline> {
        let limit = timeout?;
        Some(Deadline {
            at: Instant::now().checked_add(limit)?,
            limit,
            alarm: None,
            cancel: None,
        })
    }

    /// This deadline, passed as well once `cancel` is raised.
    pub(crate) fn or_cancelled(self, cancel: Arc<AtomicBool>) -> Deadline {
        Deadline {
            cancel: Some(cancel),
            ..self
        }
    }

    /// Whether the time is up or the parse was called off, as the parse checks between steps.
    pub(crate) fn passed(&self) -> bool {
        if self
            .cancel
            .as_ref()
            .is_some_and(|cancel| cancel.load(Ordering::Relaxed))
        {
            return true;
        }
        match &self.alarm {
            Some(alarm) => alarm.load(Ordering::Relaxed),
            None => Instant::now() >= self.at,
        }
    }

    /// [`Deadline::passed`], or the clock past `at` whether or not the alarm is up: for work
    /// about to hand over to a stage the alarm does not stop.
    pub(crate) fn is_past(&self) -> bool {
        self.passed() || Instant::now() >= self.at
    }
}

/// What [`guard`] records when aless stops a parse: why, and the place it had reached, for a
/// report the engine's own error cannot place ([`Stop::place`]).
#[derive(Debug)]
pub(crate) struct Stop {
    why: AtomicU8,
    line: AtomicU64,
    col: AtomicU64,
    /// The token the place is taken from: its offset, name and text.
    token: Mutex<Option<(usize, String, String)>>,
}

impl Stop {
    fn new() -> Arc<Stop> {
        Arc::new(Stop {
            why: AtomicU8::new(0),
            line: AtomicU64::new(0),
            col: AtomicU64::new(0),
            token: Mutex::new(None),
        })
    }

    /// Record `why` at the next token or, when there is none, the last one read: between steps
    /// the lookahead is often empty.
    fn record(&self, why: u8, ctx: &tabnas::Context) {
        if let Some(t) = ctx.t0().or_else(|| ctx.v1()) {
            self.line.store(t.site.ri as u64, Ordering::Relaxed);
            self.col.store(t.site.ci as u64, Ordering::Relaxed);
            *self.token.lock().unwrap_or_else(|e| e.into_inner()) =
                Some((t.site.pos, t.name.to_string(), t.src.to_string()));
        }
        self.why.store(why, Ordering::Relaxed);
    }

    /// Move `e`, the engine's `cancel` for a parse the guard stopped, to the recorded place:
    /// with no token in hand the engine puts it at the start of the document.
    fn place(&self, e: &mut tabnas::TabnasError) {
        let token = self.token.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((pos, name, src)) = token.as_ref() {
            let (line, col) = self.position();
            e.pos = *pos;
            e.row = line as usize;
            e.col = col as usize;
            e.len = src.chars().count();
            e.src = src.clone();
            e.token.name = name.clone();
            e.token.src = src.clone();
        }
    }

    /// [`STOP_DEPTH`], [`STOP_TIME`], or zero when the parse was not stopped by aless.
    pub(crate) fn why(&self) -> u8 {
        self.why.load(Ordering::Relaxed)
    }

    /// The 1-based line and column the parse was stopped at; 0 when unknown.
    pub(crate) fn position(&self) -> (u32, u32) {
        let at = |n: &AtomicU64| u32::try_from(n.load(Ordering::Relaxed)).unwrap_or(u32::MAX);
        (at(&self.line), at(&self.col))
    }
}

/// The placeholder the engine writes where a file name belongs.
const NO_FILE: &str = "<no-file>";

/// The engine's name for the token at the end of the source.
const END_TOKEN: &str = "#ZZ";

const END_HINT: &str = "The document ends before it is complete: look for an unclosed\n\
                        bracket, brace or string, or a missing value at the end.";

impl LoadError {
    /// An error aless raised itself, in the engine's layout under its own tag.
    pub fn new(message: impl Into<String>) -> LoadError {
        LoadError::tagged("io", message)
    }

    pub fn tagged(tag: &str, message: impl Into<String>) -> LoadError {
        let message = message.into();
        let report = format!(
            "\x1b[91m[aless/{tag}]:\x1b[0m {}\n  \x1b[34m-->\x1b[0m {NO_FILE}",
            escape_controls(&message)
        );
        LoadError {
            message,
            code: tag.to_string(),
            line: 0,
            col: 0,
            hint: Box::default(),
            source_line: None,
            report,
        }
    }

    /// A refusal a format's crate made outside its grammar's parse (proto's version check, a
    /// notation compiler's unplaced refusal), under `[<format>/<code>]`, with no position.
    pub(crate) fn refused(
        format: Format,
        code: &str,
        message: impl Into<String>,
        hint: &str,
    ) -> LoadError {
        let mut e = LoadError::tagged(code, message);
        e.report = e
            .report
            .replacen(&format!("[aless/{code}]"), &format!("[{format}/{code}]"), 1);
        if hint.is_empty() {
            e
        } else {
            e.with_hint(hint)
        }
    }

    /// Whether the input could not be read at all, rather than read and found not to parse.
    pub fn is_io(&self) -> bool {
        self.code == "io"
    }

    /// Whether the input was refused for its size (see [`read_within`]).
    pub fn is_too_large(&self) -> bool {
        self.code == "too_large"
    }

    /// Whether the parse was stopped for running past its time limit.
    pub fn is_timeout(&self) -> bool {
        self.code == "timeout"
    }

    /// An input over the size limit: `size` when known (a file), `None` for a stream.
    pub fn too_large(size: Option<u64>, limit: u64) -> LoadError {
        use crate::explorer::human_size;
        let message = match size {
            Some(n) => format!(
                "input is {}, over the {} limit",
                human_size(n),
                human_size(limit)
            ),
            None => format!("input is over the {} limit", human_size(limit)),
        };
        let per_byte =
            format!("Parsing takes about {MEMORY_PER_BYTE} bytes of memory per byte of input");
        let hint = match size {
            Some(n) => format!(
                "{per_byte}, so this one would need about {}. Pass --max-size {} \
                 (or more) to read it, or --max-size 0 for no limit.",
                human_size(n.saturating_mul(MEMORY_PER_BYTE)),
                size_flag(room_for(size, limit))
            ),
            None => format!(
                "{per_byte}. Pass a larger --max-size, such as {}, to read it, or \
                 --max-size 0 for no limit.",
                size_flag(room_for(size, limit))
            ),
        };
        LoadError::tagged("too_large", message).with_hint(&hint)
    }

    /// A grammar file over the size limit, worded for a file no parse of which is coming.
    pub(crate) fn grammar_too_large(size: Option<u64>, limit: u64) -> LoadError {
        LoadError::file_too_large("the grammar file", "A grammar file", size, limit)
    }

    /// A command-line file over the size limit, its hint without an input's memory estimate.
    fn file_too_large(what: &str, which: &str, size: Option<u64>, limit: u64) -> LoadError {
        use crate::explorer::human_size;
        let message = match size {
            Some(n) => format!(
                "{what} is {}, over the {} limit",
                human_size(n),
                human_size(limit)
            ),
            None => format!("{what} is over the {} limit", human_size(limit)),
        };
        let hint = format!(
            "{which} over --max-size is not read. Pass --max-size {} (or more) to read it, \
             or --max-size 0 for no limit.",
            size_flag(room_for(size, limit))
        );
        LoadError::tagged("too_large", message).with_hint(&hint)
    }

    pub(crate) fn compile_timed_out(limit: Duration) -> LoadError {
        let hint = format!(
            "The compiler cannot be interrupted, so aless stopped waiting for it.\nPass a \
             larger --timeout to let it finish, or --timeout 0 for no limit. {}",
            Self::SLOW_GRAMMARS
        );
        Self::compile_timeout(limit).with_hint(&hint)
    }

    /// A grammar not compiled because a compile aless stopped waiting for still runs.
    pub(crate) fn compile_behind(limit: Duration) -> LoadError {
        let message = format!(
            "timeout: the grammar was not compiled within {} s: a compile aless stopped \
             waiting for is still running",
            seconds(limit)
        );
        let hint = format!(
            "A compiler cannot be interrupted, so aless starts no other while one it stopped \
             waiting for still runs.\nPass a larger --timeout, or --timeout 0 for no limit, or \
             read the slow grammar on its own. {}",
            Self::SLOW_GRAMMARS
        );
        LoadError::tagged("timeout", message).with_hint(&hint)
    }

    /// A grammar not compiled because the time was up before its compile started.
    pub(crate) fn compile_unstarted(limit: Duration) -> LoadError {
        let message = format!(
            "timeout: the grammar was not compiled within {} s: the time was up before its \
             compile started",
            seconds(limit)
        );
        let hint = "The time was up before the grammar reached its compiler, which cannot be \
                    interrupted, so aless did not start it.\nPass a larger --timeout, or \
                    --timeout 0 for no limit.";
        LoadError::tagged("timeout", message).with_hint(hint)
    }

    /// A compile that ended past the limit on the caller's thread, where it runs when no thread
    /// of its own can be started, and so could not be stopped.
    pub(crate) fn compile_finished_late(limit: Duration, took: Duration) -> LoadError {
        let took = Duration::from_millis(timeout_ms(Some(took)));
        let hint = format!(
            "No thread could be started for the compiler, so it ran to its end on aless's \
             own and took {} s.\nPass a larger --timeout to have the grammar, or --timeout 0 \
             for no limit. {}",
            seconds(took),
            Self::SLOW_GRAMMARS
        );
        Self::compile_timeout(limit).with_hint(&hint)
    }

    const SLOW_GRAMMARS: &'static str = "Long repetitions (1*500word), and rules of several \
                                         alternatives that start with one another, make a \
                                         grammar slow to compile.";

    fn compile_timeout(limit: Duration) -> LoadError {
        let message = format!(
            "timeout: the grammar took longer than {} s to compile",
            seconds(limit)
        );
        LoadError::tagged("timeout", message)
    }

    /// A parse finished past its limit: it stopped nowhere, so the report gives the time taken.
    fn finished_late(limit: Duration, took: Duration) -> LoadError {
        let took = Duration::from_millis(timeout_ms(Some(took)));
        let message = format!("timeout: the parse ran longer than {} s", seconds(limit));
        let hint = format!(
            "The parse finished, but it took {} s.\nPass a larger --timeout to have its \
             result, or --timeout 0 for no limit.",
            seconds(took)
        );
        LoadError::tagged("timeout", message).with_hint(&hint)
    }

    /// Add advice to an error aless raised, under the report as the engine puts a grammar's.
    pub(crate) fn with_hint(mut self, hint: &str) -> LoadError {
        let hint = escape_controls_but_newlines(hint);
        self.report.push_str("\n\n");
        for line in wrap(&hint, 72) {
            self.report.push_str("  ");
            self.report.push_str(&line);
            self.report.push('\n');
        }
        self.report.pop();
        self.hint = hint.into();
        self
    }

    /// Name the file in place of `<no-file>`, its control characters escaped: a Unix file name
    /// may hold a newline, and the report's colour codes are obeyed.
    pub fn with_origin(mut self, origin: &str) -> LoadError {
        self.report = replace_after_header(&self.report, NO_FILE, &escape_controls(origin));
        self
    }

    /// From an engine error, with all that came from the file made safe to draw, since the
    /// report's colour codes are obeyed: control characters are escaped in the message and hint
    /// and drawn as one-column pictures in quoted lines, so the caret still lines up
    /// ([`visible`]); a long line is cut to a window ([`shape_excerpt`]). `hint_template` tells
    /// the hint's own line breaks from the token's.
    pub fn from_tabnas(e: &tabnas::TabnasError, hint_template: Option<&str>) -> LoadError {
        let mut shown = e.clone();
        shown.detail = escape_controls(&e.detail);
        shown.hint = escape_hint(&e.hint, hint_template, &e.src);
        if e.code == "unexpected" && e.token.name == END_TOKEN && e.src.is_empty() {
            // The engine calls this an unexpected character, quoting none: the input ended.
            shown.detail = "unexpected end of input".to_string();
            shown.hint = END_HINT.to_string();
        }
        shape_excerpt(&mut shown, e);
        let detail = shown.detail.trim();
        let message = if detail.starts_with(&e.code) || e.code.is_empty() {
            detail.to_string()
        } else {
            format!("{}: {}", e.code, detail)
        };
        let mut report = shown.to_string();
        if shown.col != e.col {
            // The `-->` line names the column in the file, not the window.
            report = replace_after_header(
                &report,
                &format!("{NO_FILE}:{}:{}", e.row, shown.col),
                &format!("{NO_FILE}:{}:{}", e.row, e.col),
            );
        }
        let source_line = e.row.checked_sub(1).and_then(|i| {
            e.full_source
                .split('\n')
                .nth(i)
                .map(|l| window(l.strip_suffix('\r').unwrap_or(l), e.col).into())
        });
        LoadError {
            message,
            code: if e.code.is_empty() {
                "unknown".to_string()
            } else {
                e.code.clone()
            },
            line: e.row as u32,
            col: e.col as u32,
            hint: shown.hint.trim().into(),
            source_line,
            report,
        }
    }

    pub fn plain_report(&self) -> String {
        strip_ansi(&self.report)
    }
}

/// The `--max-size` to suggest: the first doubling of `limit` that holds `size`, or the limit
/// quadrupled for a stream, whose size is unknown.
fn room_for(size: Option<u64>, limit: u64) -> u64 {
    match size {
        Some(n) => {
            let mut room = limit.saturating_mul(2);
            while room < n {
                room = room.saturating_mul(2);
            }
            room
        }
        None => limit.saturating_mul(4),
    }
}

/// Break text into lines of at most `width` characters at spaces, keeping its own line breaks.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    for para in text.split('\n') {
        let mut line = String::new();
        for word in para.split(' ') {
            if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
                out.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        out.push(line);
    }
    out
}

/// Show control characters as escapes: `\n`, `\t`, `\r`, else `\u001b`.
pub fn escape_controls(s: &str) -> String {
    escape(s, false)
}

/// [`escape_controls`], but line breaks stay.
fn escape_controls_but_newlines(s: &str) -> String {
    escape(s, true)
}

/// The hint template for `code`, found as the engine finds it: its own, else `unknown`'s.
pub fn hint_template<'a>(hints: &'a HashMap<String, String>, code: &str) -> Option<&'a str> {
    hints
        .get(code)
        .filter(|t| !t.is_empty())
        .or_else(|| hints.get("unknown").filter(|t| !t.is_empty()))
        .map(String::as_str)
}

/// A hint with what its placeholders brought in escaped and the template's own line breaks
/// kept. `{src}` alone cannot be searched for: the token may be a newline, and the template has
/// its own. Without a matching template, the token's first occurrence is taken for it.
fn escape_hint(hint: &str, template: Option<&str>, src: &str) -> String {
    if let Some(escaped) = template.and_then(|t| escape_placeholders(hint, t)) {
        return escaped;
    }
    let hint = if src.chars().any(char::is_control) {
        hint.replacen(src, &escape_controls(src), 1)
    } else {
        hint.to_string()
    };
    escape_controls_but_newlines(&hint)
}

fn escape_placeholders(hint: &str, template: &str) -> Option<String> {
    // The engine fills `{key}` up to the next `}`, and trims the result.
    let mut pattern = String::from("(?s)^");
    let mut rest = template.trim();
    while let Some(open) = rest.find('{') {
        pattern.push_str(&regex::escape(&rest[..open]));
        let close = rest[open..].find('}')? + open;
        pattern.push_str("(.*?)");
        rest = &rest[close + 1..];
    }
    pattern.push_str(&regex::escape(rest));
    pattern.push('$');
    let caps = regex::Regex::new(&pattern).ok()?.captures(hint)?;
    let mut out = String::with_capacity(hint.len());
    let mut last = 0;
    for m in caps.iter().skip(1).flatten() {
        out.push_str(&hint[last..m.start()]);
        out.push_str(&escape_controls(m.as_str()));
        last = m.end();
    }
    out.push_str(&hint[last..]);
    Some(out)
}

fn escape(s: &str, keep_newlines: bool) -> String {
    if !s.chars().any(char::is_control) {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '\n' if keep_newlines => out.push('\n'),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Replace the first `from` below a report's first line. That line is the
/// message, which quotes the file and so could hold the same text.
fn replace_after_header(report: &str, from: &str, to: &str) -> String {
    match report.find('\n') {
        Some(i) => format!("{}{}", &report[..=i], report[i + 1..].replacen(from, to, 1)),
        None => report.replacen(from, to, 1),
    }
}

/// A quoted source line longer than this many characters is cut to a window around the error.
const LONG_LINE: usize = 160;
const WINDOW: usize = 120;
/// How many of the window's characters come before the error column.
const WINDOW_LEAD: usize = 40;

/// A source line as an error quotes it: whole, or a [`WINDOW`] around the 1-based `col` when
/// longer than [`LONG_LINE`], marked `…` where cut.
fn window(line: &str, col: usize) -> String {
    let chars: Vec<char> = line.chars().collect();
    if chars.len() <= LONG_LINE {
        return line.to_string();
    }
    let start = col.max(1).saturating_sub(1).saturating_sub(WINDOW_LEAD);
    let end = (start + WINDOW).min(chars.len());
    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    out.extend(&chars[start..end]);
    if end < chars.len() {
        out.push('…');
    }
    out
}

/// Prepare the lines the engine quotes around the error for drawing
/// ([`LoadError::from_tabnas`]), those above left empty so the line numbers hold. When any is
/// longer than [`LONG_LINE`] all are cut to one window, the column moving with it. The caret
/// stops at the end of its line: an unterminated string's token is the rest of the file.
fn shape_excerpt(shown: &mut tabnas::TabnasError, e: &tabnas::TabnasError) {
    if e.full_source.is_empty() {
        return;
    }
    // The lines the engine picks: it splits on '\n' and clamps the row.
    let lines: Vec<&str> = e.full_source.split('\n').collect();
    let at = e.row.max(1).saturating_sub(1).min(lines.len() - 1);
    let lo = at.saturating_sub(2);
    let hi = (at + 2).min(lines.len() - 1);
    let quoted: Vec<Vec<char>> = lines[lo..=hi]
        .iter()
        .map(|l| l.strip_suffix('\r').unwrap_or(l).chars().collect())
        .collect();

    let c0 = e.col.max(1) - 1;
    let long = quoted.iter().any(|l| l.len() > LONG_LINE);
    let (start, end) = if long {
        let start = c0.saturating_sub(WINDOW_LEAD);
        (start, start + WINDOW)
    } else {
        (0, usize::MAX)
    };
    let mut src = "\n".repeat(lo);
    for (i, line) in quoted.iter().enumerate() {
        if i > 0 {
            src.push('\n');
        }
        if start > 0 && !line.is_empty() {
            src.push('…');
        }
        src.extend(
            line.iter()
                .skip(start)
                .take(end - start)
                .map(|&c| visible(c)),
        );
        if line.len() > end {
            src.push('…');
        }
    }
    shown.full_source = src;
    shown.col = c0 - start + 1 + usize::from(start > 0);

    // The engine draws one caret per character of the token.
    let rest_of_line = quoted[at - lo].len().saturating_sub(c0);
    let carets = e.src.chars().count().min(rest_of_line).min(end - c0).max(1);
    shown.src = e.src.chars().take(carets).collect();
}

/// A quoted character as drawn: a control becomes its one-column picture from the U+2400 block
/// (U+FFFD for C1 controls, which have none). A tab stays, drawn as a space.
fn visible(c: char) -> char {
    match c {
        '\t' => '\t',
        '\u{0}'..='\u{1f}' => char::from_u32(0x2400 + c as u32).unwrap_or('\u{fffd}'),
        '\u{7f}' => '\u{2421}',
        c if c.is_control() => '\u{fffd}',
        c => c,
    }
}

/// Remove ANSI escape sequences (CSI such as colour codes, and OSC).
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('[') => {
                for n in chars.by_ref() {
                    if ('\x40'..='\x7e').contains(&n) {
                        break;
                    }
                }
            }
            Some(']') => {
                while let Some(n) = chars.next() {
                    if n == '\x07' {
                        break;
                    }
                    if n == '\x1b' {
                        if chars.peek() == Some(&'\\') {
                            chars.next();
                        }
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line > 0 {
            write!(f, "{}:{}: {}", self.line, self.col, self.message)
        } else {
            f.write_str(&self.message)
        }
    }
}

impl std::error::Error for LoadError {}

/// A loaded document with the source text it came from.
#[derive(Clone, Debug)]
pub struct Loaded {
    pub doc: Doc,
    pub format: Format,
    pub source: String,
}

/// The tabnas parser for a format; `None` for a format aless does not drive (plain text, the
/// notations). A custom grammar that fails to install is a `grammar` error.
pub(crate) fn make_parser(format: Format) -> Result<Option<Tabnas>, LoadError> {
    let mut parser = match format {
        Format::Json => tabnas_json::make(),
        Format::Jsonl => tabnas_jsonl::make(),
        Format::Jsonic => tabnas_jsonic::make(),
        // Take a trailing comma as VS Code does; the crate's default refuses it.
        Format::Jsonc => tabnas_jsonc::make_with(
            tabnas_jsonc::JsoncOptions::new().with_allow_trailing_comma(true),
        ),
        Format::Json5 => tabnas_json5::make(),
        Format::Yaml => tabnas_yaml::make(),
        Format::Toml => tabnas_toml::make(),
        Format::Ini => tabnas_ini::make(),
        Format::Csv => tabnas_csv::make(),
        Format::Tsv => {
            let mut options = tabnas_csv::CsvOptions::default();
            options.field.separation = Some("\t".to_string());
            tabnas_csv::make_with(options)
        }
        Format::Xml => tabnas_xml::make(),
        Format::Zon => tabnas_zon::make(),
        Format::Markdown => tabnas_markdown::make(),
        Format::Feed => tabnas_feed::make(),
        Format::Css => tabnas_css::make(),
        Format::Proto => tabnas_proto::make(),
        Format::Pgn => match tabnas_chess::make(&tabnas_chess::ChessOptions::default()) {
            // The crate colours reports only when stdout is a terminal; aless styles or strips
            // the engine's codes itself, so it restores the engine's default.
            Ok(mut parser) => {
                parser.options.color.active = tabnas::Options::default().color.active;
                parser
            }
            Err(e) => return Err(grammar_failed(format, &e.to_string())),
        },
        Format::Expr => tabnas_expr::make(),
        Format::Semver => tabnas_semver::make(),
        // C23, whole preprocessor; [`parse_value`] realizes the tree from this thread's arena.
        Format::C => tabnas_c::make(),
        // A notation's compiler parses with a parser of its own ([`compile_notation`]).
        Format::Text | Format::Abnf | Format::Ebnf | Format::Gbnf => return Ok(None),
        Format::Custom(id) => {
            let Some(g) = grammar::get(id) else {
                return Err(grammar_failed(format, "the grammar is not registered"));
            };
            // The compiler's install is a plugin's code too.
            match catch_grammar(|| g.parser()) {
                Ok(Ok(parser)) => parser,
                Ok(Err(e)) | Err(e) => return Err(grammar_failed(format, &e)),
            }
        }
    };
    // A grammar's own, tighter bound (CSS's) is kept; one from the command line sets none.
    parser.options.rule.history.get_or_insert(RULE_HISTORY);
    Ok(Some(parser))
}

/// The text a parser reads: the source without its byte-order mark, positions unchanged.
pub(crate) fn parser_text(src: &str) -> &str {
    src.strip_prefix('\u{feff}').unwrap_or(src)
}

/// [`parser_text`], and a `semver` version without the line break its file ends with, which the
/// grammar, taking no whitespace, would refuse.
pub(crate) fn parse_text(src: &str, format: Format) -> &str {
    let text = parser_text(src);
    match format {
        Format::Semver => text
            .strip_suffix("\r\n")
            .or_else(|| text.strip_suffix('\n'))
            .unwrap_or(text),
        _ => text,
    }
}

/// What a format's crate checks before its grammar parses: proto refuses braces nested past
/// [`tabnas_proto::MAX_NESTING_DEPTH`], since its parse tree for one is let go of by recursion.
pub(crate) fn check_text(format: Format, src: &str) -> Result<(), LoadError> {
    match format {
        Format::Proto => tabnas_proto::preflight(src).map_err(|e| proto_refused(format, e)),
        _ => Ok(()),
    }
}

/// What a document is read as, from [`parse_value`]'s value: proto's descriptor, expr's
/// S-expressions (read from this thread's arena before it parses again), and C's tree, refused
/// past [`c_tree_cap`]: its reader shares subtrees under several members, so the tree written
/// out doubles with each level of a member chain. It is measured shared.
pub(crate) fn read_as(
    format: Format,
    value: tabnas::Value,
    source_len: usize,
) -> Result<tabnas::Value, LoadError> {
    match format {
        Format::Proto => {
            tabnas_proto::to_descriptor_value(&value, None).map_err(|e| proto_refused(format, e))
        }
        Format::Expr => Ok(tabnas_expr::simplify(&value)),
        Format::C => {
            let cap = c_tree_cap(source_len);
            if written_out(&value, cap).past(cap) {
                drop_deep(value);
                let (message, hint) = too_deep_words(format, Deep::Repeated);
                return Err(LoadError::tagged("too_deep", message).with_hint(&hint));
            }
            Ok(value)
        }
        _ => Ok(value),
    }
}

/// Values a C tree may hold written out per source byte, far past tabnas-c's Csmith corpus.
pub const C_TREE_PER_BYTE: u64 = 256;

/// The fewest values [`c_tree_cap`] allows, so a short document is not refused for its length.
/// [`Deep::Repeated`]'s hint spells it out.
pub const C_TREE_FLOOR: u64 = 1 << 20;

/// The most values a C document may hold written out, however long: without it a long comment
/// beside a short member chain would raise that chain's cap. The hint spells it out.
pub const C_TREE_CEILING: u64 = 1 << 25;

/// Bytes of names and strings a C tree may hold written out per source byte: the value count
/// takes a scalar once, so this holds a long name in a repeated subtree.
pub const C_TREE_BYTES_PER_BYTE: u64 = 2048;

/// The least text [`c_tree_cap`] allows any C document.
pub const C_TREE_BYTES_FLOOR: u64 = 8 << 20;

/// The most text a C document may hold written out: without it, a long name under a member
/// chain that stays within [`C_TREE_CEILING`] would be written out past any memory.
pub const C_TREE_BYTES_CEILING: u64 = 256 << 20;

/// What a value holds written out as a tree: every value, and the bytes of every string and
/// member name, each counted in every place a shared container puts it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct WrittenOut {
    pub(crate) values: u64,
    pub(crate) bytes: u64,
}

impl WrittenOut {
    fn plus(self, other: WrittenOut) -> WrittenOut {
        WrittenOut {
            values: self.values.saturating_add(other.values),
            bytes: self.bytes.saturating_add(other.bytes),
        }
    }

    pub(crate) fn past(self, cap: WrittenOut) -> bool {
        self.values > cap.values || self.bytes > cap.bytes
    }
}

/// What a C document `source_len` bytes long may hold written out, scaled and clamped.
pub(crate) fn c_tree_cap(source_len: usize) -> WrittenOut {
    let len = source_len as u64;
    WrittenOut {
        values: len
            .saturating_mul(C_TREE_PER_BYTE)
            .clamp(C_TREE_FLOOR, C_TREE_CEILING),
        bytes: len
            .saturating_mul(C_TREE_BYTES_PER_BYTE)
            .clamp(C_TREE_BYTES_FLOOR, C_TREE_BYTES_CEILING),
    }
}

/// What `value` holds written out ([`WrittenOut`]), the root included. A shared container (its
/// `Arc` held more than once) is measured once and remembered, so the walk follows the shared
/// tree. It stops at the first container past `cap`, answering that one's counts.
pub(crate) fn written_out(value: &tabnas::Value, cap: WrittenOut) -> WrittenOut {
    use tabnas::Value;
    /// A container's members, each with its name's bytes (none in a list).
    type Members<'a> = Vec<(&'a Value, u64)>;
    /// A container's identity, whether it is shared, and its members.
    fn container(v: &Value) -> Option<(usize, bool, Members<'_>)> {
        Some(match v {
            Value::Array(items) => (
                Arc::as_ptr(items) as *const u8 as usize,
                Arc::strong_count(items) > 1,
                items.iter().map(|m| (m, 0)).collect(),
            ),
            Value::ListRef(list) => (
                Arc::as_ptr(list) as *const u8 as usize,
                Arc::strong_count(list) > 1,
                list.value.iter().map(|m| (m, 0)).collect(),
            ),
            Value::Object(map) => (
                Arc::as_ptr(map) as *const u8 as usize,
                Arc::strong_count(map) > 1,
                map.iter().map(|(k, m)| (m, k.len() as u64)).collect(),
            ),
            Value::MapRef(map) => (
                Arc::as_ptr(map) as *const u8 as usize,
                Arc::strong_count(map) > 1,
                map.value.iter().map(|(k, m)| (m, k.len() as u64)).collect(),
            ),
            _ => return None,
        })
    }
    fn text(v: &Value) -> u64 {
        match v {
            Value::String(s) => s.len() as u64,
            Value::Text(t) => t.string.len() as u64,
            _ => 0,
        }
    }
    enum Step<'a> {
        /// A value to measure, and the bytes of the name it is under.
        Enter(&'a Value, u64),
        /// A container's members are counted: its identity, whether it is shared, where its
        /// members' counts start, and its name's bytes.
        Leave(usize, bool, usize, u64),
    }
    let mut known: HashMap<usize, WrittenOut> = HashMap::new();
    let mut counts: Vec<WrittenOut> = Vec::new();
    let mut steps = vec![Step::Enter(value, 0)];
    let named = |n: WrittenOut, name: u64| WrittenOut {
        values: n.values,
        bytes: n.bytes.saturating_add(name),
    };
    while let Some(step) = steps.pop() {
        match step {
            Step::Enter(v, name) => match container(v) {
                None => counts.push(WrittenOut {
                    values: 1,
                    bytes: text(v).saturating_add(name),
                }),
                Some((id, shared, members)) => {
                    if let Some(n) = shared.then(|| known.get(&id)).flatten() {
                        counts.push(named(*n, name));
                        continue;
                    }
                    steps.push(Step::Leave(id, shared, counts.len(), name));
                    steps.extend(
                        members
                            .into_iter()
                            .rev()
                            .map(|(m, name)| Step::Enter(m, name)),
                    );
                }
            },
            Step::Leave(id, shared, from, name) => {
                let n = counts.drain(from..).fold(
                    WrittenOut {
                        values: 1,
                        bytes: 0,
                    },
                    WrittenOut::plus,
                );
                if n.past(cap) {
                    return n;
                }
                if shared {
                    known.insert(id, n);
                }
                counts.push(named(n, name));
            }
        }
    }
    counts.pop().unwrap_or_default()
}

/// Parse `text` with `parser` from [`make_parser`]. For C, tabnas-c's own parse realizes the
/// arena tree on this thread before it parses again, refusing a tree deeper than
/// [`tabnas_c::REALIZE_DEPTH_CAP`] with an unplaced `cancel`.
pub(crate) fn parse_value(
    parser: &Tabnas,
    format: Format,
    text: &str,
) -> Result<tabnas::Value, Box<tabnas::TabnasError>> {
    match format {
        Format::C => tabnas_c::parse_with(parser, text),
        _ => parser.parse(text),
    }
    .map_err(Box::new)
}

/// Whether `e`, a C parse's `cancel`, names no place: the engine's with no token in hand and
/// tabnas-c's for a tree it could not realize are at offset 0 with no text.
pub(crate) fn unplaced_c_cancel(format: Format, e: &tabnas::TabnasError) -> bool {
    format == Format::C && e.code == "cancel" && e.pos == 0 && e.src.is_empty()
}

/// The grammar spec a notation's compiler writes for the document (the schema `grammar-spec`):
/// the pure-data spec with tree builders, which each crate says a host reads and its TypeScript
/// compiler writes. The compiler bounds nesting itself and runs uninterrupted;
/// [`compile_notation_by`] bounds the wait.
pub(crate) fn compile_notation(format: Format, src: &str) -> Result<tabnas::Value, LoadError> {
    use tabnas_abnf::AbnfConvertOptions as Convert;
    let builtins = Convert {
        builtins: true,
        ..Convert::default()
    };
    let spec = match format {
        Format::Abnf => tabnas_abnf::abnf_convert(
            src,
            Some(&Convert {
                marks: true,
                ..builtins
            }),
        )
        .map_err(Refusal::from),
        Format::Ebnf => tabnas_ebnf::ebnf_convert(src, Some(&builtins)).map_err(Refusal::from),
        Format::Gbnf => {
            tabnas_gbnf::gbnf_convert(src, Some(&tabnas_gbnf::GbnfConvertOptions::new(builtins)))
                .map_err(Refusal::from)
        }
        other => return Err(grammar_failed(other, "it is no grammar notation")),
    };
    let pure = spec
        .and_then(|spec| tabnas_abnf::to_pure_spec(&spec).map_err(|e| Refusal::message(e.message)));
    match pure {
        Ok(json) => Ok(tabnas::Value::from_json(&json)),
        Err(refusal) => Err(refusal.load_error(format, src)),
    }
}

/// How many compiles aless stopped waiting for still run; while any does,
/// [`compile_notation_by`] starts none with a deadline.
static ABANDONED_COMPILES: AtomicUsize = AtomicUsize::new(0);

/// A compile's state: running, or abandoned by its waiter or done, whichever happens first;
/// each side reads and writes it under its lock.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Compile {
    Running,
    Abandoned,
    Done,
}

/// [`compile_notation`] within `deadline`. A compile cannot be stopped part way, so with a
/// deadline it runs on its own thread, left to itself past the deadline and holding what it
/// holds until it ends: while one runs, or once the deadline has passed, no other starts.
/// Without a thread it is a `grammar` error, since it could not be held to the deadline.
pub(crate) fn compile_notation_by(
    format: Format,
    src: &str,
    deadline: Option<Deadline>,
) -> Result<tabnas::Value, LoadError> {
    let caught = |outcome: Result<Result<tabnas::Value, LoadError>, String>| {
        outcome.unwrap_or_else(|panic| Err(grammar_failed(format, &panic)))
    };
    let Some(deadline) = deadline else {
        return caught(catch_grammar(|| compile_notation(format, src)));
    };
    if ABANDONED_COMPILES.load(Ordering::SeqCst) > 0 {
        return Err(LoadError::compile_behind(deadline.limit));
    }
    let state = Arc::new(Mutex::new(Compile::Running));
    let theirs = state.clone();
    let (send, compiled) = mpsc::channel();
    let text = src.to_string();
    // The time is read just before the spawn: a compile started late would be abandoned at
    // once.
    if deadline.is_past() {
        return Err(LoadError::compile_unstarted(deadline.limit));
    }
    let spawned = std::thread::Builder::new()
        .name("aless-notation".into())
        .stack_size(PARSE_STACK)
        .spawn(move || {
            let outcome = catch_grammar(|| compile_notation(format, &text));
            drop(text);
            let mut state = theirs.lock().unwrap_or_else(|e| e.into_inner());
            if *state == Compile::Abandoned {
                // Nothing waits for it: let the outcome go, and the next compile start.
                drop(outcome);
                ABANDONED_COMPILES.fetch_sub(1, Ordering::SeqCst);
            } else {
                let _ = send.send(outcome);
            }
            *state = Compile::Done;
        });
    if spawned.is_err() {
        return Err(grammar_failed(
            format,
            "no thread could be had for its compiler, which cannot be stopped part way, so \
             it was not compiled where --timeout could not hold it",
        ));
    }
    let left = deadline.at.saturating_duration_since(Instant::now());
    match compiled.recv_timeout(left) {
        Ok(outcome) => caught(outcome),
        Err(RecvTimeoutError::Timeout) => {
            let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
            // Done in the meantime is done late: the time is the outcome.
            if *state == Compile::Running {
                *state = Compile::Abandoned;
                ABANDONED_COMPILES.fetch_add(1, Ordering::SeqCst);
            }
            Err(LoadError::compile_timed_out(deadline.limit))
        }
        Err(RecvTimeoutError::Disconnected) => {
            Err(grammar_failed(format, "the compiler gave no answer"))
        }
    }
}

/// What a notation's compiler refused a document for: its message (prefixed with the notation's
/// name), the place it names, and the engine's code when the engine refused the text.
#[derive(Debug)]
struct Refusal {
    message: String,
    line: Option<usize>,
    col: Option<usize>,
    code: Option<String>,
}

impl Refusal {
    /// A refusal that names no place.
    fn message(message: String) -> Refusal {
        Refusal {
            message,
            line: None,
            col: None,
            code: None,
        }
    }

    /// A refusal of the compiler's, at the range of the grammar's text it names.
    fn spanned(message: String, span: Option<tabnas_abnf::SrcSpan>) -> Refusal {
        Refusal {
            message,
            line: span.and_then(|sp| sp.r),
            col: span.and_then(|sp| sp.c),
            code: None,
        }
    }

    /// A refusal of the notation's parse: the engine's with its code, or the front-end's.
    fn parsed(
        message: String,
        line: Option<usize>,
        col: Option<usize>,
        code: Option<String>,
    ) -> Refusal {
        Refusal {
            message,
            line,
            col,
            code,
        }
    }

    /// The refusal as a load error of `format`: nesting past the compiler's limit is
    /// `too_deep`, the engine's refusal keeps its code, and any other is `compile`; each is
    /// placed where the refusal says, with its line quoted.
    fn load_error(self, format: Format, src: &str) -> LoadError {
        let plain = strip_ansi(&self.message);
        let detail = plain
            .strip_prefix(&format!("{format}: "))
            .unwrap_or(&plain)
            .to_string();
        // The engine's report, quoted by the front-end after its `parse error at` place.
        let detail = match (&self.code, detail.find("]: ")) {
            (Some(_), Some(at)) if detail.starts_with("parse error") => {
                detail[at + 3..].to_string()
            }
            _ => detail,
        };
        let deep =
            detail.contains("nests too deeply") || detail.contains("nests elements more than");
        let code = match (&self.code, deep) {
            (_, true) => "too_deep".to_string(),
            (Some(code), false) => code.clone(),
            (None, false) => "compile".to_string(),
        };
        let hint = if deep {
            format!(
                "The {format} compiler bounds how deep a grammar nests, and refused this one \
                 before building anything that deep.\nSplit a deeply nested rule into named \
                 rules."
            )
        } else if self.code.is_some() {
            String::new()
        } else {
            format!(
                "The document is read as the grammar spec the {format} compiler writes for it, \
                 and the compiler refused it."
            )
        };
        match self.line.zip(self.col) {
            Some((line, col)) if line > 0 && col > 0 => {
                let pos = offset_of(src, line, col);
                let token: String = src[pos..].chars().take(1).collect();
                let mut e = tabnas::TabnasError::new(code, token, src, pos, line, col);
                e.tag = format.name().to_string();
                e.detail = detail;
                if !hint.is_empty() || self.code.is_none() {
                    e.hint = hint;
                }
                LoadError::from_tabnas(&e, None)
            }
            _ => LoadError::refused(format, &code, format!("{code}: {detail}"), &hint),
        }
    }
}

impl From<tabnas_abnf::AbnfError> for Refusal {
    fn from(e: tabnas_abnf::AbnfError) -> Refusal {
        use tabnas_abnf::AbnfError;
        match e {
            AbnfError::Parse(p) => Refusal::parsed(p.message, p.line, p.column, p.code),
            AbnfError::Emit(e) => Refusal::spanned(e.message, e.sp),
            other => Refusal::message(other.to_string()),
        }
    }
}

impl From<tabnas_ebnf::EbnfError> for Refusal {
    fn from(e: tabnas_ebnf::EbnfError) -> Refusal {
        use tabnas_ebnf::EbnfError;
        match e {
            EbnfError::Parse(p) => Refusal::parsed(p.message, p.line, p.column, p.code),
            EbnfError::Compile(c) => Refusal::spanned(c.message, c.sp),
            other => Refusal::message(other.to_string()),
        }
    }
}

impl From<tabnas_gbnf::GbnfError> for Refusal {
    fn from(e: tabnas_gbnf::GbnfError) -> Refusal {
        use tabnas_gbnf::GbnfError;
        match e {
            GbnfError::Parse(p) => Refusal::parsed(p.message, p.line, p.column, p.code),
            GbnfError::Compile(c) => Refusal::spanned(c.message, c.sp),
            GbnfError::Emit(e) => Refusal::spanned(e.message, e.sp),
            other => Refusal::message(other.to_string()),
        }
    }
}

/// The byte offset of the 1-based `line` and `col` (in characters) in `src`, or the nearest
/// there is: a line's end, or the source's.
fn offset_of(src: &str, line: usize, col: usize) -> usize {
    let Some(start) = src
        .split_inclusive('\n')
        .take(line - 1)
        .map(str::len)
        .try_fold(0usize, |at, n| Some(at + n))
        .filter(|at| *at <= src.len() && (line == 1 || src[..*at].ends_with('\n')))
    else {
        return src.len();
    };
    let rest = &src[start..];
    let end = rest.find('\n').unwrap_or(rest.len());
    start
        + rest[..end]
            .char_indices()
            .nth(col - 1)
            .map_or(end, |(i, _)| i)
}

/// A refusal of proto's own, outside its grammar's parse, as a load error: braces nested too
/// deep are `too_deep`, an unknown version is `version`.
fn proto_refused(format: Format, e: tabnas_proto::ProtoError) -> LoadError {
    use tabnas_proto::ProtoError;
    match e {
        ProtoError::TooDeep(_) => {
            let (message, hint) =
                too_deep_words(format, Deep::Braces(tabnas_proto::MAX_NESTING_DEPTH));
            LoadError::tagged("too_deep", message).with_hint(&hint)
        }
        ProtoError::Version(message) => {
            let detail = message.strip_prefix("proto: ").unwrap_or(&message);
            LoadError::refused(
                format,
                "version",
                format!("version: {detail}"),
                "A .proto file declares its version as syntax = \"proto2\" or \"proto3\", or \
                 edition = \"2023\" or \"2024\"; one that declares none is read as proto2.",
            )
        }
        ProtoError::Parse(e) => LoadError::from_tabnas(&e, None),
        ProtoError::Grammar(message) => grammar_failed(format, &message),
    }
}

/// Split text into lines ended by `\n` or `\r\n`, the last one's ending optional.
pub fn lines(src: &str) -> Vec<&str> {
    let mut out: Vec<&str> = src
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .collect();
    if out.last() == Some(&"") {
        out.pop();
    }
    out
}

/// How many lines [`lines`] splits `src` into, without collecting them.
pub fn line_count(src: &str) -> usize {
    let breaks = src.bytes().filter(|&b| b == b'\n').count();
    if src.is_empty() || src.ends_with('\n') {
        breaks
    } else {
        breaks + 1
    }
}

thread_local! {
    /// How many guarded parses this thread is inside.
    static CATCHING: Cell<usize> = const { Cell::new(0) };
}

/// Whether a grammar runs under [`parse`]'s panic guard on this thread. The terminal's panic
/// hook, which restores the screen, stands down then, since the panic is caught and reported; a
/// panic on another thread is not caught, so the answer is per thread.
pub fn parse_in_progress() -> bool {
    CATCHING.try_with(|c| c.get() > 0).unwrap_or(false)
}

pub(crate) struct CatchGuard;

impl CatchGuard {
    pub(crate) fn enter() -> CatchGuard {
        CATCHING.with(|c| c.set(c.get() + 1));
        CatchGuard
    }
}

impl Drop for CatchGuard {
    fn drop(&mut self) {
        let _ = CATCHING.try_with(|c| c.set(c.get() - 1));
    }
}

/// Run a grammar's code under the panic guard: a defect in a plugin must not take the process
/// down. A panic comes back as its message.
pub(crate) fn catch_grammar<T>(work: impl FnOnce() -> T) -> Result<T, String> {
    let _guard = CatchGuard::enter();
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
        .map_err(|panic| panic_message(&*panic))
}

/// What a panic said, or `unknown panic`.
fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
    panic
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_else(|| "unknown panic".to_string())
}

/// The error for a grammar that panicked, as the loader reports one.
pub(crate) fn grammar_failed(format: Format, what: &str) -> LoadError {
    LoadError::tagged("grammar", format!("{format} grammar failed: {what}"))
}

/// Parse `src` as `format`, within the time limit of [`Limits::current`].
pub fn parse(src: &str, format: Format) -> Result<Doc, LoadError> {
    parse_within(src, format, Limits::current().timeout)
}

/// Parse `src` as `format`, stopping after `timeout`. It runs on its own thread with
/// [`PARSE_STACK`] under the rule cap ([`MAX_RULE_DEPTH`]), so nesting fails as `too_deep`
/// instead of overflowing the stack, which no error can catch. Past `timeout` it fails as
/// `timeout`, placed where it stopped or, when its last step ran late, as it finishes.
pub fn parse_within(
    src: &str,
    format: Format,
    timeout: Option<Duration>,
) -> Result<Doc, LoadError> {
    if format == Format::Text {
        let src = src.strip_prefix('\u{feff}').unwrap_or(src);
        return Ok(Doc::from_lines(&lines(src)));
    }
    let src = parse_text(src, format);
    if format.is_notation() {
        return on_parse_thread(
            timeout,
            || {},
            |deadline| compile_notation_by(format, src, deadline).map(|v| Doc::from_value(&v)),
        );
    }
    on_parse_thread(timeout, || {}, |deadline| parse_here(src, format, deadline))
}

/// Run `work`, a parse, on a thread with [`PARSE_STACK`], handing it the deadline `timeout`
/// sets from now. This thread only waits, so it keeps the time: past the deadline
/// it raises the alarm [`Deadline::passed`] reads and calls `on_alarm`. With no thread to be
/// had, `work` runs here on a clock-read deadline and `on_alarm` is never called.
pub(crate) fn on_parse_thread<T: Send>(
    timeout: Option<Duration>,
    on_alarm: impl FnOnce() + Send,
    work: impl FnOnce(Option<Deadline>) -> T + Send,
) -> T {
    // A time too far off to reach is no limit.
    let deadline = timeout.and_then(|limit| {
        let at = Instant::now().checked_add(limit)?;
        Some(Deadline {
            at,
            limit,
            alarm: None,
            cancel: None,
        })
    });
    let alarm = Arc::new(AtomicBool::new(false));
    let watched = deadline.clone().map(|d| Deadline {
        alarm: Some(alarm.clone()),
        ..d
    });
    let (done, finished) = mpsc::channel::<()>();
    // The work is taken by whichever thread runs it: a spawn that fails
    // consumes its closure, so the work cannot be moved into that closure.
    let slot = std::sync::Mutex::new(Some(work));
    let take = || slot.lock().unwrap_or_else(|e| e.into_inner()).take();
    std::thread::scope(|scope| {
        let spawned = std::thread::Builder::new()
            .name("aless-parse".into())
            .stack_size(PARSE_STACK)
            .spawn_scoped(scope, || {
                // Hung up when the parse ends, however it ends.
                let _done = done;
                let work = take().expect("the work runs once");
                work(watched)
            });
        match spawned {
            Ok(handle) => {
                if let Some(d) = &deadline {
                    let left = d.at.saturating_duration_since(Instant::now());
                    if finished.recv_timeout(left) == Err(RecvTimeoutError::Timeout) {
                        alarm.store(true, Ordering::Relaxed);
                        on_alarm();
                    }
                }
                // A panic outside the grammar is aless's own: let it through.
                handle
                    .join()
                    .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
            }
            // No thread to be had: parse on this one, same caps, reading the clock itself.
            Err(_) => {
                let work = take().expect("a failed spawn never ran the work");
                work(deadline)
            }
        }
    })
}

fn parse_here(src: &str, format: Format, deadline: Option<Deadline>) -> Result<Doc, LoadError> {
    let Some(parser) = make_parser(format)? else {
        return Ok(Doc::from_lines(&lines(src)));
    };
    parse_with(parser, src, format, deadline)
}

/// Parse `src` as `format` with `parser`, under aless's caps.
fn parse_with(
    parser: Tabnas,
    src: &str,
    format: Format,
    deadline: Option<Deadline>,
) -> Result<Doc, LoadError> {
    parse_capped(parser, src, format, deadline, MAX_RULE_DEPTH)
}

/// [`parse_with`] under a rule cap of `max_depth`, which a test lowers.
fn parse_capped(
    mut parser: Tabnas,
    src: &str,
    format: Format,
    deadline: Option<Deadline>,
    max_depth: usize,
) -> Result<Doc, LoadError> {
    let stopped = guard(&mut parser, deadline.clone(), max_depth, || {});
    parse_guarded(parser, &stopped, src, format, deadline, max_depth)
}

/// [`parse_capped`] with the guard already on the parser: `stopped` is its record.
fn parse_guarded(
    mut parser: Tabnas,
    stopped: &Stop,
    src: &str,
    format: Format,
    deadline: Option<Deadline>,
    max_depth: usize,
) -> Result<Doc, LoadError> {
    check_text(format, src)?;
    // A custom grammar's lone `*` is a word and a value; CSV and TSV align by column, from
    // separators and line ends; a C tree carries every token's place itself.
    let sink = if format == Format::C {
        None
    } else if format.is_custom() {
        Some(prov::capture_words(&mut parser))
    } else if format.keyed_by_header() {
        Some(prov::capture_cells(&mut parser))
    } else if format == Format::Pgn {
        // tabnas-chess numbers moves itself, so the tree holds numbers the source never spells,
        // which, matched by value, would take the next game's `1.`: move numbers and glyphs
        // place nothing.
        Some(prov::capture_except(&mut parser, &["#MVN", "#NAG"]))
    } else {
        Some(prov::capture(&mut parser))
    };
    // A grammar is a plugin; a defect in one must not take the viewer down.
    let outcome = {
        let _guard = CatchGuard::enter();
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            parse_value(&parser, format, src)
        }))
    };
    let stop = stopped.why();
    let now = Instant::now();
    // The time is checked between steps, and a step is not cut short, so a parse can end late
    // with a value or an error: still too late.
    let late = deadline
        .as_ref()
        .filter(|d| stop == 0 && outcome.is_ok() && now >= d.at);
    if let Some(d) = late {
        let took = d.limit + now.saturating_duration_since(d.at);
        return Err(LoadError::finished_late(d.limit, took));
    }
    match outcome {
        Ok(Ok(value)) => {
            // The parser goes first: should it share any of the value, the
            // value's containers are then the last holders.
            drop(parser);
            if format.is_custom() && nests_past(&value, MAX_VALUE_DEPTH) {
                drop_deep(value);
                let (message, hint) = too_deep_words(format, Deep::Value);
                return Err(LoadError::tagged("too_deep", message).with_hint(&hint));
            }
            let value = read_as(format, value, src.len())?;
            let mut doc = Doc::from_value(&value);
            // Dropped before the alignment, to lower the peak on a large document.
            drop(value);
            match sink {
                None => prov::align_spans(&mut doc),
                Some(sink) => {
                    if let Ok(toks) = sink.lock() {
                        // The header row is lexed once, ahead of every record it keys.
                        if format.keyed_by_header() {
                            prov::align_records(&mut doc, &toks);
                        } else {
                            prov::align(&mut doc, &toks);
                        }
                    }
                }
            }
            Ok(doc)
        }
        Ok(Err(e)) if stop == STOP_DEPTH => {
            let mut e = *e;
            stopped.place(&mut e);
            e.tag = "aless".to_string();
            e.code = "too_deep".to_string();
            (e.detail, e.hint) = too_deep_words(format, Deep::Rules(max_depth));
            Err(LoadError::from_tabnas(&e, None))
        }
        Ok(Err(e)) if stop == STOP_TIME => {
            let mut e = *e;
            stopped.place(&mut e);
            let limit = seconds(deadline.map_or(Duration::ZERO, |d| d.limit));
            e.tag = "aless".to_string();
            e.code = "timeout".to_string();
            e.detail = format!("timeout: the parse ran longer than {limit} s");
            e.hint = format!(
                "The parse had got this far when --timeout {limit} stopped it.\nPass a \
                 larger --timeout to let it finish, or --timeout 0 for no limit."
            );
            Err(LoadError::from_tabnas(&e, None))
        }
        // A C parse stopped with no token in hand (tabnas-expr's bound between tokens, or a
        // tree tabnas-c could not realize) has no place to report.
        Ok(Err(e)) if e.code == "cancel" && unplaced_c_cancel(format, &e) => {
            let (message, hint) = too_deep_words(format, Deep::Grammar);
            Err(LoadError::tagged("too_deep", message).with_hint(&hint))
        }
        // A grammar's own depth limit (jsonic's parse guard, XML's lexer) stops the parse with
        // `cancel`, which nothing else here raises; its hint would blame the caller's budget.
        Ok(Err(e)) if e.code == "cancel" => {
            let mut e = *e;
            e.tag = "aless".to_string();
            e.code = "too_deep".to_string();
            (e.detail, e.hint) = too_deep_words(format, Deep::Grammar);
            Err(LoadError::from_tabnas(&e, None))
        }
        Ok(Err(e)) => {
            let hints = parser.config().hint;
            Err(LoadError::from_tabnas(&e, hint_template(&hints, &e.code)))
        }
        Err(panic) => Err(grammar_failed(format, &panic_message(&*panic))),
    }
}

/// Stop the parse when its rule stack passes `max_depth` or its deadline passes, checked at
/// every step of the parse budget. A grammar's depth guard runs beside the budget and stays,
/// and a budget it keeps still runs at its own interval. `notify` is called on a stop.
pub(crate) fn guard(
    parser: &mut Tabnas,
    deadline: Option<Deadline>,
    max_depth: usize,
    notify: impl Fn() + Send + Sync + 'static,
) -> Arc<Stop> {
    let stopped = Stop::new();
    let flag = stopped.clone();
    let own = parser.config().parse.budget;
    let (every, check) = (own.check_every_n, own.on_check);
    parser.parse_budget(1, move |ctx| {
        if ctx.rule_stack.len() > max_depth {
            flag.record(STOP_DEPTH, ctx);
            notify();
            return false;
        }
        if deadline.as_ref().is_some_and(Deadline::passed) {
            flag.record(STOP_TIME, ctx);
            notify();
            return false;
        }
        match &check {
            Some(check) if every > 0 && ctx.iteration % every == 0 => check(ctx),
            _ => true,
        }
    });
    stopped
}

/// Why a parse was `too_deep`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Deep {
    /// Its rule stack passed this cap (the shared [`MAX_RULE_DEPTH`]).
    Rules(usize),
    /// A custom grammar's value nests past [`MAX_VALUE_DEPTH`], found after the parse.
    Value,
    /// The grammar's own nesting limit stopped it (the engine's `cancel`).
    Grammar,
    /// The format's crate found the braces nested past this, before the parse (proto's).
    Braces(usize),
    /// A C tree holds more written out than [`c_tree_cap`] allows.
    Repeated,
}

/// The message and hint of a `too_deep` error; the message starts with its code.
pub(crate) fn too_deep_words(format: Format, why: Deep) -> (String, String) {
    match why {
        Deep::Rules(cap) if format.is_custom() => (
            format!("too_deep: the {format} grammar had {cap} rules open at once"),
            "Rule depth follows the nesting of the input and of the grammar's own recursion, \
             not the length of a file: a repetition adds none. The parse stopped here.\nReal \
             documents nest a few dozen levels at most."
                .to_string(),
        ),
        Deep::Rules(cap) => (
            format!(
                "too_deep: nested deeper than aless reads (about {} levels)",
                cap / 3
            ),
            "Nesting this deep would overflow the parser's stack, so the parse stopped \
             here.\nReal documents nest a few dozen levels at most."
                .to_string(),
        ),
        Deep::Value => (
            format!(
                "too_deep: the {format} grammar built a value nested deeper than aless reads \
                 (about {MAX_VALUE_DEPTH} levels)"
            ),
            "The value is measured once the parse is done: a grammar from the command line \
             builds it in the compiler's shape, which its rule stack does not count in the \
             same units.\nReal documents nest a few dozen levels at most."
                .to_string(),
        ),
        // An expression's tree nests each operation in the next, so a long flat one is deep
        // too: the grammar bounds the operations an expression holds.
        Deep::Grammar if format == Format::Expr => (
            "too_deep: an expression larger or deeper than the expr grammar reads".to_string(),
            format!(
                "The expr grammar holds at most {limit} operations in one expression, a group \
                 in parentheses among them, and nests lists and maps at most {limit} levels \
                 deep; the parse stopped where the document passed one of them.\nSplit a long \
                 expression into several.",
                limit = tabnas_expr::NODE_LIMIT
            ),
        ),
        // C's limits are its reader's: tabnas-expr's on expressions, and tabnas-c's on the
        // realized tree, which a conditional that is a whole initializer passes.
        Deep::Grammar if format == Format::C => (
            "too_deep: an expression larger, or code nested deeper, than the c reader reads"
                .to_string(),
            format!(
                "The c reader holds at most {limit} operations in one expression, the \
                 arguments of a call among them, keeps at most {rules} rules open, and builds \
                 a syntax tree at most {tree} levels deep; the parse stopped where the document \
                 passed one of them. It reads a conditional that is a whole initializer, as \
                 in int a = b ? c : d;, as a tree that holds itself, and refuses it the same \
                 way.\nSplit a long expression into several.",
                limit = tabnas_expr::NODE_LIMIT,
                rules = tabnas_expr::RULE_LIMIT,
                tree = tabnas_c::REALIZE_DEPTH_CAP
            ),
        ),
        Deep::Grammar => (
            format!("too_deep: nested deeper than the {format} grammar reads"),
            format!(
                "The {format} grammar has a nesting limit of its own, and the parse stopped \
                 where the document passed it.\nReal documents nest a few dozen levels at most."
            ),
        ),
        Deep::Braces(levels) => (
            format!("too_deep: nested deeper than the {format} grammar reads"),
            format!(
                "The {format} reader counts a document's braces before it parses it, and \
                 refuses one that nests them more than {levels} levels deep.\nReal documents \
                 nest a few levels at most."
            ),
        ),
        Deep::Repeated => (
            format!("too_deep: a {format} syntax tree larger written out than aless reads"),
            format!(
                "The {format} reader repeats a node's subtrees under the members that name \
                 them, such as operand and cond, beside its children, so each level of a chain \
                 of subscripts or member accesses, a run of prefix operators, a conditional or \
                 a brace initializer doubles the tree written out. aless reads one of at most \
                 {C_TREE_PER_BYTE} values a byte of its source, of 1,048,576 at least and of \
                 33,554,432 at most, holding at most 2,048 bytes of names and strings a byte, \
                 of 8 MB at least and of 256 MB at most, and stopped before writing out this \
                 one.\nReal code nests these a few levels at most."
            ),
        ),
    }
}

/// Whether `value` nests more than `cap` levels deep, the root at level one, walked with its
/// own stack: it may be too deep to recurse over.
pub(crate) fn nests_past(value: &tabnas::Value, cap: usize) -> bool {
    use tabnas::Value;
    let mut stack = vec![(value, 1usize)];
    while let Some((v, depth)) = stack.pop() {
        if depth > cap {
            return true;
        }
        match v {
            Value::Array(items) => stack.extend(items.iter().map(|c| (c, depth + 1))),
            Value::ListRef(list) => {
                stack.extend(list.value.iter().map(|c| (c, depth + 1)));
                // Measured as `drop_deep` frees it: depth held in `child` is depth too.
                if let Some(child) = &list.child {
                    stack.push((child, depth + 1));
                }
            }
            Value::Object(map) => stack.extend(map.values().map(|c| (c, depth + 1))),
            Value::MapRef(map) => stack.extend(map.value.values().map(|c| (c, depth + 1))),
            _ => {}
        }
    }
    false
}

/// Let go of a value without recursing: an ordinary drop recurses once per level, and a value
/// past [`MAX_VALUE_DEPTH`] has more levels than the stack has room for. A container is taken
/// from its `Arc` when this is its last holder.
pub(crate) fn drop_deep(value: tabnas::Value) {
    use tabnas::Value;
    let mut queue = vec![value];
    while let Some(v) = queue.pop() {
        match v {
            Value::Array(items) => {
                if let Ok(items) = Arc::try_unwrap(items) {
                    queue.extend(items);
                }
            }
            Value::ListRef(list) => {
                if let Ok(list) = Arc::try_unwrap(list) {
                    queue.extend(list.value);
                    if let Some(child) = list.child {
                        queue.push(*child);
                    }
                }
            }
            Value::Object(map) => {
                if let Ok(map) = Arc::try_unwrap(map) {
                    queue.extend(map.into_values());
                }
            }
            Value::MapRef(map) => {
                if let Ok(map) = Arc::try_unwrap(map) {
                    queue.extend(map.value.into_values());
                }
            }
            _ => {}
        }
    }
}

/// Parse an in-memory source (stdin) as `format`, within [`Limits::current`]'s time limit.
pub fn load_str(source: String, format: Format) -> Result<Loaded, LoadError> {
    load_str_within(source, format, Limits::current())
}

/// [`load_str`] within its own limits; the source is read, so only the time limit applies.
pub fn load_str_within(
    source: String,
    format: Format,
    limits: Limits,
) -> Result<Loaded, LoadError> {
    let doc = parse_within(&source, format, limits.timeout)?;
    Ok(Loaded {
        doc,
        format,
        source,
    })
}

/// How a report names a file: relative to the current directory when it is
/// under it (so the line and column stay in view), else as given.
pub fn origin_of(path: &Path) -> String {
    std::env::current_dir()
        .ok()
        .and_then(|cwd| path.strip_prefix(cwd).ok().map(Path::to_path_buf))
        .filter(|rel| !rel.as_os_str().is_empty())
        .unwrap_or_else(|| path.to_path_buf())
        .display()
        .to_string()
}

/// Read and parse a file within [`Limits::current`]; `format` overrides the extension.
pub fn load_path(path: &Path, format: Option<Format>) -> Result<Loaded, LoadError> {
    load_path_within(path, format, Limits::current())
}

/// [`load_path`] within its own limits.
pub fn load_path_within(
    path: &Path,
    format: Option<Format>,
    limits: Limits,
) -> Result<Loaded, LoadError> {
    let source = read_path_within(path, limits.max_size)?;
    let format = format.unwrap_or_else(|| Format::detect(path));
    load_str_within(source, format, limits).map_err(|e| e.with_origin(&origin_of(path)))
}

/// Read a file's text within `max_size`, a file known to be over refused unread; bytes that are
/// not UTF-8 are replaced.
pub fn read_path_within(path: &Path, max_size: Option<u64>) -> Result<String, LoadError> {
    let origin = origin_of(path);
    let file = std::fs::File::open(path)
        .map_err(|e| LoadError::new(e.to_string()).with_origin(&origin))?;
    if let (Some(max), Ok(meta)) = (max_size, file.metadata()) {
        if meta.is_file() && meta.len() > max {
            return Err(LoadError::too_large(Some(meta.len()), max).with_origin(&origin));
        }
    }
    let bytes = read_within(file, max_size).map_err(|e| e.with_origin(&origin))?;
    Ok(match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(e) => String::from_utf8_lossy(e.as_bytes()).into_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{Key, Kind};

    #[test]
    fn catch_guard_marks_the_parse() {
        assert!(!parse_in_progress());
        {
            let _g = CatchGuard::enter();
            assert!(parse_in_progress());
            // Only this thread's panics are the parse's.
            let elsewhere = std::thread::spawn(parse_in_progress).join().unwrap();
            assert!(!elsewhere);
        }
        assert!(!parse_in_progress());
    }

    #[test]
    fn a_grammar_panic_is_caught_as_its_message() {
        assert_eq!(catch_grammar(|| 1 + 1), Ok(2));
        assert_eq!(catch_grammar(|| panic!("boom")).unwrap_err(), "boom");
        let n = 3;
        assert_eq!(
            catch_grammar(|| -> () { panic!("bad {n}") }).unwrap_err(),
            "bad 3"
        );
        assert!(
            !parse_in_progress(),
            "the guard is left when the panic is caught"
        );
        let e = grammar_failed(Format::Json, "boom");
        assert_eq!(e.code, "grammar");
        assert_eq!(e.message, "json grammar failed: boom");
        assert!(!e.is_io() && !e.is_timeout() && !e.is_too_large());
    }

    #[test]
    fn detection() {
        assert_eq!(Format::detect(Path::new("a/b.json")), Format::Json);
        assert_eq!(Format::detect(Path::new("x.YML")), Format::Yaml);
        assert_eq!(Format::detect(Path::new("notes")), Format::Text);
        assert_eq!(Format::detect(Path::new("weird.xyz")), Format::Text);
        assert_eq!(Format::detect_known(Path::new("weird.xyz")), None);
        assert_eq!(
            Format::detect_known(Path::new("a.toml")),
            Some(Format::Toml)
        );
        assert_eq!(Format::from_name("md"), Some(Format::Markdown));
        assert_eq!(Format::from_name("Markdown"), Some(Format::Markdown));
        assert_eq!(Format::from_name("nope"), None);
        assert_eq!(Format::from_name("ndjson"), Some(Format::Jsonl));
        assert!(Format::known_names().starts_with(&["json", "jsonl"]));
        assert!(!Format::Json.is_custom());
        assert_eq!(Format::detect(Path::new("site.css")), Format::Css);
        assert_eq!(Format::detect(Path::new("api/v1.proto")), Format::Proto);
        assert_eq!(Format::detect(Path::new("games.PGN")), Format::Pgn);
        assert_eq!(Format::detect(Path::new("src/main.c")), Format::C);
        assert_eq!(Format::detect(Path::new("include/vec.H")), Format::C);
        assert_eq!(Format::from_name("c"), Some(Format::C));
        assert!(Format::Expr.extensions().is_empty() && Format::Semver.extensions().is_empty());
        assert_eq!(Format::detect(Path::new("calc.expr")), Format::Text);
        assert_eq!(Format::detect(Path::new("VERSION")), Format::Text);
        assert_eq!(Format::from_name("expr"), Some(Format::Expr));
        assert_eq!(Format::from_name("semver"), Some(Format::Semver));
        assert_eq!(Format::from_name("pgn"), Some(Format::Pgn));
        assert_eq!(
            Format::from_name("chess"),
            None,
            "PGN goes by its languageId"
        );
    }

    #[test]
    fn custom_grammars_are_detected_by_file_name_or_extension() {
        let def = grammar::Definition::parse(
            "--grammar-expr",
            "impl-etc-hosts,impl-etc-hosts.conf=doc = *entry\nentry = TX \"=\" TX\n",
        )
        .unwrap();
        let id = grammar::register(def, Limits::NONE).unwrap();
        let custom = Format::Custom(id);
        for path in [
            "/etc/impl-etc-hosts",
            "impl-etc-hosts",
            "./IMPL-ETC-HOSTS",
            "backup/hosts.impl-etc-hosts",
            "/etc/impl-etc-hosts.conf",
            "Impl-Etc-Hosts.Conf",
        ] {
            assert_eq!(Format::detect(Path::new(path)), custom, "{path}");
            assert_eq!(
                Format::detect_known(Path::new(path)),
                Some(custom),
                "{path}"
            );
        }
        for path in ["impl-etc-hosts.bak", "impl-etc-hosts/x.json", "x.conf"] {
            assert_ne!(Format::detect(Path::new(path)), custom, "{path}");
        }
        assert_eq!(Format::from_name("impl-etc-hosts.conf"), Some(custom));
        assert!(custom.is_custom());
        assert!(Format::known_names().contains(&"impl-etc-hosts"));
        // `:` is an ordinary character in plain text.
        let doc = parse("a: = b:c\nd = e\n", custom).unwrap();
        assert!(doc.len() > 1);
        // The position is the entry the parse could not complete.
        let e = parse("a = 1\nb\nc = 3\n", custom).unwrap_err();
        assert_eq!((e.line, e.col), (2, 1), "{e}");
        assert_eq!(e.code, "unexpected");
    }

    /// The deepest the engine's rule stack gets over a parse of `src`, as the guard reads it.
    fn deepest_rule(mut parser: Tabnas, src: &str) -> usize {
        let deepest = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen = deepest.clone();
        parser.parse_budget(1, move |ctx| {
            seen.fetch_max(ctx.rule_stack.len(), Ordering::Relaxed);
            true
        });
        parser.parse(src).unwrap_or_else(|e| panic!("{e}"));
        deepest.load(Ordering::Relaxed)
    }

    /// A repetition in a command-line grammar adds no rule depth (ADR-007).
    #[test]
    fn a_custom_grammars_repetition_is_not_nesting() {
        let def = grammar::Definition::parse(
            "--grammar-expr",
            &format!("impl-kv-l={}", grammar::tests::KV),
        )
        .unwrap();
        let custom = Format::Custom(grammar::register(def, Limits::NONE).unwrap());
        let lines = |n: usize| -> String { (0..n).map(|i| format!("k{i}=v{i}\n")).collect() };
        let flat = lines(2_000);
        let doc = parse(&flat, custom).unwrap();
        assert_eq!(doc.node(0).children, 2_000);
        assert_eq!(doc.nodes.iter().map(|n| n.depth).max(), Some(2));
        // A long file's rule stack is as deep as a short one's, and far under the cap.
        let two = deepest_rule(make_parser(custom).unwrap().unwrap(), &lines(2));
        let many = deepest_rule(make_parser(custom).unwrap().unwrap(), &flat);
        assert_eq!(
            many, two,
            "2,000 lines reach rule depth {many} where two lines reach {two}"
        );
        assert!(two < MAX_RULE_DEPTH / 100, "{two}");
        // So a cap at that depth takes the whole file...
        let parser = make_parser(custom).unwrap().unwrap();
        let doc = parse_capped(parser, &flat, custom, None, two).unwrap();
        assert_eq!(doc.node(0).children, 2_000);
        // ...and under a lower one it is `too_deep`, naming the open rules and the place.
        let parser = make_parser(custom).unwrap().unwrap();
        let e = parse_capped(parser, &flat, custom, None, two - 1).unwrap_err();
        assert_eq!(e.code, "too_deep");
        assert_eq!(
            e.message,
            format!(
                "too_deep: the impl-kv-l grammar had {} rules open at once",
                two - 1
            )
        );
        assert!(e.hint.contains("a repetition adds none"), "{}", e.hint);
        assert!(e.line > 0, "{e}");
        assert!(e.plain_report().starts_with("[aless/too_deep]"));
        // A built-in grammar's message names levels.
        let (message, hint) = too_deep_words(Format::Yaml, Deep::Rules(MAX_RULE_DEPTH));
        assert_eq!(
            message,
            "too_deep: nested deeper than aless reads (about 1000 levels)"
        );
        assert!(hint.contains("overflow the parser's stack"), "{hint}");
    }

    #[test]
    fn a_ten_thousand_line_hosts_file_parses_under_the_shared_cap() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/grammars/hosts.abnf"
        );
        let def = grammar::Definition::parse("--grammar", &format!("hosts-l={path}")).unwrap();
        let custom = Format::Custom(grammar::register(def, Limits::NONE).unwrap());
        let lines = |n: usize| -> String {
            (0..n)
                .map(|i| {
                    format!(
                        "10.0.{}.{}\thost{i}.example.com alias{i}\n",
                        (i >> 8) & 255,
                        i & 255
                    )
                })
                .collect()
        };
        let doc = parse(&lines(10_000), custom).unwrap();
        assert_eq!(doc.node(0).children, 10_000);
        let three = deepest_rule(make_parser(custom).unwrap().unwrap(), &lines(3));
        let many = deepest_rule(make_parser(custom).unwrap().unwrap(), &lines(10_000));
        assert_eq!(
            many, three,
            "10,000 lines reach rule depth {many} where three lines reach {three}"
        );
        assert!(three < MAX_RULE_DEPTH / 100, "{three}");
    }

    #[test]
    fn a_custom_value_nested_past_the_cap_is_too_deep() {
        let def = grammar::Definition::parse(
            "--grammar-expr",
            "impl-nest-l=doc = \"(\" doc \")\" / \"x\"\n",
        )
        .unwrap();
        let custom = Format::Custom(grammar::register(def, Limits::NONE).unwrap());
        // The compiler's tree: an object and its `kids` array per level.
        let parens = |n: usize| "(".repeat(n) + "x" + &")".repeat(n);
        let doc = parse(&parens(400), custom).unwrap();
        let deepest = doc.nodes.iter().map(|n| n.depth).max().unwrap();
        assert!((700..1000).contains(&deepest), "{deepest}");
        let e = parse(&parens(600), custom).unwrap_err();
        assert_eq!(e.code, "too_deep");
        assert_eq!(
            e.message,
            "too_deep: the impl-nest-l grammar built a value nested deeper than aless reads \
             (about 1000 levels)"
        );
        assert_eq!(
            (e.line, e.col),
            (0, 0),
            "found after the parse: no position"
        );
        assert!(e.plain_report().starts_with("[aless/too_deep]"));
    }

    #[test]
    fn deep_values_are_measured_and_dropped_without_recursion() {
        use tabnas::Value;
        assert!(!nests_past(&Value::Null, 1));
        assert!(nests_past(&Value::Array(Arc::new(vec![Value::Null])), 1));
        let mut v = Value::String("x".into());
        for i in 0..200_000 {
            v = if i % 2 == 0 {
                Value::Array(Arc::new(vec![v]))
            } else {
                Value::Object(Arc::new(std::iter::once(("k".to_string(), v)).collect()))
            };
        }
        assert!(nests_past(&v, MAX_VALUE_DEPTH));
        assert!(!nests_past(&v, 300_000));
        drop_deep(v);
    }

    #[test]
    fn a_lists_child_counts_toward_its_depth() {
        use tabnas::{ListRef, Value};
        let mut v = Value::Null;
        for _ in 0..(MAX_VALUE_DEPTH + 10) {
            v = Value::ListRef(Arc::new(ListRef {
                value: Vec::new(),
                implicit: false,
                child: Some(Box::new(v)),
                meta: Default::default(),
            }));
        }
        assert!(nests_past(&v, MAX_VALUE_DEPTH));
        assert!(!nests_past(&v, MAX_VALUE_DEPTH + 20));
        drop_deep(v);
    }

    #[test]
    fn text_lines() {
        assert_eq!(lines("a\r\nb\n\nc\n"), vec!["a", "b", "", "c"]);
        assert_eq!(lines(""), Vec::<&str>::new());
        assert_eq!(lines("x"), vec!["x"]);
        for text in [
            "",
            "x",
            "x\n",
            "\n",
            "\n\n",
            "a\r\nb",
            "a\r\nb\r\n",
            "a\r",
            "a\n\nb\n",
        ] {
            assert_eq!(line_count(text), lines(text).len(), "{text:?}");
        }
        let d = parse("one\ntwo\n", Format::Text).unwrap();
        assert_eq!(d.len(), 3);
        assert_eq!(d.node(2).kind, Kind::Str("two".into()));
        assert_eq!(d.node(2).line, 2);
    }

    #[test]
    fn sizes_parse_as_max_size_takes_them() {
        assert_eq!(parse_size("64M"), Ok(Some(64 << 20)));
        assert_eq!(parse_size("64mib"), Ok(Some(64 << 20)));
        assert_eq!(parse_size("1G"), Ok(Some(1 << 30)));
        assert_eq!(parse_size("512k"), Ok(Some(512 << 10)));
        assert_eq!(parse_size(" 1000 "), Ok(Some(1000)));
        assert_eq!(parse_size("0"), Ok(None));
        assert_eq!(parse_size("0M"), Ok(None));
        for bad in ["", "M", "1T", "-1", "1.5M", "lots", "99999999999999999999G"] {
            assert!(parse_size(bad).is_err(), "{bad:?}");
        }
        assert_eq!(size_flag(64 << 20), "64M");
        assert_eq!(size_flag(1 << 30), "1G");
        assert_eq!(size_flag(1536), "1536");
        assert_eq!(size_flag(3 << 10), "3K");
    }

    #[test]
    fn reading_stops_at_the_limit() {
        assert_eq!(read_within(&b"abcd"[..], Some(4)).unwrap(), b"abcd");
        assert_eq!(read_within(&b"abcd"[..], None).unwrap(), b"abcd");
        let e = read_within(&b"abcde"[..], Some(4)).unwrap_err();
        assert!(e.is_too_large());
        assert_eq!(e.message, "input is over the 4 B limit");
        assert!(e.hint.contains("such as 16,"), "{}", e.hint);
        // An endless input is not read past the limit.
        let e = read_within(std::io::repeat(b'x'), Some(1 << 20)).unwrap_err();
        assert!(e.is_too_large());
    }

    fn sized(max_size: Option<u64>) -> Limits {
        Limits {
            max_size,
            timeout: None,
        }
    }

    #[test]
    fn a_file_over_the_limit_is_refused_unread() {
        let dir = std::env::temp_dir().join(format!("aless-size-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let big = dir.join("big.json");
        std::fs::write(&big, format!("[{}1]", "1, ".repeat(2000))).unwrap();
        let e = load_path_within(&big, None, sized(Some(1024))).unwrap_err();
        assert!(e.is_too_large());
        assert_eq!(e.message, "input is 5.9 KB, over the 1.0 KB limit");
        assert!(
            e.hint.contains("Pass --max-size 8K (or more)"),
            "{}",
            e.hint
        );
        let report = e.plain_report();
        assert!(
            report.starts_with("[aless/too_large]: input is 5.9 KB"),
            "{report}"
        );
        assert!(report.contains("big.json"), "{report}");
        assert!(load_path_within(&big, None, sized(Some(1 << 20))).is_ok());
        assert!(load_path_within(&big, None, sized(None)).is_ok());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// TOML of `n` array tables: a parse of many steps, to stop part-way.
    fn many_tables(n: usize) -> String {
        (0..n)
            .map(|i| format!("[[item]]\nid = {i}\nname = \"item {i}\"\n\n"))
            .collect()
    }

    #[test]
    fn timeouts_parse_as_the_option_takes_them() {
        let secs = |s: f64| Ok(Some(Duration::from_secs_f64(s)));
        assert_eq!(parse_timeout("30"), secs(30.0));
        assert_eq!(parse_timeout("2.5"), secs(2.5));
        assert_eq!(parse_timeout("2.5s"), secs(2.5));
        assert_eq!(parse_timeout(" 1.5m "), secs(90.0));
        assert_eq!(parse_timeout("0"), Ok(None));
        assert_eq!(parse_timeout("0s"), Ok(None));
        for bad in ["", "s", "-1", "inf", "NaN", "10ms", "soon", "1e400"] {
            assert!(parse_timeout(bad).is_err(), "{bad:?}");
        }
        assert_eq!(seconds(Duration::from_millis(2500)), "2.5");
        assert_eq!(seconds(Duration::from_secs(30)), "30");
    }

    #[test]
    fn the_process_limits_default_and_encode() {
        // No test sets the process's limits: a parse in a parallel test would see them.
        assert_eq!(Limits::current(), Limits::DEFAULT);
        assert_eq!(Limits::default(), Limits::DEFAULT);
        assert_eq!(timeout_ms(None), 0);
        assert_eq!(timeout_ms(Some(Duration::from_millis(1500))), 1500);
        // Under a millisecond rounds up, not to "no limit".
        assert_eq!(timeout_ms(Some(Duration::from_nanos(1))), 1);
    }

    #[test]
    fn a_parse_past_its_timeout_stops_where_it_got_to() {
        let src = many_tables(2_000);
        let e = parse_within(&src, Format::Toml, Some(Duration::from_millis(1))).unwrap_err();
        assert!(e.is_timeout(), "{}", e.message);
        assert_eq!(e.message, "timeout: the parse ran longer than 0.001 s");
        assert!(e.line > 0, "the report shows how far the parse got");
        assert!(e.hint.contains("--timeout 0 for no limit"), "{}", e.hint);
        assert!(e.plain_report().starts_with("[aless/timeout]"));
        let quick = parse_within(&many_tables(2), Format::Toml, Some(Duration::from_secs(60)));
        assert_eq!(quick.unwrap().len(), 1 + 1 + 2 * 3);
    }

    /// A JSON array of `n` small records, one a line.
    fn records(n: usize) -> String {
        let items: String = (0..n).map(|i| format!("{{\"n\": {i}}},\n")).collect();
        format!("[\n{items}{{}}]\n")
    }

    /// Raise `up` at a step with no token to read next, once one on `line` was read: where the
    /// engine's `cancel` has no place. Put on after [`guard`], which sees `up` in the same
    /// call.
    fn between_steps_past(parser: &mut Tabnas, line: usize, up: Arc<AtomicBool>) {
        let guarded = parser.config().parse.budget;
        parser.parse_budget(1, move |ctx| {
            if ctx.t.is_empty() && ctx.v1().is_some_and(|t| t.site.ri >= line) {
                up.store(true, Ordering::Relaxed);
            }
            guarded.on_check.as_ref().is_none_or(|check| check(ctx))
        });
    }

    #[test]
    fn a_timeout_between_steps_is_placed_at_the_last_token_read() {
        let src = records(300);
        let mut parser = make_parser(Format::Json).unwrap().unwrap();
        let up = Arc::new(AtomicBool::new(false));
        let deadline = Deadline::after(Some(Duration::from_secs(600)))
            .unwrap()
            .or_cancelled(up.clone());
        let stopped = guard(&mut parser, Some(deadline.clone()), MAX_RULE_DEPTH, || {});
        between_steps_past(&mut parser, 200, up);
        let e = parse_guarded(
            parser,
            &stopped,
            &src,
            Format::Json,
            Some(deadline),
            MAX_RULE_DEPTH,
        )
        .unwrap_err();
        assert!(e.is_timeout(), "{}", e.message);
        assert!(
            (200..=201).contains(&e.line) && e.col > 0,
            "stopped at {}:{}",
            e.line,
            e.col
        );
        let line = e.source_line.as_deref().unwrap_or_default();
        assert!(line.contains("\"n\": 19"), "{line:?}");
        assert!(
            e.plain_report().contains(&format!(":{}:{}", e.line, e.col)),
            "{}",
            e.plain_report()
        );
        assert_eq!(stopped.position(), (e.line, e.col));
    }

    #[test]
    fn too_deep_between_steps_is_placed_at_the_last_token_read() {
        let src = format!("{}\n{}", "[\n".repeat(30), "]".repeat(30));
        let mut parser = make_parser(Format::Json).unwrap().unwrap();
        let stopped = guard(&mut parser, None, 40, || {});
        let e = parse_guarded(parser, &stopped, &src, Format::Json, None, 40).unwrap_err();
        assert_eq!(e.code, "too_deep", "{}", e.message);
        assert!(e.line > 1, "stopped at {}:{}", e.line, e.col);
        assert_eq!(stopped.position(), (e.line, e.col));
    }

    #[test]
    fn a_parse_of_few_steps_is_held_to_its_timeout_too() {
        // One long string is a handful of steps, nearly all the time in one.
        let src = format!("\"{}\"", "x".repeat(1 << 20));
        match parse_within(&src, Format::Json, Some(Duration::from_millis(1))) {
            Err(e) => assert!(e.is_timeout(), "{}", e.message),
            Ok(_) => panic!("a parse over its time was let through"),
        }
        assert!(parse_within(&src, Format::Json, Some(Duration::from_secs(600))).is_ok());
    }

    #[test]
    fn a_parse_that_ends_after_its_deadline_is_too_late() {
        // With no alarm, as when the last step runs late, the parse is judged as it ends.
        let silent = || Deadline {
            at: Instant::now(),
            limit: Duration::from_millis(1),
            alarm: Some(Arc::new(AtomicBool::new(false))),
            cancel: None,
        };
        for src in ["[1, 2, 3]", "[1, 2,"] {
            let e = parse_here(src, Format::Json, Some(silent())).unwrap_err();
            assert!(e.is_timeout(), "{src}: {}", e.message);
            assert_eq!(e.message, "timeout: the parse ran longer than 0.001 s");
            assert_eq!((e.line, e.col), (0, 0), "it stopped nowhere");
            assert!(e.source_line.is_none());
            assert!(
                e.hint.starts_with("The parse finished, but it took "),
                "{}",
                e.hint
            );
            assert!(e.plain_report().starts_with("[aless/timeout]"));
        }
        let later = Deadline {
            at: Instant::now() + Duration::from_secs(600),
            ..silent()
        };
        assert!(parse_here("[1, 2, 3]", Format::Json, Some(later)).is_ok());
    }

    #[test]
    fn with_no_thread_to_keep_the_time_the_parse_reads_the_clock() {
        let passed = Deadline {
            at: Instant::now(),
            limit: Duration::from_millis(1),
            alarm: None,
            cancel: None,
        };
        let e = parse_here(&many_tables(50), Format::Toml, Some(passed)).unwrap_err();
        assert!(e.is_timeout(), "{}", e.message);
        assert!(e.line > 0, "stopped where it had got to");
        assert!(e.hint.contains("got this far"), "{}", e.hint);
    }

    /// Parse as [`parse`] does, with YAML's own depth limit off, leaving the cap to stop it.
    fn parse_yaml_without_its_own_limit(src: &str) -> Result<Doc, LoadError> {
        std::thread::scope(|scope| {
            std::thread::Builder::new()
                .stack_size(PARSE_STACK)
                .spawn_scoped(scope, || {
                    let mut parser = make_parser(Format::Yaml)
                        .expect("YAML's grammar installs")
                        .expect("YAML has a grammar");
                    assert!(parser.parse_guards.contains_key("depth"));
                    parser.remove_parse_guard("depth");
                    parse_with(parser, src, Format::Yaml, None)
                })
                .expect("a parse thread")
                .join()
                .expect("the parse returns")
        })
    }

    #[test]
    fn nesting_deeper_than_the_cap_stops_cleanly() {
        // Without the cap, nesting this deep can overflow the stack and end the process.
        let yaml = "- ".repeat(50_000) + "x\n";
        let e = parse_yaml_without_its_own_limit(&yaml).unwrap_err();
        assert_eq!(e.code, "too_deep");
        assert!(e.message.contains("about 1000 levels"), "{}", e.message);
        assert!(e.plain_report().starts_with("[aless/too_deep]"));
        assert!(parse_yaml_without_its_own_limit(&("- ".repeat(500) + "x\n")).is_ok());
        // With its limit, YAML stops such a document itself, sooner.
        let e = parse(&yaml, Format::Yaml).unwrap_err();
        assert!(
            e.message.contains("the yaml grammar reads"),
            "{}",
            e.message
        );
    }

    #[test]
    fn a_grammars_own_lower_limit_is_too_deep_too() {
        // Each stops with the engine's `cancel`, and is reported as aless's own cap is.
        let json = |n: usize| "[".repeat(n) + &"]".repeat(n);
        let yaml = |n: usize| "- ".repeat(n) + "x\n";
        let xml = |n: usize| "<a>".repeat(n) + &"</a>".repeat(n);
        for (format, deep, within) in [
            (Format::Json, json(200), json(100)),
            (Format::Yaml, yaml(128), yaml(127)),
            (Format::Xml, xml(257), xml(256)),
        ] {
            let e = parse(&deep, format).unwrap_err();
            assert_eq!(e.code, "too_deep", "{format}");
            let own = format!("nested deeper than the {format} grammar reads");
            assert!(e.message.contains(&own), "{}", e.message);
            assert!(e.hint.contains("a nesting limit of its own"), "{}", e.hint);
            assert!(e.plain_report().starts_with("[aless/too_deep]"));
            assert!(parse(&within, format).is_ok(), "{format}");
        }
        // XML's report is of the tag that would open the 257th element.
        let e = parse(&xml(257), Format::Xml).unwrap_err();
        assert_eq!((e.line, e.col), (1, 256 * 3 + 1));
    }

    #[test]
    fn running_out_of_input_says_so() {
        for (src, format) in [
            ("{\"a\": 1, \"b\": ", Format::Json),
            ("[1, 2", Format::Json),
        ] {
            let e = parse(src, format).unwrap_err();
            assert_eq!(e.code, "unexpected");
            assert_eq!(e.message, "unexpected end of input", "{src}");
            assert!(e.hint.contains("ends before it is complete"), "{}", e.hint);
            let report = e.plain_report();
            assert!(report.contains("unexpected end of input"), "{report}");
            assert!(!report.contains("character(s)"), "{report}");
        }
        let e = parse("[1, @]", Format::Json).unwrap_err();
        assert_eq!(e.message, "unexpected character(s): @");
        assert_eq!(e.source_line.as_deref(), Some("[1, @]"));
    }

    #[test]
    fn parse_errors_carry_positions() {
        let e = parse("{\"a\": 1,\n\"b\":\n", Format::Json).unwrap_err();
        assert_eq!(e.line, 3);
        assert!(e.message.starts_with("unexpected"), "{}", e.message);
        assert_eq!(e.to_string(), format!("3:{}: {}", e.col, e.message));
        let io = load_path(Path::new("/nonexistent/x.json"), None).unwrap_err();
        assert_eq!(io.line, 0);
    }

    #[test]
    fn reports_are_the_engines_with_the_file_named() {
        let src = "{\n  \"a\": 1,\n  \"b\": [1, 2,,]\n}\n";
        let e = parse(src, Format::Json)
            .unwrap_err()
            .with_origin("data.json");
        assert_eq!((e.line, e.col), (3, 14));
        let plain = e.plain_report();
        let lines: Vec<&str> = plain.lines().collect();
        assert_eq!(lines[0], "[tabnas/unexpected]: unexpected character(s): ,");
        assert_eq!(lines[1], "  --> data.json:3:14");
        assert!(lines.contains(&"  3 |   \"b\": [1, 2,,]"), "{plain}");
        let caret = lines
            .iter()
            .position(|l| l.trim_start().starts_with('^'))
            .unwrap();
        assert_eq!(
            lines[caret].find('^'),
            lines[caret - 1].find(",,").map(|i| i + 1)
        );
        assert!(
            plain.contains("do not match any rule alternative"),
            "the hint: {plain}"
        );
        assert!(
            e.report.contains("\x1b[91m"),
            "the engine's colours are kept"
        );
    }

    #[test]
    fn control_characters_in_the_token_do_not_break_the_report() {
        let e = parse("{\"a\": \"unterminated\n}\n", Format::Json).unwrap_err();
        assert_eq!(e.message, "unprintable character: \\n");
        let plain = e.plain_report();
        let lines: Vec<&str> = plain.lines().collect();
        assert_eq!(lines[0], "[tabnas/unprintable]: unprintable character: \\n");
        assert!(lines[1].starts_with("  --> "), "{plain}");
        assert!(
            plain.contains("The character \\n (code point below 32) is not allowed inside a\n  string literal."),
            "{plain}"
        );
        assert_eq!(escape_controls("a\tb\u{1b}"), "a\\tb\\u001b");
    }

    #[test]
    fn origins_are_relative_to_the_current_directory() {
        let cwd = std::env::current_dir().unwrap();
        assert_eq!(
            origin_of(&cwd.join("a").join("b.json")),
            Path::new("a").join("b.json").display().to_string()
        );
        assert_eq!(origin_of(Path::new("rel.json")), "rel.json");
        let outside = std::env::temp_dir().join("x.json");
        if !outside.starts_with(&cwd) {
            assert_eq!(origin_of(&outside), outside.display().to_string());
        }
    }

    #[test]
    fn io_errors_report_in_the_same_layout() {
        let e = load_path(Path::new("/nonexistent/x.json"), None).unwrap_err();
        let plain = e.plain_report();
        assert!(plain.starts_with("[aless/io]: "), "{plain}");
        assert!(plain.contains("  --> /nonexistent/x.json"), "{plain}");
        assert_eq!(strip_ansi("a\x1b[1;31mb\x1b]52;c;x\x07c\x1b[0m"), "abc");
    }

    #[test]
    fn names_and_file_text_cannot_restyle_the_report() {
        // A name with a newline and an escape, and a message quoting the placeholder.
        let e = LoadError::new("cannot read <no-file>").with_origin("evil\n\x1b[31mname");
        let lines: Vec<&str> = e.report.lines().collect();
        assert_eq!(lines.len(), 2, "{:?}", e.report);
        assert!(lines[0].ends_with("cannot read <no-file>"), "{lines:?}");
        assert!(lines[1].ends_with("evil\\n\\u001b[31mname"), "{lines:?}");

        // An escape in the quoted source is a one-column picture, so the caret points at it.
        let e = parse("{\"a\": \"x\u{1b}[31my\"}", Format::Json).unwrap_err();
        assert!(!e.report.contains("\u{1b}[31my"), "{:?}", e.report);
        let plain = e.plain_report();
        let lines: Vec<&str> = plain.lines().collect();
        let quoted = lines.iter().position(|l| l.contains("x␛[31my")).unwrap();
        let at = |l: &str, c: char| l.chars().position(|x| x == c);
        assert_eq!(
            at(lines[quoted + 1], '^'),
            at(lines[quoted], '␛'),
            "{plain}"
        );
        // On a neighbouring line too.
        let e = parse("// \u{1b}[8mhidden\n{\"b\": [1,,2]}", Format::Jsonc).unwrap_err();
        assert!(e.plain_report().contains("  1 | // ␛[8mhidden"), "{e}");
    }

    #[test]
    fn long_lines_are_cut_to_a_window_around_the_error() {
        // A minified file, the error thousands of columns in.
        let body: Vec<String> = (0..1500).map(|i| i.to_string()).collect();
        let src = format!("[{},,9]", body.join(","));
        let col = src.find(",,").unwrap() + 2;
        let e = parse(&src, Format::Json)
            .unwrap_err()
            .with_origin("min.json");
        assert_eq!(e.col as usize, col);
        let plain = e.plain_report();
        let lines: Vec<&str> = plain.lines().collect();
        assert_eq!(
            lines[1],
            format!("  --> min.json:1:{col}"),
            "the file's column"
        );
        let quoted = lines
            .iter()
            .position(|l| l.starts_with("  1 | …"))
            .unwrap_or_else(|| panic!("{plain}"));
        let caret = lines[quoted + 1].chars().position(|c| c == '^').unwrap();
        assert!(caret < 60, "the caret is on screen: {plain}");
        assert_eq!(lines[quoted].chars().nth(caret), Some(','), "{plain}");

        // A long line beside the error is cut to the same window.
        let src = format!("[\"{}\",\n,1]", "x".repeat(300));
        let e = parse(&src, Format::Json).unwrap_err();
        assert_eq!((e.line, e.col), (2, 1));
        let plain = e.plain_report();
        let lines: Vec<&str> = plain.lines().collect();
        let long = format!("  1 | [\"{}…", "x".repeat(WINDOW - 2));
        assert!(lines.contains(&long.as_str()), "{plain}");
        let quoted = lines.iter().position(|l| l.starts_with("  2 | ")).unwrap();
        assert_eq!(
            lines[quoted + 1].find('^'),
            lines[quoted].find(','),
            "{plain}"
        );
    }

    #[test]
    fn carets_stop_at_the_end_of_their_line() {
        // An unterminated string's token is the rest of the file.
        let e = parse("a: `abc\nb: 1\nc: 2\n", Format::Jsonic).unwrap_err();
        let plain = e.plain_report();
        let caret = plain
            .lines()
            .find(|l| l.trim_start().starts_with('^'))
            .unwrap();
        assert!(
            caret
                .trim_start()
                .starts_with("^^^^ unterminated string: `abc\\nb"),
            "{plain}"
        );
    }

    #[test]
    fn hints_keep_their_own_line_breaks() {
        // jsonic's hint breaks its line before the token, a newline here.
        let e = parse("{a: 'abc\n, b: 1}\n", Format::Json5).unwrap_err();
        let plain = e.plain_report();
        assert!(
            plain.contains("(character codes\n  below 32). The character \\n is unprintable."),
            "{plain}"
        );
        // What every placeholder brought in is escaped, not only the token.
        assert_eq!(
            escape_hint(
                "A \n B\nC q\u{1b}[1mr D",
                Some("A {src} B\nC {name} D"),
                "\n"
            ),
            "A \\n B\nC q\\u001b[1mr D"
        );
        // Unmatched template: the token's first occurrence and any control are escaped.
        assert_eq!(
            escape_hint("x \n y\u{7}\nz", Some("other"), "\n"),
            "x \\n y\\u0007\nz"
        );
        assert_eq!(escape_hint("x \n y", None, "\n"), "x \\n y");
    }

    #[test]
    fn grammar_codes_carry_their_own_hints() {
        // Each code has a hint of its own, not the unknown code's, which blames the parser.
        for (src, format, code) in [
            ("[s\nk = v\n", Format::Ini, "unterminated_section"),
            ("0X2A", Format::Zon, "zon_number"),
        ] {
            let plain = parse(src, format).unwrap_err().plain_report();
            assert!(plain.contains(&format!("/{code}]:")), "{plain}");
            assert!(!plain.contains("probably a bug"), "{plain}");
        }
    }

    /// A JSON5 string continued across a CRLF reads as across an LF, every node and error in
    /// the same place; a raw CRLF after an escaped backslash is no continuation.
    #[test]
    fn json5_continues_a_string_across_crlf() {
        let lf = "{\n  a: 'x\\\n  y',\n  b: [1, \"p\\\nq\"], // c \\\n  c: 3\n}\n";
        let crlf = lf.replace('\n', "\r\n");
        let one = parse(lf, Format::Json5).unwrap();
        let two = parse(&crlf, Format::Json5).unwrap_or_else(|e| panic!("{e}"));
        let json = |d: &Doc| crate::fmt::to_json_compact(d, 0);
        assert_eq!(json(&two), r#"{"a":"x  y","b":[1,"pq"],"c":3}"#);
        assert_eq!(json(&two), json(&one));
        let at = |d: &Doc| d.nodes.iter().map(|n| (n.line, n.col)).collect::<Vec<_>>();
        assert_eq!(at(&two), at(&one));
        let lf = "['a\\\nb',\n  @]";
        let one = parse(lf, Format::Json5).unwrap_err();
        let two = parse(&lf.replace('\n', "\r\n"), Format::Json5).unwrap_err();
        assert_eq!((two.line, two.col), (one.line, one.col));
        assert_eq!(one.line, 3);
        assert!(parse("['a\\\\\r\nb']", Format::Json5).is_err());
        // An escaped backslash, then a continuation.
        let three = parse("['a\\\\\\\r\nb']", Format::Json5).unwrap();
        assert_eq!(crate::fmt::to_json_compact(&three, 0), r#"["a\\b"]"#);
    }

    /// A quoted CSV field holds any character but its quote, C0 controls included.
    #[test]
    fn csv_reads_control_characters_in_a_quoted_field() {
        let src = "a,b,c\n\"x\ty\",\"\u{1e}\",\"\u{1f}z\"\n";
        let doc = parse(src, Format::Csv).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            crate::fmt::to_json_compact(&doc, 0),
            r#"[{"a":"x\ty","b":"\u001e","c":"\u001fz"}]"#
        );
    }

    /// Every parser aless makes bounds how far a rule reaches back, and the bound changes no
    /// value: fixtures, grammar samples and a long document of each format parse alike.
    #[test]
    fn the_rule_history_bound_changes_no_value() {
        // Read as aless reads it (`read_as`): an expression's tree lives in the thread's arena.
        let both = |format: Option<Format>, make: &dyn Fn() -> Tabnas, src: &str, what: &str| {
            let bounded = make();
            // aless's bound, or a grammar's own where it is tighter (CSS's).
            assert!(
                bounded
                    .options
                    .rule
                    .history
                    .is_some_and(|h| h <= RULE_HISTORY),
                "{what}"
            );
            let mut unbounded = make();
            unbounded.options.rule.history = None;
            let run = |p: Tabnas| {
                match format {
                    Some(format) => parse_value(&p, format, src),
                    None => p.parse(src).map_err(Box::new),
                }
                .map_err(|e| e.to_string())
                .and_then(|v| match format {
                    Some(format) => read_as(format, v, src.len()).map_err(|e| e.to_string()),
                    None => Ok(v),
                })
                .map(|v| v.to_json())
            };
            assert_eq!(run(bounded), run(unbounded), "{what}");
        };
        // A notation's compiler parses with its own parser, which aless cannot bound.
        for format in Format::ALL
            .iter()
            .copied()
            .filter(|f| *f != Format::Text && !f.is_notation())
        {
            let make = || make_parser(format).unwrap().unwrap();
            // A sequence well past the bound, nested once.
            let items = |item: &dyn Fn(usize) -> String, sep: &str| {
                (0..40).map(item).collect::<Vec<_>>().join(sep)
            };
            let long = match format {
                Format::Json | Format::Jsonc | Format::Json5 | Format::Jsonic => format!(
                    "[{}]",
                    items(
                        &|i| format!("{{\"k{i}\": [{i}, \"v{i}\", {{\"n\": {i}}}]}}"),
                        ", "
                    )
                ),
                Format::Jsonl => items(&|i| format!("{{\"k\": {i}, \"v\": [1, 2]}}"), "\n"),
                Format::Yaml => items(&|i| format!("- k: {i}\n  v: [1, 2]\n  w: x{i}"), "\n"),
                Format::Toml => items(&|i| format!("[t{i}]\nk = {i}\nv = [1, 2]"), "\n"),
                Format::Ini => items(&|i| format!("[s{i}]\nk = {i}\nv = x{i}"), "\n"),
                Format::Csv => format!("a,b\n{}", items(&|i| format!("{i},x{i}"), "\n")),
                Format::Tsv => format!("a\tb\n{}", items(&|i| format!("{i}\tx{i}"), "\n")),
                Format::Xml => format!(
                    "<r>{}</r>",
                    items(&|i| format!("<i n=\"{i}\">x{i}</i>"), "")
                ),
                Format::Zon => format!(".{{ {} }}", items(&|i| format!(".k{i} = {i}"), ", ")),
                Format::Markdown => items(&|i| format!("- item {i}\n\npara {i}"), "\n\n"),
                Format::Feed => format!(
                    "<rss version=\"2.0\"><channel><title>t</title>{}</channel></rss>",
                    items(&|i| format!("<item><title>i{i}</title></item>"), "")
                ),
                Format::Css => items(&|i| format!(".c{i} {{ color: red; margin: {i}px }}"), "\n"),
                Format::Proto => format!(
                    "syntax = \"proto3\";\n{}",
                    items(
                        &|i| format!("message M{i} {{ int32 a = 1; string b = 2; }}"),
                        "\n"
                    )
                ),
                Format::Pgn => items(
                    &|i| format!("[Event \"e{i}\"]\n\n1. e4 e5 2. Nf3 Nc6 1-0"),
                    "\n\n",
                ),
                Format::C => items(
                    &|i| format!("int f{i}(int a) {{ return a * {i} + b[{i}].c; }}"),
                    "\n",
                ),
                Format::Expr => format!("[{}]", items(&|i| format!("{i}+{i}*({i}-1)"), ", ")),
                Format::Semver => format!(
                    "1.2.3-{}+{}",
                    items(&|i| format!("a{i}"), "."),
                    items(&|i| format!("b{i}"), ".")
                ),
                _ => continue,
            };
            both(
                Some(format),
                &make,
                &long,
                &format!("{format}: a long document"),
            );
        }
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let mut read = 0;
        for entry in std::fs::read_dir(&fixtures).unwrap() {
            let path = entry.unwrap().path();
            let format = match fixture_format(&path) {
                Some(Format::Text) | None => continue,
                Some(format) if format.is_notation() => continue,
                Some(format) => format,
            };
            let src = std::fs::read_to_string(&path).unwrap();
            let make = || make_parser(format).unwrap().unwrap();
            both(
                Some(format),
                &make,
                parse_text(&src, format),
                &path.display().to_string(),
            );
            read += 1;
        }
        let grammars = fixtures.join("grammars");
        for entry in std::fs::read_dir(&grammars).unwrap() {
            let path = entry.unwrap().path();
            let sample = path.with_extension("sample");
            if path.extension().and_then(|e| e.to_str()) != Some("abnf") || !sample.exists() {
                continue;
            }
            let compiled = grammar::compile(&std::fs::read_to_string(&path).unwrap()).unwrap();
            let make = || {
                let mut parser = compiled.parser().unwrap();
                parser.options.rule.history.get_or_insert(RULE_HISTORY);
                parser
            };
            let src = std::fs::read_to_string(&sample).unwrap();
            both(None, &make, &src, &path.display().to_string());
            read += 1;
        }
        assert!(read >= 25, "{read} fixtures read");
        // A grammar from the command line takes the bound from `make_parser` too.
        let def = grammar::Definition::parse(
            "--grammar-expr",
            &format!("impl-kv-h={}", grammar::tests::KV),
        )
        .unwrap();
        let custom = Format::Custom(grammar::register(def, Limits::NONE).unwrap());
        let parser = make_parser(custom).unwrap().unwrap();
        assert_eq!(parser.options.rule.history, Some(RULE_HISTORY));
    }

    /// JSONC takes a trailing comma, keeping its positions; an elision is still no value.
    #[test]
    fn jsonc_accepts_a_trailing_comma() {
        let json = |src: &str| {
            let doc = parse(src, Format::Jsonc).unwrap_or_else(|e| panic!("{src:?}: {e}"));
            crate::fmt::to_json_compact(&doc, 0)
        };
        assert_eq!(json("{\"a\": 1,}"), r#"{"a":1}"#);
        assert_eq!(json("[1, [2,],]"), "[1,[2]]");
        assert_eq!(
            json("{\n  // c\n  \"a\": [1,],\n  /* x */\n}"),
            r#"{"a":[1]}"#
        );
        let doc = parse("{\n  \"a\": 1,\n  \"b\": 2,\n}", Format::Jsonc).unwrap();
        let b = doc.resolve(&[Key::Name("b".into())]).unwrap() as usize;
        assert_eq!((doc.nodes[b].line, doc.nodes[b].col), (3, 3));
        assert!(parse("[1,,2]", Format::Jsonc).is_err());
    }

    /// The format a fixture is read as: its extension's, or the format its extension names when
    /// that format declares none (`sample.expr`), as `-k` names it.
    fn fixture_format(path: &Path) -> Option<Format> {
        Format::detect_known(path).or_else(|| {
            path.extension()
                .and_then(|e| e.to_str())
                .and_then(Format::from_name)
                .filter(|f| f.extensions().is_empty() && !f.is_custom())
        })
    }

    #[test]
    fn every_format_parses_its_sample() {
        let samples: [(Format, &str); 23] = [
            (Format::Json, "{\"a\": [1, 2]}"),
            (Format::Jsonl, "{\"a\": 1}\n{\"a\": 2}\n"),
            (Format::Jsonic, "a: 1, b: {c: x}"),
            (Format::Jsonc, "// c\n{\"a\": 1, /* x */ \"b\": 2}"),
            (Format::Json5, "{a: 'x', b: 0x10,}"),
            (Format::Yaml, "a: 1\nb:\n  - x\n  - y\n"),
            (Format::Toml, "title = \"t\"\n[owner]\nname = \"ada\"\n"),
            (Format::Ini, "[s]\nk = v\n"),
            (Format::Csv, "a,b\n1,2\n"),
            (Format::Tsv, "a\tb\n1\t2\n"),
            (Format::Xml, "<r><i id=\"1\">x</i></r>"),
            (Format::Zon, ".{ .name = \"z\", .ok = true }"),
            (Format::Markdown, "# T\n\npara\n"),
            (
                Format::Feed,
                "<?xml version=\"1.0\"?><rss version=\"2.0\"><channel><title>t</title><item><title>i</title></item></channel></rss>",
            ),
            (Format::Css, "a { color: red }\n@media print { b { margin: 0 } }\n"),
            (
                Format::Proto,
                "syntax = \"proto3\";\nmessage M { int32 a = 1; }\n",
            ),
            (Format::Pgn, "[Event \"e\"]\n\n1. e4 e5 1-0\n"),
            (Format::C, "#define N 2\nint x = N;\nint f(void) { return x; }\n"),
            (Format::Abnf, "greet = \"hi\" / \"hello\"\n"),
            (Format::Ebnf, "Greet ::= \"hi\" | \"hello\"\n"),
            (Format::Gbnf, "root ::= \"hi\" | \"hello\"\n"),
            (Format::Expr, "a: 1+2*3, b: (4)\n"),
            (Format::Semver, "1.2.3-rc.1+build.5\n"),
        ];
        // `ALL` lists every built-in and no custom grammar; each but plain text has a sample.
        assert_eq!(samples.len() + 1, Format::ALL.len());
        assert!(Format::ALL.contains(&Format::Text));
        assert!(!Format::ALL.iter().any(|f| f.is_custom()));
        for (format, src) in samples {
            let doc = parse(src, format).unwrap_or_else(|e| panic!("{format}: {e}"));
            assert!(doc.len() > 1, "{format} produced a scalar-only document");
            // A grammar's spec is its compiler's writing, and places nothing.
            assert_eq!(
                doc.nodes.iter().any(|n| n.line > 0),
                !format.is_notation(),
                "{format}: which nodes carry a source line"
            );
        }
    }

    /// A notation reads as the pure-data spec its compiler writes: rules, the start wrapper,
    /// tree builders and tokens, ABNF's literals matching either case (RFC 5234).
    #[test]
    fn a_grammar_reads_as_the_spec_its_compiler_writes() {
        let spec = |src: &str, format: Format| -> serde_json::Value {
            serde_json::from_str(&read_json(src, format)).unwrap()
        };
        let abnf = spec("greet = \"hi\" / \"hello\"\n", Format::Abnf);
        let rules: Vec<&String> = abnf["rule"].as_object().unwrap().keys().collect();
        assert_eq!(rules, ["greet", "__start__"]);
        assert_eq!(abnf["options"]["rule"]["start"], "__start__");
        assert_eq!(abnf["options"]["match"]["token"]["#HI"], "@~/^hi/i");
        assert_eq!(abnf["rule"]["greet"]["open"][0]["a"], "@node$");
        assert_eq!(abnf["rule"]["greet"]["open"][0]["g"], "abnf");
        let ebnf = spec(
            "Doc ::= Greet\nGreet ::= \"hi\" | \"hello\"\n",
            Format::Ebnf,
        );
        let rules: Vec<&String> = ebnf["rule"].as_object().unwrap().keys().collect();
        assert_eq!(rules, ["Doc", "Greet", "__start__"]);
        let gbnf = spec(
            "root ::= greet\ngreet ::= \"hi\" | \"hello\"\n",
            Format::Gbnf,
        );
        assert_eq!(gbnf["options"]["tokenSet"]["IGNORE"], serde_json::json!([]));
        assert_eq!(gbnf["rule"]["greet"]["open"][0]["g"], "gbnf");
    }

    #[test]
    fn a_grammars_refusals_are_load_errors() {
        for (format, src) in [
            (
                Format::Abnf,
                format!("a = {}\"x\"{}\n", "(".repeat(20_000), ")".repeat(20_000)),
            ),
            (
                Format::Ebnf,
                format!("a ::= {}\"x\"{}\n", "(".repeat(20_000), ")".repeat(20_000)),
            ),
            (
                Format::Gbnf,
                format!(
                    "root ::= {}\"x\"{}\n",
                    "(".repeat(20_000),
                    ")".repeat(20_000)
                ),
            ),
        ] {
            let e = parse(&src, format).unwrap_err();
            assert_eq!(e.code, "too_deep", "{format}: {e}");
            assert_eq!((e.line, e.col), (0, 0), "{format}");
            assert!(e.message.contains("nests too deeply"), "{format}: {e}");
            assert!(
                e.plain_report()
                    .starts_with(&format!("[{format}/too_deep]: ")),
                "{}",
                e.plain_report()
            );
        }
        let e = parse(
            &format!("a = {}\"x\"{}\n", "(".repeat(200), ")".repeat(200)),
            Format::Abnf,
        )
        .unwrap_err();
        assert_eq!(e.code, "too_deep");
        assert!(e.message.contains("more than 128 deep"), "{e}");
        let e = parse("greet = \"hi\"\na = \"\u{1}\"\n", Format::Abnf).unwrap_err();
        assert_eq!(
            (e.code.as_str(), e.line, e.col),
            ("unprintable", 2, 6),
            "{e}"
        );
        assert!(
            e.plain_report().starts_with("[abnf/unprintable]: "),
            "{}",
            e.plain_report()
        );
        let e = parse("a = b\n", Format::Abnf).unwrap_err();
        assert_eq!(e.code, "compile");
        assert_eq!(e.message, "compile: rule 'a' references unknown rule 'b'");
        assert_eq!((e.line, e.col), (1, 5));
        assert!(
            e.plain_report().contains("  1 | a = b"),
            "{}",
            e.plain_report()
        );
        let e = parse("a ::= \"x\"\n", Format::Gbnf).unwrap_err();
        assert_eq!(e.code, "compile");
        assert!(e.message.contains("no 'root' rule"), "{e}");
        let e = parse("\n", Format::Ebnf).unwrap_err();
        assert_eq!(e.message, "compile: no productions found");
        assert_eq!((e.line, e.col), (0, 0));
        assert!(!e.is_io() && !e.is_too_large() && !e.is_timeout());
    }

    #[test]
    fn a_grammars_compile_is_waited_for_within_the_deadline() {
        let rules: String = (0..2_000)
            .map(|i| format!("r{i} = \"x{i}\" r{}\n", i + 1))
            .chain(std::iter::once("r2000 = \"y\"\n".to_string()))
            .collect();
        let started = Instant::now();
        let e = parse_within(&rules, Format::Abnf, Some(Duration::from_millis(50))).unwrap_err();
        assert!(e.is_timeout(), "{e}");
        assert_eq!(
            e.message,
            "timeout: the grammar took longer than 0.05 s to compile"
        );
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "it stopped waiting"
        );
        let small = "r0 = \"x\"\n";
        let ample = Some(Duration::from_secs(600));
        let e = parse_within(small, Format::Abnf, ample).unwrap_err();
        assert!(e.is_timeout(), "{e}");
        assert_eq!(
            e.message,
            "timeout: the grammar was not compiled within 600 s: a compile aless stopped waiting \
             for is still running"
        );
        assert_eq!(ABANDONED_COMPILES.load(Ordering::SeqCst), 1);
        // With no deadline nothing is left behind, so nothing waits.
        assert!(parse(small, Format::Abnf).is_ok());
        let until = Instant::now() + Duration::from_secs(600);
        while ABANDONED_COMPILES.load(Ordering::SeqCst) > 0 {
            assert!(Instant::now() < until, "the compile left behind ends");
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(parse_within(small, Format::Abnf, ample).is_ok());
        let past = Deadline::after(Some(Duration::ZERO));
        let e = compile_notation_by(Format::Abnf, &rules, past).unwrap_err();
        assert!(e.is_timeout(), "{e}");
        assert_eq!(
            e.message,
            "timeout: the grammar was not compiled within 0 s: the time was up before its \
             compile started"
        );
        assert_eq!(
            ABANDONED_COMPILES.load(Ordering::SeqCst),
            0,
            "no compile was started to be left behind"
        );
        assert!(parse_within(small, Format::Abnf, ample).is_ok());
    }

    /// The value as aless reads it, as compact JSON.
    fn read_json(src: &str, format: Format) -> String {
        let doc = parse(src, format).unwrap_or_else(|e| panic!("{format} {src:?}: {e}"));
        crate::fmt::to_json_compact(&doc, 0)
    }

    /// A .proto file reads as its FileDescriptorProto in protoc's JSON form and the crate's
    /// canonical member order; a number read as NaN (`1_0`) stays NaN, written as null.
    #[test]
    fn proto_reads_as_its_descriptor() {
        assert_eq!(
            read_json(
                "syntax = \"proto3\";\npackage p;\nmessage M { int32 a = 1; string b = 1_0; \
                 optional int32 c = 3; }",
                Format::Proto
            ),
            r#"{"dependency":[],"publicDependency":[],"weakDependency":[],"messageType":[{"name":"M","field":[{"name":"a","number":1,"label":"LABEL_OPTIONAL","type":"TYPE_INT32"},{"name":"b","number":null,"label":"LABEL_OPTIONAL","type":"TYPE_STRING"},{"name":"c","number":3,"label":"LABEL_OPTIONAL","proto3Optional":true,"type":"TYPE_INT32","oneofIndex":0}],"nestedType":[],"enumType":[],"oneofDecl":[{"name":"_c"}],"extension":[]}],"enumType":[],"service":[],"extension":[],"syntax":"proto3","package":"p"}"#
        );
        let doc = parse("message M { int32 b = 1_0; }", Format::Proto).unwrap();
        let number = doc
            .resolve(&[
                Key::Name("messageType".into()),
                Key::Index(0),
                Key::Name("field".into()),
                Key::Index(0),
                Key::Name("number".into()),
            ])
            .unwrap();
        assert!(matches!(doc.node(number).kind, Kind::Number(n) if n.is_nan()));
        // No declaration reads as proto2, as protoc has it.
        assert!(read_json("message M {}", Format::Proto).ends_with(r#""syntax":"proto2"}"#));
    }

    #[test]
    fn proto_refuses_what_its_crate_refuses() {
        let cap = tabnas_proto::MAX_NESTING_DEPTH;
        let nested = |n: usize| format!("{}{}", "message M {".repeat(n), "}".repeat(n));
        assert!(parse(&nested(cap), Format::Proto).is_ok());
        let e = parse(&nested(cap + 1), Format::Proto).unwrap_err();
        assert_eq!(e.code, "too_deep");
        assert_eq!(
            e.message,
            "too_deep: nested deeper than the proto grammar reads"
        );
        assert!(e.hint.contains("more than 100 levels deep"), "{}", e.hint);
        assert_eq!((e.line, e.col), (0, 0));
        let e = parse("syntax = \"proto5\";\nmessage M {}\n", Format::Proto).unwrap_err();
        assert_eq!(e.code, "version");
        assert_eq!(e.message, "version: unknown syntax version \"proto5\"");
        assert!(
            e.plain_report()
                .starts_with("[proto/version]: version: unknown syntax version \"proto5\""),
            "{}",
            e.plain_report()
        );
        assert!(!e.is_io() && !e.is_too_large() && !e.is_timeout());
    }

    /// An expression reads as its S-expressions, an operation as the list of its operator's
    /// text and its terms; past the grammar's limits it is too deep.
    #[test]
    fn expr_reads_as_its_s_expressions() {
        assert_eq!(read_json("1+2*3", Format::Expr), r#"["+",1,["*",2,3]]"#);
        assert_eq!(
            read_json("a: (1+2)*3, b: [-4, x]", Format::Expr),
            r#"{"a":["*",["(",["+",1,2]],3],"b":[["-",4],"x"]}"#
        );
        let sum = |ops: usize| vec!["1"; ops + 1].join("+");
        let limit = tabnas_expr::NODE_LIMIT;
        assert!(parse(&sum(limit), Format::Expr).is_ok());
        let e = parse(&sum(limit + 1), Format::Expr).unwrap_err();
        assert_eq!(e.code, "too_deep");
        assert_eq!(
            e.message,
            "too_deep: an expression larger or deeper than the expr grammar reads"
        );
        assert!(e.hint.contains("127 operations"), "{}", e.hint);
    }

    #[test]
    fn a_version_reads_without_its_line_break() {
        let version = r#"{"major":1,"minor":2,"patch":3,"prerelease":["rc",1],"build":[]}"#;
        for src in [
            "1.2.3-rc.1",
            "1.2.3-rc.1\n",
            "1.2.3-rc.1\r\n",
            "\u{feff}1.2.3-rc.1\n",
        ] {
            assert_eq!(read_json(src, Format::Semver), version, "{src:?}");
        }
        for src in ["1.2.3\n\n", " 1.2.3", "v1.2.3\n", "1.2\n"] {
            assert!(parse(src, Format::Semver).is_err(), "{src:?}");
        }
    }

    /// CSS and PGN read as their grammars build them; PGN's reports carry colour codes too.
    #[test]
    fn css_and_pgn_read_as_their_grammars_build_them() {
        assert_eq!(
            read_json("a { color: red }", Format::Css),
            r#"{"type":"stylesheet","rules":[{"type":"rule","selectors":["a"],"declarations":[{"type":"declaration","property":"color","value":"red"}]}]}"#
        );
        let games = read_json("[White \"a\"]\n\n1. e4 1-0\n\n1. d4 *\n", Format::Pgn);
        assert!(
            games.starts_with(r#"[{"tags":{"White":"a"},"moves":[{"san":"e4""#),
            "{games}"
        );
        assert!(games.ends_with(r#""result":"*"}]"#), "{games}");
        assert!(
            make_parser(Format::Pgn)
                .unwrap()
                .unwrap()
                .options
                .color
                .active
        );
        let e = parse("1. e4 Zz9", Format::Pgn).unwrap_err();
        assert_eq!(e.code, "unexpected");
        assert!(e.message.contains("not chess notation"), "{}", e.message);
    }

    /// A C file reads as tabnas-c's realized tree, with the subtrees repeated under `operand`.
    #[test]
    fn c_reads_as_its_syntax_tree() {
        let tree = read_json("int x = 1;", Format::C);
        assert!(
            tree.starts_with(r#"{"kind":"translation_unit","span":"#),
            "{tree}"
        );
        assert!(
            tree.contains(r#"{"kind":"token","tname":"KW_INT","src":"int","span":{"start":0,"end":3,"line":1,"col":1}}"#),
            "{tree}"
        );
        // A unary operation's operand is its child too.
        let doc = parse("int f(void) { return -x; }", Format::C).unwrap();
        let operand = doc
            .nodes
            .iter()
            .position(|n| n.key.name() == Some("operand"))
            .expect("an operand");
        assert!(doc.nodes[operand].is_container());
    }

    /// C's limits are its reader's, each `too_deep`, placed, or unplaced when stopped between
    /// tokens; a tree too large written out is refused at once.
    #[test]
    fn c_is_bounded_by_its_readers_limits_and_the_tree_written_out() {
        let terms = |n: usize| {
            (0..n)
                .map(|i| format!("a{i}"))
                .collect::<Vec<_>>()
                .join(" + ")
        };
        assert!(parse(&format!("int x = {};", terms(100)), Format::C).is_ok());
        let e = parse(&format!("int x = {};", terms(200)), Format::C).unwrap_err();
        assert_eq!(e.code, "too_deep");
        assert_eq!(
            e.message,
            "too_deep: an expression larger, or code nested deeper, than the c reader reads"
        );
        assert!(e.hint.contains("127 operations"), "{}", e.hint);
        assert_eq!(e.line, 1, "{e}");
        assert!(e.col > 1, "{e}");
        let blocks = |n: usize| format!("void f(void) {{ {}{} }}", "{".repeat(n), "}".repeat(n));
        assert!(parse(&blocks(50), Format::C).is_ok());
        for src in [blocks(300), "int a = b ? c : d;".to_string()] {
            let e = parse(&src, Format::C).unwrap_err();
            assert_eq!(e.code, "too_deep", "{src}");
            assert_eq!((e.line, e.col), (0, 0), "no place: {e}");
            assert!(e.hint.contains("256 levels"), "{}", e.hint);
        }
        let e = parse(&blocks(2000), Format::C).unwrap_err();
        assert_eq!(e.code, "too_deep");
        assert!(e.line == 1 && e.col > 300, "{e}");
        // Each member access doubles the tree written out; a long chain is refused before that.
        let members = |n: usize| format!("int f(void) {{ return a{}; }}", ".b".repeat(n));
        assert!(parse(&members(12), Format::C).is_ok());
        let started = Instant::now();
        let e = parse(&members(40), Format::C).unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(20));
        assert_eq!(e.code, "too_deep");
        assert_eq!(
            e.message,
            "too_deep: a c syntax tree larger written out than aless reads"
        );
        assert!(e.hint.contains("256 values a byte"), "{}", e.hint);
        assert_eq!((e.line, e.col), (0, 0));
    }

    #[test]
    fn written_out_counts_a_shared_subtree_in_every_place() {
        use tabnas::Value;
        let values = |values| WrittenOut {
            values,
            bytes: u64::MAX,
        };
        let leaf = Value::array(vec![Value::Number(1.0), Value::Number(2.0)]);
        let mut level = leaf;
        for _ in 0..100 {
            level = Value::array(vec![level.clone(), level]);
        }
        // A doubling chain: the count stops at the first level past the cap.
        assert_eq!(written_out(&level, values(1_000)).values, 1_023);
        let all = written_out(&level, values(u64::MAX));
        assert_eq!(all.values, u64::MAX, "saturated");
        let small = Value::array(vec![Value::array(vec![]), Value::Null]);
        assert_eq!(
            written_out(&small, values(1_000)),
            WrittenOut {
                values: 3,
                bytes: 0
            }
        );
        let shared = Value::array(vec![Value::Number(1.0)]);
        let twice = Value::array(vec![shared.clone(), shared]);
        assert_eq!(written_out(&twice, values(1_000)).values, 5);
        // Text is counted in every place too, a shared container's own name among it.
        let inner = Value::from_json(&serde_json::json!({"name": "abc"}));
        let mut outer = Value::from_json(&serde_json::json!({"key": null}));
        outer.as_object_mut().unwrap().insert(
            "key".into(),
            Value::array(vec![inner.clone(), inner.clone(), inner]),
        );
        let ample = values(u64::MAX);
        assert_eq!(
            written_out(&outer, ample),
            WrittenOut {
                values: 8,
                bytes: 24
            }
        );
        // It stops at the first container past either cap: here the list.
        let text = WrittenOut {
            values: u64::MAX,
            bytes: 20,
        };
        let stopped = written_out(&outer, text);
        assert!(stopped.past(text));
        assert_eq!(
            stopped,
            WrittenOut {
                values: 7,
                bytes: 21
            }
        );
        let cap = |values, bytes| WrittenOut { values, bytes };
        assert_eq!(c_tree_cap(10), cap(C_TREE_FLOOR, C_TREE_BYTES_FLOOR));
        assert_eq!(c_tree_cap(1 << 16), cap(256 << 16, 2048 << 16));
        // Past some length the ceilings hold, however long the file.
        let ceilings = cap(C_TREE_CEILING, C_TREE_BYTES_CEILING);
        assert_eq!(c_tree_cap(1 << 17), ceilings);
        assert_eq!(c_tree_cap(64 << 20), ceilings);
        assert_eq!(c_tree_cap(usize::MAX), ceilings);
    }

    /// A megabyte-long name under a member chain is within the value ceiling but far past the
    /// text ceiling, and is refused; under a short chain it reads.
    #[test]
    fn a_long_name_in_a_repeated_subtree_is_held_to_its_text() {
        let name = "a".repeat(1 << 20);
        let chain = |n: usize| format!("int f(void) {{ return {name}{}; }}\n", ".b".repeat(n));
        let src = chain(18);
        let parser = make_parser(Format::C).unwrap().unwrap();
        let value = parse_value(&parser, Format::C, &src).unwrap();
        let all = written_out(
            &value,
            WrittenOut {
                values: u64::MAX,
                bytes: u64::MAX,
            },
        );
        drop(value);
        let cap = c_tree_cap(src.len());
        assert_eq!(cap.values, C_TREE_CEILING);
        assert!(all.values < cap.values, "{all:?}");
        assert!(all.bytes > 1 << 38, "{all:?}");
        let e = parse(&src, Format::C).unwrap_err();
        assert_eq!(e.code, "too_deep", "{e}");
        assert!(
            e.hint.contains(
                "2,048 bytes of names and strings a byte, of 8 MB at least and of 256 MB at most"
            ),
            "{}",
            e.hint
        );
        for n in [0, 4] {
            assert!(parse(&chain(n), Format::C).is_ok(), "{n} accesses");
        }
    }

    /// The ceiling refuses a member chain whatever a long comment adds to the file's length.
    #[test]
    fn padding_does_not_raise_a_c_trees_cap() {
        let chain = format!("int f(void) {{ return a{}; }}\n", ".b".repeat(21));
        let padded = format!("/*{}*/\n{chain}", " ".repeat(4 << 20));
        for src in [chain, padded] {
            let e = parse(&src, Format::C).unwrap_err();
            assert_eq!(e.code, "too_deep", "{} bytes: {e}", src.len());
        }
        // Real code past the length at which the ceilings hold still reads.
        let functions: String = (0..4_000)
            .map(|i| format!("int f{i}(int a) {{ return a.b.c + {i}; }}\n"))
            .collect();
        assert!(parse(&functions, Format::C).is_ok());
    }

    #[test]
    fn positions_reach_the_tree() {
        let doc = parse("[s]\nk = v\n", Format::Ini).unwrap();
        let k = doc
            .resolve(&[Key::Name("s".into()), Key::Name("k".into())])
            .unwrap();
        assert_eq!(doc.node(k).line, 2);
        let doc = parse("a,b\n1,2\n3,4\n", Format::Csv).unwrap();
        let second = doc.resolve(&[Key::Index(1)]).unwrap();
        assert_eq!(doc.node(second).line, 3);
        // The first record is below the header, not on it.
        let first = doc.resolve(&[Key::Index(0)]).unwrap();
        assert_eq!(doc.node(first).line, 2);
        let a = doc
            .resolve(&[Key::Index(0), Key::Name("a".into())])
            .unwrap();
        assert_eq!((doc.node(a).line, doc.node(a).col), (2, 1));
    }
}
