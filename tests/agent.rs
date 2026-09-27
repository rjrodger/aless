//! The agent interface, end to end: the real binary, run the way a script
//! or an agent runs it, with standard output and error captured (so never
//! a terminal) and standard input piped or closed.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::{json, Value};

const BIN: &str = env!("CARGO_BIN_EXE_aless");

/// The fixture grammars and their samples.
const GRAMMARS: &str = "tests/fixtures/grammars";

/// The files (not the directories) under `tests/fixtures`, sorted.
fn fixture_files() -> Vec<String> {
    let mut files: Vec<String> = std::fs::read_dir("tests/fixtures")
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_file())
        .map(|p| p.display().to_string())
        .collect();
    files.sort();
    files
}

/// Run aless with `args`, and `stdin` piped in (none: standard input is
/// closed, as it is for most agent tool calls).
fn aless(args: &[&str], stdin: Option<&str>) -> Output {
    let mut child = Command::new(BIN)
        .args(args)
        .env_remove("NO_COLOR")
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("aless starts");
    if let Some(text) = stdin {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(text.as_bytes())
            .unwrap();
    }
    child.wait_with_output().expect("aless finishes")
}

fn json(bytes: &[u8]) -> Value {
    let text = String::from_utf8_lossy(bytes);
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("not JSON ({e}): {text}"))
}

fn code(out: &Output) -> i32 {
    out.status.code().expect("aless exits, not killed")
}

#[test]
fn piped_output_is_the_document_as_json() {
    let out = aless(&["tests/fixtures/nested.json"], None);
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    assert!(out.stderr.is_empty());
    let file = std::fs::read_to_string("tests/fixtures/nested.json").unwrap();
    assert_eq!(
        json(&out.stdout),
        serde_json::from_str::<Value>(&file).unwrap()
    );
    // Any format: the YAML fixture comes out as JSON too, key order kept.
    let out = aless(&["--compact", "tests/fixtures/sample.yaml"], None);
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "{\"name\":\"yaml sample\",\"items\":[\"one\",\"two\"],\"nested\":{\"ok\":true}}\n"
    );
}

#[test]
fn viewer_options_do_not_start_a_viewer() {
    let out = aless(
        &[
            "--mode",
            "line",
            "-n",
            "--depth",
            "1",
            "tests/fixtures/sample.toml",
        ],
        None,
    );
    assert_eq!(code(&out), 0);
    assert!(json(&out.stdout).is_object());
    assert!(!out.stdout.contains(&0x1b), "no escape codes");
}

