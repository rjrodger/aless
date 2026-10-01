//! `--render <FORMAT>` for any format a crate ships a render for: the
//! translation registry and the host's composition (tabnas/transduce's
//! `docs/translation.md`).
//!
//! A format that can be written says so in its manifest,
//! `tabnas.plugin.json`, whose `translate` object names the shapes it
//! reads as (`tree`, or `records` through a lift, in order of preference),
//! the shape its render writes from, the files that hold its lift and its
//! render (or, for a render alchemy carries, its name, `json` or `csv`),
//! and the sentences that say what a written document does not keep. The
//! format's Rust crate hands over the manifest and the parts' texts
//! (`manifest_text()`, `lift_text()`, `render_text()`), so nothing is
//! copied here: this module reads each crate's manifest once, keyed by its
//! `languageId`, and `--render` names that id.
//!
//! A lift or a render is a library of alchemy definitions with no `export`,
//! its entry point `<id>-lift` or `<id>-render`. aless composes the
//! translation the design's table says, `render ∘ adapt ∘ lift`
//! ([`compose`]): when the target writes from a shape the source reads as,
//! the source's events reach the render in that shape (through the
//! source's lift when the shape is its first and a lift exists, as they
//! are otherwise) and nothing stands between; otherwise one of two adapters
//! runs, from the source's first read shape to the target's write shape. A
//! tree becomes records through the inferred table (the policy `--render
//! csv` has: the root is an array and its elements are the rows, behind
//! the same row check), and records become a tree through the library's
//! `records` (one object per row, keyed by the column labels). The
//! composed program is linked through alchemy's `compile_sources` with a
//! one-line main and run the way `--alchemy` runs a program ([`run`],
//! through [`export::run_program`]): the input read as the format's plan
//! says, the parse pruned behind the value at the start, the deadline and
//! the transducer's limits on the whole run.
//!
//! Under `--alchemy`, the program's output shape takes the source's place
//! ([`compose_program`]): JSON events are a tree and a table is records,
//! and the same table decides the adapter; the program is linked under
//! the name `program-export` (alchemy's `Source::export_as`) and the main
//! calls it, so a program's rows reach a tree's render through `records`,
//! and its events a records render through the inferred table, in one
//! plan. A program that renders its own text takes no render, as before.
//!
//! JSON and CSV are aless's built-ins, the render crate's renderers
//! `--render` has always run ([`Renderer::Json`], [`Renderer::Csv`]). A
//! manifest that names one of them (JSON's own, JSON5's, JSONC's and
//! jsonic's name `json`; CSV's names `csv`) is a format written by that
//! renderer under its own id, with its own loss declaration
//! ([`Part::builtin`]).
//!
//! A render that writes from a tree takes a tree's events, each key once
//! per object, which a walked value keeps by construction; a parse
//! streamed as it proceeds may hand on a member its grammar reads twice,
//! or a stream no tree has, so the run holds the stream to the contract
//! (the transducer's `TreeContract`) and falls back to the parsed value,
//! as `--json` reads it, when nothing had been written yet. A render that
//! writes from records has the row check in front instead, as `--render
//! csv` has, when the rows come from the source.

use std::io::Write;
use std::sync::{Arc, OnceLock};

use serde_json::Value;
use tabnas_alchemy::{compile_sources, Output, Program, Source};
use tabnas_transduce::{Duplicates, Fail, Limits, Metrics, Sink, TreeContract};

use crate::export::{self, ExportError, Input, Job, Renderer, Rows};
use crate::load::Format;

/// A shape a format reads as or writes from: the manifest's `reads` and
/// `writes`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// A document's events, as every format reads.
    Tree,
    /// A table's rows.
    Records,
}

impl Shape {
    fn parse(name: &str) -> Option<Shape> {
        match name {
            "tree" => Some(Shape::Tree),
            "records" => Some(Shape::Records),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Shape::Tree => "tree",
            Shape::Records => "records",
        }
    }
}

/// How a format is written: an alchemy render its crate hands over, or
/// one alchemy carries, which aless runs as its own renderer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Render {
    /// A library of definitions whose entry point is `<id>-render`, named
    /// in diagnostics by its crate and the path the manifest names.
    Alc { file: String, text: &'static str },
    /// alchemy's `json`: the JSON renderer `--render json` runs.
    Json,
    /// alchemy's `csv`: the CSV renderer `--render csv` runs.
    Csv,
}

/// A format's lift: from its events to its first read shape's protocol,
/// entry point `<id>-lift`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lift {
    pub file: String,
    pub text: &'static str,
}

