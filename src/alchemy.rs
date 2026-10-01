//! `--alchemy`: a program in the alchemy language, run over a document.
//!
//! alchemy (`tabnas-alchemy`) is a small typed streaming language in which
//! transducers and renderers are written; a program's `export` takes the
//! document's `JsonEvents` and produces a text, a table or a stream of
//! JSON events. aless compiles the program, asks it what it produces, and
//! drives the document into the sink it builds through the same source
//! plumbing `--render` runs on ([`export::run_program`]): JSON Lines, CSV
//! and TSV a record at a time, a grammar the transducer has verified for
//! incremental streaming as the parse proceeds, pruned under the rows the
//! program's plan names, and every other grammar parsed whole and walked.
//! The output is written as it is produced; what the run holds is what
//! the program retains, under the transducer's limits.
//!
//! What the program produces decides the renderer: a text is written as
//! it is, a table is rendered as CSV unless `--render json` asks for JSON
//! records, and JSON events as JSON unless `--render csv` asks for a
//! table. `--explain` prints the program's plan report instead of running
//! it.
//!
//! Everything here is terminal-free. A failure of the program's own (it
//! does not parse, does not type check, uses a stream twice, or cannot be
//! shown to stream) is the command's mistake and reports as an `alchemy`
//! error. Once the input is open, where a failure came from decides
//! whose it is, and `export` says where ([`ExportError::Program`] for the
//! program's sink, `Transduce` for the source): a failure from the
//! program's sink with one of the language's codes, or with a position,
//! is the program's and is placed in it (the events a program reads
//! carry no positions, so a position on such a failure is in the
//! program's text: `fail` refusing a record, a function refusing a
//! value); one with neither, a renderer's over the rows the program
//! built, say, is reported as the input's, as the source's failures are
//! whatever their code. `headless` applies the rule.

use std::io::Write;
use std::path::{Path, PathBuf};

use tabnas_alchemy::{Output, Program};
use tabnas_transduce::{Code, Fail, Limits, Metrics};

use crate::export::{self, ExportError, Input, Job, Renderer};
use crate::load::{self, LoadError};

/// The name diagnostics give a program from `--alchemy-expr`, which has
/// no file.
pub const EXPR_NAME: &str = "--alchemy-expr";

/// Where a program comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProgramArg {
    /// `--alchemy FILE`.
    File(PathBuf),
    /// `--alchemy-expr TEXT`.
    Expr(String),
}

impl ProgramArg {
    /// The name errors and the program's own diagnostics call it by: the
    /// file's path as given, or [`EXPR_NAME`].
    pub fn name(&self) -> String {
        match self {
            ProgramArg::File(path) => path.display().to_string(),
            ProgramArg::Expr(_) => EXPR_NAME.to_string(),
        }
    }

    /// The program's file, when it has one.
    pub fn path(&self) -> Option<&Path> {
        match self {
            ProgramArg::File(path) => Some(path),
            ProgramArg::Expr(_) => None,
        }
    }

    /// The program's text: a file is read within `max_size`, as any input
    /// is, and an unreadable one fails as an input does; one over the
    /// limit is `too_large` worded for the program
    /// ([`LoadError::program_too_large`]), since no parse of it is coming.
    pub fn read(&self, max_size: Option<u64>) -> Result<String, LoadError> {
        match self {
            ProgramArg::File(path) => load::read_path_within(path, max_size).map_err(|e| {
                if e.is_too_large() {
                    let size = std::fs::metadata(path).ok().map(|m| m.len());
                    LoadError::program_too_large(size, max_size.unwrap_or(0))
                        .with_origin(&load::origin_of(path))
                } else {
                    e
                }
            }),
            ProgramArg::Expr(text) => Ok(text.clone()),
        }
    }
}

/// Compile `text`, named `file` in its diagnostics: parsed, desugared,
/// resolved, checked, and its plan built. The work runs on a thread of
/// alchemy's own stack size, bounded by its evaluation limits.
pub fn compile(text: &str, file: &str) -> Result<Program, Fail> {
    tabnas_alchemy::compile(text, file)
}

/// Whether a failure's code is one the language raises (`DSL_PARSE_ERROR`,
/// `DSL_TYPE_ERROR`, `STREAM_REUSED`, `STREAMABILITY_UNKNOWN`): from a
/// compile, or from the program's sink at run time, the program is at
/// fault and not the input. `STREAMABILITY_UNKNOWN` is the source's code
/// too (a verified grammar that refuses to stream a document part-way),
/// so for a failure met while the program ran the code alone does not
/// say whose it is: `export` says where it came from
/// ([`ExportError::Program`]), and `headless` reads that, the code and
/// the position together ([`is_placed`]).
pub fn is_programs(code: Code) -> bool {
    matches!(
        code,
        Code::DslParseError | Code::DslTypeError | Code::StreamReused | Code::StreamabilityUnknown
    )
}

/// Whether a failure the program's sink returned ([`ExportError::Program`])
/// is the program's, to be placed in the program with the input named
/// beside it: one with a code of the language's (a `match` no case takes,
/// the evaluator's `recursion`), or with a position, whatever its code
/// (`fail` refusing a record, a function refusing a value: the events a
/// program reads carry no positions, so a position on such a failure is
/// in the program's text). One with neither (a renderer's `MISSING_VALUE`
/// over the rows the program built, a table with no columns) is reported
/// as the input's. The source's failures never come this way.
pub fn is_placed(fail: &Fail) -> bool {
    is_programs(fail.code) || fail.row.is_some()
}

