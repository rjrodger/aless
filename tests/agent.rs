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
        &["--paths"],
        &["-k", "jsonl", "--where"],
        &["-k", "csv", "--find", "a"],
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

/// A document of one of the newer formats nested past what its reader
/// takes is the same `too_deep` parse error, whatever is asked of it,
/// never an abort. A `.proto` file is refused before
/// it is parsed when its braces nest past the 100 levels tabnas-proto's
/// preflight counts, since a value built past them can abort the process
/// as it is dropped; CSS stops at its grammar's 768 open rules, a feed at
/// XML's 256 open elements, an expression at 127 operations, and PGN's
/// variations, which have no limit of their own, at aless's cap. C stops at
/// its reader's limits, tabnas-expr's 127 operations in an expression (a
/// long sum, a long chain of member accesses) and 1,024 open rules (nested
/// blocks, a run of prefix operators), or tabnas-c's tree of 256 levels (a
/// conditional that is a whole initializer, which it builds holding
/// itself); and a chain within those limits whose tree, which the reader
/// repeats under several members, doubles with each level written out
/// stops at aless's bound on the tree written out, before it is, however
/// long a comment beside it makes the file. A
/// grammar stops at its own compiler's limits: groups and options nested
/// past 2,048 open rules for ABNF, 520 for EBNF and 512 for GBNF, some
/// 500, 130 and 128 levels, and elements nested past 128 levels in any
/// of them.
#[test]
fn the_newer_formats_nested_too_deep_fail_cleanly() {
    let n = 20_000;
    let dir = std::env::temp_dir().join(format!("aless-deeper-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, kind, text) in [
        (
            "deep.proto",
            "proto",
            format!(
                "syntax = \"proto3\";\n{}{}\n",
                "message M { ".repeat(n),
                "}".repeat(n)
            ),
        ),
        ("deep.css", "css", "a{".repeat(n) + &"}".repeat(n)),
        (
            "deep.rss",
            "feed",
            format!(
                "<rss version=\"2.0\"><channel>{}{}</channel></rss>\n",
                "<a>".repeat(n),
                "</a>".repeat(n)
            ),
        ),
        (
            "deep.expr",
            "expr",
            format!("{}1{}\n", "(".repeat(n), ")".repeat(n)),
        ),
        ("long.expr", "expr", vec!["1"; n].join("+") + "\n"),
        (
            "deep.pgn",
            "pgn",
            format!("1. e4 {}{} *\n", "(1. d4 ".repeat(n), ")".repeat(n)),
        ),
        (
            "deep.c",
            "c",
            format!("void f(void) {{ {}{} }}\n", "{".repeat(n), "}".repeat(n)),
        ),
        (
            "long.c",
            "c",
            format!("int x = {};\n", vec!["a"; n].join(" + ")),
        ),
        (
            "chain.c",
            "c",
            format!("int f(void) {{ return a{}; }}\n", ".b".repeat(n)),
        ),
        (
            "not.c",
            "c",
            format!("int f(void) {{ return {}a; }}\n", "!".repeat(n)),
        ),
        (
            "doubling.c",
            "c",
            format!("int f(void) {{ return a{}; }}\n", ".b".repeat(100)),
        ),
        ("ternary.c", "c", "int a = b ? c : d;\n".to_string()),
        (
            "padded.c",
            "c",
            format!(
                "/*{}*/\nint f(void) {{ return a{}; }}\n",
                " ".repeat(4 << 20),
                ".b".repeat(21)
            ),
        ),
        (
            "groups.abnf",
            "abnf",
            format!("a = {}\"x\"{}\n", "(".repeat(n), ")".repeat(n)),
        ),
        (
            "options.abnf",
            "abnf",
            format!("a = {}\"x\"{}\n", "[".repeat(n), "]".repeat(n)),
        ),
        (
            "elements.abnf",
            "abnf",
            format!("a = {}\"x\"{}\n", "(".repeat(200), ")".repeat(200)),
        ),
        (
            "groups.ebnf",
            "ebnf",
            format!("a ::= {}\"x\"{}\n", "(".repeat(n), ")".repeat(n)),
        ),
        (
            "options.ebnf",
            "ebnf",
            format!("a ::= {}\"x\"{}\n", "(".repeat(n), ")?".repeat(n)),
        ),
        (
            "groups.gbnf",
            "gbnf",
            format!("root ::= {}\"x\"{}\n", "(".repeat(n), ")".repeat(n)),
        ),
        (
            "options.gbnf",
            "gbnf",
            format!("root ::= {}\"x\"{}\n", "(".repeat(n), ")?".repeat(n)),
        ),
    ] {
        let deep = dir.join(name);
        std::fs::write(&deep, text).unwrap();
        let file = deep.to_str().unwrap();
        for mode in [
            vec!["-k", kind, "--json", file],
            vec!["-k", kind, "--paths", file],
        ] {
            let out = aless(&mode, None);
            let stderr = String::from_utf8_lossy(&out.stderr);
            assert_eq!(code(&out), 1, "{name} {mode:?}: {stderr}");
            assert!(out.stdout.is_empty(), "{name} {mode:?}");
            let e = &json(&out.stderr)["error"];
            assert_eq!(e["kind"], json!("parse"), "{name} {mode:?}: {e}");
            assert_eq!(e["code"], json!("too_deep"), "{name} {mode:?}: {e}");
            assert_eq!(e["format"], json!(kind), "{name} {mode:?}: {e}");
        }
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A cycle of `k` rules, each with two alternatives led by the next and
/// one of its own, in `notation`: the shape whose alternatives a
/// compiler's left-recursion pass multiplies, 2^k of them.
fn left_recursive_cycle(notation: &str, k: usize) -> String {
    let (define, or) = if notation == "abnf" {
        (" = ", " / ")
    } else {
        (" ::= ", " | ")
    };
    let mut text = String::new();
    if notation == "gbnf" {
        text.push_str("root ::= r0\n");
    }
    for i in 0..k {
        let next = (i + 1) % k;
        text.push_str(&format!(
            "r{i}{define}r{next} \"a{i}\"{or}r{next} \"b{i}\"{or}\"x{i}\"\n"
        ));
    }
    text
}

/// Twenty rules in such a cycle, 0.7 KB, once took the compilers past a
/// minute and then out of memory: tabnas-bnf now refuses them before it
/// multiplies anything, past 512 alternatives added to one rule. A chain
/// of 5,000 rules each led by the next, whose inlining would copy a
/// quadratic count of elements, it refuses at its bound on the elements
/// the pass adds to a grammar, 131,072. Each is a clean refusal at once,
/// placed at the rule, in every route: read, listed, checked, and as a
/// grammar from the command line.
#[test]
fn a_grammar_whose_alternatives_multiply_is_refused_at_once() {
    let dir = std::env::temp_dir().join(format!("aless-cycle-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let quick = |started: std::time::Instant, what: &str| {
        assert!(
            started.elapsed() < std::time::Duration::from_secs(30),
            "{what}: refused at once"
        );
    };
    let alternatives = "rule 'r11' exceeds the left-recursion expansion limit of 512 \
                        alternatives while inlining 'r12', which begins 2 of its alternatives \
                        and has 511 of its own. Factor 'r12' out of the alternatives it begins.";
    for (notation, line) in [("abnf", 12), ("ebnf", 12), ("gbnf", 13)] {
        let path = dir.join(format!("cycle.{notation}"));
        std::fs::write(&path, left_recursive_cycle(notation, 20)).unwrap();
        let file = path.to_str().unwrap();
        for mode in [vec!["--json", file], vec!["--paths", file]] {
            let started = std::time::Instant::now();
            let out = aless(&mode, None);
            let what = format!("{notation} {mode:?}");
            quick(started, &what);
            let stderr = String::from_utf8_lossy(&out.stderr);
            assert_eq!(code(&out), 1, "{what}: {stderr}");
            assert!(out.stdout.is_empty(), "{what}");
            let e = &json(&out.stderr)["error"];
            assert_eq!(e["kind"], "parse", "{what}: {e}");
            assert_eq!(e["code"], "compile", "{what}: {e}");
            assert_eq!(e["format"], notation, "{what}: {e}");
            assert_eq!(e["message"], format!("compile: {alternatives}"), "{what}");
            assert_eq!((&e["line"], &e["col"]), (&json!(line), &json!(1)), "{what}");
        }
        let out = aless(&["--check", file], None);
        assert_eq!(code(&out), 1, "{}", String::from_utf8_lossy(&out.stderr));
        let report = json(&out.stdout);
        assert_eq!(report["files"][0]["error"]["code"], "compile", "{report}");
    }
    // As a grammar from the command line, before any input is read.
    let cycle = format!("cycle={}", dir.join("cycle.abnf").to_str().unwrap());
    let started = std::time::Instant::now();
    let out = aless(
        &["--grammar", &cycle, "--json", "tests/fixtures/sample.json"],
        None,
    );
    quick(started, "--grammar");
    assert_eq!(code(&out), 2, "{}", String::from_utf8_lossy(&out.stderr));
    assert!(out.stdout.is_empty());
    let e = &json(&out.stderr)["error"];
    assert_eq!(e["kind"], "usage");
    assert_eq!(
        e["message"],
        format!("--grammar cycle: abnf: {alternatives}")
    );
    // A chain of 5,000 rules, each led by the next, as a document and as a
    // grammar from the command line.
    let chain: String = (0..4_999)
        .map(|i| format!("r{i} = r{} \"x{i}\"\n", i + 1))
        .chain(std::iter::once("r4999 = \"end\"\n".to_string()))
        .collect();
    let path = dir.join("chain.abnf");
    std::fs::write(&path, chain).unwrap();
    let file = path.to_str().unwrap();
    let elements = "rule 'r4486' exceeds the left-recursion element limit of 131072 elements \
                    while inlining 'r4487', which begins 1 of its alternatives and would add \
                    more than 256 elements to the 130816 already added to the grammar. Factor \
                    'r4487' out of the alternatives it begins.";
    let started = std::time::Instant::now();
    let out = aless(&["--json", file], None);
    quick(started, "the chain");
    assert_eq!(code(&out), 1, "{}", String::from_utf8_lossy(&out.stderr));
    let e = &json(&out.stderr)["error"];
    assert_eq!(
        (&e["kind"], &e["code"]),
        (&json!("parse"), &json!("compile")),
        "{e}"
    );
    assert_eq!(e["message"], format!("compile: {elements}"));
    assert_eq!(e["line"], 4487);
    let grammar = format!("chain={file}");
    let started = std::time::Instant::now();
    let out = aless(
        &[
            "--grammar",
            &grammar,
            "--json",
            "tests/fixtures/sample.json",
        ],
        None,
    );
    quick(started, "the chain as --grammar");
    assert_eq!(code(&out), 2, "{}", String::from_utf8_lossy(&out.stderr));
    let e = &json(&out.stderr)["error"];
    assert_eq!(e["kind"], "usage");
    assert_eq!(e["message"], format!("--grammar chain: abnf: {elements}"));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn help_leads_with_the_agent_interface() {
    let out = aless(&["--help"], None);
    assert_eq!(code(&out), 0);
    let text = String::from_utf8_lossy(&out.stdout);
    let head: String = text.lines().take(30).collect::<Vec<_>>().join("\n");
    for flag in ["--json", "--paths", "--find", "--where", "--check"] {
        assert!(head.contains(flag), "{flag} in the first lines:\n{head}");
    }
    assert!(text.find("WITHOUT A SCREEN") < text.find("THE VIEWER"));
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    // A section of the reference: from its title to the next one's.
    let section = |title: &str, next: &str| -> String {
        let from = flat
            .find(&format!("{title}: "))
            .unwrap_or_else(|| panic!("{title}"));
        let to = flat[from..]
            .find(&format!("{next}: "))
            .unwrap_or_else(|| panic!("{next} after {title}"));
        flat[from..from + to].to_string()
    };
    // The exit statuses say what --grammar adds to each, as the README's
    // and the skill's tables do: a grammar file that cannot be read is
    // status 3, one over --max-size status 5, and a compile past the
    // deadline status 6.
    let statuses = section("EXIT STATUS", "LARGE INPUTS AND LIMITS");
    for said in [
        "3 an input or a --grammar file could not be read",
        "5 an input or a --grammar file is over --max-size",
        "6 a parse, or a --grammar compile, ran past --timeout",
        "4 --path or --at names nothing (not_found)",
    ] {
        assert!(statuses.contains(said), "{said:?} in:\n{statuses}");
    }
    // Every error kind, with the fields it carries.
    let errors = section("ERRORS", "EXIT STATUS");
    for kind in ["parse", "io", "too_large", "timeout", "not_found", "usage"] {
        assert!(
            errors.contains(&format!(" {kind} ")),
            "{kind} in:\n{errors}"
        );
    }
    for field in ["source_line", "report", "nearest", "keys", "seconds"] {
        assert!(errors.contains(field), "{field} in:\n{errors}");
    }
    // What each output option prints.
    let output = section("OUTPUT", "ERRORS");
    for shape in [
        "{file, format, path, entries: [entry, ...], total, limit, truncated}",
        "{file, format, path, pattern, matches: [entry, ...], total, limit, truncated}",
        "{ok, files: [{file, format, ok, error}]}",
    ] {
        assert!(output.contains(shape), "{shape:?} in:\n{output}");
    }
    // Every option aless takes is listed, its other names among them, and
    // what else switches to JSON output.
    for said in [
        "-k, --kind, --format <FORMAT>",
        "--format is another name for this option",
        "--watch Reload a file when it changes, keeping your place (the default)",
        "--no-color, --no-colour Draw without colours",
        "when standard output is not a terminal (a pipe, a file, an agent's tool call), or when TERM=dumb",
        "(2.5, 90s, 2m; default none; 0 for no limit)",
        "--indent <N> Indent --json N spaces a level",
        "--compact JSON on one line",
        "--generate <WHAT>",
        "NO_COLOR set and not empty",
    ] {
        assert!(flat.contains(said), "{said:?} in:\n{text}");
    }
}

/// The man page and the completions in the repository are what the binary
/// writes, byte for byte: a release archive carries the committed files.
#[test]
fn the_generated_files_are_current() {
    for (what, path) in [
        ("man", "man/aless.1"),
        ("complete-bash", "completions/aless.bash"),
        ("complete-zsh", "completions/_aless"),
        ("complete-fish", "completions/aless.fish"),
        ("complete-powershell", "completions/_aless.ps1"),
    ] {
        let out = aless(&["--generate", what], None);
        assert_eq!(code(&out), 0, "{what}");
        let committed = std::fs::read(path).unwrap_or_default();
        assert!(
            out.stdout == committed,
            "{path} is not what `aless --generate {what}` writes: run scripts/generate.sh"
        );
    }
    // The skill is the repository's own file, as it is.
    let out = aless(&["--generate", "skill"], None);
    assert_eq!(out.stdout, std::fs::read("skills/aless/SKILL.md").unwrap());
}

/// `-h` is the summary: an option a line, and where the rest is.
#[test]
fn short_help_is_a_summary_of_the_options() {
    let out = aless(&["-h"], None);
    assert_eq!(code(&out), 0);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.lines().count() < 80, "{text}");
    for said in [
        "--json ",
        "-k, --kind, --format <FORMAT> ",
        "--no-color, --no-colour ",
        "aless --help prints the whole reference",
    ] {
        assert!(text.contains(said), "{said:?} in:\n{text}");
    }
    assert!(!text.contains("EXIT STATUS"), "{text}");
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
/// and checks.
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
    // Several names for one grammar, and several grammars at once.
    let sample = format!("{GRAMMARS}/impl-words");
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
/// under the shared cap, and `--check` takes it too. Nesting is measured
/// on the value the grammar builds, past about 1,000 levels.
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
/// grammar then reads the extension.
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
