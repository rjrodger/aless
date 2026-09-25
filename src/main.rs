//! The terminal side of aless: command-line arguments, raw-mode setup,
//! the event loop, painting, and the side effects the application asks
//! for (watching files, copying, suspending).

use std::io::{self, IsTerminal, Read, Write};
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode as CtKey, KeyEventKind,
    KeyModifiers, MouseButton, MouseEventKind,
};
use crossterm::style::{
    Attribute, Color as CtColor, Print, ResetColor, SetAttribute, SetBackgroundColor,
    SetForegroundColor,
};
use crossterm::terminal::{
    self, BeginSynchronizedUpdate, Clear, ClearType, EndSynchronizedUpdate, EnterAlternateScreen,
    LeaveAlternateScreen,
};
use crossterm::{execute, queue};

use aless::app::{App, Effect, Input, Key, KeyCode, Options};
use aless::load::Format;
use aless::render::{self, Color, Screen, Style};
use aless::watch::FileWatcher;

const USAGE: &str = "\
aless — a jless-style viewer for JSON, YAML, TOML, CSV, XML, INI, Markdown and more

USAGE:
    aless [OPTIONS] [FILE]...
    <command> | aless [OPTIONS]

Each FILE opens in its own tab (Tab / Shift-Tab switch). Files are
watched and reloaded on change, keeping your place. Without a FILE,
standard input is read when it is not a terminal.

OPTIONS:
    -k, --kind <FORMAT>     Parse every input as FORMAT instead of by extension:
                            json jsonl jsonic jsonc json5 yaml toml ini csv tsv
                            xml zon markdown feed text
        --no-watch          Do not reload files when they change
    -m, --mode <MODE>       Start in `data` (default) or `line` mode
        --depth <N>         Fold containers deeper than N levels at start
    -n, --line-numbers      Show absolute line numbers
    -N, --no-line-numbers
    -r, --relative-line-numbers
    -R, --no-relative-line-numbers
        --scrolloff <N>     Rows kept around the focus when scrolling (default 3)
        --indent <N>        Indentation per level (default 2)
        --ascii             Draw fold markers with ASCII characters
        --no-color          No colours
        --no-mouse          Do not capture the mouse
    -h, --help              This help
    -V, --version           Version

KEYS: press F1 or type :help inside aless.
";

struct Args {
    files: Vec<PathBuf>,
    kind: Option<Format>,
    opts: Options,
    mouse: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        files: Vec::new(),
        kind: None,
        opts: Options::default(),
        mouse: true,
    };
    let mut it = std::env::args_os().skip(1);
    let mut only_files = false;
    while let Some(arg) = it.next() {
        let s = arg.to_string_lossy().into_owned();
        if only_files || !s.starts_with('-') || s == "-" {
            args.files.push(PathBuf::from(arg));
            continue;
        }
        let mut value = |name: &str| -> Result<String, String> {
            it.next()
                .map(|v| v.to_string_lossy().into_owned())
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match s.as_str() {
            "--" => only_files = true,
            "-h" | "--help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            "-V" | "--version" => {
                println!("aless {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "-k" | "--kind" | "--format" => {
                let v = value(&s)?;
                args.kind =
                    Some(Format::from_name(&v).ok_or_else(|| format!("unknown format: {v}"))?);
            }
            "--no-watch" => args.opts.watch = false,
            "--watch" => args.opts.watch = true,
            "-m" | "--mode" => match value(&s)?.as_str() {
                "line" => args.opts.line_mode = true,
                "data" => args.opts.line_mode = false,
                v => return Err(format!("unknown mode: {v} (data or line)")),
            },
            "--depth" => {
                let v = value(&s)?;
                args.opts.depth = Some(
                    v.parse()
                        .map_err(|_| format!("--depth needs a number, not {v}"))?,
                );
            }
            "-n" | "--line-numbers" => args.opts.numbers = true,
            "-N" | "--no-line-numbers" => args.opts.numbers = false,
            "-r" | "--relative-line-numbers" => args.opts.relative = true,
            "-R" | "--no-relative-line-numbers" => args.opts.relative = false,
            "--scrolloff" => {
                let v = value(&s)?;
                args.opts.scrolloff = v
                    .parse()
                    .map_err(|_| format!("--scrolloff needs a number, not {v}"))?;
            }
            "--indent" => {
                let v = value(&s)?;
                args.opts.indent = v
                    .parse::<usize>()
                    .map_err(|_| format!("--indent needs a number, not {v}"))?
                    .min(16);
            }
            "--ascii" => args.opts.ascii = true,
            "--no-color" | "--no-colour" => args.opts.color = false,
            "--no-mouse" => args.mouse = false,
            other => {
                // `-k json` may also be written `--kind=json`.
                if let Some((name, v)) = other.split_once('=') {
                    match name {
                        "--kind" | "--format" => {
                            args.kind = Some(
                                Format::from_name(v)
                                    .ok_or_else(|| format!("unknown format: {v}"))?,
                            );
                            continue;
                        }
                        "--mode" => {
                            args.opts.line_mode = v == "line";
                            continue;
                        }
                        "--depth" => {
                            args.opts.depth = Some(
                                v.parse()
                                    .map_err(|_| format!("--depth needs a number, not {v}"))?,
                            );
                            continue;
                        }
                        "--scrolloff" => {
                            args.opts.scrolloff = v
                                .parse()
                                .map_err(|_| format!("--scrolloff needs a number, not {v}"))?;
                            continue;
                        }
                        "--indent" => {
                            args.opts.indent = v
                                .parse::<usize>()
                                .map_err(|_| format!("--indent needs a number, not {v}"))?
                                .min(16);
                            continue;
                        }
                        _ => {}
                    }
                }
                return Err(format!("unknown option: {other}\n\n{USAGE}"));
            }
        }
    }
    if std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()) {
        args.opts.color = false;
    }
    Ok(args)
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("aless: {e}");
            std::process::exit(2);
        }
    };
    let (width, height) = terminal::size().unwrap_or((80, 24));
    let mut app = App::new(args.opts, width, height);

    let mut stdin_text = None;
    if args.files.is_empty() && !io::stdin().is_terminal() {
        let mut buf = Vec::new();
        if let Err(e) = io::stdin().read_to_end(&mut buf) {
            eprintln!("aless: reading standard input: {e}");
            std::process::exit(1);
        }
        stdin_text = Some(String::from_utf8_lossy(&buf).into_owned());
    }
    for file in &args.files {
        // The parse blocks; on a large file say so before the screen is
        // taken over.
        if let Ok(m) = std::fs::metadata(file) {
            if m.len() > 4 << 20 {
                eprintln!("aless: loading {} ({} MB)…", file.display(), m.len() >> 20);
            }
        }
        if file.as_os_str() == "-" {
            let mut buf = Vec::new();
            let _ = io::stdin().read_to_end(&mut buf);
            app.open_source(
                "(stdin)",
                String::from_utf8_lossy(&buf).into_owned(),
                args.kind.unwrap_or(Format::Json),
            );
        } else {
            app.open_path(file, args.kind);
        }
    }
    if let Some(text) = stdin_text {
        app.open_source("(stdin)", text, args.kind.unwrap_or(Format::Json));
    }
    if app.tabs.is_empty() {
        app.open_welcome();
    }
    // The first tab is the one asked for first.
    app.active = 0;

    if let Err(e) = run(app, args.mouse) {
        let _ = restore_terminal(args.mouse);
        eprintln!("aless: {e}");
        std::process::exit(1);
    }
}

