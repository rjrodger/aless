//! The terminal side of aless: arguments, raw mode, the event loop,
//! painting, and the effects the application asks for.

use std::ffi::OsString;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crossterm::cursor::{Hide, Show};
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode as CtKey, KeyEventKind,
    KeyModifiers, MouseButton, MouseEventKind,
};
use crossterm::terminal::{
    self, BeginSynchronizedUpdate, EndSynchronizedUpdate, EnterAlternateScreen,
    LeaveAlternateScreen,
};
use crossterm::{execute, queue};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use aless::app::{App, Effect, Input, Key, KeyCode, Options};
use aless::cli;
use aless::grammar::{self, Definition};
use aless::load::Format;
use aless::render;
use aless::watch::FileWatcher;

enum Parsed {
    /// The viewer.
    Run(Box<Args>),
    /// Text to print as it is, and exit: `-h`, `--help`, `--version`,
    /// `--generate`.
    Print(String),
}

struct Args {
    files: Vec<PathBuf>,
    /// `-k`, as given: resolved once the grammars are registered, since it
    /// may name one.
    kind_name: Option<String>,
    kind: Option<Format>,
    /// `--grammar` and `--grammar-expr`, in order.
    grammars: Vec<Definition>,
    opts: Options,
    mouse: bool,
    /// The largest input to read, in bytes; `None` for no limit.
    max_size: Option<u64>,
    /// The longest a parse may run; `None` for no limit.
    timeout: Option<std::time::Duration>,
}

/// Read `argv`, without the program's name. An option [`cli::OPTIONS`]
/// does not list is refused (ADR-006).
fn parse_args(argv: impl IntoIterator<Item = OsString>) -> Result<Parsed, String> {
    let mut args = Args {
        files: Vec::new(),
        kind_name: None,
        kind: None,
        grammars: Vec::new(),
        opts: Options::default(),
        mouse: true,
        max_size: aless::load::Limits::DEFAULT.max_size,
        timeout: aless::load::Limits::DEFAULT.timeout,
    };
    let mut it = argv.into_iter();
    let mut only_files = false;
    while let Some(arg) = it.next() {
        let s = arg.to_string_lossy().into_owned();
        if only_files || !s.starts_with('-') || s == "-" {
            args.files.push(PathBuf::from(arg));
            continue;
        }
        // Options end at `--`: every argument after it is a file.
        if s == "--" {
            only_files = true;
            continue;
        }
        // A long option's value may follow it or be attached: `--kind=json`.
        let (name, mut inline) = match s.split_once('=') {
            Some((n, v)) if n.starts_with("--") => (n.to_string(), Some(v.to_string())),
            _ => (s.clone(), None),
        };
        if cli::lookup(&name).is_none() {
            return Err(format!("unknown option: {name} (see aless --help)"));
        }
        let mut value = || -> Result<String, String> {
            match inline.take() {
                Some(v) => Ok(v),
                None => it
                    .next()
                    .map(|v| v.to_string_lossy().into_owned())
                    .ok_or_else(|| format!("{name} needs a value")),
            }
        };
        let number = |v: String| -> Result<usize, String> {
            v.parse()
                .map_err(|_| format!("{name} needs a number, not {v}"))
        };
        match name.as_str() {
            "-h" => return Ok(Parsed::Print(cli::summary())),
            "--help" => return Ok(Parsed::Print(cli::reference())),
            "-V" | "--version" => return Ok(Parsed::Print(format!("aless {}\n", cli::VERSION))),
            "--generate" => return cli::generate(&value()?).map(Parsed::Print),
            "-k" | "--kind" | "--format" => args.kind_name = Some(value()?),
            "--grammar" | "--grammar-expr" => {
                // Raw, not through `value()`: FILE is a path, and a path's
                // bytes need not be UTF-8. Attached (`--grammar=NAME=FILE`),
                // the value is what follows the argument's own first `=`.
                let v = match inline.take() {
                    Some(_) => grammar::after_eq(&arg),
                    None => it.next().ok_or_else(|| format!("{name} needs a value"))?,
                };
                args.grammars.push(Definition::parse_os(&name, &v)?);
            }
            "--max-size" => args.max_size = aless::load::parse_size(&value()?)?,
            "--timeout" => args.timeout = aless::load::parse_timeout(&value()?)?,
            "--no-watch" => args.opts.watch = false,
            "--watch" => args.opts.watch = true,
            "-m" | "--mode" => args.opts.line_mode = parse_mode(&value()?)?,
            "--depth" => {
                args.opts.depth = Some(u32::try_from(number(value()?)?).unwrap_or(u32::MAX))
            }
            "-n" | "--line-numbers" => args.opts.numbers = true,
            "-N" | "--no-line-numbers" => args.opts.numbers = false,
            "-r" | "--relative-line-numbers" => args.opts.relative = true,
            "-R" | "--no-relative-line-numbers" => args.opts.relative = false,
            "--scrolloff" => args.opts.scrolloff = number(value()?)?,
            "--indent" => args.opts.indent = number(value()?)?.min(16),
            "--hidden" => args.opts.show_hidden = true,
            "--ascii" => args.opts.ascii = true,
            "--no-color" | "--no-colour" => args.opts.color = false,
            "--no-mouse" => args.mouse = false,
            other => return Err(format!("unknown option: {other} (see aless --help)")),
        }
        if let Some(v) = inline {
            return Err(format!("{name} takes no value, but was given {v:?}"));
        }
    }
    if std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()) {
        args.opts.color = false;
    }
    Ok(Parsed::Run(Box::new(args)))
}

