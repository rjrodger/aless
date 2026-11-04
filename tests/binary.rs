//! The built binary, run the way a shell runs it with no terminal: what it
//! prints about itself (`--version`, `-h`, `--help`, `--generate`), that
//! the committed man page and completions are what it writes, and that,
//! being a viewer, it refuses to start without a terminal, at once and with
//! nothing on standard output.

use std::process::{Command, Output, Stdio};

use aless::cli;

const BIN: &str = env!("CARGO_BIN_EXE_aless");

fn aless(args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap()
}

fn code(out: &Output) -> i32 {
    out.status.code().unwrap_or(-1)
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn version_is_the_crates() {
    let out = aless(&["--version"]);
    assert_eq!(code(&out), 0);
    assert_eq!(text(&out.stdout), format!("aless {}\n", cli::VERSION));
    assert!(out.stderr.is_empty());
    assert_eq!(text(&aless(&["-V"]).stdout), text(&out.stdout));
}

/// `-h` is the summary: an option a line, and where the rest is.
#[test]
fn short_help_is_a_summary_of_the_options() {
    let out = aless(&["-h"]);
    assert_eq!(code(&out), 0);
    let text = text(&out.stdout);
    assert!(text.lines().count() < 60, "{text}");
    for said in [
        "-k, --kind, --format <FORMAT> ",
        "--grammar <NAME=FILE> ",
        "--no-color, --no-colour ",
        "aless --help prints the whole reference",
    ] {
        assert!(text.contains(said), "{said:?} in:\n{text}");
    }
    assert!(!text.contains("EXIT STATUS"), "{text}");
}

/// `--help` is the whole reference, and it describes a viewer: what it
/// shows, every option, the keys, the limits and the exit statuses.
#[test]
fn help_describes_the_viewer() {
    let out = aless(&["--help"]);
    assert_eq!(code(&out), 0);
    let text = text(&out.stdout);
    assert!(
        text.find("THE VIEWER:") < text.find("INPUT OPTIONS ("),
        "{text}"
    );
    for opt in cli::OPTIONS {
        let head = opt.head();
        assert!(
            text.lines().any(|l| l.trim_start() == head),
            "--help has no line for {head}"
        );
    }
    // The reference, with its lines joined, so a wrapped sentence reads whole.
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    for said in [
        "aless is a viewer for the terminal.",
        "It refuses before reading any input",
        "KEYS:",
        "CUSTOM GRAMMARS:",
        "LARGE INPUTS AND LIMITS:",
        "EXIT STATUS: 0 You quit the viewer. 1 The viewer failed while it ran",
        "2 The viewer could not start",
        "TERM dumb: a terminal that cannot draw the viewer",
        "-k, --kind, --format <FORMAT>",
        "--format is another name for this option",
        "--watch Reload a file when it changes, and keep your place in it. Reloading is the default.",
        "SECONDS is a number of seconds (2.5, 90s) or of minutes (2m). The default is none, and 0 is no limit.",
        "--generate <WHAT>",
        "NO_COLOR set and not empty",
    ] {
        assert!(flat.contains(said), "{said:?} in:\n{text}");
    }
}

/// The man page and the completions in the repository are what the binary
/// writes, byte for byte: a release archive carries the committed files.
#[test]
fn the_generated_files_are_current() {
    for (what, path) in [
        ("man", "man/aless.1"),
        ("complete-bash", "completions/aless.bash"),
        ("complete-zsh", "completions/_aless"),
        ("complete-fish", "completions/aless.fish"),
        ("complete-powershell", "completions/_aless.ps1"),
    ] {
        let out = aless(&["--generate", what]);
        assert_eq!(code(&out), 0, "{what}");
        let committed = std::fs::read(path).unwrap_or_default();
        assert!(
            out.stdout == committed,
            "{path} is not what `aless --generate {what}` writes: run admin/scripts/generate.sh"
        );
    }
    let out = aless(&["--generate", "nope"]);
    assert_eq!(code(&out), 2);
    assert!(out.stdout.is_empty());
    assert!(
        text(&out.stderr).starts_with("aless: --generate writes man, complete-bash"),
        "{}",
        text(&out.stderr)
    );
}

/// A viewer needs a terminal. Without one it refuses at once, before any
/// input is read (the file named need not exist), with one line on
/// standard error, nothing on standard output, and the status 2.
#[test]
fn without_a_terminal_the_viewer_refuses() {
    for args in [
        &["tests/fixtures/nested.json"][..],
        &["tests/fixtures/nested.json", "tests/fixtures/sample.yaml"],
        &["no/such/file.json"],
        &["-k", "yaml", "tests/fixtures/nested.json"],
        &[
            "--grammar",
            "nope=no/such/grammar.abnf",
            "tests/fixtures/nested.json",
        ],
        &[],
        &["."],
    ] {
        let out = aless(args);
        let err = text(&out.stderr);
        assert_eq!(code(&out), 2, "{args:?}: {err}");
        assert!(out.stdout.is_empty(), "{args:?} wrote to standard output");
        assert_eq!(err.lines().count(), 1, "{args:?}: {err}");
        assert!(
            err.starts_with("aless: cannot start the viewer, which needs a terminal")
                && err.contains("standard output is not a terminal"),
            "{args:?}: {err}"
        );
    }
}

/// A command line aless cannot read is refused with one line and the
/// status 2, before the terminal is looked for.
#[test]
fn usage_errors_are_one_line_with_status_2() {
    for (args, said) in [
        (
            &["--nope"][..],
            "aless: --nope is not an option. aless --help lists the options",
        ),
        (
            &["--frob", "x.json"],
            "aless: --frob is not an option. aless --help lists the options",
        ),
        (
            &["--mode", "nope"],
            "aless: --mode takes data or line, not nope",
        ),
        (&["--depth", "x"], "aless: --depth needs a number, not x"),
        (&["--grammar", "hosts"], "aless: --grammar needs NAME=FILE"),
        (
            &["--generate", "nope"],
            "aless: --generate writes man, complete-bash",
        ),
        (&["--kind"], "aless: --kind needs a value"),
    ] {
        let out = aless(args);
        let err = text(&out.stderr);
        assert_eq!(code(&out), 2, "{args:?}: {err}");
        assert!(out.stdout.is_empty(), "{args:?} wrote to standard output");
        assert_eq!(err.lines().count(), 1, "{args:?}: {err}");
        assert!(
            err.starts_with(said),
            "{args:?}: {err:?} does not start {said:?}"
        );
    }
}

/// `aless --help | head` must not fail on the closed pipe.
#[test]
fn a_reader_that_stops_early_is_not_an_error() {
    use std::io::Read;
    let mut child = Command::new(BIN)
        .arg("--help")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut first = [0u8; 16];
    child.stdout.take().unwrap().read_exact(&mut first).unwrap();
    // The read end is closed here.
    let out = child.wait_with_output().unwrap();
    assert_eq!(code(&out), 0, "{}", text(&out.stderr));
    assert!(out.stderr.is_empty(), "{}", text(&out.stderr));
}
