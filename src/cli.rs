//! The command line's vocabulary: every option aless takes ([`OPTIONS`])
//! and the reference written from it. The summary `-h` prints, the
//! reference `--help` prints, the man page and the shells' completions
//! (`--generate`) all come from the table and the topics here, and the
//! binary's parser refuses any option the table does not list, so none of
//! them can name an option aless does not take, or leave one out.
//!
//! The reference is one of the four places that document the interface
//! for scripts and agents, with the README's "Scripts and agents" section,
//! the skill (`skills/aless/SKILL.md`) and the tests in `src/headless.rs`
//! and `tests/agent.rs`: they must agree.

use crate::load::Format;

/// This build's version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// What aless is, in a sentence: the help's first line, and the man
/// page's NAME.
pub const TAGLINE: &str = "JSON, YAML, TOML, CSV, XML, INI, Markdown and more as one \
    tree: a jless-style viewer in a terminal, and JSON on standard output for scripts \
    and agents";

/// What aless does, in a paragraph: the man page's DESCRIPTION.
const DESCRIPTION: &str = "aless reads JSON, JSON Lines, JSON5, JSONC, jsonic, YAML, \
    TOML, INI, CSV, TSV, XML, ZON, Markdown and RSS or Atom feeds, and any line-oriented \
    format an ABNF grammar describes, as one tree. In a terminal it shows the tree in a \
    jless-style viewer, with tabs, panes, search and a watch mode that reloads a changed \
    file and keeps your place. Without one it prints JSON on standard output for scripts \
    and agents, and writes any of those formats as any other.";

/// Where an option belongs, and what giving it does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    /// Asks for output without a screen: giving one prints instead of
    /// starting the viewer.
    Output,
    /// Read by the output and the viewer alike.
    Both,
    /// Read by the viewer alone: without a screen it is accepted and does
    /// nothing, but for `--panes`, which opens the viewer.
    Viewer,
    /// Prints something about aless, and exits.
    About,
}

/// What an option's value is, for the shells to complete.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Value {
    /// Text the shell cannot complete: a number, a size, a pattern.
    Text,
    /// A file to read.
    File,
    /// A format aless reads.
    Format,
    /// A format `--render` writes.
    Render,
    /// One of these words.
    Words(&'static [&'static str]),
}

/// One option.
#[derive(Debug)]
pub struct Opt {
    /// Its one-letter name, when it has one.
    pub short: Option<char>,
    pub long: &'static str,
    /// Other names for it.
    pub also: &'static [&'static str],
    /// Its value as the help names it (`FORMAT`), and what that is;
    /// `None` for a flag.
    pub arg: Option<(&'static str, Value)>,
    pub section: Section,
    /// Whether giving it again adds to it, rather than replacing it.
    pub repeats: bool,
    /// What it does, in a line of at most [`SUMMARY_WIDTH`] characters:
    /// for `-h` and the completions.
    pub summary: &'static str,
    /// What it does, in full: for `--help` and the man page.
    pub detail: &'static str,
}

impl Opt {
    /// Its names as the help writes them: `-k, --kind, --format`.
    pub fn names(&self) -> String {
        let mut names: Vec<String> = Vec::new();
        if let Some(c) = self.short {
            names.push(format!("-{c}"));
        }
        names.push(self.long.to_string());
        names.extend(self.also.iter().map(|a| a.to_string()));
        names.join(", ")
    }

    /// Every name it answers to: the long one, the others, `-x`.
    pub fn all_names(&self) -> Vec<String> {
        let mut names = vec![self.long.to_string()];
        names.extend(self.also.iter().map(|a| a.to_string()));
        if let Some(c) = self.short {
            names.push(format!("-{c}"));
        }
        names
    }

    /// Its names and value, as the help heads it: `-k, --kind, --format
    /// <FORMAT>`.
    pub fn head(&self) -> String {
        match self.arg {
            Some((arg, _)) => format!("{} <{arg}>", self.names()),
            None => self.names(),
        }
    }
}

/// The width of a summary, so that `-h` fits 80 columns.
pub const SUMMARY_WIDTH: usize = 44;

const MODES: &[&str] = &["data", "line"];
const PANES: &[&str] = &["out", "program", "out,program"];

/// What `--generate` writes.
pub const GENERATE: &[&str] = &[
    "man",
    "complete-bash",
    "complete-zsh",
    "complete-fish",
    "complete-powershell",
    "skill",
];

