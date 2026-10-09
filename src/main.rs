//! The terminal side of aless: command-line arguments, raw-mode setup,
//! the event loop, painting, and the side effects the application asks
//! for (watching files, copying, suspending).

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

use aless::alchemy::ProgramArg;
use aless::app::{App, Effect, Input, Key, KeyCode, Options};
use aless::cli;
use aless::grammar::{self, Definition};
use aless::headless::{self, Op, Request, Start};
use aless::load::Format;
use aless::pane::{Role, Through};
use aless::render;
use aless::watch::FileWatcher;

/// What the command line asks for.
enum Parsed {
    /// The viewer, or output without one.
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
    /// The operation a headless option asked for.
    op: Option<Op>,
    /// `--alchemy` or `--alchemy-expr`: combined with `op` once every
    /// option is read, since `--render` may name its renderer.
    program: Option<ProgramArg>,
    /// `--explain`.
    explain: bool,
    start: Start,
    limit: Option<usize>,
    compact: bool,
    /// An option that only means something without a screen was given.
    headless: bool,
    /// Those options, as given: `--panes` takes `--render` and the
    /// program for its output pane and refuses the rest.
    headless_given: Vec<String>,
    /// `--panes` was given, so the viewer opens, whichever panes it names.
    panes: bool,
    /// The largest input to read, in bytes; `None` for no limit.
    max_size: Option<u64>,
    /// The longest a parse may run; `None` for no limit.
    timeout: Option<std::time::Duration>,
    /// The most a program may write, in bytes; `None` for no limit.
    max_output: Option<u64>,
}

/// Read the command line, `argv` without the program's name. Every option
/// is one [`cli::OPTIONS`] lists, which the help, the man page and the
/// completions are written from: an option the table does not list is no
/// option.
fn parse_args(argv: impl IntoIterator<Item = OsString>) -> Result<Parsed, String> {
    let mut args = Args {
        files: Vec::new(),
        kind_name: None,
        kind: None,
        grammars: Vec::new(),
        opts: Options::default(),
        mouse: true,
        op: None,
        program: None,
        explain: false,
        start: Start::Root,
        limit: None,
        compact: false,
        headless: false,
        headless_given: Vec::new(),
        panes: false,
        max_size: aless::load::Limits::DEFAULT.max_size,
        timeout: aless::load::Limits::DEFAULT.timeout,
        max_output: Some(aless::load::DEFAULT_MAX_OUTPUT),
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
        if cli::asks_for_output(&name) {
            args.headless = true;
            args.headless_given.push(name.clone());
        }
        let mut op = None;
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
            "--json" => op = Some(Op::Json),
            "--paths" => op = Some(Op::Paths),
            "--find" => op = Some(Op::Find(value()?)),
            "--where" => op = Some(Op::Where),
            "--check" => op = Some(Op::Check),
            "--render" => {
                let v = value()?;
                let renderer = aless::export::Renderer::from_name(&v)
                    .ok_or_else(|| aless::translate::refusal(&v))?;
                op = Some(Op::Render(renderer));
            }
            "--alchemy" => {
                // Raw, as --grammar reads its FILE: a path's bytes need not
                // be UTF-8.
                let v = match inline.take() {
                    Some(_) => grammar::after_eq(&arg),
                    None => it.next().ok_or_else(|| format!("{name} needs a value"))?,
                };
                set_program(&mut args, &name, ProgramArg::File(PathBuf::from(v)))?;
            }
            "--alchemy-expr" => set_program(&mut args, &name, ProgramArg::Expr(value()?))?,
            "--explain" => args.explain = true,
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
            "--max-output" => {
                args.max_output = aless::load::parse_size_for("--max-output", &value()?)?
            }
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
            "--panes" => {
                args.panes = true;
                let spec = value()?;
                if spec.split(',').all(|p| p.trim().is_empty()) {
                    return Err(
                        "--panes names the panes to open beside the input: out, program, or \
                         out,program"
                            .into(),
                    );
                }
                for part in spec.split(',').filter(|p| !p.trim().is_empty()) {
                    match Role::from_name(part) {
                        // The input pane is always there.
                        Some(Role::Input) => {}
                        Some(role) if !args.opts.panes.contains(&role) => {
                            args.opts.panes.push(role)
                        }
                        Some(_) => {}
                        None => {
                            return Err(format!(
                                "--panes names out and program, not {}",
                                part.trim()
                            ))
                        }
                    }
                }
            }
            "--stacked" => args.opts.stacked = true,
            other => return Err(format!("unknown option: {other} (see aless --help)")),
        }
        if let Some(v) = inline {
            return Err(format!("{name} takes no value, but was given {v:?}"));
        }
        if let Some(op) = op {
            match &args.op {
                Some(prev) if prev.flag() != op.flag() => {
                    return Err(format!(
                        "{} and {} both say what to print: give one of --json, --paths, --find, --where, --check, --render",
                        prev.flag(),
                        op.flag()
                    ));
                }
                _ => args.op = Some(op),
            }
        }
    }
    // With --panes the viewer opens, and --render and the program say what
    // the output pane shows rather than what to print.
    if args.panes {
        let taken = ["--render", "--alchemy", "--alchemy-expr"];
        if let Some(other) = args
            .headless_given
            .iter()
            .find(|o| !taken.contains(&o.as_str()))
        {
            return Err(format!(
                "--panes opens the viewer, and {other} is for output without one: give one"
            ));
        }
        let render = match args.op.take() {
            None => None,
            Some(Op::Render(renderer)) => Some(renderer),
            Some(other) => {
                return Err(format!(
                    "--panes opens the viewer, and {} prints without one: give one",
                    other.flag()
                ))
            }
        };
        args.opts.through = match (args.program.take(), render) {
            (Some(arg), render) => Through::Program { arg, render },
            (None, Some(renderer)) => Through::Render(renderer),
            (None, None) => Through::default(),
        };
        if args.opts.panes.contains(&Role::Program)
            && !matches!(args.opts.through, Through::Program { .. })
        {
            return Err(
                "--panes program shows a program: give it with --alchemy FILE or \
                 --alchemy-expr TEXT"
                    .into(),
            );
        }
        args.headless = false;
    }
    // A program is what to print, with --render naming its renderer.
    if let Some(program) = args.program.take() {
        let render = match args.op.take() {
            None => None,
            Some(Op::Render(renderer)) => Some(renderer),
            Some(other) => {
                return Err(format!(
                    "--alchemy and {} both say what to print: the program does the selecting; \
                     give one",
                    other.flag()
                ))
            }
        };
        if args.start != Start::Root {
            return Err(
                "--alchemy takes no --path or --at: the program selects what it reads".into(),
            );
        }
        args.op = Some(Op::Alchemy {
            program,
            render,
            explain: args.explain,
        });
    } else if args.explain {
        return Err(
            "--explain reports a program's plan: give the program with --alchemy FILE or \
             --alchemy-expr TEXT"
                .into(),
        );
    }
    if args.op == Some(Op::Check) && args.start != Start::Root {
        return Err("--check parses whole files: it takes no --path or --at".into());
    }
    if matches!(args.op, Some(Op::Render(_))) && matches!(args.start, Start::At(..)) {
        return Err(headless::RENDER_TAKES_NO_AT.into());
    }
    if std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()) {
        args.opts.color = false;
    }
    Ok(Parsed::Run(Box::new(args)))
}

