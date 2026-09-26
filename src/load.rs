//! Format detection and loading: a file (or a string) goes in, a
//! positioned [`Doc`] comes out, parsed by the tabnas grammar for its
//! format, or split into lines when no grammar claims it.

use std::cell::Cell;
use std::collections::HashMap;
use std::fmt;
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tabnas::Tabnas;

use crate::doc::Doc;
use crate::prov;

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
    Text,
}

impl Format {
    pub const ALL: [Format; 15] = [
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
            Format::Text => "text",
        }
    }

    /// A format by name or by a common alias (`yml`, `md`, `ndjson`, `txt`).
    pub fn from_name(s: &str) -> Option<Format> {
        let s = s.trim().to_ascii_lowercase();
        Format::ALL
            .iter()
            .copied()
            .find(|f| f.name() == s)
            .or_else(|| Format::from_extension(&s))
    }

    /// The format a file extension (without the dot) implies.
    pub fn from_extension(ext: &str) -> Option<Format> {
        Some(match ext.to_ascii_lowercase().as_str() {
            "json" | "geojson" | "har" | "jsonld" | "webmanifest" => Format::Json,
            "jsonl" | "ndjson" => Format::Jsonl,
            "jsonic" => Format::Jsonic,
            "jsonc" => Format::Jsonc,
            "json5" => Format::Json5,
            "yaml" | "yml" => Format::Yaml,
            "toml" => Format::Toml,
            "ini" | "cfg" | "conf" | "cnf" => Format::Ini,
            "csv" => Format::Csv,
            "tsv" | "tab" => Format::Tsv,
            "xml" | "svg" | "xhtml" | "xsd" | "xsl" | "xslt" | "plist" => Format::Xml,
            "zon" => Format::Zon,
            "md" | "markdown" => Format::Markdown,
            "rss" | "atom" => Format::Feed,
            "txt" | "text" | "log" => Format::Text,
            _ => return None,
        })
    }

    /// The format of a path, from its extension; plain text when unknown.
    pub fn detect(path: &Path) -> Format {
        path.extension()
            .and_then(|e| e.to_str())
            .and_then(Format::from_extension)
            .unwrap_or(Format::Text)
    }
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Why a load failed: an I/O error (no position) or a parse error with the
/// 1-based line and column the grammar reported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadError {
    /// One line, for the status bar.
    pub message: String,
    /// What went wrong, in a word: the grammar's error code
    /// (`unexpected`, `unterminated_string`, …), or `io` or `grammar` for
    /// an error aless raised itself.
    pub code: String,
    pub line: u32,
    pub col: u32,
    /// The grammar's advice, as the report shows it; empty when it gave
    /// none. (This and `source_line` are boxed to keep the error small.)
    pub hint: Box<str>,
    /// The source line the error is on, cut to a window around the column
    /// when it is long; `None` when the error has no position.
    pub source_line: Option<Box<str>>,
    /// The full report, as the tabnas engine renders it: the `[tag/code]`
    /// header, the `-->` location, the source lines around the error with
    /// a caret under it, the grammar's hint and link, and the engine's
    /// diagnostics line. Carries the engine's ANSI colour codes, which the
    /// renderer turns into styles.
    pub report: String,
}

/// The largest input aless reads unless `--max-size` says otherwise. A
/// parse takes about [`MEMORY_PER_BYTE`] bytes of memory per byte of
/// input, so this is some 5 GB, and a minute or so of parsing.
pub const DEFAULT_MAX_SIZE: u64 = 64 << 20;

/// Roughly how many bytes of memory a parse takes per byte of input,
/// measured on JSON: 13 MB peaked at 1.0 GB, 66 MB at 5.1 GB.
pub const MEMORY_PER_BYTE: u64 = 80;

/// What a load may cost: how much input it reads, and how long it parses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Refuse an input larger than this many bytes (`--max-size`).
    pub max_size: Option<u64>,
    /// Stop a parse that runs longer than this (`--timeout`).
    pub timeout: Option<Duration>,
}