/// Every option aless takes, in the order the help lists them. `{renders}`,
/// `{formats}` and `{version}` in a detail are filled in as it is written.
pub const OPTIONS: &[Opt] = &[
    Opt {
        short: None,
        long: "--json",
        also: &[],
        arg: None,
        section: Section::Output,
        repeats: false,
        summary: "the document as JSON (the default)",
        detail: "The document as JSON, or the value at the start that --path or --at \
            gives: what aless prints without a screen when no other output option is \
            given. Indented 2 spaces a level (--indent N), or on one line with \
            --compact. Numbers are 64-bit floats, so an integer beyond 2^53 comes out \
            as the nearest one, and NaN and the infinities as null; --render json keeps \
            each number as the source spelled it.",
    },
    Opt {
        short: None,
        long: "--paths",
        also: &[],
        arg: None,
        section: Section::Output,
        repeats: false,
        summary: "an entry for the start and each node below",
        detail: "An entry for the start and for each node below it, in document order, \
            in a listing: {file, format, path, entries, total, limit, truncated} (see \
            OUTPUT). --depth N goes N levels below the start, and --limit N lists N \
            entries at most.",
    },
    Opt {
        short: None,
        long: "--find",
        also: &[],
        arg: Some(("REGEX", Value::Text)),
        section: Section::Output,
        repeats: false,
        summary: "the entries whose \"key\": value text matches",
        detail: "The entries of the nodes, the start and those below it, whose \"key\": \
            value text matches REGEX, in a listing with pattern and matches (see \
            OUTPUT). It is the viewer's search: case is ignored unless REGEX has a \
            capital letter or ends in /s, and [ ] { } match themselves unless escaped. \
            --depth and --limit apply as they do to --paths.",
    },
    Opt {
        short: None,
        long: "--where",
        also: &[],
        arg: None,
        section: Section::Output,
        repeats: false,
        summary: "the start's entry: its path and position",
        detail: "The start's entry, with file and format: with --at, the path a source \
            position is in; with --path, the line and column a path starts at; with \
            neither, the root's.",
    },
    Opt {
        short: None,
        long: "--check",
        also: &[],
        arg: None,
        section: Section::Output,
        repeats: false,
        summary: "parse each FILE and report on each",
        detail: "Parse every FILE, or standard input when none is named, and report on \
            each: {ok, files: [{file, format, ok, error}]}, where a failing file's error \
            is the error object a run on that file alone prints. The one output option \
            that reads several inputs. Exit status 1 when any input fails, with the \
            report still on standard output. Takes no --path or --at.",
    },
    Opt {
        short: None,
        long: "--render",
        also: &[],
        arg: Some(("FORMAT", Value::Render)),
        section: Section::Output,
        repeats: false,
        summary: "the value written as FORMAT, streamed",
        detail: "The value at the start written as FORMAT, streamed as the input is \
            read: csv, its records (the elements of the array at the start, a value of \
            another kind its one record; the lines of JSON Lines; the records of CSV and \
            TSV) under a header row, every field quoted, CRLF line ends; json, the value \
            as --json writes it, but with each number spelled as in the source where \
            that is JSON; or any format whose crate carries a render. FORMAT is one of \
            {renders}. What FORMAT cannot hold is written by the convention its crate \
            declares rather than refused (NaN and the infinities as null in json; a \
            root TOML or INI cannot have as the one member --key names), and what it \
            does not keep is said in a warning on standard error, on success too. Takes \
            --path, not --at. With --alchemy it names the format the program's table or \
            JSON events are written as.",
    },
    Opt {
        short: None,
        long: "--alchemy",
        also: &[],
        arg: Some(("FILE", Value::File)),
        section: Section::Output,
        repeats: false,
        summary: "run the alchemy program in FILE",
        detail: "Run the alchemy program in FILE over the input, and stream what its \
            export answers: a text as it is, a table as CSV (--render json: JSON \
            records, an object per row), JSON events as compact JSON (--render csv: a \
            table of them); --render FORMAT writes a table or JSON events as FORMAT. \
            The program selects what it reads, so no --path, --at or other output \
            option goes with it. The input reaches it as --render reads it: JSON Lines, \
            CSV and TSV a record at a time.",
    },
    Opt {
        short: None,
        long: "--alchemy-expr",
        also: &[],
        arg: Some(("TEXT", Value::Text)),
        section: Section::Output,
        repeats: false,
        summary: "the same, the program on the command line",
        detail: "--alchemy, with the program's text on the command line: --alchemy-expr \
            'def export [input] input' writes the document as JSON.",
    },
    Opt {
        short: None,
        long: "--explain",
        also: &[],
        arg: None,
        section: Section::Output,
        repeats: false,
        summary: "the program's plan as JSON, and no run",
        detail: "With --alchemy or --alchemy-expr, print the program's plan report as \
            one JSON object instead of running it, and read no input: the chain of \
            calls, the protocols, what is retained and under which limits, the \
            ordering contract, the renderer (the program's own default, whatever \
            --render names), and the guarantee with its qualification. Run it first on \
            a program you did not write.",
    },
    Opt {
        short: None,
        long: "--path",
        also: &[],
        arg: Some(("PATH", Value::Text)),
        section: Section::Output,
        repeats: false,
        summary: "start at PATH, in jq syntax",
        detail: "Start at PATH instead of the root, in the jq syntax every output \
            prints (see PATHS). Not with --at, --check or a program.",
    },
    Opt {
        short: None,
        long: "--at",
        also: &[],
        arg: Some(("LINE[:COL]", Value::Text)),
        section: Section::Output,
        repeats: false,
        summary: "start at the node at a source position",
        detail: "Start at the node at that source position, counted from 1, columns in \
            characters (see POSITIONS). Not with --path, --check, --render or a \
            program.",
    },
    Opt {
        short: None,
        long: "--limit",
        also: &[],
        arg: Some(("N", Value::Text)),
        section: Section::Output,
        repeats: false,
        summary: "at most N entries (default 200; 0 for all)",
        detail: "--paths and --find list at most N entries (default 200; 0 for all); \
            total and truncated say what was left out.",
    },
    Opt {
        short: None,
        long: "--compact",
        also: &[],
        arg: None,
        section: Section::Output,
        repeats: false,
        summary: "JSON on one line, an error's too",
        detail: "JSON on one line: the answer of --json, --paths, --find, --where, \
            --check, --render json and --explain, and an error or a warning on standard \
            error. A program's own JSON is on one line whatever is given.",
    },
    Opt {
        short: None,
        long: "--max-output",
        also: &[],
        arg: Some(("SIZE", Value::Text)),
        section: Section::Output,
        repeats: false,
        summary: "cap a program's output (default 1G)",
        detail: "Stop an --alchemy program that writes more than SIZE (default 1G; 0 \
            for no limit): a transduce error, RESOURCE_LIMIT_EXCEEDED naming \
            max_output_bytes, status 5. SIZE is bytes, or a whole number with K, M or G \
            (1024s).",
    },
    Opt {
        short: Some('k'),
        long: "--kind",
        also: &["--format"],
        arg: Some(("FORMAT", Value::Format)),
        section: Section::Both,
        repeats: false,
        summary: "parse every input as FORMAT",
        detail: "Parse every input as FORMAT instead of by its extension: a format's \
            name ({formats}), one of its extensions (yml, md, ndjson), or a --grammar \
            NAME. Standard input is read as JSON unless this says otherwise. --format is \
            another name for this option.",
    },
    Opt {
        short: None,
        long: "--grammar",
        also: &[],
        arg: Some(("NAME=FILE", Value::File)),
        section: Section::Both,
        repeats: true,
        summary: "a format of your own, from an ABNF grammar",
        detail: "Read a file whose extension or whole name is NAME, in any case \
            (x.hosts, /etc/hosts), with the ABNF grammar in FILE, which is read within \
            --max-size and compiled within --timeout before any input is read. NAME is \
            then a format, as -k takes it and every output reports it; NAME,NAME2=FILE \
            gives it two names, and the option repeats. See CUSTOM GRAMMARS.",
    },
    Opt {
        short: None,
        long: "--grammar-expr",
        also: &[],
        arg: Some(("NAME=ABNF", Value::Text)),
        section: Section::Both,
        repeats: true,
        summary: "the same, the grammar on the command line",
        detail: "--grammar, with the grammar's text on the command line: everything \
            after the first =.",
    },
    Opt {
        short: None,
        long: "--depth",
        also: &[],
        arg: Some(("N", Value::Text)),
        section: Section::Both,
        repeats: false,
        summary: "list N levels down; the viewer folds deeper",
        detail: "--paths and --find go at most N levels below the start (default: \
            every level). The viewer folds the containers deeper than N levels when it \
            opens a file.",
    },
    Opt {
        short: None,
        long: "--indent",
        also: &[],
        arg: Some(("N", Value::Text)),
        section: Section::Both,
        repeats: false,
        summary: "indentation per level (default 2)",
        detail: "Indent --json and --render json N spaces a level, and the viewer's \
            tree (default 2; at most 16).",
    },
    Opt {
        short: None,
        long: "--key",
        also: &[],
        arg: Some(("NAME", Value::Text)),
        section: Section::Both,
        repeats: false,
        summary: "member a root is written under (items)",
        detail: "The member --render writes a value under when FORMAT's document must \
            be a table and the value is not an object (toml, ini): an array or a scalar \
            at the start is written as the one member NAME (default items). The \
            viewer's output pane takes it too.",
    },
    Opt {
        short: None,
        long: "--max-size",
        also: &[],
        arg: Some(("SIZE", Value::Text)),
        section: Section::Both,
        repeats: false,
        summary: "refuse an input over SIZE (default 64M)",
        detail: "Refuse an input larger than SIZE (default 64M; 0 for no limit), and a \
            --grammar or --alchemy file too, with a too_large error, status 5: a parse \
            takes about 40 bytes of memory per byte of input. SIZE is bytes, or a whole \
            number with K, M or G (1024s). JSON Lines, CSV and TSV that --render or \
            --alchemy streams are read a record at a time, and the limit does not apply \
            to them.",
    },
    Opt {
        short: None,
        long: "--timeout",
        also: &[],
        arg: Some(("SECONDS", Value::Text)),
        section: Section::Both,
        repeats: false,
        summary: "stop a parse past SECONDS (default none)",
        detail: "Stop a parse, or a --grammar compile, that runs longer than SECONDS \
            (2.5, 90s, 2m; default none; 0 for no limit), with a timeout error, status \
            6, that says how far it got. Under --render and --alchemy it covers the \
            whole run. Without a screen, on standard input, the time runs from the \
            start, so waiting on the input counts. A parse runs at about a megabyte a \
            second: a caller with a deadline of its own should pass one a few seconds \
            shorter.",
    },
    Opt {
        short: None,
        long: "--no-watch",
        also: &[],
        arg: None,
        section: Section::Viewer,
        repeats: false,
        summary: "do not reload files when they change",
        detail: "Do not reload a file when it changes.",
    },
    Opt {
        short: None,
        long: "--watch",
        also: &[],
        arg: None,
        section: Section::Viewer,
        repeats: false,
        summary: "reload files when they change (the default)",
        detail: "Reload a file when it changes, keeping your place (the default).",
    },
    Opt {
        short: Some('m'),
        long: "--mode",
        also: &[],
        arg: Some(("MODE", Value::Words(MODES))),
        section: Section::Viewer,
        repeats: false,
        summary: "start in data (default) or line mode",
        detail: "Start in data mode (the default), the streamlined tree, or in line \
            mode, every line of a pretty-printed rendering; m switches between them.",
    },
    Opt {
        short: Some('n'),
        long: "--line-numbers",
        also: &[],
        arg: None,
        section: Section::Viewer,
        repeats: false,
        summary: "show absolute line numbers",
        detail: "Show absolute line numbers.",
    },
    Opt {
        short: Some('N'),
        long: "--no-line-numbers",
        also: &[],
        arg: None,
        section: Section::Viewer,
        repeats: false,
        summary: "hide absolute line numbers",
        detail: "Hide absolute line numbers.",
    },
    Opt {
        short: Some('r'),
        long: "--relative-line-numbers",
        also: &[],
        arg: None,
        section: Section::Viewer,
        repeats: false,
        summary: "show relative line numbers",
        detail: "Show line numbers relative to the focused row.",
    },
    Opt {
        short: Some('R'),
        long: "--no-relative-line-numbers",
        also: &[],
        arg: None,
        section: Section::Viewer,
        repeats: false,
        summary: "hide relative line numbers",
        detail: "Hide relative line numbers.",
    },
    Opt {
        short: None,
        long: "--scrolloff",
        also: &[],
        arg: Some(("N", Value::Text)),
        section: Section::Viewer,
        repeats: false,
        summary: "rows kept around the focus (default 3)",
        detail: "Keep N rows around the focus when scrolling (default 3).",
    },
    Opt {
        short: None,
        long: "--hidden",
        also: &[],
        arg: None,
        section: Section::Viewer,
        repeats: false,
        summary: "show dot-files in the explorer",
        detail: "Show dot-files in the file explorer.",
    },
    Opt {
        short: None,
        long: "--ascii",
        also: &[],
        arg: None,
        section: Section::Viewer,
        repeats: false,
        summary: "draw fold markers in ASCII",
        detail: "Draw fold markers with ASCII characters (v and >).",
    },
    Opt {
        short: None,
        long: "--no-color",
        also: &["--no-colour"],
        arg: None,
        section: Section::Viewer,
        repeats: false,
        summary: "no colours (as NO_COLOR does)",
        detail: "Draw without colours, as when NO_COLOR is set.",
    },
    Opt {
        short: None,
        long: "--no-mouse",
        also: &[],
        arg: None,
        section: Section::Viewer,
        repeats: false,
        summary: "do not capture the mouse",
        detail: "Leave the mouse to the terminal rather than capture it.",
    },
    Opt {
        short: None,
        long: "--panes",
        also: &[],
        arg: Some(("PANES", Value::Words(PANES))),
        section: Section::Viewer,
        repeats: false,
        summary: "open the output and program panes",
        detail: "Open panes beside the input: out, the document as --render or \
            --alchemy writes it (JSON when neither is given), and program, the program; \
            out,program opens both. With --panes, --render and --alchemy choose the \
            output pane's content instead of printing, the other output options are \
            refused, and so is a run without a terminal (status 2). C-w moves between \
            the panes, s shows a pane's text or its tree, and :pane out|program|close \
            opens or closes one.",
    },
    Opt {
        short: None,
        long: "--stacked",
        also: &[],
        arg: None,
        section: Section::Viewer,
        repeats: false,
        summary: "stack the panes",
        detail: "Stack the panes rather than place them side by side.",
    },
    Opt {
        short: Some('h'),
        long: "--help",
        also: &[],
        arg: None,
        section: Section::About,
        repeats: false,
        summary: "the options (-h), or the whole reference",
        detail: "Print a summary of the options (-h), or this reference (--help).",
    },
    Opt {
        short: Some('V'),
        long: "--version",
        also: &[],
        arg: None,
        section: Section::About,
        repeats: false,
        summary: "the version",
        detail: "Print the version: aless {version}.",
    },
    Opt {
        short: None,
        long: "--generate",
        also: &[],
        arg: Some(("WHAT", Value::Words(GENERATE))),
        section: Section::About,
        repeats: false,
        summary: "a man page, shell completions or the skill",
        detail: "Print a file made from this reference: man, the man page (aless.1); \
            complete-bash, complete-zsh, complete-fish or complete-powershell, that \
            shell's completions; or skill, the Agent Skill (SKILL.md) that teaches an \
            agent to drive aless. See FILES.",
    },
];