#[test]
fn every_fixture_but_the_broken_one_checks() {
    let files = fixture_files();
    let args: Vec<&str> = std::iter::once("--check")
        .chain(files.iter().map(String::as_str))
        .collect();
    let out = aless(&args, None);
    assert_eq!(code(&out), 1, "bad.json fails");
    let report = json(&out.stdout);
    assert_eq!(report["ok"], json!(false));
    for f in report["files"].as_array().unwrap() {
        let bad = f["file"].as_str().unwrap().ends_with("bad.json");
        assert_eq!(f["ok"], json!(!bad), "{f}");
    }
    let formats: Vec<&str> = report["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["format"].as_str().unwrap())
        .collect();
    for want in [
        "csv", "ini", "json5", "jsonl", "markdown", "toml", "xml", "yaml", "zon",
    ] {
        assert!(formats.contains(&want), "{want} in {formats:?}");
    }
}

#[test]
fn outline_drill_down_and_positions() {
    let out = aless(
        &["--paths", "--depth", "1", "tests/fixtures/nested.json"],
        None,
    );
    assert_eq!(code(&out), 0);
    let v = json(&out.stdout);
    assert_eq!(v["file"], json!("tests/fixtures/nested.json"));
    assert_eq!(v["format"], json!("json"));
    let paths: Vec<&str> = v["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, [".", ".store", ".version"]);
    // One entry per line, so a listing greps.
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text
        .lines()
        .any(|l| l.trim_start().starts_with("{\"path\":\".store\"")));

    // A path from the listing goes straight back in.
    let out = aless(
        &[
            "--json",
            "--compact",
            "--path",
            ".store.books[0].tags",
            "tests/fixtures/nested.json",
        ],
        None,
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "[\"cs\",\"classic\"]\n"
    );

    // And a position maps back to a path: line 7 is the second book,
    // `{"title": "TAPL", …}`, and column 9 is inside its title.
    let at = |pos: &str| {
        let out = aless(
            &["--where", "--at", pos, "tests/fixtures/nested.json"],
            None,
        );
        json(&out.stdout)
    };
    assert_eq!(at("7")["path"], json!(".store.books[1]"));
    let v = at("7:9");
    assert_eq!(v["path"], json!(".store.books[1].title"));
    assert_eq!((v["line"].clone(), v["col"].clone()), (json!(7), json!(8)));
    assert_eq!(v["value"], json!("TAPL"));
}

#[test]
fn find_lists_matches_with_positions() {
    let out = aless(&["--find", "title", "tests/fixtures/nested.json"], None);
    assert_eq!(code(&out), 0);
    let v = json(&out.stdout);
    assert_eq!(v["total"], json!(2));
    let lines: Vec<u64> = v["matches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["line"].as_u64().unwrap())
        .collect();
    assert_eq!(lines, [6, 7]);
    // `--limit` bounds the list and says so.
    let out = aless(
        &[
            "--find",
            "title",
            "--limit",
            "1",
            "tests/fixtures/nested.json",
        ],
        None,
    );
    let v = json(&out.stdout);
    assert_eq!(v["matches"].as_array().unwrap().len(), 1);
    assert_eq!(v["truncated"], json!(true));
}

#[test]
fn standard_input() {
    let out = aless(&["--compact"], Some("{\"a\": [1, 2]}"));
    assert_eq!(code(&out), 0);
    assert_eq!(String::from_utf8_lossy(&out.stdout), "{\"a\":[1,2]}\n");
    let out = aless(&["-k", "yaml", "--compact", "-"], Some("a: [1, 2]\n"));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "{\"a\":[1,2]}\n");
    // Nothing piped and nothing named: say so, do not wait.
    let out = aless(&["--json"], None);
    assert_eq!(code(&out), 2);
    assert_eq!(json(&out.stderr)["error"]["kind"], json!("usage"));
}

#[test]
fn failures_are_json_on_stderr_with_a_status() {
    let out = aless(&["tests/fixtures/bad.json"], None);
    assert_eq!(code(&out), 1);
    assert!(out.stdout.is_empty());
    let e = &json(&out.stderr)["error"];
    assert_eq!(e["kind"], json!("parse"));
    assert_eq!(e["file"], json!("tests/fixtures/bad.json"));
    assert_eq!(e["code"], json!("unexpected"));
    assert!(e["line"].is_u64() && e["col"].is_u64(), "{e}");
    assert!(e["report"]
        .as_str()
        .unwrap()
        .contains("tests/fixtures/bad.json:"));

    let out = aless(&["tests/fixtures/missing.json"], None);
    assert_eq!(code(&out), 3);
    assert_eq!(json(&out.stderr)["error"]["kind"], json!("io"));

    let out = aless(
        &["--path", ".store.nope", "tests/fixtures/nested.json"],
        None,
    );
    assert_eq!(code(&out), 4);
    let e = &json(&out.stderr)["error"];
    assert_eq!(e["kind"], json!("not_found"));
    assert_eq!(e["keys"], json!(["name", "open", "books", "counts"]));

    for args in [
        &["--json", "--paths", "tests/fixtures/nested.json"][..],
        &["--no-such-option", "tests/fixtures/nested.json"],
        &["--limit", "many", "tests/fixtures/nested.json"],
        &["--at", "0", "tests/fixtures/nested.json"],
        &["--path", ".a", "--at", "3", "tests/fixtures/nested.json"],
        &["--check", "--path", ".a", "tests/fixtures/nested.json"],
        &["--json=yes", "tests/fixtures/nested.json"],
        &["tests/fixtures"],
    ] {
        let out = aless(args, None);
        assert_eq!(code(&out), 2, "{args:?}");
        assert!(out.stdout.is_empty(), "{args:?}");
        let e = json(&out.stderr);
        assert_eq!(e["error"]["kind"], json!("usage"), "{args:?}");
    }
}

/// An output that cannot be written (here a full disk) is an `io` error
/// in the documented shape, not plain text.
#[cfg(target_os = "linux")]
#[test]
fn an_unwritable_output_is_reported_as_json() {
    let full = std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/full")
        .expect("/dev/full");
    let out = Command::new(BIN)
        .arg("tests/fixtures/nested.json")
        .stdin(Stdio::null())
        .stdout(full)
        .stderr(Stdio::piped())
        .output()
        .unwrap();
    assert_eq!(code(&out), 3);
    let e = &json(&out.stderr)["error"];
    assert_eq!(e["kind"], json!("io"));
    assert_eq!(e["file"], Value::Null);
    assert!(e["message"]
        .as_str()
        .unwrap()
        .starts_with("cannot write standard output"));
}

/// A stream that cannot be written reports the renderer's own failure,
/// with its `output` state, not the plainer error a second flush of the
/// same full disk would give.
#[cfg(target_os = "linux")]
#[test]
fn an_unwritable_render_keeps_the_renderers_report() {
    let full = std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/full")
        .expect("/dev/full");
    let out = Command::new(BIN)
        .args(["--render", "csv", "tests/fixtures/sample.jsonl"])
        .stdin(Stdio::null())
        .stdout(full)
        .stderr(Stdio::piped())
        .output()
        .unwrap();
    assert_eq!(code(&out), 3);
    let e = &json(&out.stderr)["error"];
    assert_eq!(e["kind"], json!("transduce"), "{e}");
    assert_eq!(e["code"], json!("OUTPUT_FAILED"));
    assert_eq!(e["file"], json!("tests/fixtures/sample.jsonl"));
    assert_eq!(e["output"], json!("none"));
}

#[test]
fn inputs_over_max_size_are_refused_with_status_5() {
    let len = std::fs::metadata("tests/fixtures/nested.json")
        .unwrap()
        .len();
    let out = aless(&["--max-size", "100", "tests/fixtures/nested.json"], None);
    assert_eq!(code(&out), 5);
    assert!(out.stdout.is_empty());
    let e = &json(&out.stderr)["error"];
    assert_eq!(e["kind"], json!("too_large"));
    assert_eq!(e["size"], json!(len));
    assert_eq!(e["limit"], json!(100));
    // Standard input is read no further than the limit.
    let out = aless(&["--max-size=10"], Some("{\"a\": [1, 2, 3, 4, 5]}"));
    assert_eq!(code(&out), 5);
    assert_eq!(json(&out.stderr)["error"]["size"], Value::Null);
    // 0 lifts the limit; the default reads ordinary files.
    for args in [
        &["--max-size", "0", "tests/fixtures/nested.json"][..],
        &["tests/fixtures/nested.json"],
    ] {
        assert_eq!(code(&aless(args, None)), 0, "{args:?}");
    }
    let out = aless(&["--max-size", "lots", "tests/fixtures/nested.json"], None);
    assert_eq!(code(&out), 2);
}

#[test]
fn a_parse_past_timeout_is_stopped_with_status_6() {
    let toml: String = (0..400)
        .map(|i| format!("[[item]]\nid = {i}\nname = \"item {i}\"\n\n"))
        .collect();
    let out = aless(
        &["-k", "toml", "--timeout", "0.001", "--paths"],
        Some(&toml),
    );
    assert_eq!(code(&out), 6, "{}", String::from_utf8_lossy(&out.stderr));
    let e = &json(&out.stderr)["error"];
    assert_eq!(e["kind"], json!("timeout"));
    assert_eq!(e["seconds"], json!(0.001));
    // One long string is a few steps of the parser, but no exception.
    let long = format!("\"{}\"", "x".repeat(1 << 20));
    let out = aless(&["--timeout", "0.001", "--paths"], Some(&long));
    assert_eq!(code(&out), 6, "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(json(&out.stderr)["error"]["kind"], json!("timeout"));
    // Without one, and with a bad one.
    let quick = aless(
        &["-k", "toml", "--timeout", "0", "--compact"],
        Some("a = 1\n"),
    );
    assert_eq!(String::from_utf8_lossy(&quick.stdout), "{\"a\":1}\n");
    assert_eq!(code(&aless(&["--timeout", "soon", "x.json"], None)), 2);
}

#[test]
fn nesting_too_deep_to_parse_fails_cleanly() {
    // Nesting this deep can overflow a parser's stack, and the process
    // aborts with no error to report. YAML has no limit of its own and
    // stops at aless's cap; XML stops sooner, at its own limit, and reads
    // the same way.
    let dir = std::env::temp_dir().join(format!("aless-deep-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, text) in [
        ("deep.yaml", "- ".repeat(50_000) + "x\n"),
        ("deep.xml", "<a>".repeat(50_000) + &"</a>".repeat(50_000)),
    ] {
        let deep = dir.join(name);
        std::fs::write(&deep, text).unwrap();
        let out = aless(&[deep.to_str().unwrap()], None);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(code(&out), 1, "{name}: {stderr}");
        let e = &json(&out.stderr)["error"];
        assert_eq!(e["kind"], json!("parse"), "{name}");
        assert_eq!(e["code"], json!("too_deep"), "{name}");
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn help_leads_with_the_agent_interface() {
    let out = aless(&["--help"], None);
    assert_eq!(code(&out), 0);
    let text = String::from_utf8_lossy(&out.stdout);
    let head: String = text.lines().take(30).collect::<Vec<_>>().join("\n");
    for flag in [
        "--json", "--paths", "--find", "--where", "--check", "--render",
    ] {
        assert!(head.contains(flag), "{flag} in the first lines:\n{head}");
    }
    assert!(text.find("WITHOUT A SCREEN") < text.find("THE VIEWER"));
}

#[test]
fn a_reader_that_stops_early_is_not_an_error() {
    // More than a pipe buffer, so aless is still writing when the reader
    // goes away.
    let dir = std::env::temp_dir().join(format!("aless-agent-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let big = dir.join("big.json");
    let items: Vec<String> = (0..20_000).map(|i| format!("{{\"n\": {i}}}")).collect();
    std::fs::write(&big, format!("[{}]", items.join(","))).unwrap();
    let mut child = Command::new(BIN)
        .arg(&big)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut first = [0u8; 16];
    child.stdout.take().unwrap().read_exact(&mut first).unwrap();
    // The read end is closed here.
    let out = child.wait_with_output().unwrap();
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    assert!(
        out.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

/// The worked example of the transducer's design: the records under a
/// path, as the always-quoted CSV the spec fixes, byte for byte.
#[test]
fn render_csv_exports_the_records_at_a_path() {
    let out = aless(
        &[
            "--render",
            "csv",
            "--path",
            ".response.payload.deep.records",
            "tests/fixtures/records.json",
        ],
        None,
    );
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    assert!(out.stderr.is_empty());
    assert_eq!(
        out.stdout,
        b"\"id\",\"person\",\"account\"\r\n\"123\",\"{\"\"name\"\":\"\"Alice\"\"}\",\"{\"\"balance\"\":50.25}\"\r\n\"456\",\"{\"\"name\"\":\"\"Bob\"\"}\",\"{\"\"balance\"\":72}\"\r\n"
    );
    // Line-delimited inputs, a record at a time: the rows are the lines,
    // or the records keyed by the header.
    let out = aless(&["--render", "csv", "tests/fixtures/sample.jsonl"], None);
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "\"id\",\"name\",\"tags\"\r\n\"1\",\"ada\",\"[\"\"a\"\",\"\"b\"\"]\"\r\n\"2\",\"lin\",\"[]\"\r\n\"3\",\"kim\",\"[\"\"c\"\"]\"\r\n"
    );
    let out = aless(&["--render", "csv", "tests/fixtures/sample.csv"], None);
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "\"name\",\"age\",\"city\"\r\n\"ada\",\"36\",\"london\"\r\n\"lin\",\"28\",\"helsinki\"\r\n"
    );
    // Standard input too, with -k saying the format.
    let out = aless(
        &["-k", "jsonl", "--render", "csv"],
        Some("{\"n\": 1}\n{\"n\": 2.50}\n"),
    );
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "\"n\"\r\n\"1\"\r\n\"2.50\"\r\n"
    );
}

/// `--render json` is `--json`, streamed: the same value for every fixture.
#[test]
fn render_json_agrees_with_json_for_every_fixture() {
    /// Numbers as 64-bit floats: the two paths spell a number differently
    /// (`1.0` keeps its lexeme when streamed, `--json` prints `1`), and
    /// serde_json tells an integer from a float.
    fn normal(v: Value) -> Value {
        match v {
            Value::Number(n) => json!(n.as_f64().unwrap()),
            Value::Array(a) => Value::Array(a.into_iter().map(normal).collect()),
            Value::Object(m) => Value::Object(m.into_iter().map(|(k, v)| (k, normal(v))).collect()),
            v => v,
        }
    }
    let files: Vec<String> = fixture_files()
        .into_iter()
        .filter(|f| !f.ends_with("bad.json") && !f.ends_with("lines.txt"))
        .collect();
    assert!(files.len() >= 16, "{files:?}");
    for file in &files {
        let streamed = aless(&["--render", "json", file], None);
        assert_eq!(
            code(&streamed),
            0,
            "{file}: {}",
            String::from_utf8_lossy(&streamed.stderr)
        );
        let whole = aless(&["--json", "--compact", file], None);
        assert_eq!(
            normal(json(&streamed.stdout)),
            normal(json(&whole.stdout)),
            "{file}"
        );
        assert!(streamed.stdout.ends_with(b"\n"), "{file}");
    }
    // Indented like --json unless --compact, and from a path.
    let out = aless(
        &[
            "--render",
            "json",
            "--path",
            ".store.books[0].tags",
            "tests/fixtures/nested.json",
        ],
        None,
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "[\n  \"cs\",\n  \"classic\"\n]\n"
    );
    let out = aless(
        &[
            "--render",
            "json",
            "--compact",
            "--path",
            ".store.books[0].tags",
            "tests/fixtures/nested.json",
        ],
        None,
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "[\"cs\",\"classic\"]\n"
    );
}

#[test]
fn render_failures_have_the_transduce_shape_and_status() {
    // The input did not parse: status 1, the transducer's code, the position.
    let out = aless(&["--render", "json", "tests/fixtures/bad.json"], None);
    assert_eq!(code(&out), 1);
    assert!(out.stdout.is_empty());
    let e = &json(&out.stderr)["error"];
    assert_eq!(e["kind"], json!("transduce"));
    assert_eq!(e["code"], json!("INPUT_INVALID"));
    assert_eq!(e["file"], json!("tests/fixtures/bad.json"));
    assert_eq!(e["format"], json!("json"));
    assert!(e["line"].is_u64() && e["col"].is_u64(), "{e}");
    assert_eq!(e["output"], json!("none"));
    let out = aless(&["--render", "csv"], Some("[{\"a\": 1},\n {\"b\": }]"));
    assert_eq!(code(&out), 1);
    let e = &json(&out.stderr)["error"];
    assert_eq!(e["code"], json!("INPUT_INVALID"));
    assert_eq!((e["line"].clone(), e["col"].clone()), (json!(2), json!(8)));
    // An object is not a list of records: say so, and how to pick one.
    let out = aless(&["--render", "csv", "tests/fixtures/nested.json"], None);
    assert_eq!(code(&out), 1);
    let e = &json(&out.stderr)["error"];
    assert_eq!(e["code"], json!("INPUT_INVALID"));
    assert!(e["message"].as_str().unwrap().contains("--path"), "{e}");
    // A path that names nothing: status 4, as for --json.
    let out = aless(
        &[
            "--render",
            "csv",
            "--path",
            ".store.nope",
            "tests/fixtures/nested.json",
        ],
        None,
    );
    assert_eq!(code(&out), 4);
    let e = &json(&out.stderr)["error"];
    assert_eq!(e["kind"], json!("not_found"));
    assert_eq!(e["keys"], json!(["name", "open", "books", "counts"]));
    // An index on an object is its key, as --path reads it everywhere; on
    // an array a negative one counts from the end, which a stream cannot.
    let out = aless(
        &["--render", "json", "--compact", "--path", "[-1]"],
        Some(r#"{"-1": [5]}"#),
    );
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "[5]\n");
    let out = aless(&["--render", "json", "--path", "[-1]"], Some("[[1], [2]]"));
    assert_eq!(code(&out), 2);
    let e = &json(&out.stderr)["error"];
    assert_eq!(e["kind"], json!("usage"));
    assert!(e["message"].as_str().unwrap().contains("[-1]"), "{e}");
    // A key on the path repeated after the first was taken: the grammars
    // and --json keep the last, which a stream cannot honour.
    let out = aless(
        &["--render", "csv", "--path", ".rows"],
        Some(r#"{"rows":[{"v":1}],"rows":[{"v":2}]}"#),
    );
    assert_eq!(code(&out), 1);
    assert!(out.stdout.is_empty());
    let e = &json(&out.stderr)["error"];
    assert_eq!(e["kind"], json!("transduce"));
    assert_eq!(e["code"], json!("DUPLICATE_MEMBER"));
    assert_eq!(e["path"], json!("."));
    assert_eq!(e["output"], json!("none"));
    assert!(e["message"].as_str().unwrap().contains("\"rows\""), "{e}");
    // Mistakes in the command: status 2.
    for args in [
        &["--render", "csv", "--json", "tests/fixtures/nested.json"][..],
        &["--render", "csv", "--at", "3", "tests/fixtures/nested.json"],
        &["--render", "xml", "tests/fixtures/nested.json"],
        &["--render", "csv", "tests/fixtures/lines.txt"],
        &[
            "--render",
            "csv",
            "--path",
            ".store.books[-1]",
            "tests/fixtures/nested.json",
        ],
    ] {
        let out = aless(args, None);
        assert_eq!(code(&out), 2, "{args:?}");
        assert!(out.stdout.is_empty(), "{args:?}");
        assert_eq!(
            json(&out.stderr)["error"]["kind"],
            json!("usage"),
            "{args:?}"
        );
    }
    // A reader that stops early is no failure here either.
    let dir = std::env::temp_dir().join(format!("aless-render-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let big = dir.join("big.json");
    let items: Vec<String> = (0..20_000).map(|i| format!("{{\"n\": {i}}}")).collect();
    std::fs::write(&big, format!("[{}]", items.join(","))).unwrap();
    let mut child = Command::new(BIN)
        .args(["--render", "csv"])
        .arg(&big)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut first = [0u8; 16];
    child.stdout.take().unwrap().read_exact(&mut first).unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    assert!(out.stderr.is_empty());
    std::fs::remove_dir_all(&dir).unwrap();
}

/// One fixture grammar: its name, its file, the samples it reads
/// (`NAME.sample`, and `NAME` when the fixture has the real file too), and
/// the JSON they must parse to.
struct GrammarFixture {
    name: String,
    file: PathBuf,
    samples: Vec<PathBuf>,
    expected: Value,
    /// The expected file's text: what `--json` prints, byte for byte.
    expected_text: String,
}

/// Every `NAME.abnf` under the grammars directory. A grammar without its
/// `NAME.expected.json`, or without a sample, fails the test that reads it.
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

/// Every fixture grammar reads its sample to exactly the JSON its expected
/// file holds (the value compact, and the file's bytes pretty-printed, as
/// the library's README says of it), from the grammar file and from the
/// same text inline, and a sample named as the real file is detected by
/// that name.
#[test]
fn every_fixture_grammar_parses_its_sample_to_the_expected_json() {
    let fixtures = grammar_fixtures();
    let names: Vec<&str> = fixtures.iter().map(|f| f.name.as_str()).collect();
    assert!(
        names.contains(&"impl-kv") && names.contains(&"impl-words"),
        "{names:?}"
    );
    for f in &fixtures {
        let grammar = format!("{}={}", f.name, f.file.display());
        for sample in &f.samples {
            let sample = sample.to_str().unwrap();
            // A sample named as the real file (`hosts`) is read by that
            // name; a `.sample` needs -k.
            let by_name = sample.ends_with(&format!("/{}", f.name));
            let mut args = vec!["--grammar", &grammar, "--json", "--compact"];
            if !by_name {
                args.extend(["-k", &f.name]);
            }
            args.push(sample);
            let out = aless(&args, None);
            assert_eq!(
                code(&out),
                0,
                "{sample}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert_eq!(json(&out.stdout), f.expected, "{sample}");
            assert_eq!(
                String::from_utf8_lossy(&out.stdout).lines().count(),
                1,
                "{sample}: --compact"
            );
            // Pretty-printed, --json is the expected file itself.
            let mut pretty = vec!["--grammar", &grammar, "--json"];
            if !by_name {
                pretty.extend(["-k", &f.name]);
            }
            pretty.push(sample);
            let out = aless(&pretty, None);
            assert_eq!(
                String::from_utf8_lossy(&out.stdout),
                f.expected_text,
                "{sample}: --json is not {}.expected.json byte for byte",
                f.name
            );
            // --check names the format after the grammar.
            let mut check = vec!["--grammar", &grammar, "--check"];
            if !by_name {
                check.extend(["-k", &f.name]);
            }
            check.push(sample);
            let out = aless(&check, None);
            let report = json(&out.stdout);
            assert_eq!(report["ok"], json!(true), "{sample}: {report}");
            assert_eq!(report["files"][0]["format"], json!(f.name), "{sample}");
        }
        // The same grammar text inline.
        let text = std::fs::read_to_string(&f.file).unwrap();
        let expr = format!("{}={text}", f.name);
        let sample = f.samples[0].to_str().unwrap();
        let out = aless(
            &[
                "--grammar-expr",
                &expr,
                "-k",
                &f.name,
                "--json",
                "--compact",
                sample,
            ],
            None,
        );
        assert_eq!(
            code(&out),
            0,
            "{}: {}",
            f.name,
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(json(&out.stdout), f.expected, "{}: inline", f.name);
    }
}

/// The hosts grammar, as the README's "Custom grammars" section runs it
/// on the library's own sample: the first lines on standard input with
/// `-k hosts`, a position to a path, the records as CSV with the nested
/// `names` as JSON text in its cell; then the outline, the same text
/// inline, and the two inputs that build nothing. The README's output is
/// held to this byte for byte.
#[test]
fn the_hosts_grammar_runs_as_the_readme_shows() {
    let grammar = format!("hosts={GRAMMARS}/hosts.abnf");
    let sample = format!("{GRAMMARS}/hosts");
    let text = std::fs::read_to_string(&sample).unwrap();
    // `head -5 hosts | aless … -k hosts --json --compact`
    let head: String = text.lines().take(5).map(|l| format!("{l}\n")).collect();
    assert!(head.starts_with("# /etc/hosts") && head.ends_with("workstation\n"));
    let out = aless(
        &["--grammar", &grammar, "-k", "hosts", "--json", "--compact"],
        Some(&head),
    );
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "[{\"address\":\"127.0.0.1\",\"names\":[\"localhost\"]},{\"address\":\"127.0.1.1\",\"names\":[\"workstation.example.com\",\"workstation\"]}]\n"
    );
    // `--where --at 5:17`: the first name on line 5.
    let out = aless(
        &[
            "--grammar",
            &grammar,
            "--where",
            "--at",
            "5:17",
            "--compact",
            &sample,
        ],
        None,
    );
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        format!(
            "{{\"file\":\"{sample}\",\"format\":\"hosts\",\"path\":\"[1].names[0]\",\"kind\":\"string\",\"line\":5,\"col\":17,\"value\":\"workstation.example.com\"}}\n"
        )
    );
    // `--render csv | head -3`: a header and a row per record, the names
    // array as JSON text in its cell, CRLF line ends.
    let out = aless(&["--grammar", &grammar, "--render", "csv", &sample], None);
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    let csv = String::from_utf8_lossy(&out.stdout);
    let rows: Vec<&str> = csv.split("\r\n").collect();
    assert_eq!(
        &rows[..3],
        [
            "\"address\",\"names\"",
            "\"127.0.0.1\",\"[\"\"localhost\"\"]\"",
            "\"127.0.1.1\",\"[\"\"workstation.example.com\"\",\"\"workstation\"\"]\"",
        ]
    );
    assert_eq!(rows.len(), 13, "a header, eleven records, the final CRLF");
    assert_eq!(rows[12], "");
    // The outline: the array, then one record per host line, each at
    // its line in the file (comments and blank lines take none).
    let out = aless(
        &["--grammar", &grammar, "--paths", "--depth", "1", &sample],
        None,
    );
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    let v = json(&out.stdout);
    assert_eq!(v["format"], json!("hosts"));
    assert_eq!(v["total"], json!(12));
    assert_eq!(v["entries"][0]["kind"], json!("array"));
    assert_eq!(v["entries"][0]["length"], json!(11));
    let lines: Vec<u64> = v["entries"]
        .as_array()
        .unwrap()
        .iter()
        .skip(1)
        .map(|e| e["line"].as_u64().unwrap())
        .collect();
    assert_eq!(lines, [4, 5, 6, 7, 10, 11, 12, 13, 14, 15, 16]);
    // The same grammar inline reads the same.
    let expr = format!(
        "hosts={}",
        std::fs::read_to_string(format!("{GRAMMARS}/hosts.abnf")).unwrap()
    );
    let inline = aless(
        &["--grammar-expr", &expr, "--json", "--compact", &sample],
        None,
    );
    let file = aless(
        &["--grammar", &grammar, "--json", "--compact", &sample],
        None,
    );
    assert_eq!(
        code(&inline),
        0,
        "{}",
        String::from_utf8_lossy(&inline.stderr)
    );
    assert_eq!(inline.stdout, file.stdout);
    // Nothing to build: an empty file is null, and a file of comments and
    // blank lines is what the grammar's repetition makes of nothing.
    let dir = std::env::temp_dir().join(format!("aless-hosts-agent-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let empty = dir.join("empty.hosts");
    std::fs::write(&empty, "").unwrap();
    let out = aless(
        &[
            "--grammar",
            &grammar,
            "--json",
            "--compact",
            empty.to_str().unwrap(),
        ],
        None,
    );
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "null\n");
    let out = aless(
        &["--grammar", &grammar, "-k", "hosts", "--json", "--compact"],
        Some("# only\n\n# comments\n"),
    );
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "[]\n");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A custom grammar is a format like any other to every operation: `-k`
/// on standard input, detection by extension, outlines, positions, search
/// and the streamed exports.
#[test]
fn custom_grammars_work_with_every_operation() {
    let kv = format!("impl-kv={GRAMMARS}/impl-kv.abnf");
    let words = format!("impl-words={GRAMMARS}/impl-words.abnf");
    let out = aless(
        &["--grammar", &kv, "-k", "impl-kv", "--json", "--compact"],
        Some("a = 1\nb=two\n"),
    );
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "[{\"key\":\"a\",\"value\":\"1\"},{\"key\":\"b\",\"value\":\"two\"}]\n"
    );
    // By extension, with the outline naming the format.
    let dir = std::env::temp_dir().join(format!("aless-grammar-agent-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("settings.impl-kv");
    std::fs::write(&file, "# settings\nname = aless\nmode = line\n").unwrap();
    let file = file.to_str().unwrap();
    let out = aless(&["--grammar", &kv, "--paths", "--depth", "1", file], None);
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    let v = json(&out.stdout);
    assert_eq!(v["format"], json!("impl-kv"));
    let paths: Vec<&str> = v["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, [".", "[0]", "[1]"]);
    assert_eq!(v["entries"][1]["line"], json!(2));
    // A position maps to a path: line 3, column 8 is inside `line`.
    let out = aless(&["--grammar", &kv, "--where", "--at", "3:8", file], None);
    let v = json(&out.stdout);
    assert_eq!(v["path"], json!("[1].value"));
    assert_eq!(v["value"], json!("line"));
    assert_eq!((v["line"].clone(), v["col"].clone()), (json!(3), json!(8)));
    // Search, with the viewer's pattern.
    let out = aless(&["--grammar", &kv, "--find", "aless", file], None);
    let v = json(&out.stdout);
    assert_eq!(v["total"], json!(1));
    assert_eq!(v["matches"][0]["path"], json!("[0].value"));
    // --render csv: the records as rows, a nested array as JSON text in
    // its cell; the grammar is parsed whole first (no grammar from the
    // command line streams).
    let sample = format!("{GRAMMARS}/impl-words");
    let out = aless(&["--grammar", &words, "--render", "csv", &sample], None);
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "\"first\",\"rest\"\r\n\"alpha\",\"[\"\"1\"\"]\"\r\n\"beta\",\"[\"\"two\"\",\"\"2\"\"]\"\r\n\"gamma\",\"[\"\"3.5\"\",\"\"x\"\",\"\"y\"\"]\"\r\n"
    );
    // --render json agrees with --json.
    let streamed = aless(
        &[
            "--grammar",
            &words,
            "--render",
            "json",
            "--compact",
            &sample,
        ],
        None,
    );
    let whole = aless(&["--grammar", &words, "--json", "--compact", &sample], None);
    assert_eq!(
        code(&streamed),
        0,
        "{}",
        String::from_utf8_lossy(&streamed.stderr)
    );
    assert_eq!(streamed.stdout, whole.stdout);
    // Several names for one grammar, and several grammars at once.
    let both = format!("impl-kv,conf={GRAMMARS}/impl-kv.abnf");
    let conf = dir.join("app.conf");
    std::fs::write(&conf, "k = v\n").unwrap();
    let out = aless(
        &[
            "--grammar",
            &both,
            "--grammar",
            &words,
            "--check",
            conf.to_str().unwrap(),
            &sample,
        ],
        None,
    );
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    let v = json(&out.stdout);
    assert_eq!(v["files"][0]["format"], json!("impl-kv"), "{v}");
    assert_eq!(v["files"][1]["format"], json!("impl-words"), "{v}");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A custom grammar keeps a rule open for every item of a repetition, so a
/// file of thousands of lines is not nesting: it parses, and `--check`
/// and `--render` take it too. Nesting is measured on the value the
/// grammar builds, past about 1,000 levels.
#[test]
fn custom_grammars_read_files_of_thousands_of_lines() {
    let dir = std::env::temp_dir().join(format!("aless-long-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let hosts = dir.join("long.hosts");
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
    let grammar = format!("hosts={GRAMMARS}/hosts.abnf");
    let file = hosts.to_str().unwrap();
    let out = aless(&["--grammar", &grammar, "--json", "--compact", file], None);
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    let v = json(&out.stdout);
    assert_eq!(v.as_array().unwrap().len(), 5_000);
    assert_eq!(
        v[4_999],
        json!({"address": "10.0.19.135", "names": ["host4999.example.com", "alias4999"]})
    );
    let out = aless(&["--grammar", &grammar, "--check", "--compact", file], None);
    assert_eq!(code(&out), 0);
    assert_eq!(json(&out.stdout)["ok"], json!(true));
    let out = aless(&["--grammar", &grammar, "--render", "csv", file], None);
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).lines().count(), 5_001);
    // A value nested past the cap is too_deep, found once the parse is
    // done, so without a position; within it, the compiler's tree comes
    // through (an object and its `kids` array per level).
    let nest = "n=doc = \"(\" doc \")\" / \"x\"\n";
    let deep = dir.join("deep.n");
    std::fs::write(&deep, "(".repeat(600) + "x" + &")".repeat(600)).unwrap();
    let out = aless(
        &["--grammar-expr", nest, "--json", deep.to_str().unwrap()],
        None,
    );
    assert_eq!(code(&out), 1);
    let e = &json(&out.stderr)["error"];
    assert_eq!(e["kind"], json!("parse"));
    assert_eq!(e["code"], json!("too_deep"));
    assert_eq!(e["format"], json!("n"));
    assert!(
        e["message"]
            .as_str()
            .unwrap()
            .contains("nested deeper than aless reads (about 1000 levels)"),
        "{e}"
    );
    assert_eq!(e["line"], Value::Null);
    std::fs::write(&deep, "(".repeat(400) + "x" + &")".repeat(400)).unwrap();
    let out = aless(
        &[
            "--grammar-expr",
            nest,
            "--paths",
            "--depth",
            "0",
            "--compact",
            deep.to_str().unwrap(),
        ],
        None,
    );
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    std::fs::remove_dir_all(&dir).unwrap();
}

/// The command line may give a built-in format's name to a grammar: that
/// grammar then reads the extension, and `--render` still parses whole.
#[test]
fn a_custom_grammar_may_shadow_a_built_in() {
    let dir = std::env::temp_dir().join(format!("aless-shadow-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("words.json");
    std::fs::write(&file, "hello world\n").unwrap();
    let file = file.to_str().unwrap();
    let grammar = "json=doc = *word   ; @array\nword = ( TX )\n";
    for args in [
        &["--grammar-expr", grammar, "--json", "--compact", file][..],
        &[
            "--grammar-expr",
            grammar,
            "-k",
            "json",
            "--json",
            "--compact",
            file,
        ],
        &[
            "--grammar-expr",
            grammar,
            "--render",
            "json",
            "--compact",
            file,
        ],
    ] {
        let out = aless(args, None);
        assert_eq!(
            code(&out),
            0,
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&out.stdout),
            "[\"hello\",\"world\"]\n",
            "{args:?}"
        );
    }
    let out = aless(
        &["--grammar-expr", grammar, "--paths", "--depth", "0", file],
        None,
    );
    assert_eq!(json(&out.stdout)["format"], json!("json"));
    // Without the grammar, the file is JSON and does not parse.
    assert_eq!(code(&aless(&["--json", file], None)), 1);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A grammar that does not compile is the command's mistake (status 2), a
/// grammar file that cannot be read is an io error (3), and an input the
/// grammar refuses is a parse error (1) in the grammar's name.
#[test]
fn grammar_failures_have_their_shapes_and_statuses() {
    let out = aless(
        &[
            "--grammar-expr",
            "bad=doc = nope\n",
            "--json",
            "tests/fixtures/lines.txt",
        ],
        None,
    );
    assert_eq!(code(&out), 2);
    assert!(out.stdout.is_empty());
    let e = &json(&out.stderr)["error"];
    assert_eq!(e["kind"], json!("usage"));
    assert!(
        e["message"]
            .as_str()
            .unwrap()
            .starts_with("--grammar-expr bad: abnf"),
        "{e}"
    );
    assert_eq!(e["grammar"], json!("bad"));
    assert!(e.get("file").is_none(), "{e}");
    // From a file, the file is named.
    let dir = std::env::temp_dir().join(format!("aless-badgrammar-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let bad = dir.join("bad.abnf");
    std::fs::write(&bad, "doc = nope\n").unwrap();
    let grammar = format!("bad={}", bad.display());
    let out = aless(
        &["--grammar", &grammar, "--json", "tests/fixtures/lines.txt"],
        None,
    );
    assert_eq!(code(&out), 2);
    let e = &json(&out.stderr)["error"];
    assert_eq!(e["kind"], json!("usage"));
    assert_eq!(e["grammar"], json!("bad"));
    assert_eq!(e["file"], json!(bad.display().to_string()));
    assert!(
        e["message"]
            .as_str()
            .unwrap()
            .starts_with("--grammar bad: abnf"),
        "{e}"
    );
    std::fs::remove_dir_all(&dir).unwrap();
    // A grammar file that is not there.
    let out = aless(
        &[
            "--grammar",
            "hosts=/nonexistent/hosts.abnf",
            "--json",
            "tests/fixtures/lines.txt",
        ],
        None,
    );
    assert_eq!(code(&out), 3);
    let e = &json(&out.stderr)["error"];
    assert_eq!(e["kind"], json!("io"));
    assert_eq!(e["grammar"], json!("hosts"));
    assert_eq!(e["file"], json!("/nonexistent/hosts.abnf"));
    assert_eq!(e["code"], json!("io"));
    // An input the grammar does not accept: a parse error, positioned, in
    // the grammar's name.
    let kv = format!("impl-kv={GRAMMARS}/impl-kv.abnf");
    let out = aless(
        &["--grammar", &kv, "-k", "impl-kv", "--json"],
        Some("a = 1\nb\nc = 3\n"),
    );
    assert_eq!(code(&out), 1);
    let e = &json(&out.stderr)["error"];
    assert_eq!(e["kind"], json!("parse"));
    assert_eq!(e["format"], json!("impl-kv"));
    assert_eq!(e["file"], json!("-"));
    assert_eq!(e["code"], json!("unexpected"));
    assert_eq!((e["line"].clone(), e["col"].clone()), (json!(3), json!(1)));
    assert!(e["report"].as_str().unwrap().contains("(stdin):3:1"), "{e}");
    // Mistakes in the options themselves, and a -k that names nothing.
    for args in [
        &["--grammar", "hosts", "--json", "tests/fixtures/lines.txt"][..],
        &[
            "--grammar",
            "=hosts.abnf",
            "--json",
            "tests/fixtures/lines.txt",
        ],
        &[
            "--grammar-expr",
            "hosts=",
            "--json",
            "tests/fixtures/lines.txt",
        ],
        &["--grammar", "--json", "tests/fixtures/lines.txt"],
        &["-k", "hosts", "--json", "tests/fixtures/lines.txt"],
    ] {
        let out = aless(args, None);
        assert_eq!(code(&out), 2, "{args:?}");
        let e = &json(&out.stderr)["error"];
        assert_eq!(e["kind"], json!("usage"), "{args:?}");
    }
    let out = aless(&["-k", "hosts", "--json", "tests/fixtures/lines.txt"], None);
    let message = json(&out.stderr)["error"]["message"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        message.starts_with("unknown format: hosts (one of json"),
        "{message}"
    );
    // The help names the options.
    let help = aless(&["--help"], None);
    let text = String::from_utf8_lossy(&help.stdout);
    assert!(
        text.contains("--grammar <NAME=FILE>") && text.contains("--grammar-expr"),
        "{text}"
    );
}
