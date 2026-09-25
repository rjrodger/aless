//! Drawing: the whole screen as lines of styled spans, computed from the
//! application state. Terminal-free, so tests can read what a user would
//! see; `main.rs` paints the result with crossterm.

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::{App, Mode, PromptKind};
use crate::doc::{Kind, Row};
use crate::fmt;
use crate::load;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Color {
    #[default]
    Default,
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    Grey,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Style {
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
    pub dim: bool,
    pub reverse: bool,
    pub underline: bool,
}

impl Style {
    pub const PLAIN: Style = Style {
        fg: Color::Default,
        bg: Color::Default,
        bold: false,
        dim: false,
        reverse: false,
        underline: false,
    };

    pub const fn fg(c: Color) -> Style {
        Style {
            fg: c,
            ..Style::PLAIN
        }
    }

    pub const fn bold(self) -> Style {
        Style { bold: true, ..self }
    }

    pub const fn dim(self) -> Style {
        Style { dim: true, ..self }
    }

    pub const fn reverse(self) -> Style {
        Style {
            reverse: true,
            ..self
        }
    }

    pub const fn underline(self) -> Style {
        Style {
            underline: true,
            ..self
        }
    }
}

const KEY: Style = Style::fg(Color::Blue);
const STRING: Style = Style::fg(Color::Green);
const NUMBER: Style = Style::fg(Color::Magenta);
const BOOL: Style = Style::fg(Color::Yellow);
const NULL: Style = Style::fg(Color::Grey);
const BRACKET: Style = Style::PLAIN;
const PREVIEW: Style = Style::fg(Color::Grey);
const MATCH: Style = Style::fg(Color::Yellow).underline();
const FOCUS: Style = Style::PLAIN.reverse().bold();
const GUTTER: Style = Style::fg(Color::Grey);
const GUTTER_FOCUS: Style = Style::fg(Color::Yellow);
const BAR: Style = Style::PLAIN.reverse();
const ERROR: Style = Style::fg(Color::Red);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub style: Style,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Line {
    pub spans: Vec<Span>,
}

impl Line {
    pub fn new() -> Line {
        Line::default()
    }

    /// Append text. Control characters (C0, DEL, C1) never reach the
    /// terminal: a file name or a string value carrying an escape sequence
    /// must not be able to drive it. A tab becomes a space and every other
    /// control character the replacement character.
    pub fn push(&mut self, text: impl Into<String>, style: Style) -> &mut Line {
        let text = sanitize(text.into());
        if !text.is_empty() {
            self.spans.push(Span { text, style });
        }
        self
    }

    pub fn width(&self) -> usize {
        self.spans.iter().map(|s| s.text.width()).sum()
    }

    /// Plain text, styles dropped.
    pub fn text(&self) -> String {
        self.spans.iter().map(|s| s.text.as_str()).collect()
    }

    /// Cut or pad to exactly `width` columns, padding with `style`.
    pub fn fit(&mut self, width: usize, style: Style) {
        let mut used = 0;
        let mut keep = Vec::new();
        for span in self.spans.drain(..) {
            let w = span.text.width();
            if used + w <= width {
                used += w;
                keep.push(span);
            } else {
                let (cut, cw) = take_width(&span.text, width - used);
                used += cw;
                if !cut.is_empty() {
                    keep.push(Span {
                        text: cut,
                        style: span.style,
                    });
                }
                break;
            }
        }
        self.spans = keep;
        if used < width {
            self.push(" ".repeat(width - used), style);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Screen {
    pub width: usize,
    pub height: usize,
    pub lines: Vec<Line>,
    /// Where the terminal cursor goes (prompt editing), column then row.
    pub cursor: Option<(u16, u16)>,
}

impl Screen {
    /// Every line as plain text, joined with newlines.
    pub fn text(&self) -> String {
        self.lines
            .iter()
            .map(|l| l.text().trim_end().to_string())
            .collect::<Vec<_>>()
            .join("\n")
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

/// The leading part of `s` that fits in `width` columns, and its width.
fn take_width(s: &str, width: usize) -> (String, usize) {
    let mut out = String::new();
    let mut used = 0;
    for c in s.chars() {
        let w = c.width().unwrap_or(0);
        if used + w > width {
            break;
        }
        used += w;
        out.push(c);
    }
    (out, used)
}

/// Skip `cols` columns of `s`.
fn skip_width(s: &str, cols: usize) -> &str {
    let mut used = 0;
    for (i, c) in s.char_indices() {
        if used >= cols {
            return &s[i..];
        }
        used += c.width().unwrap_or(0);
    }
    ""
}

/// Fit `text` into `avail` columns, scrolled right by `xoff`: an ellipsis
/// marks a cut at either end. Returns the text and whether it was cut on
/// the right.
pub fn clip(text: &str, xoff: usize, avail: usize) -> (String, bool) {
    let w = text.width();
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
        let (head, _) = take_width(text, avail - 1);
        return (format!("{head}…"), true);
    }
    let rest = skip_width(text, xoff);
    let rest_w = rest.width();
    if rest_w < avail {
        (format!("…{rest}"), false)
    } else {
        let (mid, _) = take_width(rest, avail - 2);
        (format!("…{mid}…"), true)
    }
}

fn digits(n: usize) -> usize {
    n.max(1).to_string().len()
}

/// Draw the whole screen.
pub fn render(app: &mut App) -> Screen {
    app.prepare();
    let width = app.width.max(1);
    let height = app.height.max(1);
    let mut lines: Vec<Line> = Vec::with_capacity(height);
    let mut cursor = None;

    if app.strip_rows() == 1 {
        lines.push(tab_strip(app, width));
    }
    let pane_h = app.pane_height();
    match app.mode {
        Mode::Overlay => lines.extend(overlay_lines(app, width, pane_h)),
        Mode::Source => lines.extend(source_lines(app, width, pane_h)),
        _ => lines.extend(pane_lines(app, width, pane_h)),
    }
    lines.push(status_bar(app, width));
    let (prompt, cur) = prompt_line(app, width);
    if let Some(col) = cur {
        cursor = Some((col as u16, (lines.len()) as u16));
    }
    lines.push(prompt);
    lines.truncate(height);
    Screen {
        width,
        height,
        lines,
        cursor,
    }
}

fn tab_strip(app: &App, width: usize) -> Line {
    let mut line = Line::new();
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
        let style = if i == app.active {
            Style::PLAIN.reverse().bold()
        } else {
            Style::fg(Color::Grey)
        };
        line.push(text, style);
        line.push(" ", Style::PLAIN);
    }
    line.fit(width, Style::PLAIN);
    line
}

fn pane_lines(app: &mut App, width: usize, pane_h: usize) -> Vec<Line> {
    let mut out = Vec::with_capacity(pane_h);
    if app.tabs.is_empty() {
        for _ in 0..pane_h {
            let mut l = Line::new();
            l.fit(width, Style::PLAIN);
            out.push(l);
        }
        return out;
    }
    let numbers = app.opts.numbers;
    let relative = app.opts.relative;
    let indent = app.opts.indent;
    let ascii = app.opts.ascii;
    let tab = app.tab();
    let rows: Vec<Row> = tab.rows().to_vec();
    let focus = tab.focus;
    let scroll = tab.scroll;
    let gutter = if numbers || relative {
        digits(rows.len()) + 1
    } else {
        0
    };
    for i in scroll..scroll + pane_h {
        let mut line = Line::new();
        if let Some(row) = rows.get(i) {
            let focused = i == focus;
            if gutter > 0 {
                let n = if relative && !(numbers && focused) {
                    i.abs_diff(focus)
                } else {
                    i + 1
                };
                let text = format!("{:>w$} ", n, w = gutter - 1);
                line.push(text, if focused { GUTTER_FOCUS } else { GUTTER });
            }
            let node = tab.doc.node(row.node);
            let line_mode = tab.line_mode;
            let is_match = tab.is_match(row.node) && !row.close;
            let has_next = tab.doc.next_sibling(row.node).is_some();
            line.push(" ".repeat(node.depth as usize * indent), Style::PLAIN);
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
            line.push(
                indicator,
                if focused {
                    Style::PLAIN.bold()
                } else {
                    Style::PLAIN
                },
            );
            if let Some(ex) = &tab.explorer {
                explorer_row(
                    &mut line, ex, &tab.doc, row.node, focused, is_match, width, tab.xoff,
                );
                line.fit(width, Style::PLAIN);
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
                    line.push(k, style);
                    line.push(": ", Style::PLAIN);
                    key_shown = true;
                }
            }
            // Value.
            let avail = width.saturating_sub(line.width());
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
            line.push(text, style);
            if comma && !cut {
                line.push(",", Style::PLAIN);
            }
        }
        line.fit(width, Style::PLAIN);
        out.push(line);
    }
    out
}

/// One explorer row after its indicator: the name (with a slash for a
/// directory), then the size and format of a file, or the names inside a
/// collapsed directory.
#[allow(clippy::too_many_arguments)]
fn explorer_row(
    line: &mut Line,
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
                Style::PLAIN
            },
        ),
        // The root row: the directory itself. A long path keeps its tail,
        // the part that says where this is.
        None => {
            let full = format!("{}/", ex.root.display());
            let avail = width.saturating_sub(line.width());
            let label = if full.width() > avail && avail > 1 {
                format!("…{}", skip_width(&full, full.width() + 1 - avail))
            } else {
                full
            };
            (label, if focused { FOCUS } else { KEY.bold() })
        }
    };
    line.push(label, label_style);
    let avail = width.saturating_sub(line.width() + 2);
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
        line.push("  ", Style::PLAIN);
        let (text, _) = clip(&value, if focused { xoff } else { 0 }, avail);
        line.push(text, PREVIEW);
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
        if out.width() + sep.len() + item.width() + 2 > budget {
            out.push_str(if first { "…" } else { ", …" });
            return out;
        }
        out.push_str(sep);
        out.push_str(&item);
        first = false;
    }
    out
}