/// The option `name` names: a long name (`--kind`), another name for it
/// (`--format`) or its letter (`-k`).
pub fn lookup(name: &str) -> Option<&'static Opt> {
    let letter = match name.strip_prefix('-').map(|n| {
        let mut chars = n.chars();
        (chars.next(), chars.next())
    }) {
        Some((Some(c), None)) if c != '-' => Some(c),
        _ => None,
    };
    OPTIONS.iter().find(|o| {
        o.long == name || o.also.contains(&name) || (letter.is_some() && o.short == letter)
    })
}

/// Whether giving `name` asks for output rather than the viewer.
pub fn asks_for_output(name: &str) -> bool {
    lookup(name).is_some_and(|o| o.section == Section::Output)
}

/// A piece of a topic.
#[derive(Debug)]
enum Block {
    /// A paragraph, filled to the width.
    Text(&'static str),
    /// Lines kept as they are: commands, samples.
    Code(&'static str),
    /// Terms, and what each means.
    Terms(&'static [(&'static str, &'static str)]),
    /// The options of a section, from [`OPTIONS`].
    Options(Section),
    /// The formats and their extensions, from [`Format`].
    Formats,
}

/// Where a topic goes in the man page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Man {
    Synopsis,
    /// Under DESCRIPTION, as a subsection with this title.
    Description(&'static str),
    /// Under OPTIONS, as a subsection with this title.
    Options(&'static str),
    /// A section of its own, as the help titles it.
    Section,
}

/// A part of the reference.
#[derive(Debug)]
struct Topic {
    title: &'static str,
    /// A note after the title, in the help.
    note: &'static str,
    man: Man,
    blocks: &'static [Block],
}

/// The reference, in the order `--help` prints it.
const TOPICS: &[Topic] = &[
    Topic {
        title: "USAGE",
        note: "",
        man: Man::Synopsis,
        blocks: &[
            Block::Terms(&[
                ("aless [OPTIONS] [FILE]...", "the viewer, in a terminal"),
                (
                    "aless --paths --depth 1 FILE",
                    "JSON on standard output, anywhere",
                ),
                (
                    "<command> | aless [OPTIONS]",
                    "standard input, read as JSON unless -k says",
                ),
            ]),
            Block::Text("-h prints a summary of the options, and --help this whole reference."),
        ],
    },
    Topic {
        title: "WITHOUT A SCREEN",
        note: "scripts, agents, pipes",
        man: Man::Description("Without a screen"),
        blocks: &[
            Block::Text(
                "aless prints instead of starting the viewer when an option listed under \
                OUTPUT OPTIONS is given, when standard output is not a terminal (a pipe, a \
                file, an agent's tool call), or when TERM=dumb, and it never waits for \
                keys. A run reads one input (--check reads several), prints one answer on \
                standard output and exits 0, or prints {\"error\": {...}} on standard error \
                and exits non-zero: see OUTPUT, ERRORS and EXIT STATUS. When the viewer \
                cannot start for want of a terminal, aless says so at once, before reading \
                any input, with a usage error and status 2.",
            ),
            Block::Text("For an agent, five rules:"),
            Block::Terms(&[
                (
                    "1.",
                    "Pass an output option: --json, --paths, --find, --where, --check, \
                    --render or --alchemy. Never --panes, which opens the viewer.",
                ),
                (
                    "2.",
                    "Name the file as an argument. Standard input is read as JSON unless \
                    -k FORMAT says otherwise.",
                ),
                (
                    "3.",
                    "Check the exit status before reading standard output: only 0 means \
                    it holds the answer (--check's report comes with 1 too).",
                ),
                ("4.", "Quote a path for the shell: --path '.items[0]'."),
                (
                    "5.",
                    "Start small on a big or unknown file: --paths --depth 1, then --path \
                    into it.",
                ),
            ]),
            Block::Text(
                "aless --generate skill prints an Agent Skill that teaches an agent all of \
                this.",
            ),
        ],
    },
    Topic {
        title: "OUTPUT OPTIONS",
        note: "any of them prints instead of starting the viewer",
        man: Man::Options("Output options"),
        blocks: &[Block::Options(Section::Output)],
    },
    Topic {
        title: "OPTIONS FOR BOTH",
        note: "the output and the viewer",
        man: Man::Options("Options for both the output and the viewer"),
        blocks: &[Block::Options(Section::Both)],
    },
    Topic {
        title: "VIEWER OPTIONS",
        note: "ignored without a screen, where --panes is refused",
        man: Man::Options("Viewer options"),
        blocks: &[Block::Options(Section::Viewer)],
    },
    Topic {
        title: "OTHER OPTIONS",
        note: "",
        man: Man::Options("Other options"),
        blocks: &[Block::Options(Section::About)],
    },
    Topic {
        title: "INPUT AND FORMATS",
        note: "",
        man: Man::Section,
        blocks: &[
            Block::Text(
                "FILE is a path, or - for standard input. Without a FILE, aless reads \
                standard input when it is not a terminal; otherwise output fails with a \
                usage error, and the viewer explores the current directory. Every output \
                option but --check reads one input: run aless once per file, or --check \
                several. A directory is a usage error without a screen, and opens the file \
                explorer in the viewer. Options end at --, after which every argument is a \
                FILE, and a long option's value may follow it or be attached to it \
                (--kind=yaml).",
            ),
            Block::Text(
                "The format comes from a file's extension, or from its whole name for a \
                --grammar NAME (/etc/hosts), and -k FORMAT overrides it for every input. A \
                file whose extension no format claims is read as plain text, an array of \
                its lines. Map keys keep their source order. The formats, with the \
                extensions that imply each:",
            ),
            Block::Formats,
        ],
    },
    Topic {
        title: "PATHS",
        note: "",
        man: Man::Section,
        blocks: &[Block::Text(
            "--path takes jq's syntax, which every output prints, so a path can go \
            straight back in: ., .a.b[0], .\"odd key\", .[\"a.b\"], .[0] for a root array's \
            first item and .[-1] for its last. Also accepted: a.b[0] and [0] without the \
            leading dot, JSONPath's $.a['b'][0], and JSON Pointer's /a/b/0. There are no \
            wildcards, slices or recursive descent: pipe --json into jq for those. Quote a \
            path for the shell, whose globbing would take [0]. Under --render, [-1] on an \
            array is a usage error, since a stream cannot count from the end; on an \
            object it is the key -1, as everywhere.",
        )],
    },
    Topic {
        title: "POSITIONS",
        note: "",
        man: Man::Section,
        blocks: &[Block::Text(
            "--at takes LINE or LINE:COL, counted from 1, columns in characters, as an \
            entry's line and col are. Inside the text, --at answers the node starting \
            last at or before the position on its line, the innermost of several starting \
            there (a line alone, or a column before the line's first node, that first \
            node; on a line with no node of its own, a comment or a closing bracket, the \
            last node before the line, or the document's first node when none is). \
            Outside the text it names nothing, status 4: a line past the last line, or a \
            column past the end of its line, where a line's text excludes its terminator \
            (LF or CRLF), a trailing terminator starts no line, and an empty line has no \
            column inside it.",
        )],
    },
    Topic {
        title: "OUTPUT",
        note: "",
        man: Man::Section,
        blocks: &[
            Block::Text("An entry describes one node:"),
            Block::Code(r#"{"path":".spec.replicas","kind":"number","line":12,"col":3,"value":3}"#),
            Block::Terms(&[
                (
                    "path",
                    "where it is, in jq syntax: give it back to --path, or to jq, \
                    unchanged",
                ),
                ("kind", "object, array, string, number, boolean or null"),
                (
                    "line, col",
                    "where the node starts, from 1, columns in characters: at its key \
                    when it has one, else at its value. Exact for the JSON family, TOML, \
                    INI, CSV and ZON, best-effort for YAML, XML and Markdown, and null \
                    when unknown",
                ),
                (
                    "length",
                    "a container's item count, or the full length of a string cut short",
                ),
                (
                    "value",
                    "a scalar's value. A string over 200 characters is cut to 200, with \
                    \"truncated\": true; NaN and the infinities are \"NaN\", \"Infinity\" \
                    and \"-Infinity\"",
                ),
            ]),
            Block::Text("What each option prints on standard output when it succeeds:"),
            Block::Terms(&[
                (
                    "--json",
                    "the value itself, indented 2 spaces a level (--indent N), or on one \
                    line (--compact)",
                ),
                (
                    "--paths",
                    "{file, format, path, entries: [entry, ...], total, limit, \
                    truncated}",
                ),
                (
                    "--find",
                    "{file, format, path, pattern, matches: [entry, ...], total, limit, \
                    truncated}",
                ),
                (
                    "--where",
                    "{file, format, ...entry}: the entry, with file and format first",
                ),
                (
                    "--check",
                    "{ok, files: [{file, format, ok, error}]}, error null for a file that \
                    parses",
                ),
                (
                    "--render csv",
                    "CSV: a header row, then a record per row, every field quoted, CRLF \
                    line ends",
                ),
                (
                    "--render json",
                    "the value, indented as --json is, each number as the source spelled \
                    it where that is JSON",
                ),
                (
                    "--render FORMAT",
                    "the document in FORMAT, in its always-quoted profile, so that \
                    nothing reads back as another kind",
                ),
                (
                    "--alchemy",
                    "what the program exports: a text as it is, a table as CSV, JSON \
                    events as compact JSON",
                ),
                (
                    "--explain",
                    "{entry, output, protocol, chain, retention, renderer, guarantee, \
                    qualification, ...}",
                ),
            ]),
            Block::Text(
                "file is the path as given, or - for standard input; format is what the \
                input was read as; path is where the listing starts. total counts every \
                entry the listing found, and truncated is true when that is more than \
                --limit let through: raise --limit (0 for all), or narrow with --path or \
                --depth.",
            ),
            Block::Text(
                "A --render that succeeds also writes a warning on standard error that says \
                what the format does not keep: {\"warning\": {\"kind\": \"loss\", \
                \"message\", \"file\", \"render\", \"loss\": [sentence, ...]}}, with \
                adapters naming each step that ran between the source and the render \
                (wrap-object, wrap-array, embed, the inferred table, records), and adapter \
                the one between a tree and a table, the inferred table or records, their \
                sentences in loss after the format's own. Status 0 is success whatever \
                standard error holds.",
            ),
        ],
    },
    Topic {
        title: "ERRORS",
        note: "",
        man: Man::Section,
        blocks: &[
            Block::Text(
                "An error is one JSON object on standard error, and standard output is \
                then empty, with two exceptions: a failed --check still prints its report, \
                and a --render or --alchemy stream that fails leaves what it had written, \
                every record whole and none in part (but for a record over 16 MB, and \
                another format's render, such as --render yaml, which can stop inside \
                one). A parse error:",
            ),
            Block::Code(
                r#"{"error": {"kind": "parse", "file": "bad.json", "format": "json",
  "code": "unexpected", "message": "unexpected end of input",
  "line": 2, "col": 1, "hint": "The document ends before it is ...",
  "source_line": "", "report": "[tabnas/unexpected]: unexpected ..."}}"#,
            ),
            Block::Text(
                "kind says what failed, and which fields come with it. A parse error's \
                fields are always there, null where they do not apply; another kind's \
                fields named \"when\" are there only then:",
            ),
            Block::Terms(&[
                (
                    "parse",
                    "the input did not parse: file, format, code (the grammar's: \
                    unexpected, unterminated_string, too_deep, ...), message, line, col, \
                    hint, source_line (the line the error is on) and report (the whole \
                    report the viewer shows, uncoloured)",
                ),
                (
                    "io",
                    "an input, a --grammar file or a program file could not be read, or \
                    the output not written: a parse error's fields, with the code io \
                    (file null when it was standard output)",
                ),
                (
                    "too_large",
                    "over --max-size: an io error's fields, plus size (null for standard \
                    input, which is read no further) and limit, in bytes; the hint names \
                    the --max-size that would read it",
                ),
                (
                    "timeout",
                    "past --timeout: a parse error's fields, line and col showing how far \
                    the parse got (null when the input was still being read, or the parse \
                    finished late; a read a record at a time names the line its record \
                    starts on, col null), plus seconds",
                ),
                (
                    "not_found",
                    "--path or --at names nothing: file, format, message, the path or at \
                    given, nearest (the entry of the deepest node the path reached, or of \
                    the node a position inside the text would have answered) and keys \
                    (that node's first keys when it is an object)",
                ),
                (
                    "usage",
                    "the command is wrong: message, and nothing more, but that a \
                    --grammar that does not compile adds grammar, and file when it came \
                    from one",
                ),
                (
                    "transduce",
                    "a --render or --alchemy stream failed: code (INPUT_INVALID, \
                    RESOURCE_LIMIT_EXCEEDED, OUTPUT_FAILED, DUPLICATE_MEMBER, \
                    TARGET_VALUE_UNREPRESENTABLE, ...), message, file, format, then path, \
                    limit ({name, value}), line, col and hint when the failure has them, \
                    and output: \"partial\" when some of the result had been written, else \
                    \"none\". One a program raised with a position of its own has file, \
                    line and col the program's, format null, and input, the document's \
                    name",
                ),
                (
                    "alchemy",
                    "the program is wrong: code (DSL_PARSE_ERROR, DSL_TYPE_ERROR, \
                    STREAM_REUSED, STREAMABILITY_UNKNOWN), message led by a finer code \
                    (unbalanced, unknown_name, arity, ...), file (the program's path, or \
                    --alchemy-expr), format null, line and col in the program, output, \
                    and input (the document's name) once the input was open",
                ),
            ]),
            Block::Text(
                "An io, too_large or timeout error about a --grammar file adds grammar, \
                with file the grammar file and format null. An error met while --render \
                was writing (transduce, parse, timeout, or the render's own alchemy one) \
                adds loss, the sentences its warning gives on success. --compact puts an \
                error on one line. The README's \"Scripts and \
                agents\" section has every case at length.",
            ),
        ],
    },
    Topic {
        title: "EXIT STATUS",
        note: "",
        man: Man::Section,
        blocks: &[Block::Terms(&[
            ("0", "success: standard output holds the answer"),
            (
                "1",
                "the input did not parse (parse); with --check, an input failed, and the \
                report says which; with --render or --alchemy, the input or its records \
                will not do (transduce: INPUT_INVALID, and the other input, protocol and \
                target codes)",
            ),
            (
                "2",
                "bad usage (usage): an unknown option, a bad path, no input, a directory, \
                a --grammar or an --alchemy program that does not compile (alchemy), or \
                the viewer without a terminal",
            ),
            (
                "3",
                "an input, a --grammar file or an --alchemy program file could not be \
                read, or the output not written (io; transduce OUTPUT_FAILED)",
            ),
            ("4", "--path or --at names nothing (not_found)"),
            (
                "5",
                "an input, a --grammar file or an --alchemy program file is over \
                --max-size (too_large); with --render or --alchemy, over a limit of the \
                transducer's, a program's output over --max-output among them (transduce \
                RESOURCE_LIMIT_EXCEEDED)",
            ),
            (
                "6",
                "a parse, or a --grammar compile, ran past --timeout, or the input was \
                still being read when it passed; with --render or --alchemy, the whole \
                run, the program's work included (timeout)",
            ),
        ])],
    },
    Topic {
        title: "LARGE INPUTS AND STREAMING",
        note: "",
        man: Man::Section,
        blocks: &[
            Block::Text(
                "An input is read whole and parsed whole before anything is printed, at \
                about a megabyte a second and 40 bytes of memory per byte of input. \
                --max-size (default 64M) refuses a larger input before it is read, and \
                --timeout stops a parse that runs too long with an error that says how far \
                it got: a caller with a deadline of its own should pass a --timeout a few \
                seconds shorter. --path and --depth shrink the output, not the parse. A \
                reader that stops early (aless --json big.json | head) ends aless quietly, \
                with status 0.",
            ),
            Block::Text(
                "--render and --alchemy stream instead. JSON Lines, CSV and TSV are read a \
                record at a time, whatever their size, so --max-size does not apply to \
                them (a record over 64 MB fails); the JSON family, jsonic, YAML, ZON and \
                Markdown write their records as the parse proceeds, and the other formats \
                after it. A document a grammar refuses to stream part-way is read whole \
                and written all the same when nothing has been written yet; otherwise the \
                error says output \"partial\".",
            ),
            Block::Text(
                "A document nested deeper than aless reads fails as a parse error with the \
                code too_deep: past about 1,000 levels, or sooner where the grammar has a \
                limit of its own (127 levels for JSON, JSONL, JSONic, JSON5, YAML, TOML, \
                INI and ZON, 256 for XML, 512 for JSONC).",
            ),
            Block::Text(
                "Numbers are 64-bit floats: --json and entries give an integer beyond 2^53 \
                as the nearest one, and --json writes NaN and the infinities as null. \
                --render json keeps a number as the source spelled it where that is JSON, \
                and writes NaN and the infinities as null too, the loss JSON declares; \
                --render csv writes them as their names.",
            ),
        ],
    },
    Topic {
        title: "CUSTOM GRAMMARS",
        note: "",
        man: Man::Section,
        blocks: &[
            Block::Text(
                "--grammar NAME=FILE reads the files whose extension or whole name is \
                NAME, in any case, with the ABNF grammar in FILE (RFC 5234, compiled by \
                tabnas/abnf), and -k NAME reads any input with it. ; @object a b and ; \
                @array comments on a rule say what it builds; without them the value is \
                the compiler's parse tree, a {\"rule\", \"src\", \"kids\"} node per rule. The \
                input is read as plain text: { } [ ] : , are ordinary characters, # starts \
                a comment to the end of the line, and a word is the token TX (NR, ST and \
                VL bring numbers, quoted strings and true, false and null back). A \
                grammar that does not compile is a usage error before any input is read, \
                and so is a repetition count over 1,024, or repetitions that would have \
                the compiler write more than 1,024 rules. An input the grammar does not \
                accept is a parse error, with format the grammar's name.",
            ),
            Block::Text(
                "Grammars for /etc/hosts, crontabs, /etc/passwd, /etc/group, /etc/fstab, \
                /etc/resolv.conf and KEY=value files, each with a sample and a guide to \
                writing one, are in aless's source:",
            ),
            Block::Code("https://github.com/rjrodger/aless/tree/main/tests/fixtures/grammars"),
        ],
    },
    Topic {
        title: "THE VIEWER",
        note: "",
        man: Man::Section,
        blocks: &[
            Block::Text(
                "In a terminal each FILE opens in a tab of its own, watched and reloaded \
                when it changes, keeping your place. A directory opens in the file \
                explorer, where Enter opens a file. Without a FILE the viewer reads \
                standard input when it is not a terminal, and otherwise explores the \
                current directory. The keys are jless's, and F1 or :help inside aless \
                lists them all; the ones used most:",
            ),
            Block::Terms(&[
                ("j k", "down / up; a count before a key repeats it (3j)"),
                (
                    "h l",
                    "collapse / expand, or go to the parent / first child",
                ),
                ("J K", "next / previous sibling"),
                ("g G", "first / last row; Ng goes to row N"),
                ("C-d C-u", "half a page down / up"),
                ("Space", "toggle the focused container"),
                ("c e", "collapse / expand the focused node and its siblings"),
                (
                    "/ ?",
                    "search forward / backward; n N for the next / previous match",
                ),
                ("yy yp yq", "copy the value / its path / its jq path"),
                ("m", "switch between data mode and line mode"),
                ("s", "show the source text at the focused node"),
                ("Tab", "the next tab (Shift-Tab the previous)"),
                (":", "a command: :open PATH, :format FORMAT, :w FILE, :help"),
                ("q", "close the tab, quitting with the last; C-c quits"),
            ]),
        ],
    },
    Topic {
        title: "ENVIRONMENT",
        note: "",
        man: Man::Section,
        blocks: &[Block::Terms(&[
            ("NO_COLOR", "set and not empty: no colours, as --no-color"),
            (
                "TERM",
                "dumb: print instead of starting the viewer, as when standard output is \
                not a terminal, since such a terminal cannot draw it",
            ),
        ])],
    },
    Topic {
        title: "FILES",
        note: "",
        man: Man::Section,
        blocks: &[
            Block::Text(
                "A release archive carries the man page and the completions, which \
                --generate also prints:",
            ),
            Block::Terms(&[
                ("man/aless.1", "the man page (--generate man)"),
                ("completions/aless.bash", "bash (--generate complete-bash)"),
                ("completions/_aless", "zsh (--generate complete-zsh)"),
                ("completions/aless.fish", "fish (--generate complete-fish)"),
                (
                    "completions/_aless.ps1",
                    "PowerShell (--generate complete-powershell)",
                ),
            ]),
            Block::Text(
                "A shell reads its completions from where it looks for them (a \
                package puts them there), or from its startup file each time it \
                starts:",
            ),
            Block::Code(
                "eval \"$(aless --generate complete-bash)\"     # ~/.bashrc\n\
                 eval \"$(aless --generate complete-zsh)\"      # ~/.zshrc, after compinit\n\
                 aless --generate complete-fish | source      # config.fish",
            ),
            Block::Text(
                "The Agent Skill (--generate skill) is a file an agent loads from a \
                directory of its own. For Claude Code:",
            ),
            Block::Code(
                "mkdir -p ~/.claude/skills/aless\n\
                 aless --generate skill > ~/.claude/skills/aless/SKILL.md",
            ),
        ],
    },
    Topic {
        title: "EXAMPLES",
        note: "",
        man: Man::Section,
        blocks: &[Block::Code(
            "aless --paths --depth 1 config.yaml       what is in it\n\
             aless --json --path '.spec.containers[0]' deploy.yaml\n\
             aless --where --at 42:7 deploy.yaml       the path at a linter's 42:7\n\
             aless --where --path .a.b config.yaml     the line a path is on\n\
             aless --find '\"image\":' deploy.yaml       every image, with its line\n\
             aless --check $(git ls-files '*.toml')    do they all parse?\n\
             aless -k csv --json < data.csv            stdin is JSON unless -k says\n\
             aless --render csv --path .items x.json   the records as CSV\n\
             aless --render json big.yaml              the document as JSON\n\
             aless --render yaml data.csv              any format as any other\n\
             aless --alchemy export.alc api.json       a program's output\n\
             aless --alchemy export.alc --explain      what the program will do\n\
             aless --grammar hosts=hosts.abnf --json /etc/hosts",
        )],
    },
    Topic {
        title: "SEE ALSO",
        note: "",
        man: Man::Section,
        blocks: &[Block::Terms(&[
            ("jq(1), jless(1)", "where the paths and the keys come from"),
            (
                "the documentation",
                concat!(
                    "tutorials, how-to guides, this reference with every key of the \
                    viewer, and how aless works: ",
                    env!("CARGO_PKG_HOMEPAGE")
                ),
            ),
            (
                "the source",
                concat!(
                    "the code, its README and the releases: ",
                    env!("CARGO_PKG_REPOSITORY")
                ),
            ),
            (
                "alchemy",
                "the language --alchemy runs, in its docs/language.md: \
                https://github.com/tabnas/alchemy",
            ),
            (
                "tabnas/abnf",
                "the ABNF compiler --grammar uses, and its annotations: \
                https://github.com/tabnas/abnf",
            ),
        ])],
    },
];

/// The width the help is written to.
const WIDTH: usize = 80;

/// The words of `text` in lines of at most `width` characters; a word
/// longer than that has a line of its own.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let len = line.chars().count();
        if len > 0 && len + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// `text` filled to the width, every line indented by `indent`.
fn push_filled(out: &mut String, text: &str, indent: usize) {
    for line in wrap(text, WIDTH - indent) {
        out.push_str(&" ".repeat(indent));
        out.push_str(&line);
        out.push('\n');
    }
}

/// The names of the formats aless reads, as a list: `json, jsonl, ...
/// or text`.
fn format_names() -> String {
    let names: Vec<&str> = Format::ALL.iter().map(|f| f.name()).collect();
    match names.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} or {last}", rest.join(", ")),
        _ => names.join(""),
    }
}

/// `text` with its `{renders}`, `{formats}` and `{version}` filled in.
fn fill(text: &str) -> String {
    let mut text = text.to_string();
    if text.contains("{renders}") {
        text = text.replace("{renders}", &crate::translate::names());
    }
    if text.contains("{formats}") {
        text = text.replace("{formats}", &format_names());
    }
    text.replace("{version}", VERSION)
}

/// What the table of formats says beside a format's extensions.
fn format_note(format: Format) -> &'static str {
    match format {
        Format::Jsonc => " (trailing commas accepted)",
        Format::Csv => " (records keyed by the header row)",
        Format::Tsv => " (as csv)",
        Format::Markdown => " (its syntax tree)",
        Format::Feed => " (normalised to an Atom shape)",
        Format::Text => " (and every extension no format claims)",
        _ => "",
    }
}

