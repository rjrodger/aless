//! The application: tabs, modes, the key map and the command line. Pure
//! state — the terminal is driven from `main.rs`, which feeds [`Input`]s in
//! and paints what [`crate::render`] draws from the state.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ratatui::layout::Rect;
use tabnas_alchemy::Program;

use crate::doc::{Kind, NodeId};
use crate::fmt;
use crate::load::{self, Format, LoadError, Loaded};
use crate::pane::{self, Arrangement, Pane, PaneArea, PaneMode, Role, Through, Workspace};
use crate::search::{self, Direction};
use crate::tab::{Reposition, Stamp, Tab, View};

// ----- input ---------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyCode {
    Char(char),
    Enter,
    Esc,
    Backspace,
    Tab,
    BackTab,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Delete,
    F(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Key {
    pub code: KeyCode,
    pub ctrl: bool,
    pub alt: bool,
}

impl Key {
    pub fn ch(c: char) -> Key {
        Key {
            code: KeyCode::Char(c),
            ctrl: false,
            alt: false,
        }
    }

    pub fn ctrl(c: char) -> Key {
        Key {
            code: KeyCode::Char(c),
            ctrl: true,
            alt: false,
        }
    }

    pub fn code(code: KeyCode) -> Key {
        Key {
            code,
            ctrl: false,
            alt: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Input {
    Key(Key),
    Resize(u16, u16),
    /// Mouse wheel, `down` rows (negative = up).
    Wheel(i32),
    /// Left click on a screen cell, column then row.
    Click(u16, u16),
    FileChanged(PathBuf),
    Tick(Instant),
}

// ----- modes -----------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptKind {
    Command,
    Search(Direction),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Browse,
    Prompt(PromptKind),
    /// The help text, the printed value of a `p` command, and so on. A
    /// pane showing its text is not a mode of the app's but the pane's
    /// ([`PaneMode::Source`]).
    Overlay,
}

#[derive(Clone, Debug, Default)]
pub struct Prompt {
    pub buf: String,
    /// Cursor position in characters.
    pub cursor: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pending {
    Z,
    Yank,
    Print,
}

#[derive(Clone, Debug)]
pub struct Overlay {
    pub title: String,
    pub lines: Vec<String>,
    pub scroll: usize,
    /// Columns panned right (`l` / `h`), for lines wider than the screen.
    pub xoff: usize,
    /// The lines carry ANSI colour codes to be turned into styles (an
    /// engine error report). Off for anything taken from a document.
    pub ansi: bool,
}

impl Overlay {
    fn new(title: String, lines: Vec<String>, ansi: bool) -> Overlay {
        Overlay {
            title,
            lines,
            scroll: 0,
            xoff: 0,
            ansi,
        }
    }

    /// The widest line, in the columns the screen gives it
    /// ([`crate::render::cols`]), which is how far a pan can go.
    fn width(&self) -> usize {
        self.lines
            .iter()
            .map(|l| {
                if self.ansi {
                    crate::render::cols(&load::strip_ansi(l))
                } else {
                    crate::render::cols(l)
                }
            })
            .max()
            .unwrap_or(0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub text: String,
    pub error: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum YankTarget {
    Pretty,
    Line,
    Str,
    Key,
    PathDot,
    PathBracket,
    PathJq,
}

impl YankTarget {
    pub fn describe(self) -> &'static str {
        match self {
            YankTarget::Pretty => "pretty-printed value",
            YankTarget::Line => "one-line value",
            YankTarget::Str => "string contents",
            YankTarget::Key => "key",
            YankTarget::PathDot => "path",
            YankTarget::PathBracket => "bracketed path",
            YankTarget::PathJq => "jq path",
        }
    }
}

/// Requests for the terminal side, produced by handling input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    Watch(PathBuf),
    Unwatch(PathBuf),
    /// Put text on the clipboard; the description names what it is.
    Copy {
        text: String,
        what: String,
    },
    Suspend,
}

#[derive(Clone, Debug)]
pub struct Options {
    pub scrolloff: usize,
    /// New tabs watch their file.
    pub watch: bool,
    /// New tabs start in line mode.
    pub line_mode: bool,
    pub numbers: bool,
    pub relative: bool,
    pub ascii: bool,
    pub indent: usize,
    /// Fold new documents to this depth (`None`: everything expanded).
    pub depth: Option<u32>,
    pub color: bool,
    /// The explorer shows dot-files.
    pub show_hidden: bool,
    /// Panes open at the start beside the input (`--panes`).
    pub panes: Vec<Role>,
    /// Several panes stacked rather than side by side (`--stacked`).
    pub stacked: bool,
    /// What the output pane writes the document through: `--render`,
    /// `--alchemy` or `--alchemy-expr` given with `--panes`, JSON when
    /// none is.
    pub through: Through,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            scrolloff: 3,
            watch: true,
            line_mode: false,
            numbers: false,
            relative: false,
            ascii: false,
            indent: 2,
            depth: None,
            color: true,
            show_hidden: false,
            panes: Vec::new(),
            stacked: false,
            through: Through::default(),
        }
    }
}

/// The output pane's document, and what it was computed from.
struct OutputView {
    tab: Tab,
    /// The input tab's id and generation, and the program's generation.
    from: (u64, u64, u64),
}

/// The program behind the output pane, as the program pane shows it.
struct ProgramView {
    /// The program's text, a line to a row.
    text: Tab,
    /// Its plan report, `--explain`'s JSON, as a tree; or why it has none.
    plan: Tab,
    /// The program the output pane runs; `None` when it did not compile.
    compiled: Option<Program>,
    /// Bumped each time the program is read again, so the output follows.
    generation: u64,
    /// The file as last read, and when a change seen since is due to be
    /// read, as a watched tab's reload is.
    stamp: Option<Stamp>,
    due: Option<Instant>,
}

/// Which tab a pane shows, found before the tab is borrowed.
enum Shown {
    Input,
    Output,
    ProgramText,
    ProgramPlan,
}

/// How long after a change notification the reload runs, so a burst of
/// writes (editors save in several steps) reloads once.
pub const RELOAD_DEBOUNCE: Duration = Duration::from_millis(120);

pub struct App {
    pub tabs: Vec<Tab>,
    pub active: usize,
    pub mode: Mode,
    pub prompt: Prompt,
    pub overlay: Option<Overlay>,
    pub message: Option<Message>,
    pub opts: Options,
    pub width: usize,
    pub height: usize,
    pub quit: bool,
    pub effects: Vec<Effect>,
    count: Option<usize>,
    pending: Option<Pending>,
    last_jump: Option<usize>,
    next_id: u64,
    /// The last search line, so an empty `/` repeats it.
    last_search: Option<String>,
    /// The panes on screen: one input pane unless others were asked for.
    pub workspace: Workspace,
    output: Option<OutputView>,
    program: Option<ProgramView>,
}

impl App {
    pub fn new(opts: Options, width: u16, height: u16) -> App {
        App {
            tabs: Vec::new(),
            active: 0,
            mode: Mode::Browse,
            prompt: Prompt::default(),
            overlay: None,
            message: None,
            opts,
            width: width as usize,
            height: height as usize,
            quit: false,
            effects: Vec::new(),
            count: None,
            pending: None,
            last_jump: None,
            next_id: 1,
            last_search: None,
            workspace: Workspace::default(),
            output: None,
            program: None,
        }
        .with_panes()
    }

    /// The panes `opts` asks for at the start.
    fn with_panes(mut self) -> App {
        if self.opts.stacked {
            self.workspace.arrangement = Arrangement::Stacked;
        }
        for role in self.opts.panes.clone() {
            self.workspace.open(role);
        }
        self
    }

    // ----- layout ----------------------------------------------------------------

    /// Rows taken by the tab strip (shown only with several tabs).
    pub fn strip_rows(&self) -> usize {
        usize::from(self.tabs.len() > 1)
    }

    /// Rows available to the tree pane.
    pub fn pane_height(&self) -> usize {
        self.height.saturating_sub(2 + self.strip_rows()).max(1)
    }

    /// Rows the error panel takes at the foot of the pane: shown while the
    /// active tab's file fails to parse but its last good document is
    /// still on screen (a broken save while watching). A tab that never
    /// loaded shows its report in the whole pane instead.
    ///
    /// This holds while an overlay or the source view covers the pane,
    /// though the panel is not drawn then: the tree keeps the height it
    /// will be shown at again, so the tab's scroll does not move.
    pub fn error_panel_rows(&self) -> usize {
        let pane = self.workspace.focused();
        match self.pane_tab_ref(pane) {
            Some(tab) => panel_rows(tab, self.body_height()),
            None => 0,
        }
    }

    /// Rows the tree itself gets in the focused pane.
    pub fn tree_height(&self) -> usize {
        self.body_height()
            .saturating_sub(self.error_panel_rows())
            .max(1)
    }

    /// Each pane's place on screen: its title row when there are several,
    /// its body, and the rule before it side by side.
    pub fn pane_areas(&self) -> Vec<PaneArea> {
        let cells = |n: usize| u16::try_from(n).unwrap_or(u16::MAX);
        let area = Rect::new(
            0,
            cells(self.strip_rows()),
            cells(self.width),
            cells(self.pane_height()),
        );
        self.workspace.areas(area)
    }

    /// Rows the focused pane's body has.
    fn body_height(&self) -> usize {
        let areas = self.pane_areas();
        let focus = self.workspace.focus.min(areas.len().saturating_sub(1));
        areas
            .get(focus)
            .map(|a| usize::from(a.body.height))
            .unwrap_or(0)
    }

    pub fn view(&self) -> View {
        View::new(self.tree_height(), self.opts.scrolloff)
    }

    /// The tab the focused pane shows: the active tab in the input pane,
    /// the output or the program in theirs. The keys that move, fold,
    /// search and copy work on it.
    pub fn tab(&mut self) -> &mut Tab {
        let pane = self.workspace.focused();
        self.pane_tab(pane)
    }

    /// The active input tab, whichever pane has the focus: the tab that
    /// reloads, watches, closes, explores and changes format.
    pub fn input(&mut self) -> &mut Tab {
        let i = self.active.min(self.tabs.len().saturating_sub(1));
        &mut self.tabs[i]
    }

    /// The active input tab.
    pub fn tab_ref(&self) -> Option<&Tab> {
        self.tabs.get(self.active)
    }

    /// Which tab `pane` shows. An output or program pane whose document is
    /// not built yet shows the input until [`App::prepare`] builds it.
    fn shown(&self, pane: Pane) -> Shown {
        match pane.role {
            Role::Output if self.output.is_some() => Shown::Output,
            Role::Program if self.program.is_some() => match pane.mode {
                PaneMode::Source => Shown::ProgramText,
                PaneMode::Structure => Shown::ProgramPlan,
            },
            _ => Shown::Input,
        }
    }

    /// The tab `pane` shows.
    pub fn pane_tab(&mut self, pane: Pane) -> &mut Tab {
        match self.shown(pane) {
            Shown::Output => &mut self.output.as_mut().expect("shown").tab,
            Shown::ProgramText => &mut self.program.as_mut().expect("shown").text,
            Shown::ProgramPlan => &mut self.program.as_mut().expect("shown").plan,
            Shown::Input => self.input(),
        }
    }

    /// The tab `pane` shows, to draw it.
    pub fn pane_tab_ref(&self, pane: Pane) -> Option<&Tab> {
        match self.shown(pane) {
            Shown::Output => self.output.as_ref().map(|o| &o.tab),
            Shown::ProgramText => self.program.as_ref().map(|p| &p.text),
            Shown::ProgramPlan => self.program.as_ref().map(|p| &p.plan),
            Shown::Input => self.tab_ref(),
        }
    }

    pub fn count_text(&self) -> Option<String> {
        self.count.map(|c| c.to_string())
    }

    /// Bring every pane up to date before a paint: the output and the
    /// program built for what they show, and each pane's rows and window
    /// followed within its own height.
    pub fn prepare(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        self.refresh_panes();
        let areas = self.pane_areas();
        let scrolloff = self.opts.scrolloff;
        for (pane, area) in self.workspace.panes.clone().into_iter().zip(areas) {
            let height = usize::from(area.body.height);
            let tab = self.pane_tab(pane);
            let rows = height.saturating_sub(panel_rows(tab, height)).max(1);
            tab.follow(View::new(rows, scrolloff));
        }
    }

    // ----- panes ---------------------------------------------------------------------

    /// Build what an open output or program pane shows, when it is missing
    /// or what it was built from has changed.
    fn refresh_panes(&mut self) {
        let wants_program = self.workspace.position(Role::Program).is_some()
            || (self.workspace.position(Role::Output).is_some()
                && matches!(self.opts.through, Through::Program { .. }));
        if wants_program && self.program.is_none() {
            self.load_program();
        }
        if self.workspace.position(Role::Output).is_some() {
            self.refresh_output();
        }
    }

    /// Read and compile the program: its text for the program pane's
    /// source, its plan report for its structure, and the compiled program
    /// for the output pane. A program that cannot be read or compiled
    /// shows why in both panes.
    fn load_program(&mut self) {
        let Through::Program { arg, .. } = self.opts.through.clone() else {
            return;
        };
        let name = arg.name();
        let (text, compiled, plan) = match arg.read(load::Limits::current().max_size) {
            Ok(text) => match crate::alchemy::compile(&text, &name) {
                Ok(program) => {
                    let json =
                        serde_json::to_string_pretty(&program.explain_json()).unwrap_or_default();
                    (text, Some(program), Ok(json))
                }
                Err(fail) => (
                    text,
                    None,
                    Err(LoadError::tagged("alchemy", fail.to_string())),
                ),
            },
            Err(e) => (String::new(), None, Err(e)),
        };
        let generation = self.program.as_ref().map_or(0, |p| p.generation + 1);
        let view = self.view();
        let old = self.program.take();
        let text_tab = match load::load_str(text.clone(), Format::Text) {
            Ok(loaded) => self.reuse(old.as_ref().map(|p| p.text.clone()), &name, loaded, view),
            Err(e) => self.error_tab(&name, text, e),
        };
        let plan_title = format!("{name} (plan)");
        let plan_tab = match plan.and_then(|json| load::load_str(json, Format::Json)) {
            Ok(loaded) => self.reuse(
                old.as_ref().map(|p| p.plan.clone()),
                &plan_title,
                loaded,
                view,
            ),
            Err(e) => self.error_tab(&plan_title, String::new(), e),
        };
        let path = arg.path().map(Path::to_path_buf);
        if old.is_none() && self.opts.watch {
            if let Some(p) = &path {
                self.effects.push(Effect::Watch(p.clone()));
            }
        }
        self.program = Some(ProgramView {
            text: text_tab,
            plan: plan_tab,
            compiled,
            generation,
            stamp: path.as_deref().and_then(Stamp::of),
            due: None,
        });
    }

    /// Write the active tab through the renderer or the program, when the
    /// output pane's document is missing or was built from another tab, an
    /// older reading of this one, or another reading of the program. The
    /// pane keeps its place across a rebuild, as a reload does.
    fn refresh_output(&mut self) {
        let program_generation = self.program.as_ref().map_or(0, |p| p.generation);
        let (id, generation) = {
            let t = self.input();
            (t.id, t.generation)
        };
        let from = (id, generation, program_generation);
        if self.output.as_ref().is_some_and(|o| o.from == from) {
            return;
        }
        let through = self.opts.through.clone();
        let arrow = if self.opts.ascii { "->" } else { "→" };
        let input = self.tab_ref().expect("a tab is open");
        let title = format!("{} {arrow} {}", input.title, through.label());
        let result = if input.explorer.is_some() {
            Err("a directory has no output: open a file in it".to_string())
        } else if !input.has_doc {
            Err(format!(
                "{} did not parse, so it has no output",
                input.title
            ))
        } else if let Some(why) = self.program_failure() {
            Err(why)
        } else {
            let compiled = self.program.as_ref().and_then(|p| p.compiled.as_ref());
            pane::render(
                &input.title,
                &input.source,
                input.format,
                &through,
                compiled,
                load::Limits::current().timeout,
            )
        };
        let view = self.view();
        let old = self.output.take().map(|o| o.tab);
        let tab = match result {
            Ok(rendered) => {
                let format = if rendered.cut {
                    Format::Text
                } else {
                    rendered.format
                };
                if rendered.cut {
                    self.error(format!(
                        "The output passed {} MB and was cut: it shows as text",
                        pane::MAX_OUTPUT_BYTES >> 20
                    ));
                }
                match load::load_str(rendered.text.clone(), format) {
                    Ok(loaded) => self.reuse(old, &title, loaded, view),
                    Err(e) => self.error_tab(&title, rendered.text, e),
                }
            }
            Err(message) => {
                self.error_tab(&title, String::new(), LoadError::tagged("output", message))
            }
        };
        self.output = Some(OutputView { tab, from });
    }

    /// Why the program behind the output did not load, when it did not:
    /// what the program pane's plan shows in its place.
    fn program_failure(&self) -> Option<String> {
        let program = self.program.as_ref()?;
        if program.compiled.is_some() || !matches!(self.opts.through, Through::Program { .. }) {
            return None;
        }
        let why = program
            .plan
            .error
            .as_ref()
            .map(|e| e.message.clone())
            .unwrap_or_default();
        Some(format!("the program did not load: {why}"))
    }

    /// A pane's tab for `loaded`: the old one with the document replaced,
    /// keeping its place, or a new one.
    fn reuse(&mut self, old: Option<Tab>, title: &str, loaded: Loaded, view: View) -> Tab {
        match old {
            Some(mut tab) if tab.has_doc => {
                tab.apply(loaded, view);
                tab.title = title.to_string();
                tab.error = None;
                tab
            }
            _ => {
                let id = self.next_id;
                self.next_id += 1;
                let mut tab = Tab::new(id, title.to_string(), None, loaded);
                tab.line_mode = self.opts.line_mode;
                tab
            }
        }
    }

    /// A pane's tab that shows why it has no document.
    fn error_tab(&mut self, title: &str, source: String, err: LoadError) -> Tab {
        let id = self.next_id;
        self.next_id += 1;
        let mut tab = Tab::new(
            id,
            title.to_string(),
            None,
            Loaded {
                doc: crate::doc::Doc::from_lines(&[]),
                format: Format::Text,
                source,
            },
        );
        tab.error = Some(err);
        tab.has_doc = false;
        tab
    }

    /// Open a pane for `role`: the output of the active tab, or the program
    /// behind it, which needs a program to show.
    pub fn open_pane(&mut self, role: Role) {
        if role == Role::Program && !matches!(self.opts.through, Through::Program { .. }) {
            self.error("No program: start aless with --panes and --alchemy FILE");
            return;
        }
        self.workspace.open(role);
        self.prepare();
    }

    /// Close the focused pane; the input pane stays.
    pub fn close_pane(&mut self) {
        let focus = self.workspace.focus;
        if !self.workspace.close(focus) {
            self.error("The input pane stays: q closes its tab");
        }
    }

    /// `:vsplit` and `:split`: arrange the panes, opening the output beside
    /// the input when it is alone.
    fn split(&mut self, arrangement: Arrangement) {
        self.workspace.arrangement = arrangement;
        if self.workspace.panes.len() == 1 {
            self.open_pane(Role::Output);
        }
    }

    fn info(&mut self, text: impl Into<String>) {
        self.message = Some(Message {
            text: text.into(),
            error: false,
        });
    }

    fn error(&mut self, text: impl Into<String>) {
        self.message = Some(Message {
            text: text.into(),
            error: true,
        });
    }

    // ----- opening -------------------------------------------------------------------

    fn adopt(&mut self, mut tab: Tab) {
        let view = self.view();
        if tab.explorer.is_none() {
            tab.line_mode = self.opts.line_mode;
            if let Some(d) = self.opts.depth {
                tab.expand_to_depth(d, view);
            }
        }
        if self.opts.watch && tab.watchable() {
            tab.watch = true;
            if let Some(p) = &tab.path {
                self.effects.push(Effect::Watch(p.clone()));
            }
        }
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
    }

    /// Open a file in a new tab (a failed load still gets a tab, showing
    /// the error and watching for a fix). A directory opens in the
    /// explorer.
    pub fn open_path(&mut self, path: &Path, format: Option<Format>) {
        if path.is_dir() {
            self.open_explorer(path);
            return;
        }
        let id = self.next_id;
        self.next_id += 1;
        let tab = Tab::open(id, path, format);
        if let Some(e) = &tab.error {
            self.error(format!("{}: {}", tab.title, e));
        }
        self.adopt(tab);
    }

    /// Open a directory tree in a new explorer tab.
    pub fn open_explorer(&mut self, dir: &Path) {
        let id = self.next_id;
        self.next_id += 1;
        let tab = Tab::explore(id, dir, self.opts.show_hidden);
        self.adopt(tab);
        self.explorer_sync();
    }

    /// Open in-memory text (stdin) in a new tab.
    pub fn open_source(&mut self, title: &str, source: String, format: Format) {
        match load::load_str(source.clone(), format) {
            Ok(loaded) => {
                let id = self.next_id;
                self.next_id += 1;
                self.adopt(Tab::new(id, title.to_string(), None, loaded));
            }
            Err(err) => self.open_failed_source(title, source, format, err),
        }
    }

    /// Open a tab for in-memory text that could not be loaded (it did not
    /// parse, or was too large to read), showing why.
    pub fn open_failed_source(
        &mut self,
        title: &str,
        source: String,
        format: Format,
        err: load::LoadError,
    ) {
        let id = self.next_id;
        self.next_id += 1;
        let mut tab = Tab::new(
            id,
            title.to_string(),
            None,
            load::Loaded {
                doc: crate::doc::Doc::from_lines(&[]),
                format,
                source,
            },
        );
        let err = err.with_origin(title);
        tab.error = Some(err.clone());
        tab.has_doc = false;
        self.error(format!("{title}: {err}"));
        self.adopt(tab);
    }

    /// The tab shown when nothing was given to open.
    pub fn open_welcome(&mut self) {
        let text = format!(
            r#"{{"aless": "a jless-style viewer for every format the tabnas parsers read",
"version": "{}",
"open": ":open <path> [format]   opens a file in a new tab",
"formats": {},
"keys": "F1 or :help shows the key map; q quits",
"watch": "opened files reload when they change, keeping your place"}}"#,
            env!("CARGO_PKG_VERSION"),
            serde_json::to_string(&Format::known_names()).unwrap_or_default()
        );
        let id = self.next_id;
        self.next_id += 1;
        let loaded = load::load_str(text, Format::Json).expect("welcome text is valid JSON");
        let mut tab = Tab::new(id, "(welcome)".to_string(), None, loaded);
        tab.line_mode = self.opts.line_mode;
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
    }

    // ----- input -----------------------------------------------------------------------

    pub fn handle(&mut self, input: Input) {
        match input {
            Input::Key(key) => self.handle_key(key),
            Input::Resize(w, h) => {
                self.width = w as usize;
                self.height = h as usize;
            }
            Input::Wheel(delta) => {
                let source = self.workspace.focused().mode == PaneMode::Source;
                if self.mode == Mode::Browse && !self.tabs.is_empty() && source {
                    self.scroll_source(delta as isize);
                } else if self.mode == Mode::Browse && !self.tabs.is_empty() {
                    let view = self.view();
                    self.tab().scroll_by(delta as isize, view);
                } else if let Some(o) = self.overlay.as_mut() {
                    o.scroll = (o.scroll as isize + delta as isize).max(0) as usize;
                }
            }
            Input::Click(col, row) => self.click(col, row),
            Input::FileChanged(path) => self.on_file_changed(&path),
            Input::Tick(now) => self.on_tick(now),
        }
        self.prepare();
    }

    /// A click gives the focus to the pane under it and, on a row of a
    /// tree, the focus to that row.
    fn click(&mut self, col: u16, row: u16) {
        if self.mode != Mode::Browse || self.tabs.is_empty() {
            return;
        }
        let at = ratatui::layout::Position::new(col, row);
        let Some((i, area)) = self
            .pane_areas()
            .into_iter()
            .enumerate()
            .find(|(_, a)| a.body.contains(at) || a.title.is_some_and(|t| t.contains(at)))
        else {
            return;
        };
        self.workspace.focus = i;
        if !area.body.contains(at) || self.workspace.focused().mode == PaneMode::Source {
            return;
        }
        let pane_row = usize::from(row - area.body.y);
        if pane_row >= self.tree_height() {
            return;
        }
        let view = self.view();
        let tab = self.tab();
        let target = tab.scroll + pane_row;
        if target < tab.row_count() {
            tab.focus = target;
            tab.follow(view);
        }
    }

    pub fn handle_key(&mut self, key: Key) {
        match self.mode {
            Mode::Browse if self.workspace.focused().mode == PaneMode::Source => {
                self.source_key(key)
            }
            Mode::Browse => self.browse_key(key),
            Mode::Prompt(kind) => self.prompt_key(kind, key),
            Mode::Overlay => self.overlay_key(key),
        }
    }

    fn take_count(&mut self) -> usize {
        self.count.take().unwrap_or(1)
    }

    fn browse_key(&mut self, key: Key) {
        // A new key clears the previous message, unless it only edits the
        // count.
        let digit = match key.code {
            KeyCode::Char(c) if !key.ctrl && !key.alt => c.to_digit(10),
            _ => None,
        };
        if let Some(pending) = self.pending.take() {
            self.message = None;
            self.pending_key(pending, key);
            return;
        }
        if let Some(d) = digit {
            if d != 0 || self.count.is_some() {
                let cur = self.count.unwrap_or(0);
                if cur < 100_000_000 {
                    self.count = Some(cur * 10 + d as usize);
                }
                return;
            }
        }
        self.message = None;
        if self.tabs.is_empty() {
            self.quit = true;
            return;
        }
        let count = self.count.take();
        let n = count.unwrap_or(1);
        let view = self.view();
        let half = (view.height / 2).max(1);
        if self.tab().explorer.is_some() {
            match (key.code, key.ctrl) {
                (KeyCode::Enter, false) => {
                    self.explorer_enter();
                    return;
                }
                (KeyCode::Char('-'), false) => {
                    self.explorer_parent();
                    return;
                }
                // Line mode, bracket matching and the source view mean
                // nothing for a directory tree.
                (KeyCode::Char('m'), false)
                | (KeyCode::Char('%'), false)
                | (KeyCode::Char('s'), false) => return,
                _ => {}
            }
        }
        match (key.code, key.ctrl) {
            (KeyCode::Char('c'), true) => self.quit = true,
            (KeyCode::Char('z'), true) => self.effects.push(Effect::Suspend),
            (KeyCode::Char('w'), true) => self.workspace.cycle(),
            (KeyCode::Char('q'), false) if self.workspace.focused().role != Role::Input => {
                self.close_pane()
            }
            (KeyCode::Char('q'), false) => self.close_tab(),
            (KeyCode::Esc, _) => {}
            (KeyCode::F(1), _) => self.show_help(),
            (KeyCode::Char('!'), false) => self.show_error(),
            (KeyCode::Char(':'), false) => self.start_prompt(PromptKind::Command),
            (KeyCode::Char('/'), false) => {
                self.start_prompt(PromptKind::Search(Direction::Forward))
            }
            (KeyCode::Char('?'), false) => {
                self.start_prompt(PromptKind::Search(Direction::Backward))
            }
            (KeyCode::Char('n'), false) => {
                let msg = self.tab().next_match(n, true, view);
                self.search_message(msg);
            }
            (KeyCode::Char('N'), false) => {
                let msg = self.tab().next_match(n, false, view);
                self.search_message(msg);
            }
            (KeyCode::Char('*'), false) => self.key_search(Direction::Forward, n),
            (KeyCode::Char('#'), false) => self.key_search(Direction::Backward, n),
            (KeyCode::Char('j'), false)
            | (KeyCode::Down, _)
            | (KeyCode::Char('n'), true)
            | (KeyCode::Enter, _) => self.tab().move_down(n, view),
            (KeyCode::Char('k'), false)
            | (KeyCode::Up, _)
            | (KeyCode::Char('p'), true)
            | (KeyCode::Backspace, _) => self.tab().move_up(n, view),
            (KeyCode::Char('h'), false) | (KeyCode::Left, _) => self.tab().move_left(view),
            (KeyCode::Char('l'), false) | (KeyCode::Right, _) => self.tab().move_right(view),
            (KeyCode::Char('H'), false) => self.tab().focus_parent(view),
            (KeyCode::Char('J'), false) => self.tab().sibling(n, true, view),
            (KeyCode::Char('K'), false) => self.tab().sibling(n, false, view),
            (KeyCode::Char('w'), false) => self.tab().depth_change(n, true, view),
            (KeyCode::Char('b'), false) => self.tab().depth_change(n, false, view),
            (KeyCode::Char('0'), false) | (KeyCode::Char('^'), false) => {
                self.tab().first_sibling(view)
            }
            (KeyCode::Char('$'), false) => self.tab().last_sibling(view),
            (KeyCode::Char('g'), false) => {
                if count.is_some() {
                    self.tab().goto_row(n, view);
                } else {
                    self.tab().top(view);
                }
            }
            (KeyCode::Home, _) => self.tab().top(view),
            (KeyCode::Char('G'), false) => {
                if count.is_some() {
                    self.tab().goto_row(n, view);
                } else {
                    self.tab().bottom(view);
                }
            }
            (KeyCode::End, _) => self.tab().bottom(view),
            (KeyCode::Char('f'), true) | (KeyCode::PageDown, _) => self.tab().page(n, true, view),
            (KeyCode::Char('b'), true) | (KeyCode::PageUp, _) => self.tab().page(n, false, view),
            (KeyCode::Char('d'), true) => {
                let d = self.jump_distance(count, half);
                self.tab().jump(d, true, view);
            }
            (KeyCode::Char('u'), true) => {
                let d = self.jump_distance(count, half);
                self.tab().jump(d, false, view);
            }
            (KeyCode::Char('e'), true) => self.tab().scroll_by(n as isize, view),
            (KeyCode::Char('y'), true) => self.tab().scroll_by(-(n as isize), view),
            (KeyCode::Char('z'), false) => self.pending = Some(Pending::Z),
            (KeyCode::Char('y'), false) => self.pending = Some(Pending::Yank),
            (KeyCode::Char('p'), false) => self.pending = Some(Pending::Print),
            (KeyCode::Char(' '), false) => self.tab().toggle(view),
            (KeyCode::Char('c'), false) => self.tab().fold_siblings(false, false, view),
            (KeyCode::Char('C'), false) => self.tab().fold_siblings(false, true, view),
            (KeyCode::Char('e'), false) => self.tab().fold_siblings(true, false, view),
            (KeyCode::Char('E'), false) => self.tab().fold_siblings(true, true, view),
            (KeyCode::Char('m'), false) => self.tab().toggle_mode(view),
            (KeyCode::Char('%'), false) => self.tab().matching_pair(view),
            (KeyCode::Char('.'), false) => self.tab().scroll_value(n as isize),
            (KeyCode::Char(','), false) => self.tab().scroll_value(-(n as isize)),
            (KeyCode::Char(';'), false) => {
                let tab = self.tab();
                tab.xoff = if tab.xoff == 0 { usize::MAX / 4 } else { 0 };
            }
            (KeyCode::Char('<'), false) => self.opts.indent = self.opts.indent.saturating_sub(n),
            (KeyCode::Char('>'), false) => self.opts.indent = (self.opts.indent + n).min(16),
            (KeyCode::Char('r'), false) => self.reload_active(),
            (KeyCode::Char('W'), false) => self.toggle_watch(None),
            (KeyCode::Tab, _) => self.next_tab(n),
            (KeyCode::BackTab, _) => self.prev_tab(n),
            (KeyCode::Char('s'), false) => self.show_source(),
            _ => {}
        }
        self.explorer_sync();
    }

    // ----- explorer ------------------------------------------------------------------

    /// After a fold change in an explorer tab, list what the expanded
    /// directories need.
    fn explorer_sync(&mut self) {
        if self.tabs.is_empty() || self.input().explorer.is_none() {
            return;
        }
        let view = self.view();
        let tab = self.input();
        tab.explorer_sync(view);
        let capped = tab.explorer.as_ref().is_some_and(|ex| ex.capped);
        if capped {
            self.error(format!(
                "Listing stopped at {} directories; collapse some to go on",
                crate::explorer::MAX_LISTED
            ));
        }
    }

    /// `Enter` in the explorer: open a file in a new tab, toggle a
    /// directory.
    fn explorer_enter(&mut self) {
        let view = self.view();
        let tab = self.input();
        let node = tab.focused_node();
        let path = tab.doc.path(node);
        let Some(ex) = tab.explorer.as_ref() else {
            return;
        };
        if path.is_empty() {
            return;
        }
        let fs_path = ex.fs_path(&path);
        match ex.entry(&path).map(|e| e.kind.clone()) {
            Some(crate::explorer::EntryKind::Dir) => {
                tab.toggle(view);
                self.explorer_sync();
            }
            Some(crate::explorer::EntryKind::File) => self.open_path(&fs_path, None),
            Some(crate::explorer::EntryKind::Symlink(_)) => match std::fs::metadata(&fs_path) {
                Ok(m) if m.is_file() || m.is_dir() => self.open_path(&fs_path, None),
                _ => self.error(format!("{}: dangling link", fs_path.display())),
            },
            Some(crate::explorer::EntryKind::Other) => {
                self.error(format!("{}: not a regular file", fs_path.display()))
            }
            None => {}
        }
    }

    /// `-` in the explorer: make the parent directory the root.
    fn explorer_parent(&mut self) {
        let tab = self.input();
        let Some(root) = tab.explorer.as_ref().map(|ex| ex.root.clone()) else {
            return;
        };
        match root.parent() {
            Some(parent) => {
                let parent = parent.to_path_buf();
                self.reroot_active(&parent);
            }
            None => self.error("Already at the top of the filesystem"),
        }
    }

    /// Re-root the active explorer, moving its watch to the new root.
    fn reroot_active(&mut self, dir: &Path) {
        let view = self.view();
        let tab = self.input();
        let old = tab.path.clone();
        tab.reroot(dir, view);
        let new = tab.path.clone();
        let watched = tab.watch;
        if watched && old != new {
            if let Some(p) = old {
                self.effects.push(Effect::Unwatch(p));
            }
            if let Some(p) = new {
                self.effects.push(Effect::Watch(p));
            }
        }
        self.explorer_sync();
    }

    /// `:cd DIR` in an explorer tab: re-root, relative to the current root.
    fn explorer_cd(&mut self, dir: &str) {
        let tab = self.input();
        let Some(root) = tab.explorer.as_ref().map(|ex| ex.root.clone()) else {
            self.open_explorer(Path::new(dir));
            return;
        };
        let target = root.join(dir);
        if !target.is_dir() {
            self.error(format!("{}: not a directory", target.display()));
            return;
        }
        self.reroot_active(&target);
    }

    /// The distance for `C-d`/`C-u`: a typed count sets it and is
    /// remembered, as in vim; otherwise the last one, else half a page.
    fn jump_distance(&mut self, count: Option<usize>, half: usize) -> usize {
        match count {
            Some(n) => {
                self.last_jump = Some(n);
                n
            }
            None => self.last_jump.unwrap_or(half),
        }
    }

    fn pending_key(&mut self, pending: Pending, key: Key) {
        let view = self.view();
        if key.ctrl || key.alt {
            return;
        }
        let KeyCode::Char(c) = key.code else { return };
        match pending {
            Pending::Z => match c {
                'z' => self.tab().reposition(Reposition::Center, view),
                't' => self.tab().reposition(Reposition::Top, view),
                'b' => self.tab().reposition(Reposition::Bottom, view),
                _ => {}
            },
            Pending::Yank => {
                let target = match c {
                    'y' => YankTarget::Pretty,
                    'v' => YankTarget::Line,
                    's' => YankTarget::Str,
                    'k' => YankTarget::Key,
                    'p' => YankTarget::PathDot,
                    'b' => YankTarget::PathBracket,
                    'q' => YankTarget::PathJq,
                    _ => return,
                };
                self.yank(target, false);
            }
            Pending::Print => {
                let target = match c {
                    'p' => YankTarget::Pretty,
                    'v' => YankTarget::Line,
                    's' => YankTarget::Str,
                    'k' => YankTarget::Key,
                    'P' => YankTarget::PathDot,
                    'b' => YankTarget::PathBracket,
                    'q' => YankTarget::PathJq,
                    _ => return,
                };
                self.yank(target, true);
            }
        }
    }

    // ----- yank / print ------------------------------------------------------------

    /// The text a yank target names for the focused node, or why there is
    /// none.
    pub fn yank_text(&mut self, target: YankTarget) -> Result<String, String> {
        let tab = self.tab();
        let node: NodeId = tab.focused_node();
        let doc = &tab.doc;
        let n = doc.node(node);
        if let Some(ex) = &tab.explorer {
            // In the explorer every path target is the filesystem path.
            return Ok(match target {
                YankTarget::Key => match n.key.name() {
                    Some(k) => k.to_string(),
                    None => ex.root.display().to_string(),
                },
                YankTarget::Str => match &n.kind {
                    Kind::Str(s) => s.to_string(),
                    _ => return Err("Focused entry is a directory".to_string()),
                },
                _ => ex.fs_path(&doc.path(node)).display().to_string(),
            });
        }
        Ok(match target {
            YankTarget::Pretty => fmt::to_json_pretty(doc, node, 2),
            YankTarget::Line => fmt::to_json_line(doc, node),
            YankTarget::Str => match &n.kind {
                Kind::Str(s) => s.to_string(),
                _ => return Err("Focused value is not a string".to_string()),
            },
            YankTarget::Key => match n.key.name() {
                Some(k) => {
                    if fmt::is_identifier(k) && !tab.line_mode {
                        k.to_string()
                    } else {
                        fmt::quote(k)
                    }
                }
                None => return Err("Focused node has no key".to_string()),
            },
            YankTarget::PathDot => fmt::path_dot(&doc.path(node)),
            YankTarget::PathBracket => fmt::path_bracket(&doc.path(node)),
            YankTarget::PathJq => fmt::path_jq(&doc.path(node)),
        })
    }

    fn yank(&mut self, target: YankTarget, print: bool) {
        match self.yank_text(target) {
            Ok(text) => {
                if print {
                    let lines = text.lines().map(str::to_string).collect();
                    self.overlay = Some(Overlay::new(
                        format!(
                            "{} — j/k scroll, h/l pan, any other key returns",
                            target.describe()
                        ),
                        lines,
                        false,
                    ));
                    self.mode = Mode::Overlay;
                } else {
                    self.effects.push(Effect::Copy {
                        text,
                        what: target.describe().to_string(),
                    });
                }
            }
            Err(e) => self.error(e),
        }
    }

    /// The terminal side reports how a copy went.
    pub fn copied(&mut self, what: &str, how: Result<&str, String>) {
        match how {
            Ok(via) => self.info(format!("Copied {what} to {via}")),
            Err(e) => self.error(format!("Could not copy {what}: {e}")),
        }
    }

    // ----- prompt ------------------------------------------------------------------------

    fn start_prompt(&mut self, kind: PromptKind) {
        self.prompt = Prompt::default();
        self.mode = Mode::Prompt(kind);
    }

    fn prompt_key(&mut self, kind: PromptKind, key: Key) {
        let p = &mut self.prompt;
        let chars: Vec<char> = p.buf.chars().collect();
        let byte_at = |chars: &[char], idx: usize| -> usize {
            chars[..idx].iter().map(|c| c.len_utf8()).sum()
        };
        match (key.code, key.ctrl) {
            (KeyCode::Esc, _) | (KeyCode::Char('c'), true) => self.mode = Mode::Browse,
            (KeyCode::Enter, _) => {
                let line = std::mem::take(&mut self.prompt.buf);
                self.mode = Mode::Browse;
                match kind {
                    PromptKind::Command => self.run_command(&line),
                    PromptKind::Search(dir) => self.run_search(dir, &line),
                }
            }
            (KeyCode::Backspace, _) | (KeyCode::Char('h'), true) => {
                if chars.is_empty() {
                    self.mode = Mode::Browse;
                } else if p.cursor > 0 {
                    let start = byte_at(&chars, p.cursor - 1);
                    let end = byte_at(&chars, p.cursor);
                    p.buf.replace_range(start..end, "");
                    p.cursor -= 1;
                }
            }
            (KeyCode::Delete, _) => {
                if p.cursor < chars.len() {
                    let start = byte_at(&chars, p.cursor);
                    let end = byte_at(&chars, p.cursor + 1);
                    p.buf.replace_range(start..end, "");
                }
            }
            (KeyCode::Left, _) | (KeyCode::Char('b'), true) => {
                p.cursor = p.cursor.saturating_sub(1)
            }
            (KeyCode::Right, _) | (KeyCode::Char('f'), true) => {
                p.cursor = (p.cursor + 1).min(chars.len())
            }
            (KeyCode::Home, _) | (KeyCode::Char('a'), true) => p.cursor = 0,
            (KeyCode::End, _) | (KeyCode::Char('e'), true) => p.cursor = chars.len(),
            (KeyCode::Char('u'), true) => {
                let end = byte_at(&chars, p.cursor);
                p.buf.replace_range(..end, "");
                p.cursor = 0;
            }
            (KeyCode::Char('k'), true) => {
                let start = byte_at(&chars, p.cursor);
                p.buf.truncate(start);
            }
            (KeyCode::Char('w'), true) => {
                let mut i = p.cursor;
                while i > 0 && chars[i - 1] == ' ' {
                    i -= 1;
                }
                while i > 0 && chars[i - 1] != ' ' {
                    i -= 1;
                }
                let start = byte_at(&chars, i);
                let end = byte_at(&chars, p.cursor);
                p.buf.replace_range(start..end, "");
                p.cursor = i;
            }
            (KeyCode::Char(c), false) if !key.alt => {
                let at = byte_at(&chars, p.cursor);
                p.buf.insert(at, c);
                p.cursor += 1;
            }
            _ => {}
        }
    }

    // ----- search --------------------------------------------------------------------------

    fn search_message(&mut self, msg: String) {
        let error = msg.starts_with("Pattern not found") || msg.starts_with("No previous");
        self.message = Some(Message { text: msg, error });
    }

    fn run_search(&mut self, dir: Direction, line: &str) {
        let line = if line.is_empty() {
            match &self.last_search {
                Some(l) => l.clone(),
                None => {
                    self.error("No previous search");
                    return;
                }
            }
        } else {
            line.to_string()
        };
        match search::compile(&line) {
            Ok(pattern) => {
                self.last_search = Some(line);
                let view = self.view();
                let n = 1;
                let msg = self.tab().search(pattern, dir, n, view);
                self.search_message(msg);
            }
            Err(e) => self.error(e),
        }
    }

    fn key_search(&mut self, dir: Direction, n: usize) {
        let view = self.view();
        let tab = self.tab();
        let node = tab.focused_node();
        let Some(key) = tab.doc.node(node).key.name().map(str::to_string) else {
            self.error("Focused node has no key");
            return;
        };
        let pattern = search::key_pattern(&key);
        self.last_search = Some(pattern.input.clone());
        let msg = self.tab().search(pattern, dir, n, view);
        self.search_message(msg);
    }

    // ----- commands ------------------------------------------------------------------------

    pub fn run_command(&mut self, line: &str) {
        let line = line.trim();
        let mut parts = line.splitn(2, char::is_whitespace);
        let cmd = parts.next().unwrap_or("").trim();
        let rest = parts.next().unwrap_or("").trim();
        let view = self.view();
        if cmd.is_empty() {
            return;
        }
        if let Ok(n) = cmd.parse::<usize>() {
            self.tab().goto_row(n, view);
            return;
        }
        // `:w!` and `:q!` carry their bang on the word.
        let (cmd, bang) = match cmd.strip_suffix('!') {
            Some(c) => (c, true),
            None => (cmd, false),
        };
        match cmd {
            // Document-only commands mean nothing for a directory tree.
            "source" | "src" | "mode" | "format" | "kind" | "ft" | "filetype"
                if self.tab().explorer.is_some() =>
            {
                self.error(format!(":{cmd} applies to a document, not to the explorer"))
            }
            "q" | "close" | "tabclose" => self.close_tab(),
            "qa" | "qall" | "quit" | "quitall" | "exit" => self.quit = true,
            "h" | "help" => self.show_help(),
            "error" | "errors" | "err" => self.show_error(),
            "set" | "se" => self.set_option(rest),
            "w" | "write" => self.write_file(rest, bang),
            // `:e!` (the bang was split off above) reloads, as in vim.
            "e" | "edit" if bang && rest.is_empty() => self.reload_active(),
            "open" | "o" | "e" | "edit" | "tabnew" | "tabe" | "tabedit" => self.cmd_open(rest),
            "tab" | "tabn" | "tabnext" | "next" | "n" if !rest.is_empty() => {
                match rest.parse::<usize>() {
                    Ok(n) if n >= 1 => self.goto_tab(n - 1),
                    _ => self.error(format!("Not a tab number: {rest}")),
                }
            }
            "tabn" | "tabnext" | "next" | "n" => self.next_tab(1),
            "tabp" | "tabprev" | "tabprevious" | "prev" | "p" => self.prev_tab(1),
            "tabfirst" | "tabfir" => self.goto_tab(0),
            "tablast" | "tabl" => {
                let last = self.tabs.len().saturating_sub(1);
                self.goto_tab(last);
            }
            "watch" => match rest {
                "" | "on" | "true" | "1" => self.toggle_watch(Some(true)),
                "off" | "false" | "0" => self.toggle_watch(Some(false)),
                _ => self.error("usage: :watch [on|off]"),
            },
            "r" | "reload" => self.reload_active(),
            "format" | "kind" | "ft" | "filetype" => match Format::from_name(rest) {
                Some(f) => match self.input().reformat(f, view) {
                    Ok(()) => self.info(format!("Parsed as {f}")),
                    Err(e) => self.error(format!("{f}: {e} — ! shows the report")),
                },
                None => self.error(format!(
                    "Unknown format: {rest} (one of {})",
                    Format::known_names().join(", ")
                )),
            },
            "depth" | "fold" => match rest.parse::<u32>() {
                Ok(d) => self.tab().expand_to_depth(d, view),
                Err(_) => self.error("usage: :depth <n>"),
            },
            "expand" | "expandall" => self.tab().fold_siblings(true, true, view),
            "collapse" | "collapseall" => {
                let tab = self.tab();
                tab.top(view);
                tab.fold_siblings(false, true, view);
            }
            "line" | "goto" => match rest.parse::<usize>() {
                Ok(n) => self.tab().goto_row(n, view),
                Err(_) => self.error("usage: :line <n>"),
            },
            "source" | "src" => self.show_source(),
            "explore" | "explorer" | "browse" | "files" => {
                let dir = if rest.is_empty() { "." } else { rest };
                if !Path::new(dir).is_dir() {
                    self.error(format!("{dir}: not a directory"));
                } else {
                    self.open_explorer(Path::new(dir));
                }
            }
            "cd" => {
                if rest.is_empty() {
                    self.error("usage: :cd <dir>");
                } else {
                    self.explorer_cd(rest);
                }
            }
            "mode" => match rest {
                "line" => {
                    if !self.tab().line_mode {
                        self.tab().toggle_mode(view);
                    }
                }
                "data" => {
                    if self.tab().line_mode {
                        self.tab().toggle_mode(view);
                    }
                }
                _ => self.error("usage: :mode data|line"),
            },
            "yank" | "y" => self.yank(YankTarget::Pretty, false),
            "vsplit" | "vs" | "vsp" => self.split(Arrangement::SideBySide),
            "split" | "sp" => self.split(Arrangement::Stacked),
            "arrange" => {
                self.workspace.arrangement = self.workspace.arrangement.toggled();
            }
            "only" | "on" => {
                self.workspace = Workspace {
                    arrangement: self.workspace.arrangement,
                    ..Workspace::default()
                }
            }
            "pane" => match rest {
                "close" => self.close_pane(),
                name => match Role::from_name(name) {
                    Some(Role::Input) | None => self.error("usage: :pane out|program|close"),
                    Some(role) => self.open_pane(role),
                },
            },
            _ => self.error(format!("Unknown command: {cmd}")),
        }
    }

    fn set_option(&mut self, rest: &str) {
        if rest.is_empty() {
            self.error("usage: :set <option>");
            return;
        }
        for word in rest.split_whitespace() {
            let (name, value) = match word.split_once('=') {
                Some((n, v)) => (n, Some(v)),
                None => (word, None),
            };
            match (name, value) {
                ("number" | "nu", None) => self.opts.numbers = true,
                ("nonumber" | "nonu", None) => self.opts.numbers = false,
                ("number!" | "nu!", None) => self.opts.numbers = !self.opts.numbers,
                ("relativenumber" | "rnu", None) => self.opts.relative = true,
                ("norelativenumber" | "nornu", None) => self.opts.relative = false,
                ("relativenumber!" | "rnu!", None) => self.opts.relative = !self.opts.relative,
                ("watch", None) => self.toggle_watch(Some(true)),
                ("nowatch", None) => self.toggle_watch(Some(false)),
                ("ascii", None) => self.opts.ascii = true,
                ("noascii", None) => self.opts.ascii = false,
                ("hidden" | "nohidden" | "hidden!", None) => {
                    let show = match name {
                        "hidden" => true,
                        "nohidden" => false,
                        _ => !self.opts.show_hidden,
                    };
                    self.opts.show_hidden = show;
                    let view = self.view();
                    for t in &mut self.tabs {
                        t.set_show_hidden(show, view);
                    }
                }
                ("scrolloff" | "so", Some(v)) => match v.parse() {
                    Ok(n) => self.opts.scrolloff = n,
                    Err(_) => self.error(format!("Not a number: {v}")),
                },
                ("indent" | "shiftwidth" | "sw", Some(v)) => match v.parse::<usize>() {
                    Ok(n) => self.opts.indent = n.min(16),
                    Err(_) => self.error(format!("Not a number: {v}")),
                },
                ("mode", Some(v)) => self.run_command(&format!("mode {v}")),
                ("depth", Some(v)) => self.run_command(&format!("depth {v}")),
                _ => self.error(format!("Unknown option: {word}")),
            }
        }
    }

    fn cmd_open(&mut self, rest: &str) {
        if rest.is_empty() {
            self.error("usage: :open <path> [format]");
            return;
        }
        // The path may contain spaces; a trailing word that names a format
        // is taken as one.
        let (path, format) = match rest.rsplit_once(char::is_whitespace) {
            Some((p, f)) if Format::from_name(f).is_some() && !Path::new(rest).exists() => {
                (p.trim(), Format::from_name(f))
            }
            _ => (rest, None),
        };
        self.open_path(Path::new(path), format);
    }

    fn write_file(&mut self, rest: &str, bang: bool) {
        if rest.is_empty() {
            self.error("usage: :w[!] <file>  (writes the document as JSON)");
            return;
        }
        let path = Path::new(rest);
        if path.exists() && !bang {
            self.error(format!("{rest} exists; use :w! to overwrite"));
            return;
        }
        let text = {
            let tab = self.tab();
            fmt::to_json_pretty(&tab.doc, 0, 2)
        };
        match std::fs::write(path, text + "\n") {
            Ok(()) => self.info(format!("Wrote {rest}")),
            Err(e) => self.error(format!("{rest}: {e}")),
        }
    }

    // ----- tabs -------------------------------------------------------------------------------

    pub fn close_tab(&mut self) {
        if self.tabs.is_empty() {
            self.quit = true;
            return;
        }
        let tab = self.tabs.remove(self.active);
        if tab.watch {
            if let Some(p) = tab.path {
                self.effects.push(Effect::Unwatch(p));
            }
        }
        if self.tabs.is_empty() {
            self.quit = true;
            return;
        }
        self.active = self.active.min(self.tabs.len() - 1);
    }

    pub fn next_tab(&mut self, n: usize) {
        if !self.tabs.is_empty() {
            self.active = (self.active + n) % self.tabs.len();
        }
    }

    pub fn prev_tab(&mut self, n: usize) {
        if !self.tabs.is_empty() {
            let len = self.tabs.len();
            self.active = (self.active + len - (n % len)) % len;
        }
    }

    pub fn goto_tab(&mut self, i: usize) {
        if i < self.tabs.len() {
            self.active = i;
        } else {
            self.error(format!("No tab {}", i + 1));
        }
    }

    // ----- watching ------------------------------------------------------------------------

    fn toggle_watch(&mut self, want: Option<bool>) {
        let tab = self.input();
        if !tab.watchable() {
            self.error("Nothing to watch: this tab has no file");
            return;
        }
        let on = want.unwrap_or(!tab.watch);
        let path = tab.path.clone().expect("watchable tab has a path");
        if on == tab.watch {
            return;
        }
        tab.watch = on;
        if on {
            tab.stamp = crate::tab::Stamp::of(&path);
            self.effects.push(Effect::Watch(path));
            self.info("Watching");
        } else {
            tab.reload_due = None;
            self.effects.push(Effect::Unwatch(path));
            self.info("Not watching");
        }
    }

    fn reload_active(&mut self) {
        if self.workspace.focused().role == Role::Program {
            self.reload_program();
            return;
        }
        let view = self.view();
        let tab = self.input();
        if !tab.watchable() {
            self.error("Nothing to reload: this tab has no file");
            return;
        }
        tab.reload(view);
        self.report_reload(self.active);
    }

    fn report_reload(&mut self, i: usize) {
        let (title, err, gone, nodes) = {
            let t = &self.tabs[i];
            (t.title.clone(), t.error.clone(), t.gone, t.doc.len())
        };
        match err {
            Some(e) if gone => self.error(format!("{title}: file gone ({e})")),
            Some(e) => self.error(format!("{title}: {e} — ! shows the report")),
            None => self.info(format!(
                "Reloaded {title} ({} nodes)",
                nodes.saturating_sub(1)
            )),
        }
    }

    pub fn on_file_changed(&mut self, path: &Path) {
        let now = Instant::now();
        for tab in &mut self.tabs {
            if tab.watch && tab.path.as_deref().is_some_and(|p| same_path(p, path)) {
                tab.reload_due = Some(now + RELOAD_DEBOUNCE);
            }
        }
        if self.program_path().is_some_and(|p| same_path(&p, path)) {
            if let Some(program) = self.program.as_mut() {
                program.due = Some(now + RELOAD_DEBOUNCE);
            }
        }
    }

    /// The program's file, when there is one.
    fn program_path(&self) -> Option<PathBuf> {
        match &self.opts.through {
            Through::Program { arg, .. } => arg.path().map(Path::to_path_buf),
            Through::Render(_) => None,
        }
    }

    /// Read the program again when a change to it is due, or when its file
    /// changed and no notice came (a watcher that missed it).
    fn reload_program_if_due(&mut self, now: Instant) {
        if !self.opts.watch {
            return;
        }
        let path = self.program_path();
        let Some(program) = self.program.as_ref() else {
            return;
        };
        let due = match program.due {
            Some(t) => t <= now,
            None => path.as_deref().and_then(Stamp::of) != program.stamp,
        };
        if due {
            self.reload_program();
        }
    }

    /// Read the program again, as `r` in the program pane or a change to
    /// its file asks, and say how that went.
    fn reload_program(&mut self) {
        let Some(path) = self.program_path() else {
            self.error("The program came from --alchemy-expr: it has no file to read again");
            return;
        };
        self.load_program();
        if self.program.as_ref().is_some_and(|p| p.compiled.is_none()) {
            let whose = if self.workspace.position(Role::Program).is_some() {
                "the program pane"
            } else {
                ":pane program"
            };
            self.error(format!(
                "{}: could not be read or compiled — {whose} shows why",
                path.display()
            ));
        } else {
            self.info(format!("Reloaded {}", path.display()));
        }
    }

    pub fn on_tick(&mut self, now: Instant) {
        self.reload_program_if_due(now);
        let view = self.view();
        for i in 0..self.tabs.len() {
            let tab = &mut self.tabs[i];
            if !tab.watch {
                continue;
            }
            let due = match tab.reload_due {
                Some(t) => t <= now,
                None => tab.stamp_changed(),
            };
            if due {
                tab.reload(view);
                self.report_reload(i);
            }
        }
    }

    /// Is a reload pending, so the terminal loop should tick soon?
    pub fn reload_pending(&self) -> bool {
        self.tabs.iter().any(|t| t.watch && t.reload_due.is_some())
            || self.program.as_ref().is_some_and(|p| p.due.is_some())
    }

    // ----- overlays --------------------------------------------------------------------------

    pub fn show_help(&mut self) {
        self.overlay = Some(Overlay::new(
            "aless help — j/k scroll, h/l pan, any other key returns".to_string(),
            help_lines(),
            false,
        ));
        self.mode = Mode::Overlay;
    }

    /// `!` / `:error`: the active tab's error report, full size: that of
    /// a `:format` that just failed, else the file's own.
    pub fn show_error(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        let tab = self.tab();
        let (what, err) = match (&tab.format_error, &tab.error) {
            (Some((f, e)), _) => (format!("{} as {f}", tab.title), e),
            (None, Some(e)) => (tab.title.clone(), e),
            (None, None) => {
                let title = tab.title.clone();
                self.info(format!("{title}: no error"));
                return;
            }
        };
        let lines = err.report.lines().map(str::to_string).collect();
        let title = format!("{what}: error report — j/k scroll, h/l pan, any other key returns");
        self.overlay = Some(Overlay::new(title, lines, true));
        self.mode = Mode::Overlay;
    }

    fn overlay_key(&mut self, key: Key) {
        let page = self.pane_height().saturating_sub(1).max(1);
        let width = self.width.max(1);
        let Some(o) = self.overlay.as_mut() else {
            self.mode = Mode::Browse;
            return;
        };
        let max = o.lines.len().saturating_sub(1);
        // Pan half a screen at a time, as far as the widest line's end.
        let pan = (width / 2).max(1);
        match (key.code, key.ctrl) {
            (KeyCode::Char('l'), false) | (KeyCode::Right, _) => {
                o.xoff = (o.xoff + pan).min(o.width().saturating_sub(width))
            }
            (KeyCode::Char('h'), false) | (KeyCode::Left, _) => o.xoff = o.xoff.saturating_sub(pan),
            (KeyCode::Char('j'), false) | (KeyCode::Down, _) => o.scroll = (o.scroll + 1).min(max),
            (KeyCode::Char('k'), false) | (KeyCode::Up, _) => o.scroll = o.scroll.saturating_sub(1),
            (KeyCode::Char('d'), true) | (KeyCode::Char('f'), true) | (KeyCode::PageDown, _) => {
                o.scroll = (o.scroll + page).min(max)
            }
            (KeyCode::Char('u'), true) | (KeyCode::Char('b'), true) | (KeyCode::PageUp, _) => {
                o.scroll = o.scroll.saturating_sub(page)
            }
            (KeyCode::Char('g'), false) | (KeyCode::Home, _) => o.scroll = 0,
            (KeyCode::Char('G'), false) | (KeyCode::End, _) => o.scroll = max,
            _ => {
                self.overlay = None;
                self.mode = Mode::Browse;
            }
        }
    }

    fn show_source(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        let h = self.body_height();
        let tab = self.tab();
        // Centre the focused node's line, or the error line.
        let line = tab
            .error
            .as_ref()
            .filter(|e| e.line > 0)
            .map(|e| e.line as usize)
            .or_else(|| tab.focused_line().map(|(l, _)| l as usize))
            .unwrap_or(1);
        let total = load::lines(&tab.source).len();
        tab.source_scroll = line
            .saturating_sub(1)
            .saturating_sub(h / 2)
            .min(total.saturating_sub(h));
        self.workspace.focused_mut().mode = PaneMode::Source;
    }

    fn scroll_source(&mut self, delta: isize) {
        let h = self.body_height();
        let tab = self.tab();
        let total = load::lines(&tab.source).len();
        let max = total.saturating_sub(h);
        tab.source_scroll = (tab.source_scroll as isize + delta).clamp(0, max as isize) as usize;
    }

    fn source_key(&mut self, key: Key) {
        let h = self.body_height() as isize;
        let n = self.take_count() as isize;
        match (key.code, key.ctrl) {
            (KeyCode::Char('j'), false) | (KeyCode::Down, _) | (KeyCode::Char('e'), true) => {
                self.scroll_source(n)
            }
            (KeyCode::Char('k'), false) | (KeyCode::Up, _) | (KeyCode::Char('y'), true) => {
                self.scroll_source(-n)
            }
            (KeyCode::Char('d'), true) => self.scroll_source(h / 2),
            (KeyCode::Char('u'), true) => self.scroll_source(-h / 2),
            (KeyCode::Char('f'), true) | (KeyCode::PageDown, _) => self.scroll_source(h),
            (KeyCode::Char('b'), true) | (KeyCode::PageUp, _) => self.scroll_source(-h),
            (KeyCode::Char('g'), false) | (KeyCode::Home, _) => self.scroll_source(isize::MIN / 2),
            (KeyCode::Char('G'), false) | (KeyCode::End, _) => self.scroll_source(isize::MAX / 2),
            (KeyCode::Char(d), false) if d.is_ascii_digit() => {
                let cur = self.count.unwrap_or(0);
                self.count = Some(cur * 10 + d.to_digit(10).unwrap() as usize);
            }
            (KeyCode::Char('c'), true) => self.quit = true,
            (KeyCode::Char('w'), true) => self.workspace.cycle(),
            (KeyCode::Char('q'), false) if self.workspace.focused().role != Role::Input => {
                self.close_pane()
            }
            // The command line and a reload leave the text on screen.
            (KeyCode::Char(':'), false) => self.start_prompt(PromptKind::Command),
            (KeyCode::Char('r'), false) => self.reload_active(),
            // Any other key shows the tree again.
            _ => self.workspace.focused_mut().mode = PaneMode::Structure,
        }
    }
}

/// Rows the error panel takes at the foot of a pane `height` rows tall
/// showing `tab`: shown while the tab's file fails to parse but its last
/// good document is still on screen (a broken save while watching). A tab
/// that never loaded shows its report in the whole pane instead.
///
/// This holds while an overlay or the source view covers the pane, though
/// the panel is not drawn then: the tree keeps the height it will be shown
/// at again, so the tab's scroll does not move.
pub fn panel_rows(tab: &Tab, height: usize) -> usize {
    let Some(err) = tab.error.as_ref() else {
        return 0;
    };
    if !tab.has_doc || tab.explorer.is_some() || height < 8 {
        return 0;
    }
    // The report and a separator line, at most half the pane.
    (err.report.lines().count() + 1).min(height / 2)
}

/// Do two paths name the same file? Canonical forms when both resolve,
/// else a plain comparison.
pub fn same_path(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => {
            a.file_name() == b.file_name() && {
                let pa = a.parent().map(|p| {
                    if p.as_os_str().is_empty() {
                        Path::new(".")
                    } else {
                        p
                    }
                });
                let pb = b.parent().map(|p| {
                    if p.as_os_str().is_empty() {
                        Path::new(".")
                    } else {
                        p
                    }
                });
                match (pa, pb) {
                    (Some(pa), Some(pb)) => {
                        match (std::fs::canonicalize(pa), std::fs::canonicalize(pb)) {
                            (Ok(x), Ok(y)) => x == y,
                            _ => pa == pb,
                        }
                    }
                    _ => false,
                }
            }
        }
    }
}

