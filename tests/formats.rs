//! Every fixture loads through its grammar, and the positions the token
//! alignment recovers point at the right source lines.

use std::path::{Path, PathBuf};

use aless::doc::Key;
use aless::load::{self, Format};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// The 1-based line on which `needle` first appears in the fixture.
fn line_of(name: &str, needle: &str) -> u32 {
    let text = std::fs::read_to_string(fixture(name)).unwrap();
    text.lines()
        .position(|l| l.contains(needle))
        .map(|i| i as u32 + 1)
        .unwrap_or_else(|| panic!("{needle:?} not in {name}"))
}

#[test]
fn every_fixture_loads() {
    let mut seen = Vec::new();
    for entry in std::fs::read_dir(fixture("")).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let result = load::load_path(&path, None);
        if name == "bad.json" {
            let err = result.expect_err("bad.json must not parse");
            assert_eq!(err.line, 2, "{err}");
            continue;
        }
        let loaded = result.unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(loaded.doc.len() > 1, "{name}: only a root");
        assert!(
            loaded.doc.nodes.iter().any(|n| n.line > 0),
            "{name}: no positions recovered"
        );
        seen.push(loaded.format);
    }
    // Every format has a fixture.
    for f in [
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
    ] {
        assert!(seen.contains(&f), "no fixture exercised {f}");
    }
}

#[test]
fn json_positions_match_the_file() {
    let loaded = load::load_path(&fixture("nested.json"), None).unwrap();
    let doc = &loaded.doc;
    let title = doc
        .resolve(&[
            Key::Name("store".into()),
            Key::Name("books".into()),
            Key::Index(1),
            Key::Name("title".into()),
        ])
        .unwrap();
    assert_eq!(doc.node(title).line, line_of("nested.json", "TAPL"));
    let version = doc.resolve(&[Key::Name("version".into())]).unwrap();
    assert_eq!(
        doc.node(version).line,
        line_of("nested.json", "\"version\"")
    );
    let counts = doc.resolve(&[Key::Name("counts".into())]).unwrap();
    assert_eq!(doc.node(counts).line, line_of("nested.json", "counts"));
}

#[test]
fn other_grammars_place_values() {
    let toml = load::load_path(&fixture("sample.toml"), None).unwrap().doc;
    let age = toml
        .resolve(&[Key::Name("owner".into()), Key::Name("age".into())])
        .unwrap();
    assert_eq!(toml.node(age).line, line_of("sample.toml", "age"));

    let yaml = load::load_path(&fixture("sample.yaml"), None).unwrap().doc;
    let two = yaml
        .resolve(&[Key::Name("items".into()), Key::Index(1)])
        .unwrap();
    assert_eq!(yaml.node(two).line, line_of("sample.yaml", "two"));

    let csv = load::load_path(&fixture("sample.csv"), None).unwrap().doc;
    let lin = csv
        .resolve(&[Key::Index(1), Key::Name("name".into())])
        .unwrap();
    assert_eq!(csv.node(lin).line, line_of("sample.csv", "lin"));

    let ini = load::load_path(&fixture("sample.ini"), None).unwrap().doc;
    let port = ini
        .resolve(&[Key::Name("server".into()), Key::Name("port".into())])
        .unwrap();
    assert_eq!(ini.node(port).line, line_of("sample.ini", "port"));

    let jsonl = load::load_path(&fixture("sample.jsonl"), None).unwrap().doc;
    assert_eq!(jsonl.root().children, 3);
    let kim = jsonl
        .resolve(&[Key::Index(2), Key::Name("name".into())])
        .unwrap();
    assert_eq!(jsonl.node(kim).line, 3);

    let text = load::load_path(&fixture("lines.txt"), None).unwrap().doc;
    assert!(text.root().children >= 2);
    assert_eq!(text.node(2).line, 2);
}

#[test]
fn unicode_and_edge_values_render() {
    let loaded = load::load_path(&fixture("unicode.jsonic"), None).unwrap();
    let doc = &loaded.doc;
    let weird = doc.resolve(&[Key::Name("weird key".into())]).unwrap();
    assert_eq!(aless::fmt::path_dot(&doc.path(weird)), "[\"weird key\"]");
    let ab = doc
        .resolve(&[Key::Name("weird key".into()), Key::Name("a-b".into())])
        .unwrap();
    assert_eq!(aless::fmt::path_jq(&doc.path(ab)), ".\"weird key\".\"a-b\"");
    let nums = doc.resolve(&[Key::Name("nums".into())]).unwrap();
    let line = aless::fmt::to_json_line(doc, nums);
    assert!(line.starts_with("[0, -1.5, 1e+21, "), "{line}");
    let pretty = aless::fmt::to_json_pretty(doc, 0, 2);
    let back: serde_json::Value = serde_json::from_str(&pretty).unwrap();
    assert_eq!(back["empty_obj"], serde_json::json!({}));
    assert_eq!(back["unicode"], "日本語 — ünïcödé 🎉");
}
