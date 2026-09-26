//! The terminal side of aless: command-line arguments, raw-mode setup,
//! the event loop, painting, and the side effects the application asks
//! for (watching files, copying, suspending).

use std::io::{self, IsTerminal, Write};
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
use aless::headless::{self, Op, Request, Start};
use aless::load::Format;
use aless::render::{self, Color, Screen, Style};
use aless::watch::FileWatcher;

const USAGE: &str = "\
aless — JSON, YAML, TOML, CSV, XML, INI, Markdown and more as one tree: a
jless-style viewer in a terminal, and JSON on standard output for scripts
and agents

USAGE:
    aless [OPTIONS] [FILE]...           the viewer, in a terminal
    aless --paths --depth 1 FILE        output, anywhere (see WITHOUT A SCREEN)
    <command> | aless [OPTIONS]

WITHOUT A SCREEN (scripts, agents, pipes):
    Any option in this section but --depth (the viewer has one too), or
    a standard output that is not a terminal, prints JSON instead of
    starting the viewer, and nothing waits for keys: `aless FILE | jq .`
    is the document as JSON.

        --json              The document as JSON (the default), or the value
                            at the start that --path or --at gives
        --paths             An entry for the start and each node below it:
                            {path, kind, line, col, value or length}
        --find <REGEX>      Entries of the nodes whose `\"key\": value` text
                            matches: smart case, `REGEX/s` to match case
        --where             The start's entry: its path and source position
        --check             Parse each FILE and report
                            {ok, files: [{file, format, ok, error}]}
        --path <PATH>       Start at PATH, in the jq syntax every output uses
                            (.a.b[0].\"odd key\"); a.b[0], $.a.b[0] and JSON
                            Pointer (/a/b/0) work too
        --at <LINE[:COL]>   Start at the node at that source position
        --depth <N>         --paths and --find go N levels below the start
        --limit <N>         At most N entries (default 200, 0 for all);
                            \"total\" and \"truncated\" say what was left out
        --compact           JSON on one line

    Quote a PATH for the shell ('.a[0]'). Positions are 1-based, and a
    keyed value is at its key. Numbers are 64-bit floats. Input is read
    whole before it is parsed, so output starts when the parse ends.
    Errors are JSON on standard error: {\"error\": {\"kind\", \"message\", …}},
    with the file, line, col, code and hint when the input did not parse.
    Exit status: 0 success, 1 the input did not parse (--check: an input
    failed), 2 bad usage or no terminal for the viewer, 3 an input could
    not be read (or the output not written), 4 --path or --at names
    nothing, 5 an input is over --max-size, 6 a parse ran past --timeout.

    aless --paths --depth 1 config.yaml     what is in it
    aless --json --path '.spec.containers[0]' deploy.yaml
    aless --where --at 42:7 deploy.yaml     the path at a linter's 42:7
    aless --find '\"image\":' deploy.yaml     every image, with its line
    aless --check $(git ls-files '*.toml')  do they all parse?
    aless -k csv --json < data.csv          stdin is JSON unless -k says

THE VIEWER (in a terminal):
    Each FILE opens in its own tab (Tab / Shift-Tab switch). Files are
    watched and reloaded on change, keeping your place. A directory opens
    in the explorer: the same tree, Enter opens a file. Without a FILE,
    standard input is read when it is not a terminal, else the current
    directory is explored. Keys: F1 or :help inside aless.

        --no-watch          Do not reload files when they change
    -m, --mode <MODE>       Start in `data` (default) or `line` mode
        --depth <N>         Fold containers deeper than N levels at start
    -n, --line-numbers      Show absolute line numbers
    -N, --no-line-numbers
    -r, --relative-line-numbers
    -R, --no-relative-line-numbers
        --scrolloff <N>     Rows kept around the focus when scrolling (default 3)
        --indent <N>        Indentation per level (default 2; --json too)
        --hidden            Show dot-files in the explorer
        --ascii             Draw fold markers with ASCII characters
        --no-color          No colours (as does NO_COLOR)
        --no-mouse          Do not capture the mouse

BOTH:
    -k, --kind <FORMAT>     Parse every input as FORMAT instead of by extension:
                            json jsonl jsonic jsonc json5 yaml toml ini csv tsv
                            xml zon markdown feed text
        --max-size <SIZE>   Refuse an input larger than SIZE (default 64M; K, M
                            or G; 0 for no limit): a parse takes about 80 bytes
                            of memory per byte of input
        --timeout <SECONDS> Stop a parse that runs longer than this (2.5, 90s,
                            2m; default none): a large input can take minutes
    -h, --help              This help
    -V, --version           Version
";

struct Args {
    files: Vec<PathBuf>,
    kind: Option<Format>,
    opts: Options,
    mouse: bool,
    /// The operation a headless option asked for.
    op: Option<Op>,
    start: Start,
    limit: Option<usize>,
    compact: bool,
    /// An option that only means something without a screen was given.
    headless: bool,
    /// The largest input to read, in bytes; `None` for no limit.
    max_size: Option<u64>,
    /// The longest a parse may run; `None` for no limit.
    timeout: Option<std::time::Duration>,
}

/// Options that ask for output rather than the viewer.
const HEADLESS_OPTIONS: &[&str] = &[
    "--json",
    "--paths",
    "--find",
    "--where",
    "--check",
    "--path",
    "--at",
    "--limit",
    "--compact",
];

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        files: Vec::new(),
        kind: None,
        opts: Options::default(),
        mouse: true,
        op: None,
        start: Start::Root,
        limit: None,
        compact: false,
        headless: false,
        max_size: aless::load::Limits::DEFAULT.max_size,
        timeout: aless::load::Limits::DEFAULT.timeout,
    };
    let mut it = std::env::args_os().skip(1);
    let mut only_files = false;
    while let Some(arg) = it.next() {
        let s = arg.to_string_lossy().into_owned();
        if only_files || !s.starts_with('-') || s == "-" {
            args.files.push(PathBuf::from(arg));
            continue;
        }
        // A long option's value may follow it or be attached: `--kind=json`.
        let (name, mut inline) = match s.split_once('=') {
            Some((n, v)) if n.starts_with("--") => (n.to_string(), Some(v.to_string())),
            _ => (s.clone(), None),
        };
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
        if HEADLESS_OPTIONS.contains(&name.as_str()) {
            args.headless = true;
        }
        let mut op = None;
        match name.as_str() {
            "--" => only_files = true,
            "-h" | "--help" => {
                // `aless --help | head` must not panic on the closed pipe.
                let _ = io::stdout().write_all(USAGE.as_bytes());
                std::process::exit(0);
            }
            "-V" | "--version" => {
                let version = format!("aless {}\n", env!("CARGO_PKG_VERSION"));
                let _ = io::stdout().write_all(version.as_bytes());
                std::process::exit(0);
            }
            "-k" | "--kind" | "--format" => {
                let v = value()?;
                args.kind =
                    Some(Format::from_name(&v).ok_or_else(|| format!("unknown format: {v}"))?);
            }
            "--json" => op = Some(Op::Json),
            "--paths" => op = Some(Op::Paths),
            "--find" => op = Some(Op::Find(value()?)),
            "--where" => op = Some(Op::Where),
            "--check" => op = Some(Op::Check),
            "--path" => {
                let v = value()?;
                if matches!(args.start, Start::At(..)) {
                    return Err("--path and --at both say where to start: give one".into());
                }
                args.start = Start::Path(v);
            }
            "--at" => {
                let v = value()?;
                if matches!(args.start, Start::Path(_)) {
                    return Err("--path and --at both say where to start: give one".into());
                }
                args.start = Start::parse_at(&v)?;
            }
            "--limit" => args.limit = Some(number(value()?)?),
            "--max-size" => args.max_size = aless::load::parse_size(&value()?)?,
            "--timeout" => args.timeout = aless::load::parse_timeout(&value()?)?,
            "--compact" => args.compact = true,
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
        if let Some(op) = op {
            match &args.op {
                Some(prev) if prev.flag() != op.flag() => {
                    return Err(format!(
                        "{} and {} both say what to print: give one of --json, --paths, --find, --where, --check",
                        prev.flag(),
                        op.flag()
                    ));
                }
                _ => args.op = Some(op),
            }
        }
    }
    if args.op == Some(Op::Check) && args.start != Start::Root {
        return Err("--check parses whole files: it takes no --path or --at".into());
    }
    if std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()) {
        args.opts.color = false;
    }
    Ok(args)
}