impl Limits {
    /// [`DEFAULT_MAX_SIZE`], and no time limit: how long a parse may take
    /// depends on the machine, so only the caller can say.
    pub const DEFAULT: Limits = Limits {
        max_size: Some(DEFAULT_MAX_SIZE),
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

/// Set the limits that [`load_path`], [`load_str`] and [`parse`] apply
/// from now on in this process.
pub fn set_limits(limits: Limits) {
    MAX_SIZE.store(limits.max_size.unwrap_or(0), Ordering::Relaxed);
    TIMEOUT_MS.store(timeout_ms(limits.timeout), Ordering::Relaxed);
}

/// A timeout as [`TIMEOUT_MS`] holds it: milliseconds, and at least one,
/// since 0 stands for no limit.
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

/// A size as `--max-size` takes it: bytes, or with a `K`, `M` or `G`
/// suffix counted in 1024s (`KB`, `MiB` and the like are the same), and 0
/// for no limit.
pub fn parse_size(text: &str) -> Result<Option<u64>, String> {
    let t = text.trim();
    let digits = t.find(|c: char| !c.is_ascii_digit()).unwrap_or(t.len());
    let bad = || format!("--max-size needs a size such as 64M, 1G or 0, not {text:?}");
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
        if bytes >= 1 << shift && bytes % (1 << shift) == 0 {
            return format!("{}{unit}", bytes >> shift);
        }
    }
    bytes.to_string()
}

/// Read all of `input`, but hold no more than `max` bytes of it: an input
/// longer than that is refused as too large, and the rest is not read.
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

/// How deep a parse's rule stack may grow before aless stops it: about
/// three rules a level, so some 1,000 levels of nesting, far past any real
/// document. Most grammars stop sooner, each as `too_deep` too: JSON,
/// JSONL, JSONic, JSON5, YAML, TOML, INI and ZON at 127 levels, XML at
/// 256 and JSONC at 512. This cap is for a grammar with no limit of its
/// own: deeper, some recurse until the stack runs out, which ends the
/// process, and slow down with the square of the depth long before that.
pub const MAX_RULE_DEPTH: usize = 3_000;

/// The stack a parse runs on: room for [`MAX_RULE_DEPTH`] with a wide
/// margin, whatever the calling thread has (a Windows main thread has
/// 1 MB).
const PARSE_STACK: usize = 64 << 20;

/// Why aless stopped a parse itself: nesting past [`MAX_RULE_DEPTH`], or
/// time past the timeout.
const STOP_DEPTH: u8 = 1;
const STOP_TIME: u8 = 2;

/// When a parse must be done by.
#[derive(Clone)]
struct Deadline {
    at: Instant,
    /// The time the parse was given, for its report.
    limit: Duration,
    /// Raised by the thread waiting on the parse once `at` passes, so that
    /// the parse has only a flag to look at between its steps, not the
    /// clock. `None` when no thread waits, and the parse reads the clock.
    alarm: Option<Arc<AtomicBool>>,
}

impl Deadline {
    /// Whether the time is up, as the parse finds it between two steps.
    fn passed(&self) -> bool {
        match &self.alarm {
            Some(alarm) => alarm.load(Ordering::Relaxed),
            None => Instant::now() >= self.at,
        }
    }
}

/// The placeholder the engine writes where a file name belongs.
const NO_FILE: &str = "<no-file>";

/// The engine's name for the token at the end of the source.
const END_TOKEN: &str = "#ZZ";

const END_HINT: &str = "The document ends before it is complete: look for an unclosed\n\
                        bracket, brace or string, or a missing value at the end.";

impl LoadError {
    /// An error the viewer raised itself (reading the file, a grammar that
    /// panicked), reported in the engine's layout under `[aless/<tag>]`.
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

    /// Whether the input could not be read at all, as opposed to read and
    /// found not to parse.
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

