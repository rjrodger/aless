//! The application: tabs, modes, the key map and the command line. Pure
//! state: the terminal is driven from `main.rs`, which feeds [`Input`]s in
//! and paints what [`crate::render`] draws from the state.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use crate::doc::Key as PathKey;
use crate::doc::{Kind, NodeId};
use crate::fmt;
use crate::highlight::{self, Painted, Painter};
use crate::history::{self, ChangeKind, History, Item, Keep};
use crate::load::{self, Format, Loaded};
use crate::search::{self, Direction};
use crate::tab::{Frozen, Reloaded, Reposition, Tab, View};

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
    /// The help text, the printed value of a `p` command, and so on. The
    /// source view (`s`) is not a mode of the app's but the view's
    /// ([`App::source_view`]).
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
    /// The explorer shows dot-files, as it does unless `:set nohidden`
    /// hides them.
    pub show_hidden: bool,
    /// Mark what each version changed (`:set changes`, `--no-changes`).
    pub changes: bool,
    /// The most versions a tab keeps.
    pub history: usize,
    /// The local zone, in seconds east of UTC, read once at start: unknown,
    /// times are shown in UTC.
    pub utc_offset: Option<i32>,
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
            show_hidden: true,
            changes: true,
            history: history::DEFAULT_VERSIONS,
            utc_offset: None,
        }
    }
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
    /// The source view: the active tab's text in place of its tree (`s`).
    pub source_view: bool,
    /// Colours for the text the source view shows, made off this thread.
    painter: Painter,
    /// A fixed time to stamp versions with, for tests and captures; the
    /// system clock when unset.
    pub clock: Option<SystemTime>,
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
            source_view: false,
            painter: Painter::new(load::Limits::current().timeout),
            clock: None,
        }
    }

    // ----- layout ----------------------------------------------------------------

    /// Rows taken by the tab strip (shown only with several tabs).
    pub fn strip_rows(&self) -> usize {
        usize::from(self.tabs.len() > 1)
    }

    /// The row of the active tab's timeline, shown once it has versions
    /// or failed saves to set side by side.
    pub fn timeline_rows(&self) -> usize {
        let shown = self.mode != Mode::Overlay
            && self
                .tab_ref()
                .is_some_and(|t| t.explorer.is_none() && t.history.has_timeline());
        usize::from(shown)
    }

    /// Rows available to the pane: the tree, or the text the source view
    /// shows.
    pub fn pane_height(&self) -> usize {
        self.height
            .saturating_sub(2 + self.strip_rows() + self.timeline_rows())
            .max(1)
    }

    /// Rows the error panel takes at the foot of the pane ([`panel_rows`]).
    pub fn error_panel_rows(&self) -> usize {
        match self.tab_ref() {
            Some(tab) => panel_rows(tab, self.pane_height()),
            None => 0,
        }
    }

    /// Rows the note of the version's changes takes above the error panel:
    /// its rule, and as many changes as a third of the pane holds. It takes
    /// only rows below the focus and its scrolloff, so that the focused row
    /// keeps its screen row; with no room for a change, it does not open.
    pub fn note_rows(&self) -> usize {
        let Some(tab) = self.tab_ref() else {
            return 0;
        };
        if !tab.note || !self.opts.changes || self.source_view || self.error_panel_rows() > 0 {
            return 0;
        }
        let changes = tab.history.shown().map_or(0, |v| v.changes.len());
        let pane = self.pane_height();
        let below = pane.saturating_sub(tab.focus.saturating_sub(tab.scroll) + 1);
        let free = below.saturating_sub(self.opts.scrolloff);
        let rows = 1 + changes.min((pane / 3).max(1));
        if free < 2 {
            0
        } else {
            rows.min(free)
        }
    }

    /// Rows the tree itself gets.
    pub fn tree_height(&self) -> usize {
        self.pane_height()
            .saturating_sub(self.error_panel_rows() + self.note_rows())
            .max(1)
    }

    /// The time a new version is stamped with.
    pub fn now(&self) -> SystemTime {
        self.clock.unwrap_or_else(SystemTime::now)
    }

    fn keep(&self) -> Keep {
        Keep {
            versions: self.opts.history,
            ..Keep::default()
        }
    }

    pub fn view(&self) -> View {
        View::new(self.tree_height(), self.opts.scrolloff)
    }

    /// The active tab: the one the keys move, fold, search and copy in,
    /// and that reloads, watches, closes, explores and changes format.
    pub fn tab(&mut self) -> &mut Tab {
        let i = self.active.min(self.tabs.len().saturating_sub(1));
        &mut self.tabs[i]
    }

    /// The active tab.
    pub fn tab_ref(&self) -> Option<&Tab> {
        self.tabs.get(self.active)
    }

    pub fn count_text(&self) -> Option<String> {
        self.count.map(|c| c.to_string())
    }

    /// Bring the view up to date before a paint: the active tab's rows and
    /// window followed within the pane's height, and the colours of the
    /// text the source view shows.
    pub fn prepare(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        let height = self.pane_height();
        let rows = self.tree_height();
        let scrolloff = self.opts.scrolloff;
        let source = self.source_view;
        let tab = self.tab();
        tab.follow(View::new(rows, scrolloff));
        // The text view fills the pane, which may have grown taller.
        if source {
            tab.clamp_source_scroll(height);
        }
        self.paint_source();
    }

    // ----- colours -------------------------------------------------------------------

    /// Ask for the colours of the text the source view shows, take in those
    /// that have come, and forget those of closed tabs; a text not shown when
    /// its turn comes is skipped. Without colour (`--no-color`, `NO_COLOR`)
    /// nothing is asked, so no text is parsed and the loop never ticks for it.
    fn paint_source(&mut self) {
        if !self.opts.color {
            return;
        }
        let mut painter = std::mem::take(&mut self.painter);
        painter.collect();
        let shown = self.source_view.then(|| self.paint_key()).flatten();
        // Named before it is asked for, so the worker never skips it.
        painter.want(shown);
        if let (Some(key), Some(tab)) = (shown, self.tab_ref()) {
            painter.ask(key, &tab.source);
        }
        let live: HashSet<u64> = self.tabs.iter().map(|t| t.id).collect();
        painter.retain(|id| live.contains(&id));
        self.painter = painter;
    }

    /// Which colours the active tab's text is drawn with: the tab, its
    /// generation, and the format whose grammar lexes it. A directory has
    /// none.
    pub fn paint_key(&self) -> Option<highlight::Key> {
        let tab = self.tab_ref()?;
        if tab.explorer.is_some() {
            return None;
        }
        Some((tab.id, tab.generation, tab.format))
    }

    /// The colours of the active tab's text, once they are in.
    pub fn painted(&self) -> Option<Arc<Painted>> {
        self.painter.get(self.paint_key()?)
    }

    /// Are colours on their way, so the terminal loop should tick soon?
    pub fn highlight_pending(&self) -> bool {
        self.painter.pending()
    }

    /// Ask for the colours the source view needs, wait for them and take them
    /// in, as the terminal loop's ticks would: for a screen drawn whole.
    pub fn settle_colours(&mut self) {
        self.prepare();
        self.painter.settle(Duration::from_secs(30));
        self.prepare();
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
        if tab.explorer.is_none() && tab.listing.is_none() {
            tab.line_mode = self.opts.line_mode;
            if let Some(d) = self.opts.depth {
                tab.expand_to_depth(d, view);
            }
        }
        if tab.frozen.is_none() {
            tab.history.stamp_first(self.now());
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
"open": ":open PATH [FORMAT]   opens a file in a new tab",
"formats": {},
"keys": "F1 or :help lists every key and command, and q quits",
"watch": "an open file reloads when it changes, and keeps your place"}}"#,
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
                let source = self.source_view;
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

    /// A click on a row of the tree gives the focus to that row.
    fn click(&mut self, col: u16, row: u16) {
        if self.mode != Mode::Browse || self.tabs.is_empty() || self.source_view {
            return;
        }
        if usize::from(col) >= self.width {
            return;
        }
        let Some(pane_row) = usize::from(row).checked_sub(self.strip_rows()) else {
            return;
        };
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
            Mode::Browse if self.source_view => self.source_key(key),
            Mode::Browse => self.browse_key(key),
            Mode::Prompt(kind) => self.prompt_key(kind, key),
            Mode::Overlay => self.overlay_key(key),
        }
    }

    fn take_count(&mut self) -> usize {
        self.count.take().unwrap_or(1)
    }

    fn browse_key(&mut self, key: Key) {
        let keeps_note = !key.ctrl
            && !key.alt
            && matches!(key.code, KeyCode::Char('[' | ']' | '{' | '}' | 'd' | 'D'));
        if !keeps_note {
            for t in &mut self.tabs {
                t.note = false;
            }
        }
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
        if self.tab().listing.is_some() {
            match (key.code, key.ctrl) {
                (KeyCode::Enter, false) => {
                    self.history_enter();
                    return;
                }
                (KeyCode::Char('m' | '%' | 's'), false) => return,
                (KeyCode::Char('[' | ']' | '{' | '}' | 'd' | 'D'), false) => {
                    self.info("The history tab lists the versions: Enter opens one");
                    return;
                }
                _ => {}
            }
        }
        match (key.code, key.ctrl) {
            (KeyCode::Char('c'), true) => self.quit = true,
            (KeyCode::Char('z'), true) => self.effects.push(Effect::Suspend),
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
            (KeyCode::Char('v'), false) => self.open_history(),
            (KeyCode::Char('['), false) => self.step_version(-(n as isize)),
            (KeyCode::Char(']'), false) => self.step_version(n as isize),
            (KeyCode::Char('{'), false) => self.step_version(isize::MIN / 2),
            (KeyCode::Char('}'), false) => self.step_version(isize::MAX / 2),
            (KeyCode::Char('d'), false) => {
                let msg = self.tab().next_change(n, true, view);
                self.info(msg);
            }
            (KeyCode::Char('D'), false) => {
                let msg = self.tab().next_change(n, false, view);
                self.info(msg);
            }
            _ => {}
        }
        self.explorer_sync();
    }

    /// `[` `]` `{` `}`: show the version `delta` from the one on screen.
    fn step_version(&mut self, delta: isize) {
        let view = self.view();
        let changes = self.opts.changes;
        let offset = self.opts.utc_offset;
        let tab = self.tab();
        let title = tab.title.clone();
        if tab.explorer.is_some() {
            self.info("A directory keeps no versions");
            return;
        }
        if let Some(f) = tab.frozen {
            self.info(format!(
                "This tab holds version {} alone. [ and ] step in the file's own tab",
                f.number
            ));
            return;
        }
        if tab.history.len() < 2 {
            self.info(format!("{title} has one version so far"));
            return;
        }
        let (to, moved) = tab.history.step(delta);
        if !moved {
            let number = tab.history.shown().map_or(1, |v| v.number);
            let end = if delta < 0 {
                "the oldest kept"
            } else {
                "the newest"
            };
            self.info(format!("Version {number} is {end}"));
            return;
        }
        match tab.show_version(to, view) {
            Ok(()) => {
                tab.note = changes;
                if !changes {
                    let text = version_line(tab, offset);
                    self.info(text);
                }
                self.sync_listings();
            }
            Err(e) => self.error(e),
        }
    }

    // ----- the listing of versions -------------------------------------------------

    /// `v`: go to the tab that lists the versions of the file on screen,
    /// and open it first if need be.
    fn open_history(&mut self) {
        let tab = self.tab();
        if tab.listing.is_some() {
            self.info("This is the history tab: Enter opens a version");
            return;
        }
        if tab.explorer.is_some() {
            self.info("A directory keeps no versions");
            return;
        }
        let of = tab.frozen.map_or(tab.id, |f| f.of);
        let open = self
            .tabs
            .iter()
            .position(|t| t.listing.as_ref().is_some_and(|l| l.of == of));
        if let Some(i) = open {
            self.active = i;
            return;
        }
        let Some(src) = self.tabs.iter().position(|t| t.id == of) else {
            self.error("The file's own tab is closed, so its versions are gone");
            return;
        };
        let name = self.tabs[src].title.clone();
        let count = self.tabs[src].history.len();
        if count == 0 {
            self.info(format!("{name} has no versions yet"));
            return;
        }
        let offset = self.opts.utc_offset;
        let (doc, list) = history::listing(&self.tabs[src].history, of, &name, offset);
        let shown = list
            .items
            .iter()
            .position(|i| matches!(i, Item::Version { shown: true, .. }));
        let id = self.next_id;
        self.next_id += 1;
        let source = String::new();
        let loaded = Loaded {
            doc,
            format: Format::Text,
            source,
        };
        let mut tab = Tab::new(id, format!("{name} history"), None, loaded);
        tab.history = History::default();
        tab.listing = Some(list);
        self.adopt(tab);
        let view = self.view();
        if let Some(row) = shown {
            self.tab().focus_node(row as NodeId, view);
        }
        let s = if count == 1 { "" } else { "s" };
        self.info(format!(
            "{name}: {count} version{s}. Enter opens one in a tab of its own"
        ));
    }

    /// Build each listing tab again from the versions of the tab it lists.
    fn sync_listings(&mut self) {
        let view = self.view();
        let offset = self.opts.utc_offset;
        for li in 0..self.tabs.len() {
            let Some(of) = self.tabs[li].listing.as_ref().map(|l| l.of) else {
                continue;
            };
            let Some(src) = self.tabs.iter().position(|t| t.id == of) else {
                continue;
            };
            let name = self.tabs[src].title.clone();
            let (doc, list) = history::listing(&self.tabs[src].history, of, &name, offset);
            let tab = &mut self.tabs[li];
            let source = String::new();
            let loaded = Loaded {
                doc,
                format: Format::Text,
                source,
            };
            // Before the tree, so a search's matches are found in its rows.
            tab.listing = Some(list);
            tab.apply(loaded, view);
        }
    }

    /// In a listing tab: the tab listed, and the index of the focused
    /// version, or of the version of the focused change.
    fn listed_version(&mut self) -> Option<(usize, usize)> {
        let node = self.tab().focused_node();
        let list = self.tab_ref()?.listing.as_ref()?;
        let index = match list.items.get(node as usize)? {
            Item::Version { index, .. } | Item::Change { index, .. } => *index,
            _ => return None,
        };
        let src = self.tabs.iter().position(|t| t.id == list.of)?;
        (index < self.tabs[src].history.len()).then_some((src, index))
    }

    /// Enter in a listing tab: the focused version in a tab of its own,
    /// or a focused change's version, at that change.
    fn history_enter(&mut self) {
        let view = self.view();
        let node = self.tab().focused_node();
        let Some((of, item)) = self
            .tab_ref()
            .and_then(|t| t.listing.as_ref())
            .map(|l| (l.of, l.items.get(node as usize).cloned()))
        else {
            return;
        };
        match item {
            Some(Item::Root { .. }) => self.tab().toggle(view),
            Some(Item::Failed { .. }) => {
                self.info("This save did not parse, so it has no tree to open")
            }
            Some(Item::Version { index, .. }) => self.open_version(of, index, None),
            Some(Item::Change { index, mut change }) => {
                if change.kind == ChangeKind::Removed {
                    change.path.pop();
                }
                self.open_version(of, index, Some(change.path))
            }
            None => {}
        }
    }

    /// Version `index` of the tab `of`, in a tab of its own that is not
    /// watched, at `at` or at the place the file's own tab is. A version
    /// already in a tab is not opened twice.
    fn open_version(&mut self, of: u64, index: usize, at: Option<Vec<PathKey>>) {
        let Some(src) = self.tabs.iter().position(|t| t.id == of) else {
            self.error("The file's own tab is closed, so its versions are gone");
            return;
        };
        let Some(v) = self.tabs[src].history.versions().get(index).cloned() else {
            return;
        };
        let name = self.tabs[src].title.clone();
        let (number, time) = (v.number, v.time);
        let epoch = self.tabs[src].history.epoch();
        let frozen = Frozen { of, epoch, number };
        if let Some(i) = self.tabs.iter().position(|t| t.frozen == Some(frozen)) {
            self.active = i;
            if let Some(place) = at {
                let view = self.view();
                let tab = self.tab();
                let (_, node) = tab.doc.resolve_prefix(&place);
                tab.focus_node(node, view);
            }
        } else {
            let place = match at {
                Some(p) => p,
                None => {
                    let t = &mut self.tabs[src];
                    let n = t.focused_node();
                    t.doc.path(n)
                }
            };
            let loaded = match load::load_str(v.source.to_string(), v.format) {
                Ok(l) => l,
                Err(e) => {
                    self.error(format!("Version {number} no longer parses: {e}"));
                    return;
                }
            };
            let id = self.next_id;
            self.next_id += 1;
            let mut tab = Tab::new(id, format!("{name} v{number}"), None, loaded);
            tab.history = History::of_version(v);
            tab.frozen = Some(frozen);
            self.adopt(tab);
            let view = self.view();
            let changes = self.opts.changes;
            let tab = self.tab();
            tab.refresh_marks();
            tab.note = changes && !tab.marks.is_empty();
            let (_, node) = tab.doc.resolve_prefix(&place);
            tab.focus_node(node, view);
        }
        let verb = if number == 1 { "opened" } else { "saved" };
        self.info(format!(
            "Version {number} of {name}, {verb} {}, in a tab of its own",
            history::clock(time, self.opts.utc_offset)
        ));
    }

    // ----- explorer ------------------------------------------------------------------

    /// After a fold change in an explorer tab, list what the expanded
    /// directories need.
    fn explorer_sync(&mut self) {
        if self.tabs.is_empty() || self.tab().explorer.is_none() {
            return;
        }
        let view = self.view();
        let tab = self.tab();
        tab.explorer_sync(view);
        let capped = tab.explorer.as_ref().is_some_and(|ex| ex.capped);
        if capped {
            self.error(format!(
                "Listing stopped at {} directories. Fold some to list more.",
                crate::explorer::MAX_LISTED
            ));
        }
    }

    /// `Enter` in the explorer: open a file in a new tab, toggle a
    /// directory.
    fn explorer_enter(&mut self) {
        let view = self.view();
        let tab = self.tab();
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
                _ => self.error(format!(
                    "{} is a link to nothing aless can read, so aless cannot open it",
                    fs_path.display()
                )),
            },
            Some(crate::explorer::EntryKind::Other) => self.error(format!(
                "{} is not a regular file, so aless cannot open it",
                fs_path.display()
            )),
            None => {}
        }
    }

    /// `-` in the explorer: make the parent directory the root.
    fn explorer_parent(&mut self) {
        let tab = self.tab();
        let Some(root) = tab.explorer.as_ref().map(|ex| ex.root.clone()) else {
            return;
        };
        match root.parent() {
            Some(parent) => {
                let parent = parent.to_path_buf();
                self.reroot_active(&parent);
            }
            None => self.error("This directory is the top of the filesystem, so it has no parent"),
        }
    }

    /// Re-root the active explorer at `dir`, moving its watch there.
    fn reroot_active(&mut self, dir: &Path) {
        let view = self.view();
        let tab = self.tab();
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
        let tab = self.tab();
        let Some(root) = tab.explorer.as_ref().map(|ex| ex.root.clone()) else {
            self.open_explorer(Path::new(dir));
            return;
        };
        let target = root.join(dir);
        if !target.is_dir() {
            self.error(format!(
                "{} is not a directory, so aless cannot change to it",
                target.display()
            ));
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
        if self.tab_ref().is_some_and(|t| t.listing.is_some()) {
            return Err("Open a version with Enter to copy from it".to_string());
        }
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
                    _ => {
                        return Err(
                            "The focused entry is a directory, so it has no string contents"
                                .to_string(),
                        )
                    }
                },
                _ => ex.fs_path(&doc.path(node)).display().to_string(),
            });
        }
        Ok(match target {
            YankTarget::Pretty => fmt::to_json_pretty(doc, node, 2),
            YankTarget::Line => fmt::to_json_line(doc, node),
            YankTarget::Str => match &n.kind {
                Kind::Str(s) => s.to_string(),
                _ => {
                    return Err(
                        "The focused value is not a string, so there are no string contents to copy or print"
                            .to_string(),
                    )
                }
            },
            YankTarget::Key => match n.key.name() {
                Some(k) => {
                    if fmt::is_identifier(k) && !tab.line_mode {
                        k.to_string()
                    } else {
                        fmt::quote(k)
                    }
                }
                None => return Err("The focused node has no key to copy or print".to_string()),
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
                            "{}: j and k scroll, h and l pan, and any other key returns",
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
            self.error("The focused node has no key to search for");
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
            "source" | "src" | "mode" if self.tab().explorer.is_some() => {
                self.error(format!(":{cmd} applies to a document, not to the explorer"))
            }
            "format" | "kind" | "ft" | "filetype"
                if self.tab_ref().is_some_and(|t| t.explorer.is_some()) =>
            {
                self.error(format!(":{cmd} applies to a document, not to the explorer"))
            }
            "source" | "src" | "mode" | "format" | "kind" | "ft" | "filetype"
                if self.tab_ref().is_some_and(|t| t.listing.is_some()) =>
            {
                self.error(format!(
                    ":{cmd} applies to a document, not to the history tab"
                ))
            }
            "q" | "close" | "tabclose" => self.close_tab(),
            "qa" | "qall" | "quit" | "quitall" | "exit" => self.quit = true,
            "h" | "help" => self.show_help(),
            "error" | "errors" | "err" => self.show_error(),
            "set" | "se" => self.set_option(rest),
            "w" | "write" => self.write_file(rest, bang),
            "ws" | "writesource" => self.write_source(rest, bang),
            "history" => self.open_history(),
            // `:e!` (the bang was split off above) reloads, as in vim.
            "e" | "edit" if bang && rest.is_empty() => self.reload_active(),
            "open" | "o" | "e" | "edit" | "tabnew" | "tabe" | "tabedit" => self.cmd_open(rest),
            "tab" | "tabn" | "tabnext" | "next" | "n" if !rest.is_empty() => {
                match rest.parse::<usize>() {
                    Ok(n) if n >= 1 => self.goto_tab(n - 1),
                    _ => self.error(format!(":{cmd} needs a tab number, not {rest}")),
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
                _ => self.error(":watch takes on or off, or no word at all"),
            },
            "r" | "reload" => self.reload_active(),
            "format" | "kind" | "ft" | "filetype" => match Format::from_name(rest) {
                Some(f) => match self.tab().reformat(f, view) {
                    Ok(()) => {
                        self.sync_listings();
                        self.info(format!("Parsed as {f}"))
                    }
                    Err(e) => self.error(format!("{f}: {e} (! shows the report)")),
                },
                None if rest.is_empty() => self.error(format!(
                    ":{cmd} needs a format. The formats are {}.",
                    Format::known_names().join(", ")
                )),
                None => self.error(format!(
                    "{rest} is not a format. The formats are {}.",
                    Format::known_names().join(", ")
                )),
            },
            "depth" | "fold" => match rest.parse::<u32>() {
                Ok(d) => self.tab().expand_to_depth(d, view),
                Err(_) => self.error(":depth takes the number of levels to open, as in :depth 2"),
            },
            "expand" | "expandall" => self.tab().fold_siblings(true, true, view),
            "collapse" | "collapseall" => {
                let tab = self.tab();
                tab.top(view);
                tab.fold_siblings(false, true, view);
            }
            "line" | "goto" => match rest.parse::<usize>() {
                Ok(n) => self.tab().goto_row(n, view),
                Err(_) => self.error(":line takes a row number, as in :line 12"),
            },
            "source" | "src" => self.show_source(),
            "explore" | "explorer" | "browse" | "files" => {
                let dir = if rest.is_empty() { "." } else { rest };
                if !Path::new(dir).is_dir() {
                    self.error(format!(
                        "{dir} is not a directory, so aless cannot explore it"
                    ));
                } else {
                    self.open_explorer(Path::new(dir));
                }
            }
            "cd" => {
                if rest.is_empty() {
                    self.error(":cd takes a directory, as in :cd src");
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
                _ => self.error(":mode takes data or line"),
            },
            "yank" | "y" => self.yank(YankTarget::Pretty, false),
            _ => self.error(format!(
                ":{cmd} is not a command. F1 or :help lists every command."
            )),
        }
    }

    fn set_option(&mut self, rest: &str) {
        if rest.is_empty() {
            self.error(":set takes a setting, as in :set number. F1 lists every setting.");
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
                    Err(_) => self.error(format!(":set {name} needs a number, not {v}")),
                },
                ("indent" | "shiftwidth" | "sw", Some(v)) => match v.parse::<usize>() {
                    Ok(n) => self.opts.indent = n.min(16),
                    Err(_) => self.error(format!(":set {name} needs a number, not {v}")),
                },
                ("changes", None) => self.opts.changes = true,
                ("nochanges", None) => self.opts.changes = false,
                ("changes!", None) => self.opts.changes = !self.opts.changes,
                ("history", Some(v)) => match v.parse::<usize>() {
                    Ok(n) => {
                        self.opts.history = n;
                        let keep = self.keep();
                        for t in &mut self.tabs {
                            t.history.prune(keep);
                            t.refresh_marks();
                        }
                        self.sync_listings();
                    }
                    Err(_) => self.error(format!(":set {name} needs a number, not {v}")),
                },
                ("mode", Some(v)) => self.run_command(&format!("mode {v}")),
                ("depth", Some(v)) => self.run_command(&format!("depth {v}")),
                _ => self.error(format!("{word} is not a setting. F1 lists every setting.")),
            }
        }
    }

    fn cmd_open(&mut self, rest: &str) {
        if rest.is_empty() {
            self.error(":open needs a path. A format can follow the path.");
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
            self.error(":w needs a file name. :w FILE writes the document to FILE as JSON.");
            return;
        }
        match self.tree_json() {
            Ok(text) => self.write_out(rest, bang, "w", text + "\n"),
            Err(e) => self.error(e),
        }
    }

    /// `:ws FILE`: the text of the version on screen, as it was saved.
    fn write_source(&mut self, rest: &str, bang: bool) {
        if rest.is_empty() {
            self.error(":ws needs a file name. :ws FILE writes the text of the version on screen.");
            return;
        }
        match self.version_text() {
            Ok(text) => self.write_out(rest, bang, "ws", text),
            Err(e) => self.error(e),
        }
    }

    fn write_out(&mut self, rest: &str, bang: bool, cmd: &str, text: String) {
        let path = Path::new(rest);
        if path.exists() && !bang {
            self.error(format!("{rest} exists. :{cmd}! {rest} writes over it."));
            return;
        }
        match std::fs::write(path, text) {
            Ok(()) => self.info(format!("Wrote {rest}")),
            Err(e) => self.error(format!("Could not write {rest}: {e}")),
        }
    }

    /// What `:w` writes: the tree on screen as JSON, or in a listing tab
    /// the focused version's.
    fn tree_json(&mut self) -> Result<String, String> {
        if let Some((src, index)) = self.listed_version() {
            let v = &self.tabs[src].history.versions()[index];
            let loaded = load::load_str(v.source.to_string(), v.format)
                .map_err(|e| format!("Version {} no longer parses: {e}", v.number))?;
            return Ok(fmt::to_json_pretty(&loaded.doc, 0, 2));
        }
        let tab = self.tab();
        if tab.listing.is_some() {
            return Err("Focus a version to write it".to_string());
        }
        Ok(fmt::to_json_pretty(&tab.doc, 0, 2))
    }

    /// What `:ws` writes: the text of the version on screen, or in a
    /// listing tab the focused version's.
    fn version_text(&mut self) -> Result<String, String> {
        if let Some((src, index)) = self.listed_version() {
            return Ok(self.tabs[src].history.versions()[index].source.to_string());
        }
        let tab = self.tab();
        if tab.listing.is_some() {
            return Err("Focus a version to write its text".to_string());
        }
        if tab.explorer.is_some() {
            return Err("A directory has no text to write".to_string());
        }
        Ok(match tab.history.shown() {
            Some(v) => v.source.to_string(),
            None => tab.source.clone(),
        })
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
        // A listing tab's versions live in the tab just closed.
        let mut i = 0;
        while i < self.tabs.len() {
            if self.tabs[i]
                .listing
                .as_ref()
                .is_some_and(|l| l.of == tab.id)
            {
                self.tabs.remove(i);
                if i < self.active {
                    self.active -= 1;
                }
            } else {
                i += 1;
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
            self.error(format!("There is no tab {}", i + 1));
        }
    }

    // ----- watching ------------------------------------------------------------------------

    fn toggle_watch(&mut self, want: Option<bool>) {
        let tab = self.tab();
        if !tab.watchable() {
            self.error("This tab has no file, so there is nothing to watch");
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
            self.info("Watching the file");
        } else {
            tab.reload_due = None;
            self.effects.push(Effect::Unwatch(path));
            self.info("Not watching the file");
        }
    }

    fn reload_active(&mut self) {
        let view = self.view();
        let (now, keep) = (self.now(), self.keep());
        let tab = self.tab();
        if !tab.watchable() {
            self.error("This tab has no file, so there is nothing to reload");
            return;
        }
        let outcome = tab.reload_at(view, now, keep);
        self.report_reload(self.active, outcome);
        self.sync_listings();
    }

    fn report_reload(&mut self, i: usize, outcome: Reloaded) {
        let offset = self.opts.utc_offset;
        let (title, err, gone, nodes) = {
            let t = &self.tabs[i];
            (t.title.clone(), t.error.clone(), t.gone, t.doc.len())
        };
        if let Reloaded::Shown(counts) = outcome {
            self.tabs[i].note = self.opts.changes && i == self.active && counts.total() > 0;
        }
        match (err, outcome) {
            (Some(e), _) if gone => self.error(format!("{title}: the file is gone ({e})")),
            (Some(e), _) => self.error(format!("{title}: {e} (! shows the report)")),
            (None, Reloaded::Shown(counts)) if counts.total() > 0 => {
                self.info(format!("Reloaded {title}: {}", counts.describe()))
            }
            (None, Reloaded::Shown(_)) => self.info(format!(
                "Reloaded {title}: the text changed, the data did not"
            )),
            (None, Reloaded::Waiting(number, time)) => self.info(format!(
                "{title} changed at {}: version {number} is the newest. }} shows it",
                history::clock(time, offset)
            )),
            (None, _) => self.info(format!(
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
    }

    pub fn on_tick(&mut self, now: Instant) {
        let view = self.view();
        let (wall, keep) = (self.now(), self.keep());
        let mut reloaded = false;
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
                let outcome = tab.reload_at(view, wall, keep);
                self.report_reload(i, outcome);
                reloaded = true;
            }
        }
        if reloaded {
            self.sync_listings();
        }
    }

    /// Is a reload pending, so the terminal loop should tick soon?
    pub fn reload_pending(&self) -> bool {
        self.tabs.iter().any(|t| t.watch && t.reload_due.is_some())
    }

    // ----- overlays --------------------------------------------------------------------------

    pub fn show_help(&mut self) {
        self.overlay = Some(Overlay::new(
            "aless help: j and k scroll, h and l pan, and any other key returns".to_string(),
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
                self.info(format!("{title} has no error report"));
                return;
            }
        };
        let lines = err.report.lines().map(str::to_string).collect();
        let title =
            format!("{what}, error report: j and k scroll, h and l pan, and any other key returns");
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
        let h = self.pane_height();
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
        self.source_view = true;
    }

    fn scroll_source(&mut self, delta: isize) {
        let h = self.pane_height();
        let tab = self.tab();
        let total = load::lines(&tab.source).len();
        let max = total.saturating_sub(h);
        tab.source_scroll = (tab.source_scroll as isize + delta).clamp(0, max as isize) as usize;
    }

    fn source_key(&mut self, key: Key) {
        let h = self.pane_height() as isize;
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
            // The command line and a reload leave the text on screen.
            (KeyCode::Char(':'), false) => self.start_prompt(PromptKind::Command),
            (KeyCode::Char('r'), false) => self.reload_active(),
            // Any other key shows the tree again.
            _ => self.source_view = false,
        }
    }
}

/// Rows the error panel takes at the foot of a pane `height` rows tall showing
/// `tab`: while its file fails to parse and the last good document is on screen.
/// Counted even while an overlay or the source view covers it, so the tree keeps
/// the height it returns to and the tab's scroll does not move.
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

/// Do `a` and `b` name the same file? Canonical forms when both resolve,
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

/// `Version 2 of 3, saved 14:02:11`: the version on screen.
pub fn version_line(tab: &Tab, offset: Option<i32>) -> String {
    let Some(v) = tab.history.shown() else {
        return String::new();
    };
    let newest = tab.history.newest().map_or(v.number, |n| n.number);
    let verb = if v.number == 1 { "opened" } else { "saved" };
    format!(
        "Version {} of {newest}, {verb} {}",
        v.number,
        history::clock(v.time, offset)
    )
}

/// A part of the key map: what F1 shows, and the site's list of keys.
/// Code in a note or an action is between backticks, which F1 leaves out.
pub struct KeyGroup {
    pub title: &'static str,
    /// What holds for every binding in the group, or nothing.
    pub note: &'static str,
    /// The bindings are typed words, commands or settings, rather than
    /// keys.
    pub typed: bool,
    /// Each binding: its keys, and what they do.
    pub keys: &'static [(&'static str, &'static str)],
}

/// Every key and command of the viewer, in the order F1 lists them.
pub const KEY_MAP: &[KeyGroup] = &[
    KeyGroup {
        title: "Moving",
        note: "A count before a key repeats it: `3j` moves down three rows.",
        typed: false,
        keys: &[
            ("j k  ↓ ↑  Ctrl+N Ctrl+P", "Move down and up a row."),
            ("Enter Backspace", "Move down and up a row, as `j` and `k` do."),
            ("h  ←", "Fold the focused container, or go to its parent."),
            ("l  →", "Unfold the focused container, or go to its first child."),
            ("H", "Go to the parent, and leave it unfolded."),
            ("J K", "Go to the next and the previous sibling, and stop at the last and the first."),
            ("w b", "Go forward and back to the next row at another depth."),
            ("0  ^", "Go to the first sibling."),
            ("$", "Go to the last sibling."),
            ("g  Home", "Go to the first row. `Ng` goes to row N."),
            ("G  End", "Go to the last row. `NG` goes to row N."),
            ("Ctrl+F Ctrl+B  PgDn PgUp", "Move a page down and up."),
            ("Ctrl+D Ctrl+U", "Move half a page down and up. A count sets the distance, and later presses keep it."),
            ("Ctrl+E Ctrl+Y", "Scroll a row down and up. The focus moves only to keep `scrolloff` rows between it and the edge."),
            ("zz zt zb", "Put the focused row at the centre, the top, or the bottom of the screen."),
            (". ,", "Scroll a value longer than the screen a character right and left."),
            (";", "Scroll a long value to its end, and back to its start."),
            ("Esc", "Drop a count, or the first key of a two-key command."),
        ],
    },
    KeyGroup {
        title: "Folding and views",
        note: "",
        typed: false,
        keys: &[
            ("Space", "Fold or unfold the focused container."),
            ("c", "Fold the focused node and its siblings."),
            ("C", "Fold the focused node and its siblings, and everything inside them."),
            ("e", "Unfold the focused node and its siblings."),
            ("E", "Unfold the focused node and its siblings, and everything inside them."),
            ("m", "Switch between data mode and line mode."),
            ("%", "Go between a container's opening and closing brackets, in line mode."),
            ("s", "Show the source view in place of the tree: the text aless read, at the focused node's line. Any key but the scrolling ones, `:`, and `r` goes back."),
            ("< >", "Indent each level one space less or more."),
        ],
    },
    KeyGroup {
        title: "Versions of a watched file",
        note: "Each reload that parses is a version. Marks show what the version on screen \
               changed from the one before: yellow for a changed value, green for a new one, \
               a red count for removed ones, and rails from the root down to each change.",
        typed: false,
        keys: &[
            ("[ ]", "Show the version before, and the version after, the one on screen. `3[` goes back three."),
            ("{ }", "Show the oldest version kept, and the newest."),
            ("d D", "Go to the next and the previous change, round the end of the document."),
            ("v", "List the versions in the history tab, the newest first."),
        ],
    },
    KeyGroup {
        title: "Search",
        note: "A pattern is a regular expression. It ignores case unless it holds a capital \
               letter or ends in `/s`. The characters `[ ] { }` match themselves.",
        typed: false,
        keys: &[
            ("/ ?", "Search forward and backward. `Enter` with no pattern searches for the last pattern again."),
            ("n N", "Go to the next and the previous match, round the end of the document."),
            ("* #", "Search forward and backward for the focused node's key."),
        ],
    },
    KeyGroup {
        title: "Copy and print",
        note: "`y` and a second key copy the text to the clipboard. `p` and the same second \
               key print it on screen instead. `pP` prints the path, because `pp` prints \
               the value.",
        typed: false,
        keys: &[
            ("yy pp", "The value, pretty-printed as JSON"),
            ("yv pv", "The value as JSON on one line"),
            ("ys ps", "A string's contents, without its quotes"),
            ("yk pk", "The key"),
            ("yp pP", "The path, as jless writes it: `.a[0].b`"),
            ("yb pb", "The path, with every step in brackets: `[\"a\"][0][\"b\"]`"),
            ("yq pq", "The path, as jq writes it"),
        ],
    },
    KeyGroup {
        title: "Tabs and files",
        note: "",
        typed: false,
        keys: &[
            ("Tab Shift+Tab", "Go to the next and the previous tab."),
            ("q", "Close the tab. Closing the last tab quits."),
            ("Ctrl+C", "Quit."),
            ("r", "Read the file again now."),
            ("W", "Stop or start watching the file."),
            ("!", "Show the parse error report full size. `h` and `l` pan a long line."),
            (":", "Start a command, from the Commands list."),
            ("F1", "Show this help."),
            ("Ctrl+Z", "Suspend aless, on Unix."),
        ],
    },
    KeyGroup {
        title: "The explorer",
        note: "A directory is a tree with the same keys. `m`, `%`, and `s` do nothing in it.",
        typed: false,
        keys: &[
            ("Enter", "Open the focused file in a new tab, or fold or unfold a directory."),
            ("-", "Go up: the parent directory becomes the root."),
            ("yp", "Copy the focused entry's path on disk."),
            ("/", "Search the entries listed so far: each row's name, size, and format."),
        ],
    },
    KeyGroup {
        title: "The history tab",
        note: "`v` lists the versions of a watched file in a tab, the newest first, each with \
               what it changed. The tab follows the file as it changes, and closes with the \
               file's tab. `:w` and `:ws` write the focused version. `m`, `%`, and `s` do \
               nothing in it.",
        typed: false,
        keys: &[
            ("Enter", "Open the focused version in a tab of its own, which is not watched, at the focused change."),
            ("v", "Go to the history tab from a version's own tab, or from the file's tab."),
        ],
    },
    KeyGroup {
        title: "Commands",
        note: "Type `:` and the command, then `Enter`.",
        typed: true,
        keys: &[
            (":open PATH [FORMAT]  :e", "Open a file or a directory in a new tab."),
            (":q  :close", "Close the tab."),
            (":qa  :quit  :exit", "Quit."),
            (":tab N", "Go to tab N."),
            (":tabnext  :tabprev", "Go to the next and the previous tab."),
            (":tabfirst  :tablast", "Go to the first and the last tab."),
            (":w FILE", "Write the document to FILE as JSON, but not over a file that exists."),
            (":w! FILE", "Write the document to FILE as JSON, over a file that exists."),
            (":ws FILE", "Write the text of the version on screen to FILE, but not over a file that exists."),
            (":ws! FILE", "Write the text of the version on screen to FILE, over a file that exists."),
            (":history", "List the versions in the history tab."),
            (":format FORMAT", "Read the tab's text again as FORMAT."),
            (":mode data  :mode line", "Switch to data mode or line mode."),
            (":depth N", "Open the first N levels, and fold every container below them."),
            (":expand", "Unfold the focused node and its siblings, and everything inside them."),
            (":collapse", "Fold everything, and go to the first row."),
            (":N  :line N", "Go to row N."),
            (":reload  :e!", "Read the file again now."),
            (":watch on  :watch off", "Start or stop watching the file."),
            (":source", "Show the source view."),
            (":error", "Show the parse error report full size."),
            (":cd DIR", "Make DIR the explorer's root. DIR is relative to the current root. In a document's tab, open DIR in a new explorer tab."),
            (":explore [DIR]", "Open DIR, or the current directory, in a new explorer tab."),
            (":set SETTING", "Change a setting, from the Settings list."),
            (":help", "Show this help."),
        ],
    },
    KeyGroup {
        title: "Settings",
        note: "Each follows `:set`, and one `:set` takes several.",
        typed: true,
        keys: &[
            ("number  nonumber  number!", "Number the rows, or stop, or switch between the two. `nu` is the short form."),
            ("relativenumber  norelativenumber  relativenumber!", "Number each row by its distance from the focus, or stop, or switch between the two. `rnu` is the short form."),
            ("so=N", "Keep N rows between the focus and the edge of the screen."),
            ("indent=N  sw=N", "Indent N spaces a level, at most 16."),
            ("ascii  noascii", "Draw the fold markers as `v` and `>`, or as `▼ ▽ ▶ ▷`."),
            ("hidden  nohidden  hidden!", "Show dot-files in the explorer, or hide them, or switch between the two."),
            ("watch  nowatch", "Start or stop watching the file."),
            ("changes  nochanges  changes!", "Mark what each version changed, or stop, or switch between the two."),
            ("history=N", "Keep up to N versions of each file, and fewer of a large one. The version on screen and the newest always stay."),
            ("mode=data  mode=line", "Switch to data mode or line mode."),
            ("depth=N", "Open the first N levels, and fold every container below them."),
        ],
    },
    KeyGroup {
        title: "Typing a command or a pattern",
        note: "",
        typed: false,
        keys: &[
            ("Enter", "Run the command, or search for the pattern."),
            ("Esc  Ctrl+C", "Leave without running the command or the search."),
            ("Backspace  Ctrl+H  Delete", "Delete the character before or under the cursor."),
            ("← →  Ctrl+B Ctrl+F", "Move the cursor a character."),
            ("Home End  Ctrl+A Ctrl+E", "Move the cursor to the start or the end."),
            ("Ctrl+U  Ctrl+K  Ctrl+W", "Delete to the start, to the end, or the word before the cursor."),
        ],
    },
    KeyGroup {
        title: "This help, the error report, and printed text",
        note: "Any other key goes back to the document.",
        typed: false,
        keys: &[
            ("j k  ↓ ↑", "Scroll a line."),
            ("Ctrl+D Ctrl+U  Ctrl+F Ctrl+B  PgDn PgUp", "Scroll a page."),
            ("g G  Home End", "Go to the first and the last line."),
            ("h l  ← →", "Pan a line wider than the screen."),
        ],
    },
];

/// The help F1 shows: [`KEY_MAP`], a group a section, its keys in a
/// column.
pub fn help_lines() -> Vec<String> {
    const WIDTH: usize = 78;
    let mut out = vec![format!(
        "aless {}: every key and command",
        env!("CARGO_PKG_VERSION")
    )];
    for group in KEY_MAP {
        out.push(String::new());
        out.push(group.title.to_uppercase());
        if !group.note.is_empty() {
            let note = group.note.replace('`', "");
            out.extend(wrap(&note, WIDTH - 2).into_iter().map(|l| format!("  {l}")));
        }
        let column = group
            .keys
            .iter()
            .map(|(keys, _)| keys.chars().count())
            .filter(|&w| w <= 24)
            .max()
            .unwrap_or(0);
        for (keys, does) in group.keys {
            let shown = keys.chars().count();
            let mut lines = wrap(&does.replace('`', ""), WIDTH - 4 - column).into_iter();
            if shown > column {
                out.push(format!("  {keys}"));
            } else if let Some(first) = lines.next() {
                out.push(format!("  {keys}{}  {first}", " ".repeat(column - shown)));
            }
            for line in lines {
                out.push(format!("  {}  {line}", " ".repeat(column)));
            }
        }
    }
    out
}

/// `text` in lines of at most `width` characters, broken at spaces.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split(' ') {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What aless prints of itself follows the documentation's style: no
    /// em dash in the key map, `--help` or `-h`.
    #[test]
    fn its_own_text_has_no_em_dash() {
        for (what, text) in [
            ("F1", help_lines().join("\n")),
            ("--help", crate::cli::reference()),
            ("-h", crate::cli::summary()),
        ] {
            assert!(!text.contains('\u{2014}'), "{what} has an em dash");
        }
    }

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

    /// A file saved over and over: a year changes, a tag goes, a title
    /// changes and a tag comes, a book comes.
    const V1: &str = "[\n  {\"title\": \"A\", \"year\": 1999, \"tags\": [\"x\", \"y\"]},\n  {\"title\": \"B\", \"tags\": []}\n]\n";
    const V2: &str = "[\n  {\"title\": \"A\", \"year\": 2001, \"tags\": [\"x\", \"y\"]},\n  {\"title\": \"B\", \"tags\": []}\n]\n";
    const V3: &str = "[\n  {\"title\": \"A\", \"year\": 2001, \"tags\": [\"x\"]},\n  {\"title\": \"B!\", \"tags\": [\"z\"]}\n]\n";
    const V4: &str = "[\n  {\"title\": \"A\", \"year\": 2001, \"tags\": [\"x\"]},\n  {\"title\": \"B!\", \"tags\": [\"z\"]},\n  {\"title\": \"C\"}\n]\n";

    /// An app watching `text` in a file of its own, its clock at 14:00:05.
    fn watched(name: &str, text: &str) -> (App, PathBuf) {
        let p = write_temp(name, text);
        let mut app = App::new(Options::default(), 80, 24);
        app.opts.utc_offset = Some(0);
        app.clock = Some(std::time::UNIX_EPOCH + Duration::from_secs(14 * 3600 + 5));
        app.open_path(&p, None);
        (app, p)
    }

    /// Write `text` and reload it as the watcher would, `secs` after 14:00.
    fn save(app: &mut App, p: &Path, text: &str, secs: u64) {
        std::fs::write(p, text).unwrap();
        app.clock = Some(std::time::UNIX_EPOCH + Duration::from_secs(14 * 3600 + secs));
        app.handle(Input::FileChanged(p.to_path_buf()));
        app.handle(Input::Tick(Instant::now() + RELOAD_DEBOUNCE * 2));
    }

    fn said(app: &App) -> String {
        app.message
            .as_ref()
            .map(|m| m.text.clone())
            .unwrap_or_default()
    }

    fn shown(app: &App) -> usize {
        app.tab_ref().unwrap().history.shown().unwrap().number
    }

    #[test]
    fn a_reload_records_a_version_and_marks_what_it_changed() {
        let (mut app, p) = watched("versions.json", V1);
        assert_eq!(app.timeline_rows(), 0, "one version has no timeline");
        save(&mut app, &p, V2, 131);
        assert_eq!(app.tab().history.len(), 2);
        assert_eq!(said(&app), "Reloaded versions.json: 1 changed");
        assert_eq!(app.timeline_rows(), 1);
        assert_eq!(app.note_rows(), 2, "the rule and the change");
        keys(&mut app, "j");
        assert_eq!(app.note_rows(), 0, "the next key closes the note");
        assert!(!app.tab().marks.is_empty(), "the marks stay");
        save(&mut app, &p, V2, 140);
        assert_eq!(
            app.tab().history.len(),
            2,
            "the same text is no new version"
        );
        assert!(said(&app).starts_with("Reloaded"));
    }

    #[test]
    fn brackets_step_through_the_versions_and_a_save_waits() {
        let (mut app, p) = watched("steps.json", V1);
        save(&mut app, &p, V2, 131);
        save(&mut app, &p, V3, 220);
        keys(&mut app, "[");
        assert_eq!(shown(&app), 2);
        assert!(app.tab().source.contains("\"y\""), "version 2's text");
        assert!(app.note_rows() > 0, "the note opens for the version");
        save(&mut app, &p, V4, 302);
        assert_eq!(
            shown(&app),
            2,
            "a save waits while an older version is on screen"
        );
        assert_eq!(
            said(&app),
            "steps.json changed at 14:05:02: version 4 is the newest. } shows it"
        );
        keys(&mut app, "{");
        assert_eq!(shown(&app), 1);
        assert!(
            app.tab().marks.is_empty(),
            "the file as opened changed nothing"
        );
        keys(&mut app, "[");
        assert_eq!(said(&app), "Version 1 is the oldest kept");
        keys(&mut app, "}");
        assert_eq!(shown(&app), 4);
        assert!(app.tab().history.at_newest());
        keys(&mut app, "2[");
        assert_eq!(shown(&app), 2);
        keys(&mut app, "]");
        assert_eq!(shown(&app), 3);
    }

    #[test]
    fn d_and_shift_d_go_round_the_changes() {
        let (mut app, p) = watched("jumps.json", V2);
        save(&mut app, &p, V3, 220);
        keys(&mut app, "d");
        assert_eq!(said(&app), "Change 1 of 3: [0].tags: 1 removed");
        assert_eq!(path(&mut app), "[0].tags");
        keys(&mut app, "d");
        assert_eq!(said(&app), "Change 2 of 3: [1].title changed");
        keys(&mut app, "d");
        assert_eq!(said(&app), "Change 3 of 3: [1].tags[0] added");
        keys(&mut app, "d");
        assert_eq!(
            said(&app),
            "Change 1 of 3: [0].tags: 1 removed",
            "round the end"
        );
        keys(&mut app, "D");
        assert_eq!(said(&app), "Change 3 of 3: [1].tags[0] added");
        assert!(app.note_rows() > 0, "d and D keep the note open");
    }

    #[test]
    fn settings_turn_the_marks_off_and_bound_the_versions() {
        let (mut app, p) = watched("settings.json", V1);
        save(&mut app, &p, V2, 131);
        app.run_command("set nochanges");
        assert_eq!(app.note_rows(), 0, "no note without the marks");
        save(&mut app, &p, V3, 220);
        assert!(!app.tab().note);
        keys(&mut app, "[");
        assert_eq!(said(&app), "Version 2 of 3, saved 14:02:11");
        app.run_command("set changes history=2");
        let numbers: Vec<usize> = app
            .tab()
            .history
            .versions()
            .iter()
            .map(|v| v.number)
            .collect();
        assert_eq!(numbers, [2, 3], "the version on screen and the newest");
        app.run_command("set history=x");
        assert!(app.message.as_ref().unwrap().error);
    }

    fn listed(app: &App) -> usize {
        let list = app.tab_ref().unwrap().listing.as_ref().unwrap();
        list.items
            .iter()
            .filter(|i| matches!(i, Item::Version { .. }))
            .count()
    }

    /// A path for a file a test writes, with nothing there yet.
    fn fresh(name: &str) -> PathBuf {
        let p = write_temp(name, "");
        std::fs::remove_file(&p).unwrap();
        p
    }

    #[test]
    fn v_lists_the_versions_and_enter_opens_one_in_its_own_tab() {
        let (mut app, p) = watched("listed.json", V1);
        save(&mut app, &p, V2, 131);
        save(&mut app, &p, V3, 220);
        keys(&mut app, "v");
        assert_eq!(app.tab().title, "listed.json history");
        assert_eq!(
            said(&app),
            "listed.json: 3 versions. Enter opens one in a tab of its own"
        );
        assert_eq!(app.tab().focused_node(), 1, "on the version on screen");
        keys(&mut app, "J");
        app.handle(Input::Key(Key::code(KeyCode::Enter)));
        let tab = app.tab();
        assert_eq!(tab.title, "listed.json v2");
        assert!(!tab.watch, "a version's own tab is not watched");
        assert_eq!(tab.frozen.map(|f| f.number), Some(2));
        assert!(!tab.marks.is_empty(), "version 2's own marks");
        assert_eq!(
            said(&app),
            "Version 2 of listed.json, saved 14:02:11, in a tab of its own"
        );
        let text = fresh("v2.json");
        app.run_command(&format!("ws {}", text.display()));
        assert_eq!(
            std::fs::read_to_string(&text).unwrap(),
            V2,
            "the text as saved"
        );
        let json = fresh("v2-tree.json");
        app.run_command(&format!("w {}", json.display()));
        let written = std::fs::read_to_string(&json).unwrap();
        assert!(
            written.contains("2001") && written.contains("\"y\""),
            "{written}"
        );
        keys(&mut app, "[");
        assert!(said(&app).starts_with("This tab holds version 2 alone"));
        keys(&mut app, "v");
        assert_eq!(
            app.tab().title,
            "listed.json history",
            "v goes back to the list"
        );
    }

    #[test]
    fn the_history_tab_follows_the_saves_and_writes_the_focused_version() {
        let (mut app, p) = watched("follows.json", V1);
        save(&mut app, &p, V2, 131);
        keys(&mut app, "v");
        assert_eq!(listed(&app), 2);
        save(&mut app, &p, V3, 220);
        assert_eq!(listed(&app), 3, "a save to the file adds a row");
        let at = app.tab().focused_node();
        let list = app.tab_ref().unwrap().listing.as_ref().unwrap();
        assert!(
            matches!(list.items[at as usize], Item::Version { number: 2, .. }),
            "the focus stays on its version"
        );
        let text = fresh("focused.json");
        app.run_command(&format!("ws {}", text.display()));
        assert_eq!(std::fs::read_to_string(&text).unwrap(), V2);
        keys(&mut app, "yy");
        assert_eq!(said(&app), "Open a version with Enter to copy from it");
        app.run_command("format yaml");
        assert_eq!(
            said(&app),
            ":format applies to a document, not to the history tab"
        );
        app.run_command("set mode=line");
        assert!(!app.tab().line_mode, "{}", said(&app));
        keys(&mut app, "[");
        assert_eq!(
            said(&app),
            "The history tab lists the versions: Enter opens one"
        );
    }

    #[test]
    fn enter_on_a_change_opens_its_version_there_and_reuses_the_tab() {
        let (mut app, p) = watched("there.json", V1);
        save(&mut app, &p, V2, 131);
        save(&mut app, &p, V3, 220);
        save(&mut app, &p, "[1,,]", 300);
        app.run_command("history");
        assert_eq!(app.tab().title, "there.json history");
        // The focus starts on the version on screen: `j` is its first change.
        keys(&mut app, "j");
        app.handle(Input::Key(Key::code(KeyCode::Enter)));
        assert_eq!(app.tab().title, "there.json v3");
        assert_eq!(path(&mut app), "[0].tags", "a removed value: its parent");
        let tabs = app.tabs.len();
        keys(&mut app, "v");
        keys(&mut app, "jj");
        app.handle(Input::Key(Key::code(KeyCode::Enter)));
        assert_eq!(app.tabs.len(), tabs, "the tab of version 3 again");
        assert_eq!(app.tab().title, "there.json v3");
        assert_eq!(path(&mut app), "[1].tags[0]");
        keys(&mut app, "v");
        // The failed save, as the newest entry, is the first row inside.
        keys(&mut app, "ggj");
        app.handle(Input::Key(Key::code(KeyCode::Enter)));
        assert_eq!(
            said(&app),
            "This save did not parse, so it has no tree to open"
        );
        keys(&mut app, "G");
        app.handle(Input::Key(Key::code(KeyCode::Enter)));
        assert_eq!(app.tab().title, "there.json v1");
        // The file's own tab goes, and its listing tab with it.
        app.goto_tab(0);
        app.close_tab();
        assert!(app.tabs.iter().all(|t| t.listing.is_none()));
        assert_eq!(app.tab().title, "there.json v3", "a version keeps its tab");
        keys(&mut app, "v");
        assert_eq!(
            said(&app),
            "The file's own tab is closed, so its versions are gone"
        );
    }

    #[test]
    fn format_starts_the_versions_again_in_the_history_tab_too() {
        let (mut app, p) = watched("again.json", V1);
        save(&mut app, &p, V2, 131);
        save(&mut app, &p, V3, 220);
        keys(&mut app, "vG");
        app.handle(Input::Key(Key::code(KeyCode::Enter)));
        assert_eq!(app.tab().title, "again.json v1");
        let tabs = app.tabs.len();
        app.goto_tab(0);
        app.run_command("format text");
        keys(&mut app, "v");
        assert_eq!(listed(&app), 1, "the versions start again");
        keys(&mut app, "G");
        let text = fresh("again-text.json");
        app.run_command(&format!("ws {}", text.display()));
        assert_eq!(std::fs::read_to_string(&text).unwrap(), V3);
        // After `:format`, Enter on the oldest version opens another tab.
        app.handle(Input::Key(Key::code(KeyCode::Enter)));
        assert_eq!(app.tabs.len(), tabs + 1);
        assert_eq!(app.tab().format, Format::Text);
    }

    #[test]
    fn search_in_the_history_tab_matches_what_its_rows_show() {
        let (mut app, p) = watched("found.json", V1);
        save(&mut app, &p, V2, 131);
        keys(&mut app, "v/year");
        app.handle(Input::Key(Key::code(KeyCode::Enter)));
        let at = app.tab().focused_node() as usize;
        let list = app.tab_ref().unwrap().listing.as_ref().unwrap();
        assert!(
            history::item_text(list, at as NodeId).starts_with("[0].year  1999 → 2001"),
            "{}",
            said(&app)
        );
        // A save rebuilds the rows, and the matches with them.
        save(&mut app, &p, V3, 220);
        let tab = app.tab_ref().unwrap();
        let list = tab.listing.as_ref().unwrap();
        let found = &tab.search.as_ref().unwrap().matches;
        assert_eq!(found.len(), 1);
        assert!(history::item_text(list, found[0]).contains("year"));
    }

    #[test]
    fn ws_writes_the_text_of_the_version_on_screen() {
        let (mut app, p) = watched("text.json", V1);
        save(&mut app, &p, V2, 131);
        keys(&mut app, "[");
        let text = fresh("first.json");
        app.run_command(&format!("ws {}", text.display()));
        assert_eq!(std::fs::read_to_string(&text).unwrap(), V1);
        app.run_command(&format!("ws {}", text.display()));
        assert!(said(&app).ends_with(&format!(":ws! {} writes over it.", text.display())));
    }

    #[test]
    fn a_tab_with_one_version_says_so() {
        let mut app = app_with(SRC);
        keys(&mut app, "[");
        assert_eq!(said(&app), "t.json has one version so far");
        keys(&mut app, "d");
        assert_eq!(said(&app), "t.json has no changes to go to");
    }

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
            "The focused value is not a string, so there are no string contents to copy or print"
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
            .starts_with(":bogus is not a command"));
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
        assert!(app.source_view);
        keys(&mut app, "j");
        assert!(app.source_view);
        app.handle(Input::Key(Key::code(KeyCode::Esc)));
        assert!(!app.source_view);
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
        // root, sub (collapsed), .hidden.json, a.json: dot-files show
        assert_eq!(app.tab().row_count(), 4);
        keys(&mut app, "j");
        assert_eq!(path(&mut app), ".sub");
        keys(&mut app, "l"); // expand: deep gets listed for its preview
        assert_eq!(app.tab().row_count(), 6);
        let ex = app.tab().explorer.as_ref().unwrap();
        assert!(ex.is_listed(&ex.root.join("sub").join("deep")));
        keys(&mut app, "j");
        assert_eq!(path(&mut app), ".sub.deep");
        app.handle(Input::Key(Key::code(KeyCode::Enter))); // toggle a directory
        assert_eq!(path(&mut app), ".sub.deep");
        assert_eq!(app.tab().row_count(), 7);
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
        // :set nohidden hides the dot-file; hidden shows it again.
        assert_eq!(app.tab().doc.root().children, 3);
        app.run_command("set nohidden");
        assert_eq!(app.tab().doc.root().children, 2);
        app.run_command("set hidden");
        assert_eq!(app.tab().doc.root().children, 3);
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
        // The failed save goes on the timeline, whose row the pane gives up.
        assert_eq!(app.timeline_rows(), 1);
        assert_eq!(app.view().height, full - 1 - panel);
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
        assert!(app
            .message
            .as_ref()
            .unwrap()
            .text
            .ends_with("no error report"));
    }

    #[test]
    fn a_failed_format_keeps_its_report_for_bang() {
        let p = write_temp("fmt.json", "{\"a\": [1, 2]}");
        let mut app = App::new(Options::default(), 80, 24);
        app.open_path(&p, None);
        app.run_command("format toml");
        let m = app.message.clone().unwrap();
        assert!(m.error && m.text.ends_with("(! shows the report)"), "{m:?}");
        // The document still parses as JSON: nothing is docked or marked.
        assert!(app.tab().error.is_none());
        assert_eq!(app.error_panel_rows(), 0);
        keys(&mut app, "!");
        assert_eq!(app.mode, Mode::Overlay);
        let o = app.overlay.as_ref().unwrap();
        assert!(
            o.title.starts_with("fmt.json as toml, error report"),
            "{}",
            o.title
        );
        assert!(o.lines[1].contains("fmt.json:1:"), "{:?}", o.lines);
        keys(&mut app, "x");
        // A reload drops it...
        keys(&mut app, "r");
        app.run_command("error");
        assert!(app
            .message
            .as_ref()
            .unwrap()
            .text
            .ends_with("no error report"));
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
            let covered = app.mode != Mode::Browse || app.source_view;
            assert!(covered, "{open} covers the tree");
            crate::render::screen(&mut app);
            keys(&mut app, close);
            crate::render::screen(&mut app);
            assert_eq!((app.tab().focus, app.tab().scroll), place, "after {open}");
        }
    }

    /// The source view is coloured once the colours come in. A reload drops
    /// them until the reloaded text's come, so no colour lands on a token it
    /// was not made for.
    #[test]
    fn colours_follow_the_text() {
        use tabnas_lsp::TokenType;
        let p = write_temp("colours.json", "{\"a\": 1}\n");
        let mut app = App::new(Options::default(), 80, 24);
        app.open_path(&p, None);
        keys(&mut app, "s");
        app.settle_colours();
        let kinds = |app: &App| -> Option<Vec<TokenType>> {
            let painted = app.painted()?;
            Some(painted.line(0).iter().map(|r| r.kind).collect())
        };
        assert!(kinds(&app).unwrap().contains(&TokenType::Number));
        std::fs::write(&p, "{\"a\": \"one\"}\n").unwrap();
        keys(&mut app, "r");
        assert!(app.source_view);
        assert_eq!(kinds(&app), None, "no colours for the new text yet");
        app.settle_colours();
        let after = kinds(&app).unwrap();
        assert!(!after.contains(&TokenType::Number), "{after:?}");
        assert_eq!(after.iter().filter(|k| **k == TokenType::String).count(), 2);
    }

    /// Without colour no colours are made: nothing is asked of the
    /// painter, so no text is parsed for them and nothing is pending.
    #[test]
    fn no_colours_are_made_without_colour() {
        let opts = Options {
            color: false,
            ..Options::default()
        };
        let mut app = App::new(opts, 80, 24);
        app.open_source("t.json", "{\"a\": 1}\n".into(), Format::Json);
        keys(&mut app, "s");
        app.prepare();
        assert!(!app.highlight_pending(), "nothing is asked");
        assert!(app.source_view);
        assert!(app.painted().is_none());
    }

    /// A click gives the focus to the row under it; below the tree, and in
    /// the source view, it moves nothing.
    #[test]
    fn a_click_focuses_the_row() {
        let mut app = app_with("[1, 2, 3]");
        app.prepare();
        app.handle(Input::Click(3, 2));
        assert_eq!(app.tab().focus, 2);
        app.handle(Input::Click(3, 20));
        assert_eq!(app.tab().focus, 2, "below the tree, nothing moves");
        keys(&mut app, "s");
        app.handle(Input::Click(3, 0));
        assert_eq!(app.tab().focus, 2, "the source view takes no click");
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
