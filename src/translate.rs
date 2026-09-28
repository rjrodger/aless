//! `--render <FORMAT>` for a format written by its own render: the
//! translation registry (tabnas/transduce's `docs/translation.md`).
//!
//! A format that can be written says so in its manifest,
//! `tabnas.plugin.json`, whose `translate` object names the shape it
//! writes from (`tree` or `records`), the file that holds its render, and
//! the sentences that say what a written document does not keep. The
//! format's Rust crate hands over the manifest and the render's text
//! (`manifest_text()`, `render_text()`), so nothing is copied here: this
//! module reads each crate's manifest once, keyed by its `languageId`, and
//! `--render` names that id.
//!
//! A render is a library of alchemy definitions with no `export`, its entry
//! point `<id>-render`. aless links it with a one-line program that exports
//! the render of its input ([`compose`], through alchemy's
//! `compile_sources`) and runs that program the way `--alchemy` runs one
//! ([`run`], through [`export::run_program`]): the input read as the
//! format's plan says, the parse pruned behind the value at the start, the
//! deadline and the transducer's limits on the whole run. JSON and CSV
//! stay aless's built-ins, the render crate's renderers `--render` has
//! always run ([`Renderer::Json`], [`Renderer::Csv`]).
//!
//! Every format aless reads is read as a tree: its events are the
//! document's. A render that writes from a tree (YAML's) therefore runs over
//! the source's events as they are, with no adapter between. It takes a
//! tree's events, each key once per object, which a walked value keeps by
//! construction; a parse streamed as it proceeds hands on a member its
//! grammar reads twice, so the run refuses one ([`export::UniqueMembers`]),
//! and falls back to the parsed value, as `--json` reads it, when nothing
//! had been written yet. A render that writes from records needs the tree
//! adapted to rows in front of it (the inferred table, behind the row check
//! `--render csv` has), which aless does not compose yet, so the registry
//! lists only the renders that write from a tree ([`runs`]): until then, a
//! format whose render writes from records is read and not written.

use std::io::Write;
use std::sync::{Arc, OnceLock};

use serde_json::Value;
use tabnas_alchemy::{compile_sources, Program, Source};
use tabnas_transduce::{Fail, Limits, Metrics};

use crate::export::{self, ExportError, Input, Job, Renderer};

/// The shape a format is written from: the manifest's `writes`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// A document's events, as every format reads.
    Tree,
    /// A table's rows.
    Records,
}

/// A format's render, as its manifest names it and its crate hands it over.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    /// The manifest's `languageId`, which `--render` names.
    pub id: String,
    /// What the render writes from.
    pub writes: Shape,
    /// The name the render's diagnostics give its text: the crate and the
    /// path the manifest names.
    pub file: String,
    /// The render's text.
    pub text: &'static str,
    /// What a written document does not keep, a sentence each, as the
    /// manifest says it.
    pub loss: Vec<String>,
}

impl Part {
    /// The render's entry point.
    pub fn entry(&self) -> String {
        format!("{}-render", self.id)
    }
}

/// Each crate aless reads whose manifest may name a render: its name, its
/// manifest, and its render's text.
fn crates() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![(
        "tabnas-yaml",
        tabnas_yaml::manifest_text(),
        tabnas_yaml::render_text(),
    )]
}

/// The part a crate's manifest describes, or `None` when it names no
/// render of its own (none at all, or one alchemy carries, `json` or
/// `csv`), or does not say what it writes from.
fn read_part(krate: &str, manifest: &str, text: &'static str) -> Option<Part> {
    let manifest: Value = serde_json::from_str(manifest).ok()?;
    let id = manifest.get("languageId")?.as_str()?;
    let translate = manifest.get("translate")?;
    let render = translate.get("render")?.as_str()?;
    if !render.ends_with(".alc") {
        return None;
    }
    let writes = match translate.get("writes")?.as_str()? {
        "tree" => Shape::Tree,
        "records" => Shape::Records,
        _ => return None,
    };
    let loss = translate
        .get("loss")
        .and_then(Value::as_array)
        .map(|lines| {
            lines
                .iter()
                .filter_map(|l| l.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    Some(Part {
        id: id.to_string(),
        writes,
        file: format!("{krate}/{render}"),
        text,
        loss,
    })
}

/// Every part the crates hand over that aless can run, read once.
fn parts() -> &'static [Part] {
    static PARTS: OnceLock<Vec<Part>> = OnceLock::new();
    PARTS.get_or_init(|| {
        crates()
            .into_iter()
            .filter_map(|(krate, manifest, text)| read_part(krate, manifest, text))
            .filter(runs)
            .collect()
    })
}

/// Whether aless can run a part: one that writes from a tree takes the
/// source's events as they are. One that writes from records would need
/// the inferred table in front of it, which aless does not compose yet.
fn runs(part: &Part) -> bool {
    part.writes == Shape::Tree
}

/// The part `--render` names by its id.
pub fn part(id: &str) -> Option<&'static Part> {
    parts().iter().find(|p| p.id == id)
}

/// The registry's id for a name `--render` was given, when a part has it.
pub fn id_of(name: &str) -> Option<&'static str> {
    part(name).map(|p| p.id.as_str())
}