    /// An input over the size limit: `size` is its size when known (a
    /// file), `None` for a stream that ran past the limit.
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
            Some(n) => {
                // Suggest the first doubling of the limit that holds it.
                let mut room = limit.saturating_mul(2);
                while room < n {
                    room = room.saturating_mul(2);
                }
                format!(
                    "{per_byte}, so this one would need about {}. Pass --max-size {} \
                     (or more) to read it, or --max-size 0 for no limit.",
                    human_size(n.saturating_mul(MEMORY_PER_BYTE)),
                    size_flag(room)
                )
            }
            None => format!(
                "{per_byte}. Pass a larger --max-size, such as {}, to read it, or \
                 --max-size 0 for no limit.",
                size_flag(limit.saturating_mul(4))
            ),
        };
        LoadError::tagged("too_large", message).with_hint(&hint)
    }

    /// A parse that finished, but after its time limit. It stopped nowhere
    /// in particular, so the report says how long it took instead.
    fn finished_late(limit: Duration, took: Duration) -> LoadError {
        // To the millisecond, and never 0.
        let took = Duration::from_millis(timeout_ms(Some(took)));
        let message = format!("timeout: the parse ran longer than {} s", seconds(limit));
        let hint = format!(
            "The parse finished, but it took {} s.\nPass a larger --timeout to have its \
             result, or --timeout 0 for no limit.",
            seconds(took)
        );
        LoadError::tagged("timeout", message).with_hint(&hint)
    }

    /// Add advice to an error aless raised itself, shown under the report
    /// as the engine shows a grammar's.
    fn with_hint(mut self, hint: &str) -> LoadError {
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

    /// Name the file the report is about, in place of the engine's
    /// `<no-file>`. A path may hold control characters (a newline is legal
    /// in a Unix file name), so they are shown escaped: the report's colour
    /// codes are obeyed when it is drawn, and a name must not add lines or
    /// styles of its own.
    pub fn with_origin(mut self, origin: &str) -> LoadError {
        self.report = replace_after_header(&self.report, NO_FILE, &escape_controls(origin));
        self
    }

    /// From an engine error. Everything in the report that came from the
    /// file is made safe to draw, since the report's own colour codes are
    /// obeyed:
    ///
    /// - control characters in the message and hint are shown escaped
    ///   (`\n`); the offending token of an unterminated string runs into a
    ///   newline, and would otherwise break the report's lines;
    /// - in the quoted source lines they are shown as their one-column
    ///   Unicode pictures (`␛`), so the caret, which the engine places by
    ///   character count, still lines up;
    /// - a quoted line too long for a screen (a minified file) is cut to a
    ///   window around the error column, and the caret stops at the end of
    ///   its line, so the caret and the message after it stay in view.
    ///
    /// `hint_template` is the grammar's template for the error's hint (see
    /// [`hint_template`]); it tells the hint's own line breaks from those
    /// the token brought in.
    pub fn from_tabnas(e: &tabnas::TabnasError, hint_template: Option<&str>) -> LoadError {
        let mut shown = e.clone();
        shown.detail = escape_controls(&e.detail);
        shown.hint = escape_hint(&e.hint, hint_template, &e.src);
        if e.code == "unexpected" && e.token.name == END_TOKEN && e.src.is_empty() {
            // The engine words this as an unexpected character, and quotes
            // none: the document stopped before it was complete.
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

    /// The report without its colour codes.
    pub fn plain_report(&self) -> String {
        strip_ansi(&self.report)
    }
}

/// Break text into lines of at most `width` characters, at spaces, keeping
/// its own line breaks.
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

/// The template a grammar's hint for `code` was rendered from, looked up
/// the way the engine does: the code's own, else the `unknown` one.
pub fn hint_template<'a>(hints: &'a HashMap<String, String>, code: &str) -> Option<&'a str> {
    hints
        .get(code)
        .filter(|t| !t.is_empty())
        .or_else(|| hints.get("unknown").filter(|t| !t.is_empty()))
        .map(String::as_str)
}

/// A hint with the control characters its placeholders brought in shown
/// escaped, and the template's own line breaks kept.
///
/// The template says which text is its own: the hint is matched against
/// it, each `{name}` standing for any text, and only what the placeholders
/// matched is escaped. `{src}` alone cannot be searched for, since the
/// token may be a single newline and the template has newlines of its own.
/// Without a template, or when the hint does not match it (a grammar that
/// set its own), the first occurrence of the token is taken for it.
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
    // The engine fills `{key}` for any key up to the next `}`, and trims
    // the result.
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

/// A quoted source line longer than this many characters is cut to a
/// window around the error column.
const LONG_LINE: usize = 160;
/// The window's width in characters...
const WINDOW: usize = 120;
/// ...and how many of them come before the error column.
const WINDOW_LEAD: usize = 40;

/// A source line as an error quotes it: whole, or when longer than
/// [`LONG_LINE`] a [`WINDOW`] of it around the 1-based `col`, marked `…`
/// where cut.
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