fn parse_mode(v: &str) -> Result<bool, String> {
    match v {
        "line" => Ok(true),
        "data" => Ok(false),
        v => Err(format!("unknown mode: {v} (data or line)")),
    }
}

/// Whether this run prints rather than shows: an option asked for output,
/// or there is no screen to show on (standard output is not a terminal,
/// or it is one that cannot draw one, `TERM=dumb`).
fn headless_wanted(args_ask: bool) -> bool {
    args_ask || !io::stdout().is_terminal() || std::env::var_os("TERM").is_some_and(|t| t == "dumb")
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            // The options could not be read, so ask the raw ones whether
            // this was meant to run without a screen.
            let asked = std::env::args_os().skip(1).any(|a| {
                let a = a.to_string_lossy();
                HEADLESS_OPTIONS.contains(&a.split('=').next().unwrap_or_default())
            });
            if headless_wanted(asked) {
                let error = serde_json::json!({"error": {"kind": "usage", "message": e}});
                eprint!("{}", headless::render(&error, false));
            } else {
                eprintln!("aless: {e}");
            }
            std::process::exit(headless::status::USAGE);
        }
    };
    aless::load::set_limits(aless::load::Limits {
        max_size: args.max_size,
        timeout: args.timeout,
    });
    if headless_wanted(args.headless) {
        std::process::exit(print_headless(args));
    }
    // Before any input is read: input that never ends (a pipe left open, a
    // FIFO) would otherwise keep a viewer that cannot start waiting.
    if let Err(e) = key_terminal() {
        refuse_viewer(&e);
    }
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
        // Nothing named and nothing piped: explore the current directory.
        app.open_explorer(std::path::Path::new("."));
    }
    // The first tab is the one asked for first.
    app.active = 0;
    attach_stdin_to_terminal();

    match run(app, args.mouse) {
        Ok(()) => {}
        Err(Failed::Setup(e)) => refuse_viewer(&e),
        Err(Failed::Running(e)) => {
            let _ = restore_terminal(args.mouse);
            eprintln!("aless: {e}");
            std::process::exit(1);
        }
    }
}