/// A format's translation parts, as its manifest names them and its crate
/// hands them over.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    /// The manifest's `languageId`, which `--render` names.
    pub id: String,
    /// What the format reads as, in order of preference; never empty.
    pub reads: Vec<Shape>,
    /// What the render writes from.
    pub writes: Shape,
    /// The lift, where the events do not carry the first read shape.
    pub lift: Option<Lift>,
    pub render: Render,
    /// What a written document does not keep, a sentence each, as the
    /// manifest says it.
    pub loss: Vec<String>,
}

impl Part {
    /// The render's entry point.
    pub fn render_entry(&self) -> String {
        format!("{}-render", self.id)
    }

    /// The lift's entry point.
    pub fn lift_entry(&self) -> String {
        format!("{}-lift", self.id)
    }

    /// The built-in renderer that writes this format, when its manifest
    /// names a render alchemy carries rather than one of its own.
    pub fn builtin(&self) -> Option<Renderer> {
        match self.render {
            Render::Json => Some(Renderer::Json),
            Render::Csv => Some(Renderer::Csv),
            Render::Alc { .. } => None,
        }
    }
}

/// One crate aless reads: its name, its manifest, and the parts' texts it
/// hands over.
struct Crate {
    name: &'static str,
    manifest: &'static str,
    lift: Option<&'static str>,
    render: Option<&'static str>,
}

/// Each crate aless reads whose manifest may name translation parts.
fn crates() -> Vec<Crate> {
    let plain = |name, manifest| Crate {
        name,
        manifest,
        lift: None,
        render: None,
    };
    let rendered = |name, manifest, render| Crate {
        name,
        manifest,
        lift: None,
        render: Some(render),
    };
    vec![
        plain("tabnas-json", tabnas_json::manifest_text()),
        plain("tabnas-jsonc", tabnas_jsonc::manifest_text()),
        plain("tabnas-json5", tabnas_json5::manifest_text()),
        plain("tabnas-jsonic", tabnas_jsonic::manifest_text()),
        plain("tabnas-csv", tabnas_csv::manifest_text()),
        rendered(
            "tabnas-jsonl",
            tabnas_jsonl::manifest_text(),
            tabnas_jsonl::render_text(),
        ),
        rendered(
            "tabnas-yaml",
            tabnas_yaml::manifest_text(),
            tabnas_yaml::render_text(),
        ),
        rendered(
            "tabnas-toml",
            tabnas_toml::manifest_text(),
            tabnas_toml::render_text(),
        ),
        rendered(
            "tabnas-ini",
            tabnas_ini::manifest_text(),
            tabnas_ini::render_text(),
        ),
        rendered(
            "tabnas-xml",
            tabnas_xml::manifest_text(),
            tabnas_xml::render_text(),
        ),
        rendered(
            "tabnas-zon",
            tabnas_zon::manifest_text(),
            tabnas_zon::render_text(),
        ),
        Crate {
            name: "tabnas-markdown",
            manifest: tabnas_markdown::manifest_text(),
            lift: Some(tabnas_markdown::lift_text()),
            render: Some(tabnas_markdown::render_text()),
        },
    ]
}

/// The part a crate's manifest describes, or `None` when it names no
/// `translate` object, or one this host cannot take: a shape it does not
/// know, a lift or a render the crate does not hand over, a render that
/// is neither an alchemy file nor a name alchemy carries.
fn read_part(
    krate: &str,
    manifest: &str,
    lift: Option<&'static str>,
    render: Option<&'static str>,
) -> Option<Part> {
    let manifest: Value = serde_json::from_str(manifest).ok()?;
    let id = manifest.get("languageId")?.as_str()?;
    let translate = manifest.get("translate")?;
    let reads: Vec<Shape> = match translate.get("reads")? {
        Value::String(one) => vec![Shape::parse(one)?],
        Value::Array(list) => list
            .iter()
            .map(|v| v.as_str().and_then(Shape::parse))
            .collect::<Option<Vec<Shape>>>()?,
        _ => return None,
    };
    if reads.is_empty() {
        return None;
    }
    let writes = Shape::parse(translate.get("writes")?.as_str()?)?;
    let render = match translate.get("render")?.as_str()? {
        "json" => Render::Json,
        "csv" => Render::Csv,
        path if path.ends_with(".alc") => Render::Alc {
            file: format!("{krate}/{path}"),
            text: render?,
        },
        _ => return None,
    };
    let lift = match translate.get("lift").and_then(Value::as_str) {
        Some(path) if path.ends_with(".alc") => Some(Lift {
            file: format!("{krate}/{path}"),
            text: lift?,
        }),
        Some(_) => return None,
        None => None,
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
        reads,
        writes,
        lift,
        render,
        loss,
    })
}

/// Every part the crates hand over, read once.
fn parts() -> &'static [Part] {
    static PARTS: OnceLock<Vec<Part>> = OnceLock::new();
    PARTS.get_or_init(|| {
        crates()
            .into_iter()
            .filter_map(|c| read_part(c.name, c.manifest, c.lift, c.render))
            .collect()
    })
}

