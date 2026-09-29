//! Panes: the active tab's document beside what a renderer or an alchemy
//! program makes of it, and the program itself.
//!
//! The screen's pane area holds one pane or several, side by side or
//! stacked. Each pane has a role, what it shows, and a mode, how it shows
//! it:
//!
//! - the input pane shows the active tab: its tree (structure) or its text
//!   (source), as the single view always has;
//! - the output pane shows the active tab written as `--render` or
//!   `--alchemy` would write it, computed in memory within
//!   [`MAX_OUTPUT_BYTES`]: the text (source), or that text read back in its
//!   format as a tree (structure);
//! - the program pane shows the program: its text (source), or its plan
//!   report, `--explain`'s JSON, as a tree (structure).
//!
//! What a pane shows is a [`crate::tab::Tab`], so the keys that move
//! through a tree or scroll a source work in whichever pane has the focus.

use std::io::{self, BufRead, Cursor, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ratatui::layout::Rect;
use tabnas_alchemy::{Output, Program};
use tabnas_transduce::Metrics;

use crate::alchemy::ProgramArg;
use crate::export::{self, ExportError, Input, Job, Plan, Renderer, What};
use crate::load::Format;
use crate::translate;

/// What the output pane keeps of a document's output: 16 MiB, as much as
/// the transducer lets one scalar or one metadata record hold. Past it the
/// text is cut and the pane says so.
pub const MAX_OUTPUT_BYTES: usize = 16 << 20;

/// What a pane shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    /// The active tab's document.
    Input,
    /// That document written through a renderer or a program.
    Output,
    /// The program.
    Program,
}

impl Role {
    /// The role's name in titles, commands and `--panes`.
    pub fn name(self) -> &'static str {
        match self {
            Role::Input => "input",
            Role::Output => "output",
            Role::Program => "program",
        }
    }

    /// A role named in a command or in `--panes`: `out` or `output`,
    /// `program`, `in` or `input`.
    pub fn from_name(name: &str) -> Option<Role> {
        match name.trim().to_ascii_lowercase().as_str() {
            "in" | "input" => Some(Role::Input),
            "out" | "output" => Some(Role::Output),
            "program" | "prog" => Some(Role::Program),
            _ => None,
        }
    }
}

/// How a pane shows its document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneMode {
    /// The tree.
    Structure,
    /// The text.
    Source,
}

impl PaneMode {
    /// The other mode: what `s` switches to.
    pub fn toggled(self) -> PaneMode {
        match self {
            PaneMode::Structure => PaneMode::Source,
            PaneMode::Source => PaneMode::Structure,
        }
    }
}

/// How several panes share the pane area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arrangement {
    /// Left to right, a rule between them.
    SideBySide,
    /// Top to bottom.
    Stacked,
}

impl Arrangement {
    pub fn toggled(self) -> Arrangement {
        match self {
            Arrangement::SideBySide => Arrangement::Stacked,
            Arrangement::Stacked => Arrangement::SideBySide,
        }
    }
}

/// One pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pane {
    pub role: Role,
    pub mode: PaneMode,
}

impl Pane {
    /// A pane as it first opens: a program as its text, a document as its
    /// tree.
    pub fn new(role: Role) -> Pane {
        let mode = match role {
            Role::Program => PaneMode::Source,
            Role::Input | Role::Output => PaneMode::Structure,
        };
        Pane { role, mode }
    }
}

/// Where a pane is drawn: its title row, when there are several panes,
/// its body, and the rule drawn before it when it sits beside another.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaneArea {
    pub title: Option<Rect>,
    pub body: Rect,
    pub rule: Option<Rect>,
}

/// The panes on screen, in role order (input, output, program), how they
/// are arranged, and which has the focus. One input pane, as the viewer
/// has always been, unless panes are asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Workspace {
    pub panes: Vec<Pane>,
    pub arrangement: Arrangement,
    pub focus: usize,
}

impl Default for Workspace {
    fn default() -> Workspace {
        Workspace {
            panes: vec![Pane::new(Role::Input)],
            arrangement: Arrangement::SideBySide,
            focus: 0,
        }
    }
}

impl Workspace {
    /// The focused pane.
    pub fn focused(&self) -> Pane {
        self.panes[self.focus.min(self.panes.len() - 1)]
    }