fn setup_terminal(mouse: bool) -> io::Result<()> {
    terminal::enable_raw_mode()?;
    let mut out = io::stdout();
    execute!(out, EnterAlternateScreen, Hide)?;
    if mouse {
        let _ = execute!(out, EnableMouseCapture);
    }
    Ok(())
}

fn restore_terminal(mouse: bool) -> io::Result<()> {
    let mut out = io::stdout();
    if mouse {
        let _ = execute!(out, DisableMouseCapture);
    }
    execute!(out, Show, LeaveAlternateScreen)?;
    terminal::disable_raw_mode()
}

fn run(mut app: App, mouse: bool) -> io::Result<()> {
    setup_terminal(mouse)?;
    // A panic anywhere must not leave the terminal in raw mode.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = restore_terminal(mouse);
        default_hook(info);
    }));

    let (tx, rx) = mpsc::channel::<Input>();
    let input_tx = tx.clone();
    thread::spawn(move || {
        while let Ok(ev) = event::read() {
            if let Some(input) = convert(ev) {
                if input_tx.send(input).is_err() {
                    break;
                }
            }
        }
    });
    let mut watcher = FileWatcher::new(tx.clone());

    let mut out = io::BufWriter::new(io::stdout());
    let mut last_screen: Option<Screen> = None;
    loop {
        apply_effects(&mut app, &mut watcher, &mut out, mouse)?;
        if app.quit {
            break;
        }
        let screen = render::render(&mut app);
        if last_screen.as_ref() != Some(&screen) {
            paint(&mut out, &screen, app.opts.color)?;
            last_screen = Some(screen);
        }
        let wait = if app.reload_pending() {
            Duration::from_millis(40)
        } else {
            Duration::from_millis(500)
        };
        match rx.recv_timeout(wait) {
            Ok(input) => app.handle(input),
            Err(mpsc::RecvTimeoutError::Timeout) => app.handle(Input::Tick(Instant::now())),
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        // Drain whatever else is queued before painting again.
        while let Ok(input) = rx.try_recv() {
            app.handle(input);
        }
    }
    restore_terminal(mouse)
}

