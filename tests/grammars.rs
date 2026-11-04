//! Custom grammars, through the library the viewer is built on. The grammar
//! registry is one per process and these tests share it, so each registers
//! grammars under names of its own, or under the fixtures'.

use std::path::{Path, PathBuf};

use serde_json::Value;

use aless::fmt;
use aless::grammar::{self, Definition};
use aless::load::{self, Format, Limits};

const GRAMMARS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/grammars");

/// The document as `:w` writes it, final line break included, and as a value.
fn json_of(doc: &aless::doc::Doc) -> (String, Value) {
    let pretty = format!("{}\n", fmt::to_json_pretty(doc, 0, 2));
    let value: Value = serde_json::from_str(&fmt::to_json_compact(doc, 0)).unwrap();
    (pretty, value)
}

fn register(option: &str, value: &str) -> Format {
    let def = Definition::parse(option, value).unwrap_or_else(|e| panic!("{value}: {e}"));
    let id = grammar::register(def, Limits::DEFAULT).unwrap_or_else(|e| panic!("{value}: {e}"));
    Format::Custom(id)
}

/// One fixture grammar: its samples are `NAME.sample`, and `NAME` when the real file is there too.
struct GrammarFixture {
    name: String,
    file: PathBuf,
    samples: Vec<PathBuf>,
    expected: Value,
    expected_text: String,
}

/// Every `NAME.abnf` fixture; one without `NAME.expected.json` or a sample fails the test.
fn grammar_fixtures() -> Vec<GrammarFixture> {
    let dir = Path::new(GRAMMARS);
    let mut grammars: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "abnf"))
        .collect();
    grammars.sort();
    grammars
        .into_iter()
        .map(|file| {
            let name = file.file_stem().unwrap().to_str().unwrap().to_string();
            let expected = dir.join(format!("{name}.expected.json"));
            let expected_text = std::fs::read_to_string(&expected)
                .unwrap_or_else(|e| panic!("{}: {e}", expected.display()));
            let expected: Value = serde_json::from_str(&expected_text).unwrap();
            let samples: Vec<PathBuf> = [dir.join(format!("{name}.sample")), dir.join(&name)]
                .into_iter()
                .filter(|p| p.is_file())
                .collect();
            assert!(
                !samples.is_empty(),
                "{name}: no {name}.sample or {name} to read"
            );
            GrammarFixture {
                name,
                file,
                samples,
                expected,
                expected_text,
            }
        })
        .collect()
}

/// Every fixture grammar reads each sample to its expected JSON, as a value and
/// byte for byte, from the grammar file and from the same text inline.
#[test]
fn every_fixture_grammar_parses_its_sample_to_the_expected_json() {
    let fixtures = grammar_fixtures();
    let names: Vec<&str> = fixtures.iter().map(|f| f.name.as_str()).collect();
    assert!(
        names.contains(&"impl-kv") && names.contains(&"impl-words"),
        "{names:?}"
    );
    for f in &fixtures {
        let format = register("--grammar", &format!("{}={}", f.name, f.file.display()));
        assert_eq!(format.name(), f.name);
        for sample in &f.samples {
            // `hosts` is detected by its name; a `.sample` needs the format, as `-k` gives it.
            let by_name = sample.file_name().is_some_and(|n| n == f.name.as_str());
            let named = if by_name { None } else { Some(format) };
            let loaded = load::load_path(sample, named)
                .unwrap_or_else(|e| panic!("{}: {e}", sample.display()));
            assert_eq!(loaded.format, format, "{}", sample.display());
            let (pretty, value) = json_of(&loaded.doc);
            assert_eq!(value, f.expected, "{}", sample.display());
            assert_eq!(
                pretty,
                f.expected_text,
                "{}: not {}.expected.json byte for byte",
                sample.display(),
                f.name
            );
        }
        // The same grammar text inline.
        let text = std::fs::read_to_string(&f.file).unwrap();
        let inline = register("--grammar-expr", &format!("{}={text}", f.name));
        let loaded = load::load_path(&f.samples[0], Some(inline)).unwrap();
        assert_eq!(json_of(&loaded.doc).1, f.expected, "{}: inline", f.name);
    }
}