    /// The focused pane, to change its mode.
    pub fn focused_mut(&mut self) -> &mut Pane {
        let i = self.focus.min(self.panes.len() - 1);
        &mut self.panes[i]
    }

    /// Where the pane with `role` is, if it is open.
    pub fn position(&self, role: Role) -> Option<usize> {
        self.panes.iter().position(|p| p.role == role)
    }

    /// Open a pane for `role`, in its place in role order, and return its
    /// index; one already open stays as it is. The focus stays on the
    /// pane that had it.
    pub fn open(&mut self, role: Role) -> usize {
        if let Some(i) = self.position(role) {
            return i;
        }
        let focused = self.focused().role;
        let at = self.panes.partition_point(|p| p.role < role);
        self.panes.insert(at, Pane::new(role));
        self.focus = self.position(focused).unwrap_or(0);
        at
    }

    /// Close the pane at `index`; the input pane cannot be closed. The
    /// focus moves to the pane before it.
    pub fn close(&mut self, index: usize) -> bool {
        if index >= self.panes.len() || self.panes[index].role == Role::Input {
            return false;
        }
        let focused = self.focused().role;
        self.panes.remove(index);
        self.focus = self
            .position(focused)
            .unwrap_or_else(|| index.saturating_sub(1).min(self.panes.len() - 1));
        true
    }

    /// Give the focus to the next pane, round to the first.
    pub fn cycle(&mut self) {
        self.focus = (self.focus + 1) % self.panes.len();
    }

    /// Each pane's place in `area`: equal shares along the arrangement, the
    /// last taking what does not divide; side by side, a column between
    /// panes for the rule. With several panes each has a title row.
    pub fn areas(&self, area: Rect) -> Vec<PaneArea> {
        let n = self.panes.len() as u16;
        if n <= 1 {
            return vec![PaneArea {
                title: None,
                body: area,
                rule: None,
            }];
        }
        let mut out = Vec::with_capacity(self.panes.len());
        match self.arrangement {
            Arrangement::SideBySide => {
                let rules = n - 1;
                let share = area.width.saturating_sub(rules) / n;
                let mut x = area.x;
                for i in 0..n {
                    let rule = (i > 0 && x < area.right()).then(|| {
                        let r = Rect::new(x, area.y, 1, area.height);
                        x += 1;
                        r
                    });
                    let width = if i + 1 == n {
                        area.right().saturating_sub(x)
                    } else {
                        share
                    };
                    out.push(titled(Rect::new(x, area.y, width, area.height), rule));
                    x = (x + width).min(area.right());
                }
            }
            Arrangement::Stacked => {
                let share = area.height / n;
                let mut y = area.y;
                for i in 0..n {
                    let height = if i + 1 == n {
                        area.bottom().saturating_sub(y)
                    } else {
                        share
                    };
                    out.push(titled(Rect::new(area.x, y, area.width, height), None));
                    y = (y + height).min(area.bottom());
                }
            }
        }
        out
    }
}

/// A pane's area split into its title row and its body.
fn titled(outer: Rect, rule: Option<Rect>) -> PaneArea {
    let title = (outer.height > 0).then(|| Rect::new(outer.x, outer.y, outer.width, 1));
    let body = Rect::new(
        outer.x,
        outer.y + u16::from(title.is_some()),
        outer.width,
        outer.height.saturating_sub(1),
    );
    PaneArea { title, body, rule }
}

/// What the output pane writes the document through.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Through {
    /// `--render`: CSV, JSON, or a format's own render.
    Render(Renderer),
    /// `--alchemy` or `--alchemy-expr`, and the renderer for the table or
    /// the events the program exports (its default when `None`).
    Program {
        arg: ProgramArg,
        render: Option<Renderer>,
    },
}

impl Default for Through {
    /// JSON, as `--render json` writes it.
    fn default() -> Through {
        Through::Render(Renderer::Json)
    }
}

impl Through {
    /// What titles call it: the format, or the program's file.
    pub fn label(&self) -> String {
        match self {
            Through::Render(r) => r.name().to_string(),
            Through::Program { arg, .. } => match arg.path() {
                Some(p) => p
                    .file_name()
                    .map(|f| f.to_string_lossy().into_owned())
                    .unwrap_or_else(|| arg.name()),
                None => arg.name(),
            },
        }
    }
}