/// The part `--render` names by its id.
pub fn part(id: &str) -> Option<&'static Part> {
    parts().iter().find(|p| p.id == id)
}

/// The registry's id for a name `--render` was given, when a part has it.
pub fn id_of(name: &str) -> Option<&'static str> {
    part(name).map(|p| p.id.as_str())
}

/// The part that describes how a source format reads, when one does: TSV
/// reads as CSV's; a grammar from the command line, plain text and a
/// format with no `translate` object have none, and read as a tree.
pub fn source_part(format: Format) -> Option<&'static Part> {
    let id = match format {
        Format::Tsv => "csv",
        Format::Custom(_) | Format::Text => return None,
        other => other.name(),
    };
    part(id)
}

/// Every name `--render` takes, as a message lists them: the built-ins and
/// every id a crate's manifest names, in order, `csv, ini, json, … or zon`.
pub fn names() -> String {
    let mut names: Vec<&str> = vec![Renderer::Csv.name(), Renderer::Json.name()];
    names.extend(parts().iter().map(|p| p.id.as_str()));
    names.sort_unstable();
    names.dedup();
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
    match Format::from_name(name) {
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

/// The name a program is linked under when its output feeds a render.
pub const PROGRAM_EXPORT: &str = "program-export";

/// The adapter a composition runs between the source's shape and the
/// render's, when the two differ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Adapter {
    /// A tree's rows as records: the inferred table, the policy `--render
    /// csv` has.
    InferredTable,
    /// Records as a tree: one object per row, keyed by the column labels.
    Records,
}

impl Adapter {
    /// The adapter's name, as the loss note gives it.
    pub fn name(self) -> &'static str {
        match self {
            Adapter::InferredTable => "the inferred table",
            Adapter::Records => "records",
        }
    }

    /// What the adapter loses, the host's sentences, printed only when it
    /// runs.
    pub fn loss(self) -> Vec<String> {
        match self {
            Adapter::InferredTable => vec![
                "The rows are the elements of the array at the start: an object row's members \
                 are its cells, and a scalar row is one cell named value."
                    .to_string(),
                "The columns are the first row's members: a member a later row adds is not \
                 written, a member it lacks is written empty, and a member repeated in a row \
                 keeps its last value."
                    .to_string(),
            ],
            Adapter::Records => vec![
                "Each row is written as an object keyed by the column labels: a cell the row \
                 lacks is an absent member, and of two columns with one label the last gives \
                 the member."
                    .to_string(),
            ],
        }
    }

    /// The expression that adapts `inner`, a stream in the source's
    /// shape, to the render's.
    fn apply(self, inner: &str) -> String {
        match self {
            Adapter::InferredTable => format!(
                "(table-from-json (record (entry :columns :infer) (entry :rows (path \
                 each-index))) {inner})"
            ),
            Adapter::Records => format!("(records {inner})"),
        }
    }
}

/// A translation ready to run: the composed program, and what stands in
/// front of it.
#[derive(Debug)]
pub struct Composition {
    pub program: Program,
    /// The render writes from a tree and the source's events are one: the
    /// stream is held to a tree's contract.
    pub tree: bool,
    /// The render writes from records the inferred table takes from the
    /// source's events: the row check stands in front, as `--render csv`
    /// has it.
    pub rows: bool,
    pub adapter: Option<Adapter>,
}

impl Composition {
    /// The sentences a run prints: the render's loss declaration, and the
    /// adapter's when one runs.
    pub fn loss(&self, part: &Part) -> Vec<String> {
        let mut loss = part.loss.clone();
        if let Some(adapter) = self.adapter {
            loss.extend(adapter.loss());
        }
        loss
    }
}

/// How a source in `reads` shapes, lifted by `lift` when it has one,
/// reaches a render that writes from `writes`: the expression over
/// `input` that the main exports, and the adapter it runs, if any.
fn route(
    reads: &[Shape],
    lift: Option<&str>,
    writes: Shape,
    input: &str,
) -> (String, Option<Adapter>) {
    let first = reads[0];
    let lifted = match lift {
        Some(entry) => format!("({entry} {input})"),
        None => input.to_string(),
    };
    if writes == first {
        // The first shape, through the lift when there is one.
        return (lifted, None);
    }
    if reads.contains(&writes) {
        // Another shape the format reads as: its events as they are.
        return (input.to_string(), None);
    }
    let adapter = match (first, writes) {
        (Shape::Tree, Shape::Records) => Adapter::InferredTable,
        (Shape::Records, Shape::Tree) => Adapter::Records,
        _ => unreachable!("two shapes that differ are these two"),
    };
    (adapter.apply(&lifted), Some(adapter))
}

