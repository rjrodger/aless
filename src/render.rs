//! Drawing: the whole screen as ratatui widgets, computed from the
//! application state. Terminal-free: [`draw`] renders into a ratatui
//! `Frame`, whatever backend holds it. `main.rs` hands it the terminal's,
//! and [`screen`] an in-memory one, so tests read what a user would see.
//!
//! Every width here is measured the way ratatui's buffer places text: by
//! grapheme cluster, each taking the cells [`CellWidth`] gives it. A line
//! fitted to the pane's width then fills exactly that many cells, whether
//! it holds CJK, an emoji sequence, a combining mark or a halfwidth sound
//! mark.

use std::borrow::Cow;

use ratatui::backend::TestBackend;
use ratatui::buffer::{Buffer, CellWidth};
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;
use ratatui::{Frame, Terminal};

use crate::app::{App, Mode, PromptKind};
use crate::doc::{Kind, Row};
use crate::fmt;
use crate::load;

const PLAIN: Style = Style::new();
const GREY: Style = Style::new().fg(Color::DarkGray);
const KEY: Style = Style::new().fg(Color::Blue);
const STRING: Style = Style::new().fg(Color::Green);
const NUMBER: Style = Style::new().fg(Color::Magenta);
const BOOL: Style = Style::new().fg(Color::Yellow);
const NULL: Style = GREY;
const BRACKET: Style = PLAIN;
const PREVIEW: Style = GREY;
const MATCH: Style = Style::new()
    .fg(Color::Yellow)
    .add_modifier(Modifier::UNDERLINED);
const FOCUS: Style = Style::new()
    .add_modifier(Modifier::REVERSED)
    .add_modifier(Modifier::BOLD);
const GUTTER: Style = GREY;
const GUTTER_FOCUS: Style = Style::new().fg(Color::Yellow);
const REVERSED: Style = Style::new().add_modifier(Modifier::REVERSED);
const BAR: Style = REVERSED;
const ERROR: Style = Style::new().fg(Color::Red);

/// What aless needs of a line beyond what ratatui's has: text that never
/// carries a control character, and widths as the buffer counts them.
pub trait Cells {
    /// Append text. Control characters (C0, DEL, C1) never reach the
    /// terminal: a file name or a string value carrying an escape sequence
    /// must not be able to drive it. A tab becomes a space and every other
    /// control character the replacement character ([`sanitize`]).
    fn put(&mut self, text: impl Into<String>, style: Style) -> &mut Self;

    /// The cells the line takes.
    fn cols(&self) -> usize;

    /// Plain text, styles dropped.
    fn plain(&self) -> String;

    /// Drop the first `cols` columns. A wide grapheme the cut would split
    /// goes whole.
    fn skip_cols(&mut self, cols: usize);

    /// Cut or pad to exactly `width` columns, padding with `style`.
    fn fit_cols(&mut self, width: usize, style: Style);
}

impl Cells for Line<'static> {
    fn put(&mut self, text: impl Into<String>, style: Style) -> &mut Self {
        let text = sanitize(text.into());
        if !text.is_empty() {
            self.spans.push(Span::styled(text, style));
        }
        self
    }

    fn cols(&self) -> usize {
        self.spans.iter().map(|s| cols(&s.content)).sum()
    }

    fn plain(&self) -> String {
        self.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    fn skip_cols(&mut self, cols: usize) {
        let mut left = cols;
        let mut keep = Vec::new();
        for span in self.spans.drain(..) {
            let w = self::cols(&span.content);
            if left == 0 {
                keep.push(span);
            } else if w <= left {
                left -= w;
            } else {
                let text = skip_cols(&span.content, left);
                left = 0;
                keep.push(Span::styled(text, span.style));
            }
        }
        self.spans = keep;
    }

    fn fit_cols(&mut self, width: usize, style: Style) {
        let mut used = 0;
        let mut keep = Vec::new();
        for span in self.spans.drain(..) {
            let w = cols(&span.content);
            if used + w <= width {
                used += w;
                keep.push(span);
            } else {
                let (cut, cw) = take_cols(&span.content, width - used);
                used += cw;
                if !cut.is_empty() {
                    keep.push(Span::styled(cut, span.style));
                }
                break;
            }
        }
        self.spans = keep;
        if used < width {
            self.put(" ".repeat(width - used), style);
        }
    }
}

/// Replace control characters so that rendered text cannot carry terminal
/// escape sequences.
pub fn sanitize(text: String) -> String {
    if !text.chars().any(char::is_control) {
        return text;
    }
    text.chars()
        .map(|c| match c {
            '\t' => ' ',
            c if c.is_control() => '\u{fffd}',
            c => c,
        })
        .collect()
}

/// `text` as it is drawn: sanitised, borrowed when nothing needed it.
fn clean(text: &str) -> Cow<'_, str> {
    if text.chars().any(char::is_control) {
        Cow::Owned(sanitize(text.to_string()))
    } else {
        Cow::Borrowed(text)
    }
}

/// The grapheme clusters of clean `text` with the cells each takes, as
/// ratatui's buffer counts them (a zero-width cluster takes none, and the
/// buffer draws it nowhere).
fn graphemes(text: &str) -> Vec<(&str, usize)> {
    let span = Span::raw(text);
    let mut at = 0;
    span.styled_graphemes(PLAIN)
        .map(|g| {
            // Clean text has no control character for the iterator to
            // drop, so the clusters tile the text in order.
            let cluster = &text[at..at + g.symbol.len()];
            debug_assert_eq!(cluster, g.symbol);
            at += g.symbol.len();
            (cluster, usize::from(g.symbol.cell_width()))
        })
        .collect()
}

/// The columns `text` takes on screen.
pub fn cols(text: &str) -> usize {
    graphemes(&clean(text)).iter().map(|(_, w)| w).sum()
}

/// The leading part of `text` that fits in `width` columns, and its width.
fn take_cols(text: &str, width: usize) -> (String, usize) {
    let text = clean(text);
    let mut out = String::new();
    let mut used = 0;
    for (cluster, w) in graphemes(&text) {
        if used + w > width {
            break;
        }
        used += w;
        out.push_str(cluster);
    }
    (out, used)
}

/// `text` with its first `cols` columns skipped; a cluster the cut would
/// split goes whole.
fn skip_cols(text: &str, cols: usize) -> String {
    let text = clean(text);
    let mut used = 0;
    let mut at = 0;
    for (cluster, w) in graphemes(&text) {
        if used >= cols {
            break;
        }
        used += w;
        at += cluster.len();
    }
    text[at..].to_string()
}