fn overlay_lines(app: &App, width: usize, pane_h: usize) -> Vec<Line> {
    let mut out = Vec::with_capacity(pane_h);
    let Some(o) = app.overlay.as_ref() else {
        for _ in 0..pane_h {
            let mut l = Line::new();
            l.fit(width, Style::PLAIN);
            out.push(l);
        }
        return out;
    };
    let mut title = Line::new();
    title.push(format!(" {} ", o.title), Style::PLAIN.reverse());
    title.fit(width, Style::PLAIN.reverse());
    out.push(title);
    for i in 0..pane_h.saturating_sub(1) {
        let mut l = Line::new();
        if let Some(text) = o.lines.get(o.scroll + i) {
            let (t, _) = clip(text, 0, width);
            l.push(t, Style::PLAIN);
        }
        l.fit(width, Style::PLAIN);
        out.push(l);
    }
    out
}

fn source_lines(app: &mut App, width: usize, pane_h: usize) -> Vec<Line> {
    let mut out = Vec::with_capacity(pane_h);
    if app.tabs.is_empty() {
        return pane_lines(app, width, pane_h);
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
        let mut line = Line::new();
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
            line.push(
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
            let (t, _) = clip(&shown, 0, width.saturating_sub(line.width()));
            let style = if is_focus {
                Style::PLAIN.reverse()
            } else if is_error {
                ERROR
            } else {
                Style::PLAIN
            };
            line.push(t, style);
            if is_focus {
                line.fit(width, Style::PLAIN.reverse());
            }
        }
        line.fit(width, Style::PLAIN);
        out.push(line);
    }
    out
}