/// The formats and their extensions, as terms.
fn format_terms() -> Vec<(String, String)> {
    Format::ALL
        .iter()
        .map(|f| {
            (
                f.name().to_string(),
                format!("{}{}", f.extensions().join(" "), format_note(*f)),
            )
        })
        .collect()
}

/// Terms in two columns: the term, then what it means, filled beside it,
/// or below it when the term is wider than the column.
fn push_terms(out: &mut String, terms: &[(String, String)], indent: usize) {
    const MAX_TERM: usize = 30;
    let width = terms
        .iter()
        .map(|(t, _)| t.chars().count())
        .filter(|&w| w <= MAX_TERM)
        .max()
        .unwrap_or(0);
    let hang = indent + width + 2;
    for (term, meaning) in terms {
        let lines = wrap(meaning, WIDTH - hang);
        let len = term.chars().count();
        let mut rest = lines.iter();
        out.push_str(&" ".repeat(indent));
        out.push_str(term);
        if len <= width {
            if let Some(first) = rest.next() {
                out.push_str(&" ".repeat(width - len + 2));
                out.push_str(first);
            }
        }
        out.push('\n');
        for line in rest {
            out.push_str(&" ".repeat(hang));
            out.push_str(line);
            out.push('\n');
        }
    }
}

fn static_terms(terms: &[(&str, &str)]) -> Vec<(String, String)> {
    terms.iter().map(|(t, m)| (fill(t), fill(m))).collect()
}