/// A line of text carrying ANSI SGR colour codes (an engine error report)
/// as styled spans. Other escape sequences are dropped, and what remains
/// is sanitised like any other text.
pub fn ansi_line(text: &str) -> Line<'static> {
    let mut line = Line::default();
    let mut style = PLAIN;
    let mut buf = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            buf.push(c);
            continue;
        }
        match chars.next() {
            Some('[') => {
                let mut params = String::new();
                let mut fin = None;
                for n in chars.by_ref() {
                    if ('\x40'..='\x7e').contains(&n) {
                        fin = Some(n);
                        break;
                    }
                    params.push(n);
                }
                if fin == Some('m') {
                    line.put(std::mem::take(&mut buf), style);
                    style = apply_sgr(style, &params);
                }
            }
            Some(']') => {
                while let Some(n) = chars.next() {
                    if n == '\x07' {
                        break;
                    }
                    if n == '\x1b' {
                        if chars.peek() == Some(&'\\') {
                            chars.next();
                        }
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    line.put(buf, style);
    line
}

fn apply_sgr(mut style: Style, params: &str) -> Style {
    let codes: Vec<u32> = if params.is_empty() {
        vec![0]
    } else {
        params.split(';').map(|p| p.parse().unwrap_or(0)).collect()
    };
    let color = |n: u32| match n % 10 {
        0 => Color::Black,
        1 => Color::Red,
        2 => Color::Green,
        3 => Color::Yellow,
        4 => Color::Blue,
        5 => Color::Magenta,
        6 => Color::Cyan,
        _ => Color::White,
    };
    let mut i = 0;
    while i < codes.len() {
        // An attribute turned off leaves the style as if it had never been
        // on: the modifier is cleared, not recorded as removed.
        match codes[i] {
            0 => style = PLAIN,
            1 => style.add_modifier.insert(Modifier::BOLD),
            2 => style.add_modifier.insert(Modifier::DIM),
            4 => style.add_modifier.insert(Modifier::UNDERLINED),
            7 => style.add_modifier.insert(Modifier::REVERSED),
            22 => style.add_modifier.remove(Modifier::BOLD | Modifier::DIM),
            24 => style.add_modifier.remove(Modifier::UNDERLINED),
            27 => style.add_modifier.remove(Modifier::REVERSED),
            n @ 30..=37 => style.fg = Some(color(n)),
            39 => style.fg = None,
            n @ 40..=47 => style.bg = Some(color(n)),
            49 => style.bg = None,
            90 => style.fg = Some(Color::DarkGray),
            n @ 91..=97 => style.fg = Some(color(n)),
            100 => style.bg = Some(Color::DarkGray),
            n @ 101..=107 => style.bg = Some(color(n)),
            // 256-colour and true-colour forms: skip their arguments.
            38 | 48 => {
                i += match codes.get(i + 1) {
                    Some(5) => 2,
                    Some(2) => 4,
                    _ => 0,
                };
            }
            _ => {}
        }
        i += 1;
    }
    style
}

/// Fit `text` into `avail` columns, scrolled right by `xoff`: an ellipsis
/// marks a cut at either end. Returns the text and whether it was cut on
/// the right.
pub fn clip(text: &str, xoff: usize, avail: usize) -> (String, bool) {
    let w = cols(text);
    if avail == 0 {
        return (String::new(), w > 0);
    }
    if w <= avail && xoff == 0 {
        return (text.to_string(), false);
    }
    if avail == 1 {
        return ("…".to_string(), true);
    }
    // The furthest useful offset shows the tail with a leading ellipsis.
    let max_xoff = w.saturating_sub(avail - 1);
    let xoff = xoff.min(max_xoff);
    if xoff == 0 {
        let (head, _) = take_cols(text, avail - 1);
        return (format!("{head}…"), true);
    }
    let rest = skip_cols(text, xoff);
    let rest_w = cols(&rest);
    if rest_w < avail {
        (format!("…{rest}"), false)
    } else {
        let (mid, _) = take_cols(&rest, avail - 2);
        (format!("…{mid}…"), true)
    }
}

fn digits(n: usize) -> usize {
    n.max(1).to_string().len()
}

/// A count of cells as a terminal coordinate.
fn cells_u16(n: usize) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}

/// The next `rows` rows of `area` from `*top` down, cut at its foot;
/// `*top` moves past them.
fn band(area: Rect, top: &mut u16, rows: usize) -> Rect {
    let y = (*top).clamp(area.top(), area.bottom());
    let height = cells_u16(rows).min(area.bottom() - y);
    *top = y + height;
    Rect::new(area.x, y, area.width, height)
}

/// Lines drawn top to bottom into an area, each cut at its width.
struct Rows(Vec<Line<'static>>);

impl Widget for Rows {
    fn render(self, area: Rect, buf: &mut Buffer) {
        for (line, y) in self.0.iter().zip(area.top()..area.bottom()) {
            buf.set_line(area.x, y, line, area.width);
        }
    }
}

/// Draw the whole screen into `frame`, a widget in its own rows for each
/// part: the tab strip, the pane (the tree and its error panel, the
/// source view, or an overlay), the status bar and the prompt. While a
/// prompt is edited, the terminal cursor goes to its insertion point,
/// which is returned.
pub fn draw(frame: &mut Frame, app: &mut App) -> Option<Position> {
    app.prepare();
    let width = app.width.max(1);
    // The app lays out for the size it was last told; a terminal resized
    // a moment ago may be smaller until the resize arrives.
    let area =
        Rect::new(0, 0, cells_u16(width), cells_u16(app.height.max(1))).intersection(frame.area());
    let mut top = area.top();
    let strip = band(area, &mut top, app.strip_rows());
    let pane_h = app.pane_height();
    let pane = band(area, &mut top, pane_h);
    let status = band(area, &mut top, 1);
    let prompt = band(area, &mut top, 1);

    if strip.height > 0 {
        frame.render_widget(Rows(vec![tab_strip(app, width)]), strip);
    }
    match app.mode {
        Mode::Overlay => frame.render_widget(Rows(overlay_lines(app, width, pane_h)), pane),
        Mode::Source => frame.render_widget(Rows(source_lines(app, width, pane_h)), pane),
        _ => draw_tree_pane(frame, app, width, pane_h, pane),
    }
    frame.render_widget(Rows(vec![status_bar(app, width)]), status);
    let (line, col) = prompt_line(app, width);
    frame.render_widget(Rows(vec![line]), prompt);
    let cursor = col
        .filter(|_| prompt.height > 0)
        .map(|col| Position::new(prompt.x + cells_u16(col), prompt.y));
    if let Some(at) = cursor {
        frame.set_cursor_position(at);
    }
    if !app.opts.color {
        drop_colour(frame.buffer_mut(), area);
    }
    cursor
}

/// Without colour, a cell keeps bold and reverse, which mark the focus
/// and the bars, and loses its colours, dim and underline.
fn drop_colour(buf: &mut Buffer, area: Rect) {
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            let cell = &mut buf[(x, y)];
            cell.fg = Color::Reset;
            cell.bg = Color::Reset;
            cell.modifier.remove(Modifier::DIM | Modifier::UNDERLINED);
        }
    }
}

/// What a terminal of the app's size shows after one [`draw`].
#[derive(Clone, Debug, PartialEq)]
pub struct Screen {
    pub width: usize,
    pub height: usize,
    /// The cells as drawn.
    pub buffer: Buffer,
    /// Where the terminal cursor is shown, column then row, when it is.
    pub cursor: Option<(u16, u16)>,
}

impl Screen {
    /// Row `y` as text: each cell's grapheme once, a wide one standing for
    /// the cells it covers.
    pub fn row(&self, y: usize) -> String {
        let mut out = String::new();
        let mut covered = 0;
        for x in 0..self.width {
            if covered > 0 {
                covered -= 1;
                continue;
            }
            let symbol = self.buffer[(cells_u16(x), cells_u16(y))].symbol();
            out.push_str(symbol);
            covered = usize::from(symbol.cell_width()).saturating_sub(1);
        }
        out
    }

    /// Every row as text, trailing blanks dropped, joined with newlines.
    pub fn text(&self) -> String {
        (0..self.height)
            .map(|y| self.row(y).trim_end().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The style of the cell at column `x` of row `y`.
    pub fn style(&self, x: usize, y: usize) -> Style {
        self.buffer[(cells_u16(x), cells_u16(y))].style()
    }
}

/// Draw the app into an in-memory terminal of its size, and return what it
/// shows: the screen as a user would see it, for tests.
pub fn screen(app: &mut App) -> Screen {
    let (width, height) = (app.width.max(1), app.height.max(1));
    let backend = TestBackend::new(cells_u16(width), cells_u16(height));
    let mut terminal = Terminal::new(backend).expect("an in-memory terminal cannot fail");
    let mut cursor = None;
    terminal
        .draw(|frame| cursor = draw(frame, app))
        .expect("an in-memory terminal cannot fail");
    Screen {
        width,
        height,
        buffer: terminal.backend().buffer().clone(),
        cursor: cursor.map(|p| (p.x, p.y)),
    }
}

fn tab_strip(app: &App, width: usize) -> Line<'static> {
    let mut line = Line::default();
    for (i, tab) in app.tabs.iter().enumerate() {
        let mut marks = String::new();
        if tab.watch {
            marks.push('*');
        }
        if tab.error.is_some() {
            marks.push('!');
        }
        if tab.gone {
            marks.push(if app.opts.ascii { 'x' } else { '✗' });
        }
        let text = format!(" {}:{}{} ", i + 1, tab.title, marks);
        let style = if i == app.active { FOCUS } else { GREY };
        line.put(text, style);
        line.put(" ", PLAIN);
    }
    line.fit_cols(width, PLAIN);
    line
}

/// The tree pane: the document's rows and, while a reload fails, the
/// error panel under them; a tab that never loaded shows its report in
/// the whole pane.
fn draw_tree_pane(frame: &mut Frame, app: &mut App, width: usize, pane_h: usize, area: Rect) {
    if app.tabs.is_empty() {
        return;
    }
    let numbers = app.opts.numbers;
    let relative = app.opts.relative;
    let indent = app.opts.indent;
    let ascii = app.opts.ascii;
    let panel = app.error_panel_rows();
    let tab = app.tab();
    if tab.shows_error_only() {
        let report = tab
            .error
            .as_ref()
            .map(|e| e.report.clone())
            .unwrap_or_default();
        let lines = report_lines(&report, width, pane_h, "! shows the full report");
        frame.render_widget(Rows(lines), area);
        return;
    }
    let rows: Vec<Row> = tab.rows().to_vec();
    let focus = tab.focus;
    let scroll = tab.scroll;
    let gutter = if numbers || relative {
        digits(rows.len()) + 1
    } else {
        0
    };
    let tree = TreeStyle {
        width,
        gutter,
        numbers,
        relative,
        indent,
        ascii,
    };
    let report = (panel > 0).then(|| {
        tab.error
            .as_ref()
            .map(|e| e.report.clone())
            .unwrap_or_default()
    });
    let mut top = area.top();
    let tree_area = band(area, &mut top, pane_h - panel);
    let panel_area = band(area, &mut top, panel);
    let lines = tree_lines(tab, &rows, &tree, pane_h - panel, scroll, focus);
    frame.render_widget(Rows(lines), tree_area);
    if let Some(report) = report {
        frame.render_widget(Rows(error_panel(&report, width, panel, ascii)), panel_area);
    }
}

/// The error panel docked under the tree: a rule naming it, then as much
/// of the report as fits in the rest of its `height` rows.
fn error_panel(report: &str, width: usize, height: usize, ascii: bool) -> Vec<Line<'static>> {
    let mut rule = Line::default();
    let label = " parse error · ! shows the full report ";
    let dash = if ascii { "-" } else { "─" };
    rule.put(dash.repeat(2), ERROR);
    rule.put(label, ERROR.bold());
    rule.put(dash.repeat(width.saturating_sub(cols(label) + 2)), ERROR);
    rule.fit_cols(width, PLAIN);
    let mut out = vec![rule];
    out.extend(report_lines(report, width, height - 1, "…"));
    out
}

/// An error report in `height` rows. When it does not fit, the last row
/// says so with `more`.
fn report_lines(report: &str, width: usize, height: usize, more: &str) -> Vec<Line<'static>> {
    let all: Vec<&str> = report.lines().collect();
    let mut out = Vec::with_capacity(height);
    for i in 0..height {
        let mut l = if all.len() > height && i + 1 == height {
            let mut l = Line::default();
            l.put(format!("  {more}"), GREY);
            l
        } else {
            all.get(i).map(|t| ansi_line(t)).unwrap_or_default()
        };
        l.fit_cols(width, PLAIN);
        out.push(l);
    }
    out
}