/// How `--render ID` writes a source: through one of aless's own renderers
/// when the source's events reach it as they are (the JSON renderer over
/// a tree, the CSV renderer's inferred table over a tree's rows, the paths
/// `--render json` and `--render csv` have always run), or through a
/// composed program otherwise.
#[derive(Debug)]
pub enum Translation {
    Native {
        renderer: Renderer,
        /// The adapter the renderer runs natively: the inferred table,
        /// for CSV over a tree.
        adapter: Option<Adapter>,
    },
    Composed(Composition),
}

impl Translation {
    /// The adapter the translation runs, if any.
    pub fn adapter(&self) -> Option<Adapter> {
        match self {
            Translation::Native { adapter, .. } => *adapter,
            Translation::Composed(c) => c.adapter,
        }
    }

    /// The sentences a run prints: the render's loss declaration, and the
    /// adapter's when one runs.
    pub fn loss(&self, part: &Part) -> Vec<String> {
        let mut loss = part.loss.clone();
        if let Some(adapter) = self.adapter() {
            loss.extend(adapter.loss());
        }
        loss
    }

    /// The composition, when the translation is one.
    pub fn composed(&self) -> Option<&Composition> {
        match self {
            Translation::Composed(c) => Some(c),
            Translation::Native { .. } => None,
        }
    }
}

/// The CSV options a composed `csv` runs under: the standard options with
/// the export's policy for an absent member, an empty field, as the
/// native CSV export and the markdown render have it.
const CSV_OPTIONS: &str = "(record (entry :delimiter \",\") (entry :newline \"\\r\\n\") \
                           (entry :header true) (entry :null-text \"\") (entry :missing \"\"))";

/// The render a composed program calls for `target`, and the source it
/// links when the render is a part's own: alchemy's `json`, its `csv`
/// under the export's options, or the part's entry.
fn render_of(target: &Part) -> (String, Option<Source<'_>>) {
    match &target.render {
        Render::Json => ("json".to_string(), None),
        Render::Csv => (format!("csv {CSV_OPTIONS}"), None),
        Render::Alc { file, text } => (target.render_entry(), Some(Source::new(file, text))),
    }
}

/// How `--render ID` writes a source `source` describes (a tree when it
/// has no part). The route the shapes decide ([`route`]) is run by aless's
/// own renderer when it can run it as it is: JSON over a tree's events as
/// they are, CSV's inferred table over a tree's rows. Otherwise the
/// target's render (a part's own, or alchemy's `json` or `csv`), the
/// source's lift when the route takes it, and one line that exports the
/// composed expression are linked into one program. A part that does not
/// compile is a failure naming its file, never a panic.
pub fn translation(source: Option<&Part>, target: &Part) -> Result<Translation, Fail> {
    let tree = [Shape::Tree];
    let reads = source.map_or(&tree[..], |p| &p.reads[..]);
    let lift = source.and_then(|p| p.lift.as_ref());
    let lift_entry = source.map(Part::lift_entry);
    let (inner, adapter) = route(
        reads,
        lift.and(lift_entry.as_deref()),
        target.writes,
        "input",
    );
    let as_is = inner == "input";
    match &target.render {
        Render::Json if as_is => {
            return Ok(Translation::Native {
                renderer: Renderer::Json,
                adapter: None,
            })
        }
        Render::Csv if reads[0] == Shape::Tree && lift.is_none() => {
            return Ok(Translation::Native {
                renderer: Renderer::Csv,
                adapter: Some(Adapter::InferredTable),
            })
        }
        _ => {}
    }
    let (render, render_source) = render_of(target);
    let main = format!("def export [input] ({render} {inner})");
    let main_name = main_name(target);
    let mut sources = Vec::new();
    if let Some(source) = render_source {
        sources.push(source);
    }
    if let Some(l) = lift {
        sources.push(Source::new(&l.file, l.text));
    }
    sources.push(Source::new(&main_name, &main));
    let program = compile_sources(&sources)?;
    // The source's events are a tree's: held to the contract unless the
    // inferred table reads them as rows, which has the row check instead.
    let rows = adapter == Some(Adapter::InferredTable);
    Ok(Translation::Composed(Composition {
        program: with_policies(program, adapter)?,
        tree: !rows,
        rows,
        adapter,
    }))
}

/// [`translation`] where it composes a program: the tests' and the YAML
/// round trip's way in.
pub fn compose(source: Option<&Part>, target: &Part) -> Result<Composition, Fail> {
    match translation(source, target)? {
        Translation::Composed(c) => Ok(c),
        Translation::Native { renderer, .. } => Err(Fail::new(
            tabnas_transduce::Code::DslTypeError,
            format!(
                "{} is written by aless's own {} renderer over these events, not by a composed \
                 program",
                target.id,
                renderer.name()
            ),
        )),
    }
}