/// Prepare the source the engine quotes (up to two lines either side of
/// the error) for drawing; see [`LoadError::from_tabnas`]. Only those lines
/// are kept, the ones above them left empty so the line numbers hold. When
/// any of them is longer than [`LONG_LINE`], all of them are cut to the same
/// window, marked `…` where cut, and the column moves with the window. The
/// caret runs from the error column to the end of its line at most: the
/// token of an unterminated string is the rest of the file.
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

/// A character of a quoted source line as it is drawn: a control
/// character becomes its picture from the U+2400 block (or U+FFFD, for the
/// C1 controls, which have none), one column wide like the character it
/// stands for. A tab stays; the renderer draws it as a space.
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

fn make_parser(format: Format) -> Option<Tabnas> {
    Some(match format {
        Format::Json => tabnas_json::make(),
        Format::Jsonl => tabnas_jsonl::make(),
        Format::Jsonic => tabnas_jsonic::make(),
        Format::Jsonc => tabnas_jsonc::make(),
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
        Format::Text => return None,
    })
}

/// Split text into lines: `\n` or `\r\n` terminated, the terminator of the
/// last line optional.
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

thread_local! {
    /// How many guarded parses this thread is inside.
    static CATCHING: Cell<usize> = const { Cell::new(0) };
}

/// Is a grammar running under [`parse`]'s panic guard on this thread right
/// now? A panic hook (the terminal's, which restores the screen) runs on
/// the panicking thread and should stand down then: the panic is caught
/// and reported as a [`LoadError`]. A panic on any other thread, such as
/// the one reading input, is not caught, so the answer is per thread.
pub fn parse_in_progress() -> bool {
    CATCHING.try_with(|c| c.get() > 0).unwrap_or(false)
}

struct CatchGuard;

impl CatchGuard {
    fn enter() -> CatchGuard {
        CATCHING.with(|c| c.set(c.get() + 1));
        CatchGuard
    }
}

impl Drop for CatchGuard {
    fn drop(&mut self) {
        let _ = CATCHING.try_with(|c| c.set(c.get() - 1));
    }
}

/// Parse `src` as `format`, within the time limit of [`Limits::current`].
pub fn parse(src: &str, format: Format) -> Result<Doc, LoadError> {
    parse_within(src, format, Limits::current().timeout)
}

/// Parse `src` as `format`, stopping after `timeout`.
///
/// The parse runs on a thread of its own with a large stack
/// ([`PARSE_STACK`]), and stops at [`MAX_RULE_DEPTH`]: nesting that deep
/// fails as `too_deep` rather than overflowing the stack, which would end
/// the process where no error can be caught, as does nesting past a
/// grammar's own, lower limit. Past `timeout` it fails as
/// `timeout`: stopped at the place it had reached, or, when its last step
/// ran past the time, as soon as it finishes.
pub fn parse_within(
    src: &str,
    format: Format,
    timeout: Option<Duration>,
) -> Result<Doc, LoadError> {
    let src = src.strip_prefix('\u{feff}').unwrap_or(src);
    if format == Format::Text {
        return Ok(Doc::from_lines(&lines(src)));
    }
    // A time too far off to reach is no limit.
    let deadline = timeout.and_then(|limit| {
        let at = Instant::now().checked_add(limit)?;
        Some(Deadline {
            at,
            limit,
            alarm: None,
        })
    });
    let alarm = Arc::new(AtomicBool::new(false));
    let watched = deadline.clone().map(|d| Deadline {
        alarm: Some(alarm.clone()),
        ..d
    });
    let (done, finished) = mpsc::channel::<()>();
    std::thread::scope(|scope| {
        let spawned = std::thread::Builder::new()
            .name("aless-parse".into())
            .stack_size(PARSE_STACK)
            .spawn_scoped(scope, move || {
                // Hung up when the parse ends, however it ends.
                let _done = done;
                parse_here(src, format, watched)
            });
        match spawned {
            Ok(handle) => {
                // This thread has only to wait, so it keeps the time.
                if let Some(d) = &deadline {
                    let left = d.at.saturating_duration_since(Instant::now());
                    if finished.recv_timeout(left) == Err(RecvTimeoutError::Timeout) {
                        alarm.store(true, Ordering::Relaxed);
                    }
                }
                // A panic outside the grammar is aless's own: let it through.
                handle
                    .join()
                    .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
            }
            // No thread to be had: parse on this one, within the same caps,
            // reading the clock itself.
            Err(_) => parse_here(src, format, deadline),
        }
    })
}