/// How tree rows are laid out.
struct TreeStyle {
    width: usize,
    gutter: usize,
    numbers: bool,
    relative: bool,
    indent: usize,
    ascii: bool,
}

fn tree_lines(
    tab: &mut crate::tab::Tab,
    rows: &[Row],
    style: &TreeStyle,
    pane_h: usize,
    scroll: usize,
    focus: usize,
) -> Vec<Line<'static>> {
    let TreeStyle {
        width,
        gutter,
        numbers,
        relative,
        indent,
        ascii,
    } = *style;
    let mut out = Vec::with_capacity(pane_h);
    for i in scroll..scroll + pane_h {
        let mut line = Line::default();
        if let Some(row) = rows.get(i) {
            let focused = i == focus;
            if gutter > 0 {
                let n = if relative && !(numbers && focused) {
                    i.abs_diff(focus)
                } else {
                    i + 1
                };
                let text = format!("{:>w$} ", n, w = gutter - 1);
                line.put(text, if focused { GUTTER_FOCUS } else { GUTTER });
            }
            let node = tab.doc.node(row.node);
            let line_mode = tab.line_mode;
            let is_match = tab.is_match(row.node) && !row.close;
            let has_next = tab.doc.next_sibling(row.node).is_some();
            line.put(" ".repeat(node.depth as usize * indent), PLAIN);
            // Indicator.
            let indicator = if row.close || !node.is_foldable() {
                "  "
            } else if ascii {
                if node.expanded {
                    "v "
                } else {
                    "> "
                }
            } else {
                match (node.expanded, focused) {
                    (true, true) => "▼ ",
                    (true, false) => "▽ ",
                    (false, true) => "▶ ",
                    (false, false) => "▷ ",
                }
            };
            line.put(indicator, if focused { PLAIN.bold() } else { PLAIN });
            if let Some(ex) = &tab.explorer {
                explorer_row(
                    &mut line, ex, &tab.doc, row.node, focused, is_match, width, tab.xoff,
                );
                line.fit_cols(width, PLAIN);
                out.push(line);
                continue;
            }
            let mut key_shown = false;
            if !row.close {
                if let Some(k) = fmt::key_text(&node.key, line_mode) {
                    let style = if focused {
                        FOCUS
                    } else if is_match {
                        MATCH
                    } else {
                        KEY
                    };
                    line.put(k, style);
                    line.put(": ", PLAIN);
                    key_shown = true;
                }
            }
            // Value.
            let avail = width.saturating_sub(line.cols());
            let (value, style, comma) = if row.close {
                (
                    fmt::close_bracket(&node.kind).to_string(),
                    BRACKET,
                    line_mode && has_next,
                )
            } else if node.is_foldable() {
                if node.expanded {
                    (fmt::open_bracket(&node.kind).to_string(), BRACKET, false)
                } else {
                    (
                        fmt::preview(&tab.doc, row.node, avail.saturating_sub(1)),
                        PREVIEW,
                        line_mode && has_next,
                    )
                }
            } else {
                let style = match &node.kind {
                    Kind::Str(_) => STRING,
                    Kind::Number(_) => NUMBER,
                    Kind::Bool(_) => BOOL,
                    Kind::Null => NULL,
                    Kind::Object | Kind::Array => BRACKET,
                };
                let text = match &node.kind {
                    Kind::Object => "{}".to_string(),
                    Kind::Array => "[]".to_string(),
                    k => fmt::leaf_text(k),
                };
                (text, style, line_mode && has_next)
            };
            let style = if focused && !key_shown {
                FOCUS
            } else if is_match && !key_shown {
                MATCH
            } else {
                style
            };
            let xoff = if focused { tab.xoff } else { 0 };
            let comma_w = usize::from(comma);
            let (text, cut) = clip(&value, xoff, avail.saturating_sub(comma_w));
            line.put(text, style);
            if comma && !cut {
                line.put(",", PLAIN);
            }
        }
        line.fit_cols(width, PLAIN);
        out.push(line);
    }
    out
}