/// Record the program `--alchemy` or `--alchemy-expr` (`name`) names,
/// once: the option given twice, and the two together, are each refused
/// by a message that names what was given.
fn set_program(args: &mut Args, name: &str, program: ProgramArg) -> Result<(), String> {
    match &args.program {
        None => {}
        Some(prev) if prev.path().is_some() == program.path().is_some() => {
            return Err(format!("{name} was given twice: give one program"));
        }
        Some(_) => {
            return Err("--alchemy and --alchemy-expr both name the program: give one".into());
        }
    }
    args.program = Some(program);
    Ok(())
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

/// Say that the command line asks for what cannot be done, as JSON on
/// standard error without a screen (on one line with `--compact`) and as a
/// line with one, and exit with status 2.
fn refuse_usage(e: &str, headless: bool, compact: bool) -> ! {
    if headless {
        eprint!("{}", headless::usage_failure(e, compact));
    } else {
        eprintln!("aless: {e}");
    }
    std::process::exit(headless::status::USAGE);
}

fn main() {
    let mut args = match parse_args(std::env::args_os().skip(1)) {
        Ok(Parsed::Run(a)) => *a,
        Ok(Parsed::Print(text)) => {
            // `aless --help | head` must not panic on the closed pipe.
            let _ = io::stdout().write_all(text.as_bytes());
            std::process::exit(0);
        }
        Err(e) => {
            // The options could not be read, so ask the raw ones whether
            // this was meant to run without a screen, and on one line.
            let raw: Vec<String> = std::env::args_os()
                .skip(1)
                .map(|a| a.to_string_lossy().into_owned())
                .collect();
            let asked = raw
                .iter()
                .any(|a| cli::asks_for_output(a.split('=').next().unwrap_or_default()));
            // Options end at `--`; after it, `--compact` is a file's name.
            let compact = raw
                .iter()
                .take_while(|a| a.as_str() != "--")
                .any(|a| a == "--compact");
            refuse_usage(&e, headless_wanted(asked), compact);
        }
    };
    let headless = headless_wanted(args.headless);
    if headless && args.panes {
        refuse_usage(
            "--panes opens the viewer, which needs a terminal: without one, give --render or \
             --alchemy without --panes",
            true,
            args.compact,
        );
    }
    // Before any input is read, grammar files included: input that never
    // ends (a pipe left open, a FIFO) would otherwise keep a viewer that
    // cannot start waiting.
    if !headless {
        if let Err(e) = key_terminal() {
            refuse_viewer(&e);
        }
    }
    // The grammars come first: -k may name one, and so may a file's
    // extension. A grammar file is read within --max-size as any input is,
    // and compiled within --timeout as any parse runs.
    let grammars = std::mem::take(&mut args.grammars);
    let limits = aless::load::Limits {
        max_size: args.max_size,
        timeout: args.timeout,
        started: None,
    };
    if let Err(e) = grammar::register_all(grammars, limits) {
        let (text, status) = headless::grammar_failure(&e, args.compact);
        if headless {
            eprint!("{text}");
        } else {
            eprintln!("aless: {e}");
        }
        std::process::exit(status);
    }
    if let Some(name) = &args.kind_name {
        match Format::from_name(name) {
            Some(f) => args.kind = Some(f),
            None => refuse_usage(
                &format!(
                    "unknown format: {name} (one of {})",
                    Format::known_names().join(" ")
                ),
                headless,
                args.compact,
            ),
        }
    }
    aless::load::set_limits(limits);
    if headless {
        std::process::exit(print_headless(args));
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
/// status 2. Nothing has been drawn, so there is nothing to restore. There
/// is no terminal to talk to, so this is said as every usage error is
/// without one: `{"error": {"kind": "usage", "message"}}` on standard
/// error, and nothing on standard output.
fn refuse_viewer(e: &io::Error) -> ! {
    refuse_usage(
        &format!(
            "cannot start the viewer, which needs a terminal to draw on and read keys from: {e}. \
             To read a file without a screen, use --json, --paths, --find, --where, --check or \
             --render (see aless --help); aless FILE > out.json writes the document as JSON"
        ),
        true,
        false,
    )
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
    req.max_output = args.max_output;
    // The deadline runs from here, and covers reading the input: standard
    // input is read on a thread of its own, so that a writer that sends
    // nothing, or never closes it, cannot hold the run past --timeout.
    let started = Instant::now();
    req.started = Some(started);
    let mut input = io::stdin();
    // Read on a thread of its own once the run reads it, and never before.
    let mut timed = args
        .timeout
        .filter(|_| !input.is_terminal())
        .and_then(|limit| aless::load::DeadlineReader::new(io::stdin(), started, limit));
    let stdin: headless::Stdin = if input.is_terminal() {
        None
    } else if let Some(timed) = timed.as_mut() {
        Some(timed)
    } else {
        Some(&mut input)
    };
    // A streamed result (`--render`) is written as it is produced, through
    // the renderer's own buffer; the other results come back whole.
    let out = headless::run_to(&req, stdin, Box::new(io::stdout()));
    // A stream that failed on its way out has said so in detail, and left
    // nothing to print: standard output is then not touched again, so that
    // a flush failing once more cannot replace that report with a plainer
    // one. (A failed --check still has its report to print.) A reader that
    // stops early (`| head`) is not an error.
    if !out.stdout.is_empty() || out.status == headless::status::OK {
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

    let mut terminal = Terminal::new(CrosstermBackend::new(io::BufWriter::new(io::stdout())))?;
    loop {
        if apply_effects(&mut app, &mut watcher, terminal.backend_mut(), mouse)? {
            // The screen was left and re-entered: nothing on it survives.
            terminal.clear()?;
        }
        if app.quit {
            break;
        }
        // ratatui compares the frame with the last one and writes only the
        // cells that changed; the terminal shows the update all at once.
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

    /// A value the option takes.
    fn sample(long: &str, value: Value) -> &'static str {
        match (long, value) {
            (_, Value::Words(words)) => words[0],
            (_, Value::Format | Value::Render) => "json",
            ("--grammar", _) => "g=g.abnf",
            ("--grammar-expr", _) => "g=doc = TX",
            ("--alchemy-expr", _) => "def export [input] input",
            ("--path", _) => ".a",
            ("--at", _) => "1:1",
            ("--find", _) => "x",
            ("--max-size" | "--max-output", _) => "1M",
            ("--alchemy", _) => "x.alc",
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
                // --explain needs a program to explain.
                if opt.long == "--explain" {
                    argv.extend(["--alchemy-expr", "def export [input] input"]);
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
        for argv in [&["--nope"][..], &["-x"], &["--Json"], &["-kjson"]] {
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
        assert_eq!(print(&["--generate=skill"]), cli::SKILL);
        let Err(e) = parse(&["--generate", "nope"]) else {
            panic!("--generate nope");
        };
        assert!(e.starts_with("--generate writes man, "), "{e}");
    }
}
