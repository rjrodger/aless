//! `--render <FORMAT>` for any format a crate ships a render for: the
//! registry of the formats' translation parts, and how aless runs the
//! translation alchemy composes from them (admin ADR-27; tabnas/transduce's
//! `docs/translation.md`).
//!
//! A format that can be written says so in its manifest,
//! `tabnas.plugin.json`, whose `translate` object names the shapes it
//! reads as (`tree`, or `records` through a lift, in order of preference),
//! the shape its render writes from, the root that render needs, the
//! schema its events carry when they are not a plain tree, the files that
//! hold its lift, its embed and its render (or, for a render alchemy
//! carries, its name, `json` or `csv`), and the sentences that say what a
//! written document does not keep. The format's Rust crate hands over one
//! structural descriptor, `translate()`: the manifest, and each part's
//! entry point and source text. aless reads each crate's descriptor once
//! into alchemy's [`Part`], keyed by its `languageId`, and `--render`
//! names that id. Nothing is copied here, and nothing here knows a format
//! by name but JSON Lines, whose render writes a record a line
//! ([`records`]).
//!
//! alchemy composes the translation ([`tabnas_alchemy::translate`]): the
//! source's lift, where the target writes from records and the source
//! reads as records first; for a target that writes from a tree, the embed
//! into its schema, or the root adapter its render needs (`wrap-object`,
//! the root as the one member `--key` names, or `wrap-array`); for one
//! that writes from records, a tree's rows through the inferred table, the
//! root an array; then the target's render. aless links the composition
//! with alchemy's `compile_sources` and runs it the way `--alchemy` runs a
//! program ([`run`], through [`export::run_program`]): the input read as
//! the format's plan says, the deadline and the transducer's limits on the
//! whole run, and a tree's contract in front of a render that writes from
//! one, where alchemy says the source's events reach it whole
//! ([`Front::Tree`]). What a target cannot carry is a declared convention,
//! not a refusal: each adapter's sentences join the render's own in the
//! loss note. The one refusal is a schema-only target's (a schema and no
//! embed), which writes only the tree its own documents read as, and which
//! a program that makes that tree can write ([`schema_only`]).
//!
//! Under `--alchemy`, the program's output takes the source's place
//! ([`compose_program`]): its JSON events are a tree and its table is
//! records, so a program's rows reach a tree's render through `records`,
//! and its events a records render through the inferred table, in one
//! plan.
//!
//! Two routes run on aless's own renderers instead ([`translation`]),
//! the paths `--render json` and `--render csv` have always taken, each
//! writing what the composed route writes: JSON over a source's events as
//! they are (alchemy's [`Native::Json`]), which keeps `--compact` and
//! `--indent` and writes a number that is not finite as null, as the
//! composed `json` does; and CSV over a tree's rows, its root made an
//! array, which runs the table without the interpreted `wrap-array` stage
//! and writes a number that is not finite as its word and a table of no
//! columns as the empty document, as the composed `csv` does.

use std::io::Write;
use std::sync::{Arc, OnceLock};

pub use tabnas_alchemy::translate::{Adapter, Front, Native, Options, Part, Render, Root, Shape};
use tabnas_alchemy::translate::{Composition as Composed, Descriptor, PartText};
use tabnas_alchemy::{Output, Program, Source};
use tabnas_render::WriteOut;
use tabnas_transduce::{Code, Fail, Limits, Metrics, Sink, TreeContract};

use crate::alchemy::{renderers, routers};
use crate::export::{self, ExportError, Input, Job, Records, Renderer};
use crate::load::Format;