fn options_in(section: Section) -> impl Iterator<Item = &'static Opt> {
    OPTIONS.iter().filter(move |o| o.section == section)
}

/// What `--help` prints: the whole reference.
pub fn reference() -> String {
    let mut out = String::new();
    push_filled(&mut out, &format!("aless {VERSION} — {TAGLINE}"), 0);
    for topic in TOPICS {
        out.push('\n');
        match topic.note {
            "" => out.push_str(&format!("{}:\n", topic.title)),
            note => out.push_str(&format!("{} ({note}):\n", topic.title)),
        }
        for (i, block) in topic.blocks.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            match block {
                Block::Text(text) => push_filled(&mut out, &fill(text), 4),
                Block::Code(code) => {
                    let indent = if i == 0 { "    " } else { "      " };
                    for line in fill(code).lines() {
                        out.push_str(indent);
                        out.push_str(line);
                        out.push('\n');
                    }
                }
                Block::Terms(terms) => push_terms(&mut out, &static_terms(terms), 4),
                Block::Formats => push_terms(&mut out, &format_terms(), 6),
                Block::Options(section) => {
                    for (j, opt) in options_in(*section).enumerate() {
                        if j > 0 {
                            out.push('\n');
                        }
                        out.push_str("    ");
                        out.push_str(&opt.head());
                        out.push('\n');
                        push_filled(&mut out, &fill(opt.detail), 8);
                    }
                }
            }
        }
    }
    out
}

/// What `-h` prints: the options, a line each.
pub fn summary() -> String {
    const NAMES: usize = WIDTH - 4 - 2 - SUMMARY_WIDTH;
    let mut out = String::new();
    push_filled(&mut out, &format!("aless {VERSION} — {TAGLINE}"), 0);
    out.push_str("\nUSAGE:\n");
    if let Some(Block::Terms(terms)) = TOPICS[0].blocks.first() {
        push_terms(&mut out, &static_terms(terms), 4);
    }
    for topic in TOPICS {
        let Some(Block::Options(section)) = topic.blocks.first() else {
            continue;
        };
        out.push('\n');
        match topic.note {
            "" => out.push_str(&format!("{}:\n", topic.title)),
            note => out.push_str(&format!("{} ({note}):\n", topic.title)),
        }
        for opt in options_in(*section) {
            let head = match opt.short {
                Some(_) => opt.head(),
                None => format!("    {}", opt.head()),
            };
            let len = head.chars().count();
            out.push_str("    ");
            out.push_str(&head);
            if len > NAMES {
                out.push('\n');
                out.push_str(&" ".repeat(4 + NAMES + 2));
            } else {
                out.push_str(&" ".repeat(NAMES - len + 2));
            }
            out.push_str(opt.summary);
            out.push('\n');
        }
    }
    out.push('\n');
    push_filled(
        &mut out,
        "aless --help prints the whole reference: what each option prints, the error \
        objects and exit statuses, paths, positions, formats, limits and the viewer's \
        keys.",
        0,
    );
    out
}

