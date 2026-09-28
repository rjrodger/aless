//! YAML's render against YAML's own reader: the round trip the translation
//! design asks of it (tabnas/transduce `docs/translation.md`, step 3).
//! Every YAML input the pinned tabnas-yaml checkout's fixtures hold, read,
//! written through the render as `--render yaml` writes it, and read back,
//! is the same value. It lives here rather than in tabnas/yaml because that
//! repository does not depend on alchemy, and aless has both crates. The
//! fixtures are the checkout cargo pinned, found through `cargo metadata`,
//! so they move with the pin: the parity rows of `test/spec/*.tsv` that
//! parse, and every case of the vendored YAML Test Suite that is not an
//! error case.
//!
//! The inputs a reader defect misreads are a checked ledger,
//! [`READER_DEFECT`]: each must still come back different, so a fix to the
//! reader fails this test until its lines are deleted, and every input not
//! in it must come back the same. The render's output for each is valid
//! YAML 1.2; it is YAML's reader that misreads it.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use aless::export::{Input, Job, What};
use aless::load::Format;
use aless::translate;
use serde_json::Value;
use tabnas_transduce::Metrics;

/// The inputs whose output YAML's reader misreads, each with the issue
/// that records the defect, named as this test names an input:
/// `spec/<file>.tsv:<line>`, or the YAML Test Suite case's id.
///
/// - tabnas/yaml#86: a quoted key at the start of a line after a block
///   sequence is read into the sequence (`"a":\n  - 1\n"b": 2` reads as
///   `{"a":[1,{"b":2}]}`), or the parse fails.
/// - tabnas/yaml#88: a flow sequence first in an indented block sequence
///   replaces it (`-\n  - []` reads as `[[]]`), or the parse fails.
const READER_DEFECT: &[(&str, u32)] = &[
    ("spec/anchors-aliases.tsv:4", 86),
    ("spec/block-sequences.tsv:10", 86),
    ("spec/flow-collections.tsv:24", 86),
    ("spec/indentation.tsv:7", 86),
    ("spec/line-endings.tsv:9", 86),
    ("spec/real-world.tsv:3", 86),
    ("spec/suite-basic.tsv:17", 86),
    ("spec/suite-realworld.tsv:3", 86),
    ("spec/suite-structure.tsv:16", 86),
    ("57H4", 86),
    ("7BUB", 86),
    ("7ZZ5", 88),
    ("AZ63", 86),
    ("DC7X", 86),
    ("J9HZ", 86),
    ("PBJ2", 86),
    ("R52L", 86),
    ("RLU9", 86),
    ("S9E8", 86),
    ("UDR7", 86),
    ("UGM3", 86),
];

/// A writer into a shared buffer.
#[derive(Clone, Default)]
struct Shared(Arc<Mutex<Vec<u8>>>);

impl Write for Shared {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The root of the tabnas-yaml checkout this build uses: the repository
/// above its crate, as `cargo metadata` names the crate's manifest.
fn yaml_checkout() -> PathBuf {
    let out = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1", "--offline"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo metadata runs");
    assert!(
        out.status.success(),
        "cargo metadata: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let metadata: Value = serde_json::from_slice(&out.stdout).expect("cargo metadata is JSON");
    let manifest = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "tabnas-yaml")
        .and_then(|p| p["manifest_path"].as_str())
        .expect("tabnas-yaml is a dependency");
    Path::new(manifest)
        .parent()
        .and_then(Path::parent)
        .expect("the crate is rs/ in its repository")
        .to_path_buf()
}

/// A fixture cell's escapes decoded, as the fixture runners decode them:
/// `\n`, `\r`, `\t` and `\\`, and any other backslash kept.
fn unescape(cell: &str) -> String {
    let mut out = String::with_capacity(cell.len());
    let mut chars = cell.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Every input the checkout's fixtures hold that is meant to parse, named.
fn inputs(root: &Path) -> Vec<(String, String)> {
    let mut inputs = Vec::new();
    let mut tsvs: Vec<PathBuf> = std::fs::read_dir(root.join("test/spec"))
        .expect("the checkout has test/spec")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "tsv"))
        .collect();
    tsvs.sort();
    for tsv in tsvs {
        let file = tsv.file_name().unwrap().to_string_lossy().into_owned();
        let text = std::fs::read_to_string(&tsv).unwrap();
        let mut header = true;
        for (i, line) in text.lines().enumerate() {
            if line.is_empty() || (line.starts_with('#') && !line.contains('\t')) {
                continue;
            }
            if header {
                header = false;
                continue;
            }
            let mut cells = line.split('\t');
            let input = cells.next().unwrap_or("");
            let expected = cells.next().unwrap_or("");
            if expected.starts_with("ERROR") {
                continue;
            }
            inputs.push((format!("spec/{file}:{}", i + 1), unescape(input)));
        }
    }
    // The suite's cases as tabnas-yaml's own runner gathers them: each
    // directory holding an `in.yaml`, or its numbered sub-tests (`AB12/00`),
    // once each; the `name` and `tags` indexes link to the same cases.
    let suite = root.join("test/yaml-test-suite");
    let mut cases = Vec::new();
    for entry in std::fs::read_dir(&suite).expect("the checkout vendors the suite") {
        let dir = entry.unwrap().path();
        if !dir.is_dir() {
            continue;
        }
        if dir.join("in.yaml").is_file() {
            cases.push(dir);
            continue;
        }
        for sub in std::fs::read_dir(&dir).unwrap() {
            let sub = sub.unwrap().path();
            let numbered = sub
                .file_name()
                .is_some_and(|n| n.to_string_lossy().chars().all(|c| c.is_ascii_digit()));
            if numbered && sub.join("in.yaml").is_file() {
                cases.push(sub);
            }
        }
    }
    cases.sort();
    for case in cases {
        if case.join("error").exists() {
            continue;
        }
        let name = case
            .strip_prefix(&suite)
            .unwrap()
            .display()
            .to_string()
            .replace('\\', "/");
        let text = std::fs::read_to_string(case.join("in.yaml")).unwrap();
        inputs.push((name, text));
    }
    inputs
}