fn apply_effects(
    app: &mut App,
    watcher: &mut Option<FileWatcher>,
    out: &mut impl Write,
    mouse: bool,
) -> io::Result<()> {
    let effects: Vec<Effect> = std::mem::take(&mut app.effects);
    for effect in effects {
        match effect {
            Effect::Watch(path) => {
                if let Some(w) = watcher.as_mut() {
                    let _ = w.watch(&path);
                }
            }
            Effect::Unwatch(path) => {
                if let Some(w) = watcher.as_mut() {
                    w.unwatch(&path);
                }
            }
            Effect::Copy { text, what } => {
                let how = aless::clip::copy(out, &text);
                app.copied(&what, how);
            }
            Effect::Suspend => suspend(mouse)?,
        }
    }
    Ok(())
}

#[cfg(unix)]
fn suspend(mouse: bool) -> io::Result<()> {
    restore_terminal(mouse)?;
    // SAFETY: raising SIGTSTP on our own process has no preconditions.
    unsafe {
        libc::raise(libc::SIGTSTP);
    }
    setup_terminal(mouse)
}

#[cfg(not(unix))]
fn suspend(_mouse: bool) -> io::Result<()> {
    Ok(())
}

fn convert(ev: Event) -> Option<Input> {
    match ev {
        Event::Key(k) => {
            if k.kind == KeyEventKind::Release {
                return None;
            }
            let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
            let alt = k.modifiers.contains(KeyModifiers::ALT);
            let shift = k.modifiers.contains(KeyModifiers::SHIFT);
            let code = match k.code {
                CtKey::Char(c) => KeyCode::Char(c),
                CtKey::Enter => KeyCode::Enter,
                CtKey::Esc => KeyCode::Esc,
                CtKey::Backspace => KeyCode::Backspace,
                CtKey::Tab => KeyCode::Tab,
                CtKey::BackTab => KeyCode::BackTab,
                CtKey::Up => KeyCode::Up,
                CtKey::Down => KeyCode::Down,
                CtKey::Left => KeyCode::Left,
                CtKey::Right => KeyCode::Right,
                CtKey::Home => KeyCode::Home,
                CtKey::End => KeyCode::End,
                CtKey::PageUp => KeyCode::PageUp,
                CtKey::PageDown => KeyCode::PageDown,
                CtKey::Delete => KeyCode::Delete,
                CtKey::F(n) => KeyCode::F(n),
                _ => return None,
            };
            // Shift-Tab arrives as BackTab on most terminals, as Tab+SHIFT
            // on some.
            let code = if code == KeyCode::Tab && shift {
                KeyCode::BackTab
            } else {
                code
            };
            Some(Input::Key(Key { code, ctrl, alt }))
        }
        Event::Resize(w, h) => Some(Input::Resize(w, h)),
        Event::Mouse(m) => match m.kind {
            MouseEventKind::ScrollDown => Some(Input::Wheel(3)),
            MouseEventKind::ScrollUp => Some(Input::Wheel(-3)),
            MouseEventKind::Down(MouseButton::Left) => Some(Input::Click(m.row)),
            _ => None,
        },
        _ => None,
    }
}

fn ct_color(c: Color) -> CtColor {
    match c {
        Color::Default => CtColor::Reset,
        Color::Black => CtColor::Black,
        Color::Red => CtColor::Red,
        Color::Green => CtColor::Green,
        Color::Yellow => CtColor::Yellow,
        Color::Blue => CtColor::Blue,
        Color::Magenta => CtColor::Magenta,
        Color::Cyan => CtColor::Cyan,
        Color::White => CtColor::White,
        Color::Grey => CtColor::DarkGrey,
    }
}

fn paint(out: &mut impl Write, screen: &Screen, color: bool) -> io::Result<()> {
    queue!(out, BeginSynchronizedUpdate, Hide)?;
    for (y, line) in screen.lines.iter().enumerate() {
        queue!(out, MoveTo(0, y as u16))?;
        for span in &line.spans {
            let s: Style = span.style;
            queue!(out, SetAttribute(Attribute::Reset), ResetColor)?;
            if color {
                if s.fg != Color::Default {
                    queue!(out, SetForegroundColor(ct_color(s.fg)))?;
                }
                if s.bg != Color::Default {
                    queue!(out, SetBackgroundColor(ct_color(s.bg)))?;
                }
                if s.dim {
                    queue!(out, SetAttribute(Attribute::Dim))?;
                }
                if s.underline {
                    queue!(out, SetAttribute(Attribute::Underlined))?;
                }
            }
            if s.bold {
                queue!(out, SetAttribute(Attribute::Bold))?;
            }
            if s.reverse {
                queue!(out, SetAttribute(Attribute::Reverse))?;
            }
            queue!(out, Print(&span.text))?;
        }
        queue!(
            out,
            SetAttribute(Attribute::Reset),
            ResetColor,
            Clear(ClearType::UntilNewLine)
        )?;
    }
    for y in screen.lines.len()..screen.height {
        queue!(out, MoveTo(0, y as u16), Clear(ClearType::UntilNewLine))?;
    }
    match screen.cursor {
        Some((x, y)) => queue!(out, MoveTo(x, y), Show)?,
        None => queue!(out, Hide)?,
    }
    queue!(out, EndSynchronizedUpdate)?;
    out.flush()
}