/// The reference `--help` prints, as HTML, for the documentation site:
/// a section for each topic, in the same order, titled in sentence case
/// with an `id` a link can name (`exit-status`), and each option with an
/// `id` of its own (`opt-render` for `--render`), which a mention of the
/// option elsewhere in the reference links to.
pub fn reference_html() -> String {
    let mut out = String::new();
    for topic in TOPICS {
        out.push_str(&format!(
            "<h2 id=\"{}\">{}</h2>\n",
            title_id(topic.title),
            html_escape(&sentence_case(topic.title))
        ));
        if !topic.note.is_empty() {
            out.push_str(&format!(
                "<p class=\"note\">{}</p>\n",
                html_text(&sentence_case(topic.note))
            ));
        }
        for block in topic.blocks {
            match block {
                Block::Text(text) => {
                    out.push_str(&format!("<p>{}</p>\n", html_text(&fill(text))));
                }
                Block::Code(code) => out.push_str(&format!(
                    "<pre><code>{}</code></pre>\n",
                    html_escape(&fill(code))
                )),
                Block::Terms(terms) => push_html_terms(&mut out, &static_terms(terms)),
                Block::Formats => {
                    out.push_str("<table>\n<thead><tr><th>Format</th><th>Extensions</th></tr></thead>\n<tbody>\n");
                    for (name, extensions) in format_terms() {
                        out.push_str(&format!(
                            "<tr><td><code>{}</code></td><td>{}</td></tr>\n",
                            html_escape(&name),
                            html_escape(&extensions)
                        ));
                    }
                    out.push_str("</tbody>\n</table>\n");
                }
                Block::Options(section) => {
                    out.push_str("<dl class=\"options\">\n");
                    for opt in options_in(*section) {
                        out.push_str(&format!(
                            "<dt id=\"{}\"><code>{}</code></dt>\n<dd><p>{}</p></dd>\n",
                            option_id(opt),
                            html_escape(&opt.head()),
                            html_text(&fill(opt.detail))
                        ));
                    }
                    out.push_str("</dl>\n");
                }
            }
        }
    }
    out
}

/// The `id` of an option's entry in [`reference_html`]: `opt-render`.
pub fn option_id(opt: &Opt) -> String {
    format!("opt-{}", opt.long.trim_start_matches('-'))
}

/// The `id` of a topic's section in [`reference_html`]: `exit-status`.
fn title_id(title: &str) -> String {
    title.to_lowercase().replace(' ', "-")
}

/// `EXIT STATUS` as a heading is written: `Exit status`.
fn sentence_case(text: &str) -> String {
    let lower = text.to_lowercase();
    let mut chars = lower.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Terms as HTML: numbered rules as a list, the rest as a definition list.
fn push_html_terms(out: &mut String, terms: &[(String, String)]) {
    let numbered = terms.iter().all(|(term, _)| {
        term.strip_suffix('.')
            .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
    });
    if numbered {
        out.push_str("<ol>\n");
        for (_, meaning) in terms {
            out.push_str(&format!("<li>{}</li>\n", html_text(meaning)));
        }
        out.push_str("</ol>\n");
        return;
    }
    out.push_str("<dl>\n");
    for (term, meaning) in terms {
        out.push_str(&format!(
            "<dt><code>{}</code></dt>\n<dd>{}</dd>\n",
            html_escape(term),
            html_text(meaning)
        ));
    }
    out.push_str("</dl>\n");
}

/// `text` with `& < > "` escaped, for HTML.
fn html_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}

/// Prose as HTML: escaped, with each web address a link, each option
/// named in it set as code and linked to its entry, and each topic named
/// by its title (`see EXIT STATUS`) linked to its section.
fn html_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + text.len() / 4);
    let mut rest = text;
    while let Some((at, title)) = next_title(rest) {
        out.push_str(&html_words(&rest[..at]));
        out.push_str(&format!(
            "<a href=\"#{}\">{}</a>",
            title_id(title),
            html_escape(&sentence_case(title))
        ));
        rest = &rest[at + title.len()..];
    }
    out.push_str(&html_words(rest));
    out
}

/// Where the first topic named by its title starts in `text`, and the
/// title: a whole word or words in capitals, as the terminal's reference
/// names a section. Of two titles that start there, the longer, so
/// `OUTPUT OPTIONS` is not read as `OUTPUT`.
fn next_title(text: &str) -> Option<(usize, &'static str)> {
    let word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '-');
    TOPICS
        .iter()
        .filter_map(|topic| {
            text.match_indices(topic.title)
                .find(|(at, _)| {
                    !word(text[..*at].chars().next_back())
                        && !word(text[at + topic.title.len()..].chars().next())
                })
                .map(|(at, _)| (at, topic.title))
        })
        .min_by_key(|(at, title)| (*at, std::cmp::Reverse(title.len())))
}

/// Words of prose as HTML: escaped, with each web address a link, and
/// each option named set as code and linked to its entry.
fn html_words(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + text.len() / 4);
    for (i, word) in text.split(' ').enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let start = word.len() - word.trim_start_matches('(').len();
        let core = word[start..].trim_end_matches(['.', ',', ';', ':', ')']);
        let (lead, rest) = word.split_at(start);
        let trail = &rest[core.len()..];
        out.push_str(&html_escape(lead));
        if core.starts_with("https://") || core.starts_with("http://") {
            let url = html_escape(core);
            out.push_str(&format!("<a href=\"{url}\">{url}</a>"));
        } else if let Some(opt) = mentioned_option(core) {
            out.push_str(&format!(
                "<a href=\"#{}\"><code>{}</code></a>",
                option_id(opt),
                html_escape(core)
            ));
        } else {
            out.push_str(&html_escape(core));
        }
        out.push_str(&html_escape(trail));
    }
    out
}

/// The option a word of the reference names: `--kind`, `--kind=yaml`, `-k`.
fn mentioned_option(word: &str) -> Option<&'static Opt> {
    let name = word.split('=').next().unwrap_or(word);
    let body = name.strip_prefix("--").or_else(|| name.strip_prefix('-'))?;
    if !body.starts_with(|c: char| c.is_ascii_alphabetic())
        || !body.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return None;
    }
    lookup(name)
}

/// What `--generate WHAT` prints.
pub fn generate(what: &str) -> Result<String, String> {
    match what {
        "man" => Ok(man_page()),
        "complete-bash" => Ok(complete_bash()),
        "complete-zsh" => Ok(complete_zsh()),
        "complete-fish" => Ok(complete_fish()),
        "complete-powershell" => Ok(complete_powershell()),
        "skill" => Ok(SKILL.to_string()),
        other => Err(format!(
            "--generate writes {}, not {other}",
            GENERATE.join(", ")
        )),
    }
}

/// The Agent Skill that teaches an agent to drive aless.
pub const SKILL: &str = include_str!("../skills/aless/SKILL.md");

/// The changelog, for the date of this version's release.
const CHANGELOG: &str = include_str!("../CHANGELOG.md");

/// The date this version was released, as the changelog's heading for it
/// gives it (`## [0.1.0] - 2026-10-09`); empty before then.
fn release_date() -> &'static str {
    let heading = format!("## [{VERSION}] - ");
    CHANGELOG
        .lines()
        .find_map(|l| l.strip_prefix(heading.as_str()))
        .map_or("", str::trim)
}

/// `text` as roff text: its backslashes, dashes and quotes made literal.
fn roff(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\e"),
            '-' => out.push_str("\\-"),
            '\'' => out.push_str("\\(aq"),
            '`' => out.push_str("\\(ga"),
            '^' => out.push_str("\\(ha"),
            '~' => out.push_str("\\(ti"),
            '—' => out.push_str("\\(em"),
            c => out.push(c),
        }
    }
    out
}

/// A line of roff text, kept from being read as a request.
fn roff_line(out: &mut String, line: &str) {
    if line.starts_with('.') {
        out.push_str("\\&");
    }
    out.push_str(line);
    out.push('\n');
}

/// Roff text, filled to lines of 80 bytes at most where its words allow,
/// as man pages are written; the formatter fills them again.
fn roff_text(out: &mut String, text: &str) {
    for line in wrap(text, 79) {
        roff_line(out, &line);
    }
}

/// Terms, each with what it means beside it, indented past the widest
/// term; a list whose meanings are a line each is set without space
/// between its items.
fn man_terms(out: &mut String, terms: &[(String, String)]) {
    let width = terms
        .iter()
        .map(|(t, _)| t.chars().count())
        .max()
        .unwrap_or(0);
    let indent = (width + 2).clamp(7, 18);
    let compact = terms.iter().all(|(_, m)| m.chars().count() <= 60);
    for (i, (term, meaning)) in terms.iter().enumerate() {
        if compact && i == 1 {
            out.push_str(".PD 0\n");
        }
        out.push_str(&format!(".TP {indent}\n"));
        roff_line(out, &format!("\\fB{}\\fR", roff(term)));
        roff_text(out, &roff(meaning));
    }
    if compact && terms.len() > 1 {
        out.push_str(".PD\n");
    }
}

/// A topic's blocks. A heading starts a paragraph, so the first block
/// opens none of its own.
fn man_blocks(out: &mut String, blocks: &[Block]) {
    for (i, block) in blocks.iter().enumerate() {
        let paragraph = if i == 0 { "" } else { ".PP\n" };
        match block {
            Block::Text(text) => {
                out.push_str(paragraph);
                roff_text(out, &roff(&fill(text)));
            }
            Block::Code(code) => {
                out.push_str(paragraph);
                out.push_str(if i == 0 { ".nf\n" } else { ".RS 2\n.nf\n" });
                for line in fill(code).lines() {
                    roff_line(out, &roff(line));
                }
                out.push_str(if i == 0 { ".fi\n" } else { ".fi\n.RE\n" });
            }
            Block::Terms(terms) => man_terms(out, &static_terms(terms)),
            Block::Formats => man_terms(out, &format_terms()),
            Block::Options(section) => {
                for opt in options_in(*section) {
                    out.push_str(".TP 7\n");
                    let names: Vec<String> = opt
                        .short
                        .map(|c| format!("-{c}"))
                        .into_iter()
                        .chain(std::iter::once(opt.long.to_string()))
                        .chain(opt.also.iter().map(|a| a.to_string()))
                        .map(|n| format!("\\fB{}\\fR", roff(&n)))
                        .collect();
                    let mut head = names.join(", ");
                    if let Some((arg, _)) = opt.arg {
                        head.push_str(&format!(" \\fI{}\\fR", roff(arg)));
                    }
                    roff_line(out, &head);
                    roff_text(out, &roff(&fill(opt.detail)));
                }
            }
        }
    }
}

