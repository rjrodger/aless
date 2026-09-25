//! Format detection and loading: a file (or a string) goes in, a
//! positioned [`Doc`] comes out, parsed by the tabnas grammar for its
//! format, or split into lines when no grammar claims it.

use std::cell::Cell;
use std::collections::HashMap;
use std::fmt;
use std::path::Path;

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
    pub line: u32,
    pub col: u32,
    /// The full report, as the tabnas engine renders it: the `[tag/code]`
    /// header, the `-->` location, the source lines around the error with
    /// a caret under it, the grammar's hint and link, and the engine's
    /// diagnostics line. Carries the engine's ANSI colour codes, which the
    /// renderer turns into styles.
    pub report: String,
}

/// The placeholder the engine writes where a file name belongs.
const NO_FILE: &str = "<no-file>";

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
            line: 0,
            col: 0,
            report,
        }
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
        LoadError {
            message,
            line: e.row as u32,
            col: e.col as u32,
            report,
        }
    }

    /// The report without its colour codes.
    pub fn plain_report(&self) -> String {
        strip_ansi(&self.report)
    }
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

/// Parse `src` as `format`.
pub fn parse(src: &str, format: Format) -> Result<Doc, LoadError> {
    let src = src.strip_prefix('\u{feff}').unwrap_or(src);
    let Some(mut parser) = make_parser(format) else {
        return Ok(Doc::from_lines(&lines(src)));
    };
    let sink = prov::capture(&mut parser);
    // A grammar is a plugin; a defect in one must not take the viewer down.
    let outcome = {
        let _guard = CatchGuard::enter();
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            parser.parse(src).map_err(Box::new)
        }))
    };
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

/// Parse an in-memory source (stdin) as `format`.
pub fn load_str(source: String, format: Format) -> Result<Loaded, LoadError> {
    let doc = parse(&source, format)?;
    Ok(Loaded {
        doc,
        format,
        source,
    })
}

/// Read and parse a file; `format` overrides the extension.
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

pub fn load_path(path: &Path, format: Option<Format>) -> Result<Loaded, LoadError> {
    let origin = origin_of(path);
    let bytes =
        std::fs::read(path).map_err(|e| LoadError::new(e.to_string()).with_origin(&origin))?;
    let source = match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(e) => String::from_utf8_lossy(e.as_bytes()).into_owned(),
    };
    let format = format.unwrap_or_else(|| Format::detect(path));
    load_str(source, format).map_err(|e| e.with_origin(&origin))
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
