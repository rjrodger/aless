//! The application: tabs, modes, the key map and the command line. Pure
//! state — the terminal is driven from `main.rs`, which feeds [`Input`]s in
//! and paints what [`crate::render`] draws from the state.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::doc::{Kind, NodeId};
use crate::fmt;
use crate::load::{self, Format};
use crate::search::{self, Direction};
use crate::tab::{Reposition, Tab, View};

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
    /// Left click on a screen row.
    Click(u16),
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
    /// The help text, the printed value of a `p` command, and so on.
    Overlay,
    /// The raw source text of the active tab.
    Source,
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
        }
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

    pub fn view(&self) -> View {
        View::new(self.pane_height(), self.opts.scrolloff)
    }

    pub fn tab(&mut self) -> &mut Tab {
        let i = self.active.min(self.tabs.len().saturating_sub(1));
        &mut self.tabs[i]
    }

    pub fn tab_ref(&self) -> Option<&Tab> {
        self.tabs.get(self.active)
    }

    pub fn count_text(&self) -> Option<String> {
        self.count.map(|c| c.to_string())
    }

    /// Bring the active tab's rows and window up to date before a paint.
    pub fn prepare(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        let view = self.view();
        let tab = self.tab();
        tab.follow(view);
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
        let id = self.next_id;
        self.next_id += 1;
        match load::load_str(source.clone(), format) {
            Ok(loaded) => self.adopt(Tab::new(id, title.to_string(), None, loaded)),
            Err(err) => {
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
                tab.error = Some(err.clone());
                self.error(format!("{title}: {err}"));
                self.adopt(tab);
            }
        }
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
            serde_json::to_string(&Format::ALL.iter().map(|f| f.name()).collect::<Vec<_>>())
                .unwrap_or_default()
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
                if self.mode == Mode::Browse && !self.tabs.is_empty() {
                    let view = self.view();
                    self.tab().scroll_by(delta as isize, view);
                } else if self.mode == Mode::Source {
                    self.scroll_source(delta as isize);
                } else if let Some(o) = self.overlay.as_mut() {
                    o.scroll = (o.scroll as isize + delta as isize).max(0) as usize;
                }
            }
            Input::Click(row) => self.click(row as usize),
            Input::FileChanged(path) => self.on_file_changed(&path),
            Input::Tick(now) => self.on_tick(now),
        }
        self.prepare();
    }

    fn click(&mut self, row: usize) {
        if self.mode != Mode::Browse || self.tabs.is_empty() {
            return;
        }
        let strip = self.strip_rows();
        if strip == 1 && row == 0 {
            return;
        }
        let pane_row = row.saturating_sub(strip);
        if pane_row >= self.pane_height() {
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
            Mode::Browse => self.browse_key(key),
            Mode::Prompt(kind) => self.prompt_key(kind, key),
            Mode::Overlay => self.overlay_key(key),
            Mode::Source => self.source_key(key),
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
            (KeyCode::Char('q'), false) => self.close_tab(),
            (KeyCode::Esc, _) => {}
            (KeyCode::F(1), _) => self.show_help(),
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
        if self.tabs.is_empty() || self.tab().explorer.is_none() {
            return;
        }
        let view = self.view();
        let tab = self.tab();
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
        let view = self.view();
        let tab = self.tab();
        let Some(root) = tab.explorer.as_ref().map(|ex| ex.root.clone()) else {
            return;
        };
        match root.parent() {
            Some(parent) => {
                let parent = parent.to_path_buf();
                tab.reroot(&parent, view);
                self.explorer_sync();
            }
            None => self.error("Already at the top of the filesystem"),
        }
    }

    /// `:cd DIR` in an explorer tab: re-root, relative to the current root.
    fn explorer_cd(&mut self, dir: &str) {
        let view = self.view();
        let tab = self.tab();
        let Some(root) = tab.explorer.as_ref().map(|ex| ex.root.clone()) else {
            self.open_explorer(Path::new(dir));
            return;
        };
        let target = root.join(dir);
        if !target.is_dir() {
            self.error(format!("{}: not a directory", target.display()));
            return;
        }
        tab.reroot(&target, view);
        self.explorer_sync();
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
                    self.overlay = Some(Overlay {
                        title: format!("{} — press any key to continue", target.describe()),
                        lines,
                        scroll: 0,
                    });
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
            "q" | "close" | "tabclose" => self.close_tab(),
            "qa" | "qall" | "quit" | "quitall" | "exit" => self.quit = true,
            "h" | "help" => self.show_help(),
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
                Some(f) => match self.tab().reformat(f, view) {
                    Ok(()) => self.info(format!("Parsed as {f}")),
                    Err(e) => self.error(format!("{f}: {e}")),
                },
                None => self.error(format!(
                    "Unknown format: {rest} (one of {})",
                    Format::ALL
                        .iter()
                        .map(|f| f.name())
                        .collect::<Vec<_>>()
                        .join(", ")
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
        let tab = self.tab();
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
        let view = self.view();
        let tab = self.tab();
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
            Some(e) => self.error(format!("{title}: {e}")),
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
    }

    pub fn on_tick(&mut self, now: Instant) {
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
    }

    // ----- overlays --------------------------------------------------------------------------

    pub fn show_help(&mut self) {
        self.overlay = Some(Overlay {
            title: "aless help — j/k scroll, any other key returns".to_string(),
            lines: help_lines(),
            scroll: 0,
        });
        self.mode = Mode::Overlay;
    }

    fn overlay_key(&mut self, key: Key) {
        let page = self.pane_height().saturating_sub(1).max(1);
        let Some(o) = self.overlay.as_mut() else {
            self.mode = Mode::Browse;
            return;
        };
        let max = o.lines.len().saturating_sub(1);
        match (key.code, key.ctrl) {
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
        self.mode = Mode::Source;
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
            _ => self.mode = Mode::Browse,
        }
    }
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

TABS, FILES AND WATCHING
  Tab / Shift-Tab   next / previous tab      :tab N   go to tab N
  :open PATH [FORMAT]   open a file (or a directory) in a new tab (:e is an alias)
  q / :q    close the tab (the last one closed quits)   :qa / :quit   quit
  W / :watch on|off   toggle reloading the tab when its file changes
  r / :reload         reload now
  s / :source         show the raw source (the focused node's line centred)
  :format FORMAT      parse the tab's text as another format
  :depth N            fold everything below depth N
  :w[!] FILE          write the document as JSON
  :set number | nonumber | relativenumber | norelativenumber | so=N | indent=N
  :N or :line N       go to row N
  F1 / :help          this help

  A watched tab reloads when its file changes and keeps your place: the
  focused node is found again by path, or by its nearest surviving
  ancestor and then the node closest to its old source line; folds that
  still exist are kept, and the focus stays on the same screen row.
  Formats: json jsonl jsonic jsonc json5 yaml toml ini csv tsv xml zon
  markdown feed text (by extension; --kind or :open ... FORMAT to force).
"#;
    HELP.lines().map(str::to_string).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(app.mode, Mode::Source);
        keys(&mut app, "j");
        assert_eq!(app.mode, Mode::Source);
        app.handle(Input::Key(Key::code(KeyCode::Esc)));
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
        assert!(ex.is_listed(&ex.root.join("sub/deep")));
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
        assert_eq!(root, std::fs::canonicalize(&dir).unwrap());
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