/// What `--render` may ask of a program: nothing of one that renders its
/// own text. A format's own render takes a program's table or JSON events
/// through the composition `translate::compose_program` builds; a program
/// that renders its own text takes none. The message is the usage error's.
pub fn check_render(program: &Program, render: Option<Renderer>) -> Result<(), String> {
    match (program.output(), render) {
        (Output::Text, Some(renderer)) => Err(format!(
            "--render {} was given, but the program renders its own text ({}): drop --render, \
             or export a table or JSON events for aless to render",
            renderer.name(),
            program.file()
        )),
        _ => Ok(()),
    }
}

/// alchemy's renderer for aless's built-in; `None` for a part's, whose
/// program runs through the composition instead.
fn renderer(render: Renderer) -> Option<tabnas_alchemy::Renderer> {
    match render {
        Renderer::Csv => Some(tabnas_alchemy::Renderer::Csv),
        Renderer::Json => Some(tabnas_alchemy::Renderer::Json),
        Renderer::Part(_) => None,
    }
}

/// Run `program` over `input`, writing what it produces to `out`; `render`
/// chooses the renderer for a table or JSON events (the program's default
/// when `None`). The document reaches the program's sink the way an export
/// reaches its renderer ([`export::run_program`]), with the transducer's
/// default limits and the program's abort flag handed to it, which the
/// deadline's alarm raises in every mode, so `--timeout` stops a long
/// computation on one item as it stops a parse. A failure the program's
/// sink raised comes back as [`ExportError::Program`], the source's as
/// [`ExportError::Transduce`]; [`is_placed`] says which of the former
/// are the program's own.
pub fn run(
    job: &Job,
    program: &Program,
    render: Option<Renderer>,
    input: Input<'_>,
    out: Box<dyn Write + Send>,
) -> Result<(), ExportError> {
    let limits = Limits::default();
    export::run_program(job, input, out, |pipe, abort| {
        // A sink that cannot be built (a renderer that does not fit what
        // the program exports) is the program's side's failure too.
        program
            .with_abort(abort)
            .sink(pipe, render.and_then(renderer), &limits, Metrics::new())
            .map_err(|fail| ExportError::Program(Box::new(fail)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_program_is_named_by_its_file_or_by_the_option() {
        let file = ProgramArg::File(PathBuf::from("p.alc"));
        assert_eq!(file.name(), "p.alc");
        assert_eq!(file.path(), Some(Path::new("p.alc")));
        let expr = ProgramArg::Expr("def export [input] (json input)".into());
        assert_eq!(expr.name(), EXPR_NAME);
        assert_eq!(expr.path(), None);
        assert_eq!(
            expr.read(Some(4)).unwrap(),
            "def export [input] (json input)",
            "a program on the command line is not measured against --max-size"
        );
        let missing = ProgramArg::File(PathBuf::from("/nonexistent/p.alc"));
        assert!(missing.read(None).unwrap_err().is_io());
    }

    #[test]
    fn the_programs_own_codes_are_told_from_the_inputs() {
        for code in [
            Code::DslParseError,
            Code::DslTypeError,
            Code::StreamReused,
            Code::StreamabilityUnknown,
        ] {
            assert!(is_programs(code), "{code:?}");
        }
        for code in [
            Code::InputInvalid,
            Code::InputOrderViolation,
            Code::ResourceLimitExceeded,
            Code::Aborted,
            Code::OutputFailed,
            Code::ProtocolOrderError,
        ] {
            assert!(!is_programs(code), "{code:?}");
        }
    }

    /// A failure from the program's sink is the program's by its code or
    /// by its position; with neither it is reported as the input's.
    #[test]
    fn a_sinks_failure_is_placed_in_the_program_by_code_or_position() {
        let recursion = Fail::new(Code::StreamabilityUnknown, "recursion: ...");
        assert!(is_placed(&recursion), "a language code, no position");
        let mut refused = Fail::new(Code::InputInvalid, "no Bob");
        assert!(!is_placed(&refused), "the transducer's code, no position");
        refused.row = Some(3);
        refused.column = Some(14);
        assert!(is_placed(&refused), "the transducer's code at a form");
        let missing = Fail::new(Code::MissingValue, "row 2 has no value");
        assert!(
            !is_placed(&missing),
            "a renderer's, over the program's rows"
        );
        let limit = Fail::new(Code::ResourceLimitExceeded, "max_columns");
        assert!(!is_placed(&limit));
    }

    #[test]
    fn render_is_refused_for_a_program_that_renders_its_own_text() {
        let text = compile("def export [input] (json input)", "t.alc").unwrap();
        assert_eq!(text.output(), Output::Text);
        assert!(check_render(&text, None).is_ok());
        let e = check_render(&text, Some(Renderer::Json)).unwrap_err();
        assert!(e.contains("--render json") && e.contains("t.alc"), "{e}");
        let events = compile("def export [input] input", "e.alc").unwrap();
        assert_eq!(events.output(), Output::JsonEvents);
        assert!(check_render(&events, Some(Renderer::Csv)).is_ok());
        assert!(check_render(&events, Some(Renderer::Json)).is_ok());
    }

    #[test]
    fn a_program_that_does_not_compile_fails_with_its_own_code() {
        let e = compile("def export [input]\n  (json input", "bad.alc").unwrap_err();
        assert_eq!(e.code, Code::DslParseError);
        assert!(e.row.is_some() && e.column.is_some(), "{e}");
        let e = compile("def export [input] (nope input)", "bad.alc").unwrap_err();
        assert_eq!(e.code, Code::DslTypeError);
        assert!(e.message.starts_with("unknown_name"), "{}", e.message);
    }
}
