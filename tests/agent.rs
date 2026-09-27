//! The agent interface, end to end: the real binary, run the way a script
//! or an agent runs it, with standard output and error captured (so never
//! a terminal) and standard input piped or closed.

use std::io::{Read, Write};
use std::process::{Command, Output, Stdio};

use serde_json::{json, Value};

const BIN: &str = env!("CARGO_BIN_EXE_aless");

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
    let mut files: Vec<String> = std::fs::read_dir("tests/fixtures")
        .unwrap()
        .map(|e| e.unwrap().path().display().to_string())
        .collect();
    files.sort();
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
    let mut files: Vec<String> = std::fs::read_dir("tests/fixtures")
        .unwrap()
        .map(|e| e.unwrap().path().display().to_string())
        .filter(|f| !f.ends_with("bad.json") && !f.ends_with("lines.txt"))
        .collect();
    files.sort();
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