fn parse_mode(v: &str) -> Result<bool, String> {
    match v {
        "line" => Ok(true),
        "data" => Ok(false),
        v => Err(format!("unknown mode: {v} (data or line)")),
    }
}

/// The exit status of a run that could not start (ADR-001).
const USAGE: i32 = 2;

/// Why there is no screen to draw on, when there is none.
fn no_screen() -> Option<&'static str> {
    if !io::stdout().is_terminal() {
        Some("standard output is not a terminal")
    } else if std::env::var_os("TERM").is_some_and(|t| t == "dumb") {
        Some("TERM=dumb names a terminal that cannot draw it")
    } else {
        None
    }
}

/// One line on standard error, then the exit status 2.
fn refuse_usage(e: &str) -> ! {
    eprintln!("aless: {e}");
    std::process::exit(USAGE);
}

fn main() {
    let mut args = match parse_args(std::env::args_os().skip(1)) {
        Ok(Parsed::Run(a)) => *a,
        Ok(Parsed::Print(text)) => {
            // `aless --help | head` must not panic on the closed pipe.
            let _ = io::stdout().write_all(text.as_bytes());
            std::process::exit(0);
        }
        Err(e) => refuse_usage(&e),
    };
    // Before any input is read, grammar files included: a pipe left open
    // or a FIFO would otherwise keep a viewer that cannot start waiting.
    if let Some(why) = no_screen() {
        refuse_viewer(why);
    }
    if let Err(e) = key_terminal() {
        refuse_viewer(&format!("no terminal to read keys from ({e})"));
    }
    // Grammars first, since -k or a file's extension may name one; a
    // grammar file is held to --max-size and --timeout as any input is.
    let grammars = std::mem::take(&mut args.grammars);
    let limits = aless::load::Limits {
        max_size: args.max_size,
        timeout: args.timeout,
        started: None,
    };
    if let Err(e) = grammar::register_all(grammars, limits) {
        refuse_usage(&e.to_string());
    }
    if let Some(name) = &args.kind_name {
        match Format::from_name(name) {
            Some(f) => args.kind = Some(f),
            None => refuse_usage(&format!(
                "unknown format: {name} (one of {})",
                Format::known_names().join(" ")
            )),
        }
    }
    aless::load::set_limits(limits);
    let (width, height) = terminal::size().unwrap_or((80, 24));
    let mut app = App::new(args.opts, width, height);

    let stdin_format = args.kind.unwrap_or(Format::Json);
    let mut stdin_read = None;
    if args.files.is_empty() && !io::stdin().is_terminal() {
        stdin_read = Some(aless::load::read_within(io::stdin(), args.max_size));
    }
    for file in &args.files {
        // The parse blocks; on a large file say so before the screen is
        // taken over (one over the limit is refused at once instead).
        if let Ok(m) = std::fs::metadata(file) {
            if m.len() > 4 << 20 && args.max_size.is_none_or(|max| m.len() <= max) {
                eprintln!("aless: loading {} ({} MB)…", file.display(), m.len() >> 20);
            }
        }
        if file.as_os_str() == "-" {
            let read = aless::load::read_within(io::stdin(), args.max_size);
            open_stdin(&mut app, read, stdin_format);
        } else {
            app.open_path(file, args.kind);
        }
    }
    if let Some(read) = stdin_read {
        open_stdin(&mut app, read, stdin_format);
    }
    if app.tabs.is_empty() {
        app.open_explorer(std::path::Path::new("."));
    }
    // The first tab is the one asked for first.
    app.active = 0;
    attach_stdin_to_terminal();

    match run(app, args.mouse) {
        Ok(()) => {}
        Err(Failed::Setup(e)) => refuse_viewer(&e.to_string()),
        Err(Failed::Running(e)) => {
            let _ = restore_terminal(args.mouse);
            eprintln!("aless: {e}");
            std::process::exit(1);
        }
    }
}