/// Open what standard input held in a tab, or the reason it could not be
/// read (too large, say) as a failed one.
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

/// Say that the viewer cannot start and what works instead, then exit with
/// status 2. Nothing has been drawn, so there is nothing to restore.
fn refuse_viewer(e: &io::Error) -> ! {
    eprintln!(
        "aless: cannot start the viewer: {e}\n\
         The viewer needs a terminal to draw on and read keys from. To read a\n\
         file without a screen, use --json, --paths, --find, --where or --check\n\
         (see aless --help); aless FILE > out.json writes the document as JSON."
    );
    std::process::exit(headless::status::USAGE);
}

/// Check that the viewer will have a terminal to read keys from: the one
/// crossterm takes, standard input when it is a terminal, else `/dev/tty`,
/// which opens only for a process with a controlling terminal.
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

/// A console process whose output is a console has one to read keys from;
/// crossterm opens it when the viewer starts.
#[cfg(not(unix))]
fn key_terminal() -> io::Result<()> {
    Ok(())
}

/// When the document came through a pipe, point standard input at the
/// terminal before the viewer starts, opened by the device's own name
/// (standard output's). crossterm would otherwise read keys from
/// `/dev/tty`, which macOS cannot watch (kqueue rejects it: crossterm
/// issue 500), and the viewer would draw and then never hear a key.
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

/// Run without a screen and print the result; returns the exit status.
fn print_headless(args: Args) -> i32 {
    let mut req = Request::new(args.op.unwrap_or(Op::Json));
    req.files = args.files;
    req.kind = args.kind;
    req.start = args.start;
    req.depth = args.opts.depth;
    req.limit = args.limit.unwrap_or(headless::DEFAULT_LIMIT);
    req.compact = args.compact;
    req.indent = args.opts.indent;
    req.max_size = args.max_size;
    req.timeout = args.timeout;
    let mut input = io::stdin();
    let stdin: headless::Stdin = if input.is_terminal() {
        None
    } else {
        Some(&mut input)
    };
    let out = headless::run(&req, stdin);
    // A reader that stops early (`| head`) is not an error.
    let mut stdout = io::stdout().lock();
    let written = stdout
        .write_all(out.stdout.as_bytes())
        .and_then(|()| stdout.flush());
    if let Err(e) = written {
        if e.kind() != io::ErrorKind::BrokenPipe {
            let _ = io::stderr().write_all(headless::write_failure(&e, req.compact).as_bytes());
            return headless::status::IO;
        }
    }
    let _ = io::stderr().write_all(out.stderr.as_bytes());
    out.status
}

/// Why the viewer stopped: it never started (no terminal), or something
/// failed while it ran.
enum Failed {
    Setup(io::Error),
    Running(io::Error),
}

/// Take the terminal over. On failure, whatever was done is undone, so
/// the caller has nothing to restore.
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

    let mut out = io::BufWriter::new(io::stdout());
    let mut last_screen: Option<Screen> = None;
    loop {
        if apply_effects(&mut app, &mut watcher, &mut out, mouse)? {
            // The screen was left and re-entered: nothing on it survives.
            last_screen = None;
        }
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

/// Carry out the application's requests. Returns whether the terminal
/// contents must be repainted from scratch.
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