/// One explorer row after its indicator: the name (with a slash for a
/// directory), then the size and format of a file, or the names inside a
/// collapsed directory.
#[allow(clippy::too_many_arguments)]
fn explorer_row(
    line: &mut Line<'static>,
    ex: &crate::explorer::Explorer,
    doc: &crate::doc::Doc,
    id: crate::doc::NodeId,
    focused: bool,
    is_match: bool,
    width: usize,
    xoff: usize,
) {
    let node = doc.node(id);
    let is_dir = node.is_container();
    let name = match &node.key {
        crate::doc::Key::Name(n) => Some(if is_dir {
            format!("{n}/")
        } else {
            n.to_string()
        }),
        _ => None,
    };
    let (label, label_style) = match name {
        Some(n) => (
            n,
            if focused {
                FOCUS
            } else if is_match {
                MATCH
            } else if is_dir {
                KEY
            } else {
                PLAIN
            },
        ),
        // The root row: the directory itself. A long path keeps its tail,
        // the part that says where this is.
        None => {
            let full = format!("{}/", ex.root.display());
            let avail = width.saturating_sub(line.cols());
            let label = if cols(&full) > avail && avail > 1 {
                format!("…{}", skip_cols(&full, cols(&full) + 1 - avail))
            } else {
                full
            };
            (label, if focused { FOCUS } else { KEY.bold() })
        }
    };
    line.put(label, label_style);
    let avail = width.saturating_sub(line.cols() + 2);
    let value = if is_dir {
        if node.children == 0 {
            "(empty)".to_string()
        } else if node.expanded {
            String::new()
        } else {
            explorer_preview(doc, id, avail.saturating_sub(1))
        }
    } else {
        match &node.kind {
            Kind::Str(s) => s.to_string(),
            k => fmt::leaf_text(k),
        }
    };
    if !value.is_empty() {
        line.put("  ", PLAIN);
        let (text, _) = clip(&value, if focused { xoff } else { 0 }, avail);
        line.put(text, PREVIEW);
    }
}