pub fn help_lines() -> Vec<String> {
    const HELP: &str = r#"aless — a jless-style viewer for every format the tabnas parsers read

MOVING
  j k / ↓ ↑ / C-n C-p / Enter Backspace   down / up one row (count allowed)
  h / ←     collapse an expanded container, else go to the parent
  l / →     expand a collapsed container, else step to the first child
  H         go to the parent without collapsing
  J K       next / previous sibling
  w b       forward / back to the next change in depth
  0 ^ $     first / last sibling
  g G / Home End    first / last row;  Ng NG  go to row N
  C-f C-b / PgDn PgUp   a page down / up
  C-d C-u   half a page down / up (a count sets the distance)
  C-e C-y   scroll one row without leaving the focus behind
  zz zt zb  focused row to the centre / top / bottom
  . , ;     scroll a long value right / left / to its end and back
  < >       less / more indentation

FOLDING
  Space     toggle the focused container
  c C       collapse the focused node and its siblings (C: deeply)
  e E       expand the focused node and its siblings (E: deeply)
  m         switch between data mode and line mode
  %         in line mode, jump between a container's brackets

SEARCH  (regular expressions; smart case; add /s to force case; [ ] { } are literal)
  /pat ?pat   search forward / backward       n N   next / previous match
  * #         search for the focused key forward / backward

COPY AND PRINT  (y copies, p prints to the screen)
  yy pp   pretty-printed value     yv pv   one-line value    ys ps   string contents
  yk pk   key                      yp pP   path .a[0].b      yb pb   ["a"][0]["b"]
  yq pq   jq path

EXPLORER  (aless DIR, or aless alone for the current directory)
  the directory tree uses the same keys: l / Space expand a directory (its
  contents are listed as you go), h collapses, / searches the listed names
  Enter     open the file under the cursor in a new tab (toggle a directory)
  -         go up: the parent directory becomes the root
  :cd DIR   change the root     :explore [DIR]   open another directory tab
  :set hidden | nohidden | hidden!   show dot-files (also --hidden)
  yp        copy the entry's filesystem path

ERRORS
  A file that does not parse shows the parser's report: the message, the
  source lines around the error with a caret under it, and the grammar's
  hint. While a watched file is broken its last good document stays on
  screen with the report docked below it, until the file parses again.
  ! / :error   the active tab's report, full size (after a failed
               :format, that attempt's report); h/l pan a long line

TABS, FILES AND WATCHING
  Tab / Shift-Tab   next / previous tab      :tab N   go to tab N
  :open PATH [FORMAT]   open a file (or a directory) in a new tab (:e is an alias)
  q / :q    close the tab (the last one closed quits)   :qa / :quit   quit
  W / :watch on|off   toggle reloading the tab when its file changes
  r / :reload         reload now (in the program pane, the program)
  s / :source         show the raw source (the focused node's line centred)
  :format FORMAT      parse the tab's text as another format
  :depth N            fold everything below depth N
  :w[!] FILE          write the document as JSON
  :set number | nonumber | relativenumber | norelativenumber | so=N | indent=N
  :N or :line N       go to row N
  F1 / :help          this help

PANES  (aless --panes out[,program], or :vsplit)
  C-w                 move the focus to the next pane; the keys work there
  s                   show the focused pane's text, or its tree again; in the
                      text, : and r work too, and any other key but the
                      scrolling ones shows the tree
  q                   close an output or program pane (the input pane stays)
  :vsplit / :split    side by side / stacked, opening the output beside the input
  :arrange            switch between side by side and stacked
  :pane out|program|close   open the output or the program pane, or close one
  :only               keep the input pane alone
  The output is the document as --render or --alchemy writes it, read
  back as a tree; it is written again when the tab reloads or changes.

  A watched tab reloads when its file changes and keeps your place: the
  focused node is found again by path, or by its nearest surviving
  ancestor and then the node closest to its old source line; folds that
  still exist are kept, and the focus stays on the same screen row.
  Formats: json jsonl jsonic jsonc json5 yaml toml ini csv tsv xml zon
  markdown feed text, and the NAME of any --grammar (by extension or whole
  file name; --kind or :open ... FORMAT to force).
"#;
    HELP.lines().map(str::to_string).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alchemy::ProgramArg;

    fn write_temp(name: &str, contents: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aless-app-tests-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(name);
        std::fs::write(&p, contents).unwrap();
        p
    }

    fn app_with(src: &str) -> App {
        let mut app = App::new(Options::default(), 80, 24);
        app.open_source("t.json", src.to_string(), Format::Json);
        app
    }

    fn keys(app: &mut App, s: &str) {
        for c in s.chars() {
            app.handle(Input::Key(Key::ch(c)));
        }
    }

    fn path(app: &mut App) -> String {
        let t = app.tab();
        let n = t.focused_node();
        fmt::path_dot(&t.doc.path(n))
    }

    const SRC: &str = r#"{"a": 1, "b": [true, {"c": "x", "d": [1, 2]}, null], "e": {"f": {"g": 2}}, "h": "last"}"#;

    #[test]
    fn counts_and_movement() {
        let mut app = app_with(SRC);
        keys(&mut app, "3j");
        assert_eq!(path(&mut app), ".b[0]");
        keys(&mut app, "2k");
        assert_eq!(path(&mut app), ".a");
        keys(&mut app, "0");
        assert_eq!(path(&mut app), ".a");
        keys(&mut app, "$");
        assert_eq!(path(&mut app), ".h");
        keys(&mut app, "5g");
        assert_eq!(path(&mut app), ".b[1]");
        keys(&mut app, "G");
        assert_eq!(path(&mut app), ".h");
        keys(&mut app, "1G");
        assert_eq!(path(&mut app), "", "an explicit count of one is a count");
        keys(&mut app, "G");
        keys(&mut app, "gg");
        assert_eq!(path(&mut app), "");
        keys(&mut app, "12345678901234");
        assert!(app.count.unwrap() < 1_000_000_000);
        app.handle(Input::Key(Key::code(KeyCode::Esc)));
        assert_eq!(app.count, None);
    }

    #[test]
    fn z_and_y_prefixes() {
        let mut app = app_with(SRC);
        keys(&mut app, "jjzt");
        assert_eq!(app.tab().scroll, 0); // short document: nothing to scroll
        keys(&mut app, "yp");
        assert_eq!(
            app.effects.pop(),
            Some(Effect::Copy {
                text: ".b".to_string(),
                what: "path".to_string()
            })
        );
        keys(&mut app, "yv");
        match app.effects.pop() {
            Some(Effect::Copy { text, .. }) => {
                assert_eq!(text, r#"[true, {"c": "x", "d": [1, 2]}, null]"#)
            }
            e => panic!("{e:?}"),
        }
        keys(&mut app, "ys");
        assert_eq!(
            app.message.as_ref().unwrap().text,
            "Focused value is not a string"
        );
        keys(&mut app, "yk");
        assert_eq!(
            app.effects.pop().map(|e| match e {
                Effect::Copy { text, .. } => text,
                _ => String::new(),
            }),
            Some("b".into())
        );
        keys(&mut app, "pp");
        assert_eq!(app.mode, Mode::Overlay);
        assert!(app.overlay.as_ref().unwrap().lines.len() > 3);
        keys(&mut app, "x");
        assert_eq!(app.mode, Mode::Browse);
        keys(&mut app, "yx"); // not a target: cancelled
        assert!(app.effects.is_empty());
    }

    #[test]
    fn search_prompt() {
        let mut app = app_with(SRC);
        keys(&mut app, "/\"g\"");
        assert_eq!(
            app.mode,
            Mode::Prompt(PromptKind::Search(Direction::Forward))
        );
        app.handle(Input::Key(Key::code(KeyCode::Enter)));
        assert_eq!(app.mode, Mode::Browse);
        assert_eq!(path(&mut app), ".e.f.g");
        assert!(app.message.as_ref().unwrap().text.contains("[1/1]"));
        keys(&mut app, "/zzz");
        app.handle(Input::Key(Key::code(KeyCode::Enter)));
        assert!(app.message.as_ref().unwrap().error);
        keys(&mut app, "/(");
        app.handle(Input::Key(Key::code(KeyCode::Enter)));
        assert!(app
            .message
            .as_ref()
            .unwrap()
            .text
            .starts_with("Invalid regex"));
        // Prompt editing.
        keys(&mut app, ":abc");
        app.handle(Input::Key(Key::code(KeyCode::Left)));
        app.handle(Input::Key(Key::code(KeyCode::Backspace)));
        assert_eq!(app.prompt.buf, "ac");
        app.handle(Input::Key(Key::ctrl('u')));
        assert_eq!(app.prompt.buf, "c");
        app.handle(Input::Key(Key::ctrl('e')));
        app.handle(Input::Key(Key::code(KeyCode::Backspace)));
        app.handle(Input::Key(Key::code(KeyCode::Backspace)));
        assert_eq!(
            app.mode,
            Mode::Browse,
            "backspace on an empty prompt cancels"
        );
        // * searches for the focused key.
        keys(&mut app, "gj*");
        assert_eq!(path(&mut app), ".a");
        assert!(app.message.as_ref().unwrap().text.contains("[1/1]"));
    }

    #[test]
    fn commands() {
        let mut app = app_with(SRC);
        app.run_command("set number relativenumber");
        assert!(app.opts.numbers && app.opts.relative);
        app.run_command("set nonu");
        assert!(!app.opts.numbers);
        app.run_command("set so=5");
        assert_eq!(app.opts.scrolloff, 5);
        app.run_command("7");
        assert_eq!(path(&mut app), ".b[1].d");
        app.run_command("depth 1");
        assert_eq!(app.tab().row_count(), 5);
        assert_eq!(path(&mut app), ".b", "focus falls to the visible ancestor");
        app.run_command("expand");
        assert_eq!(app.tab().row_count(), 14);
        app.run_command("mode line");
        assert!(app.tab().line_mode);
        app.run_command("bogus");
        assert!(app
            .message
            .as_ref()
            .unwrap()
            .text
            .starts_with("Unknown command"));
        app.run_command("format text");
        assert_eq!(app.tab().format, Format::Text);
        app.run_command("help");
        assert_eq!(app.mode, Mode::Overlay);
        keys(&mut app, "q");
        assert_eq!(app.mode, Mode::Browse);
        app.run_command("quit");
        assert!(app.quit);
    }

    #[test]
    fn tabs_open_close() {
        let p1 = write_temp("one.json", r#"{"one": 1}"#);
        let p2 = write_temp("two.yaml", "two: 2\n");
        let mut app = App::new(Options::default(), 80, 24);
        app.open_path(&p1, None);
        app.open_path(&p2, None);
        assert_eq!(app.tabs.len(), 2);
        assert_eq!(app.active, 1);
        assert_eq!(app.strip_rows(), 1);
        assert_eq!(app.effects.len(), 2, "both tabs are watched");
        app.handle(Input::Key(Key::code(KeyCode::Tab)));
        assert_eq!(app.active, 0);
        app.handle(Input::Key(Key::code(KeyCode::BackTab)));
        assert_eq!(app.active, 1);
        app.run_command("tab 1");
        assert_eq!(app.active, 0);
        app.run_command("tab 9");
        assert!(app.message.as_ref().unwrap().error);
        app.run_command(&format!("open {} yaml", p1.display()));
        assert_eq!(app.tabs.len(), 3);
        assert_eq!(app.tab().format, Format::Yaml);
        keys(&mut app, "q");
        assert_eq!(app.tabs.len(), 2);
        assert!(!app.quit);
        keys(&mut app, "qq");
        assert!(app.quit);
    }

    #[test]
    fn watch_reloads_on_change_and_poll() {
        let p = write_temp("w.json", r#"{"a": 1}"#);
        let mut app = App::new(Options::default(), 80, 24);
        app.open_path(&p, None);
        assert!(app.tab().watch);
        std::fs::write(&p, r#"{"a": 1, "b": 2}"#).unwrap();
        // A notification schedules a debounced reload.
        app.handle(Input::FileChanged(p.clone()));
        assert!(app.reload_pending());
        app.handle(Input::Tick(Instant::now()));
        assert!(app.reload_pending(), "not due yet");
        app.handle(Input::Tick(Instant::now() + RELOAD_DEBOUNCE * 2));
        assert!(!app.reload_pending());
        assert_eq!(app.tab().doc.len(), 3);
        assert!(app.message.as_ref().unwrap().text.starts_with("Reloaded"));
        // Without a notification the stamp poll catches the change.
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(&p, r#"{"a": 1, "b": 2, "c": 3}"#).unwrap();
        // Force a distinct stamp even on coarse filesystems.
        app.tab().stamp = None;
        app.handle(Input::Tick(Instant::now()));
        assert_eq!(app.tab().doc.len(), 4);
        // W turns watching off; edits are then ignored.
        keys(&mut app, "W");
        assert!(!app.tab().watch);
        assert_eq!(app.effects.last(), Some(&Effect::Unwatch(p.clone())));
        std::fs::write(&p, r#"{"a": 1}"#).unwrap();
        app.tab().stamp = None;
        app.handle(Input::Tick(Instant::now()));
        assert_eq!(app.tab().doc.len(), 4);
        keys(&mut app, "r");
        assert_eq!(app.tab().doc.len(), 2);
        // `:e!` reloads too.
        std::fs::write(&p, r#"{"a": 1, "z": 26}"#).unwrap();
        app.run_command("e!");
        assert_eq!(app.tab().doc.len(), 3);
        assert!(app.message.as_ref().unwrap().text.starts_with("Reloaded"));
        // Turning watching off during the debounce drops the pending reload.
        keys(&mut app, "W");
        assert!(app.tab().watch);
        app.handle(Input::FileChanged(p.clone()));
        assert!(app.reload_pending());
        keys(&mut app, "W");
        assert!(!app.reload_pending(), "no wake-ups for an unwatched tab");
    }

    #[test]
    fn source_view_and_help() {
        let mut app = app_with("{\n\"a\": 1,\n\"b\": 2\n}\n");
        keys(&mut app, "jjs");
        assert_eq!(app.workspace.focused().mode, PaneMode::Source);
        keys(&mut app, "j");
        assert_eq!(app.workspace.focused().mode, PaneMode::Source);
        app.handle(Input::Key(Key::code(KeyCode::Esc)));
        assert_eq!(app.workspace.focused().mode, PaneMode::Structure);
        assert_eq!(app.mode, Mode::Browse);
        app.handle(Input::Key(Key::code(KeyCode::F(1))));
        assert_eq!(app.mode, Mode::Overlay);
        assert!(app
            .overlay
            .as_ref()
            .unwrap()
            .lines
            .iter()
            .any(|l| l.contains("FOLDING")));
    }

    #[test]
    fn welcome_and_empty() {
        let mut app = App::new(Options::default(), 80, 24);
        app.open_welcome();
        assert_eq!(app.tabs.len(), 1);
        assert!(!app.tab().watchable());
        keys(&mut app, "W");
        assert!(app.message.as_ref().unwrap().error);
        keys(&mut app, "q");
        assert!(app.quit);
    }

    fn tree(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "aless-app-explorer-{}-{}",
            std::process::id(),
            name
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub/deep")).unwrap();
        std::fs::write(dir.join("a.json"), "{\"a\": 1}").unwrap();
        std::fs::write(dir.join(".hidden.json"), "{}").unwrap();
        std::fs::write(dir.join("sub/c.toml"), "c = 3\n").unwrap();
        std::fs::write(dir.join("sub/deep/x.txt"), "x\n").unwrap();
        dir
    }

    #[test]
    fn explorer_browses_lists_and_opens() {
        let dir = tree("browse");
        let mut app = App::new(Options::default(), 80, 24);
        app.open_path(&dir, None);
        assert!(app.tab().explorer.is_some());
        assert_eq!(
            app.tab().title,
            format!("{}/", dir.file_name().unwrap().to_string_lossy())
        );
        // root, sub (collapsed), a.json
        assert_eq!(app.tab().row_count(), 3);
        keys(&mut app, "j");
        assert_eq!(path(&mut app), ".sub");
        keys(&mut app, "l"); // expand: deep gets listed for its preview
        assert_eq!(app.tab().row_count(), 5);
        let ex = app.tab().explorer.as_ref().unwrap();
        assert!(ex.is_listed(&ex.root.join("sub").join("deep")));
        keys(&mut app, "j");
        assert_eq!(path(&mut app), ".sub.deep");
        app.handle(Input::Key(Key::code(KeyCode::Enter))); // toggle a directory
        assert_eq!(path(&mut app), ".sub.deep");
        assert_eq!(app.tab().row_count(), 6);
        // Search finds a listed name; Enter on the file opens it in a new tab.
        keys(&mut app, "/x.txt");
        app.handle(Input::Key(Key::code(KeyCode::Enter)));
        assert_eq!(path(&mut app), ".sub.deep[\"x.txt\"]");
        app.handle(Input::Key(Key::code(KeyCode::Enter)));
        assert_eq!(app.tabs.len(), 2);
        assert_eq!(app.active, 1);
        assert_eq!(app.tab().format, Format::Text);
        assert!(app.tab().explorer.is_none());
        // yp in the explorer copies the filesystem path.
        app.handle(Input::Key(Key::code(KeyCode::BackTab)));
        keys(&mut app, "yp");
        match app.effects.pop() {
            Some(Effect::Copy { text, .. }) => {
                assert!(text.ends_with("x.txt") && text.contains("deep"))
            }
            e => panic!("{e:?}"),
        }
        // :set hidden shows the dot-file; nohidden hides it again.
        app.run_command("set hidden");
        assert_eq!(app.tab().doc.root().children, 3);
        app.run_command("set nohidden");
        assert_eq!(app.tab().doc.root().children, 2);
    }

    #[test]
    fn explorer_parent_and_refresh() {
        let dir = tree("parent");
        let mut app = App::new(Options::default(), 80, 24);
        app.open_explorer(&dir.join("sub"));
        keys(&mut app, "jl"); // expand deep
        assert_eq!(path(&mut app), ".deep");
        keys(&mut app, "-"); // up to the tree root: sub is expanded, deep still expanded, focus kept
        let root = app.tab().explorer.as_ref().unwrap().root.clone();
        assert_eq!(
            root,
            crate::explorer::simplify(std::fs::canonicalize(&dir).unwrap())
        );
        assert_eq!(path(&mut app), ".sub.deep");
        assert!(
            app.tab().row_count() >= 6,
            "sub and deep stay open: {}",
            app.tab().row_count()
        );
        // A new file appears on the next tick.
        std::fs::write(dir.join("sub/new.yaml"), "n: 1\n").unwrap();
        app.handle(Input::FileChanged(dir.join("sub")));
        app.handle(Input::Tick(Instant::now() + RELOAD_DEBOUNCE * 2));
        assert!(app
            .tab()
            .doc
            .resolve(&[
                crate::doc::Key::Name("sub".into()),
                crate::doc::Key::Name("new.yaml".into())
            ])
            .is_some());
        assert_eq!(
            path(&mut app),
            ".sub.deep",
            "the focus survives the refresh"
        );
        // :cd re-roots; -, at the filesystem root, complains.
        app.run_command("cd sub/deep");
        assert!(app.tab().explorer.as_ref().unwrap().root.ends_with("deep"));
        assert_eq!(app.tab().doc.root().children, 1);
        app.run_command("cd nowhere");
        assert!(app.message.as_ref().unwrap().error);
        // Document-only commands are refused in the explorer.
        let rows = app.tab().row_count();
        for cmd in ["mode line", "format text", "source"] {
            app.run_command(cmd);
            assert!(app.message.as_ref().unwrap().error, "{cmd}");
            assert_eq!(app.mode, Mode::Browse, "{cmd}");
        }
        assert_eq!(app.tab().row_count(), rows);
        assert!(!app.tab().line_mode);
        assert!(app.tab().explorer.is_some());
    }

    #[test]
    fn rerooting_moves_the_watch() {
        let dir = tree("rewatch");
        let mut app = App::new(Options::default(), 80, 24);
        app.open_explorer(&dir.join("sub"));
        assert!(app.tab().watch);
        let old = app.tab().path.clone().unwrap();
        app.effects.clear();
        keys(&mut app, "-");
        let new = app.tab().path.clone().unwrap();
        assert_ne!(old, new);
        assert_eq!(
            app.effects,
            vec![Effect::Unwatch(old.clone()), Effect::Watch(new.clone())]
        );
        app.effects.clear();
        app.run_command("cd sub");
        assert_eq!(app.effects, vec![Effect::Unwatch(new), Effect::Watch(old)]);
    }

    #[test]
    fn errors_show_the_engine_report() {
        let p = write_temp("broken.json", "{\"a\": 1, \"b\": \n");
        let mut app = App::new(Options::default(), 80, 24);
        app.open_path(&p, None);
        assert!(app.tab().shows_error_only());
        assert_eq!(app.error_panel_rows(), 0, "the whole pane is the report");
        keys(&mut app, "!");
        assert_eq!(app.mode, Mode::Overlay);
        let o = app.overlay.as_ref().unwrap();
        assert!(o.ansi);
        assert!(o.lines[0].contains("[tabnas/unexpected]"));
        assert!(o.lines[1].contains(&format!("{}:2:1", p.display())));
        keys(&mut app, "x");
        // The file is fixed: a document appears, the error goes.
        std::fs::write(&p, "{\"a\": 1, \"b\": 2}").unwrap();
        keys(&mut app, "r");
        assert!(!app.tab().shows_error_only() && app.tab().error.is_none());
        let full = app.pane_height();
        // Broken again: the document stays and the report docks below it.
        std::fs::write(&p, "{\"a\": 1, \"b\": [1,,]}").unwrap();
        keys(&mut app, "r");
        assert!(app.tab().error.is_some() && app.tab().has_doc);
        let panel = app.error_panel_rows();
        assert!(panel > 2 && panel <= full / 2, "{panel}");
        assert_eq!(app.view().height, full - panel);
        assert!(app
            .message
            .as_ref()
            .unwrap()
            .text
            .contains("! shows the report"));
        // No error, no report.
        std::fs::write(&p, "[1]").unwrap();
        keys(&mut app, "r");
        assert_eq!(app.error_panel_rows(), 0);
        app.run_command("error");
        assert_eq!(app.mode, Mode::Browse);
        assert!(app.message.as_ref().unwrap().text.ends_with("no error"));
    }

    #[test]
    fn a_failed_format_keeps_its_report_for_bang() {
        let p = write_temp("fmt.json", "{\"a\": [1, 2]}");
        let mut app = App::new(Options::default(), 80, 24);
        app.open_path(&p, None);
        app.run_command("format toml");
        let m = app.message.clone().unwrap();
        assert!(m.error && m.text.ends_with("! shows the report"), "{m:?}");
        // The document still parses as JSON: nothing is docked or marked.
        assert!(app.tab().error.is_none());
        assert_eq!(app.error_panel_rows(), 0);
        keys(&mut app, "!");
        assert_eq!(app.mode, Mode::Overlay);
        let o = app.overlay.as_ref().unwrap();
        assert!(
            o.title.starts_with("fmt.json as toml: error report"),
            "{}",
            o.title
        );
        assert!(o.lines[1].contains("fmt.json:1:"), "{:?}", o.lines);
        keys(&mut app, "x");
        // A reload drops it...
        keys(&mut app, "r");
        app.run_command("error");
        assert!(app.message.as_ref().unwrap().text.ends_with("no error"));
        // ...and so does a :format that works.
        app.run_command("format toml");
        assert!(app.tab().format_error.is_some());
        app.run_command("format yaml");
        assert!(app.tab().format_error.is_none());
    }

    #[test]
    fn viewing_the_report_keeps_the_readers_place() {
        let good = format!(
            "[{}]",
            (0..100)
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(",\n")
        );
        let p = write_temp("place.json", &good);
        let mut app = App::new(Options::default(), 80, 24);
        app.open_path(&p, None);
        std::fs::write(&p, good.replace("99]", "99,,]")).unwrap();
        keys(&mut app, "r");
        assert!(app.error_panel_rows() > 0);
        // At the foot of the document, the focus near the window's top:
        // a taller window would scroll back, a shorter one forward again.
        keys(&mut app, "G");
        for _ in 0..app.opts.scrolloff + 4 {
            keys(&mut app, "k");
        }
        crate::render::screen(&mut app);
        let place = (app.tab().focus, app.tab().scroll);
        let rows = app.tab().rows().len();
        assert!(place.1 > rows - app.pane_height(), "{place:?}");
        for (open, close) in [("!", "x"), ("s", "x")] {
            keys(&mut app, open);
            let covered =
                app.mode != Mode::Browse || app.workspace.focused().mode == PaneMode::Source;
            assert!(covered, "{open} covers the tree");
            crate::render::screen(&mut app);
            keys(&mut app, close);
            crate::render::screen(&mut app);
            assert_eq!((app.tab().focus, app.tab().scroll), place, "after {open}");
        }
    }

    fn ctrl(c: char) -> Input {
        Input::Key(Key {
            code: KeyCode::Char(c),
            ctrl: true,
            alt: false,
        })
    }

    fn with_panes(panes: &[Role], through: Through) -> App {
        let opts = Options {
            panes: panes.to_vec(),
            through,
            ..Options::default()
        };
        App::new(opts, 100, 24)
    }

    /// The output pane shows the document as `--render` writes it, and
    /// follows the input: a reload, another tab.
    #[test]
    fn the_output_pane_follows_the_input() {
        let p = write_temp("panes.csv", "name,age\nada,36\n");
        let mut app = with_panes(&[Role::Output], Through::default());
        app.open_path(&p, None);
        app.prepare();
        let out = app.pane_tab(Pane::new(Role::Output));
        assert_eq!(out.format, Format::Json);
        assert!(out.source.contains("\"ada\""), "{}", out.source);
        assert!(out.title.ends_with("→ json"), "{}", out.title);
        std::fs::write(&p, "name,age\nlin,28\n").unwrap();
        keys(&mut app, "r");
        let out = app.pane_tab(Pane::new(Role::Output));
        assert!(
            out.source.contains("\"lin\""),
            "a reload rebuilds it: {}",
            out.source
        );
        app.open_source("u.json", "[1, 2]".into(), Format::Json);
        app.prepare();
        let out = app.pane_tab(Pane::new(Role::Output));
        assert_eq!(out.source, "[\n  1,\n  2\n]\n", "the active tab's output");
        // A directory has none, and says so.
        app.open_explorer(p.parent().unwrap());
        app.prepare();
        let out = app.pane_tab(Pane::new(Role::Output));
        assert!(!out.has_doc && out.error.is_some());
    }

    /// `:format` parses the input as another format, and the output
    /// follows it.
    #[test]
    fn the_output_pane_follows_a_format_change() {
        let mut app = with_panes(&[Role::Output], Through::default());
        app.open_source("t.conf", "a = 1\n".into(), Format::Ini);
        app.prepare();
        let out = |app: &mut App| app.pane_tab(Pane::new(Role::Output)).source.clone();
        assert_eq!(
            out(&mut app),
            "{\n  \"a\": \"1\"\n}\n",
            "ini reads a string"
        );
        app.run_command("format toml");
        app.prepare();
        assert_eq!(out(&mut app), "{\n  \"a\": 1\n}\n", "toml reads a number");
    }

    /// C-w moves between panes, the keys move the focused pane's tree, s
    /// switches it between its tree and its text, and q closes a pane but
    /// never the input's.
    #[test]
    fn the_keys_work_on_the_focused_pane() {
        let mut app = with_panes(&[Role::Output], Through::default());
        app.open_source("t.json", r#"{"a": [1, 2], "b": 3}"#.into(), Format::Json);
        app.prepare();
        app.handle(ctrl('w'));
        assert_eq!(app.workspace.focused().role, Role::Output);
        keys(&mut app, "j");
        assert_eq!(app.tab().focus, 1, "the output's tree moves");
        assert_eq!(app.input().focus, 0, "the input's stays");
        keys(&mut app, "s");
        assert_eq!(app.workspace.focused().mode, PaneMode::Source);
        assert_eq!(app.workspace.panes[0].mode, PaneMode::Structure);
        keys(&mut app, "s");
        assert_eq!(app.workspace.focused().mode, PaneMode::Structure);
        app.handle(ctrl('w'));
        assert_eq!(
            app.workspace.focused().role,
            Role::Input,
            "round to the first"
        );
        app.handle(ctrl('w'));
        keys(&mut app, "q");
        assert_eq!(app.workspace.panes.len(), 1, "q closes the output pane");
        assert!(!app.quit);
        assert_eq!(app.tabs.len(), 1, "and no tab");
    }

    #[test]
    fn the_pane_commands() {
        let mut app = app_with("[1, 2]");
        let roles = |app: &App| {
            app.workspace
                .panes
                .iter()
                .map(|p| p.role)
                .collect::<Vec<_>>()
        };
        app.run_command("vsplit");
        assert_eq!(roles(&app), [Role::Input, Role::Output]);
        assert_eq!(app.workspace.arrangement, Arrangement::SideBySide);
        app.run_command("arrange");
        assert_eq!(app.workspace.arrangement, Arrangement::Stacked);
        app.run_command("split");
        assert_eq!(
            roles(&app),
            [Role::Input, Role::Output],
            "a second split opens nothing"
        );
        app.run_command("pane program");
        assert!(app.message.as_ref().unwrap().text.starts_with("No program"));
        assert_eq!(roles(&app), [Role::Input, Role::Output]);
        app.run_command("pane close");
        assert!(app
            .message
            .as_ref()
            .unwrap()
            .text
            .starts_with("The input pane stays"));
        app.handle(ctrl('w'));
        app.run_command("pane close");
        assert_eq!(roles(&app), [Role::Input]);
        app.run_command("pane out");
        assert_eq!(roles(&app), [Role::Input, Role::Output]);
        app.run_command("only");
        assert_eq!(roles(&app), [Role::Input]);
        assert_eq!(
            app.workspace.arrangement,
            Arrangement::Stacked,
            "the arrangement stays"
        );
        app.run_command("pane tree");
        assert!(app
            .message
            .as_ref()
            .unwrap()
            .text
            .starts_with("usage: :pane"));
    }

    /// The program pane shows the program's text and its plan; a change to
    /// the program's file is read again after the debounce, and the output
    /// follows; a program that does not compile says why in both panes.
    #[test]
    fn the_program_pane_follows_its_file() {
        let prog = write_temp("panes.alc", r#"def export [input] "one""#);
        let through = Through::Program {
            arg: ProgramArg::File(prog.clone()),
            render: None,
        };
        let mut app = with_panes(&[Role::Output, Role::Program], through);
        app.open_source("t.json", "[1]".into(), Format::Json);
        app.prepare();
        assert!(
            app.effects.contains(&Effect::Watch(prog.clone())),
            "{:?}",
            app.effects
        );
        let output = Pane::new(Role::Output);
        let plan = Pane {
            role: Role::Program,
            mode: PaneMode::Structure,
        };
        assert_eq!(app.pane_tab(output).source, "one");
        assert!(app
            .pane_tab(Pane::new(Role::Program))
            .source
            .contains("def export"));
        let report = &app.pane_tab(plan).source;
        assert!(report.contains("\"entry\": \"export\""), "{report}");
        let later = || Instant::now() + RELOAD_DEBOUNCE * 2;
        std::fs::write(&prog, r#"def export [input] "two""#).unwrap();
        app.handle(Input::FileChanged(prog.clone()));
        assert!(app.reload_pending());
        app.handle(Input::Tick(later()));
        assert_eq!(app.pane_tab(output).source, "two");
        assert!(app.message.as_ref().unwrap().text.starts_with("Reloaded"));
        std::fs::write(&prog, "def export [input] (nope input)").unwrap();
        app.handle(Input::FileChanged(prog.clone()));
        app.handle(Input::Tick(later()));
        assert!(!app.pane_tab(output).has_doc);
        let why = app.pane_tab(plan).error.clone().unwrap();
        assert!(why.message.contains("unknown_name"), "{}", why.message);
        assert!(
            app.message.as_ref().unwrap().error,
            "the status line says so"
        );
    }

    /// Without watching, `r` in the program pane reads the program again
    /// and the output follows; `:` and `r` leave the program's text on
    /// screen. A program given as an expression has no file to read.
    #[test]
    fn r_in_the_program_pane_reads_the_program_again() {
        let prog = write_temp("panes-r.alc", r#"def export [input] "one""#);
        let through = Through::Program {
            arg: ProgramArg::File(prog.clone()),
            render: None,
        };
        let opts = Options {
            panes: vec![Role::Output, Role::Program],
            through,
            watch: false,
            ..Options::default()
        };
        let mut app = App::new(opts, 100, 24);
        app.open_source("t.json", "[1]".into(), Format::Json);
        app.prepare();
        let output = Pane::new(Role::Output);
        assert_eq!(app.pane_tab(output).source, "one");
        std::fs::write(&prog, r#"def export [input] "two""#).unwrap();
        app.handle(Input::Tick(Instant::now() + RELOAD_DEBOUNCE * 2));
        assert_eq!(app.pane_tab(output).source, "one", "nothing watches it");
        app.handle(ctrl('w'));
        app.handle(ctrl('w'));
        assert_eq!(app.workspace.focused(), Pane::new(Role::Program));
        keys(&mut app, "r");
        assert_eq!(app.pane_tab(output).source, "two", "r reads it again");
        assert!(app.message.as_ref().unwrap().text.starts_with("Reloaded"));
        assert_eq!(app.workspace.focused().mode, PaneMode::Source);
        keys(&mut app, ":");
        assert_eq!(app.mode, Mode::Prompt(PromptKind::Command));
        app.handle(Input::Key(Key::code(KeyCode::Esc)));
        assert_eq!(app.workspace.focused().mode, PaneMode::Source);
        // A program that no longer reads says why in the output pane.
        std::fs::remove_file(&prog).unwrap();
        keys(&mut app, "r");
        let message = app.message.clone().unwrap();
        assert!(message.error, "{}", message.text);
        assert!(
            message.text.contains("the program pane shows why"),
            "{}",
            message.text
        );
        let why = app.pane_tab(output).error.clone().unwrap();
        assert!(
            why.message.contains("the program did not load"),
            "{}",
            why.message
        );

        let through = Through::Program {
            arg: ProgramArg::Expr(r#"def export [input] "x""#.into()),
            render: None,
        };
        let mut app = with_panes(&[Role::Output, Role::Program], through);
        app.open_source("t.json", "[1]".into(), Format::Json);
        app.prepare();
        app.handle(ctrl('w'));
        app.handle(ctrl('w'));
        keys(&mut app, "r");
        assert!(app
            .message
            .as_ref()
            .unwrap()
            .text
            .contains("--alchemy-expr"));
    }

    /// A click gives the focus to the pane under it, and to the row.
    #[test]
    fn a_click_focuses_the_pane_and_the_row() {
        let mut app = with_panes(&[Role::Output], Through::default());
        app.open_source("t.json", "[1, 2, 3]".into(), Format::Json);
        app.prepare();
        // The output pane's body starts at column 50 and row 1, under its
        // title.
        app.handle(Input::Click(60, 3));
        assert_eq!(app.workspace.focused().role, Role::Output);
        assert_eq!(app.tab().focus, 2);
        app.handle(Input::Click(3, 0));
        assert_eq!(app.workspace.focused().role, Role::Input, "a title focuses");
        assert_eq!(app.tab().focus, 0, "and moves no row");
    }

    #[test]
    fn same_path_variants() {
        let p = write_temp("same.json", "1");
        assert!(same_path(&p, &p));
        let rel = p.strip_prefix(std::env::temp_dir()).unwrap();
        let rel = std::env::temp_dir().join(".").join(rel);
        assert!(same_path(&p, &rel));
        assert!(!same_path(&p, &p.with_file_name("other.json")));
    }
}