fn parse_here(src: &str, format: Format, deadline: Option<Deadline>) -> Result<Doc, LoadError> {
    let Some(parser) = make_parser(format) else {
        return Ok(Doc::from_lines(&lines(src)));
    };
    parse_with(parser, src, format, deadline)
}

/// Parse `src` as `format` with `parser`, within aless's caps: the work of
/// [`parse_here`] once it has a parser.
fn parse_with(
    mut parser: Tabnas,
    src: &str,
    format: Format,
    deadline: Option<Deadline>,
) -> Result<Doc, LoadError> {
    let stopped = guard(&mut parser, deadline.clone());
    let sink = prov::capture(&mut parser);
    // A grammar is a plugin; a defect in one must not take the viewer down.
    let outcome = {
        let _guard = CatchGuard::enter();
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            parser.parse(src).map_err(Box::new)
        }))
    };
    let stop = stopped.load(Ordering::Relaxed);
    let now = Instant::now();
    // The time is looked at between steps, and a step is not cut short: one
    // that reads a long string can run past the deadline and be the parse's
    // last. A parse that comes to its end late, with a value or an error,
    // is still too late.
    let late = deadline
        .as_ref()
        .filter(|d| stop == 0 && outcome.is_ok() && now >= d.at);
    if let Some(d) = late {
        let took = d.limit + now.saturating_duration_since(d.at);
        return Err(LoadError::finished_late(d.limit, took));
    }
    match outcome {
        Ok(Ok(value)) => {
            let mut doc = Doc::from_value(&value);
            // The engine's tree is not needed past this point; letting it
            // go before the alignment lowers the peak on a large document.
            drop(value);
            drop(parser);
            if let Ok(toks) = sink.lock() {
                prov::align(&mut doc, &toks);
            }
            Ok(doc)
        }
        Ok(Err(e)) if stop == STOP_DEPTH => {
            let mut e = *e;
            e.tag = "aless".to_string();
            e.code = "too_deep".to_string();
            e.detail = format!(
                "too_deep: nested deeper than aless reads (about {} levels)",
                MAX_RULE_DEPTH / 3
            );
            e.hint = "Nesting this deep would overflow the parser's stack, so the parse \
                      stopped here.\nReal documents nest a few dozen levels at most."
                .to_string();
            Err(LoadError::from_tabnas(&e, None))
        }
        Ok(Err(e)) if stop == STOP_TIME => {
            let mut e = *e;
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
        // A grammar's own depth limit, lower than aless's: JSON's (and
        // YAML's, INI's and the rest of jsonic's family) is a parse guard
        // and XML's is in its lexer, and each stops the parse with the
        // engine's `cancel`, which no grammar here raises for anything
        // else. Its hint would blame a budget of the caller's.
        Ok(Err(e)) if e.code == "cancel" => {
            let mut e = *e;
            e.tag = "aless".to_string();
            e.code = "too_deep".to_string();
            e.detail = format!("too_deep: nested deeper than the {format} grammar reads");
            e.hint = format!(
                "The {format} grammar has a nesting limit of its own, and the parse stopped \
                 where the document passed it.\nReal documents nest a few dozen levels at most."
            );
            Err(LoadError::from_tabnas(&e, None))
        }
        Ok(Err(e)) => {
            let hints = parser.config().hint;
            Err(LoadError::from_tabnas(&e, hint_template(&hints, &e.code)))
        }
        Err(panic) => {
            let what = panic
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "unknown panic".to_string());
            Err(LoadError::tagged(
                "grammar",
                format!("{format} grammar failed: {what}"),
            ))
        }
    }
}