/// A long file reads at a constant rule depth, and a value nested past
/// the cap is `too_deep`, found once the parse is done, so without a position.
#[test]
fn custom_grammars_read_files_of_thousands_of_lines() {
    let dir = std::env::temp_dir().join(format!("aless-long-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let hosts = dir.join("long.hosts-long");
    let text: String = std::iter::once("# generated\n".to_string())
        .chain((0..5_000).map(|i| {
            format!(
                "10.{}.{}.{}\thost{i}.example.com alias{i}\n",
                (i >> 16) & 255,
                (i >> 8) & 255,
                i & 255
            )
        }))
        .collect();
    std::fs::write(&hosts, text).unwrap();
    let format = register("--grammar", &format!("hosts-long={GRAMMARS}/hosts.abnf"));
    let loaded = load::load_path(&hosts, None).unwrap();
    assert_eq!(loaded.format, format);
    let (_, value) = json_of(&loaded.doc);
    assert_eq!(value.as_array().unwrap().len(), 5_000);
    assert_eq!(
        value[4_999],
        serde_json::json!({"address": "10.0.19.135", "names": ["host4999.example.com", "alias4999"]})
    );
    let nest = register(
        "--grammar-expr",
        "nest-deep=doc = \"(\" doc \")\" / \"x\"\n",
    );
    let deep = dir.join("deep.nest-deep");
    std::fs::write(&deep, "(".repeat(600) + "x" + &")".repeat(600)).unwrap();
    let err = load::load_path(&deep, None).expect_err("600 levels are too deep");
    assert_eq!(err.code, "too_deep", "{err}");
    assert!(
        err.message
            .contains("nested deeper than aless reads (about 1000 levels)"),
        "{err}"
    );
    assert_eq!((err.line, err.col), (0, 0), "{err}");
    std::fs::write(&deep, "(".repeat(400) + "x" + &")".repeat(400)).unwrap();
    let loaded = load::load_path(&deep, Some(nest)).unwrap();
    assert!(loaded.doc.len() > 400);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// The command line may give a built-in format's name to a grammar: that
/// grammar then reads the extension, and `-k` of the name.
#[test]
fn a_custom_grammar_may_shadow_a_built_in() {
    let format = register(
        "--grammar-expr",
        "json=doc = *word   ; @array\nword = ( TX )\n",
    );
    assert_eq!(Format::detect(Path::new("words.json")), format);
    assert_eq!(Format::from_name("json"), Some(format));
    assert_eq!(format.name(), "json");
    let loaded = load::load_str("hello world\n".into(), format).unwrap();
    assert_eq!(
        fmt::to_json_compact(&loaded.doc, 0),
        "[\"hello\",\"world\"]"
    );
    // The built-in is still there, by its own variant: the text is not JSON.
    assert!(load::load_str("hello world\n".into(), Format::Json).is_err());
}

/// A grammar that does not compile is a usage error naming the option, the
/// grammar, the compiler's complaint and the file it came from, if any.
#[test]
fn a_grammar_that_does_not_compile_is_a_usage_error() {
    let def = Definition::parse("--grammar-expr", "bad=doc = nope\n").unwrap();
    let e = grammar::register(def, Limits::DEFAULT).expect_err("nope names no rule");
    assert!(e.is_usage(), "{e}");
    assert!(e.to_string().starts_with("--grammar-expr bad: abnf"), "{e}");
    assert_eq!(e.grammar, "bad");
    assert_eq!(e.file, None);
    let dir = std::env::temp_dir().join(format!("aless-badgrammar-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let bad = dir.join("bad.abnf");
    std::fs::write(&bad, "doc = nope\n").unwrap();
    let def = Definition::parse("--grammar", &format!("bad={}", bad.display())).unwrap();
    let e = grammar::register(def, Limits::DEFAULT).expect_err("nope names no rule");
    assert!(e.is_usage(), "{e}");
    assert_eq!(e.file, Some(bad.display().to_string()));
    assert!(e.to_string().starts_with("--grammar bad: abnf"), "{e}");
    // A grammar file that cannot be read is refused, though not as a usage error.
    let def = Definition::parse("--grammar", "gone=no/such/grammar.abnf").unwrap();
    let e = grammar::register(def, Limits::DEFAULT).expect_err("no such file");
    assert!(!e.is_usage(), "{e}");
    assert!(e.error.is_io(), "{e}");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A `--grammar` FILE is a path like any input's: one whose bytes are not
/// UTF-8 is read at those bytes, given as the next argument or attached
/// (`--grammar=NAME=FILE`), and not at a lossy spelling of them.
#[cfg(unix)]
#[test]
fn a_grammar_file_whose_path_is_not_utf8_is_read_where_it_is() {
    use std::ffi::{OsStr, OsString};
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let dir = std::env::temp_dir().join(format!("aless-nonutf8-grammar-{}", std::process::id()));
    let raw = dir.join(OsStr::from_bytes(b"g\xff"));
    let _ = std::fs::remove_dir_all(&dir);
    if let Err(e) = std::fs::create_dir_all(&raw) {
        eprintln!("skipping: this filesystem refuses non-UTF-8 names ({e})");
        return;
    }
    let file = raw.join("hosts.abnf");
    std::fs::copy(format!("{GRAMMARS}/hosts.abnf"), &file).unwrap();
    // A different grammar at the lossy spelling, so reading it there is a wrong
    // answer rather than a missing file.
    let lossy = dir.join("g\u{fffd}");
    std::fs::create_dir_all(&lossy).unwrap();
    std::fs::write(lossy.join("hosts.abnf"), "doc = *TX\n").unwrap();
    let sample = PathBuf::from(format!("{GRAMMARS}/hosts.sample"));
    let expected: Value = serde_json::from_str(
        &std::fs::read_to_string(format!("{GRAMMARS}/hosts.expected.json")).unwrap(),
    )
    .unwrap();
    let mut value = b"hosts-raw=".to_vec();
    value.extend_from_slice(file.as_os_str().as_bytes());
    let mut attached = b"--grammar=".to_vec();
    attached.extend_from_slice(&value);
    for raw_value in [
        OsString::from_vec(value.clone()),
        OsString::from_vec(grammar::after_eq(&OsString::from_vec(attached)).into_vec()),
    ] {
        let def = Definition::parse_os("--grammar", &raw_value).unwrap();
        assert_eq!(
            def.source.file().as_deref(),
            Some(file.to_string_lossy().as_ref())
        );
        let id = grammar::register(def, Limits::DEFAULT).unwrap_or_else(|e| panic!("{e}"));
        let loaded = load::load_path(&sample, Some(Format::Custom(id))).unwrap();
        assert_eq!(json_of(&loaded.doc).1, expected, "{raw_value:?}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
