//! Drawing: the whole screen as ratatui widgets, computed from the app state.
//! [`draw`] renders into any backend's `Frame`, and [`screen`] into an
//! in-memory one, so tests read what a user would see. Every width is measured
//! as ratatui's buffer places text, by grapheme cluster at the cells
//! [`CellWidth`] gives it, so a fitted line fills exactly its width.

use std::borrow::Cow;
use std::ops::ControlFlow;

use ratatui::backend::TestBackend;
use ratatui::buffer::{Buffer, CellWidth};
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;
use ratatui::{Frame, Terminal};
use tabnas_lsp::TokenType;

use crate::app::{panel_rows, App, Mode, PromptKind};
use crate::doc::{Doc, Kind, Row, NO_NODE};
use crate::fmt;
use crate::highlight::Run;
use crate::history::{self, ChangeKind, Counts, Data, Entry, Item, Listing, Marks};
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
/// The marks of a version's changes. Changed data is in reverse, as a
/// screen without colour keeps reverse and drops the rest.
const CHANGE: Style = Style::new().fg(Color::Yellow);
const ADD: Style = Style::new().fg(Color::Green);
const REMOVE: Style = Style::new().fg(Color::Red);
const CHANGED_DATA: Style = Style::new()
    .fg(Color::Yellow)
    .bg(Color::Black)
    .add_modifier(Modifier::REVERSED);
const ADDED_DATA: Style = Style::new()
    .fg(Color::Green)
    .bg(Color::Black)
    .add_modifier(Modifier::REVERSED);