/// The program `--alchemy PROGRAM --render ID` runs when the render is
/// composed over the program's output ([`program_translation`]): the
/// user's program linked under [`PROGRAM_EXPORT`], its output shape
/// standing where the source's would (JSON events a tree, a table
/// records), and the target's render over it through the route the shapes
/// decide. `output` is what the program compiled alone answers; a program
/// that renders its own text is the caller's to refuse.
pub fn compose_program(
    program: Source<'_>,
    output: Output,
    target: &Part,
) -> Result<Composition, Fail> {
    let reads = match output {
        Output::JsonEvents => [Shape::Tree],
        Output::TableRows => [Shape::Records],
        Output::Text => {
            return Err(Fail::new(
                tabnas_transduce::Code::DslTypeError,
                "render_of_text: the program renders its own text, which no render takes",
            ))
        }
    };
    let (inner, adapter) = route(
        &reads,
        None,
        target.writes,
        &format!("({PROGRAM_EXPORT} input)"),
    );
    let (render, render_source) = render_of(target);
    let main = format!("def export [input] ({render} {inner})");
    let main_name = main_name(target);
    let mut sources = Vec::new();
    if let Some(source) = render_source {
        sources.push(source);
    }
    sources.push(program.export_as(PROGRAM_EXPORT));
    sources.push(Source::new(&main_name, &main));
    let program = compile_sources(&sources)?;
    // The program's events are not the source's: the row check and the
    // tree contract stand in front of the source's events only, and the
    // program's output meets the transducer's own refusals.
    Ok(Composition {
        program: with_policies(program, adapter)?,
        tree: false,
        rows: false,
        adapter,
    })
}

/// How `--alchemy PROGRAM --render ID` writes the program's output, as
/// [`translation`] decides it for a source: through one of aless's own
/// renderers when the output reaches it as it is (JSON over the program's
/// events; CSV over its table; and JSON over its table, whose rows the
/// renderer writes as `records` does, named as that adapter), or through
/// the render composed over the output ([`compose_program`]), CSV over
/// the program's events through the inferred table among them. `output`
/// is what the program compiled alone answers; a program that renders its
/// own text is the caller's to refuse.
pub fn program_translation(
    program: Source<'_>,
    output: Output,
    target: &Part,
) -> Result<Translation, Fail> {
    let native = match (&target.render, output) {
        (Render::Json, Output::JsonEvents) => Some((Renderer::Json, None)),
        (Render::Json, Output::TableRows) => Some((Renderer::Json, Some(Adapter::Records))),
        (Render::Csv, Output::TableRows) => Some((Renderer::Csv, None)),
        _ => None,
    };
    match native {
        Some((renderer, adapter)) => Ok(Translation::Native { renderer, adapter }),
        None => compose_program(program, output, target).map(Translation::Composed),
    }
}

/// The policies an adapter runs under: the inferred table keeps a repeated
/// member's last value, as `--render csv` and `--json` do, rather than
/// refusing the row.
fn with_policies(program: Program, adapter: Option<Adapter>) -> Result<Program, Fail> {
    match adapter {
        Some(Adapter::InferredTable) => program.with_duplicates(Duplicates::LastWins),
        _ => Ok(program),
    }
}

