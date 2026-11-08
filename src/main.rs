//! The terminal side of aless: arguments, raw mode, the event loop,
//! painting, and the effects the application asks for.

use std::io::{self, IsTerminal, Write};
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

use aless::app::{App, Effect, Input, Key, KeyCode};
use aless::cli::{self, Parsed};
use aless::load::Format;
use aless::render;
use aless::watch::FileWatcher;

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
    // The local zone can be read only while the process has one thread.
    let utc_offset = time::UtcOffset::current_local_offset()
        .ok()
        .map(|o| o.whole_seconds());
    let mut args = match cli::parse_args(std::env::args_os().skip(1)) {
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
    if let Err(e) = args.prepare() {
        refuse_usage(&e);
    }
    let (width, height) = terminal::size().unwrap_or((80, 24));
    args.opts.utc_offset = utc_offset;
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