/// The status bar while an older version is on screen.
const BAR_PAST: Style = CHANGED_DATA;

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
            if left == 0 {
                keep.push(span);
                continue;
            }
            // Exact while the span ends before the cut.
            let w = cols_upto(&clean(&span.content), left.saturating_add(1));
            if w <= left {
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
            let room = width - used;
            // Exact while the span fits the room left.
            let w = cols_upto(&clean(&span.content), room.saturating_add(1));
            if w <= room {
                used += w;
                keep.push(span);
            } else {
                let (cut, cw) = take_cols(&span.content, room);
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

/// Walk the grapheme clusters of clean `text` in order, each with its length
/// in bytes and the cells ratatui's buffer gives it (none for a zero-width
/// cluster), until `each` breaks. Nothing is held per cluster, so a huge value
/// is clipped holding no more than what the clip returns.
fn walk(text: &str, mut each: impl FnMut(usize, usize) -> ControlFlow<()>) {
    let span = Span::raw(text);
    let mut at = 0;
    for g in span.styled_graphemes(PLAIN) {
        // Clean text has no control character for the iterator to drop,
        // so the clusters tile the text in order.
        debug_assert_eq!(&text[at..at + g.symbol.len()], g.symbol);
        at += g.symbol.len();
        if each(g.symbol.len(), usize::from(g.symbol.cell_width())).is_break() {
            return;
        }
    }
}

/// The columns `text` takes on screen.
pub fn cols(text: &str) -> usize {
    cols_upto(&clean(text), usize::MAX)
}

/// The columns clean `text` takes, measured no further than `limit`:
/// exact below it, and at least `limit` when the text reaches it.
fn cols_upto(text: &str, limit: usize) -> usize {
    let mut used = 0;
    walk(text, |_, w| {
        if used >= limit {
            return ControlFlow::Break(());
        }
        used += w;
        ControlFlow::Continue(())
    });
    used
}

/// How much of clean `text` fits in `width` columns: its length in
/// bytes, and its width.
fn fit(text: &str, width: usize) -> (usize, usize) {
    let (mut len, mut used) = (0, 0);
    walk(text, |bytes, w| {
        if used + w > width {
            return ControlFlow::Break(());
        }
        used += w;
        len += bytes;
        ControlFlow::Continue(())
    });
    (len, used)
}

/// Where clean `text` resumes once its first `cols` columns are skipped,
/// in bytes; a cluster the cut would split goes whole.
fn skip(text: &str, cols: usize) -> usize {
    let (mut at, mut used) = (0, 0);
    walk(text, |bytes, w| {
        if used >= cols {
            return ControlFlow::Break(());
        }
        used += w;
        at += bytes;
        ControlFlow::Continue(())
    });
    at
}

/// The leading part of `text` that fits in `width` columns, and its width.
fn take_cols(text: &str, width: usize) -> (String, usize) {
    let text = clean(text);
    let (len, used) = fit(&text, width);
    (text[..len].to_string(), used)
}

/// `text` with its first `cols` columns skipped; a cluster the cut would
/// split goes whole.
fn skip_cols(text: &str, cols: usize) -> String {
    let text = clean(text);
    text[skip(&text, cols)..].to_string()
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

/// Fit `text` into `avail` columns, scrolled right by `xoff`, an ellipsis
/// marking a cut at either end; true when cut on the right. Each measure walks
/// only as far as the answer needs: a screenful past the offset, or the tail.
pub fn clip(text: &str, xoff: usize, avail: usize) -> (String, bool) {
    let text = clean(text);
    if avail == 0 {
        return (String::new(), cols_upto(&text, 1) > 0);
    }
    if xoff == 0 && cols_upto(&text, avail.saturating_add(1)) <= avail {
        return (text.into_owned(), false);
    }
    if avail == 1 {
        return ("…".to_string(), true);
    }
    // The furthest useful offset shows the tail with a leading ellipsis:
    // the width less `avail - 1`. Short of the end, the text reaches
    // `xoff + avail - 1` columns only when `xoff` is within it.
    let reach = xoff.saturating_add(avail - 1);
    let seen = cols_upto(&text, reach);
    let xoff = if seen >= reach {
        xoff
    } else {
        seen.saturating_sub(avail - 1)
    };
    if xoff == 0 {
        let (len, _) = fit(&text, avail - 1);
        return (format!("{}…", &text[..len]), true);
    }
    let rest = &text[skip(&text, xoff)..];
    if cols_upto(rest, avail) < avail {
        (format!("…{rest}"), false)
    } else {
        let (len, _) = fit(rest, avail - 2);
        (format!("…{}…", &rest[..len]), true)
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
/// part: the tab strip, the main area (the tree and its error panel, the
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
    let timeline = band(area, &mut top, app.timeline_rows());
    let status = band(area, &mut top, 1);
    let prompt = band(area, &mut top, 1);

    if strip.height > 0 {
        frame.render_widget(Rows(vec![tab_strip(app, width)]), strip);
    }
    if app.mode == Mode::Overlay {
        frame.render_widget(Rows(overlay_lines(app, width, pane_h)), pane);
    } else {
        draw_pane(frame, app, pane);
    }
    if timeline.height > 0 {
        frame.render_widget(Rows(vec![timeline_line(app, width)]), timeline);
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

/// The main area: the active tab's text in the source view, else its tree.
fn draw_pane(frame: &mut Frame, app: &mut App, area: Rect) {
    if app.tabs.is_empty() {
        return;
    }
    if app.source_view {
        let (width, height) = (usize::from(area.width), usize::from(area.height));
        let lines = source_lines(app, width, height);
        frame.render_widget(Rows(lines), area);
    } else {
        draw_tree_pane(frame, app, area);
    }
}

/// The tree: the document's rows and, while a reload fails, the error panel
/// under them; a tab that never loaded shows its report in their place.
fn draw_tree_pane(frame: &mut Frame, app: &mut App, area: Rect) {
    let (width, pane_h) = (usize::from(area.width), usize::from(area.height));
    let numbers = app.opts.numbers;
    let relative = app.opts.relative;
    let indent = app.opts.indent;
    let ascii = app.opts.ascii;
    let changes = app.opts.changes;
    let note = app.note_rows();
    let note_text = (note > 0).then(|| note_lines(app, width, note));
    let tab = app.tab();
    let panel = panel_rows(tab, pane_h);
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
        changes,
    };
    let report = (panel > 0).then(|| {
        tab.error
            .as_ref()
            .map(|e| e.report.clone())
            .unwrap_or_default()
    });
    let tree_h = pane_h.saturating_sub(panel + note);
    let mut top = area.top();
    let tree_area = band(area, &mut top, tree_h);
    let note_area = band(area, &mut top, note);
    let panel_area = band(area, &mut top, panel);
    let lines = tree_lines(tab, &rows, &tree, tree_h, scroll, focus);
    frame.render_widget(Rows(lines), tree_area);
    if let Some(text) = note_text {
        frame.render_widget(Rows(text), note_area);
    }
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
    changes: bool,
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
        changes,
    } = *style;
    let marked = changes && !tab.marks.is_empty();
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
            let mark = tab.marks.get(row.node).copied().filter(|_| marked);
            let data = mark.and_then(|m| m.data).filter(|_| !row.close);
            if marked {
                for (text, rail) in indentation(&tab.doc, &tab.marks, *row, indent, ascii) {
                    line.put(text, if rail { CHANGE } else { PLAIN });
                }
            } else {
                line.put(" ".repeat(node.depth as usize * indent), PLAIN);
            }
            // Indicator: a leaf's slot holds the mark of its change.
            let item = tab
                .listing
                .as_ref()
                .and_then(|l| l.items.get(row.node as usize));
            let (indicator, mut ind_style) = if row.close {
                ("  ", PLAIN)
            } else if let Some(Item::Change { change, .. }) = item {
                (
                    match change.kind {
                        ChangeKind::Changed => "~ ",
                        ChangeKind::Added => "+ ",
                        ChangeKind::Removed if ascii => "- ",
                        ChangeKind::Removed => "− ",
                    },
                    kind_style(change.kind),
                )
            } else if let Some(Item::Failed { .. }) = item {
                ("! ", ERROR)
            } else if !node.is_foldable() {
                match data {
                    Some(Data::Changed) => ("~ ", CHANGE),
                    Some(Data::Added) => ("+ ", ADD),
                    None => ("  ", PLAIN),
                }
            } else {
                let glyph = if ascii {
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
                let style = match (data, mark) {
                    (Some(Data::Added), _) => ADD,
                    (_, Some(_)) => CHANGE,
                    _ => PLAIN,
                };
                (glyph, style)
            };
            if focused {
                ind_style = ind_style.bold();
            }
            line.put(indicator, ind_style);
            if let Some(list) = &tab.listing {
                listing_row(&mut line, list, row.node, focused, ascii);
                line.fit_cols(width, PLAIN);
                out.push(line);
                continue;
            }
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
            // Value, then the counts of what changed inside or was removed.
            let badge = match mark {
                Some(m) if !row.close && data.is_none() => {
                    if node.is_foldable() && !node.expanded {
                        counts_badge(m.inside, ascii)
                    } else if m.removed > 0 {
                        vec![(
                            format!(" {}{}", ChangeKind::Removed.glyph(ascii), m.removed),
                            REMOVE,
                        )]
                    } else {
                        Vec::new()
                    }
                }
                _ => Vec::new(),
            };
            let badge_w: usize = badge.iter().map(|(t, _)| cols(t)).sum();
            let avail = width.saturating_sub(line.cols() + badge_w);
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
                    _ => fmt::value_text(&tab.doc, row.node),
                };
                (text, style, line_mode && has_next)
            };
            let style = if focused && !key_shown {
                FOCUS
            } else if is_match && !key_shown {
                MATCH
            } else {
                match data {
                    Some(Data::Changed) => CHANGED_DATA,
                    Some(Data::Added) => ADDED_DATA,
                    None => style,
                }
            };
            let xoff = if focused { tab.xoff } else { 0 };
            let comma_w = usize::from(comma);
            let (text, cut) = clip(&value, xoff, avail.saturating_sub(comma_w));
            line.put(text, style);
            if comma && !cut {
                line.put(",", PLAIN);
            }
            for (text, style) in badge {
                line.put(text, style);
            }
        }
        line.fit_cols(width, PLAIN);
        out.push(line);
    }
    out
}

/// The indentation of `row`, with the rails that pass through it: each
/// rail runs down from its node's marker to the node's last marked child,
/// where it turns into that child's marker.
fn indentation(
    doc: &Doc,
    marks: &Marks,
    row: Row,
    indent: usize,
    ascii: bool,
) -> Vec<(String, bool)> {
    let width = doc.node(row.node).depth as usize * indent;
    let mut cells: Vec<Option<char>> = vec![None; width];
    if indent > 0 {
        let (down, tee, last, across) = if ascii {
            ('|', '|', '`', '-')
        } else {
            ('│', '├', '└', '─')
        };
        let mut child = row.node;
        let mut parent = doc.parent(row.node);
        while let Some(a) = parent {
            if let Some(m) = marks.get(a).filter(|m| m.last != NO_NODE) {
                let col = doc.node(a).depth as usize * indent;
                let own_row = child == row.node && !row.close;
                if own_row && marks.carries(child) && child <= m.last {
                    cells[col] = Some(if child == m.last { last } else { tee });
                    for c in &mut cells[col + 1..(col + indent).min(width)] {
                        *c = Some(across);
                    }
                } else if child < m.last {
                    cells[col] = Some(down);
                }
            }
            child = a;
            parent = doc.parent(a);
        }
    }
    let mut out: Vec<(String, bool)> = Vec::new();
    for cell in cells {
        let (ch, rail) = match cell {
            Some(ch) => (ch, true),
            None => (' ', false),
        };
        match out.last_mut() {
            Some((text, r)) if *r == rail => text.push(ch),
            _ => out.push((ch.to_string(), rail)),
        }
    }
    out
}

/// A row of a listing tab after its marker: the file and its count of
/// versions, a version's number, time and changes, a change's path and
/// values, or a failed save's time and message.
fn listing_row(
    line: &mut Line<'static>,
    list: &Listing,
    id: crate::doc::NodeId,
    focused: bool,
    ascii: bool,
) {
    let label = |plain: Style| if focused { FOCUS } else { plain };
    match list.items.get(id as usize) {
        Some(Item::Root { versions, failed }) => {
            line.put(list.name.clone(), label(KEY.bold()));
            let s = if *versions == 1 { "" } else { "s" };
            let mut text = format!("  {versions} version{s}");
            if *failed > 0 {
                let s = if *failed == 1 { "a save" } else { "saves" };
                text.push_str(&format!(", and {s} that did not parse"));
            }
            line.put(text, GREY);
        }
        Some(Item::Version {
            number,
            time,
            counts,
            newest,
            shown,
            ..
        }) => {
            line.put(format!("version {number}"), label(KEY));
            line.put(format!("  {time}"), GREY);
            for (text, style) in counts_badge(*counts, ascii) {
                line.put(text, style);
            }
            let notes: Vec<&str> = [(*newest, "the newest"), (*shown, "on screen")]
                .iter()
                .filter(|(on, _)| *on)
                .map(|(_, w)| *w)
                .collect();
            if !notes.is_empty() {
                line.put(format!("  {}", notes.join(", ")), GREY);
            }
        }
        Some(Item::Change { change, .. }) => {
            let path = fmt::path_dot(&change.path);
            let path = if path.is_empty() {
                ".".to_string()
            } else {
                path
            };
            line.put(path, label(PLAIN));
            line.put("  ", PLAIN);
            match change.kind {
                ChangeKind::Changed => {
                    line.put(change.old.clone().unwrap_or_default(), GREY);
                    line.put(if ascii { " -> " } else { " → " }, GREY);
                    let new = change.new.clone().unwrap_or_default();
                    let style = value_style(&new);
                    line.put(new, style);
                }
                ChangeKind::Added => {
                    let new = change.new.clone().unwrap_or_default();
                    let style = value_style(&new);
                    line.put(new, style);
                }
                ChangeKind::Removed => {
                    let old = change.old.clone().unwrap_or_default();
                    line.put(old, REMOVE.add_modifier(Modifier::CROSSED_OUT));
                }
            }
        }
        Some(Item::Failed { time, message }) => {
            line.put("did not parse", label(ERROR));
            line.put(format!("  {time}  {message}"), GREY);
        }
        None => {}
    }
}

/// `  ~1 +2 −1`: the changes inside a folded container, kind by kind.
fn counts_badge(c: Counts, ascii: bool) -> Vec<(String, Style)> {
    let mut out: Vec<(String, Style)> = Vec::new();
    for (n, kind, style) in [
        (c.changed, ChangeKind::Changed, CHANGE),
        (c.added, ChangeKind::Added, ADD),
        (c.removed, ChangeKind::Removed, REMOVE),
    ] {
        if n > 0 {
            out.push((if out.is_empty() { "  " } else { " " }.to_string(), PLAIN));
            out.push((format!("{}{n}", kind.glyph(ascii)), style));
        }
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

/// `(3) src/, Cargo.toml, README.md`: the names inside a collapsed
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
    let changes = app.opts.changes;
    let painted = app.painted();
    let tab = app.tab();
    // A tab with no text to show, one that never read (a program or a
    // file that could not be opened), shows why instead.
    if tab.shows_error_only() && tab.source.is_empty() {
        let report = tab
            .error
            .as_ref()
            .map(|e| e.report.clone())
            .unwrap_or_default();
        return report_lines(&report, width, pane_h, "! shows the full report");
    }
    let focus_line = tab.focused_line().map(|(l, _)| l as usize);
    let error_line = tab
        .error
        .as_ref()
        .filter(|e| e.line > 0)
        .map(|e| e.line as usize);
    let lines = load::lines(&tab.source);
    let gutter = digits(lines.len()) + 1;
    let scroll = tab.source_scroll;
    let (kinds, spans) = if changes {
        source_marks(tab, &lines)
    } else {
        Default::default()
    };
    let mark_col = if kinds.is_empty() { 0 } else { 2 };
    for i in 0..pane_h {
        let mut line = Line::default();
        let n = scroll + i;
        if let Some(text) = lines.get(n) {
            if mark_col > 0 {
                match kinds.get(&n) {
                    Some(&k) => line.put(format!("{} ", k.glyph(ascii)), kind_style(k)),
                    None => line.put("  ", PLAIN),
                };
            }
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
            let avail = width.saturating_sub(line.cols());
            let body_at = line.cols();
            // The focused and the failing line keep their one style.
            let marks = spans.get(&n).filter(|_| !is_focus && !is_error);
            let runs = painted.as_ref().filter(|_| !is_focus && !is_error);
            if let Some(runs) = runs.map(|p| p.line(n)) {
                let mut body = coloured(text, runs, avail);
                line.spans.append(&mut body.spans);
                for &(from, to, style) in marks.into_iter().flatten() {
                    line = restyle(line, body_at + from, body_at + to, style);
                }
                line.fit_cols(width, PLAIN);
                out.push(line);
                continue;
            }
            let shown: String = text
                .chars()
                .map(|c| match c {
                    '\t' => "    ".to_string(),
                    c if (c as u32) < 0x20 => "·".to_string(),
                    c => c.to_string(),
                })
                .collect();
            let (t, _) = clip(&shown, 0, avail);
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
            for &(from, to, style) in marks.into_iter().flatten() {
                line = restyle(line, body_at + from, body_at + to, style);
            }
        }
        line.fit_cols(width, PLAIN);
        out.push(line);
    }
    out
}

fn kind_style(kind: ChangeKind) -> Style {
    match kind {
        ChangeKind::Changed => CHANGE,
        ChangeKind::Added => ADD,
        ChangeKind::Removed => REMOVE,
    }
}

/// Columns of a line of text marked in a style: (first, past the last, style).
type Spans = std::collections::HashMap<usize, Vec<(usize, usize, Style)>>;

/// The marks of the version on screen in its text: the kind of change on
/// each line, a changed value outranking an added one and both a removal,
/// and the columns of each changed value where the line spells it.
fn source_marks(
    tab: &crate::tab::Tab,
    lines: &[&str],
) -> (std::collections::HashMap<usize, ChangeKind>, Spans) {
    let mut kinds = std::collections::HashMap::new();
    let mut spans = Spans::new();
    for (id, m) in tab.marks.iter() {
        let node = tab.doc.node(id);
        let Some(n) = (node.line as usize).checked_sub(1) else {
            continue;
        };
        if let Some(d) = m.data {
            let (kind, style) = match d {
                Data::Changed => (ChangeKind::Changed, CHANGED_DATA),
                Data::Added => (ChangeKind::Added, ADDED_DATA),
            };
            let e = kinds.entry(n).or_insert(kind);
            if kind == ChangeKind::Changed || *e == ChangeKind::Removed {
                *e = kind;
            }
            let text = lines.get(n).copied().unwrap_or("");
            if !node.is_container() {
                if let Some((from, to)) = value_cols(text, node, &fmt::value_text(&tab.doc, id)) {
                    spans.entry(n).or_default().push((from, to, style));
                }
            }
        }
        if m.removed > 0 {
            kinds.entry(n).or_insert(ChangeKind::Removed);
        }
    }
    (kinds, spans)
}

/// The columns `value` takes where `text` spells it, at the node's column
/// and after its key; none where the text spells it otherwise.
fn value_cols(text: &str, node: &crate::doc::Node, value: &str) -> Option<(usize, usize)> {
    let col = (node.col as usize).checked_sub(1)?;
    let mut at = text.char_indices().nth(col).map(|(b, _)| b)?;
    if let crate::doc::Key::Name(k) = &node.key {
        for spelled in [fmt::quote(k), k.to_string()] {
            if text[at..].starts_with(&spelled) {
                at += spelled.len();
                break;
            }
        }
    }
    let start = at + text[at..].find(value)?;
    let from = shown_cols(&text[..start]);
    Some((from, from + shown_cols(value)))
}

/// The columns the source view gives `text`, a tab as four.
fn shown_cols(text: &str) -> usize {
    text.chars()
        .map(|c| match c {
            '\t' => 4,
            c if (c as u32) < 0x20 || c.is_control() => 1,
            c => cols(c.encode_utf8(&mut [0; 4])),
        })
        .sum()
}

/// `line` with its columns `from..to` drawn in `style`.
fn restyle(line: Line<'static>, from: usize, to: usize, style: Style) -> Line<'static> {
    let mut out = Line::default();
    let mut col = 0;
    for span in line.spans {
        let mut run = String::new();
        let mut run_style = None;
        for ch in span.content.chars() {
            let here = if (from..to).contains(&col) {
                style
            } else {
                span.style
            };
            if run_style != Some(here) && !run.is_empty() {
                out.put(std::mem::take(&mut run), run_style.unwrap_or(span.style));
            }
            run_style = Some(here);
            run.push(ch);
            col += cols(ch.encode_utf8(&mut [0; 4]));
        }
        if !run.is_empty() {
            out.put(run, run_style.unwrap_or(span.style));
        }
    }
    out
}

/// A line of the source view's text with its runs coloured, shown and cut to
/// `avail` columns as the plain view does. Each grapheme cluster takes its
/// first character's style, so a style never splits what the terminal draws
/// as one, and the line is read only until what shows is settled.
pub fn coloured(text: &str, runs: &[Run], avail: usize) -> Line<'static> {
    let mut line = Line::default();
    if avail == 0 {
        return line;
    }
    // Read a chunk at a time until the clusters that show, and whether the
    // line is cut, cannot depend on what follows. Each chunk doubles the last,
    // so a line that never fills the window, all zero-width marks, is
    // measured a few times over rather than once a chunk.
    let mut shown = String::new();
    let mut marks: Vec<(usize, Style)> = Vec::new();
    let mut chars = text.char_indices().peekable();
    let mut next = runs.iter().peekable();
    let mut chunk = avail.saturating_mul(2).saturating_add(64);
    loop {
        for (i, c) in chars.by_ref().take(chunk) {
            while next.peek().is_some_and(|r| r.end <= i) {
                next.next();
            }
            let here = match next.peek() {
                Some(r) if r.start <= i => token_style(r.kind),
                _ => PLAIN,
            };
            if marks.last().is_none_or(|&(_, style)| style != here) {
                marks.push((shown.len(), here));
            }
            match c {
                '\t' => shown.push_str("    "),
                c if (c as u32) < 0x20 => shown.push('·'),
                c if c.is_control() => shown.push('\u{fffd}'),
                c => shown.push(c),
            }
        }
        if chars.peek().is_none() || overflows(&shown, avail) {
            break;
        }
        chunk = chunk.saturating_mul(2);
    }
    // Text left unread means the window overflowed.
    let cut = chars.peek().is_some() || cols_upto(&shown, avail.saturating_add(1)) > avail;
    if cut && avail == 1 {
        line.put("…", PLAIN);
        return line;
    }
    let width = if cut { avail - 1 } else { avail };
    let (mut used, mut at, mut mark) = (0, 0, 0);
    let mut style = PLAIN;
    let mut run = String::new();
    walk(&shown, |bytes, w| {
        if used + w > width {
            return ControlFlow::Break(());
        }
        while marks.get(mark + 1).is_some_and(|&(from, _)| from <= at) {
            mark += 1;
        }
        let here = marks.get(mark).map_or(PLAIN, |&(_, style)| style);
        if here != style && !run.is_empty() {
            line.put(std::mem::take(&mut run), style);
        }
        style = here;
        run.push_str(&shown[at..at + bytes]);
        used += w;
        at += bytes;
        ControlFlow::Continue(())
    });
    line.put(run, style);
    if cut {
        line.put("…", PLAIN);
    }
    line
}

/// Whether clean `text` takes more than `avail` columns before its last
/// cluster. Then what fits in `avail` columns is made of whole clusters,
/// and the text is cut there, whatever follows the last one.
fn overflows(text: &str, avail: usize) -> bool {
    let mut used = 0;
    let mut over = false;
    walk(text, |_, w| {
        // A cluster after those past the window: they are whole.
        if used > avail {
            over = true;
            return ControlFlow::Break(());
        }
        used += w;
        ControlFlow::Continue(())
    });
    over
}

/// How a token is coloured in the source view, as the tree colours values:
/// strings green, numbers magenta, keywords (`true`, `null`, a language's
/// own) yellow, keys and names blue, comments grey, punctuation plain.
fn token_style(kind: TokenType) -> Style {
    match kind {
        TokenType::String => STRING,
        TokenType::Number => NUMBER,
        TokenType::Keyword => BOOL,
        TokenType::Comment => GREY,
        TokenType::Property | TokenType::Variable => KEY,
        TokenType::Type | TokenType::Macro => Style::new().fg(Color::Cyan),
        TokenType::Operator => PLAIN,
    }
}

fn status_bar(app: &mut App, width: usize) -> Line<'static> {
    let mut line = Line::default();
    if app.tabs.is_empty() {
        line.put(" aless ", BAR);
        line.fit_cols(width, BAR);
        return line;
    }
    let source = app.source_view;
    let tab = app.tab();
    let node = tab.focused_node();
    let node_path = tab.doc.path(node);
    let mut left = format!(" {}", tab.title);
    let mut right = Vec::new();
    if let Some(list) = &tab.listing {
        let what = match list.items.get(node as usize) {
            Some(Item::Version { number, .. }) => format!(" version {number}"),
            Some(Item::Change { change, .. }) => format!(" {}", fmt::path_dot(&change.path)),
            Some(Item::Failed { time, .. }) => format!(" the save at {time}"),
            _ => String::new(),
        };
        left = format!(" {}{what}", tab.title);
        right.push("history".to_string());
        let n = list
            .items
            .iter()
            .filter(|i| matches!(i, Item::Version { .. }))
            .count();
        right.push(format!("{n} version{}", if n == 1 { "" } else { "s" }));
    } else if let Some(ex) = &tab.explorer {
        left = format!(" {}", ex.fs_path(&node_path).display());
        right.push("directory".to_string());
        let n = tab.doc.node(node);
        if n.is_container() {
            right.push(format!("{} entries", n.children));
        }
    } else {
        let path = fmt::path_dot(&node_path);
        if source {
            left.push_str("  (source)");
        } else {
            left.push(' ');
            left.push_str(if path.is_empty() { "." } else { &path });
        }
        right.push(tab.format.name().to_string());
        if let Some(f) = tab.frozen {
            right.push(format!("version {}", f.number));
        }
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
    let past = !tab.history.at_newest();
    if past {
        if let (Some(v), Some(n)) = (tab.history.shown(), tab.history.newest()) {
            right.push(format!("version {} of {}", v.number, n.number));
        }
    }
    let bar = if past { BAR_PAST } else { BAR };
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
    line.put(&left, bar);
    let pad = width.saturating_sub(cols(&left) + cols(&right) + err_w);
    line.put(" ".repeat(pad), bar);
    if let Some(e) = err_text {
        line.put(e, bar.bold());
    }
    line.put(right, bar);
    line.fit_cols(width, bar);
    line
}

/// The timeline: a dot for each version, spaced by the time between them,
/// a `!` for each save that did not parse, the version on screen lit; the
/// first and last times at the ends.
fn timeline_line(app: &App, width: usize) -> Line<'static> {
    let mut line = Line::default();
    let Some(tab) = app.tab_ref() else {
        return line;
    };
    let h = &tab.history;
    let entries = h.entries();
    let (Some(&(first, _)), Some(&(last, _))) = (entries.first(), entries.last()) else {
        return line;
    };
    let offset = app.opts.utc_offset;
    let ascii = app.opts.ascii;
    let shown = h.shown().map_or(0, |v| v.number);
    let newest = h.newest().map_or(0, |v| v.number);
    let left = format!(" {} ", history::clock(first, offset));
    let right = format!(" {}  ", history::clock(last, offset));
    let label = format!("{shown} of {newest}");
    let len = width.saturating_sub(cols(&left) + cols(&right) + cols(&label) + 1);
    if len < 4 {
        line.put(format!(" {label}"), PLAIN);
        line.fit_cols(width, PLAIN);
        return line;
    }
    let span = last.duration_since(first).unwrap_or_default().as_secs_f64();
    let at = |t: std::time::SystemTime| -> usize {
        if span <= 0.0 {
            return len - 1;
        }
        let x = t.duration_since(first).unwrap_or_default().as_secs_f64() / span;
        ((x * (len - 1) as f64).round() as usize).min(len - 1)
    };
    let dash = if ascii { "-" } else { "─" };
    let mut track: Vec<(&str, Style)> = vec![(dash, GREY); len];
    // Drawn in rising order of weight: a version, a failed save, the
    // version on screen, each over what shares its cell.
    for &(t, e) in &entries {
        if let Entry::Version(i) = e {
            if i != h.shown_index() {
                track[at(t)] = (if ascii { "o" } else { "○" }, PLAIN);
            }
        }
    }
    for &(t, e) in &entries {
        if let Entry::Failed(_) = e {
            track[at(t)] = ("!", ERROR);
        }
    }
    if let Some(v) = h.shown() {
        track[at(v.time)] = (if ascii { "@" } else { "●" }, BAR_PAST.bold());
    }
    line.put(left, GREY);
    for (text, style) in track {
        line.put(text, style);
    }
    line.put(right, GREY);
    line.put(label, PLAIN);
    line.fit_cols(width, PLAIN);
    line
}

/// The note docked under the tree: a rule naming the version on screen and
/// how to move, then a row for each of its changes, as many as fit.
fn note_lines(app: &App, width: usize, height: usize) -> Vec<Line<'static>> {
    let mut out = Vec::with_capacity(height);
    let Some(tab) = app.tab_ref() else {
        return out;
    };
    let Some(v) = tab.history.shown() else {
        return out;
    };
    let ascii = app.opts.ascii;
    let count = v.changes.len();
    let head = if count == 0 {
        crate::app::version_line(tab, app.opts.utc_offset)
    } else {
        let s = if count == 1 { "" } else { "s" };
        format!("{count} change{s} in version {}", v.number)
    };
    let hint = if tab.frozen.is_some() {
        "d next change · v history"
    } else if tab.history.at_newest() {
        "[ older · d next change"
    } else {
        "] newer · } newest · d next change"
    };
    let label = format!(" {head} · {hint} ");
    let dash = if ascii { "-" } else { "─" };
    let mut rule = Line::default();
    rule.put(dash.repeat(2), CHANGE);
    rule.put(label.clone(), CHANGE.bold());
    rule.put(dash.repeat(width.saturating_sub(cols(&label) + 2)), CHANGE);
    rule.fit_cols(width, PLAIN);
    out.push(rule);
    let paths: Vec<String> = v
        .changes
        .iter()
        .map(|c| {
            let p = fmt::path_dot(&c.path);
            if p.is_empty() {
                ".".to_string()
            } else {
                p
            }
        })
        .collect();
    let pw = paths
        .iter()
        .map(|p| cols(p))
        .max()
        .unwrap_or(0)
        .min(width / 3);
    let room = height.saturating_sub(1);
    let more = count.saturating_sub(room);
    let listed = if more > 0 {
        room.saturating_sub(1)
    } else {
        room
    };
    for (c, path) in v.changes.iter().zip(&paths).take(listed) {
        let mut l = Line::default();
        let style = match c.kind {
            ChangeKind::Changed => CHANGE,
            ChangeKind::Added => ADD,
            ChangeKind::Removed => REMOVE,
        };
        l.put("  ", PLAIN);
        l.put(c.kind.glyph(ascii), style);
        l.put(" ", PLAIN);
        let (p, _) = clip(path, 0, pw);
        l.put(format!("{p:<pw$}  "), PLAIN);
        match c.kind {
            ChangeKind::Changed => {
                l.put(c.old.clone().unwrap_or_default(), GREY);
                l.put(if ascii { " -> " } else { " → " }, GREY);
                let new = c.new.clone().unwrap_or_default();
                let st = value_style(&new);
                l.put(new, st);
            }
            ChangeKind::Added => {
                let new = c.new.clone().unwrap_or_default();
                let st = value_style(&new);
                l.put(new, st);
            }
            ChangeKind::Removed => {
                l.put(
                    c.old.clone().unwrap_or_default(),
                    REMOVE.add_modifier(Modifier::CROSSED_OUT),
                );
            }
        }
        out.push(fit_line(l, width));
    }
    if more > 0 {
        let mut l = Line::default();
        let n = more + usize::from(room > 0);
        l.put(format!("  … and {n} more: d and D go to each"), GREY);
        out.push(fit_line(l, width));
    }
    out.truncate(height);
    out
}