fn status_bar(app: &mut App, width: usize) -> Line {
    let mut line = Line::new();
    if app.tabs.is_empty() {
        line.push(" aless ", BAR);
        line.fit(width, BAR);
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
    let remaining = width.saturating_sub(right.width());
    let err_text = error.map(|e| {
        let room = remaining.saturating_sub(left.width()).max(remaining / 2);
        clip(&e, 0, room).0
    });
    let err_w = err_text.as_deref().map(|e| e.width()).unwrap_or(0);
    let avail_left = remaining.saturating_sub(err_w);
    let left = if left.width() > avail_left {
        let cut = left.width() + 1 - avail_left.max(1);
        format!("…{}", skip_width(&left, cut))
    } else {
        left
    };
    line.push(&left, BAR);
    let pad = width.saturating_sub(left.width() + right.width() + err_w);
    line.push(" ".repeat(pad), BAR);
    if let Some(e) = err_text {
        line.push(e, BAR.bold());
    }
    line.push(right, BAR);
    line.fit(width, BAR);
    line
}

fn prompt_line(app: &mut App, width: usize) -> (Line, Option<usize>) {
    let mut line = Line::new();
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
            let width_of = |from: usize| -> usize {
                chars[from..at].iter().map(|c| c.width().unwrap_or(0)).sum()
            };
            let mut start = 0;
            while start < at && width_of(start) >= avail {
                start += 1;
            }
            line.push(prefix.to_string(), Style::PLAIN);
            line.push(chars[start..].iter().collect::<String>(), Style::PLAIN);
            cursor = Some((1 + width_of(start)).min(width.saturating_sub(1)));
        }
        _ => {
            if let Some(m) = &app.message {
                let (t, _) = clip(&m.text, 0, width);
                line.push(t, if m.error { ERROR.bold() } else { Style::PLAIN });
            } else if let Some(s) = app.tab_ref().and_then(|t| t.search.as_ref()) {
                if let Some(i) = s.current {
                    line.push(
                        format!(
                            "{}{}  [{}/{}]{}",
                            s.direction.prompt(),
                            s.pattern.input,
                            i + 1,
                            s.matches.len(),
                            if s.wrapped { "  W" } else { "" }
                        ),
                        Style::fg(Color::Grey),
                    );
                }
            }
            if let Some(c) = app.count_text() {
                let used = line.width();
                let pad = width.saturating_sub(used + c.width() + 1);
                line.push(" ".repeat(pad), Style::PLAIN);
                line.push(c, Style::PLAIN.bold());
            }
        }
    }
    line.fit(width, Style::PLAIN);
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
        let s = render(&mut a);
        assert_eq!(s.lines.len(), 12);
        assert!(s.lines.iter().all(|l| l.width() == 40));
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
        let text = render(&mut a).text();
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
        let text = render(&mut a).text();
        assert!(text.contains("▶ a: (2) [1, 2]"), "screen was:\n{text}");
        assert!(text.contains(" t.json .a"), "screen was:\n{text}");
    }

    #[test]
    fn truncation_and_scrolling_of_long_values() {
        let long = "x".repeat(100);
        let mut a = app(&format!(r#"{{"k": "{long}"}}"#), 30, 8);
        a.handle(Input::Key(Key::ch('j')));
        let text = render(&mut a).text();
        let row = text.lines().nth(1).unwrap();
        assert_eq!(row.width(), 30);
        assert!(row.ends_with('…'), "{row}");
        a.handle(Input::Key(Key::ch(';')));
        let text = render(&mut a).text();
        let row = text.lines().nth(1).unwrap();
        assert!(row.contains("…xxx"), "{row}");
        assert!(row.ends_with("\""), "{row}");
    }

    #[test]
    fn numbers_prompt_and_tabs() {
        let mut a = app(r#"[1, 2, 3]"#, 30, 8);
        a.run_command("set number");
        let text = render(&mut a).text();
        assert!(text.lines().nth(1).unwrap().starts_with("2 "), "{text}");
        a.run_command("set relativenumber nonumber");
        a.handle(Input::Key(Key::ch('j')));
        let text = render(&mut a).text();
        assert!(text.lines().next().unwrap().starts_with("1 "), "{text}");
        assert!(text.lines().nth(1).unwrap().starts_with("0 "), "{text}");
        for c in ":op".chars() {
            a.handle(Input::Key(Key::ch(c)));
        }
        let s = render(&mut a);
        assert_eq!(s.cursor, Some((3, 7)));
        assert!(s.lines[7].text().starts_with(":op"));
        a.handle(Input::Key(Key::code(crate::app::KeyCode::Esc)));
        a.open_source("u.json", "1".into(), Format::Json);
        let s = render(&mut a);
        let strip = s.lines[0].text();
        assert!(
            strip.contains("1:t.json") && strip.contains("2:u.json"),
            "{strip}"
        );
        assert_eq!(s.lines.len(), 8);
    }

    #[test]
    fn error_and_source_view() {
        let mut a = App::new(Options::default(), 50, 8);
        a.open_source(
            "bad.json",
            "{\n  \"a\": 1,\n  \"b\":\n".into(),
            Format::Json,
        );
        let text = render(&mut a).text();
        assert!(
            text.contains("!4:"),
            "status carries the error position: {text}"
        );
        a.handle(Input::Key(Key::ch('s')));
        let text = render(&mut a).text();
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
        let s = render(&mut a);
        let all: String = s.lines.iter().map(|l| l.text()).collect();
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
        let s = render(&mut a);
        let (col, _) = s.cursor.unwrap();
        assert!(col < 20, "cursor stays on screen: {col}");
        let text = s.lines[5].text();
        assert!(
            text.trim_end().ends_with("file.json"),
            "the tail is shown: {text:?}"
        );
        for _ in 0..12 {
            a.handle(Input::Key(Key::code(crate::app::KeyCode::Left)));
        }
        let s = render(&mut a);
        let text = s.lines[5].text();
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
        let s = render(&mut a);
        assert_eq!(s.cursor.unwrap().0, 1);
        assert!(s.lines[5].text().starts_with(":open"));
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
        let text = render(&mut a).text();
        let first = text.lines().next().unwrap().trim_end();
        let dir_name = dir.file_name().unwrap().to_string_lossy();
        assert!(
            first.starts_with("▼ ") && first.ends_with('/') && first.contains(&*dir_name),
            "root shows its path: {text}"
        );
        assert!(!first.contains(r"\\?\"), "no verbatim prefix: {first}");
        // Narrow: a root path that does not fit keeps its tail.
        a.handle(Input::Resize(20, 8));
        let narrow = render(&mut a);
        let full = narrow.lines[0].text();
        assert_eq!(full.width(), 20, "{full:?}");
        let first = full.trim_end();
        assert!(
            first.starts_with("▼ …") && first.ends_with('/'),
            "{first:?}"
        );
        a.handle(Input::Resize(240, 8));
        let text = render(&mut a).text();
        assert!(text.contains("▷ sub/  (1) c.toml"), "{text}");
        assert!(text.contains("  a.json  8 B  json"), "{text}");
        assert!(!text.contains('"'), "no quoting in the explorer: {text}");
        assert!(text.contains("directory"), "{text}");
        a.handle(Input::Key(Key::ch('j')));
        a.handle(Input::Key(Key::ch('l')));
        let text = render(&mut a).text();
        assert!(
            text.contains("▼ sub/\n"),
            "an expanded directory shows just its name: {text}"
        );
        assert!(text.contains("    c.toml  6 B  toml"), "{text}");
        assert!(text.contains("1 entries"), "{text}");
    }

    #[test]
    fn help_overlay_renders() {
        let mut a = app("1", 60, 10);
        a.handle(Input::Key(Key::code(crate::app::KeyCode::F(1))));
        let text = render(&mut a).text();
        assert!(text.contains("aless help"));
        assert!(text.contains("MOVING"));
    }
}