/// The man page, `aless.1`.
pub fn man_page() -> String {
    let mut out = String::new();
    out.push_str(&format!(
        ".TH ALESS 1 \"{}\" \"aless {VERSION}\" \"User Commands\"\n",
        release_date()
    ));
    // No hyphenation: options and paths read as they are typed.
    out.push_str(".nh\n");
    out.push_str(".SH NAME\n");
    roff_text(&mut out, &format!("aless \\- {}", roff(TAGLINE)));
    for topic in TOPICS {
        match topic.man {
            Man::Synopsis => {
                out.push_str(".SH SYNOPSIS\n");
                if let Some(Block::Terms(terms)) = topic.blocks.first() {
                    for (i, (usage, _)) in terms.iter().enumerate() {
                        if i > 0 {
                            out.push_str(".br\n");
                        }
                        roff_line(&mut out, &roff(usage));
                    }
                }
                out.push_str(".PP\n");
                man_blocks(&mut out, &topic.blocks[1..]);
                out.push_str(".SH DESCRIPTION\n");
                roff_text(&mut out, &roff(DESCRIPTION));
            }
            Man::Description(title) => {
                out.push_str(&format!(".SS \"{title}\"\n"));
                man_blocks(&mut out, topic.blocks);
            }
            Man::Options(title) => {
                if title == "Output options" {
                    out.push_str(".SH OPTIONS\n");
                }
                out.push_str(&format!(".SS \"{title}\"\n"));
                man_blocks(&mut out, topic.blocks);
            }
            Man::Section => {
                out.push_str(&format!(".SH \"{}\"\n", topic.title));
                man_blocks(&mut out, topic.blocks);
            }
        }
    }
    out
}

/// The words a shell offers for an option's value, when it has a list.
fn value_words(value: Value) -> Option<Vec<&'static str>> {
    match value {
        Value::Words(words) => Some(words.to_vec()),
        Value::Format => Some(Format::ALL.iter().map(|f| f.name()).collect()),
        Value::Render => Some(crate::translate::render_names()),
        Value::Text | Value::File => None,
    }
}

/// The completions for bash.
pub fn complete_bash() -> String {
    let mut out = String::from(
        "# bash completion for aless, written by `aless --generate complete-bash`.\n\
         # Source it, or install it as bash-completion/completions/aless.\n\
         _aless() {\n    \
             local cur=\"${COMP_WORDS[COMP_CWORD]}\"\n    \
             local prev=\"${COMP_WORDS[COMP_CWORD-1]}\"\n    \
             COMPREPLY=()\n    \
             case \"$prev\" in\n",
    );
    let mut free = Vec::new();
    for opt in OPTIONS {
        let Some((_, value)) = opt.arg else { continue };
        let names = opt.all_names().join("|");
        match value_words(value) {
            Some(words) => out.push_str(&format!(
                "        {names})\n            \
                 COMPREPLY=($(compgen -W \"{}\" -- \"$cur\"))\n            \
                 return 0\n            \
                 ;;\n",
                words.join(" ")
            )),
            None => free.push(names),
        }
    }
    // A file, or text of the caller's own: what the shell offers by
    // default (-o default below), never an option.
    out.push_str(&format!(
        "        {})\n            \
         return 0\n            \
         ;;\n    \
         esac\n    \
         if [[ \"$cur\" == -* ]]; then\n        \
         COMPREPLY=($(compgen -W \"{}\" -- \"$cur\"))\n    \
         fi\n    \
         return 0\n\
         }}\n\
         complete -o default -F _aless aless\n",
        free.join("|"),
        OPTIONS
            .iter()
            .flat_map(|o| o.all_names())
            .collect::<Vec<_>>()
            .join(" ")
    ));
    out
}