/// Run a composition over `input`, writing the document to `out`, through
/// the plumbing a program runs on ([`export::run_program`]), from the
/// value at the job's path. The tree contract or the row check stands in
/// front as the composition says; `metrics` collects what the program's
/// stages report, the high-water mark of what they retain among it.
pub fn run(
    job: &Job,
    composition: &Composition,
    input: Input<'_>,
    out: Box<dyn Write + Send>,
    metrics: Arc<Metrics>,
) -> Result<(), ExportError> {
    let limits = Limits::default();
    export::run_program(job, input, out, |pipe, abort| {
        let sink = composition
            .program
            .with_abort(abort)
            .sink(pipe, None, &limits, metrics.clone())
            .map_err(|fail| ExportError::Program(Box::new(fail)))?;
        let sink: Box<dyn Sink + Send> = if composition.rows {
            Box::new(Rows::new(sink))
        } else if composition.tree {
            Box::new(TreeContract::new(sink))
        } else {
            Box::new(sink)
        };
        Ok(sink)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::What;
    use std::io::BufRead;

    /// Every format whose crate ships parts is in the registry with the
    /// shapes its manifest declares, and the names `--render` takes list
    /// them all.
    #[test]
    fn the_registry_has_every_crates_parts() {
        let tree_renders = ["yaml", "toml", "ini", "xml", "zon", "jsonl"];
        for id in tree_renders {
            let p = part(id).unwrap_or_else(|| panic!("{id}'s manifest names its render"));
            assert_eq!(p.reads, vec![Shape::Tree], "{id}");
            assert_eq!(p.writes, Shape::Tree, "{id}");
            assert!(p.lift.is_none(), "{id}");
            let Render::Alc { file, text } = &p.render else {
                panic!("{id} renders through its own file");
            };
            assert_eq!(file, &format!("tabnas-{id}/alchemy/render.alc"));
            assert!(text.contains(&format!("def {}-render [", id)), "{id}");
            assert!(p.builtin().is_none(), "{id}");
        }
        for id in ["json", "json5", "jsonc", "jsonic"] {
            let p = part(id).unwrap_or_else(|| panic!("{id}'s manifest names json"));
            assert_eq!(p.reads, vec![Shape::Tree], "{id}");
            assert_eq!(p.writes, Shape::Tree, "{id}");
            assert_eq!(p.render, Render::Json, "{id}");
            assert_eq!(p.builtin(), Some(Renderer::Json), "{id}");
        }
        let csv = part("csv").expect("csv's manifest names csv");
        assert_eq!(csv.reads, vec![Shape::Tree]);
        assert_eq!(csv.writes, Shape::Records);
        assert_eq!(csv.builtin(), Some(Renderer::Csv));
        let md = part("markdown").expect("markdown's manifest names its parts");
        assert_eq!(md.reads, vec![Shape::Records, Shape::Tree]);
        assert_eq!(md.writes, Shape::Records);
        let lift = md.lift.as_ref().expect("a lift");
        assert_eq!(lift.file, "tabnas-markdown/alchemy/lift.alc");
        assert!(lift.text.contains("def markdown-lift ["));
        assert_eq!(md.lift_entry(), "markdown-lift");
        assert_eq!(md.render_entry(), "markdown-render");
        assert!(part("yaml")
            .unwrap()
            .loss
            .iter()
            .any(|l| l.starts_with("Comments")));
        assert!(
            part("json").unwrap().loss.is_empty(),
            "JSON declares no loss"
        );
        assert_eq!(id_of("yaml"), Some("yaml"));
        assert_eq!(id_of("feed"), None, "feed's manifest names no render");
        assert_eq!(id_of("tsv"), None, "TSV is written as csv");
        assert_eq!(
            names(),
            "csv, ini, json, json5, jsonc, jsonic, jsonl, markdown, toml, xml, yaml or zon"
        );
        assert_eq!(source_part(Format::Tsv).map(|p| p.id.as_str()), Some("csv"));
        assert_eq!(source_part(Format::Feed), None);
        assert_eq!(source_part(Format::Text), None);
        assert_eq!(
            source_part(Format::Markdown).map(|p| p.id.as_str()),
            Some("markdown")
        );
    }

    #[test]
    fn a_name_with_no_render_is_refused_by_what_it_is() {
        assert_eq!(
            refusal("rss"),
            "--render rss: aless reads feed but has no render for it; --render writes csv, ini, \
             json, json5, jsonc, jsonic, jsonl, markdown, toml, xml, yaml or zon"
        );
        assert_eq!(
            refusal("yml"),
            "--render yml: aless reads yaml but has no render for it; --render writes csv, ini, \
             json, json5, jsonc, jsonic, jsonl, markdown, toml, xml, yaml or zon",
            "an extension names the format it reads, not the render"
        );
        assert_eq!(
            refusal("docx"),
            "--render writes csv, ini, json, json5, jsonc, jsonic, jsonl, markdown, toml, xml, \
             yaml or zon, not docx"
        );
    }

    /// A manifest that names no translate object, a shape or a render this
    /// host cannot take, or a part the crate does not hand over, gives no
    /// part; one that names `json` or `csv` is a built-in's.
    #[test]
    fn a_manifest_this_host_cannot_take_gives_no_part() {
        let text = "def x-render [input] input";
        for manifest in [
            r#"{"languageId": "x"}"#,
            r#"{"languageId": "x", "translate": {"reads": "tree"}}"#,
            r#"{"languageId": "x", "translate": {"reads": "tree", "writes": "text", "render": "x.alc"}}"#,
            r#"{"languageId": "x", "translate": {"reads": "blob", "writes": "tree", "render": "x.alc"}}"#,
            r#"{"languageId": "x", "translate": {"reads": [], "writes": "tree", "render": "x.alc"}}"#,
            r#"{"languageId": "x", "translate": {"reads": "tree", "writes": "tree", "render": "x.txt"}}"#,
            r#"{"languageId": "x", "translate": {"reads": "tree", "writes": "tree", "render": "x.alc", "lift": "l.alc"}}"#,
            "not json",
        ] {
            assert_eq!(
                read_part("x", manifest, None, Some(text)),
                None,
                "{manifest}"
            );
        }
        assert_eq!(
            read_part(
                "x",
                r#"{"languageId": "x", "translate": {"reads": "tree", "writes": "tree", "render": "alchemy/render.alc"}}"#,
                None,
                None,
            ),
            None,
            "a render the crate does not hand over"
        );
        let part = read_part(
            "tabnas-x",
            r#"{"languageId": "x", "translate": {"reads": "tree", "writes": "records", "render": "csv"}}"#,
            None,
            None,
        )
        .unwrap();
        assert_eq!(part.render, Render::Csv);
        assert_eq!(part.builtin(), Some(Renderer::Csv));
        assert!(part.loss.is_empty());
        let part = read_part(
            "tabnas-x",
            r#"{"languageId": "x", "translate": {"reads": ["records", "tree"], "writes": "records", "lift": "alchemy/lift.alc", "render": "alchemy/render.alc", "loss": ["A.", 1]}}"#,
            Some("def x-lift [input] input"),
            Some(text),
        )
        .unwrap();
        assert_eq!(part.reads, vec![Shape::Records, Shape::Tree]);
        assert_eq!(
            part.lift.as_ref().unwrap().file,
            "tabnas-x/alchemy/lift.alc"
        );
        assert_eq!(part.loss, vec!["A.".to_string()]);
    }

    /// The design's table: a tree's render takes a tree's events as they
    /// are; a records render takes a format's first shape through its
    /// lift; a tree reaches a records render through the inferred table,
    /// with the row check in front; a lifted format whose second shape is
    /// the render's takes its events as they are.
    #[test]
    fn the_route_follows_the_designs_table() {
        let tree = [Shape::Tree];
        let md = [Shape::Records, Shape::Tree];
        assert_eq!(
            route(&tree, None, Shape::Tree, "input"),
            ("input".to_string(), None)
        );
        assert_eq!(
            route(&md, Some("markdown-lift"), Shape::Records, "input"),
            ("(markdown-lift input)".to_string(), None)
        );
        assert_eq!(
            route(&md, Some("markdown-lift"), Shape::Tree, "input"),
            ("input".to_string(), None),
            "a shape the format reads as, after its first: the events as they are"
        );
        assert_eq!(
            route(&tree, None, Shape::Records, "input"),
            (
                "(table-from-json (record (entry :columns :infer) (entry :rows (path each-index))) input)".to_string(),
                Some(Adapter::InferredTable)
            )
        );
        assert_eq!(
            route(
                &[Shape::Records],
                None,
                Shape::Tree,
                "(program-export input)"
            ),
            (
                "(records (program-export input))".to_string(),
                Some(Adapter::Records)
            )
        );
    }

    /// Every composition over the source's events compiles to a text
    /// program, with what stands in front decided by the shapes.
    #[test]
    fn every_composition_compiles_and_writes_text() {
        let yaml = part("yaml").unwrap();
        let json = source_part(Format::Json);
        let c = compose(json, yaml).unwrap_or_else(|f| panic!("{f}"));
        assert_eq!(c.program.output(), Output::Text);
        assert!(c.tree && !c.rows && c.adapter.is_none());
        assert_eq!(c.loss(yaml), yaml.loss);
        // A tree into a records render: the inferred table, the row check.
        let md = part("markdown").unwrap();
        let c = compose(json, md).unwrap_or_else(|f| panic!("{f}"));
        assert_eq!(c.program.output(), Output::Text);
        assert!(c.rows && !c.tree);
        assert_eq!(c.adapter, Some(Adapter::InferredTable));
        assert_eq!(c.loss(md).len(), md.loss.len() + 2);
        // A lifted format into its own render: no adapter, the contract.
        let c = compose(Some(md), md).unwrap_or_else(|f| panic!("{f}"));
        assert!(c.tree && !c.rows && c.adapter.is_none());
        // A lifted format into a tree's render: its tree as it is.
        let c = compose(Some(md), yaml).unwrap_or_else(|f| panic!("{f}"));
        assert!(c.tree && !c.rows && c.adapter.is_none());
        // Every render of its own, from a tree.
        for id in ["toml", "ini", "xml", "zon", "jsonl", "yaml", "markdown"] {
            let target = part(id).unwrap();
            let c =
                compose(source_part(Format::Toml), target).unwrap_or_else(|f| panic!("{id}: {f}"));
            assert_eq!(c.program.output(), Output::Text, "{id}");
        }
        // A built-in's renderer runs when the events reach it as they are:
        // JSON over a tree, CSV's inferred table over a tree's rows; a
        // lifted format reaches CSV through its lift, composed.
        assert!(matches!(
            translation(json, part("json5").unwrap()),
            Ok(Translation::Native {
                renderer: Renderer::Json,
                adapter: None
            })
        ));
        assert!(matches!(
            translation(json, part("csv").unwrap()),
            Ok(Translation::Native {
                renderer: Renderer::Csv,
                adapter: Some(Adapter::InferredTable)
            })
        ));
        assert!(matches!(
            translation(Some(md), part("json").unwrap()),
            Ok(Translation::Native {
                renderer: Renderer::Json,
                ..
            })
        ));
        let csv = part("csv").unwrap();
        let c = compose(Some(md), csv).unwrap_or_else(|f| panic!("{f}"));
        assert_eq!(c.program.output(), Output::Text);
        assert!(c.tree && !c.rows && c.adapter.is_none());
        assert!(compose(json, part("json5").unwrap()).is_err());
        let t = translation(json, csv).unwrap();
        assert_eq!(
            t.loss(csv).len(),
            csv.loss.len() + 2,
            "the adapter's loss is printed"
        );
        assert!(t.composed().is_none());
    }

    /// Under `--alchemy`, the program's output stands where the source's
    /// events would: its events into a tree's render as they are, its
    /// table through `records`; its table into a records render as it is,
    /// its events through the inferred table.
    #[test]
    fn a_programs_output_is_composed_by_its_shape() {
        let yaml = part("yaml").unwrap();
        let md = part("markdown").unwrap();
        let events = Source::new("p.alc", "def export [input] input");
        let table = Source::new(
            "t.alc",
            "def export [input] (table-from-json (record (entry :columns :infer) (entry :rows (path each-index))) input)",
        );
        let c = compose_program(events, Output::JsonEvents, yaml).unwrap_or_else(|f| panic!("{f}"));
        assert_eq!(c.program.output(), Output::Text);
        assert!(c.adapter.is_none() && !c.tree && !c.rows);
        let c = compose_program(table, Output::TableRows, yaml).unwrap_or_else(|f| panic!("{f}"));
        assert_eq!(c.adapter, Some(Adapter::Records));
        let c = compose_program(table, Output::TableRows, md).unwrap_or_else(|f| panic!("{f}"));
        assert!(c.adapter.is_none());
        let c = compose_program(events, Output::JsonEvents, md).unwrap_or_else(|f| panic!("{f}"));
        assert_eq!(c.adapter, Some(Adapter::InferredTable));
        // Through aless's own renderers when the output reaches them as it
        // is (JSON over events; CSV over a table; JSON over a table, as
        // records), composed otherwise: CSV over events goes through the
        // inferred table, as --render csv reads a tree.
        let json = part("json").unwrap();
        let csv = part("csv").unwrap();
        assert!(matches!(
            program_translation(events, Output::JsonEvents, json),
            Ok(Translation::Native {
                renderer: Renderer::Json,
                adapter: None
            })
        ));
        assert!(matches!(
            program_translation(events, Output::JsonEvents, part("json5").unwrap()),
            Ok(Translation::Native {
                renderer: Renderer::Json,
                adapter: None
            })
        ));
        assert!(matches!(
            program_translation(table, Output::TableRows, json),
            Ok(Translation::Native {
                renderer: Renderer::Json,
                adapter: Some(Adapter::Records)
            })
        ));
        assert!(matches!(
            program_translation(table, Output::TableRows, csv),
            Ok(Translation::Native {
                renderer: Renderer::Csv,
                adapter: None
            })
        ));
        let t =
            program_translation(events, Output::JsonEvents, csv).unwrap_or_else(|f| panic!("{f}"));
        let c = t.composed().expect("CSV over events is composed");
        assert_eq!(c.adapter, Some(Adapter::InferredTable));
        assert_eq!(c.program.output(), Output::Text);
        assert_eq!(t.loss(csv).len(), csv.loss.len() + 2);
        let t = program_translation(table, Output::TableRows, json).unwrap();
        assert_eq!(t.loss(json), Adapter::Records.loss());
        let fail = compose_program(events, Output::Text, yaml).unwrap_err();
        assert!(fail.message.starts_with("render_of_text"), "{fail}");
        // A program that does not define export is refused by name.
        let fail = compose_program(
            Source::new("n.alc", "def x [a] a"),
            Output::JsonEvents,
            yaml,
        )
        .unwrap_err();
        assert!(fail.message.starts_with("no_export: n.alc"), "{fail}");
    }

    /// CSV to YAML streams: CSV at the root is read a record at a time, and
    /// the render keeps one marker per open container, so ten times the
    /// rows write ten times the text and leave the retained high-water mark
    /// where it was.
    #[test]
    fn ten_times_the_rows_leave_what_the_render_retains_flat() {
        let composition = compose(source_part(Format::Csv), part("yaml").unwrap()).unwrap();
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
                &composition,
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
            reads: vec![Shape::Tree],
            writes: Shape::Tree,
            lift: None,
            render: Render::Alc {
                file: "tabnas-x/alchemy/render.alc".into(),
                text: "def x-render [input]\n  (nope input)",
            },
            loss: Vec::new(),
        };
        let fail = compose(None, &broken).unwrap_err();
        assert!(fail.message.starts_with("unknown_name"), "{fail}");
        assert_eq!(fail.file.as_deref(), Some("tabnas-x/alchemy/render.alc"));
        assert_eq!((fail.row, fail.column), (Some(2), Some(4)));
    }
}