/// Standard input's document in a tab, or a failed tab saying why not.
fn open_stdin(app: &mut App, read: Result<Vec<u8>, aless::load::LoadError>, format: Format) {
    match read {
        Ok(bytes) => {
            let text = match String::from_utf8(bytes) {
                Ok(s) => s,
                Err(e) => String::from_utf8_lossy(e.as_bytes()).into_owned(),
            };
            app.open_source("(stdin)", text, format);
        }
        Err(err) => app.open_failed_source("(stdin)", String::new(), format, err),
    }
}

/// Say why the viewer cannot start, then exit with status 2. Nothing has
/// been drawn, so nothing needs restoring.
fn refuse_viewer(why: &str) -> ! {
    refuse_usage(&format!(
        "cannot start the viewer, which needs a terminal to draw on and read keys from: {why} \
         (aless --help says what aless does)"
    ))
}

/// Whether the viewer will have a terminal to read keys from: standard
/// input when it is one, else `/dev/tty`, which needs a controlling one.
#[cfg(unix)]
fn key_terminal() -> io::Result<()> {
    if io::stdin().is_terminal() {
        return Ok(());
    }
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .map(drop)
}

/// A console process whose output is a console can read keys from it.
#[cfg(not(unix))]
fn key_terminal() -> io::Result<()> {
    Ok(())
}

/// With the document piped in, point standard input at the terminal, by
/// standard output's device name: crossterm would otherwise read keys from
/// `/dev/tty`, which kqueue on macOS cannot watch, and never hear one.
#[cfg(unix)]
fn attach_stdin_to_terminal() {
    use std::ffi::CStr;
    use std::os::unix::fs::OpenOptionsExt;
    use std::os::unix::io::AsRawFd;

    if io::stdin().is_terminal() {
        return;
    }
    let mut name = [0 as libc::c_char; 256];
    // SAFETY: the buffer is valid for its whole length, which is passed.
    if unsafe { libc::ttyname_r(libc::STDOUT_FILENO, name.as_mut_ptr(), name.len()) } != 0 {
        return;
    }
    // SAFETY: ttyname_r succeeded, so the buffer holds a NUL-terminated name.
    let name = unsafe { CStr::from_ptr(name.as_ptr()) };
    let Ok(path) = name.to_str() else { return };
    let Ok(tty) = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOCTTY)
        .open(path)
    else {
        return;
    };
    // SAFETY: both descriptors are open; dup2 makes 0 a copy of the terminal.
    unsafe {
        libc::dup2(tty.as_raw_fd(), libc::STDIN_FILENO);
    }
}

#[cfg(not(unix))]
fn attach_stdin_to_terminal() {}

/// Why the viewer stopped: it never started, or it failed while it ran.
enum Failed {
    Setup(io::Error),
    Running(io::Error),
}