/// `text` in a zsh `_arguments` spec, inside single quotes, with each of
/// `special` (and the backslash) escaped by a backslash.
fn zsh_text(text: &str, special: &[char]) -> String {
    let mut out = String::new();
    for c in text.chars() {
        match c {
            '\'' => out.push_str("'\\''"),
            c if c == '\\' || special.contains(&c) => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    out
}

/// A description, in brackets: `[` and `]` end it, and `:` the spec.
fn zsh_description(text: &str) -> String {
    zsh_text(text, &['[', ']', ':'])
}

/// A value's message, between colons.
fn zsh_message(text: &str) -> String {
    zsh_text(text, &[':'])
}

/// The completions for zsh.
pub fn complete_zsh() -> String {
    let mut out = String::from(
        "#compdef aless\n\
         # zsh completion for aless, written by `aless --generate complete-zsh`.\n\
         # Install it as _aless in a directory on $fpath, or load it from\n\
         # ~/.zshrc after compinit: eval \"$(aless --generate complete-zsh)\"\n\
         \n\
         _aless() {\n  \
           _arguments -S \\\n",
    );
    for opt in OPTIONS {
        let names = opt.all_names();
        let exclude = if names.len() > 1 {
            format!("({})", names.join(" "))
        } else {
            String::new()
        };
        let star = if opt.repeats { "*" } else { "" };
        let summary = zsh_description(opt.summary);
        for name in &names {
            // A long option's value may be attached (--kind=json) or
            // follow it; a letter's follows it.
            let (spec_name, value) = match opt.arg {
                Some((arg, value)) => {
                    let action = match value_words(value) {
                        Some(words) => format!("({})", words.join(" ")),
                        None if value == Value::File => "_files".to_string(),
                        None => " ".to_string(),
                    };
                    let name = if name.starts_with("--") {
                        format!("{name}=")
                    } else {
                        name.clone()
                    };
                    (name, format!(":{}:{action}", zsh_message(arg)))
                }
                None => (name.clone(), String::new()),
            };
            out.push_str(&format!(
                "    '{exclude}{star}{spec_name}[{summary}]{value}' \\\n"
            ));
        }
    }
    // Autoloaded from $fpath, the file is the body of _aless, which runs
    // the completion; sourced, it registers the function with compdef.
    out.push_str(
        "    '*:file:_files'\n}\n\n\
         if [ \"$funcstack[1]\" = \"_aless\" ]; then\n  \
           _aless \"$@\"\n\
         else\n  \
           compdef _aless aless\n\
         fi\n",
    );
    out
}

/// `text` in fish's single quotes.
fn fish_text(text: &str) -> String {
    text.replace('\\', "\\\\").replace('\'', "\\'")
}

/// The completions for fish.
pub fn complete_fish() -> String {
    let mut out = String::from(
        "# fish completion for aless, written by `aless --generate complete-fish`.\n\
         # Install it as aless.fish in a directory on $fish_complete_path.\n",
    );
    for opt in OPTIONS {
        let mut line = String::from("complete -c aless");
        if let Some(c) = opt.short {
            line.push_str(&format!(" -s {c}"));
        }
        line.push_str(&format!(" -l {}", &opt.long[2..]));
        for also in opt.also {
            line.push_str(&format!(" -l {}", &also[2..]));
        }
        if let Some((_, value)) = opt.arg {
            match value_words(value) {
                Some(words) => line.push_str(&format!(" -x -a '{}'", words.join(" "))),
                None if value == Value::File => line.push_str(" -r -F"),
                None => line.push_str(" -x"),
            }
        }
        line.push_str(&format!(" -d '{}'\n", fish_text(opt.summary)));
        out.push_str(&line);
    }
    out
}

/// `text` in PowerShell's single quotes.
fn ps_text(text: &str) -> String {
    text.replace('\'', "''")
}

/// The completions for PowerShell.
pub fn complete_powershell() -> String {
    let mut out = String::from(
        "# PowerShell completion for aless, written by\n\
         # `aless --generate complete-powershell`. Add it to $PROFILE, or\n\
         # dot-source it from there.\n\
         Register-ArgumentCompleter -Native -CommandName 'aless' -ScriptBlock {\n    \
             param($wordToComplete, $commandAst, $cursorPosition)\n    \
             $words = @($commandAst.CommandElements |\n        \
                 Where-Object { $_.Extent.StartOffset -lt $cursorPosition } |\n        \
                 ForEach-Object { $_.ToString() })\n    \
             $prev = if ($wordToComplete -eq '') { $words[-1] } else { $words[-2] }\n    \
             $values = $null\n    \
             switch -CaseSensitive ($prev) {\n",
    );
    let mut free = Vec::new();
    for opt in OPTIONS {
        let Some((_, value)) = opt.arg else { continue };
        let names: Vec<String> = opt.all_names().iter().map(|n| format!("'{n}'")).collect();
        match value_words(value) {
            Some(words) => out.push_str(&format!(
                "        {{ $_ -cin {} }} {{ $values = @({}) }}\n",
                names.join(", "),
                words
                    .iter()
                    .map(|w| format!("'{w}'"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
            None => free.extend(names),
        }
    }
    // A file, or text of the caller's own: nothing of aless's, so that
    // PowerShell offers what it does by default.
    out.push_str(&format!(
        "        {{ $_ -cin {} }} {{ return }}\n    \
         }}\n    \
         if ($null -ne $values) {{\n        \
             $values | Where-Object {{ $_ -like \"$wordToComplete*\" }} | ForEach-Object {{\n            \
                 [System.Management.Automation.CompletionResult]::new($_, $_, [System.Management.Automation.CompletionResultType]::ParameterValue, $_)\n        \
             }}\n        \
             return\n    \
         }}\n    \
         if (-not $wordToComplete.StartsWith('-')) {{ return }}\n    \
         @(\n",
        free.join(", ")
    ));
    for opt in OPTIONS {
        for name in opt.all_names() {
            out.push_str(&format!(
                "        [System.Management.Automation.CompletionResult]::new('{name}', '{name}', \
                 [System.Management.Automation.CompletionResultType]::ParameterName, '{}')\n",
                ps_text(opt.summary)
            ));
        }
    }
    out.push_str(
        "    ) | Where-Object { $_.CompletionText -clike \"$wordToComplete*\" }\n\
         }\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_is_one_options_and_looks_it_up() {
        let mut seen = std::collections::HashSet::new();
        for opt in OPTIONS {
            for name in opt.all_names() {
                assert!(seen.insert(name.clone()), "{name} names two options");
                assert_eq!(lookup(&name).map(|o| o.long), Some(opt.long), "{name}");
            }
            // The completions write a long name without its dashes.
            for long in std::iter::once(&opt.long).chain(opt.also) {
                assert!(long.starts_with("--") && long.len() > 3, "{long}");
            }
        }
        assert!(lookup("--nope").is_none());
        assert!(lookup("-x").is_none());
        assert!(lookup("k").is_none());
        assert!(asks_for_output("--json") && asks_for_output("--max-output"));
        assert!(!asks_for_output("--depth") && !asks_for_output("-k"));
    }

    #[test]
    fn a_summary_fits_its_column_and_a_detail_is_a_sentence() {
        for opt in OPTIONS {
            assert!(
                opt.summary.chars().count() <= SUMMARY_WIDTH,
                "{}: {:?} is wider than {SUMMARY_WIDTH}",
                opt.long,
                opt.summary
            );
            assert!(!opt.summary.ends_with('.'), "{}", opt.long);
            assert!(opt.detail.ends_with('.'), "{}", opt.long);
            assert!(!opt.detail.contains("  "), "{}: a double space", opt.long);
        }
    }

    /// The help is read in a terminal 80 columns wide.
    #[test]
    fn the_help_fits_80_columns() {
        for (name, text) in [("--help", reference()), ("-h", summary())] {
            for line in text.lines() {
                assert!(
                    line.chars().count() <= WIDTH,
                    "{name}: {} columns: {line}",
                    line.chars().count()
                );
                assert!(!line.ends_with(' '), "{name}: trailing space: {line:?}");
                assert!(!line.contains('\t'), "{name}: a tab: {line:?}");
            }
        }
    }

    /// Every option is in the reference with its value, and in the
    /// summary with its own line.
    #[test]
    fn the_help_lists_every_option() {
        let reference = reference();
        let summary = summary();
        for opt in OPTIONS {
            let head = opt.head();
            assert!(
                reference.lines().any(|l| l.trim_start() == head),
                "--help has no line for {head}"
            );
            assert!(
                summary.lines().any(|l| l.trim_start().starts_with(&head)),
                "-h has no line for {head}"
            );
            assert!(summary.contains(opt.summary), "-h: {}", opt.summary);
        }
        for placeholder in ["{renders}", "{formats}", "{version}"] {
            assert!(!reference.contains(placeholder), "{placeholder}");
        }
    }

    /// The reference names every format aless reads, with every extension
    /// that implies it, and every format --render writes.
    #[test]
    fn the_help_names_the_formats() {
        let reference = reference();
        for format in Format::ALL {
            // The table is indented under the paragraph that opens it.
            let line = reference
                .lines()
                .find(|l| l.starts_with(&format!("      {} ", format.name())))
                .unwrap_or_else(|| panic!("no line for {format}"));
            for ext in format.extensions() {
                assert!(
                    line.split_whitespace().any(|w| w == *ext),
                    "{format}: {ext} in {line:?}"
                );
            }
        }
        let flat = reference.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(flat.contains(&crate::translate::names()), "{flat}");
        assert!(flat.contains(&format_names()), "{flat}");
    }

    #[test]
    fn generate_writes_each_file_and_refuses_the_rest() {
        for what in GENERATE {
            let text = generate(what).unwrap();
            assert!(!text.is_empty(), "{what}");
        }
        let err = generate("nope").unwrap_err();
        assert!(err.contains("man, complete-bash"), "{err}");
        assert_eq!(generate("skill").unwrap(), SKILL);
        assert!(SKILL.starts_with("---\nname: aless\n"));
    }

    /// The man page and the completions are ASCII, so no encoding a
    /// shell or a formatter reads them in can change them.
    #[test]
    fn the_generated_files_are_ascii() {
        for what in GENERATE.iter().filter(|w| **w != "skill") {
            let text = generate(what).unwrap();
            let bad: Vec<char> = text.chars().filter(|c| !c.is_ascii()).collect();
            assert!(bad.is_empty(), "{what}: {bad:?}");
        }
    }

    /// The man page and every shell's completions name every option.
    #[test]
    fn the_generated_files_name_every_option() {
        let man = man_page();
        assert!(man.starts_with(".TH ALESS 1 "), "{man}");
        for opt in OPTIONS {
            assert!(
                man.contains(&format!("\\fB{}\\fR", roff(opt.long))),
                "man: {}",
                opt.long
            );
        }
        for (shell, text) in [
            ("bash", complete_bash()),
            ("zsh", complete_zsh()),
            ("powershell", complete_powershell()),
        ] {
            for opt in OPTIONS {
                for name in opt.all_names() {
                    assert!(text.contains(&name), "{shell}: {name}");
                }
            }
        }
        let fish = complete_fish();
        for opt in OPTIONS {
            assert!(
                fish.contains(&format!(" -l {} ", &opt.long[2..])),
                "fish: {}",
                opt.long
            );
        }
    }

    /// A man page sets code as it is, 7 columns in (9 under a paragraph),
    /// so code fits 80 columns there too.
    #[test]
    fn code_fits_a_man_page() {
        for topic in TOPICS {
            for (i, block) in topic.blocks.iter().enumerate() {
                if let Block::Code(code) = block {
                    let room = if i == 0 { 73 } else { 71 };
                    for line in fill(code).lines() {
                        assert!(line.chars().count() <= room, "{}: {line}", topic.title);
                    }
                }
            }
        }
    }

    #[test]
    fn roff_text_cannot_be_read_as_a_request() {
        assert_eq!(roff("--a 'b' \\c — d"), "\\-\\-a \\(aqb\\(aq \\ec \\(em d");
        let mut out = String::new();
        roff_line(&mut out, ".a.b");
        assert_eq!(out, "\\&.a.b\n");
    }

    #[test]
    fn wrap_fills_lines_to_the_width() {
        assert_eq!(wrap("a bb ccc dddd", 6), ["a bb", "ccc", "dddd"]);
        assert_eq!(wrap("toolongword x", 4), ["toolongword", "x"]);
        assert!(wrap("  ", 10).is_empty());
    }

    #[test]
    fn the_release_date_is_the_changelogs() {
        let date = release_date();
        assert!(
            date.is_empty() || (date.len() == 10 && date.as_bytes()[4] == b'-'),
            "{date:?}"
        );
    }

    /// The site's reference has every topic and every option, each with an
    /// id a link can name, and links an option where the text mentions it.
    #[test]
    fn the_html_reference_names_every_topic_and_option() {
        let html = reference_html();
        for topic in TOPICS {
            let id = format!("<h2 id=\"{}\">", title_id(topic.title));
            assert!(html.contains(&id), "{}", topic.title);
        }
        for opt in OPTIONS {
            let id = format!("<dt id=\"{}\">", option_id(opt));
            assert!(html.contains(&id), "{}", opt.long);
        }
        assert!(html.contains("<a href=\"#opt-kind\"><code>-k</code></a>"));
        assert!(!html.contains("{renders}") && !html.contains("{version}"));
        assert_eq!(
            html_text("(--kind=yaml)."),
            "(<a href=\"#opt-kind\"><code>--kind=yaml</code></a>)."
        );
        assert_eq!(html_text("a - b -1 non-zero --"), "a - b -1 non-zero --");
        assert_eq!(
            html_text("see https://x.example/a."),
            "see <a href=\"https://x.example/a\">https://x.example/a</a>."
        );
        assert_eq!(html_text("{\"a\": <1>}"), "{&quot;a&quot;: &lt;1&gt;}");
        assert_eq!(sentence_case("EXIT STATUS"), "Exit status");
        assert_eq!(
            html_text("see OUTPUT, ERRORS and EXIT STATUS. Under OUTPUT OPTIONS --json"),
            "see <a href=\"#output\">Output</a>, <a href=\"#errors\">Errors</a> and \
             <a href=\"#exit-status\">Exit status</a>. Under \
             <a href=\"#output-options\">Output options</a> \
             <a href=\"#opt-json\"><code>--json</code></a>"
        );
        assert_eq!(html_text("OUTPUTS NO_PATHS"), "OUTPUTS NO_PATHS");
    }

    /// Cargo.toml's description is the line crates.io shows, and dist
    /// writes it into the Homebrew formula as its `desc`, which `brew audit`
    /// holds to these rules (Homebrew's rubocops/shared/desc_helper.rb).
    #[test]
    fn the_description_keeps_homebrews_rules() {
        let desc = env!("CARGO_PKG_DESCRIPTION");
        let breaks = |pattern: &str| regex::Regex::new(pattern).unwrap().is_match(desc);
        assert!(!desc.is_empty());
        assert!(
            !breaks(r"^\s|\s$"),
            "no leading or trailing space: {desc:?}"
        );
        assert!(!breaks(r"(?i)command ?line"), "\"command-line\": {desc:?}");
        assert!(!breaks(r"(?i)^(?:the|an?)\s"), "no article first: {desc:?}");
        let first = desc.split_whitespace().next().unwrap_or_default();
        assert!(
            !breaks("^[a-z]") || ["iOS", "iPhone", "macOS"].contains(&first),
            "a capital letter first: {desc:?}"
        );
        assert!(
            !breaks(r"(?i)^a[\s-]?l[\s-]?e[\s-]?s[\s-]?s\b"),
            "not the formula's name first: {desc:?}"
        );
        assert!(
            !desc.ends_with('.') || desc.ends_with("etc."),
            "no full stop: {desc:?}"
        );
        assert!(!breaks(r"\p{So}"), "no emoji or symbols: {desc:?}");
        assert!(
            env!("CARGO_PKG_HOMEPAGE").starts_with("https://"),
            "the formula's homepage, and SEE ALSO's"
        );
        assert!(
            desc.chars().count() <= 80,
            "at most 80 characters: {desc:?}"
        );
    }
}