/// Every name `--render` takes, the built-ins first, as a message lists
/// them: `csv, json or yaml`.
pub fn names() -> String {
    let mut names = vec![Renderer::Csv.name(), Renderer::Json.name()];
    names.extend(parts().iter().map(|p| p.id.as_str()));
    match names.split_last() {
        Some((last, [])) => last.to_string(),
        Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
        None => String::new(),
    }
}

/// Why `--render` refuses a name: a format aless reads and has no render
/// for, or a name it does not know at all.
pub fn refusal(name: &str) -> String {
    let writes = names();
    match crate::load::Format::from_name(name) {
        Some(format) => format!(
            "--render {}: aless reads {} but has no render for it; --render writes {writes}",
            name.trim(),
            format.name()
        ),
        None => format!("--render writes {writes}, not {}", name.trim()),
    }
}

/// The name the composed program's own line goes by in diagnostics.
pub fn main_name(part: &Part) -> String {
    format!("--render {}", part.id)
}

/// The program `--render ID` runs: the render linked with one line that
/// exports its render of the input. A render that does not compile is a
/// failure naming its file, never a panic.
pub fn compose(part: &Part) -> Result<Program, Fail> {
    let main = format!("def export [input] ({} input)", part.entry());
    compile_sources(&[
        Source {
            file: &part.file,
            text: part.text,
        },
        Source {
            file: &main_name(part),
            text: &main,
        },
    ])
}

