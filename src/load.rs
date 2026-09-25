//! Format detection and loading: a file (or a string) goes in, a
//! positioned [`Doc`] comes out, parsed by the tabnas grammar for its
//! format, or split into lines when no grammar claims it.

use std::fmt;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

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
    /// `<no-file>`.
    pub fn with_origin(mut self, origin: &str) -> LoadError {
        self.report = self.report.replacen(NO_FILE, origin, 1);
        self
    }

    /// From an engine error. Control characters in the offending token
    /// (an unterminated string runs into a newline, YAML indentation is a
    /// newline and spaces) would otherwise break the report's lines, so
    /// they are shown escaped, the way the source view would show them.
    pub fn from_tabnas(e: &tabnas::TabnasError) -> LoadError {
        let mut shown = e.clone();
        shown.detail = escape_controls(&e.detail);
        if e.src.chars().any(char::is_control) {
            // The hint template injects the token text once, before any of
            // its own line breaks, so the first occurrence is the injected
            // one.
            shown.hint = e.hint.replacen(&e.src, &escape_controls(&e.src), 1);
        }
        let detail = shown.detail.trim();
        let message = if detail.starts_with(&e.code) || e.code.is_empty() {
            detail.to_string()
        } else {
            format!("{}: {}", e.code, detail)
        };
        LoadError {
            message,
            line: e.row as u32,
            col: e.col as u32,
            report: shown.to_string(),
        }
    }

    /// The report without its colour codes.
    pub fn plain_report(&self) -> String {
        strip_ansi(&self.report)
    }
}

/// Show control characters as escapes: `\n`, `\t`, `\r`, else `\u001b`.
pub fn escape_controls(s: &str) -> String {
    if !s.chars().any(char::is_control) {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
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

static CATCHING: AtomicUsize = AtomicUsize::new(0);

/// Is a grammar running under [`parse`]'s panic guard right now? A panic
/// hook (the terminal's, which restores the screen) should stand down
/// then: the panic is caught and reported as a [`LoadError`].
pub fn parse_in_progress() -> bool {
    CATCHING.load(Ordering::SeqCst) > 0
}

struct CatchGuard;

impl CatchGuard {
    fn enter() -> CatchGuard {
        CATCHING.fetch_add(1, Ordering::SeqCst);
        CatchGuard
    }
}

impl Drop for CatchGuard {
    fn drop(&mut self) {
        CATCHING.fetch_sub(1, Ordering::SeqCst);
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
        Ok(Err(e)) => Err(LoadError::from_tabnas(&e)),
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