/// A value as text to compare: JSON, with the non-finite numbers the reader
/// builds from `.inf` and `.nan` spelled as they are, which JSON cannot. A
/// stream with no document has no value, which aless shows, and the render
/// writes, as null (tabnas/yaml's DIVERGENCE.md: the Rust port keeps the
/// undefined result for an empty source alone).
fn read(text: &str) -> Option<String> {
    tabnas_yaml::parse(text).ok().map(|v| match v {
        tabnas::Value::Undefined => "null".to_string(),
        v => v.to_string(),
    })
}

/// `text` written as `--render yaml` writes it.
fn render(program: &tabnas_alchemy::Program, text: &str) -> Result<String, String> {
    let job = Job {
        name: "fixture.yaml".into(),
        origin: "fixture.yaml".into(),
        format: Format::Yaml,
        what: What::Part,
        path: Vec::new(),
        compact: false,
        indent: 2,
        timeout: None,
    };
    let out = Shared::default();
    translate::run(
        &job,
        program,
        Input::Text(text),
        Box::new(out.clone()),
        Metrics::new(),
    )
    .map_err(|e| format!("{e:?}"))?;
    let bytes = out.0.lock().unwrap().clone();
    Ok(String::from_utf8(bytes).expect("the render writes UTF-8"))
}

#[test]
fn every_yaml_fixture_written_as_yaml_reads_back_as_the_same_value() {
    let root = yaml_checkout();
    let yaml = translate::part("yaml").expect("tabnas-yaml names its render");
    let inputs = inputs(&root);
    let total = inputs.len();
    assert!(total > 600, "only {total} inputs under {}", root.display());
    // The inputs are shared out over a few threads, each with the render
    // compiled once, so the run takes seconds in a debug build.
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get().clamp(2, 8));
    let chunks: Vec<&[(String, String)]> = inputs.chunks(total.div_ceil(threads)).collect();
    let results: Vec<(usize, usize, Vec<String>)> = std::thread::scope(|scope| {
        let handles: Vec<_> = chunks
            .iter()
            .map(|chunk| {
                scope.spawn(move || {
                    let program = translate::compose(yaml).unwrap_or_else(|f| panic!("{f}"));
                    let (mut same, mut unread) = (0, 0);
                    let mut differ: Vec<String> = Vec::new();
                    for (name, text) in chunk.iter() {
                        // The reader refuses some inputs the suite holds
                        // valid; its own ledger records them. They have
                        // nothing to write.
                        let Some(value) = read(text) else {
                            unread += 1;
                            continue;
                        };
                        let written =
                            render(&program, text).unwrap_or_else(|e| panic!("{name}: {e}"));
                        match read(&written) {
                            Some(back) if back == value => same += 1,
                            back => differ.push(format!(
                                "{name}\n  in:   {text:?}\n  out:  {written:?}\n  read: {}\n  back: {}",
                                value,
                                back.as_deref().unwrap_or("(does not parse)")
                            )),
                        }
                    }
                    eprintln!("round trip: {} inputs done", chunk.len());
                    (same, unread, differ)
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let same: usize = results.iter().map(|r| r.0).sum();
    let unread: usize = results.iter().map(|r| r.1).sum();
    let differ: Vec<String> = results.into_iter().flat_map(|r| r.2).collect();
    eprintln!(
        "round trip: {total} inputs, {same} the same, {unread} the reader refuses, {} not",
        differ.len()
    );
    let named = |d: &String| d.lines().next().unwrap().to_string();
    let listed = |name: &str| READER_DEFECT.iter().any(|(n, _)| *n == name);
    let unlisted: Vec<&String> = differ.iter().filter(|d| !listed(&named(d))).collect();
    assert!(
        unlisted.is_empty(),
        "{} inputs come back different and are not in READER_DEFECT:\n{}",
        unlisted.len(),
        unlisted
            .iter()
            .map(|d| d.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
    let fixed: Vec<String> = READER_DEFECT
        .iter()
        .filter(|(n, _)| !differ.iter().any(|d| named(d) == *n))
        .map(|(n, issue)| format!("{n} (tabnas/yaml#{issue})"))
        .collect();
    assert!(
        fixed.is_empty(),
        "these now come back the same, or are no longer fixtures: delete them from \
         READER_DEFECT: {fixed:?}"
    );
}