/// `(3) src/, Cargo.toml, README.md` — the names inside a collapsed
/// directory, as many as fit.
fn explorer_preview(doc: &crate::doc::Doc, id: crate::doc::NodeId, budget: usize) -> String {
    let node = doc.node(id);
    let mut out = format!("({}) ", node.children);
    let mut first = true;
    for c in doc.children(id) {
        let child = doc.node(c);
        let mut item = child.key.name().unwrap_or("").to_string();
        if child.is_container() {
            item.push('/');
        }
        let sep = if first { "" } else { ", " };
        if cols(&out) + cols(sep) + cols(&item) + 2 > budget {
            out.push_str(if first { "…" } else { ", …" });
            return out;
        }
        out.push_str(sep);
        out.push_str(&item);
        first = false;
    }
    out
}

fn overlay_lines(app: &App, width: usize, pane_h: usize) -> Vec<Line<'static>> {
    let mut out = Vec::with_capacity(pane_h);
    let Some(o) = app.overlay.as_ref() else {
        return out;
    };
    let mut title = Line::default();
    title.put(format!(" {} ", o.title), REVERSED);
    title.fit_cols(width, REVERSED);
    out.push(title);
    for i in 0..pane_h.saturating_sub(1) {
        let mut l = Line::default();
        if let Some(text) = o.lines.get(o.scroll + i) {
            if o.ansi {
                l = ansi_line(text);
            } else {
                l.put(text.as_str(), PLAIN);
            }
            l = pan(l, o.xoff, width);
        }
        l.fit_cols(width, PLAIN);
        out.push(l);
    }
    out
}

/// An overlay row: `line` panned right by `xoff` columns and cut to
/// `width`, with `…` at either edge that hides text.
fn pan(mut line: Line<'static>, xoff: usize, width: usize) -> Line<'static> {
    if xoff > 0 {
        if line.cols() <= xoff {
            return Line::default();
        }
        line.skip_cols(xoff + 1);
        let mut cut = Line::default();
        cut.put("…", PREVIEW);
        cut.spans.append(&mut line.spans);
        line = cut;
    }
    if line.cols() > width {
        line.fit_cols(width.saturating_sub(1), PLAIN);
        line.put("…", PREVIEW);
    }
    line
}

fn source_lines(app: &mut App, width: usize, pane_h: usize) -> Vec<Line<'static>> {
    let mut out = Vec::with_capacity(pane_h);
    if app.tabs.is_empty() {
        return out;
    }
    let ascii = app.opts.ascii;
    let tab = app.tab();
    let focus_line = tab.focused_line().map(|(l, _)| l as usize);
    let error_line = tab
        .error
        .as_ref()
        .filter(|e| e.line > 0)
        .map(|e| e.line as usize);
    let lines = load::lines(&tab.source);
    let gutter = digits(lines.len()) + 1;
    let scroll = tab.source_scroll;
    for i in 0..pane_h {
        let mut line = Line::default();
        let n = scroll + i;
        if let Some(text) = lines.get(n) {
            let lineno = n + 1;
            let is_focus = focus_line == Some(lineno);
            let is_error = error_line == Some(lineno);
            let num_style = if is_error {
                ERROR.bold()
            } else if is_focus {
                GUTTER_FOCUS
            } else {
                GUTTER
            };
            let marker = if is_error {
                "!"
            } else if is_focus {
                if ascii {
                    ">"
                } else {
                    "▶"
                }
            } else {
                " "
            };
            line.put(
                format!("{:>w$}{} ", lineno, marker, w = gutter - 1),
                num_style,
            );
            let shown: String = text
                .chars()
                .map(|c| match c {
                    '\t' => "    ".to_string(),
                    c if (c as u32) < 0x20 => "·".to_string(),
                    c => c.to_string(),
                })
                .collect();
            let (t, _) = clip(&shown, 0, width.saturating_sub(line.cols()));
            let style = if is_focus {
                REVERSED
            } else if is_error {
                ERROR
            } else {
                PLAIN
            };
            line.put(t, style);
            if is_focus {
                line.fit_cols(width, REVERSED);
            }
        }
        line.fit_cols(width, PLAIN);
        out.push(line);
    }
    out
}

fn status_bar(app: &mut App, width: usize) -> Line<'static> {
    let mut line = Line::default();
    if app.tabs.is_empty() {
        line.put(" aless ", BAR);
        line.fit_cols(width, BAR);
        return line;
    }
    let mode = app.mode;
    let tab = app.tab();
    let node = tab.focused_node();
    let node_path = tab.doc.path(node);
    let mut left = format!(" {}", tab.title);
    let mut right = Vec::new();
    if let Some(ex) = &tab.explorer {
        left = format!(" {}", ex.fs_path(&node_path).display());
        right.push("directory".to_string());
        let n = tab.doc.node(node);
        if n.is_container() {
            right.push(format!("{} entries", n.children));
        }
    } else {
        let path = fmt::path_dot(&node_path);
        if mode == Mode::Source {
            left.push_str("  (source)");
        } else {
            left.push(' ');
            left.push_str(if path.is_empty() { "." } else { &path });
        }
        right.push(tab.format.name().to_string());
    }
    if tab.line_mode {
        right.push("line".to_string());
    }
    if tab.watch {
        right.push("watching".to_string());
    }
    if tab.gone {
        right.push("file gone".to_string());
    }
    if let Some((l, c)) = tab.focused_line() {
        right.push(format!("{l}:{c}"));
    }
    let error = tab.error.as_ref().map(|e| format!(" !{e} "));
    let right = format!(" {} ", right.join(" · "));
    // The format block is short and always shown; the error and the path
    // share what is left, the error clipped first, the path cut from its
    // start (its tail is the interesting part).
    let remaining = width.saturating_sub(cols(&right));
    let err_text = error.map(|e| {
        let room = remaining.saturating_sub(cols(&left)).max(remaining / 2);
        clip(&e, 0, room).0
    });
    let err_w = err_text.as_deref().map(cols).unwrap_or(0);
    let avail_left = remaining.saturating_sub(err_w);
    let left = if cols(&left) > avail_left {
        let cut = cols(&left) + 1 - avail_left.max(1);
        format!("…{}", skip_cols(&left, cut))
    } else {
        left
    };
    line.put(&left, BAR);
    let pad = width.saturating_sub(cols(&left) + cols(&right) + err_w);
    line.put(" ".repeat(pad), BAR);
    if let Some(e) = err_text {
        line.put(e, BAR.bold());
    }
    line.put(right, BAR);
    line.fit_cols(width, BAR);
    line
}