/// A document's output, as the output pane holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rendered {
    /// The text, cut at [`MAX_OUTPUT_BYTES`].
    pub text: String,
    /// The format the text reads back in: CSV, JSON, a format's own, or
    /// plain text for a program that renders its own.
    pub format: Format,
    /// Whether the text was cut.
    pub cut: bool,
}

/// Write `source`, read as `format`, through `through`, into memory: the
/// same run `--render` or `--alchemy` makes, with its plan for the format
/// (line by line for JSON Lines and CSV), so what the pane shows is what
/// the command would print, JSON indented by `indent` as `--indent`
/// gives it. `program` is the compiled program a [`Through::Program`]
/// runs. A failure is the message the pane shows.
pub fn render(
    name: &str,
    source: &str,
    format: Format,
    through: &Through,
    program: Option<&Program>,
    indent: usize,
    timeout: Option<Duration>,
) -> Result<Rendered, String> {
    render_within(
        name,
        source,
        format,
        through,
        program,
        indent,
        timeout,
        MAX_OUTPUT_BYTES,
    )
}

/// [`render`] keeping at most `max` bytes of the output.
#[allow(clippy::too_many_arguments)]
fn render_within(
    name: &str,
    source: &str,
    format: Format,
    through: &Through,
    program: Option<&Program>,
    indent: usize,
    timeout: Option<Duration>,
    max: usize,
) -> Result<Rendered, String> {
    let Some(plan) = export::plan(format, true) else {
        return Err(format!(
            "{name} is plain text, which has no values to write: open it with -k to name its format"
        ));
    };
    let (what, out_format) = match through {
        Through::Render(Renderer::Part(id)) => {
            (What::Part, Format::from_name(id).unwrap_or(Format::Text))
        }
        Through::Render(r) => (What::Render(*r), format_of(*r)),
        Through::Program { render, .. } => {
            let program = program.ok_or("the program did not compile")?;
            crate::alchemy::check_render(program, *render)?;
            // A renderer given writes its own format; without one, a
            // table is CSV and events are JSON, the program's defaults.
            let out = match (program.output(), render) {
                (Output::Text, _) => Format::Text,
                (_, Some(r)) => format_of(*r),
                (Output::TableRows, None) => Format::Csv,
                (Output::JsonEvents, None) => Format::Json,
            };
            (
                What::Program {
                    rows: program.row_selector().cloned(),
                },
                out,
            )
        }
    };
    let job = Job {
        name: name.to_string(),
        origin: name.to_string(),
        format,
        what,
        path: Vec::new(),
        compact: false,
        indent,
        timeout,
    };
    let kept = Arc::new(Mutex::new(Vec::new()));
    let cut = Arc::new(AtomicBool::new(false));
    let out = Box::new(Capped {
        kept: kept.clone(),
        cut: cut.clone(),
        max,
    });
    let input = match plan {
        Plan::Lines => {
            let reader: Box<dyn BufRead + Send> = Box::new(Cursor::new(source.as_bytes()));
            Input::Lines(reader)
        }
        _ => Input::Text(source),
    };
    let result = match through {
        Through::Render(Renderer::Part(id)) => {
            let part = translate::part(id).ok_or_else(|| format!("no render for {id}"))?;
            let composed = translate::compose(part).map_err(|fail| fail.to_string())?;
            translate::run(&job, &composed, input, out, Metrics::new())
        }
        Through::Render(_) => export::export(&job, input, out),
        Through::Program { render, .. } => {
            let program = program.ok_or("the program did not compile")?;
            crate::alchemy::run(&job, program, *render, input, out)
        }
    };
    let cut = cut.load(Ordering::Relaxed);
    match result {
        Ok(()) => {}
        // The cap stops the run as a reader that went away does.
        Err(ExportError::ReaderGone) if cut => {}
        Err(e) => return Err(failure(e)),
    }
    let bytes = std::mem::take(&mut *kept.lock().unwrap_or_else(|e| e.into_inner()));
    Ok(Rendered {
        text: String::from_utf8_lossy(&bytes).into_owned(),
        format: out_format,
        cut,
    })
}

/// The format a built-in renderer's text reads back in.
fn format_of(r: Renderer) -> Format {
    match r {
        Renderer::Csv => Format::Csv,
        Renderer::Json => Format::Json,
        Renderer::Part(id) => Format::from_name(id).unwrap_or(Format::Text),
    }
}