/// Each crate aless reads whose manifest may name translation parts, as
/// alchemy's structural descriptor.
fn crates() -> Vec<Descriptor> {
    // Every grammar deliberately owns its interface types. This macro is the
    // narrow host adapter: it reads the same fields from each package-local
    // value and immediately normalizes them into alchemy's descriptor.
    macro_rules! descriptor {
        ($name:literal, $module:ident) => {
            $module::translate().map(|parts| Descriptor {
                package: $name.to_string(),
                manifest: parts.manifest.to_string(),
                lift: parts.lift.map(|p| PartText::new(p.entry, p.source)),
                embed: parts.embed.map(|p| PartText::new(p.entry, p.source)),
                render: parts.render.map(|p| PartText::new(p.entry, p.source)),
            })
        };
    }
    vec![
        descriptor!("tabnas-json", tabnas_json),
        descriptor!("tabnas-jsonc", tabnas_jsonc),
        descriptor!("tabnas-json5", tabnas_json5),
        descriptor!("tabnas-jsonic", tabnas_jsonic),
        descriptor!("tabnas-csv", tabnas_csv),
        descriptor!("tabnas-jsonl", tabnas_jsonl),
        descriptor!("tabnas-yaml", tabnas_yaml),
        descriptor!("tabnas-toml", tabnas_toml),
        descriptor!("tabnas-ini", tabnas_ini),
        descriptor!("tabnas-xml", tabnas_xml),
        descriptor!("tabnas-zon", tabnas_zon),
        descriptor!("tabnas-markdown", tabnas_markdown),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// Every part the crates hand over, read once. A crate whose manifest
/// names no `translate` object (a format read and not written), or one
/// alchemy cannot take, gives none.
fn parts() -> &'static [Part] {
    static PARTS: OnceLock<Vec<Part>> = OnceLock::new();
    PARTS.get_or_init(|| crates().iter().filter_map(Part::from_descriptor).collect())
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
/// format with no `translate` object have none, and read as a plain tree.
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
    let names = render_names();
    match names.split_last() {
        Some((last, [])) => last.to_string(),
        Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
        None => String::new(),
    }
}

/// Every format `--render` writes, sorted: aless's own two, and each
/// format whose crate carries a render.
pub fn render_names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = vec![Renderer::Csv.name(), Renderer::Json.name()];
    names.extend(parts().iter().map(|p| p.id.as_str()));
    names.sort_unstable();
    names.dedup();
    names
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

/// Where the records of what `part`'s render writes end, for a run that
/// fails part way ([`Records`]): alchemy's CSV and JSON write records the
/// pipe can see, and JSON Lines a record a line; another format's render
/// writes text of its own shape.
pub fn records(part: &Part) -> Records {
    match &part.render {
        Render::Csv => Records::Csv,
        Render::Json => Records::Json,
        Render::Alc(_) if part.id == "jsonl" => Records::Lines,
        Render::Alc(_) => Records::Any,
    }
}

/// The adapter between the two shapes among `adapters`, the inferred table
/// or `records`, which the loss note names as `adapter`.
pub fn shape_adapter(adapters: &[Adapter]) -> Option<&Adapter> {
    adapters
        .iter()
        .find(|a| matches!(a, Adapter::InferredTable | Adapter::Records))
}

/// The usage error for a target that refuses the source's tree, or `None`
/// for any other failure: a schema-only format (a schema and no embed)
/// writes only the tree its own documents read as, which a program that
/// makes one can give it. `name` is the input's.
pub fn schema_only(fail: &Fail, target: &Part, name: &str) -> Option<String> {
    if fail.code != Code::TargetValueUnrepresentable || !fail.message.starts_with("schema_only:") {
        return None;
    }
    let schema = target.schema.as_deref().unwrap_or(&target.id);
    Some(format!(
        "--render {id}: {id} writes a {schema} tree, the tree its own documents read as, and \
         {name} is not one; a program that makes one can write it: --alchemy FILE --render {id}",
        id = target.id
    ))
}

/// A translation ready to run: the composed program, and what stands in
/// front of it.
#[derive(Debug)]
pub struct Composition {
    pub program: Program,
    /// The render writes from a tree and the source's events reach it
    /// whole: the stream is held to a tree's contract.
    pub tree: bool,
    /// What runs between the source's events and the render, in order.
    pub adapters: Vec<Adapter>,
    /// The render's loss declaration, then each adapter's.
    pub loss: Vec<String>,
}

/// How `--render ID` writes a source: through one of aless's own renderers
/// ([`translation`]), or through the composed program.
#[derive(Debug)]
pub enum Translation {
    Native {
        renderer: Renderer,
        /// What the renderer's route runs, as the composition names it:
        /// the inferred table and its root, for CSV.
        adapters: Vec<Adapter>,
        loss: Vec<String>,
    },
    Composed(Composition),
}

impl Translation {
    /// What runs between the source's events and the render, in order.
    pub fn adapters(&self) -> &[Adapter] {
        match self {
            Translation::Native { adapters, .. } => adapters,
            Translation::Composed(c) => &c.adapters,
        }
    }

    /// The sentences a run prints: the render's loss declaration, then
    /// each adapter's.
    pub fn loss(&self) -> &[String] {
        match self {
            Translation::Native { loss, .. } => loss,
            Translation::Composed(c) => &c.loss,
        }
    }

    /// The composition, when the translation is one.
    pub fn composed(&self) -> Option<&Composition> {
        match self {
            Translation::Composed(c) => Some(c),
            Translation::Native { .. } => None,
        }
    }
}

/// Link alchemy's composition into a program, `program` linked under
/// `program-export` when the composition is over its output. A part that
/// does not compile is a failure naming its file, never a panic.
fn compiled(composed: Composed, program: Option<Source<'_>>) -> Result<Composition, Fail> {
    let linked = composed.compile(program, routers(), renderers())?;
    Ok(Composition {
        program: linked,
        tree: composed.front == Front::Tree,
        adapters: composed.adapters,
        loss: composed.loss,
    })
}

/// Whether a composition is the one aless's own CSV export runs: alchemy's
/// `csv` over the inferred table of a tree whose root is made an array,
/// with no part of a format's own linked (a lifted source's rows go
/// through its lift, composed).
fn native_csv(target: &Part, composed: &Composed) -> bool {
    target.render == Render::Csv
        && composed.sources.is_empty()
        && composed.adapters == [Adapter::WrapArray, Adapter::InferredTable]
}

/// How `--render ID` writes a source `source` describes (a plain tree when
/// it has none, or below the root): the route alchemy composes, run by one
/// of aless's own renderers where that writes the same document (JSON over
/// the source's events as they are; CSV over a tree's rows, its root made
/// an array), and linked into one program otherwise.
pub fn translation(
    source: Option<&Part>,
    target: &Part,
    options: &Options,
) -> Result<Translation, Fail> {
    let composed = tabnas_alchemy::translate::compose(source, target, options, &main_name(target))?;
    let native = match composed.native {
        Some(Native::Json) => Some(Renderer::Json),
        None if native_csv(target, &composed) => Some(Renderer::Csv),
        None => None,
    };
    match native {
        Some(renderer) => Ok(Translation::Native {
            renderer,
            adapters: composed.adapters,
            loss: composed.loss,
        }),
        None => compiled(composed, None).map(Translation::Composed),
    }
}

/// The composed program for a source, whichever renderer [`translation`]
/// would choose: the tests' and the YAML round trip's way in.
pub fn compose(
    source: Option<&Part>,
    target: &Part,
    options: &Options,
) -> Result<Composition, Fail> {
    compiled(
        tabnas_alchemy::translate::compose(source, target, options, &main_name(target))?,
        None,
    )
}

/// The program `--alchemy PROGRAM --render ID` runs: the user's program
/// linked under `program-export`, its output standing where a source's
/// events would (JSON events a plain tree, a table records), and the
/// target's render over it through the route alchemy composes. `output` is
/// what the program compiled alone answers; one that renders its own text
/// is refused (`render_of_text`), as is one with no `export`.
pub fn compose_program(
    program: Source<'_>,
    output: Output,
    target: &Part,
    options: &Options,
) -> Result<Composition, Fail> {
    let composed =
        tabnas_alchemy::translate::compose_program(output, target, options, &main_name(target))?;
    compiled(composed, Some(program))
}

/// Run a composition over `input`, writing the document to `out`, through
/// the plumbing a program runs on ([`export::run_program`]), from the
/// value at the job's path. A tree's contract stands in front where the
/// composition says; `metrics` collects what the program's stages report,
/// the high-water mark of what they retain among it.
pub fn run(
    job: &Job,
    composition: &Composition,
    records: Records,
    input: Input<'_>,
    out: Box<dyn Write + Send>,
    metrics: Arc<Metrics>,
) -> Result<(), ExportError> {
    // A program's output is bounded where the job says (`--max-output`);
    // a render's follows its input.
    let limits = Limits {
        max_output_bytes: job.max_output,
        ..Limits::default()
    };
    export::run_program(job, input, out, records, |pipe, abort| {
        // The writer stage is aless's, as for a program (`alchemy::run`):
        // the program's own coalescing writer, with no budget, so every
        // fragment reaches the pipe as it comes and a record is on the
        // output the moment it ends, rather than held, up to 32 KB of
        // them, until the run's flush that a failure never makes.
        let out = WriteOut::new(pipe)
            .with_budget(0)
            .with_limits(&limits)
            .with_metrics(metrics.clone());
        let sink = composition
            .program
            .with_abort(abort)
            .sink_out(Box::new(out), None, &limits, metrics.clone())
            .map_err(|fail| ExportError::Program(Box::new(fail)))?;
        let sink: Box<dyn Sink + Send> = if composition.tree {
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
    use std::sync::Mutex;
    use tabnas_alchemy::translate::Alc;

    /// A writer the test reads back after the run took it.
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

    impl Shared {
        fn text(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
        }
    }

    fn job(name: &str, format: Format, what: What, compact: bool) -> Job {
        Job {
            name: name.into(),
            origin: name.into(),
            format,
            what,
            path: Vec::new(),
            compact,
            indent: 2,
            timeout: None,
            started: None,
            max_output: None,
        }
    }

    fn options() -> Options {
        Options::default()
    }

    /// Every format whose crate ships parts is in the registry with what
    /// its manifest declares, and the names `--render` takes list them all.
    #[test]
    fn the_registry_has_every_crates_parts() {
        let own = [
            ("yaml", Root::Any),
            ("toml", Root::Object),
            ("ini", Root::Object),
            ("xml", Root::Any),
            ("zon", Root::Any),
            ("jsonl", Root::Array),
            ("json5", Root::Any),
        ];
        for (id, root) in own {
            let p = part(id).unwrap_or_else(|| panic!("{id}'s manifest names its render"));
            assert_eq!(p.reads, vec![Shape::Tree], "{id}");
            assert_eq!(p.writes, Shape::Tree, "{id}");
            assert_eq!(p.root, root, "{id}");
            assert!(p.lift.is_none(), "{id}");
            let Render::Alc(alc) = &p.render else {
                panic!("{id} renders through its own file");
            };
            assert_eq!(alc.file, format!("tabnas-{id}/alchemy/render.alc"));
            assert_eq!(alc.entry, format!("{id}-render"), "{id}");
            assert!(alc.text.contains(&format!("def {id}-render [")), "{id}");
            assert_eq!(
                records(p),
                if id == "jsonl" {
                    Records::Lines
                } else {
                    Records::Any
                }
            );
        }
        let xml = part("xml").unwrap();
        assert_eq!(xml.schema.as_deref(), Some("xml-element"));
        let embed = xml.embed.as_ref().expect("XML embeds a plain tree");
        assert_eq!(embed.file, "tabnas-xml/alchemy/embed.alc");
        assert_eq!(embed.entry, "xml-embed");
        assert!(embed.text.contains("def xml-embed ["));
        for id in ["json", "jsonc", "jsonic"] {
            let p = part(id).unwrap_or_else(|| panic!("{id}'s manifest names json"));
            assert_eq!(p.reads, vec![Shape::Tree], "{id}");
            assert_eq!(p.writes, Shape::Tree, "{id}");
            assert_eq!(p.root, Root::Any, "{id}");
            assert_eq!(p.render, Render::Json, "{id}");
            assert_eq!(records(p), Records::Json, "{id}");
        }
        let csv = part("csv").expect("csv's manifest names csv");
        assert_eq!(csv.reads, vec![Shape::Tree]);
        assert_eq!(csv.writes, Shape::Records);
        assert_eq!(csv.root, Root::Array);
        assert_eq!(csv.render, Render::Csv);
        assert_eq!(records(csv), Records::Csv);
        let md = part("markdown").expect("markdown's manifest names its parts");
        assert_eq!(md.reads, vec![Shape::Records, Shape::Tree]);
        assert_eq!(md.writes, Shape::Records);
        assert_eq!(md.root, Root::Array);
        assert_eq!(md.schema.as_deref(), Some("markdown-ast"));
        let lift = md.lift.as_ref().expect("a lift");
        assert_eq!(lift.file, "tabnas-markdown/alchemy/lift.alc");
        assert_eq!(lift.entry, "markdown-lift");
        assert!(lift.text.contains("def markdown-lift ["));
        assert!(matches!(&md.render, Render::Alc(alc) if alc.entry == "markdown-render"));
        assert!(part("yaml")
            .unwrap()
            .loss
            .iter()
            .any(|l| l.starts_with("Comments")));
        assert_eq!(
            part("json").unwrap().loss,
            vec![
                "JSON has no spelling for Infinity or NaN, so a number that is not finite is \
                 written as null."
                    .to_string()
            ],
            "JSON declares its one loss"
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

    /// The routes the parts decide: a tree's render takes a tree's events
    /// as they are, through the root adapter its render needs or the embed
    /// into its schema; a records render takes a lifted format's rows
    /// through its lift, and a tree's through the inferred table, its root
    /// an array; JSON over events as they are, and CSV over a tree's rows,
    /// run on aless's own renderers.
    #[test]
    fn the_route_is_the_one_the_parts_decide() {
        let route = |source: Option<&Part>, id: &str, options: &Options| {
            let t = translation(source, part(id).unwrap(), options)
                .unwrap_or_else(|f| panic!("{id}: {f}"));
            let tree = t.composed().map(|c| c.tree);
            let native = match &t {
                Translation::Native { renderer, .. } => Some(*renderer),
                Translation::Composed(_) => None,
            };
            (native, t.adapters().to_vec(), tree)
        };
        let json = source_part(Format::Json);
        let md = source_part(Format::Markdown);
        let o = options();
        assert_eq!(route(json, "yaml", &o), (None, vec![], Some(true)));
        assert_eq!(route(json, "json5", &o), (None, vec![], Some(true)));
        assert_eq!(
            route(json, "toml", &o),
            (None, vec![Adapter::WrapObject("items".into())], Some(true))
        );
        assert_eq!(
            route(json, "ini", &Options { key: "rows".into() }),
            (None, vec![Adapter::WrapObject("rows".into())], Some(true)),
            "--key names the member"
        );
        assert_eq!(
            route(json, "jsonl", &o),
            (None, vec![Adapter::WrapArray], Some(true))
        );
        assert_eq!(
            route(json, "xml", &o),
            (None, vec![Adapter::Embed], Some(true))
        );
        assert_eq!(
            route(json, "markdown", &o),
            (
                None,
                vec![Adapter::WrapArray, Adapter::InferredTable],
                Some(false)
            )
        );
        assert_eq!(
            route(json, "csv", &o),
            (
                Some(Renderer::Csv),
                vec![Adapter::WrapArray, Adapter::InferredTable],
                None
            )
        );
        for id in ["json", "jsonc", "jsonic"] {
            assert_eq!(route(json, id, &o), (Some(Renderer::Json), vec![], None));
            assert_eq!(route(md, id, &o), (Some(Renderer::Json), vec![], None));
        }
        // A lifted format: its rows through its lift into a records render,
        // its own tree into a tree's, embedded into a schema's.
        assert_eq!(route(md, "markdown", &o), (None, vec![], Some(false)));
        assert_eq!(route(md, "csv", &o), (None, vec![], Some(false)));
        assert_eq!(route(md, "yaml", &o), (None, vec![], Some(true)));
        assert_eq!(
            route(md, "xml", &o),
            (None, vec![Adapter::Embed], Some(true))
        );
        // Below the root a value is a plain tree, whatever the document.
        assert_eq!(
            route(None, "markdown", &o),
            (
                None,
                vec![Adapter::WrapArray, Adapter::InferredTable],
                Some(false)
            )
        );
        let t = translation(json, part("csv").unwrap(), &o).unwrap();
        let csv = part("csv").unwrap();
        assert_eq!(
            t.loss().len(),
            csv.loss.len() + Adapter::WrapArray.loss().len() + Adapter::InferredTable.loss().len(),
            "the adapters' loss is printed"
        );
        assert_eq!(shape_adapter(t.adapters()), Some(&Adapter::InferredTable));
    }

    /// Every pair composes into a program that writes text: each source
    /// part, and a plain tree, into each render.
    #[test]
    fn every_composition_compiles_and_writes_text() {
        let sources: Vec<Option<&Part>> = std::iter::once(None)
            .chain(parts().iter().map(Some))
            .collect();
        for source in &sources {
            for target in parts() {
                let c = compose(*source, target, &options()).unwrap_or_else(|f| {
                    panic!(
                        "{} into {}: {f}",
                        source.map_or("tree", |p| &p.id),
                        target.id
                    )
                });
                assert_eq!(c.program.output(), Output::Text, "{}", target.id);
            }
        }
    }

    /// Under `--alchemy`, the program's output stands where the source's
    /// events would: its events into a tree's render as they are, its
    /// table through `records`; its table into a records render as it is,
    /// its events through the inferred table. Nothing stands in front of a
    /// program's own events.
    #[test]
    fn a_programs_output_is_composed_by_its_shape() {
        let events = Source::new("p.alc", "def export [input] input");
        let table = Source::new(
            "t.alc",
            "def export [input] (table-from-json (record (entry :columns :infer) (entry :rows (path each-index))) input)",
        );
        let shape = |program: Source<'_>, output: Output, id: &str| {
            let c = compose_program(program, output, part(id).unwrap(), &options())
                .unwrap_or_else(|f| panic!("{id}: {f}"));
            assert_eq!(c.program.output(), Output::Text, "{id}");
            assert!(!c.tree, "{id}");
            c.adapters
        };
        assert_eq!(shape(events, Output::JsonEvents, "yaml"), vec![]);
        assert_eq!(shape(events, Output::JsonEvents, "json"), vec![]);
        assert_eq!(
            shape(events, Output::JsonEvents, "toml"),
            vec![Adapter::WrapObject("items".into())]
        );
        assert_eq!(
            shape(events, Output::JsonEvents, "csv"),
            vec![Adapter::WrapArray, Adapter::InferredTable]
        );
        assert_eq!(
            shape(events, Output::JsonEvents, "markdown"),
            vec![Adapter::WrapArray, Adapter::InferredTable]
        );
        assert_eq!(
            shape(table, Output::TableRows, "yaml"),
            vec![Adapter::Records]
        );
        assert_eq!(
            shape(table, Output::TableRows, "json"),
            vec![Adapter::Records]
        );
        assert_eq!(shape(table, Output::TableRows, "csv"), vec![]);
        assert_eq!(shape(table, Output::TableRows, "markdown"), vec![]);
        let json = part("json").unwrap();
        let c = compose_program(table, Output::TableRows, json, &options()).unwrap();
        let mut loss = json.loss.clone();
        loss.extend(Adapter::Records.loss());
        assert_eq!(c.loss, loss);
        let fail =
            compose_program(events, Output::Text, part("yaml").unwrap(), &options()).unwrap_err();
        assert!(fail.message.starts_with("render_of_text"), "{fail}");
        // A program that does not define export is refused by name.
        let fail = compose_program(
            Source::new("n.alc", "def x [a] a"),
            Output::JsonEvents,
            part("yaml").unwrap(),
            &options(),
        )
        .unwrap_err();
        assert!(fail.message.starts_with("no_export: n.alc"), "{fail}");
    }

    /// The two routes on aless's own renderers write what the composed
    /// routes write, byte for byte: CSV over a tree's rows (a root of
    /// every kind, rows of every kind and of mixed kinds, no columns, a
    /// repeated member, the numbers that are not finite), and JSON over
    /// the events as they are, compact, as the composed `json` writes.
    #[test]
    fn the_native_routes_write_what_the_composed_routes_write() {
        let documents: &[(Format, &str)] = &[
            (Format::Json, "[]"),
            (Format::Json, "{}"),
            (Format::Json, "[{}]"),
            (Format::Json, "[{}, {}]"),
            (Format::Json, "1"),
            (Format::Json, "\"a, \\\"b\\\"\\nc\""),
            (Format::Json, "null"),
            (Format::Json, "{\"a\": 1, \"b\": [1, 2]}"),
            (Format::Json, "[{\"a\": 1, \"b\": 2}, {\"b\": 3, \"c\": 4}]"),
            (Format::Json, "[[1, 2], [3], [4, 5, 6]]"),
            (Format::Json, "[1, \"x\", null, true, 1.50, 1e300, -0]"),
            (Format::Json, "[{\"a\": [1, {\"b\": 2}]}, {\"a\": {}}]"),
            (Format::Json, "[{\"a\": 1}, 5, [7, 8]]"),
            (Format::Json, "[1, {\"a\": 1}, [2]]"),
            (Format::Json, "[[1], {\"0\": 2, \"1\": 3}, 4]"),
            (Format::Json, "[{\"a\": 1, \"a\": 2}]"),
            (Format::Json5, "[Infinity, -Infinity, NaN, 1]"),
            (
                Format::Json5,
                "[{a: Infinity, b: [NaN, 1]}, {a: -Infinity}]",
            ),
            (Format::Json5, "{x: NaN}"),
            (Format::Yaml, "- a: .inf\n  b: x\n- a: 2\n"),
            (Format::Csv, "a,b\n1,2\n3,\n"),
            (Format::Jsonl, "{\"a\": 1}\n[2]\n3\n"),
        ];
        let write = |format: Format, text: &str, native: Renderer| -> (String, String) {
            let name = format!("doc.{}", format.name());
            let source = source_part(format);
            let target = part(native.name()).unwrap();
            let composition = compose(source, target, &options()).unwrap();
            let plan = export::plan(format, true).unwrap();
            let input = || match plan {
                export::Plan::Lines => {
                    let reader: Box<dyn BufRead + Send> =
                        Box::new(std::io::Cursor::new(text.as_bytes().to_vec()));
                    Input::Lines(reader)
                }
                _ => Input::Text(text),
            };
            let composed = Shared::default();
            let result = run(
                &job(&name, format, What::Part, true),
                &composition,
                records(target),
                input(),
                Box::new(composed.clone()),
                Metrics::new(),
            );
            let composed = match result {
                Ok(()) => composed.text(),
                Err(e) => format!("{}FAILED {e:?}", composed.text()),
            };
            let natively = Shared::default();
            let result = export::export(
                &job(&name, format, What::Render(native), true),
                input(),
                Box::new(natively.clone()),
            );
            let natively = match result {
                Ok(()) => natively.text(),
                Err(e) => format!("{}FAILED {e:?}", natively.text()),
            };
            (composed, natively)
        };
        for (format, text) in documents {
            for native in [Renderer::Csv, Renderer::Json] {
                let (composed, natively) = write(*format, text, native);
                assert!(
                    !composed.contains("FAILED"),
                    "{} {text:?} as {}: {composed}",
                    format.name(),
                    native.name()
                );
                assert_eq!(
                    natively,
                    composed,
                    "{} {text:?} as {}",
                    format.name(),
                    native.name()
                );
            }
        }
    }

    /// `--key` names the member a root that is not an object is written
    /// under, for a target whose document is a table.
    #[test]
    fn the_key_names_the_member_a_root_is_written_under() {
        let composition = compose(
            source_part(Format::Json),
            part("toml").unwrap(),
            &Options { key: "rows".into() },
        )
        .unwrap();
        let out = Shared::default();
        run(
            &job("doc.json", Format::Json, What::Part, false),
            &composition,
            Records::Any,
            Input::Text("[1, 2]"),
            Box::new(out.clone()),
            Metrics::new(),
        )
        .unwrap();
        assert_eq!(out.text(), "\"rows\" = [ 1, 2 ]\n");
    }

    /// A schema-only target writes only its own tree: a document of another
    /// is refused before any output, with the route that can write it, and
    /// a program's events are composed with its render.
    #[test]
    fn a_schema_only_target_names_the_program_route() {
        let target = Part {
            id: "x".into(),
            reads: vec![Shape::Tree],
            writes: Shape::Tree,
            root: Root::Any,
            schema: Some("x-tree".into()),
            lift: None,
            embed: None,
            render: Render::Alc(Alc {
                file: "tabnas-x/alchemy/render.alc".into(),
                entry: "x-render".into(),
                text: "def x-render [input] (json input)".into(),
            }),
            loss: Vec::new(),
        };
        let fail = translation(source_part(Format::Json), &target, &options()).unwrap_err();
        assert_eq!(fail.code, Code::TargetValueUnrepresentable);
        assert_eq!(
            schema_only(&fail, &target, "in.json").as_deref(),
            Some(
                "--render x: x writes a x-tree tree, the tree its own documents read as, and \
                 in.json is not one; a program that makes one can write it: --alchemy FILE \
                 --render x"
            )
        );
        assert_eq!(
            schema_only(
                &Fail::new(Code::TargetValueUnrepresentable, "other"),
                &target,
                "a"
            ),
            None
        );
        let c = compose_program(
            Source::new("p.alc", "def export [input] input"),
            Output::JsonEvents,
            &target,
            &options(),
        )
        .unwrap_or_else(|f| panic!("{f}"));
        assert!(c.adapters.is_empty());
    }

    /// CSV to YAML streams: CSV at the root is read a record at a time, and
    /// the render keeps one marker per open container, so ten times the
    /// rows write ten times the text and leave the retained high-water mark
    /// where it was.
    #[test]
    fn ten_times_the_rows_leave_what_the_render_retains_flat() {
        let composition =
            compose(source_part(Format::Csv), part("yaml").unwrap(), &options()).unwrap();
        let run_rows = |rows: usize| {
            let mut csv = String::from("name,age,city\n");
            for i in 0..rows {
                csv.push_str(&format!("name {i},{i},city {i}\n"));
            }
            let metrics = Metrics::new();
            let reader: Box<dyn BufRead + Send> = Box::new(std::io::Cursor::new(csv.into_bytes()));
            run(
                &job("rows.csv", Format::Csv, What::Part, false),
                &composition,
                Records::Any,
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
            root: Root::Any,
            schema: None,
            lift: None,
            embed: None,
            render: Render::Alc(Alc {
                file: "tabnas-x/alchemy/render.alc".into(),
                entry: "x-render".into(),
                text: "def x-render [input]\n  (nope input)".into(),
            }),
            loss: Vec::new(),
        };
        let fail = compose(None, &broken, &options()).unwrap_err();
        assert!(fail.message.starts_with("unknown_name"), "{fail}");
        assert_eq!(fail.file.as_deref(), Some("tabnas-x/alchemy/render.alc"));
        assert_eq!((fail.row, fail.column), (Some(2), Some(4)));
    }
}
