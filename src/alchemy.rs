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
//! error; every other failure reports as an export's does.

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
    /// is, and an unreadable or oversized one fails as an input does.
    pub fn read(&self, max_size: Option<u64>) -> Result<String, LoadError> {
        match self {
            ProgramArg::File(path) => load::read_path_within(path, max_size),
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

/// Whether a failure is the program's own: its code is one the language
/// raises (`DSL_PARSE_ERROR`, `DSL_TYPE_ERROR`, `STREAM_REUSED`,
/// `STREAMABILITY_UNKNOWN`), so the program is at fault and not the
/// input.
pub fn is_programs(code: Code) -> bool {
    matches!(
        code,
        Code::DslParseError | Code::DslTypeError | Code::StreamReused | Code::StreamabilityUnknown
    )
}

/// What `--render` may ask of a program: nothing of one that renders its
/// own text. The message is the usage error's.
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

/// alchemy's renderer for aless's.
fn renderer(render: Renderer) -> tabnas_alchemy::Renderer {
    match render {
        Renderer::Csv => tabnas_alchemy::Renderer::Csv,
        Renderer::Json => tabnas_alchemy::Renderer::Json,
    }
}

/// Run `program` over `input`, writing what it produces to `out`; `render`
/// chooses the renderer for a table or JSON events (the program's default
/// when `None`). The document reaches the program's sink the way an export
/// reaches its renderer ([`export::run_program`]), with the transducer's
/// default limits and the run's abort flag handed to the program, so
/// `--timeout` stops a long computation on one item as it stops a parse.
pub fn run(
    job: &Job,
    program: &Program,
    render: Option<Renderer>,
    input: Input<'_>,
    out: Box<dyn Write + Send>,
) -> Result<(), ExportError> {
    let limits = Limits::default();
    export::run_program(job, input, out, |pipe, abort| {
        program
            .with_abort(abort)
            .sink(pipe, render.map(renderer), &limits, Metrics::new())
            .map_err(|fail| ExportError::Transduce(Box::new(fail)))
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