/// The colour a value's text takes in the tree, read from its spelling.
fn value_style(text: &str) -> Style {
    match text.chars().next() {
        Some('"') => STRING,
        Some('t' | 'f') if text == "true" || text == "false" => BOOL,
        Some('n') if text == "null" => NULL,
        Some(c) if c == '-' || c.is_ascii_digit() => NUMBER,
        _ => PLAIN,
    }
}

/// A line cut to `width`, with `…` where it is cut.
fn fit_line(mut line: Line<'static>, width: usize) -> Line<'static> {
    if line.cols() > width {
        line.fit_cols(width.saturating_sub(1), PLAIN);
        line.put("…", GREY);
    }
    line.fit_cols(width, PLAIN);
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

    const BEFORE: &str = "[\n  {\"title\": \"A\", \"year\": 2001, \"tags\": [\"x\", \"y\"]},\n  {\"title\": \"B\", \"tags\": []}\n]\n";
    const AFTER: &str = "[\n  {\"title\": \"A\", \"year\": 2001, \"tags\": [\"x\"]},\n  {\"title\": \"B!\", \"tags\": [\"z\"]}\n]\n";

    /// A file opened at 14:00:05 and saved at 14:02:11, in UTC as the zone.
    fn saved(name: &str, opts: Options, w: u16, h: u16) -> App {
        let dir = std::env::temp_dir().join(format!("aless-render-tests-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(name);
        std::fs::write(&p, BEFORE).unwrap();
        let at =
            |secs: u64| std::time::UNIX_EPOCH + std::time::Duration::from_secs(14 * 3600 + secs);
        let mut app = App::new(
            Options {
                utc_offset: Some(0),
                ..opts
            },
            w,
            h,
        );
        app.clock = Some(at(5));
        app.open_path(&p, None);
        std::fs::write(&p, AFTER).unwrap();
        app.clock = Some(at(131));
        app.handle(Input::FileChanged(p.clone()));
        app.handle(Input::Tick(
            std::time::Instant::now() + crate::app::RELOAD_DEBOUNCE * 2,
        ));
        app
    }

    /// What a cell's style draws: its colours and modifiers.
    fn drawn(st: Style) -> (Option<Color>, Option<Color>, Modifier) {
        (st.fg, st.bg, st.add_modifier)
    }

    fn rows(s: &Screen) -> Vec<String> {
        (0..s.height)
            .map(|y| s.row(y).trim_end().to_string())
            .collect()
    }

    /// The rails run in the indentation from the root to each change, and
    /// the changed data is drawn in reverse in its kind's colour.
    #[test]
    fn rails_lead_from_the_root_to_each_change() {
        let mut a = saved("rails.json", Options::default(), 60, 20);
        let s = screen(&mut a);
        let r = rows(&s);
        assert_eq!(
            r[1..10],
            [
                "├─▽ {",
                "│ │   title: \"A\"",
                "│ │   year: 2001",
                "│ └─▽ tags: [ −1",
                "│       \"x\"",
                "└─▽ {",
                "  ├─~ title: \"B!\"",
                "  └─▽ tags: [",
                "    └─+ \"z\"",
            ],
            "{}",
            s.text()
        );
        let (x, y) = find(&s, "\"B!\"");
        assert_eq!(drawn(s.style(x, y)), drawn(CHANGED_DATA), "a changed value");
        let (x, y) = find(&s, "\"z\"");
        assert_eq!(drawn(s.style(x, y)), drawn(ADDED_DATA), "an added value");
        assert_eq!(s.style(0, 1).fg, Some(Color::Yellow), "a rail");
        let (x, y) = find(&s, "−1");
        assert_eq!(s.style(x, y).fg, Some(Color::Red), "a removal, counted");
        find(&s, "── 3 changes in version 2 · [ older · d next change ──");
        find(&s, "~ [1].title    \"B\" → \"B!\"");
        let timeline = &r[17];
        assert!(timeline.starts_with(" 14:00:05 ○─"), "{timeline}");
        assert!(timeline.ends_with("● 14:02:11  2 of 2"), "{timeline}");
    }

    #[test]
    fn the_history_tab_lists_the_versions_newest_first() {
        let mut a = saved("list.json", Options::default(), 70, 16);
        a.handle(Input::Key(Key::ch('v')));
        let s = screen(&mut a);
        let r = rows(&s);
        assert_eq!(
            r[1..7],
            [
                "▽ list.json  2 versions",
                "  ▼ version 2  14:02:11  ~1 +1 −1  the newest, on screen",
                "    − [0].tags[1]  \"y\"",
                "    ~ [1].title  \"B\" → \"B!\"",
                "    + [1].tags[0]  \"z\"",
                "    version 1  14:00:05",
            ],
            "{}",
            s.text()
        );
        assert!(
            r[0].contains("2:list.json history"),
            "the tab strip: {}",
            r[0]
        );
        find(&s, "list.json history version 2");
        find(&s, "history · 2 versions");
    }

    #[test]
    fn a_version_in_a_tab_of_its_own_names_its_number() {
        let mut a = saved("own.json", Options::default(), 70, 16);
        a.handle(Input::Key(Key::ch('v')));
        a.handle(Input::Key(Key::code(crate::app::KeyCode::Enter)));
        let s = screen(&mut a);
        assert!(rows(&s)[0].ends_with("3:own.json v2"), "{}", s.text());
        find(
            &s,
            "── 3 changes in version 2 · d next change · v history ──",
        );
        find(&s, "json · version 2 · 1:1");
        let (x, y) = find(&s, "−1");
        assert_eq!(s.style(x, y).fg, REMOVE.fg, "its marks are its own");
    }

    #[test]
    fn an_older_version_turns_the_status_bar_yellow() {
        let mut a = saved("older.json", Options::default(), 60, 20);
        a.handle(Input::Key(Key::ch('[')));
        let s = screen(&mut a);
        let (x, y) = find(&s, "version 1 of 2");
        assert_eq!(drawn(s.style(x, y)), drawn(BAR_PAST));
        assert_eq!(drawn(s.style(0, y)), drawn(BAR_PAST), "the whole bar");
        find(&s, "Version 1 of 2, opened 14:00:05 · ] newer");
    }

    /// Without colour the rails are ASCII where asked, and the changed data
    /// keeps its reverse.
    #[test]
    fn without_colour_the_marks_keep_their_shape() {
        let opts = Options {
            color: false,
            ascii: true,
            ..Options::default()
        };
        let mut a = saved("plain.json", opts, 60, 20);
        let s = screen(&mut a);
        let r = rows(&s);
        assert_eq!(r[1], "|-v {");
        assert_eq!(r[4], "| `-v tags: [ -1");
        let (x, y) = find(&s, "\"B!\"");
        let st = s.style(x, y);
        assert!(st.add_modifier.contains(Modifier::REVERSED), "{st:?}");
        assert_eq!(st.fg, Some(Color::Reset));
        assert!(rows(&s)[17].contains("@ 14:02:11"), "{}", s.text());
    }

    #[test]
    fn the_source_view_marks_the_lines_that_hold_a_change() {
        let mut a = saved("source.json", Options::default(), 60, 20);
        a.handle(Input::Key(Key::ch('s')));
        a.settle_colours();
        let s = screen(&mut a);
        let r = rows(&s);
        assert!(r[1].starts_with("− 2 "), "{}", s.text());
        assert!(r[2].starts_with("~ 3 "), "a change outranks an addition");
        assert!(r[3].starts_with("  4 "));
        let (x, y) = find(&s, "\"B!\"");
        assert_eq!(drawn(s.style(x, y)), drawn(CHANGED_DATA));
        let (x, y) = find(&s, "\"z\"");
        assert_eq!(drawn(s.style(x, y)), drawn(ADDED_DATA));
    }

    /// Where `needle` first shows on screen: its column and row.
    fn find(s: &Screen, needle: &str) -> (usize, usize) {
        for y in 0..s.height {
            let row = s.row(y);
            if let Some(at) = row.find(needle) {
                return (cols(&row[..at]), y);
            }
        }
        panic!("{needle:?} is not on screen:\n{}", s.text());
    }

    /// The source view colours each token as the tab's grammar lexed it
    /// once the colours are in, the focused line aside, which keeps its
    /// one style.
    #[test]
    fn the_source_view_is_coloured_by_the_tabs_grammar() {
        let mut a = app(
            "{\n  \"name\": \"ada\",\n  \"age\": 36,\n  \"ok\": true\n}\n",
            40,
            10,
        );
        a.handle(Input::Key(Key::ch('s')));
        a.settle_colours();
        let s = screen(&mut a);
        let fg = |needle: &str| {
            let (x, y) = find(&s, needle);
            s.style(x, y).fg
        };
        assert_eq!(fg("\"ada\""), Some(Color::Green), "{}", s.text());
        assert_eq!(fg("36"), Some(Color::Magenta));
        assert_eq!(fg("true"), Some(Color::Yellow));
        assert_eq!(fg(","), Some(Color::Reset), "punctuation stays plain");
        let (x, y) = find(&s, "{");
        let focused = s.style(x, y);
        assert!(
            focused.add_modifier.contains(Modifier::REVERSED),
            "{focused:?}"
        );
        assert_eq!(focused.fg, Some(Color::Reset));
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

    /// A number is drawn as its source spelled it in JSON's syntax, in its
    /// row and in a collapsed preview.
    #[test]
    fn a_number_is_drawn_as_spelled() {
        let mut a = app(r#"{"a": 1.0, "big": [12345678901234567890, 1e21]}"#, 50, 8);
        let text = screen(&mut a).text();
        assert!(text.lines().any(|l| l.contains("a: 1.0")), "{text}");
        assert!(text.contains("12345678901234567890"), "{text}");
        assert!(text.contains("1e21") && !text.contains("1e+21"), "{text}");
        a.handle(Input::Key(Key::ch('j')));
        a.handle(Input::Key(Key::ch('j')));
        a.handle(Input::Key(Key::ch('h')));
        let text = screen(&mut a).text();
        assert!(text.contains("(2) [12345678901234567890, 1e21]"), "{text}");
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

    /// A coloured line shows what the plain view shows, control characters
    /// and the cut included, and a line of a million characters keeps a
    /// screenful of spans.
    #[test]
    fn a_coloured_line_shows_and_cuts_as_a_plain_one() {
        let plain = |text: &str| -> String {
            text.chars()
                .map(|c| match c {
                    '\t' => "    ".to_string(),
                    c if (c as u32) < 0x20 => "·".to_string(),
                    c => c.to_string(),
                })
                .collect()
        };
        let run = |start, end, kind| Run { start, end, kind };
        let text = "a\tb\u{1}c\u{7f}d\u{85}e 日本 x";
        let all = [run(0, text.len(), TokenType::String)];
        for avail in 0..=24 {
            let line = coloured(text, &all, avail);
            assert_eq!(line.plain(), clip(&plain(text), 0, avail).0, "{avail}");
        }

        let line = coloured(
            "\"k\": 12",
            &[run(0, 3, TokenType::String), run(5, 7, TokenType::Number)],
            20,
        );
        let spans: Vec<(&str, Style)> = line
            .spans
            .iter()
            .map(|s| (s.content.as_ref(), s.style))
            .collect();
        assert_eq!(spans, [("\"k\"", STRING), (": ", PLAIN), ("12", NUMBER)]);

        let item = "\"k\": 1, ";
        let long = item.repeat(125_000);
        let runs: Vec<Run> = (0..125_000)
            .flat_map(|i| {
                let at = i * item.len();
                [
                    run(at, at + 3, TokenType::String),
                    run(at + 5, at + 6, TokenType::Number),
                ]
            })
            .collect();
        let line = coloured(&long, &runs, 40);
        assert_eq!(line.plain(), clip(&long, 0, 40).0);
        assert!(line.spans.len() <= 40, "{} spans", line.spans.len());
    }

    /// A style boundary inside a grapheme cluster, a combining mark lexed
    /// apart from its base or an emoji sequence split at its joiners, does
    /// not split the cluster: it takes its first character's style, and
    /// takes the columns the plain view gives it.
    #[test]
    fn a_cluster_takes_the_style_of_its_first_character() {
        let run = |start, end, kind| Run { start, end, kind };
        let spans = |line: &Line| -> Vec<(String, Style)> {
            line.spans
                .iter()
                .map(|s| (s.content.to_string(), s.style))
                .collect()
        };
        let text = "e\u{301}x";
        let runs = [
            run(0, 1, TokenType::String),
            run(1, 3, TokenType::Number),
            run(3, 4, TokenType::Keyword),
        ];
        let line = coloured(text, &runs, 10);
        assert_eq!(
            spans(&line),
            [("e\u{301}".to_string(), STRING), ("x".to_string(), BOOL)]
        );

        let family = "👨\u{200d}👩\u{200d}👧 ok";
        let kinds = [TokenType::String, TokenType::Number];
        let runs: Vec<Run> = family
            .char_indices()
            .take(5)
            .enumerate()
            .map(|(k, (i, c))| run(i, i + c.len_utf8(), kinds[k % 2]))
            .collect();
        for avail in 0..=8 {
            let line = coloured(family, &runs, avail);
            assert_eq!(line.plain(), clip(family, 0, avail).0, "{avail}");
            assert!(line.cols() <= avail, "{avail}: {}", line.cols());
        }
        let line = coloured(family, &runs, 10);
        assert_eq!(
            spans(&line)[0],
            ("👨\u{200d}👩\u{200d}👧".to_string(), STRING)
        );
        assert_eq!(line.cols(), cols(family));
    }

    /// A line that never fills the window, all marks of no width, is
    /// measured a few times over, not once a chunk.
    #[test]
    fn a_line_of_marks_of_no_width_is_coloured_at_once() {
        let marks = format!("e{}", "\u{301}".repeat(200_000));
        let runs = [Run {
            start: 0,
            end: marks.len(),
            kind: TokenType::String,
        }];
        let began = std::time::Instant::now();
        let line = coloured(&marks, &runs, 80);
        assert!(
            began.elapsed() < std::time::Duration::from_secs(5),
            "{:?}",
            began.elapsed()
        );
        assert_eq!(line.plain(), clip(&marks, 0, 80).0);
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

    /// An overlay pans as far as its widest line's end as the screen measures
    /// it: a halfwidth kana with its sound mark is wider there than its
    /// string width.
    #[test]
    fn an_overlay_pans_to_the_end_of_what_the_screen_shows() {
        let mut a = app(&format!(r#"{{"k": "{}end"}}"#, "ﾊﾟ".repeat(30)), 40, 8);
        for c in "jpv".chars() {
            a.handle(Input::Key(Key::ch(c)));
        }
        assert_eq!(a.mode, Mode::Overlay);
        for _ in 0..200 {
            a.handle(Input::Key(Key::code(crate::app::KeyCode::Right)));
        }
        // Thirty double-width kana, `end` and its quotes are 65 wide: 25 past a
        // 40-column screen.
        assert_eq!(a.overlay.as_ref().unwrap().xoff, 25);
        let s = screen(&mut a);
        let row = s.row(1);
        assert!(row.starts_with('…'), "{row:?}");
        assert!(
            row.trim_end().ends_with("ﾊﾟend\""),
            "the tail is reached: {row:?}"
        );
    }

    /// Wide, joined and combining characters take the columns the terminal
    /// gives them, as ratatui's buffer places them: a value that fits is
    /// shown whole, one that does not ends in its ellipsis without
    /// splitting a character, and a bar's highlight reaches the right edge.
    #[test]
    fn wide_and_joined_characters_take_the_columns_the_terminal_gives_them() {
        let family = "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}";
        // CJK, an emoji, a family joined by zero-width joiners, a flag and a
        // halfwidth kana with its sound mark are each double width; a letter
        // with its combining accent is single.
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
        // The family value fits beside its key.
        let row = s.row(1);
        assert!(
            row.trim_end()
                .ends_with(&format!("\"{}\"", family.repeat(4))),
            "{row:?}"
        );
        // The kana overflow: the ellipsis follows the last whole one, a column
        // short of the edge, since the next is double.
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