/// Stop the parse when its rule stack passes [`MAX_RULE_DEPTH`], or when
/// its deadline passes. Both are looked at between every two steps of the
/// engine, in the parse budget. A grammar's own depth limit is a parse
/// guard, which the engine runs beside the budget rather than in it, so
/// setting this one leaves it in place; a budget the grammar keeps itself
/// still runs, at its own interval. Returns why aless stopped the parse,
/// if it did: [`STOP_DEPTH`], [`STOP_TIME`], else 0.
fn guard(parser: &mut Tabnas, deadline: Option<Deadline>) -> Arc<AtomicU8> {
    let stopped = Arc::new(AtomicU8::new(0));
    let flag = stopped.clone();
    let own = parser.config().parse.budget;
    let (every, check) = (own.check_every_n, own.on_check);
    parser.parse_budget(1, move |ctx| {
        if ctx.rule_stack.len() > MAX_RULE_DEPTH {
            flag.store(STOP_DEPTH, Ordering::Relaxed);
            return false;
        }
        if deadline.as_ref().is_some_and(Deadline::passed) {
            flag.store(STOP_TIME, Ordering::Relaxed);
            return false;
        }
        match &check {
            Some(check) if every > 0 && ctx.iteration % every == 0 => check(ctx),
            _ => true,
        }
    });
    stopped
}

/// Parse an in-memory source (stdin) as `format`, within the time limit of
/// [`Limits::current`].
pub fn load_str(source: String, format: Format) -> Result<Loaded, LoadError> {
    load_str_within(source, format, Limits::current())
}

/// [`load_str`] within its own limits (the source is already read, so only
/// the time limit applies).
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

/// Read and parse a file within [`Limits::current`]; `format` overrides the
/// extension.
pub fn load_path(path: &Path, format: Option<Format>) -> Result<Loaded, LoadError> {
    load_path_within(path, format, Limits::current())
}