/// Run a composed program over `input`, writing the document to `out`,
/// through the plumbing a program runs on ([`export::run_program`]), from
/// the value at the job's path. The run keeps a tree's contract
/// ([`export::UniqueMembers`]); `metrics` collects what the program's
/// stages report, the high-water mark of what they retain among it.
pub fn run(
    job: &Job,
    program: &Program,
    input: Input<'_>,
    out: Box<dyn Write + Send>,
    metrics: Arc<Metrics>,
) -> Result<(), ExportError> {
    let limits = Limits::default();
    export::run_program(job, input, out, |pipe, abort| {
        let sink = program
            .with_abort(abort)
            .sink(pipe, None, &limits, metrics.clone())
            .map_err(|fail| ExportError::Program(Box::new(fail)))?;
        Ok(Box::new(export::UniqueMembers::new(sink)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::What;
    use crate::load::Format;
    use std::io::BufRead;

    #[test]
    fn yamls_manifest_names_its_render() {
        let yaml = part("yaml").expect("tabnas-yaml's manifest names a render");
        assert_eq!(yaml.writes, Shape::Tree);
        assert!(runs(yaml));
        assert_eq!(yaml.file, "tabnas-yaml/alchemy/render.alc");
        assert_eq!(yaml.entry(), "yaml-render");
        assert!(yaml.text.contains("def yaml-render [input]"));
        assert!(
            yaml.loss.iter().any(|l| l.starts_with("Comments")),
            "{:?}",
            yaml.loss
        );
        assert_eq!(id_of("yaml"), Some("yaml"));
        assert_eq!(id_of("toml"), None, "toml's manifest names no render");
        assert_eq!(names(), "csv, json or yaml");
    }

    #[test]
    fn a_name_with_no_render_is_refused_by_what_it_is() {
        assert_eq!(
            refusal("toml"),
            "--render toml: aless reads toml but has no render for it; --render writes csv, \
             json or yaml"
        );
        assert_eq!(
            refusal("yml"),
            "--render yml: aless reads yaml but has no render for it; --render writes csv, \
             json or yaml",
            "an extension names the format it reads, not the render"
        );
        assert_eq!(
            refusal("docx"),
            "--render writes csv, json or yaml, not docx"
        );
    }

    /// A manifest that names no render of its own, or one alchemy carries,
    /// or no shape to write from, gives no part; one whose render writes
    /// from records gives a part aless does not run yet.
    #[test]
    fn a_manifest_without_a_render_of_its_own_gives_no_part() {
        let text = "def x-render [input] input";
        for manifest in [
            r#"{"languageId": "x"}"#,
            r#"{"languageId": "x", "translate": {"reads": "tree"}}"#,
            r#"{"languageId": "x", "translate": {"reads": "tree", "writes": "records", "render": "csv"}}"#,
            r#"{"languageId": "x", "translate": {"reads": "tree", "writes": "text", "render": "x.alc"}}"#,
            "not json",
        ] {
            assert_eq!(read_part("x", manifest, text), None, "{manifest}");
        }
        let part = read_part(
            "tabnas-x",
            r#"{"languageId": "x", "translate": {"reads": "tree", "writes": "records", "render": "alchemy/render.alc"}}"#,
            text,
        )
        .unwrap();
        assert_eq!(part.writes, Shape::Records);
        assert_eq!(part.file, "tabnas-x/alchemy/render.alc");
        assert!(part.loss.is_empty());
        assert!(
            !runs(&part),
            "a records render needs the inferred table first"
        );
    }

    #[test]
    fn the_composed_program_compiles_and_writes_text() {
        let yaml = part("yaml").unwrap();
        let program = compose(yaml).unwrap_or_else(|f| panic!("{f}"));
        assert_eq!(program.output(), tabnas_alchemy::Output::Text);
    }

    /// CSV to YAML streams: CSV at the root is read a record at a time, and
    /// the render keeps one marker per open container, so ten times the
    /// rows write ten times the text and leave the retained high-water mark
    /// where it was.
    #[test]
    fn ten_times_the_rows_leave_what_the_render_retains_flat() {
        let program = compose(part("yaml").unwrap()).unwrap();
        let run_rows = |rows: usize| {
            let mut csv = String::from("name,age,city\n");
            for i in 0..rows {
                csv.push_str(&format!("name {i},{i},city {i}\n"));
            }
            let job = Job {
                name: "rows.csv".into(),
                origin: "rows.csv".into(),
                format: Format::Csv,
                what: What::Part,
                path: Vec::new(),
                compact: false,
                indent: 2,
                timeout: None,
            };
            let metrics = Metrics::new();
            let reader: Box<dyn BufRead + Send> = Box::new(std::io::Cursor::new(csv.into_bytes()));
            run(
                &job,
                &program,
                Input::Lines(reader),
                Box::new(std::io::sink()),
                metrics.clone(),
            )
            .unwrap();
            (
                Metrics::get(&metrics.retained_bytes_high),
                Metrics::get(&metrics.output_bytes),
            )
        };
        let (one, written_one) = run_rows(200);
        let (ten, written_ten) = run_rows(2000);
        eprintln!("retention: 200 rows retain at most {one} bytes, 2000 rows {ten}");
        assert!(one > 0, "the render's state is measured");
        assert!(written_ten > 9 * written_one, "{written_one} {written_ten}");
        assert_eq!(ten, one, "the peak is one row's, not the count's");
    }

    /// A render that does not compile is a failure naming its file.
    #[test]
    fn a_render_that_does_not_compile_names_its_file() {
        let broken = Part {
            id: "x".into(),
            writes: Shape::Tree,
            file: "tabnas-x/alchemy/render.alc".into(),
            text: "def x-render [input]\n  (nope input)",
            loss: Vec::new(),
        };
        let fail = compose(&broken).unwrap_err();
        assert!(fail.message.starts_with("unknown_name"), "{fail}");
        assert_eq!(fail.file.as_deref(), Some("tabnas-x/alchemy/render.alc"));
        assert_eq!((fail.row, fail.column), (Some(2), Some(4)));
    }
}