fn prompt_line(app: &mut App, width: usize) -> (Line<'static>, Option<usize>) {
    let mut line = Line::default();
    let mut cursor = None;
    match app.mode {
        Mode::Prompt(kind) => {
            let prefix = match kind {
                PromptKind::Command => ':',
                PromptKind::Search(d) => d.prompt(),
            };
            // Show the part of the buffer around the cursor: a long path or
            // pattern scrolls rather than hiding what is being typed.
            let chars: Vec<char> = app.prompt.buf.chars().collect();
            let at = app.prompt.cursor.min(chars.len());
            let avail = width.saturating_sub(1).max(1);
            let width_of =
                |from: usize| -> usize { cols(&chars[from..at].iter().collect::<String>()) };
            let mut start = 0;
            while start < at && width_of(start) >= avail {
                start += 1;
            }
            line.put(prefix.to_string(), PLAIN);
            line.put(chars[start..].iter().collect::<String>(), PLAIN);
            cursor = Some((1 + width_of(start)).min(width.saturating_sub(1)));
        }
        _ => {
            if let Some(m) = &app.message {
                let (t, _) = clip(&m.text, 0, width);
                line.put(t, if m.error { ERROR.bold() } else { PLAIN });
            } else if let Some(s) = app.tab_ref().and_then(|t| t.search.as_ref()) {
                if let Some(i) = s.current {
                    line.put(
                        format!(
                            "{}{}  [{}/{}]{}",
                            s.direction.prompt(),
                            s.pattern.input,
                            i + 1,
                            s.matches.len(),
                            if s.wrapped { "  W" } else { "" }
                        ),
                        GREY,
                    );
                }
            }
            if let Some(c) = app.count_text() {
                let used = line.cols();
                let pad = width.saturating_sub(used + cols(&c) + 1);
                line.put(" ".repeat(pad), PLAIN);
                line.put(c, PLAIN.bold());
            }
        }
    }
    line.fit_cols(width, PLAIN);
    (line, cursor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{Input, Key, Options};
    use crate::load::Format;

    fn app(src: &str, w: u16, h: u16) -> App {
        let mut app = App::new(Options::default(), w, h);
        app.open_source("t.json", src.to_string(), Format::Json);
        app
    }

    #[test]
    fn clipping() {
        assert_eq!(clip("hello", 0, 10), ("hello".into(), false));
        assert_eq!(clip("hello world", 0, 6), ("hello…".into(), true));
        assert_eq!(clip("hello world", 3, 6), ("…lo w…".into(), true));
        assert_eq!(clip("hello world", 6, 6), ("…world".into(), false));
        assert_eq!(clip("hello world", 99, 6), ("…world".into(), false));
        assert_eq!(clip("日本語", 0, 4), ("日…".into(), true));
        assert_eq!(clip("abc", 0, 1), ("…".into(), true));
        assert_eq!(clip("abc", 0, 0), ("".into(), true));
    }

    #[test]
    fn data_mode_screen() {
        let mut a = app(
            r#"{"name": "x", "list": [1, {"k": true}], "e": {}, "n": null}"#,
            40,
            12,
        );
        let s = screen(&mut a);
        assert_eq!(s.height, 12);
        assert!((0..12).all(|y| cols(&s.row(y)) == 40));
        let text = s.text();
        let expect = "\
▼ {
    name: \"x\"
  ▽ list: [
      1
    ▽ {
        k: true
      }
    e: {}
    n: null";
        // Data mode shows no closing brackets: the `}` line above must not
        // appear.
        let expect = expect.replace("      }\n", "");
        assert!(text.starts_with(&expect), "screen was:\n{text}");
        assert!(
            text.contains(" t.json ."),
            "status shows the title and root path"
        );
        assert!(text.contains("json"), "status shows the format");
    }

    #[test]
    fn line_mode_and_collapse() {
        let mut a = app(r#"{"a": [1, 2], "b": "s"}"#, 40, 10);
        a.handle(Input::Key(Key::ch('m')));
        let text = screen(&mut a).text();
        let expect = "\
▼ {
  ▽ \"a\": [
      1,
      2
    ],
    \"b\": \"s\"
  }";
        assert!(text.starts_with(expect), "screen was:\n{text}");
        a.handle(Input::Key(Key::ch('m')));
        a.handle(Input::Key(Key::ch('j')));
        a.handle(Input::Key(Key::ch('h')));
        let text = screen(&mut a).text();
        assert!(text.contains("▶ a: (2) [1, 2]"), "screen was:\n{text}");
        assert!(text.contains(" t.json .a"), "screen was:\n{text}");
    }

    #[test]
    fn truncation_and_scrolling_of_long_values() {
        let long = "x".repeat(100);
        let mut a = app(&format!(r#"{{"k": "{long}"}}"#), 30, 8);
        a.handle(Input::Key(Key::ch('j')));
        let text = screen(&mut a).text();
        let row = text.lines().nth(1).unwrap();
        assert_eq!(cols(row), 30);
        assert!(row.ends_with('…'), "{row}");
        a.handle(Input::Key(Key::ch(';')));
        let text = screen(&mut a).text();
        let row = text.lines().nth(1).unwrap();
        assert!(row.contains("…xxx"), "{row}");
        assert!(row.ends_with("\""), "{row}");
    }

    #[test]
    fn numbers_prompt_and_tabs() {
        let mut a = app(r#"[1, 2, 3]"#, 30, 8);
        a.run_command("set number");
        let text = screen(&mut a).text();
        assert!(text.lines().nth(1).unwrap().starts_with("2 "), "{text}");
        a.run_command("set relativenumber nonumber");
        a.handle(Input::Key(Key::ch('j')));
        let text = screen(&mut a).text();
        assert!(text.lines().next().unwrap().starts_with("1 "), "{text}");
        assert!(text.lines().nth(1).unwrap().starts_with("0 "), "{text}");
        for c in ":op".chars() {
            a.handle(Input::Key(Key::ch(c)));
        }
        let s = screen(&mut a);
        assert_eq!(s.cursor, Some((3, 7)));
        assert!(s.row(7).starts_with(":op"));
        a.handle(Input::Key(Key::code(crate::app::KeyCode::Esc)));
        a.open_source("u.json", "1".into(), Format::Json);
        let s = screen(&mut a);
        let strip = s.row(0);
        assert!(
            strip.contains("1:t.json") && strip.contains("2:u.json"),
            "{strip}"
        );
        assert_eq!(s.height, 8);
    }

    #[test]
    fn error_and_source_view() {
        let mut a = App::new(Options::default(), 50, 8);
        a.open_source(
            "bad.json",
            "{\n  \"a\": 1,\n  \"b\":\n".into(),
            Format::Json,
        );
        let text = screen(&mut a).text();
        assert!(
            text.contains("!4:"),
            "status carries the error position: {text}"
        );
        a.handle(Input::Key(Key::ch('s')));
        let text = screen(&mut a).text();
        assert!(text.contains("(source)"), "{text}");
        assert!(text.lines().nth(1).unwrap().starts_with("2 "), "{text}");
        assert!(text.contains("\"a\": 1,"), "{text}");
        assert!(
            text.contains(" bad.json"),
            "the title survives a long error: {text}"
        );
    }

    #[test]
    fn control_characters_never_reach_the_screen() {
        let mut a = App::new(Options::default(), 40, 8);
        a.open_source(
            "evil\x1b]52;c;spoof\x07.json",
            "{\"k\": \"a\\u001bb\"}".into(),
            Format::Json,
        );
        let s = screen(&mut a);
        let all: String = (0..s.height).map(|y| s.row(y)).collect();
        assert!(!all.chars().any(char::is_control), "{all:?}");
        assert!(
            all.contains("evil\u{fffd}]52;c;spoof\u{fffd}.json"),
            "{all}"
        );
        assert_eq!(sanitize("a\tb\u{7f}c\u{85}".into()), "a b\u{fffd}c\u{fffd}");
    }

    #[test]
    fn long_prompts_scroll_to_the_cursor() {
        let mut a = app("1", 20, 6);
        a.handle(Input::Key(Key::ch(':')));
        for c in "open /a/very/long/path/to/some/file.json".chars() {
            a.handle(Input::Key(Key::ch(c)));
        }
        let s = screen(&mut a);
        let (col, _) = s.cursor.unwrap();
        assert!(col < 20, "cursor stays on screen: {col}");
        let text = s.row(5);
        assert!(
            text.trim_end().ends_with("file.json"),
            "the tail is shown: {text:?}"
        );
        for _ in 0..12 {
            a.handle(Input::Key(Key::code(crate::app::KeyCode::Left)));
        }
        let s = screen(&mut a);
        let text = s.row(5);
        assert!(
            text.ends_with("to/som"),
            "the window follows the cursor left: {text:?}"
        );
        assert_eq!(
            s.cursor.unwrap().0,
            19,
            "the cursor sits on its character in the last column"
        );
        a.handle(Input::Key(Key::code(crate::app::KeyCode::Home)));
        let s = screen(&mut a);
        assert_eq!(s.cursor.unwrap().0, 1);
        assert!(s.row(5).starts_with(":open"));
    }

    #[test]
    fn explorer_screen() {
        let dir =
            std::env::temp_dir().join(format!("aless-render-explorer-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.json"), "{\"a\": 1}").unwrap();
        std::fs::write(dir.join("sub/c.toml"), "c = 3\n").unwrap();
        // Wide enough for any temp path (macOS puts them under
        // /private/var/folders/…, Windows under a long AppData path).
        let mut a = App::new(Options::default(), 240, 8);
        a.open_explorer(&dir);
        let text = screen(&mut a).text();
        let first = text.lines().next().unwrap().trim_end();
        let dir_name = dir.file_name().unwrap().to_string_lossy();
        assert!(
            first.starts_with("▼ ") && first.ends_with('/') && first.contains(&*dir_name),
            "root shows its path: {text}"
        );
        assert!(!first.contains(r"\\?\"), "no verbatim prefix: {first}");
        // Narrow: a root path that does not fit keeps its tail.
        a.handle(Input::Resize(20, 8));
        let narrow = screen(&mut a);
        let full = narrow.row(0);
        assert_eq!(cols(&full), 20, "{full:?}");
        let first = full.trim_end();
        assert!(
            first.starts_with("▼ …") && first.ends_with('/'),
            "{first:?}"
        );
        a.handle(Input::Resize(240, 8));
        let text = screen(&mut a).text();
        assert!(text.contains("▷ sub/  (1) c.toml"), "{text}");
        assert!(text.contains("  a.json  8 B  json"), "{text}");
        assert!(!text.contains('"'), "no quoting in the explorer: {text}");
        assert!(text.contains("directory"), "{text}");
        a.handle(Input::Key(Key::ch('j')));
        a.handle(Input::Key(Key::ch('l')));
        let text = screen(&mut a).text();
        assert!(
            text.contains("▼ sub/\n"),
            "an expanded directory shows just its name: {text}"
        );
        assert!(text.contains("    c.toml  6 B  toml"), "{text}");
        assert!(text.contains("1 entries"), "{text}");
    }

    #[test]
    fn ansi_codes_become_styles() {
        let l = ansi_line("\x1b[91m[tabnas/unexpected]:\x1b[0m bad \x1b[2mdim\x1b[0m");
        assert_eq!(l.plain(), "[tabnas/unexpected]: bad dim");
        assert_eq!(l.spans[0].style, ERROR);
        assert_eq!(l.spans[1].style, PLAIN);
        assert!(l.spans[2].style.add_modifier.contains(Modifier::DIM));
        let l = ansi_line("a\x1b[38;5;196mb\x1b[1;34mc\x1b[Kd\x1b]52;c;eA==\x07e");
        assert_eq!(l.plain(), "abcde");
        assert_eq!(l.spans.last().unwrap().style, KEY.bold());
        assert!(!l.plain().contains('\x1b'));
    }

    #[test]
    fn a_file_that_never_parsed_shows_the_report() {
        let mut a = App::new(Options::default(), 70, 16);
        a.open_source(
            "broken.json",
            "{\n  \"a\": 1,\n  \"b\": [1, 2,,]\n}\n".into(),
            Format::Json,
        );
        let s = screen(&mut a);
        let text = s.text();
        assert!(
            text.starts_with("[tabnas/unexpected]: unexpected character(s): ,"),
            "{text}"
        );
        assert!(text.contains("  --> broken.json:3:14"), "{text}");
        assert!(text.contains("  3 |   \"b\": [1, 2,,]"), "{text}");
        assert!(text.contains("^ unexpected character(s): ,"), "{text}");
        assert_eq!(s.style(0, 0).fg, Some(Color::Red), "the engine's colours");
        // Too short for the whole report: the last row says there is more.
        a.handle(Input::Resize(70, 8));
        let text = screen(&mut a).text();
        assert!(text.contains("! shows the full report"), "{text}");
    }

    #[test]
    fn the_report_overlay_pans_long_lines() {
        let dir = std::env::temp_dir().join(format!("aless-render-pan-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("wide.json");
        // One line wider than the screen, the error near its end.
        std::fs::write(&p, format!("{{\"a\": [{}2,,3]}}", "1, ".repeat(30))).unwrap();
        let mut a = App::new(Options::default(), 40, 20);
        a.open_path(&p, None);
        a.handle(Input::Key(Key::ch('!')));
        let text = screen(&mut a).text();
        assert!(!text.contains('^'), "the caret is off to the right: {text}");
        assert!(
            text.lines().any(|l| l.ends_with('…')),
            "a cut is marked: {text}"
        );
        let mut presses = 0;
        while !screen(&mut a).text().contains('^') {
            assert!(presses < 10, "{}", screen(&mut a).text());
            a.handle(Input::Key(Key::ch('l')));
            presses += 1;
        }
        let text = screen(&mut a).text();
        assert!(presses > 0 && a.mode == Mode::Overlay);
        assert!(text.lines().skip(1).any(|l| l.starts_with('…')), "{text}");
        // As far as the widest line's end, and back.
        for _ in 0..20 {
            a.handle(Input::Key(Key::code(crate::app::KeyCode::Right)));
        }
        let o = a.overlay.as_ref().unwrap();
        let widest = o
            .lines
            .iter()
            .map(|l| cols(&crate::load::strip_ansi(l)))
            .max()
            .unwrap();
        assert_eq!(o.xoff, widest - 40);
        for _ in 0..20 {
            a.handle(Input::Key(Key::ch('h')));
        }
        assert_eq!(a.overlay.as_ref().unwrap().xoff, 0);
        assert_eq!(a.mode, Mode::Overlay);
    }

    #[test]
    fn a_broken_reload_docks_the_report_under_the_document() {
        let dir = std::env::temp_dir().join(format!("aless-render-dock-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("d.json");
        let good: String = format!(
            "[{}]",
            (0..40)
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(",\n")
        );
        std::fs::write(&p, &good).unwrap();
        let mut a = App::new(Options::default(), 60, 30);
        a.open_path(&p, None);
        a.handle(Input::Key(Key::ch('G')));
        std::fs::write(&p, good.replace("39]", "39,,]")).unwrap();
        a.handle(Input::Key(Key::ch('r')));
        let s = screen(&mut a);
        let text = s.text();
        let rule = text
            .lines()
            .position(|l| l.contains("parse error · ! shows the full report"))
            .unwrap();
        assert!(rule > 5, "the document keeps the top of the pane: {text}");
        assert!(
            text.lines()
                .nth(rule + 1)
                .unwrap()
                .starts_with("[tabnas/unexpected]"),
            "{text}"
        );
        // The focused row, the last one, stays above the panel.
        let focused = text
            .lines()
            .position(|l| l.trim_end().ends_with("39"))
            .unwrap();
        assert!(focused < rule, "focus hidden behind the panel: {text}");
        assert!(text.contains("d.json:"), "the file is named: {text}");
    }

    #[test]
    fn help_overlay_renders() {
        let mut a = app("1", 60, 10);
        a.handle(Input::Key(Key::code(crate::app::KeyCode::F(1))));
        let text = screen(&mut a).text();
        assert!(text.contains("aless help"));
        assert!(text.contains("MOVING"));
    }

    /// Wide, joined and combining characters take the columns the terminal
    /// gives them, as ratatui's buffer places them: a value that fits is
    /// shown whole, one that does not ends in its ellipsis without
    /// splitting a character, and a bar's highlight reaches the right edge.
    #[test]
    fn wide_and_joined_characters_take_the_columns_the_terminal_gives_them() {
        let family = "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}";
        // CJK two cells each; an emoji two; a family joined by zero-width
        // joiners two, not six; a flag two; a letter and its combining
        // accent one; a halfwidth kana and its sound mark two, not one.
        for (text, cells) in [
            ("日本語", 6),
            ("\u{1f600}", 2),
            (family, 2),
            ("\u{1f1ef}\u{1f1f5}", 2),
            ("e\u{301}", 1),
            ("ﾊﾟ", 2),
        ] {
            assert_eq!(cols(text), cells, "{text:?}");
        }
        let doc = format!(
            r#"{{"日本語": "{}", "k": "{}", "e": "{}"}}"#,
            family.repeat(4),
            "ﾊﾟ".repeat(12),
            "e\u{301}".repeat(30)
        );
        let mut a = app(&doc, 24, 8);
        a.handle(Input::Key(Key::ch('j')));
        let s = screen(&mut a);
        for y in 0..s.height {
            assert_eq!(cols(&s.row(y)), 24, "row {y}: {:?}", s.row(y));
        }
        // Four families take eight cells, so the value fits beside its key.
        let row = s.row(1);
        assert!(
            row.trim_end()
                .ends_with(&format!("\"{}\"", family.repeat(4))),
            "{row:?}"
        );
        // Twelve kana with their marks take 24 cells: the ellipsis follows
        // the last whole one, a column short of the edge, since the next
        // needs two.
        let row = s.row(2);
        assert!(row.contains(&format!("\"{}…", "ﾊﾟ".repeat(7))), "{row:?}");
        assert_eq!(cols(row.trim_end()), 23, "{row:?}");
        // Each accented letter takes one cell, so the ellipsis is in the
        // last column, the accents still on their letters.
        let row = s.row(3);
        assert!(
            row.ends_with(&format!("\"{}…", "e\u{301}".repeat(15))),
            "{row:?}"
        );
        // The status bar names the focused key, its highlight to the edge.
        assert!(s.row(6).contains(".日本語"), "{:?}", s.row(6));
        for x in [0, 23] {
            let style = s.style(x, 6);
            assert!(
                style.add_modifier.contains(Modifier::REVERSED),
                "{x}: {style:?}"
            );
        }
    }
}