/// A run's failure as the pane says it.
fn failure(e: ExportError) -> String {
    match e {
        ExportError::Usage(m) => m,
        ExportError::NotFound { message, .. } => message,
        ExportError::Load { error, .. } => error.to_string(),
        ExportError::Program(fail) | ExportError::Transduce(fail) => fail.to_string(),
        ExportError::ReaderGone => "the output could not be kept".to_string(),
    }
}

/// A writer into memory that stops the run at `max` bytes: it keeps what
/// fits, marks the text cut, and answers as a pipe whose reader has gone,
/// which ends an export at once.
struct Capped {
    kept: Arc<Mutex<Vec<u8>>>,
    cut: Arc<AtomicBool>,
    max: usize,
}

impl Write for Capped {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut kept = self.kept.lock().unwrap_or_else(|e| e.into_inner());
        let room = self.max.saturating_sub(kept.len());
        if buf.len() > room {
            kept.extend_from_slice(&buf[..room]);
            self.cut.store(true, Ordering::Relaxed);
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "the output pane keeps no more",
            ));
        }
        kept.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roles(w: &Workspace) -> Vec<Role> {
        w.panes.iter().map(|p| p.role).collect()
    }

    #[test]
    fn panes_open_in_role_order_and_the_input_stays() {
        let mut w = Workspace::default();
        assert_eq!(roles(&w), [Role::Input]);
        assert_eq!(w.open(Role::Program), 1);
        assert_eq!(w.open(Role::Output), 1);
        assert_eq!(roles(&w), [Role::Input, Role::Output, Role::Program]);
        assert_eq!(w.focused().role, Role::Input, "opening moves no focus");
        assert_eq!(
            w.panes[2].mode,
            PaneMode::Source,
            "a program opens as its text"
        );
        assert_eq!(w.open(Role::Output), 1, "an open pane stays as it is");
        w.cycle();
        w.cycle();
        assert_eq!(w.focused().role, Role::Program);
        assert!(!w.close(0), "the input pane stays");
        assert!(w.close(2));
        assert_eq!(w.focused().role, Role::Output, "the focus moves back");
        w.cycle();
        assert_eq!(w.focused().role, Role::Input, "cycling goes round");
        assert_eq!(Role::from_name(" OUT "), Some(Role::Output));
        assert_eq!(Role::from_name("tree"), None);
    }

    #[test]
    fn one_pane_takes_the_area_without_a_title() {
        let area = Rect::new(0, 1, 80, 20);
        let w = Workspace::default();
        assert_eq!(
            w.areas(area),
            [PaneArea {
                title: None,
                body: area,
                rule: None
            }]
        );
    }

    #[test]
    fn side_by_side_panes_share_the_width_with_a_rule_between() {
        let mut w = Workspace::default();
        w.open(Role::Output);
        w.open(Role::Program);
        let area = Rect::new(0, 1, 80, 20);
        let a = w.areas(area);
        assert_eq!(a.len(), 3);
        // 78 columns after two rules: 26 each.
        assert_eq!(a[0].title, Some(Rect::new(0, 1, 26, 1)));
        assert_eq!(a[0].body, Rect::new(0, 2, 26, 19));
        assert_eq!(a[0].rule, None);
        assert_eq!(a[1].rule, Some(Rect::new(26, 1, 1, 20)));
        assert_eq!(a[1].body, Rect::new(27, 2, 26, 19));
        assert_eq!(a[2].rule, Some(Rect::new(53, 1, 1, 20)));
        assert_eq!(a[2].body.x, 54);
        assert_eq!(a[2].body.right(), 80, "the last pane takes the remainder");
    }

    #[test]
    fn stacked_panes_share_the_height() {
        let mut w = Workspace {
            arrangement: Arrangement::Stacked,
            ..Workspace::default()
        };
        w.open(Role::Output);
        let a = w.areas(Rect::new(0, 1, 40, 21));
        assert_eq!(a[0].title, Some(Rect::new(0, 1, 40, 1)));
        assert_eq!(a[0].body, Rect::new(0, 2, 40, 9));
        assert_eq!(a[1].title, Some(Rect::new(0, 11, 40, 1)));
        assert_eq!(a[1].body, Rect::new(0, 12, 40, 10));
        assert!(a.iter().all(|p| p.rule.is_none()));
        // Too small to share: nothing drawn outside the area.
        let a = w.areas(Rect::new(0, 0, 3, 1));
        assert!(a.iter().all(|p| p.body.bottom() <= 1));
    }

    #[test]
    fn the_output_is_what_render_would_print() {
        let csv = "name,age\nada,36\n";
        let json = render(
            "t.csv",
            csv,
            Format::Csv,
            &Through::default(),
            None,
            2,
            None,
        )
        .unwrap();
        assert_eq!(json.format, Format::Json);
        assert!(!json.cut);
        let value: serde_json::Value = serde_json::from_str(&json.text).unwrap();
        assert_eq!(value, serde_json::json!([{"name": "ada", "age": "36"}]));
        let back = render(
            "t.json",
            &json.text,
            Format::Json,
            &Through::Render(Renderer::Csv),
            None,
            2,
            None,
        )
        .unwrap();
        assert_eq!(
            (back.text.as_str(), back.format),
            ("\"name\",\"age\"\r\n\"ada\",\"36\"\r\n", Format::Csv)
        );
        let yaml = render(
            "t.csv",
            csv,
            Format::Csv,
            &Through::Render(Renderer::Part("yaml")),
            None,
            2,
            None,
        )
        .unwrap();
        assert_eq!(yaml.format, Format::Yaml);
        assert!(yaml.text.contains("\"name\": \"ada\""), "{}", yaml.text);
        let err = render(
            "t.txt",
            "a\nb\n",
            Format::Text,
            &Through::default(),
            None,
            2,
            None,
        )
        .unwrap_err();
        assert!(err.contains("plain text"), "{err}");
    }

    #[test]
    fn a_programs_output_reads_back_in_its_kind() {
        let program = |text: &str| crate::alchemy::compile(text, "p.alc").unwrap();
        let through = Through::Program {
            arg: ProgramArg::Expr(String::new()),
            render: None,
        };
        let text = program(r#"def export [input] "hi""#);
        let out = render("t.json", "1", Format::Json, &through, Some(&text), 2, None).unwrap();
        assert_eq!((out.text.as_str(), out.format), ("hi", Format::Text));
        let events = program("def export [input] input");
        let out = render(
            "t.json",
            "[1, 2]",
            Format::Json,
            &through,
            Some(&events),
            2,
            None,
        )
        .unwrap();
        assert_eq!(out.format, Format::Json);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&out.text).unwrap(),
            serde_json::json!([1, 2])
        );
        let failed = render(
            "t.json",
            "[1",
            Format::Json,
            &through,
            Some(&events),
            2,
            None,
        );
        assert!(failed.is_err(), "{failed:?}");
        // A renderer given writes its own format: a table rendered as
        // JSON reads back as JSON. Events rendered as CSV the program's
        // runtime refuses, and the pane shows why.
        let table = program(
            "def rows (record (entry :columns :infer) (entry :rows (path each-index)))\n\
             def export [input] (table-from-json rows input)",
        );
        let with = |render| Through::Program {
            arg: ProgramArg::Expr(String::new()),
            render: Some(render),
        };
        let records = r#"[{"a": 1}, {"a": 2}]"#;
        let out = render(
            "t.json",
            records,
            Format::Json,
            &with(Renderer::Json),
            Some(&table),
            2,
            None,
        )
        .unwrap();
        assert_eq!(out.format, Format::Json, "{}", out.text);
        assert!(serde_json::from_str::<serde_json::Value>(&out.text).is_ok());
        let refused = render(
            "t.json",
            records,
            Format::Json,
            &with(Renderer::Csv),
            Some(&events),
            2,
            None,
        )
        .unwrap_err();
        assert!(refused.contains("protocol_mismatch"), "{refused}");
    }

    #[test]
    fn the_output_is_cut_at_its_cap() {
        let long = format!("[{}]", vec!["\"xxxxxxxx\""; 100].join(","));
        let through = Through::default();
        let out =
            render_within("big.json", &long, Format::Json, &through, None, 2, None, 64).unwrap();
        assert!(out.cut);
        assert_eq!(out.text.len(), 64);
        let whole = render_within(
            "big.json",
            &long,
            Format::Json,
            &through,
            None,
            2,
            None,
            1 << 20,
        );
        assert!(!whole.unwrap().cut);
    }

    #[test]
    fn a_through_is_labelled_by_its_format_or_its_file() {
        assert_eq!(Through::default().label(), "json");
        let program = Through::Program {
            arg: ProgramArg::File("dir/export.alc".into()),
            render: None,
        };
        assert_eq!(program.label(), "export.alc");
    }
}