/// [`load_path`] within its own limits. A file known to be over the size
/// limit is refused before any of it is read.
pub fn load_path_within(
    path: &Path,
    format: Option<Format>,
    limits: Limits,
) -> Result<Loaded, LoadError> {
    let origin = origin_of(path);
    let file = std::fs::File::open(path)
        .map_err(|e| LoadError::new(e.to_string()).with_origin(&origin))?;
    if let (Some(max), Ok(meta)) = (limits.max_size, file.metadata()) {
        if meta.is_file() && meta.len() > max {
            return Err(LoadError::too_large(Some(meta.len()), max).with_origin(&origin));
        }
    }
    let bytes = read_within(file, limits.max_size).map_err(|e| e.with_origin(&origin))?;
    let source = match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(e) => String::from_utf8_lossy(e.as_bytes()).into_owned(),
    };
    let format = format.unwrap_or_else(|| Format::detect(path));
    load_str_within(source, format, limits).map_err(|e| e.with_origin(&origin))
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
    fn detection() {
        assert_eq!(Format::detect(Path::new("a/b.json")), Format::Json);
        assert_eq!(Format::detect(Path::new("x.YML")), Format::Yaml);
        assert_eq!(Format::detect(Path::new("notes")), Format::Text);
        assert_eq!(Format::detect(Path::new("weird.xyz")), Format::Text);
        assert_eq!(Format::from_name("md"), Some(Format::Markdown));
        assert_eq!(Format::from_name("Markdown"), Some(Format::Markdown));
        assert_eq!(Format::from_name("nope"), None);
        assert_eq!(Format::from_name("ndjson"), Some(Format::Jsonl));
    }

    #[test]
    fn text_lines() {
        assert_eq!(lines("a\r\nb\n\nc\n"), vec!["a", "b", "", "c"]);
        assert_eq!(lines(""), Vec::<&str>::new());
        assert_eq!(lines("x"), vec!["x"]);
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
        // No test sets the process's limits: a parse in a test running
        // alongside would see them.
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
        // A generous limit lets a small document through.
        let quick = parse_within(&many_tables(2), Format::Toml, Some(Duration::from_secs(60)));
        assert_eq!(quick.unwrap().len(), 1 + 1 + 2 * 3);
    }

    #[test]
    fn a_parse_of_few_steps_is_held_to_its_timeout_too() {
        // One long string is a handful of the engine's steps, with nearly
        // all the time spent in one of them.
        let src = format!("\"{}\"", "x".repeat(1 << 20));
        match parse_within(&src, Format::Json, Some(Duration::from_millis(1))) {
            Err(e) => assert!(e.is_timeout(), "{}", e.message),
            Ok(_) => panic!("a parse over its time was let through"),
        }
        assert!(parse_within(&src, Format::Json, Some(Duration::from_secs(600))).is_ok());
    }

    #[test]
    fn a_parse_that_ends_after_its_deadline_is_too_late() {
        // With no alarm raised, as when the last step runs past the
        // deadline, the parse is judged as it ends, value or error.
        let silent = || Deadline {
            at: Instant::now(),
            limit: Duration::from_millis(1),
            alarm: Some(Arc::new(AtomicBool::new(false))),
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
        // In time, it stands.
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
        };
        let e = parse_here(&many_tables(50), Format::Toml, Some(passed)).unwrap_err();
        assert!(e.is_timeout(), "{}", e.message);
        assert!(e.line > 0, "stopped where it had got to");
        assert!(e.hint.contains("got this far"), "{}", e.hint);
    }

    /// Parse as [`parse`] does, on a stack of [`PARSE_STACK`], with YAML's
    /// own depth limit taken off: YAML standing for a grammar with no limit
    /// of its own, which is what the cap is for.
    fn parse_yaml_without_its_own_limit(src: &str) -> Result<Doc, LoadError> {
        std::thread::scope(|scope| {
            std::thread::Builder::new()
                .stack_size(PARSE_STACK)
                .spawn_scoped(scope, || {
                    let mut parser = make_parser(Format::Yaml).expect("YAML has a grammar");
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
        // This deep (50,000 levels), a grammar with no limit of its own can
        // overflow the stack without the cap, ending the process; with it,
        // the parse fails at the cap.
        let yaml = "- ".repeat(50_000) + "x\n";
        let e = parse_yaml_without_its_own_limit(&yaml).unwrap_err();
        assert_eq!(e.code, "too_deep");
        assert!(e.message.contains("about 1000 levels"), "{}", e.message);
        assert!(e.plain_report().starts_with("[aless/too_deep]"));
        // Nesting well inside the cap parses.
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
        // JSON stops at 127 levels and YAML's compact sequences at 127, each
        // with a parse guard, which aless's budget does not replace, and XML
        // at 256 open elements, in its lexer, all with the engine's
        // `cancel`, whose hint blames a budget of the caller's. aless
        // reports them as it does its own cap, at the place the grammar
        // stopped.
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
        // An unexpected character is still one.
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
        // A file name holding a newline and an escape sequence, and a
        // message that happens to quote the placeholder.
        let e = LoadError::new("cannot read <no-file>").with_origin("evil\n\x1b[31mname");
        let lines: Vec<&str> = e.report.lines().collect();
        assert_eq!(lines.len(), 2, "{:?}", e.report);
        assert!(lines[0].ends_with("cannot read <no-file>"), "{lines:?}");
        assert!(lines[1].ends_with("evil\\n\\u001b[31mname"), "{lines:?}");

        // An escape sequence in the quoted source is shown as a picture,
        // one column wide, so the caret still points at it.
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
        // A hint that does not match its template: the token's first
        // occurrence, and any other control character.
        assert_eq!(
            escape_hint("x \n y\u{7}\nz", Some("other"), "\n"),
            "x \\n y\\u0007\nz"
        );
        assert_eq!(escape_hint("x \n y", None, "\n"), "x \\n y");
    }

    #[test]
    fn grammar_codes_carry_their_own_hints() {
        // Until ini 0.5.10 and zon 0.5.9 these codes had no hint, so the
        // report fell back to the one for an unknown code, which calls the
        // error a bug in the parser.
        for (src, format, code) in [
            ("[s\nk = v\n", Format::Ini, "unterminated_section"),
            ("0X2A", Format::Zon, "zon_number"),
        ] {
            let plain = parse(src, format).unwrap_err().plain_report();
            assert!(plain.contains(&format!("/{code}]:")), "{plain}");
            assert!(!plain.contains("probably a bug"), "{plain}");
        }
    }

    #[test]
    fn every_format_parses_its_sample() {
        let samples: [(Format, &str); 14] = [
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
        ];
        for (format, src) in samples {
            let doc = parse(src, format).unwrap_or_else(|e| panic!("{format}: {e}"));
            assert!(doc.len() > 1, "{format} produced a scalar-only document");
            assert!(
                doc.nodes.iter().any(|n| n.line > 0),
                "{format}: no node carries a source line"
            );
        }
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
    }
}
