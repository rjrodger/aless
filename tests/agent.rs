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
        let written = child.stdin.take().unwrap().write_all(text.as_bytes());
        // aless may stop reading before the end: an input over --max-size,
        // or a --timeout that passes while it is still being read.
        if let Err(e) = written {
            assert_eq!(e.kind(), std::io::ErrorKind::BrokenPipe, "{e}");
        }
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

/// A failed `--check` still prints its report on standard output, with
/// status 1 and nothing on standard error. Each failing file's `error` is
/// the object a run on that file alone would print: the parse error's
/// every field for a file that does not parse, an `io` error for one that
/// is not there, and for a directory a `usage` error, its kind and message.
#[test]
fn a_failed_check_reports_each_files_own_error() {
    let out = aless(
        &[
            "--check",
            "tests/fixtures/bad.json",
            "tests/fixtures/grammars",
            "tests/fixtures/missing.json",
        ],
        None,
    );
    assert_eq!(code(&out), 1);
    assert!(
        out.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report = json(&out.stdout);
    assert_eq!(report["ok"], json!(false));
    let errors: Vec<&Value> = report["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| &f["error"])
        .collect();
    assert_eq!(errors[0]["kind"], json!("parse"));
    for key in [
        "file",
        "format",
        "code",
        "message",
        "line",
        "col",
        "hint",
        "source_line",
        "report",
    ] {
        assert!(errors[0].get(key).is_some(), "{key}: {}", errors[0]);
    }
    assert_eq!(
        *errors[1],
        json!({
            "kind": "usage",
            "message": "tests/fixtures/grammars is a directory: aless reads files (list a \
                directory with ls or find)"
        })
    );
    assert_eq!(errors[2]["kind"], json!("io"));
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

/// CSV and TSV records are keyed by the header row, which is lexed before
/// any value: the first record's first field used to be placed on the
/// header line, at the cell that spells its key. Every field is at its
/// value, and nothing sits on the header line.
#[test]
fn csv_and_tsv_positions_start_below_the_header() {
    for (file, format) in [
        ("tests/fixtures/sample.csv", "csv"),
        ("tests/fixtures/sample.tsv", "tsv"),
    ] {
        // A root array's item is printed `.[0]`, as jq takes it; the form
        // without the dot is still read.
        let out = aless(&["--where", "--path", "[0].name", file], None);
        assert_eq!(code(&out), 0, "{file}");
        let v = json(&out.stdout);
        assert_eq!(v["path"], json!(".[0].name"), "{file}");
        let again = aless(&["--where", "--path", ".[0].name", file], None);
        assert_eq!(json(&again.stdout), v, "{file}");
        assert_eq!(v["format"], json!(format));
        assert_eq!(
            (v["line"].clone(), v["col"].clone()),
            (json!(2), json!(1)),
            "{file}: the first field is on the first record's line"
        );
        assert_eq!(v["value"], json!("ada"));
        let out = aless(&["--where", "--at", "2:1", file], None);
        assert_eq!(json(&out.stdout)["path"], json!(".[0].name"), "{file}");
        let out = aless(&["--paths", file], None);
        let v = json(&out.stdout);
        let on_header = v["entries"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["line"] == json!(1))
            .count();
        assert_eq!(on_header, 0, "{file}: nothing is placed on the header line");
    }
}

/// `--at` inside the text answers the last node starting at or before the
/// position; outside it (a line past the last, a column past the end of
/// its line) it names nothing, status 4, with the node it would have
/// answered as `nearest`.
#[test]
fn positions_outside_the_text_are_not_found() {
    // 12 lines; line 11 is `  "version": 3` and line 12 is `}`.
    let nested = "tests/fixtures/nested.json";
    let at = |pos: &str, file: &str| aless(&["--where", "--at", pos, "--compact", file], None);
    // The last position inside the text, the closing brace at 12:1,
    // answers the node before it.
    let out = at("12:1", nested);
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(json(&out.stdout)["path"], json!(".version"));
    // Between two nodes on a line (line 6: `"title"` at 8, `"price"` at
    // 25), the one before.
    assert_eq!(
        json(&at("6:20", nested).stdout)["path"],
        json!(".store.books[0].title")
    );
    // A line past the last.
    let out = at("13", nested);
    assert_eq!(code(&out), 4);
    assert!(out.stdout.is_empty());
    let e = &json(&out.stderr)["error"];
    assert_eq!(e["kind"], json!("not_found"));
    assert_eq!(e["file"], json!(nested));
    assert_eq!(e["format"], json!("json"));
    assert_eq!(e["at"], json!("13"));
    assert_eq!(
        e["message"],
        json!("no line 13 in tests/fixtures/nested.json: it has 12 lines")
    );
    assert_eq!(e["nearest"]["path"], json!(".version"));
    assert_eq!(e["nearest"]["line"], json!(11));
    assert_eq!(e["nearest"]["value"], json!(3));
    assert_eq!(e["keys"], Value::Null);
    // A column past the end of its line: 12:2, after the brace.
    let out = at("12:2", nested);
    assert_eq!(code(&out), 4);
    let e = &json(&out.stderr)["error"];
    assert_eq!(e["at"], json!("12:2"));
    assert_eq!(
        e["message"],
        json!("no column 2 on line 12 of tests/fixtures/nested.json: the line has 1 character")
    );
    assert_eq!(e["nearest"]["path"], json!(".version"));
    // --json and --paths answer the same way.
    for op in ["--json", "--paths"] {
        let out = aless(&[op, "--at", "99", nested], None);
        assert_eq!(code(&out), 4, "{op}");
        assert!(out.stdout.is_empty(), "{op}");
        let e = &json(&out.stderr)["error"];
        assert_eq!(e["kind"], json!("not_found"), "{op}");
        assert_eq!(e["nearest"]["path"], json!(".version"), "{op}");
    }
    // Plain text, four lines, the third empty: an empty line exists, and
    // has no column inside it; the fourth, `fourth line`, has eleven.
    let lines = "tests/fixtures/lines.txt";
    assert_eq!(json(&at("3", lines).stdout)["path"], json!(".[2]"));
    let out = at("3:1", lines);
    assert_eq!(code(&out), 4);
    let e = &json(&out.stderr)["error"];
    assert_eq!(
        e["message"],
        json!("no column 1 on line 3 of tests/fixtures/lines.txt: the line is empty")
    );
    assert_eq!(e["nearest"]["path"], json!(".[2]"));
    assert_eq!(
        json(&at("4:11", lines).stdout)["value"],
        json!("fourth line")
    );
    assert_eq!(code(&at("4:12", lines)), 4);
    let out = at("5", lines);
    assert_eq!(code(&out), 4);
    assert_eq!(json(&out.stderr)["error"]["nearest"]["path"], json!(".[3]"));
    // CRLF, and a last line without a terminator.
    let dir = std::env::temp_dir().join(format!("aless-at-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let crlf = dir.join("crlf.json");
    std::fs::write(&crlf, "{\"a\": 1,\r\n \"b\": 2}\r\n").unwrap();
    let crlf = crlf.to_str().unwrap();
    // `{"a": 1,` is eight characters, and so is ` "b": 2}`: the CR after
    // each is not text, and the trailing CRLF starts no line.
    assert_eq!(json(&at("1:8", crlf).stdout)["path"], json!(".a"));
    let out = at("1:9", crlf);
    assert_eq!(code(&out), 4);
    assert_eq!(json(&out.stderr)["error"]["nearest"]["path"], json!(".a"));
    assert_eq!(json(&at("2:8", crlf).stdout)["path"], json!(".b"));
    assert_eq!(code(&at("2:9", crlf)), 4);
    let out = at("3", crlf);
    assert_eq!(code(&out), 4);
    assert_eq!(
        json(&out.stderr)["error"]["message"],
        json!(format!("no line 3 in {crlf}: it has 2 lines"))
    );
    let bare = dir.join("bare.json");
    std::fs::write(&bare, "{\"a\": 1,\n \"b\": 2}").unwrap();
    let bare = bare.to_str().unwrap();
    assert_eq!(json(&at("2:8", bare).stdout)["path"], json!(".b"));
    assert_eq!(code(&at("2:9", bare)), 4);
    let out = at("3", bare);
    assert_eq!(code(&out), 4);
    let e = &json(&out.stderr)["error"];
    assert_eq!(
        e["message"],
        json!(format!("no line 3 in {bare}: it has 2 lines"))
    );
    assert_eq!(e["nearest"]["path"], json!(".b"));
    // A CR that no LF follows ends no line: it is a character of its
    // line, as it is to the grammars, so ` "b": 2}` plus the CR is nine.
    let cr = dir.join("cr.json");
    std::fs::write(&cr, "{\"a\": 1,\n \"b\": 2}\r").unwrap();
    let cr = cr.to_str().unwrap();
    assert_eq!(json(&at("2:9", cr).stdout)["path"], json!(".b"));
    let out = at("2:10", cr);
    assert_eq!(code(&out), 4);
    assert_eq!(
        json(&out.stderr)["error"]["message"],
        json!(format!(
            "no column 10 on line 2 of {cr}: the line has 9 characters"
        ))
    );
    let out = at("3", cr);
    assert_eq!(code(&out), 4);
    assert_eq!(
        json(&out.stderr)["error"]["message"],
        json!(format!("no line 3 in {cr}: it has 2 lines"))
    );
    std::fs::remove_dir_all(&dir).ok();
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
    // --compact puts a usage error from the command line on one line too,
    // wherever it stands among the options; after `--` it is a file.
    let out = aless(
        &[
            "--no-such-option",
            "--compact",
            "tests/fixtures/nested.json",
        ],
        None,
    );
    assert_eq!(code(&out), 2);
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        "{\"error\":{\"kind\":\"usage\",\"message\":\"unknown option: --no-such-option (see \
         aless --help)\"}}\n"
    );
    let out = aless(
        &["--compact", "-k", "nope", "tests/fixtures/nested.json"],
        None,
    );
    assert_eq!(code(&out), 2);
    assert_eq!(String::from_utf8_lossy(&out.stderr).lines().count(), 1);
    assert!(json(&out.stderr)["error"]["message"]
        .as_str()
        .unwrap()
        .starts_with("unknown format: nope"));
    let out = aless(&["--no-such-option", "--", "--compact"], None);
    assert_eq!(code(&out), 2);
    assert!(String::from_utf8_lossy(&out.stderr).lines().count() > 1);
    assert_eq!(json(&out.stderr)["error"]["kind"], json!("usage"));
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

/// `--timeout` ends a run whose standard input is open and silent, as a
/// pipe from a writer that never writes is, in every mode: the deadline
/// runs from the start and covers reading the input (aless#16).
#[test]
fn a_silent_stdin_is_stopped_at_the_timeout_with_status_6() {
    let modes: [&[&str]; 4] = [
        &["--json"],
        &["-k", "jsonl", "--render", "csv"],
        &["--render", "json"],
        &["-k", "jsonl", "--alchemy-expr", "def export [input] input"],
    ];
    for mode in modes {
        let started = std::time::Instant::now();
        let mut child = Command::new(BIN)
            .args(mode)
            .args(["--timeout", "1"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("aless starts");
        // The writer's end stays open, and nothing is written to it.
        let held = child.stdin.take();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if started.elapsed() > std::time::Duration::from_secs(30) {
                let _ = child.kill();
                panic!("{mode:?}: still waiting on standard input after 30 s");
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        };
        drop(held);
        let out = child.wait_with_output().unwrap();
        assert_eq!(
            status.code(),
            Some(6),
            "{mode:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let e = &json(&out.stderr)["error"];
        assert_eq!(e["kind"], json!("timeout"), "{mode:?}");
        assert_eq!(e["seconds"], json!(1.0), "{mode:?}");
        assert!(out.stdout.is_empty(), "{mode:?}");
    }
    // A named file is read, and standard input left alone, open and
    // silent though it is.
    let mut child = Command::new(BIN)
        .args(["--timeout", "1", "--compact", "tests/fixtures/sample.json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("aless starts");
    let held = child.stdin.take();
    let started = std::time::Instant::now();
    while child.try_wait().unwrap().is_none() {
        assert!(started.elapsed() < std::time::Duration::from_secs(30));
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    drop(held);
    let out = child.wait_with_output().unwrap();
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
}

/// `--max-output` bounds what a program writes, with status 5 and the
/// transducer's limit named (aless#18).
#[test]
fn a_programs_output_is_held_to_max_output() {
    let mut doubling = String::from("def a0 \"0123456789\"\n");
    for i in 1..=40 {
        doubling.push_str(&format!("def a{i} (concat a{} a{})\n", i - 1, i - 1));
    }
    doubling.push_str("def export [input] (concat a40 (json input))\n");
    let out = aless(
        &["--alchemy-expr", &doubling, "--max-output", "64K"],
        Some("{\"a\": 1}"),
    );
    assert_eq!(code(&out), 5, "{}", String::from_utf8_lossy(&out.stderr));
    let e = &json(&out.stderr)["error"];
    assert_eq!(e["code"], json!("RESOURCE_LIMIT_EXCEEDED"));
    assert_eq!(e["limit"]["name"], json!("max_output_bytes"));
    assert_eq!(e["limit"]["value"], json!(65536));
    assert!(out.stdout.len() <= 65536, "{}", out.stdout.len());
    let bad = aless(&["--max-output", "lots", "x.json"], None);
    assert_eq!(code(&bad), 2);
    assert!(
        String::from_utf8_lossy(&bad.stderr).contains("--max-output needs a size"),
        "{}",
        String::from_utf8_lossy(&bad.stderr)
    );
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
    // The exit statuses say what --grammar and --alchemy add to each, as
    // the README's and the skill's tables do: a grammar or program file
    // that cannot be read is status 3, one over --max-size status 5, and
    // the deadline covers the program's run.
    let statuses = &text[text.find("Exit status:").unwrap()..text.find("THE VIEWER").unwrap()];
    for (status, what) in [
        (
            "3 an input (or a --grammar or --alchemy",
            "program file) could not be read",
        ),
        (
            "5 an input (or a --grammar or --alchemy",
            "file) is over --max-size",
        ),
        (
            "6 a parse (or a --grammar compile) ran past --timeout",
            "the whole run",
        ),
    ] {
        assert!(statuses.contains(status), "{status:?} in:\n{statuses}");
        assert!(statuses.contains(what), "{what:?} in:\n{statuses}");
    }
    // Every option aless takes is listed, its other names among them, and
    // what else switches to JSON output.
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    for said in [
        "--format is another name for this option",
        "--watch Reload them (the default)",
        "--no-color, --no-colour No colours",
        "a standard output that is not a terminal, or TERM=dumb prints JSON",
        "(2.5, 90s, 2m; default none; 0 for no limit)",
        "--indent <N> Indent --json and --render json N spaces a level",
        "--compact JSON on one line, an error's too",
    ] {
        assert!(flat.contains(said), "{said:?} in:\n{text}");
    }
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
    // CSV declares what it does not keep, the inferred table's sentences
    // after its own, as every format's render does.
    let note = json(&out.stderr);
    assert_eq!(note["warning"]["kind"], "loss");
    assert_eq!(note["warning"]["render"], "csv");
    assert_eq!(note["warning"]["adapter"], "the inferred table");
    assert!(
        note["warning"]["loss"]
            .as_array()
            .is_some_and(|l| l.len() >= 6),
        "{note}"
    );
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

/// Every record whole when a stream failed is on standard output before
/// the error is reported, and none in part, on every path the output
/// takes: `--render csv` and `--render json`, a program's table and a
/// program's JSON events (aless#17). Nothing whole waits in a renderer's
/// buffer above the record pipe when the failure comes.
#[test]
fn whole_records_reach_standard_output_before_the_error() {
    let text = "{\"a\":1,\"b\":\"x\"}\n{\"a\":2,\"b\":\"y\"}\n{\"a\":\n";
    for (mode, expected) in record_modes() {
        let out = aless(mode, Some(text));
        assert_eq!(
            code(&out),
            1,
            "{mode:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&out.stdout), expected, "{mode:?}");
        let e = &json(&out.stderr)["error"];
        assert_eq!(e["kind"], json!("transduce"), "{mode:?}");
        assert_eq!(e["code"], json!("INPUT_INVALID"), "{mode:?}");
        assert_eq!(
            (e["line"].clone(), e["col"].clone()),
            (json!(3), json!(6)),
            "{mode:?}"
        );
        assert_eq!(e["output"], json!("partial"), "{mode:?}");
    }
    // A scalar record is whole where it ends, a string at its closing
    // quote: the second string is on the output though nothing followed
    // it. A bare number may go on, and is whole at the comma after it.
    let json_compact = ["-k", "jsonl", "--render", "json", "--compact"];
    let out = aless(&json_compact, Some("\"x\"\n\"y\"\n\"unterminated\n"));
    assert_eq!(code(&out), 1, "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(out.stdout, b"[\"x\",\"y\"");
    assert_eq!(json(&out.stderr)["error"]["output"], json!("partial"));
    let out = aless(&json_compact, Some("1\n2\n\"unterminated\n"));
    assert_eq!(code(&out), 1);
    assert_eq!(out.stdout, b"[1");
}

/// A writer that goes quiet part way, past `--timeout`: the records it
/// had written whole are on standard output before the `timeout` error,
/// with `output` partial, on every path (aless#17).
#[test]
fn a_slow_stdin_leaves_whole_records_at_the_timeout() {
    for (mode, expected) in record_modes() {
        let started = std::time::Instant::now();
        let mut child = Command::new(BIN)
            .args(mode)
            .args(["--timeout", "1"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("aless starts");
        // Two records, then the writer's end stays open and silent.
        let mut held = child.stdin.take().unwrap();
        held.write_all(b"{\"a\":1,\"b\":\"x\"}\n{\"a\":2,\"b\":\"y\"}\n")
            .unwrap();
        held.flush().unwrap();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if started.elapsed() > std::time::Duration::from_secs(30) {
                let _ = child.kill();
                panic!("{mode:?}: still waiting on standard input after 30 s");
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        };
        drop(held);
        let out = child.wait_with_output().unwrap();
        assert_eq!(
            status.code(),
            Some(6),
            "{mode:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&out.stdout), expected, "{mode:?}");
        let e = &json(&out.stderr)["error"];
        assert_eq!(e["kind"], json!("timeout"), "{mode:?}");
        assert_eq!(e["seconds"], json!(1.0), "{mode:?}");
        assert_eq!(e["output"], json!("partial"), "{mode:?}");
    }
}

/// The five ways two whole JSON Lines records reach standard output, and
/// what they are there: the plain renders, a program's table (its
/// columns inferred from the first row) and JSON events, and a composed
/// render.
fn record_modes() -> [(&'static [&'static str], &'static str); 5] {
    const TABLE: &str = "def export [input] (table-from-json \
        (record (entry :columns :infer) (entry :rows (path each-index))) input)";
    const CSV: &str = "\"a\",\"b\"\r\n\"1\",\"x\"\r\n\"2\",\"y\"\r\n";
    const JSON: &str = "[{\"a\":1,\"b\":\"x\"},{\"a\":2,\"b\":\"y\"}";
    const LINES: &str = "{\"a\":1,\"b\":\"x\"}\n{\"a\":2,\"b\":\"y\"}\n";
    [
        (&["-k", "jsonl", "--render", "csv"], CSV),
        (&["-k", "jsonl", "--render", "json", "--compact"], JSON),
        (
            &["-k", "jsonl", "--alchemy-expr", TABLE, "--render", "csv"],
            CSV,
        ),
        (
            &["-k", "jsonl", "--alchemy-expr", "def export [input] input"],
            JSON,
        ),
        // A composed render (another format's, run as a program).
        (&["-k", "jsonl", "--render", "jsonl"], LINES),
    ]
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

/// One of the two places `--render json` and `--json` part (the other is a
/// number's spelling, `render_json_keeps_a_numbers_source_spelling`): NaN
/// and the infinities, which JSON cannot hold, are `null` in `--json`'s
/// output, and a stream refuses each with `TARGET_VALUE_UNREPRESENTABLE`,
/// status 1, what came before it written.
#[test]
fn render_json_refuses_the_numbers_json_cannot_hold() {
    let out = aless(
        &["-k", "yaml", "--json", "--compact"],
        Some("a: .nan\nb: .inf\nc: -.inf\n"),
    );
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "{\"a\":null,\"b\":null,\"c\":null}\n"
    );
    for (value, said) in [(".nan", "NaN"), (".inf", "inf"), ("-.inf", "-inf")] {
        let out = aless(
            &["-k", "yaml", "--render", "json", "--compact"],
            Some(&format!("a: 1\nb: {value}\n")),
        );
        assert_eq!(code(&out), 1, "{value}");
        assert_eq!(out.stdout, b"{\"a\":1", "{value}");
        let e = &json(&out.stderr)["error"];
        assert_eq!(e["kind"], json!("transduce"), "{value}");
        assert_eq!(e["code"], json!("TARGET_VALUE_UNREPRESENTABLE"), "{value}");
        assert_eq!(
            e["message"],
            json!(format!("{said} has no representation as a number")),
            "{value}"
        );
        assert_eq!(e["output"], json!("partial"), "{value}");
    }
}

/// The other place they part: a stream writes a number as the source
/// spells it wherever that spelling is JSON, so `1.0`, `1e2` and `-0`
/// keep theirs and an integer beyond 2^53 keeps every digit, where
/// `--json` writes the 64-bit float. A spelling JSON lacks (YAML's
/// `0x1F`) is written as its value.
#[test]
fn render_json_keeps_a_numbers_source_spelling() {
    for (kind, text, streamed, whole) in [
        (
            "json",
            "{\"a\": 1.0, \"b\": 12345678901234567890, \"c\": 1e2, \"d\": -0}",
            "{\"a\":1.0,\"b\":12345678901234567890,\"c\":1e2,\"d\":-0}",
            "{\"a\":1,\"b\":1.2345678901234567e+19,\"c\":100,\"d\":0}\n",
        ),
        (
            "yaml",
            "a: 1.0\nb: 12345678901234567890\nc: 0x1F\n",
            "{\"a\":1.0,\"b\":12345678901234567890,\"c\":31}",
            "{\"a\":1,\"b\":1.2345678901234567e+19,\"c\":31}\n",
        ),
    ] {
        let out = aless(&["-k", kind, "--render", "json", "--compact"], Some(text));
        assert_eq!(
            code(&out),
            0,
            "{kind}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).trim_end(),
            streamed,
            "{kind}"
        );
        let out = aless(&["-k", kind, "--json", "--compact"], Some(text));
        assert_eq!(
            code(&out),
            0,
            "{kind}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&out.stdout), whole, "{kind}");
    }
}

/// A grammar that refuses to stream a document part-way (jsonic's implicit
/// list with a container first, a YAML stream of documents) is run again
/// from the whole value, and the answer is --json's.
#[test]
fn render_falls_back_when_a_grammar_refuses_to_stream() {
    for (kind, text) in [("jsonic", "{a:1}\n{b:2}\n"), ("yaml", "a: 1\n---\nb: 2\n")] {
        let streamed = aless(&["-k", kind, "--render", "json", "--compact"], Some(text));
        assert_eq!(
            code(&streamed),
            0,
            "{kind}: {}",
            String::from_utf8_lossy(&streamed.stderr)
        );
        assert!(streamed.stderr.is_empty(), "{kind}");
        let whole = aless(&["-k", kind, "--json", "--compact"], Some(text));
        assert_eq!(streamed.stdout, whole.stdout, "{kind}");
        let csv = aless(&["-k", kind, "--render", "csv"], Some(text));
        assert_eq!(
            code(&csv),
            0,
            "{kind}: {}",
            String::from_utf8_lossy(&csv.stderr)
        );
        assert!(
            String::from_utf8_lossy(&csv.stdout).starts_with("\"a\""),
            "{kind}: {}",
            String::from_utf8_lossy(&csv.stdout)
        );
    }
}

#[test]
fn render_failures_have_the_transduce_shape_and_status() {
    // The input did not parse: status 1, the transducer's code, the position.
    let out = aless(&["--render", "json", "tests/fixtures/bad.json"], None);
    assert_eq!(code(&out), 1);
    // The first member was whole when the failure came, so it is on the
    // output, and the output is partial (aless#17).
    assert_eq!(out.stdout, b"{\n  \"a\": 1");
    let e = &json(&out.stderr)["error"];
    assert_eq!(e["kind"], json!("transduce"));
    assert_eq!(e["code"], json!("INPUT_INVALID"));
    assert_eq!(e["file"], json!("tests/fixtures/bad.json"));
    assert_eq!(e["format"], json!("json"));
    assert!(e["line"].is_u64() && e["col"].is_u64(), "{e}");
    assert_eq!(e["output"], json!("partial"));
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
    // A usage error is its kind and its message, whatever the format;
    // the path in the message is jq's, counting from the end as given.
    assert_eq!(
        json(&out.stderr),
        json!({"error": {
            "kind": "usage",
            "message": "--render reads the input once, front to back, so it cannot count from \
                the end of an array: [-1] in .[-1] names an item of the array at .; use --json \
                --path, which reads the whole document"
        }})
    );
    for render in ["csv", "yaml"] {
        let other = aless(&["--render", render, "--path", "[-1]"], Some("[[1], [2]]"));
        assert_eq!(code(&other), 2, "{render}");
        assert_eq!(json(&other.stderr), json(&out.stderr), "{render}");
    }
    // A key on the path repeated after the first was taken: the grammars
    // and --json keep the last, which a stream cannot honour.
    let out = aless(
        &["--render", "csv", "--path", ".rows"],
        Some(r#"{"rows":[{"v":1}],"rows":[{"v":2}]}"#),
    );
    assert_eq!(code(&out), 1);
    // The first's row was whole when the refusal came: on the output.
    assert_eq!(out.stdout, b"\"v\"\r\n\"1\"\r\n");
    let e = &json(&out.stderr)["error"];
    assert_eq!(e["kind"], json!("transduce"));
    assert_eq!(e["code"], json!("DUPLICATE_MEMBER"));
    assert_eq!(e["path"], json!("."));
    assert_eq!(e["output"], json!("partial"));
    assert!(e["message"].as_str().unwrap().contains("\"rows\""), "{e}");
    // Mistakes in the command: status 2.
    for args in [
        &["--render", "csv", "--json", "tests/fixtures/nested.json"][..],
        &["--render", "csv", "--at", "3", "tests/fixtures/nested.json"],
        &["--render", "rss", "tests/fixtures/nested.json"],
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
    // --at with --render, word for word, whichever comes first.
    for args in [
        &["--render", "csv", "--at", "3", "tests/fixtures/nested.json"][..],
        &[
            "--at",
            "3:1",
            "--render",
            "yaml",
            "tests/fixtures/nested.json",
        ],
    ] {
        let out = aless(args, None);
        assert_eq!(code(&out), 2, "{args:?}");
        assert_eq!(
            json(&out.stderr),
            json!({"error": {
                "kind": "usage",
                "message": "--render reads the input once, front to back, and cannot find a \
                    source position in it: start it with --path, or use --where --at to find \
                    the path"
            }}),
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
    // Standard error holds CSV's loss declaration and nothing else.
    assert_eq!(json(&out.stderr)["warning"]["kind"], "loss");
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
            "{{\"file\":\"{sample}\",\"format\":\"hosts\",\"path\":\".[1].names[0]\",\"kind\":\"string\",\"line\":5,\"col\":17,\"value\":\"workstation.example.com\"}}\n"
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
    assert_eq!(paths, [".", ".[0]", ".[1]"]);
    assert_eq!(v["entries"][1]["line"], json!(2));
    // A position maps to a path: line 3, column 8 is inside `line`.
    let out = aless(&["--grammar", &kv, "--where", "--at", "3:8", file], None);
    let v = json(&out.stdout);
    assert_eq!(v["path"], json!(".[1].value"));
    assert_eq!(v["value"], json!("line"));
    assert_eq!((v["line"].clone(), v["col"].clone()), (json!(3), json!(8)));
    // A word of one punctuation character is a value, placed like any
    // word: the `*` password of `guest:*:1002:1002::/home/guest:`.
    let passwd = format!("passwd={GRAMMARS}/passwd.abnf");
    let sample = format!("{GRAMMARS}/passwd.sample");
    let out = aless(
        &[
            "--grammar",
            &passwd,
            "-k",
            "passwd",
            "--where",
            "--at",
            "15:7",
            "--compact",
            &sample,
        ],
        None,
    );
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    let v = json(&out.stdout);
    assert_eq!(v["path"], json!(".[12].password"));
    assert_eq!((v["line"].clone(), v["col"].clone()), (json!(15), json!(7)));
    assert_eq!(v["value"], json!("*"));
    // Inside a value assembled from several tokens (the gecos field), the
    // answer is the last placed node before the position: the field
    // before it.
    let out = aless(
        &[
            "--grammar",
            &passwd,
            "-k",
            "passwd",
            "--where",
            "--at",
            "13:22",
            "--compact",
            &sample,
        ],
        None,
    );
    assert_eq!(json(&out.stdout)["path"], json!(".[10].gid"));
    // Search, with the viewer's pattern.
    let out = aless(&["--grammar", &kv, "--find", "aless", file], None);
    let v = json(&out.stdout);
    assert_eq!(v["total"], json!(1));
    assert_eq!(v["matches"][0]["path"], json!(".[0].value"));
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

/// A repetition adds no rule depth, so a file of thousands of lines parses
/// under the shared cap, and `--check` and `--render` take it too. Nesting
/// is measured on the value the grammar builds, past about 1,000 levels.
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

/// When no thread can be started for the compiler, the compile runs on
/// aless's own thread, where it cannot be stopped, and is still held to
/// `--timeout`: once it ends, one that ended past the limit is refused as
/// a `timeout`, status 6, with a hint that says it ran to its end. What
/// makes the thread impossible here is an address-space limit below the
/// 64 MiB stack it asks for; the limit that lets aless start and compile
/// but not start that thread depends on the build, so it is found by
/// raising the limit 8 MiB at a time until aless answers at all.
#[cfg(target_os = "linux")]
#[test]
fn a_compile_with_no_thread_of_its_own_is_held_to_the_timeout() {
    use std::os::unix::process::CommandExt;
    let mut seen = Vec::new();
    for mib in (16..=512).step_by(8) {
        let bytes: libc::rlim_t = mib << 20;
        let mut command = Command::new(BIN);
        command
            .args([
                "--grammar-expr",
                "slow=doc = 1*300\"a\"\n",
                "--timeout",
                "0.001",
                "--json",
                "tests/fixtures/lines.txt",
            ])
            .stdin(Stdio::null());
        // SAFETY: the closure runs in the child between fork and exec, and
        // calls only setrlimit, which is async-signal-safe.
        unsafe {
            command.pre_exec(move || {
                let limit = libc::rlimit {
                    rlim_cur: bytes,
                    rlim_max: bytes,
                };
                match libc::setrlimit(libc::RLIMIT_AS, &limit) {
                    0 => Ok(()),
                    _ => Err(std::io::Error::last_os_error()),
                }
            });
        }
        let out = command.output().unwrap();
        if out.status.code() != Some(6) {
            seen.push((mib, out.status.code()));
            continue;
        }
        let e = &json(&out.stderr)["error"];
        assert_eq!(e["kind"], json!("timeout"), "{e}");
        assert_eq!(e["grammar"], json!("slow"));
        assert_eq!(e["seconds"], json!(0.001));
        assert_eq!(
            e["message"],
            json!("--grammar-expr slow: timeout: the grammar took longer than 0.001 s to compile")
        );
        // The first limit aless answers under is one it could not start
        // the compiler's thread under: the compile ran on its own thread,
        // to its end, and was refused for its time, not waited on.
        let hint = e["hint"].as_str().unwrap();
        assert!(
            hint.starts_with(
                "No thread could be started for the compiler, so it ran to its end on \
                 aless's own and took "
            ),
            "at {mib} MiB, after {seen:?}: {e}"
        );
        return;
    }
    panic!("aless never answered with a timeout: {seen:?}");
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
    // With the fields of any io error: the grammar file is the file, and
    // there is no input format.
    for key in ["format", "line", "col", "hint", "source_line"] {
        assert_eq!(e[key], Value::Null, "{key}: {e}");
    }
    assert!(
        e["report"].as_str().unwrap().starts_with("[aless/io]: "),
        "{e}"
    );
    // A grammar file over --max-size: too_large, status 5, with its size
    // and the limit.
    let hosts = format!("hosts={GRAMMARS}/hosts.abnf");
    let out = aless(
        &[
            "--max-size",
            "100",
            "--grammar",
            &hosts,
            "--json",
            "tests/fixtures/lines.txt",
        ],
        None,
    );
    assert_eq!(code(&out), 5);
    assert!(out.stdout.is_empty());
    let e = &json(&out.stderr)["error"];
    assert_eq!(e["kind"], json!("too_large"));
    assert_eq!(e["code"], json!("too_large"));
    assert_eq!(e["grammar"], json!("hosts"));
    assert_eq!(e["file"], json!(format!("{GRAMMARS}/hosts.abnf")));
    assert_eq!(e["format"], Value::Null);
    let len = std::fs::metadata(format!("{GRAMMARS}/hosts.abnf"))
        .unwrap()
        .len();
    assert_eq!(e["size"], json!(len));
    assert_eq!(e["limit"], json!(100));
    assert!(e["hint"].as_str().unwrap().contains("--max-size"), "{e}");
    // A compile past --timeout: timeout, status 6, with the seconds (300
    // nested optionals take the compiler well over a millisecond).
    let out = aless(
        &[
            "--grammar-expr",
            "slow=doc = 1*300\"a\"\n",
            "--timeout",
            "0.001",
            "--json",
            "tests/fixtures/lines.txt",
        ],
        None,
    );
    assert_eq!(code(&out), 6, "{}", String::from_utf8_lossy(&out.stderr));
    let e = &json(&out.stderr)["error"];
    assert_eq!(e["kind"], json!("timeout"));
    assert_eq!(e["code"], json!("timeout"));
    assert_eq!(e["grammar"], json!("slow"));
    assert_eq!(e["file"], Value::Null);
    assert_eq!(e["seconds"], json!(0.001));
    assert!(
        e["message"].as_str().unwrap().ends_with("to compile"),
        "{e}"
    );
    // A repetition count the compiler would write out by the million is
    // the command's mistake, refused before it starts.
    let out = aless(
        &[
            "--grammar-expr",
            "big=doc = 99999999999999999999999\"a\"\n",
            "--json",
            "tests/fixtures/lines.txt",
        ],
        None,
    );
    assert_eq!(code(&out), 2);
    let e = &json(&out.stderr)["error"];
    assert_eq!(e["kind"], json!("usage"));
    assert!(
        e["message"]
            .as_str()
            .unwrap()
            .contains("a repetition count of"),
        "{e}"
    );
    // Counts within the limit each still add up, where the compiler writes
    // them: `r`'s 599 rules again into each rule whose alternative starts
    // with it, 1,797 in all, past the 1,024 aless compiles. A chain of
    // seven such rules around `1*1024` kept the compiler busy for over a
    // minute and the parse after it for more than twelve. The timeout
    // turns a regression into a status 6 within ten seconds, not a held
    // suite.
    let out = aless(
        &[
            "--grammar-expr",
            "many=doc = s \"x\"\ns = r \"x\"\nr = 1*300\"q\"\n",
            "--timeout",
            "10",
            "--json",
            "tests/fixtures/lines.txt",
        ],
        None,
    );
    assert_eq!(code(&out), 2, "{}", String::from_utf8_lossy(&out.stderr));
    let e = &json(&out.stderr)["error"];
    assert_eq!(e["kind"], json!("usage"));
    assert_eq!(e["grammar"], json!("many"));
    assert!(
        e["message"]
            .as_str()
            .unwrap()
            .contains("the repetitions would have the compiler write 1797 rules"),
        "{e}"
    );
    // The compiler's message is one plain line: no colour codes, no line
    // breaks.
    let out = aless(
        &[
            "--grammar-expr",
            "g=doc = *\n",
            "--json",
            "tests/fixtures/lines.txt",
        ],
        None,
    );
    assert_eq!(code(&out), 2);
    let message = json(&out.stderr)["error"]["message"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        !message.contains('\u{1b}') && !message.contains('\n'),
        "{message:?}"
    );
    assert!(
        message.ends_with("[tabnas/unexpected]: unexpected character(s): *"),
        "{message}"
    );
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
    // A name registered twice is listed once.
    let kv = format!("hosts={GRAMMARS}/kv.abnf");
    let hosts = format!("hosts={GRAMMARS}/hosts.abnf");
    let out = aless(
        &[
            "--grammar",
            &kv,
            "--grammar",
            &hosts,
            "-k",
            "nope",
            "--json",
            "tests/fixtures/lines.txt",
        ],
        None,
    );
    assert_eq!(code(&out), 2);
    let message = json(&out.stderr)["error"]["message"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(message.ends_with(" text hosts)"), "{message}");
    // A name matches in any case, Unicode included.
    let out = aless(
        &[
            "--grammar-expr",
            "Ärger=doc = *word   ; @array\nword = ( TX )\n",
            "-k",
            "ärger",
            "--json",
            "--compact",
            "tests/fixtures/lines.txt",
        ],
        None,
    );
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    // The help names the options.
    let help = aless(&["--help"], None);
    let text = String::from_utf8_lossy(&help.stdout);
    assert!(
        text.contains("--grammar <NAME=FILE>") && text.contains("--grammar-expr"),
        "{text}"
    );
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
    // A different grammar at the lossy spelling: reading it would be a
    // silent wrong answer, not just a missing file.
    let lossy = dir.join("g\u{fffd}");
    std::fs::create_dir_all(&lossy).unwrap();
    std::fs::write(lossy.join("hosts.abnf"), "doc = *TX\n").unwrap();
    let sample = format!("{GRAMMARS}/hosts.sample");
    let expected: Value = json(&std::fs::read(format!("{GRAMMARS}/hosts.expected.json")).unwrap());
    let mut value = b"hosts=".to_vec();
    value.extend_from_slice(file.as_os_str().as_bytes());
    let mut attached = b"--grammar=".to_vec();
    attached.extend_from_slice(&value);
    for grammar in [
        vec![
            OsString::from("--grammar"),
            OsString::from_vec(value.clone()),
        ],
        vec![OsString::from_vec(attached)],
    ] {
        let out = Command::new(BIN)
            .args(&grammar)
            .args(["-k", "hosts", "--json", "--compact", &sample])
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(json(&out.stdout), expected, "{grammar:?}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

// ----- programs (--alchemy) --------------------------------------------------

const PROGRAMS: &str = "tests/fixtures/programs";

/// The worked example of the transducer's design: the program binds a
/// table by the document's own metadata and renders the spec's CSV, byte
/// for byte; the same table as JSON records with `--render json`.
#[test]
fn alchemy_runs_the_worked_example_byte_for_byte() {
    let program = format!("{PROGRAMS}/export.alc");
    let out = aless(
        &["--alchemy", &program, "tests/fixtures/records.json"],
        None,
    );
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    assert!(out.stderr.is_empty());
    assert_eq!(
        out.stdout,
        b"\"Identifier\",\"Full name\",\"Balance\"\r\n\"123\",\"Alice\",\"50.25\"\r\n\"456\",\"Bob\",\"72\"\r\n"
    );
    assert_eq!(out.stdout.len(), 77);
    // The table alone, rendered by aless: CSV by default, JSON records on
    // request, keyed by the column labels.
    let table = format!("{PROGRAMS}/table.alc");
    let csv = aless(&["--alchemy", &table, "tests/fixtures/records.json"], None);
    assert_eq!(csv.stdout, out.stdout);
    let records = aless(
        &[
            "--alchemy",
            &table,
            "--render",
            "json",
            "tests/fixtures/records.json",
        ],
        None,
    );
    assert_eq!(
        code(&records),
        0,
        "{}",
        String::from_utf8_lossy(&records.stderr)
    );
    assert_eq!(
        json(&records.stdout),
        json!([
            {"Identifier": 123, "Full name": "Alice", "Balance": 50.25},
            {"Identifier": 456, "Full name": "Bob", "Balance": 72}
        ])
    );
    // A program on the command line, and the option's `=` form.
    let out = aless(
        &[
            "--alchemy-expr=def export [input] (json input)",
            "--compact",
            "tests/fixtures/records.json",
        ],
        None,
    );
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("{\"response\":{\"metadata\""));
}

/// A program that passes the document through gives `--json`'s value,
/// for every fixture and through every path the input can take: parsed
/// whole, streamed as the parse proceeds, or read a record at a time.
#[test]
fn alchemy_echo_agrees_with_json_for_every_fixture() {
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
        for program in [
            "def export [input] input",
            "def export [input] (json input)",
        ] {
            let streamed = aless(&["--alchemy-expr", program, file], None);
            assert_eq!(
                code(&streamed),
                0,
                "{file} {program}: {}",
                String::from_utf8_lossy(&streamed.stderr)
            );
            let whole = aless(&["--json", "--compact", file], None);
            assert_eq!(
                normal(json(&streamed.stdout)),
                normal(json(&whole.stdout)),
                "{file} {program}"
            );
            assert!(streamed.stdout.ends_with(b"\n"), "{file}");
        }
    }
    // Standard input, a record at a time, with -k saying the format; and
    // a table of the rows from CSV.
    let out = aless(
        &["-k", "jsonl", "--alchemy-expr", "def export [input] input"],
        Some("{\"n\": 1}\n{\"n\": 2.50}\n"),
    );
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "[{\"n\":1},{\"n\":2.50}]\n"
    );
    let out = aless(
        &[
            "--alchemy-expr",
            "def export [input] input",
            "tests/fixtures/sample.csv",
        ],
        None,
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "[{\"name\":\"ada\",\"age\":\"36\",\"city\":\"london\"},{\"name\":\"lin\",\"age\":\"28\",\"city\":\"helsinki\"}]\n"
    );
    // A grammar that refuses to stream a document part-way is run again
    // from the whole value.
    let out = aless(
        &["-k", "jsonic", "--alchemy-expr", "def export [input] input"],
        Some("{a:1}\n{b:2}\n"),
    );
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "[{\"a\":1},{\"b\":2}]\n"
    );
}

/// `--explain` prints the plan report as one JSON object, and reads no
/// input.
#[test]
fn alchemy_explain_prints_the_plan_as_json() {
    let program = format!("{PROGRAMS}/export.alc");
    let out = aless(&["--alchemy", &program, "--explain"], None);
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    assert!(out.stderr.is_empty());
    let v = json(&out.stdout);
    assert_eq!(v["entry"], json!("export"));
    assert_eq!(v["output"], json!("Text"));
    assert_eq!(
        v["protocol"],
        json!(["JsonEvents/1", "TableRows/1", "Text"])
    );
    assert_eq!(v["chain"], json!(["api-table", "csv"]));
    assert!(v["retention"].is_array(), "{v}");
    assert!(String::from_utf8_lossy(&out.stdout).contains("\n  \"entry\""));
    let compact = aless(&["--alchemy", &program, "--explain", "--compact"], None);
    assert_eq!(json(&compact.stdout), v);
    assert_eq!(String::from_utf8_lossy(&compact.stdout).lines().count(), 1);
    // No FILE goes with it.
    let out = aless(
        &[
            "--alchemy",
            &program,
            "--explain",
            "tests/fixtures/records.json",
        ],
        None,
    );
    assert_eq!(code(&out), 2);
    assert_eq!(json(&out.stderr)["error"]["kind"], json!("usage"));
    // --render is checked against the program before the report, so one
    // the program refuses is the same usage error with --explain as
    // without it.
    let out = aless(
        &["--alchemy", &program, "--render", "json", "--explain"],
        None,
    );
    assert_eq!(code(&out), 2, "{}", String::from_utf8_lossy(&out.stderr));
    assert!(out.stdout.is_empty());
    let e = &json(&out.stderr)["error"];
    assert_eq!(e["kind"], json!("usage"));
    assert!(
        e["message"]
            .as_str()
            .unwrap()
            .contains("renders its own text"),
        "{e}"
    );
    // The report is the program's: its `renderer` is the program's own
    // default, whatever --render names.
    let table = format!("{PROGRAMS}/table.alc");
    let plain = aless(&["--alchemy", &table, "--explain"], None);
    assert_eq!(
        code(&plain),
        0,
        "{}",
        String::from_utf8_lossy(&plain.stderr)
    );
    let out = aless(
        &["--alchemy", &table, "--render", "json", "--explain"],
        None,
    );
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    let v = json(&out.stdout);
    assert_eq!(v["renderer"]["name"], json!("csv"), "{v}");
    assert_eq!(v, json(&plain.stdout));
}

/// Each way the command line can ask for what a program cannot do.
#[test]
fn alchemy_usage_errors_say_what_to_give_instead() {
    let program = format!("{PROGRAMS}/export.alc");
    let records = "tests/fixtures/records.json";
    let cases: [(&[&str], &str); 11] = [
        (
            &["--alchemy", &program, "--alchemy-expr", "x", records],
            "both name the program",
        ),
        // The one option twice is that option given twice, not a clash
        // between the two.
        (
            &["--alchemy", &program, "--alchemy", &program, records],
            "--alchemy was given twice",
        ),
        (
            &["--alchemy-expr", "x", "--alchemy-expr", "y", records],
            "--alchemy-expr was given twice",
        ),
        (
            &["--alchemy", &program, "--json", records],
            "--json both say what to print",
        ),
        (
            &["--alchemy", &program, "--check", records],
            "--check both say what to print",
        ),
        (
            &["--alchemy", &program, "--path", ".a", records],
            "no --path or --at",
        ),
        (
            &["--alchemy", &program, "--at", "1:1", records],
            "no --path or --at",
        ),
        (&["--explain", records], "give the program with --alchemy"),
        (
            &["--alchemy", &program, "--render", "json", records],
            "renders its own text",
        ),
        (
            &["--alchemy", &program, "-k", "text", records],
            "plain text, which has no values",
        ),
        (
            &["--alchemy", &program, records, "tests/fixtures/nested.json"],
            "reads one input",
        ),
    ];
    for (args, expected) in cases {
        let out = aless(args, None);
        assert_eq!(code(&out), 2, "{args:?}");
        assert!(out.stdout.is_empty(), "{args:?}");
        let e = &json(&out.stderr)["error"];
        assert_eq!(e["kind"], json!("usage"), "{args:?}: {e}");
        let message = e["message"].as_str().unwrap();
        assert!(message.contains(expected), "{args:?}: {message}");
        if expected.contains("twice") {
            assert!(!message.contains("both"), "{args:?}: {message}");
        }
    }
}

/// The program's own failures and the input's, each in its shape and
/// with its status.
#[test]
fn alchemy_failures_have_their_shapes_and_statuses() {
    let records = "tests/fixtures/records.json";
    // The program does not parse: status 2, the language's code, the
    // finer code leading the message, the position in the program.
    let out = aless(
        &[
            "--alchemy-expr",
            "def export [input]\n  (json input",
            records,
        ],
        None,
    );
    assert_eq!(code(&out), 2);
    assert!(out.stdout.is_empty());
    let e = &json(&out.stderr)["error"];
    assert_eq!(e["kind"], json!("alchemy"));
    assert_eq!(e["file"], json!("--alchemy-expr"));
    assert_eq!(e["format"], json!(null));
    assert_eq!(e["code"], json!("DSL_PARSE_ERROR"));
    assert!(
        e["message"].as_str().unwrap().starts_with("unbalanced: "),
        "{e}"
    );
    assert_eq!((e["line"].clone(), e["col"].clone()), (json!(2), json!(14)));
    assert_eq!(e["output"], json!("none"));
    assert!(e.get("row").is_none());
    // Nor type checks; and the program's file is named.
    let dir = std::env::temp_dir().join(format!("aless-alchemy-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let bad = dir.join("bad.alc");
    std::fs::write(&bad, "def export [input] (nope input)\n").unwrap();
    let out = aless(&["--alchemy", bad.to_str().unwrap(), records], None);
    assert_eq!(code(&out), 2);
    let e = &json(&out.stderr)["error"];
    assert_eq!(e["kind"], json!("alchemy"));
    assert_eq!(e["file"], json!(bad.to_str().unwrap()));
    assert_eq!(e["code"], json!("DSL_TYPE_ERROR"));
    assert!(
        e["message"]
            .as_str()
            .unwrap()
            .starts_with("unknown_name: nope"),
        "{e}"
    );
    assert_eq!((e["line"].clone(), e["col"].clone()), (json!(1), json!(21)));
    // The rows before the metadata: the input's failure, in the
    // transducer's shape, before anything was written.
    let text = std::fs::read_to_string(records).unwrap();
    let mut v: Value = serde_json::from_str(&text).unwrap();
    let meta = v["response"]
        .as_object_mut()
        .unwrap()
        .remove("metadata")
        .unwrap();
    v["response"]["metadata"] = meta;
    let rows_first = dir.join("rows-first.json");
    std::fs::write(&rows_first, serde_json::to_string(&v).unwrap()).unwrap();
    let program = format!("{PROGRAMS}/export.alc");
    let out = aless(&["--alchemy", &program, rows_first.to_str().unwrap()], None);
    assert_eq!(code(&out), 1);
    assert!(out.stdout.is_empty());
    let e = &json(&out.stderr)["error"];
    assert_eq!(e["kind"], json!("transduce"));
    assert_eq!(e["code"], json!("INPUT_ORDER_VIOLATION"));
    assert_eq!(e["file"], json!(rows_first.to_str().unwrap()));
    assert_eq!(e["format"], json!("json"));
    assert_eq!(e["path"], json!(".response.payload.deep.records[0]"));
    assert_eq!(e["output"], json!("none"));
    // The input does not parse: the transducer's code and position.
    let out = aless(
        &["--alchemy-expr", "def export [input] input"],
        Some("[{\"a\": 1},\n {\"b\": }]"),
    );
    assert_eq!(code(&out), 1);
    let e = &json(&out.stderr)["error"];
    assert_eq!(e["kind"], json!("transduce"));
    assert_eq!(e["code"], json!("INPUT_INVALID"));
    assert_eq!((e["line"].clone(), e["col"].clone()), (json!(2), json!(8)));
    // A failure the program raises at a record (`fail`) keeps the
    // transducer's code and status 1, but is placed in the program: `file`
    // is the program's, `line` and `col` are in it (the `(fail` form),
    // `format` is null and `input` names the document, since the events a
    // program reads carry no positions of the input's.
    let failing = dir.join("failing.alc");
    std::fs::write(
        &failing,
        "def check [r]\n  match (get \"id\" r)\n    case 456 (fail \"no Bob\")\n    case _ (get \"name\" (get \"person\" r))\n\
         def export [input]\n  join \"\\n\"\n    map check\n      select (path \"response\" \"payload\" \"deep\" \"records\" each-index) input\n",
    )
    .unwrap();
    let out = aless(&["--alchemy", failing.to_str().unwrap(), records], None);
    assert_eq!(code(&out), 1, "{}", String::from_utf8_lossy(&out.stderr));
    // The first item was whole when the failure came, so it is on the
    // output, and the output is partial (aless#17).
    assert_eq!(out.stdout, b"Alice");
    let e = &json(&out.stderr)["error"];
    assert_eq!(e["kind"], json!("transduce"));
    assert_eq!(e["code"], json!("INPUT_INVALID"));
    assert_eq!(e["message"], json!("no Bob"));
    assert_eq!(e["file"], json!(failing.to_str().unwrap()));
    assert_eq!(e["format"], json!(null));
    assert_eq!((e["line"].clone(), e["col"].clone()), (json!(3), json!(14)));
    assert_eq!(e["input"], json!(records));
    assert_eq!(e["output"], json!("partial"));
    // The same rule by code: a failure from the program's sink with one
    // of the language's codes (the evaluator's `recursion`, met on an
    // item) is the program's, status 2, placed in the program, with
    // `input`.
    let out = aless(
        &[
            "--alchemy-expr",
            "def w [f] (f f)\ndef export [input]\n  concat-map (fn [row] (let [x (w w)] \".\")) \
             (select (path each-index) input)",
        ],
        Some("[1, 2]"),
    );
    assert_eq!(code(&out), 2, "{}", String::from_utf8_lossy(&out.stderr));
    assert!(out.stdout.is_empty());
    let e = &json(&out.stderr)["error"];
    assert_eq!(e["kind"], json!("alchemy"));
    assert_eq!(e["code"], json!("STREAMABILITY_UNKNOWN"));
    assert!(
        e["message"].as_str().unwrap().starts_with("recursion: "),
        "{e}"
    );
    assert_eq!(e["file"], json!("--alchemy-expr"));
    assert_eq!(e["format"], json!(null));
    assert_eq!((e["line"].clone(), e["col"].clone()), (json!(1), json!(12)));
    assert_eq!(e["input"], json!("-"));
    // A failure from the program's sink with neither a language code nor
    // a position, a renderer's over the rows the program built (a record
    // without a value for a bound column, rendered as CSV): the input's,
    // in an export's shape, `file` and `format` the document's and no
    // `input`, `line` or `col`.
    let mut v: Value = serde_json::from_str(&text).unwrap();
    v["response"]["payload"]["deep"]["records"][1]["person"]
        .as_object_mut()
        .unwrap()
        .remove("name");
    let missing = dir.join("missing.json");
    std::fs::write(&missing, serde_json::to_string(&v).unwrap()).unwrap();
    let table = format!("{PROGRAMS}/table.alc");
    let out = aless(&["--alchemy", &table, missing.to_str().unwrap()], None);
    assert_eq!(code(&out), 1, "{}", String::from_utf8_lossy(&out.stderr));
    // The header and the first row were whole when the failure came, so
    // they are on the output, and the output is partial.
    assert_eq!(
        out.stdout,
        b"\"Identifier\",\"Full name\",\"Balance\"\r\n\"123\",\"Alice\",\"50.25\"\r\n"
    );
    let e = &json(&out.stderr)["error"];
    assert_eq!(e["kind"], json!("transduce"));
    assert_eq!(e["code"], json!("MISSING_VALUE"));
    assert_eq!(e["file"], json!(missing.to_str().unwrap()));
    assert_eq!(e["format"], json!("json"));
    assert!(e.get("input").is_none(), "{e}");
    assert!(e.get("line").is_none() && e.get("col").is_none(), "{e}");
    assert_eq!(e["output"], json!("partial"));
    // Rendered as JSON, the same rows have no missing value to refuse.
    let out = aless(
        &[
            "--alchemy",
            &table,
            "--render",
            "json",
            missing.to_str().unwrap(),
        ],
        None,
    );
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    // A program file that cannot be read: io, status 3, no format.
    let out = aless(&["--alchemy", "/nonexistent/p.alc", records], None);
    assert_eq!(code(&out), 3);
    let e = &json(&out.stderr)["error"];
    assert_eq!(e["kind"], json!("io"));
    assert_eq!(e["file"], json!("/nonexistent/p.alc"));
    assert_eq!(e["format"], json!(null));
    // One over --max-size: too_large, status 5, with its size and the
    // limit; the message names the program, and the hint does not claim
    // the parser's memory per byte, since no parse of it is coming.
    let out = aless(&["--alchemy", &program, "--max-size", "10", records], None);
    assert_eq!(code(&out), 5);
    let e = &json(&out.stderr)["error"];
    assert_eq!(e["kind"], json!("too_large"));
    assert_eq!(e["code"], json!("too_large"));
    assert_eq!(e["file"], json!(program));
    assert_eq!(e["format"], json!(null));
    assert_eq!(e["limit"], json!(10));
    assert!(e["size"].as_u64().unwrap() > 10, "{e}");
    let size = e["size"].as_u64().unwrap();
    assert_eq!(
        e["message"],
        json!(format!("program is {size} B, over the 10 B limit"))
    );
    let hint = e["hint"].as_str().unwrap();
    assert!(!hint.contains("bytes of memory"), "{hint}");
    assert!(hint.contains("--max-size"), "{hint}");
    // A transducer limit met while the plan is built (forty nested
    // doublings ask for 2^40 values before the program reads anything):
    // the program's, as a transduce error with status 5, `file` the
    // program's, `format` null, and no `input`, since none was opened.
    let mut doubling = String::from("def d0 [x] (vector x x)\n");
    for n in 1..=40 {
        doubling.push_str(&format!("def d{n} [x] (d{} (d{} x))\n", n - 1, n - 1));
    }
    doubling.push_str("def export [input]\n  let [v (d40 1)]\n    json input\n");
    let steps = dir.join("steps.alc");
    std::fs::write(&steps, &doubling).unwrap();
    let out = aless(&["--alchemy", steps.to_str().unwrap(), records], None);
    assert_eq!(code(&out), 5, "{}", String::from_utf8_lossy(&out.stderr));
    assert!(out.stdout.is_empty());
    let e = &json(&out.stderr)["error"];
    assert_eq!(e["kind"], json!("transduce"));
    assert_eq!(e["code"], json!("RESOURCE_LIMIT_EXCEEDED"));
    assert_eq!(e["file"], json!(steps.to_str().unwrap()));
    assert_eq!(e["format"], json!(null));
    assert_eq!(e["limit"]["name"], json!("max_plan_steps"));
    assert!(e.get("input").is_none(), "{e}");
    assert_eq!(e["output"], json!("none"));
    // A run past --timeout: status 6, output none, as under --render.
    let big = dir.join("big.json");
    let mut text = String::from("[");
    for i in 0..60_000 {
        if i > 0 {
            text.push(',');
        }
        text.push_str(&format!("{{\"i\":{i},\"s\":\"xxxxxxxxxxxxxxxxxxxxxxxx\"}}"));
    }
    text.push(']');
    std::fs::write(&big, text).unwrap();
    let out = aless(
        &[
            "--alchemy-expr",
            "def export [input] input",
            "--timeout",
            "0.001",
            big.to_str().unwrap(),
        ],
        None,
    );
    assert_eq!(code(&out), 6, "{}", String::from_utf8_lossy(&out.stderr));
    let e = &json(&out.stderr)["error"];
    assert_eq!(e["kind"], json!("timeout"));
    assert_eq!(e["seconds"], json!(0.001));
    assert!(e.get("output").is_some(), "{e}");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A program slow on one item works inside one of the parser's events,
/// where no guard of aless's can stop it: `--timeout` must reach the
/// program's own flag, in the incremental mode JSON runs in, or the
/// command runs as long as the item's work (seconds here, for one row).
/// It ends a moment after the deadline instead, status 6, with a
/// `timeout` worded for the run, the parse and the program, and `output`,
/// and with no input position: `line` and `col` null, the report's `-->`
/// naming the file alone, since the program's own span (its evaluator
/// stamps one on the failure) is no place in the input and the parse was
/// not what stopped. The deadline is one the parse meets many times
/// over (a few milliseconds for three hundred numbers) and the item's
/// cubic work does not (seconds), so it is the program's flag that ends
/// the run, on a slow runner too.
#[test]
fn a_program_slow_on_one_item_is_stopped_at_the_timeout() {
    use std::time::{Duration, Instant};
    // Cubic work per row: every element mapped over every element, for
    // every element. Three hundred numbers parse in a few milliseconds and
    // take seconds of work, so the deadline lands in the program.
    let slow =
        "def slow [row]\n  let [v (as-vector row)]\n    let [w (map (fn [x] (map (fn [y] (map (fn [z] z) v)) \
                v)) v)]\n      \".\"\ndef export [input]\n  concat-map slow (select (path \
                each-index) input)\n";
    let dir = std::env::temp_dir().join(format!("aless-slow-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let doc = dir.join("row.json");
    let row: Vec<String> = (0..300).map(|i| i.to_string()).collect();
    std::fs::write(&doc, format!("[[{}]]", row.join(","))).unwrap();
    let started = Instant::now();
    let out = aless(
        &[
            "--alchemy-expr",
            slow,
            "--timeout",
            "1.5",
            doc.to_str().unwrap(),
        ],
        None,
    );
    let elapsed = started.elapsed();
    assert_eq!(code(&out), 6, "{}", String::from_utf8_lossy(&out.stderr));
    assert!(
        elapsed < Duration::from_millis(4_500),
        "the command ran {elapsed:?} against a 1.5 s deadline"
    );
    assert!(out.stdout.is_empty());
    let e = &json(&out.stderr)["error"];
    assert_eq!(e["kind"], json!("timeout"));
    assert_eq!(e["code"], json!("timeout"));
    assert_eq!(e["seconds"], json!(1.5));
    assert_eq!(e["output"], json!("none"));
    assert_eq!(e["file"], json!(doc.to_str().unwrap()));
    assert_eq!(
        e["message"],
        json!("timeout: the run (the parse and the program) ran longer than 1.5 s")
    );
    let hint = e["hint"].as_str().unwrap();
    assert!(hint.contains("the program's work on an item"), "{e}");
    assert!(!hint.contains("got this far"), "{e}");
    // Stopped in the program's work on the item, not in the parse: no
    // position in the input, and none in the report.
    assert_eq!(e["line"], json!(null), "{e}");
    assert_eq!(e["col"], json!(null), "{e}");
    let report = e["report"].as_str().unwrap();
    let origin = format!("--> {}", doc.to_str().unwrap());
    assert!(
        report.contains(&format!("{origin}\n")) && !report.contains(&format!("{origin}:")),
        "{report}"
    );
    // The same program over a small row finishes within the same limit.
    let small = dir.join("small.json");
    std::fs::write(&small, "[[1, 2, 3]]").unwrap();
    let out = aless(
        &[
            "--alchemy-expr",
            slow,
            "--timeout",
            "30",
            small.to_str().unwrap(),
        ],
        None,
    );
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(out.stdout, b".");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// `--render yaml` writes the value at the start as YAML, through
/// tabnas-yaml's own render: CSV's records as a sequence of mappings,
/// every string and key double-quoted, and the render's loss declaration
/// on standard error as a JSON warning, the answer alone on standard
/// output.
#[test]
fn render_yaml_writes_csv_as_a_sequence_of_mappings() {
    let out = aless(&["--render", "yaml", "tests/fixtures/sample.csv"], None);
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "- \"name\": \"ada\"\n  \"age\": \"36\"\n  \"city\": \"london\"\n\
         - \"name\": \"lin\"\n  \"age\": \"28\"\n  \"city\": \"helsinki\"\n"
    );
    let note = json(&out.stderr);
    assert_eq!(note["warning"]["kind"], "loss");
    assert_eq!(note["warning"]["render"], "yaml");
    assert_eq!(note["warning"]["file"], "tests/fixtures/sample.csv");
    let loss = note["warning"]["loss"].as_array().unwrap();
    assert!(
        loss.iter().any(|l| l == "Comments are not kept."),
        "{loss:?}"
    );
    // From a path, as `--render json` starts there.
    let out = aless(
        &[
            "--render",
            "yaml",
            "--path",
            ".store.books[1]",
            "tests/fixtures/nested.json",
        ],
        None,
    );
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "\"title\": \"TAPL\"\n\"price\": 55\n\"tags\":\n  - \"types\"\n"
    );
    // A number is its lexeme, a jsonic spelling that is no JSON number
    // arrives as its value, and YAML's non-finite numbers keep YAML's
    // spellings.
    let out = aless(
        &["-k", "jsonic", "--render", "yaml"],
        Some("a: 0xFF\nb: 1_000\nc: 1.50\n"),
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "\"a\": 255\n\"b\": 1000\n\"c\": 1.50\n"
    );
    let out = aless(
        &["-k", "yaml", "--render", "yaml"],
        Some("- .inf\n- -.inf\n- .nan\n- '.inf'\n- {}\n- []\n"),
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "- .inf\n- -.inf\n- .nan\n- \".inf\"\n- {}\n- []\n"
    );
}

/// Every fixture can be written as YAML: a document ending in a newline,
/// with the loss declaration beside it. Whether each reads back as its
/// value is `tests/yaml_render.rs`'s round trip.
#[test]
fn every_fixture_renders_as_yaml() {
    let files: Vec<String> = fixture_files()
        .into_iter()
        .filter(|f| !f.ends_with("bad.json") && !f.ends_with("lines.txt"))
        .collect();
    assert!(files.len() >= 16, "{files:?}");
    for file in &files {
        let out = aless(&["--render", "yaml", "--compact", file], None);
        assert_eq!(
            code(&out),
            0,
            "{file}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            out.stdout.len() > 1 && out.stdout.ends_with(b"\n"),
            "{file}"
        );
        assert_eq!(json(&out.stderr)["warning"]["kind"], "loss", "{file}");
    }
}

/// A render takes a tree's events, each key once per mapping. A member the
/// parse streams twice (JSON's `{"a":1,"a":2}`, whose value keeps the last)
/// is refused before the render writes it, and the run falls back to the
/// parsed value, as `--json` reads it, when nothing was written yet; once
/// output has left, the refusal is the answer, the output partial. A
/// stream no tree has (the member's value before its key, which the
/// incremental YAML source streams for a key that is a mapping) falls back
/// the same way.
#[test]
fn render_yaml_holds_a_stream_to_a_trees_events() {
    let out = aless(&["--render", "yaml"], Some("{\"a\":1,\"a\":2}"));
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "\"a\": 2\n");
    let mut late = String::from("[");
    for i in 0..5000 {
        late.push_str(&format!("{{\"k0\":{i},\"k1\":{i},\"k2\":{i}}},"));
    }
    late.push_str("{\"a\":1,\"a\":2}]");
    let out = aless(&["--render", "yaml", "--compact"], Some(&late));
    assert_eq!(code(&out), 1, "{}", String::from_utf8_lossy(&out.stderr));
    let error = &json(&out.stderr)["error"];
    assert_eq!(error["kind"], "transduce");
    assert_eq!(error["code"], "DUPLICATE_MEMBER");
    assert_eq!(error["output"], "partial");
    assert!(
        error["loss"].as_array().is_some_and(|l| !l.is_empty()),
        "{error}"
    );
    let out = aless(
        &["-k", "yaml", "--render", "yaml"],
        Some("- sun: yellow\n- ? earth: blue\n  : moon: white\n"),
    );
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "- \"sun\": \"yellow\"\n- \"earth: blue\":\n    \"moon\": \"white\"\n"
    );
}

/// `--render` says what it writes: a format aless reads and has no render
/// for, a name it does not know, a format's own render after a program,
/// and plain text are each refused as usage, before anything is written.
#[test]
fn render_refuses_what_it_cannot_write() {
    for (args, message) in [
        (
            vec!["--render", "rss", "tests/fixtures/sample.rss"],
            "--render rss: aless reads feed but has no render for it; --render writes csv, ini, \
             json, json5, jsonc, jsonic, jsonl, markdown, toml, xml, yaml or zon",
        ),
        (
            vec!["--render", "docx", "tests/fixtures/nested.json"],
            "--render writes csv, ini, json, json5, jsonc, jsonic, jsonl, markdown, toml, xml, \
             yaml or zon, not docx",
        ),
        (
            vec![
                "--alchemy",
                "tests/fixtures/programs/export.alc",
                "--render",
                "yaml",
                "tests/fixtures/records.json",
            ],
            "--render yaml was given, but the program renders its own text \
             (tests/fixtures/programs/export.alc): drop --render, or export a table or JSON \
             events for aless to render",
        ),
        (
            vec!["--render", "yaml", "tests/fixtures/lines.txt"],
            "tests/fixtures/lines.txt is plain text, which has no values to write as yaml: name \
             its format with -k, such as -k jsonl or -k csv",
        ),
    ] {
        let out = aless(&args, None);
        assert_eq!(code(&out), 2, "{args:?}");
        assert!(out.stdout.is_empty(), "{args:?}");
        let error = &json(&out.stderr)["error"];
        assert_eq!(error["kind"], "usage", "{args:?}");
        assert_eq!(error["message"], message, "{args:?}");
    }
}

/// Every format with a render writes its own sample and reads it back as
/// the same value, with the loss declaration beside it: the translation
/// is a round trip through each format's always-quoted profile.
#[test]
fn every_format_with_a_render_writes_its_own_sample_and_reads_it_back() {
    for (id, file) in [
        ("toml", "tests/fixtures/sample.toml"),
        ("ini", "tests/fixtures/sample.ini"),
        ("zon", "tests/fixtures/sample.zon"),
        ("xml", "tests/fixtures/sample.xml"),
        ("jsonl", "tests/fixtures/sample.jsonl"),
        ("yaml", "tests/fixtures/sample.yaml"),
        ("json5", "tests/fixtures/sample.json5"),
        ("jsonc", "tests/fixtures/sample.jsonc"),
        ("jsonic", "tests/fixtures/sample.jsonic"),
        ("json", "tests/fixtures/sample.json"),
        ("csv", "tests/fixtures/sample.csv"),
    ] {
        let written = aless(&["--render", id, file], None);
        assert_eq!(
            code(&written),
            0,
            "{id}: {}",
            String::from_utf8_lossy(&written.stderr)
        );
        assert!(written.stdout.ends_with(b"\n"), "{id}");
        if id == "json" {
            assert!(written.stderr.is_empty(), "json declares no loss");
        } else {
            let note = json(&written.stderr);
            assert_eq!(note["warning"]["kind"], "loss", "{id}");
            assert_eq!(note["warning"]["render"], id, "{id}");
            assert_eq!(note["warning"]["file"], file, "{id}");
        }
        let text = String::from_utf8(written.stdout).unwrap();
        let back = aless(&["-k", id, "--json", "--compact", "-"], Some(&text));
        assert_eq!(
            code(&back),
            0,
            "{id}: {}",
            String::from_utf8_lossy(&back.stderr)
        );
        let original = aless(&["--json", "--compact", file], None);
        assert_eq!(
            String::from_utf8_lossy(&back.stdout),
            String::from_utf8_lossy(&original.stdout),
            "{id}: {text}"
        );
    }
}

/// A tree reaches a render that writes from records through the inferred
/// table, behind the row check `--render csv` has; a Markdown table's rows
/// reach a render that writes from a tree, or CSV, through its lift; and a
/// tree reaches a render of another tree format as it is.
#[test]
fn render_composes_the_adapters_the_shapes_need() {
    // Records to a Markdown table: the inferred table, its loss declared.
    let out = aless(
        &[
            "--render",
            "markdown",
            "--path",
            ".response.payload.deep.records",
            "tests/fixtures/records.json",
        ],
        None,
    );
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "| id | person | account |\n| --- | --- | --- |\n| 123 | {\"name\":\"Alice\"} | \
         {\"balance\":50.25} |\n| 456 | {\"name\":\"Bob\"} | {\"balance\":72} |\n"
    );
    let note = json(&out.stderr);
    assert_eq!(note["warning"]["adapter"], "the inferred table");
    // A root that is not an array is the row check's refusal, as csv's.
    let out = aless(
        &["--render", "markdown", "tests/fixtures/sample.json"],
        None,
    );
    assert_eq!(code(&out), 1);
    let error = &json(&out.stderr)["error"];
    assert_eq!(error["code"], "INPUT_INVALID");
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("not an array of records"),
        "{error}"
    );
    // A Markdown table lifts to records: as CSV through its lift, as a
    // table again, and as JSON its tree as it is.
    let table = "| Name | Age |\n| --- | ---: |\n| Ada | 36 |\n| Bob *b* | 42 |\n";
    let out = aless(&["-k", "markdown", "--render", "csv"], Some(table));
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "\"Name\",\"Age\"\r\n\"Ada\",\"36\"\r\n\"Bob b\",\"42\"\r\n"
    );
    assert!(
        json(&out.stderr)["warning"]["adapter"].is_null(),
        "no adapter: the lift"
    );
    let out = aless(&["-k", "markdown", "--render", "markdown"], Some(table));
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "| Name | Age |\n| --- | --- |\n| Ada | 36 |\n| Bob b | 42 |\n"
    );
    let out = aless(
        &["-k", "markdown", "--render", "json", "--compact"],
        Some(table),
    );
    assert_eq!(code(&out), 0);
    assert!(
        String::from_utf8_lossy(&out.stdout).starts_with("{\"type\":\"document\""),
        "a tree format takes the Markdown tree as it is"
    );
    // Below the root the lift does not run: --path selects a value of the
    // document's tree, rows by their own shape, through the inferred
    // table as csv reads a tree.
    let out = aless(
        &["-k", "markdown", "--render", "csv", "--path", ".children"],
        Some(table),
    );
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    assert!(
        String::from_utf8_lossy(&out.stdout)
            .starts_with("\"type\",\"align\",\"children\"\r\n\"table\","),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert_eq!(
        json(&out.stderr)["warning"]["adapter"],
        "the inferred table"
    );
    let out = aless(
        &[
            "-k",
            "markdown",
            "--render",
            "json",
            "--compact",
            "--path",
            ".children[0].children[1]",
        ],
        Some(table),
    );
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    assert!(
        String::from_utf8_lossy(&out.stdout).starts_with("{\"type\":\"tableRow\""),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    // A Markdown document with no table has no records: the lift says so.
    let out = aless(&["--render", "csv", "tests/fixtures/sample.md"], None);
    assert_eq!(code(&out), 1);
    let error = &json(&out.stderr)["error"];
    assert_eq!(error["code"], "INPUT_INVALID");
    assert_eq!(error["kind"], "transduce");
    // A format's typed refusal of what it cannot carry.
    let out = aless(&["--render", "toml", "tests/fixtures/sample.csv"], None);
    assert_eq!(code(&out), 1);
    let error = &json(&out.stderr)["error"];
    assert_eq!(error["code"], "INPUT_INVALID");
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .starts_with("TOML's root is a table"),
        "{error}"
    );
    assert_eq!(error["output"], "none");
    assert!(error["loss"].as_array().is_some(), "{error}");
}

/// A program's output takes any format with a render: JSON events reach a
/// tree's render as they are, so the identity program writes what
/// `--render yaml` writes, byte for byte; a table reaches it through
/// `records`, as a sequence of mappings, and a records render as it is.
#[test]
fn a_programs_output_is_written_as_any_format() {
    let composed = aless(
        &[
            "--alchemy-expr",
            "def export [input] input",
            "--render",
            "yaml",
            "tests/fixtures/nested.json",
        ],
        None,
    );
    assert_eq!(
        code(&composed),
        0,
        "{}",
        String::from_utf8_lossy(&composed.stderr)
    );
    let direct = aless(&["--render", "yaml", "tests/fixtures/nested.json"], None);
    assert_eq!(composed.stdout, direct.stdout);
    assert_eq!(json(&composed.stderr)["warning"]["render"], "yaml");
    let out = aless(
        &[
            "--alchemy",
            "tests/fixtures/programs/table.alc",
            "--render",
            "yaml",
            "tests/fixtures/records.json",
        ],
        None,
    );
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "- \"Identifier\": 123\n  \"Full name\": \"Alice\"\n  \"Balance\": 50.25\n\
         - \"Identifier\": 456\n  \"Full name\": \"Bob\"\n  \"Balance\": 72\n"
    );
    assert_eq!(json(&out.stderr)["warning"]["adapter"], "records");
    let out = aless(
        &[
            "--alchemy",
            "tests/fixtures/programs/table.alc",
            "--render",
            "markdown",
            "tests/fixtures/records.json",
        ],
        None,
    );
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "| Identifier | Full name | Balance |\n| --- | --- | --- |\n| 123 | Alice | 50.25 |\n\
         | 456 | Bob | 72 |\n"
    );
    // A format written by aless's own renderer: the renderer, and the
    // format's loss declaration.
    let out = aless(
        &[
            "--alchemy",
            "tests/fixtures/programs/table.alc",
            "--render",
            "json5",
            "tests/fixtures/records.json",
        ],
        None,
    );
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("[{\"Identifier\":123,"));
    assert_eq!(json(&out.stderr)["warning"]["render"], "json5");
    // A table written as JSON is aless's own renderer over the rows, which
    // writes them as `records` does: the adapter is named, with its loss,
    // where JSON's own declaration has none.
    let out = aless(
        &[
            "--alchemy",
            "tests/fixtures/programs/table.alc",
            "--render",
            "json",
            "tests/fixtures/records.json",
        ],
        None,
    );
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("[{\"Identifier\":123,"));
    let note = json(&out.stderr);
    assert_eq!(note["warning"]["render"], "json");
    assert_eq!(note["warning"]["adapter"], "records");
    assert_eq!(note["warning"]["loss"].as_array().map(Vec::len), Some(1));
    // Events written as CSV go through the inferred table, composed: the
    // identity program writes what --render csv writes, byte for byte.
    let composed = aless(
        &[
            "--alchemy-expr",
            "def export [input] input",
            "--render",
            "csv",
            "tests/fixtures/records.yaml",
        ],
        None,
    );
    assert_eq!(
        code(&composed),
        0,
        "{}",
        String::from_utf8_lossy(&composed.stderr)
    );
    let direct = aless(&["--render", "csv", "tests/fixtures/records.yaml"], None);
    assert_eq!(composed.stdout, direct.stdout);
    assert_eq!(
        json(&composed.stderr)["warning"]["adapter"],
        "the inferred table"
    );
    // A program's events reach a records render under the inferred
    // table's row policy, as a source's do: a scalar row is one cell named
    // value, and a mixture of rows, an array row and a root that is not an
    // array are refused with the reason before anything is written.
    let identity = [
        "--alchemy-expr",
        "def export [input] input",
        "--render",
        "markdown",
    ];
    let scalars = aless(&identity, Some("[1,2]"));
    assert_eq!(
        code(&scalars),
        0,
        "{}",
        String::from_utf8_lossy(&scalars.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&scalars.stdout),
        "| value |\n| --- |\n| 1 |\n| 2 |\n"
    );
    let direct = aless(&["--render", "markdown"], Some("[1,2]"));
    assert_eq!(scalars.stdout, direct.stdout);
    // Another format's render writes as it goes: a row refused after the
    // first was written leaves the table's head on the output, and the
    // output is partial (aless#17); a document refused at its first row
    // leaves nothing.
    for (input, reason, written) in [
        (
            "[{\"a\":1},2]",
            "but the first row was an object",
            "| a |\n| --- |\n| 1 |\n",
        ),
        (
            "[1,{\"a\":1}]",
            "but the first row was a scalar",
            "| value |\n| --- |\n| 1 |\n",
        ),
        ("[[1],[2]]", "is an array; a row is an object", ""),
        ("{\"a\":1}", "is an object, not an array of records", ""),
    ] {
        let out = aless(&identity, Some(input));
        assert_eq!(
            code(&out),
            1,
            "{input}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&out.stdout), written, "{input}");
        let error = &json(&out.stderr)["error"];
        assert_eq!(error["code"], "INPUT_INVALID", "{input}: {error}");
        assert!(
            error["message"].as_str().unwrap().contains(reason),
            "{input}: {error}"
        );
        let output = if written.is_empty() {
            "none"
        } else {
            "partial"
        };
        assert_eq!(error["output"], output, "{input}");
    }
}

/// A YAML array of objects written as CSV through the inferred table (a
/// program's `table-from-json` with `:columns :infer`) is the bytes
/// `--render csv` writes natively.
#[test]
fn yaml_records_through_the_inferred_table_are_the_native_csv() {
    let native = aless(&["--render", "csv", "tests/fixtures/records.yaml"], None);
    assert_eq!(
        code(&native),
        0,
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&native.stdout),
        "\"name\",\"age\",\"active\",\"city\"\r\n\"ada\",\"36\",\"true\",\"london\"\r\n\
         \"lin\",\"28.5\",\"false\",\"helsinki, uusimaa\"\r\n\"quote \"\" mark\",\"41\",\"true\",\"\"\r\n"
    );
    let program = "def rows (record (entry :columns :infer) (entry :rows (path each-index)))\n\
                   def export [input] (table-from-json rows input)";
    let inferred = aless(
        &["--alchemy-expr", program, "tests/fixtures/records.yaml"],
        None,
    );
    assert_eq!(
        code(&inferred),
        0,
        "{}",
        String::from_utf8_lossy(&inferred.stderr)
    );
    assert_eq!(inferred.stdout, native.stdout);
}

/// `--panes` opens the viewer, so without a terminal it refuses, writing
/// nothing on standard output, and with an option that prints instead it
/// says the two do not go together. `--render` and the program choose the
/// output pane's content with it, so they are no refusal.
#[test]
fn panes_open_the_viewer_or_refuse() {
    for (args, message) in [
        (
            vec!["--panes", "out", "tests/fixtures/nested.json"],
            "--panes opens the viewer, which needs a terminal: without one, give --render or \
             --alchemy without --panes",
        ),
        (
            vec![
                "--panes",
                "out",
                "--render",
                "yaml",
                "tests/fixtures/sample.csv",
            ],
            "--panes opens the viewer, which needs a terminal: without one, give --render or \
             --alchemy without --panes",
        ),
        (
            vec!["--panes", "out", "--json", "tests/fixtures/nested.json"],
            "--panes opens the viewer, and --json is for output without one: give one",
        ),
        (
            vec!["--panes", "tree", "tests/fixtures/nested.json"],
            "--panes names out and program, not tree",
        ),
        (
            vec!["--panes", "program", "tests/fixtures/nested.json"],
            "--panes program shows a program: give it with --alchemy FILE or --alchemy-expr TEXT",
        ),
        (
            vec!["--panes", " , ", "tests/fixtures/nested.json"],
            "--panes names the panes to open beside the input: out, program, or out,program",
        ),
        (
            // The input alone is still the viewer.
            vec!["--panes", "in", "tests/fixtures/nested.json"],
            "--panes opens the viewer, which needs a terminal: without one, give --render or \
             --alchemy without --panes",
        ),
    ] {
        let out = aless(&args, None);
        assert_eq!(code(&out), 2, "{args:?}");
        assert!(out.stdout.is_empty(), "{args:?} wrote to standard output");
        let err = json(&out.stderr);
        assert_eq!(err["error"]["kind"], "usage", "{args:?}");
        assert_eq!(err["error"]["message"], message, "{args:?}");
    }
}
