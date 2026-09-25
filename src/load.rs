//! Format detection and loading: a file (or a string) goes in, a
//! positioned [`Doc`] comes out, parsed by the tabnas grammar for its
//! format, or split into lines when no grammar claims it.

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
    pub message: String,
    pub line: u32,
    pub col: u32,
}

impl LoadError {
    pub fn new(message: impl Into<String>) -> LoadError {
        LoadError {
            message: message.into(),
            line: 0,
            col: 0,
        }
    }
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

/// Parse `src` as `format`.
pub fn parse(src: &str, format: Format) -> Result<Doc, LoadError> {
    let src = src.strip_prefix('\u{feff}').unwrap_or(src);
    let Some(mut parser) = make_parser(format) else {
        return Ok(Doc::from_lines(&lines(src)));
    };
    let sink = prov::capture(&mut parser);
    // A grammar is a plugin; a defect in one must not take the viewer down.
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        parser.parse(src).map_err(Box::new)
    }));
    match outcome {
        Ok(Ok(value)) => {
            let mut doc = Doc::from_value(&value);
            if let Ok(toks) = sink.lock() {
                prov::align(&mut doc, &toks);
            }
            Ok(doc)
        }
        Ok(Err(e)) => Err(LoadError {
            message: {
                let detail = e.detail.trim();
                if detail.starts_with(&e.code) || e.code.is_empty() {
                    detail.to_string()
                } else {
                    format!("{}: {}", e.code, detail)
                }
            },
            line: e.row as u32,
            col: e.col as u32,
        }),
        Err(panic) => {
            let what = panic
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "unknown panic".to_string());
            Err(LoadError::new(format!("{format} grammar failed: {what}")))
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
pub fn load_path(path: &Path, format: Option<Format>) -> Result<Loaded, LoadError> {
    let bytes = std::fs::read(path).map_err(|e| LoadError::new(e.to_string()))?;
    let source = match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(e) => String::from_utf8_lossy(e.as_bytes()).into_owned(),
    };
    let format = format.unwrap_or_else(|| Format::detect(path));
    load_str(source, format)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{Key, Kind};

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