/// Take the terminal over; on failure, whatever was done is undone.
fn setup_terminal(mouse: bool) -> io::Result<()> {
    terminal::enable_raw_mode()?;
    let mut out = io::stdout();
    if let Err(e) = execute!(out, EnterAlternateScreen, Hide) {
        // Some of that may have reached the terminal before the failure.
        let _ = execute!(out, Show, LeaveAlternateScreen);
        let _ = terminal::disable_raw_mode();
        return Err(e);
    }
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

fn run(app: App, mouse: bool) -> Result<(), Failed> {
    setup_terminal(mouse).map_err(Failed::Setup)?;
    event_loop(app, mouse).map_err(Failed::Running)
}

fn event_loop(mut app: App, mouse: bool) -> io::Result<()> {
    // A panic anywhere must not leave the terminal in raw mode.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // A grammar panic is caught by the loader and shown as a load
        // error; the terminal must stay as it is.
        if aless::load::parse_in_progress() {
            return;
        }
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

    let mut terminal = Terminal::new(CrosstermBackend::new(io::BufWriter::new(io::stdout())))?;
    loop {
        if apply_effects(&mut app, &mut watcher, terminal.backend_mut(), mouse)? {
            // The screen was left and re-entered: nothing on it survives.
            terminal.clear()?;
        }
        if app.quit {
            break;
        }
        queue!(terminal.backend_mut(), BeginSynchronizedUpdate)?;
        terminal.draw(|frame| {
            render::draw(frame, &mut app);
        })?;
        execute!(terminal.backend_mut(), EndSynchronizedUpdate)?;
        let wait = if app.reload_pending() || app.highlight_pending() {
            Duration::from_millis(40)
        } else {
            Duration::from_millis(500)
        };
        match rx.recv_timeout(wait) {
            Ok(input) => app.handle(input),
            Err(mpsc::RecvTimeoutError::Timeout) => app.handle(Input::Tick(Instant::now())),
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        while let Ok(input) = rx.try_recv() {
            app.handle(input);
        }
    }
    restore_terminal(mouse)
}

/// Carry out the application's requests; true when the screen is to be redrawn whole.
fn apply_effects(
    app: &mut App,
    watcher: &mut Option<FileWatcher>,
    out: &mut impl Write,
    mouse: bool,
) -> io::Result<bool> {
    let effects: Vec<Effect> = std::mem::take(&mut app.effects);
    let mut repaint = false;
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
            Effect::Suspend => {
                suspend(mouse)?;
                repaint = true;
            }
        }
    }
    Ok(repaint)
}

#[cfg(unix)]
fn suspend(mouse: bool) -> io::Result<()> {
    restore_terminal(mouse)?;
    // SAFETY: raising SIGTSTP on this process has no preconditions.
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
            MouseEventKind::Down(MouseButton::Left) => Some(Input::Click(m.column, m.row)),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aless::cli::{Value, OPTIONS};

    fn parse(argv: &[&str]) -> Result<Parsed, String> {
        parse_args(argv.iter().map(OsString::from))
    }

    fn sample(long: &str, value: Value) -> &'static str {
        match (long, value) {
            (_, Value::Words(words)) => words[0],
            (_, Value::Format) => "json",
            ("--grammar", _) => "g=g.abnf",
            ("--grammar-expr", _) => "g=doc = TX",
            ("--max-size", _) => "1M",
            _ => "1",
        }
    }

    /// The table is the parser's gate, so every option the help, the man
    /// page and the completions name is read, under each of its names.
    #[test]
    fn every_option_in_the_table_is_read() {
        for opt in OPTIONS {
            for name in opt.all_names() {
                let mut argv = vec![name.as_str()];
                if let Some((_, value)) = opt.arg {
                    argv.push(sample(opt.long, value));
                }
                if let Err(e) = parse(&argv) {
                    panic!("{argv:?}: {e}");
                }
                // Attached, a long option's value reads the same.
                if let (Some((_, value)), true) = (opt.arg, name.starts_with("--")) {
                    let attached = format!("{name}={}", sample(opt.long, value));
                    if let Err(e) = parse(&[attached.as_str()]) {
                        panic!("{attached}: {e}");
                    }
                }
            }
        }
    }

    #[test]
    fn an_option_the_table_does_not_list_is_refused() {
        for argv in [&["--nope"][..], &["-x"], &["--Kind"], &["-kjson"]] {
            match parse(argv) {
                Err(e) => assert!(e.starts_with("unknown option: "), "{argv:?}: {e}"),
                Ok(_) => panic!("{argv:?} was read"),
            }
        }
        // After `--`, an option's name is a file's.
        match parse(&["--", "--nope"]) {
            Ok(Parsed::Run(args)) => assert_eq!(args.files, [PathBuf::from("--nope")]),
            _ => panic!("-- --nope"),
        }
    }

    #[test]
    fn help_version_and_generate_print() {
        let print = |argv: &[&str]| match parse(argv) {
            Ok(Parsed::Print(text)) => text,
            _ => panic!("{argv:?} prints nothing"),
        };
        assert_eq!(print(&["-h"]), cli::summary());
        assert_eq!(print(&["--help"]), cli::reference());
        assert_eq!(print(&["-V"]), format!("aless {}\n", cli::VERSION));
        assert_eq!(print(&["--generate", "man"]), cli::man_page());
        assert_eq!(print(&["--generate=complete-fish"]), cli::complete_fish());
        let Err(e) = parse(&["--generate", "nope"]) else {
            panic!("--generate nope");
        };
        assert!(e.starts_with("--generate writes man, "), "{e}");
    }
}
