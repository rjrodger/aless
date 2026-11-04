//! Every option aless takes ([`OPTIONS`]), and the reference written from
//! it: `-h`, `--help`, the man page and the completions (ADR-006).

use crate::load::Format;

/// This build's version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// What aless is, in a sentence: the help's first line, and the man
/// page's NAME.
pub const TAGLINE: &str = "JSON, YAML, TOML, CSV, XML, INI, Markdown and more as one \
    tree, in a jless-style terminal viewer";

/// What aless does, in a paragraph: the man page's DESCRIPTION.
const DESCRIPTION: &str = "aless reads JSON, JSON Lines, JSON5, JSONC, jsonic, YAML, \
    TOML, INI, CSV, TSV, XML, ZON, Markdown, RSS or Atom feeds, CSS, Protocol Buffers' \
    .proto files, PGN chess games, C source, grammars in ABNF, EBNF or GBNF, expressions \
    and semantic versions, and any line-oriented format an ABNF grammar describes, as one \
    tree, and shows it in a jless-style viewer in the terminal, with tabs, search, a \
    source view and a watch mode that reloads a changed file and keeps your place.";

/// Where an option belongs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    /// What is read, and how: the inputs' formats, the grammars, the
    /// limits.
    Input,
    /// How the viewer shows it.
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

/// The width of a summary, so that `-h` fits a terminal of `WIDTH`.
pub const SUMMARY_WIDTH: usize = 44;

const MODES: &[&str] = &["data", "line"];

/// What `--generate` writes.
pub const GENERATE: &[&str] = &[
    "man",
    "complete-bash",
    "complete-zsh",
    "complete-fish",
    "complete-powershell",
];

/// Every option aless takes, in the order the help lists them. `{formats}`
/// and `{version}` in a detail are filled in as it is written.
pub const OPTIONS: &[Opt] = &[
    Opt {
        short: Some('k'),
        long: "--kind",
        also: &["--format"],
        arg: Some(("FORMAT", Value::Format)),
        section: Section::Input,
        repeats: false,
        summary: "parse every input as FORMAT",
        detail: "Read every input as FORMAT, whatever its extension says: a format's \
            name ({formats}), one of its extensions (yml, md, ndjson), or a --grammar \
            NAME. Standard input is read as JSON unless this says otherwise. --format is \
            another name for this option.",
    },
    Opt {
        short: None,
        long: "--grammar",
        also: &[],
        arg: Some(("NAME=FILE", Value::File)),
        section: Section::Input,
        repeats: true,
        summary: "a format of your own, from an ABNF grammar",
        detail: "Read every file whose extension or whole name is NAME, in any case \
            (x.hosts, /etc/hosts), with the ABNF grammar in FILE. NAME is then a format, \
            which -k and :format take and the status bar shows; NAME,NAME2=FILE gives it \
            two names, and the option repeats. FILE is read within --max-size and \
            compiled within --timeout, before any input is read. See CUSTOM GRAMMARS.",
    },
    Opt {
        short: None,
        long: "--grammar-expr",
        also: &[],
        arg: Some(("NAME=ABNF", Value::Text)),
        section: Section::Input,
        repeats: true,
        summary: "the same, the grammar on the command line",
        detail: "As --grammar, with the grammar's text on the command line: everything \
            after the first =.",
    },
    Opt {
        short: None,
        long: "--max-size",
        also: &[],
        arg: Some(("SIZE", Value::Text)),
        section: Section::Input,
        repeats: false,
        summary: "refuse an input over SIZE (default 64M)",
        detail: "Refuse an input larger than SIZE, a --grammar file included (default \
            64M; 0 for no limit). SIZE is bytes, or a whole number with K, M or G, in \
            1024s. A file over the limit opens in a tab that says so and names the size \
            that would read it; a grammar file over it is a usage error. A parse needs \
            about 40 bytes of memory for each byte of input.",
    },
    Opt {
        short: None,
        long: "--timeout",
        also: &[],
        arg: Some(("SECONDS", Value::Text)),
        section: Section::Input,
        repeats: false,
        summary: "stop a parse past SECONDS (default none)",
        detail: "Stop a parse, or a --grammar compile, that runs longer than SECONDS \
            (2.5, 90s, 2m; default none; 0 for no limit). A parse that is stopped opens \
            in a tab that says how far it got; a compile that is stopped is a usage \
            error. A parse runs at about a megabyte a second. See LARGE INPUTS AND \
            LIMITS.",
    },
    Opt {
        short: None,
        long: "--no-watch",
        also: &[],
        arg: None,
        section: Section::Viewer,
        repeats: false,
        summary: "do not reload files when they change",
        detail: "Do not reload a file when it changes on disk. W or :watch on starts \
            watching a tab inside aless.",
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
        detail: "Start in data mode (the default), the tree without its punctuation, or \
            in line mode, every line as JSON prints it; m switches between them.",
    },
    Opt {
        short: None,
        long: "--depth",
        also: &[],
        arg: Some(("N", Value::Text)),
        section: Section::Viewer,
        repeats: false,
        summary: "open only the first N levels",
        detail: "Open only the first N levels when a file opens: a container N levels \
            below the root starts folded, and so does everything in it, so --depth 0 \
            folds the root (by default every container is open). :depth N does the \
            same inside aless.",
    },
    Opt {
        short: None,
        long: "--indent",
        also: &[],
        arg: Some(("N", Value::Text)),
        section: Section::Viewer,
        repeats: false,
        summary: "indentation per level (default 2)",
        detail: "Indent the tree N spaces a level (default 2, at most 16); < and > \
            change it inside aless.",
    },
    Opt {
        short: Some('n'),
        long: "--line-numbers",
        also: &[],
        arg: None,
        section: Section::Viewer,
        repeats: false,
        summary: "number the rows",
        detail: "Number the rows on screen, as a count before g or G takes them.",
    },
    Opt {
        short: Some('N'),
        long: "--no-line-numbers",
        also: &[],
        arg: None,
        section: Section::Viewer,
        repeats: false,
        summary: "do not number the rows",
        detail: "Do not number the rows, undoing an -n before it.",
    },
    Opt {
        short: Some('r'),
        long: "--relative-line-numbers",
        also: &[],
        arg: None,
        section: Section::Viewer,
        repeats: false,
        summary: "number rows by distance from the focus",
        detail: "Number each row by its distance from the focused row, the count j or \
            k needs to reach it. With -n as well, the focused row shows its own number.",
    },
    Opt {
        short: Some('R'),
        long: "--no-relative-line-numbers",
        also: &[],
        arg: None,
        section: Section::Viewer,
        repeats: false,
        summary: "do not number rows by distance",
        detail: "Do not number the rows by distance, undoing an -r before it.",
    },
    Opt {
        short: None,
        long: "--scrolloff",
        also: &[],
        arg: Some(("N", Value::Text)),
        section: Section::Viewer,
        repeats: false,
        summary: "rows kept around the focus (default 3)",
        detail: "Keep N rows between the focused row and the top or the bottom of the \
            screen (default 3).",
    },
    Opt {
        short: None,
        long: "--ascii",
        also: &[],
        arg: None,
        section: Section::Viewer,
        repeats: false,
        summary: "draw fold markers in ASCII",
        detail: "Draw the fold markers as v and >, for a font without the triangles \
            aless draws by default.",
    },
    Opt {
        short: None,
        long: "--no-color",
        also: &["--no-colour"],
        arg: None,
        section: Section::Viewer,
        repeats: false,
        summary: "no colours (as NO_COLOR does)",
        detail: "Draw without colour, as when NO_COLOR is set.",
    },
    Opt {
        short: None,
        long: "--no-mouse",
        also: &[],
        arg: None,
        section: Section::Viewer,
        repeats: false,
        summary: "do not capture the mouse",
        detail: "Leave the mouse to the terminal, for its own selection, rather than \
            take its wheel and clicks.",
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
        summary: "a man page or a shell's completions",
        detail: "Print a file written from this reference: man, the man page (aless.1), \
            or complete-bash, complete-zsh, complete-fish or complete-powershell, that \
            shell's completions. See FILES.",
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
                ("aless [OPTIONS] [FILE]...", "each FILE in a tab of its own"),
                ("aless [OPTIONS] DIRECTORY", "the file explorer"),
                (
                    "<command> | aless [OPTIONS]",
                    "standard input, read as JSON unless -k says otherwise",
                ),
            ]),
            Block::Text("-h prints a summary of the options, and --help this whole reference."),
        ],
    },
    Topic {
        title: "THE VIEWER",
        note: "",
        man: Man::Description("The viewer"),
        blocks: &[
            Block::Text(
                "aless is a viewer for the terminal. Each FILE opens in a tab of its own, \
                as a tree to fold, search and copy from, and a directory opens in the file \
                explorer. With no FILE, aless reads standard input when it is not a \
                terminal, and otherwise explores the current directory. Every file is \
                watched: when it changes on disk, aless reads it again and keeps your \
                place. A file that does not parse opens as the parser's report, with the \
                line and column where the parse stopped.",
            ),
            Block::Text(
                "aless refuses to start without a terminal to draw on and read keys \
                from: when standard output is a pipe or a file, when there is no terminal \
                to open, or when TERM is dumb. It refuses before reading any input, with \
                one line on standard error and the status 2. Only -h, --help, -V and \
                --generate write to standard output.",
            ),
        ],
    },
    Topic {
        title: "INPUT OPTIONS",
        note: "what is read, and how",
        man: Man::Options("Input options"),
        blocks: &[Block::Options(Section::Input)],
    },
    Topic {
        title: "VIEWER OPTIONS",
        note: "how it is shown",
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
                "FILE is a path, or - for standard input. Options end at --, after which \
                every argument is a FILE, and a long option's value follows it or is \
                attached with = (--kind=yaml).",
            ),
            Block::Text(
                "A file's format comes from its extension, or from its whole name when \
                that is a --grammar NAME (/etc/hosts), and -k FORMAT overrides it for every \
                input. Inside aless, :format FORMAT reads a tab again as FORMAT, and :open \
                PATH FORMAT opens a file as one. A file whose extension no format claims is \
                read as plain text, an array of its lines.",
            ),
            Block::Text(
                "Keys keep their order in the source. A number keeps its spelling where \
                JSON spells it the same way: 1.0, 1e21 and an integer beyond 2^53 are \
                shown, searched, copied and written by :w as the source writes them. A \
                spelling JSON has no syntax for (0x1F, 1_000, +1) gives the 64-bit float it \
                stands for, and the source view (s) shows the spelling. The formats, with \
                the extensions that imply each:",
            ),
            Block::Formats,
        ],
    },
    Topic {
        title: "KEYS",
        note: "",
        man: Man::Section,
        blocks: &[
            Block::Text(
                "The keys are jless's. F1 or :help inside aless lists every key and \
                command; these are the ones used most:",
            ),
            Block::Terms(&[
                (
                    "j k",
                    "down and up a row; a count before a key repeats it (3j)",
                ),
                (
                    "h l",
                    "fold and unfold, or go to the parent and the first child",
                ),
                ("J K", "the next and the previous sibling"),
                ("g G", "the first and the last row; Ng goes to row N"),
                ("C-d C-u", "half a page down and up"),
                ("Space", "fold or unfold the focused container"),
                ("c e", "fold and unfold the focused node and its siblings"),
                (
                    "/ ?",
                    "search forward and backward; n and N go to the next and the \
                    previous match",
                ),
                (
                    "yy yp yq",
                    "copy the value, its path, or its path as jq writes it",
                ),
                ("m", "switch between data mode and line mode"),
                ("s", "the source view, at the focused node's line"),
                ("!", "the parse error report, full size"),
                ("Tab S-Tab", "the next and the previous tab"),
                (":", "a command: :open PATH, :format FORMAT, :w FILE, :help"),
                ("q", "close the tab, quitting with the last one; C-c quits"),
            ]),
        ],
    },
    Topic {
        title: "CUSTOM GRAMMARS",
        note: "",
        man: Man::Section,
        blocks: &[
            Block::Text(
                "--grammar NAME=FILE reads every file whose extension or whole name is \
                NAME, in any case, with the ABNF grammar in FILE (RFC 5234, compiled by \
                tabnas/abnf), and -k NAME reads any input with it, standard input \
                included. Comments on a rule say what it builds: ; @object a b makes an \
                object of its parts a and b, and ; @array an array. Without them the \
                value is the compiler's parse tree, a {\"rule\", \"src\", \"kids\"} node \
                for each rule.",
            ),
            Block::Text(
                "A grammar reads its input as plain text: { } [ ] : , are ordinary \
                characters, # starts a comment to the end of the line, and a word is the \
                token TX. NR, ST and VL bring back numbers, quoted strings, and true, \
                false and null.",
            ),
            Block::Text(
                "A grammar that does not compile is refused before any input is read, \
                as a usage error. So is a repetition count over 1,024, or repetitions \
                that would have the compiler write more than 1,024 rules. An input the \
                grammar does not accept opens as a parse error, with the grammar's name \
                as its format.",
            ),
            Block::Text(
                "aless's source holds grammars for /etc/hosts, crontabs, /etc/passwd, \
                /etc/group, /etc/fstab, /etc/resolv.conf and KEY=value files, each with a \
                sample, and a guide to writing one:",
            ),
            Block::Code("https://github.com/rjrodger/aless/tree/main/tests/fixtures/grammars"),
        ],
    },
    Topic {
        title: "LARGE INPUTS AND LIMITS",
        note: "",
        man: Man::Section,
        blocks: &[
            Block::Text(
                "aless reads an input whole and parses it whole before it shows \
                anything. A parse runs at about a megabyte a second, and needs about 40 \
                bytes of memory for each byte of input. The viewer waits for it: before a \
                large file is read aless says so, and a reload of a large watched file \
                pauses the viewer until the parse ends.",
            ),
            Block::Text(
                "--max-size (64M unless set) refuses a larger input before reading it, \
                in a tab that says so and names the size that would read it. --timeout \
                stops a parse that runs too long, in a tab that says how far it got.",
            ),
            Block::Text(
                "A document nested deeper than aless reads opens as a parse error with \
                the code too_deep. Every parse stops at 3,000 open rules, about 1,000 \
                levels, and most grammars stop sooner: at 127 levels for JSON, JSONL, \
                jsonic, JSON5, YAML, TOML, INI, ZON and expressions, 256 for XML, 512 for \
                JSONC, and 768 open rules for CSS. A .proto file's braces may nest 100 \
                deep, counted before it is parsed. Markdown's tree stops at 205 levels, \
                without an error.",
            ),
            Block::Text(
                "C stops at an expression of about 127 operations, 1,024 open rules or a \
                syntax tree 256 levels deep. Its tree repeats subtrees, so it also stops \
                at a tree of more than 256 values, or more than 2,048 bytes of names and \
                strings, for each byte of the file (between a million and 33,554,432 \
                values, and between 8 MB and 256 MB). A grammar in ABNF, EBNF or GBNF may \
                nest its groups and elements 127 deep.",
            ),
        ],
    },
    Topic {
        title: "EXIT STATUS",
        note: "",
        man: Man::Section,
        blocks: &[Block::Terms(&[
            ("0", "the viewer was quit"),
            (
                "1",
                "the viewer failed while it ran: the terminal could not be drawn on or \
                read",
            ),
            (
                "2",
                "the viewer could not start: bad usage (an unknown option, a bad value, a \
                --grammar that does not compile or cannot be read), or no terminal to \
                draw on and read keys from",
            ),
        ])],
    },
    Topic {
        title: "ENVIRONMENT",
        note: "",
        man: Man::Section,
        blocks: &[Block::Terms(&[
            (
                "NO_COLOR",
                "set and not empty: draw without colour, as --no-color does",
            ),
            (
                "TERM",
                "dumb: a terminal that cannot draw the viewer, so aless refuses to start, \
                as it does without a terminal",
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
                "A shell reads its completions where it looks for them, which is where a \
                package puts them, or from its startup file each time it starts:",
            ),
            Block::Code(
                "eval \"$(aless --generate complete-bash)\"     # ~/.bashrc\n\
                 eval \"$(aless --generate complete-zsh)\"      # ~/.zshrc, after compinit\n\
                 aless --generate complete-fish | source      # config.fish",
            ),
        ],
    },
    Topic {
        title: "EXAMPLES",
        note: "",
        man: Man::Section,
        blocks: &[Block::Code(
            "aless config.yaml                      one file, in a tab\n\
             aless deploy.yaml values.toml a.csv    several files, a tab each\n\
             curl -s https://api.example/x | aless  standard input, read as JSON\n\
             aless -k yaml settings.conf            a file whose extension misleads\n\
             aless --grammar hosts=hosts.abnf /etc/hosts\n\
             \x20                                      a format of your own\n\
             aless --no-watch big.json              no reload when the file changes\n\
             aless -m line -n x.json                line mode, the rows numbered\n\
             aless                                  explore the current directory",
        )],
    },
    Topic {
        title: "SEE ALSO",
        note: "",
        man: Man::Section,
        blocks: &[Block::Terms(&[
            ("jless(1)", "the viewer whose keys and design aless follows"),
            ("jq(1)", "whose path syntax yq copies"),
            (
                "the documentation",
                concat!(
                    "a tutorial, how-to guides, this reference with every key, and why \
                    aless works as it does: ",
                    env!("CARGO_PKG_HOMEPAGE")
                ),
            ),
            (
                "the source",
                concat!("the code and the releases: ", env!("CARGO_PKG_REPOSITORY")),
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

/// `text` with its `{formats}` and `{version}` filled in.
fn fill(text: &str) -> String {
    let mut text = text.to_string();
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
        Format::Css => " (its syntax tree)",
        Format::Proto => " (its FileDescriptorProto, as protoc writes one in JSON)",
        Format::Pgn => " (chess games, each its tags and moves)",
        Format::Expr => {
            " (jsonic with arithmetic, an operation the list of its operator and terms)"
        }
        Format::Semver => " (a version: its major, minor, patch, prerelease and build)",
        Format::C => " (its concrete syntax tree, every token with its place)",
        Format::Abnf | Format::Ebnf | Format::Gbnf => {
            " (the grammar spec its compiler writes, the three notations' one schema)"
        }
        Format::Text => " (and any file no format claims)",
        _ => "",
    }
}

/// The formats and their extensions, as terms; a format that has none is
/// named with -k.
fn format_terms() -> Vec<(String, String)> {
    Format::ALL
        .iter()
        .map(|f| {
            let extensions = match f.extensions() {
                [] => format!("none, -k {}", f.name()),
                list => list.join(" "),
            };
            (
                f.name().to_string(),
                format!("{extensions}{}", format_note(*f)),
            )
        })
        .collect()
}

/// Each term, then what it means, filled beside it, or below it when the
/// term is wider than its column.
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
    push_filled(&mut out, &format!("aless {VERSION}: {TAGLINE}"), 0);
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
    push_filled(&mut out, &format!("aless {VERSION}: {TAGLINE}"), 0);
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
        "aless --help prints the whole reference: what each option does, the formats, \
        the keys used most, custom grammars, the limits, the exit statuses and the files \
        a release carries.",
        0,
    );
    out
}

/// The reference `--help` prints, as HTML, for the documentation site:
/// a section for each topic, in the same order, titled in sentence case
/// with an `id` a link can name (`exit-status`), and each option with an
/// `id` of its own (`opt-kind` for `--kind`), which a mention of the
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

/// The `id` of an option's entry in [`reference_html`]: `opt-check`.
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
/// names a section; where titles start at one place, the longest.
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
        other => Err(format!(
            "--generate writes {}, not {other}",
            GENERATE.join(", ")
        )),
    }
}

/// The changelog, for the date of this version's release.
const CHANGELOG: &str = include_str!("../CHANGELOG.md");

/// The date this version was released, from its heading in the
/// changelog; empty before it is.
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

/// Roff text, filled to lines shorter than `WIDTH` where its words allow,
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
    let mut options_started = false;
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
                if !options_started {
                    out.push_str(".SH OPTIONS\n");
                    options_started = true;
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

/// What a command line asks for.
pub enum Parsed {
    /// The viewer.
    Run(Box<Args>),
    /// Text to print as it is, and exit: `-h`, `--help`, `--version`,
    /// `--generate`.
    Print(String),
}

/// The viewer's command line, read.
pub struct Args {
    pub files: Vec<std::path::PathBuf>,
    /// `-k`, as given: resolved once the grammars are registered, since it
    /// may name one.
    pub kind_name: Option<String>,
    pub kind: Option<Format>,
    /// `--grammar` and `--grammar-expr`, in order.
    pub grammars: Vec<crate::grammar::Definition>,
    pub opts: crate::app::Options,
    pub mouse: bool,
    /// The largest input to read, in bytes; `None` for no limit.
    pub max_size: Option<u64>,
    /// The longest a parse may run; `None` for no limit.
    pub timeout: Option<std::time::Duration>,
}

impl Args {
    /// Register the grammars, resolve `-k` (which may name one of them)
    /// and set the limits: what is done before any input is read. An
    /// error is a usage error.
    pub fn prepare(&mut self) -> Result<(), String> {
        let grammars = std::mem::take(&mut self.grammars);
        let limits = crate::load::Limits {
            max_size: self.max_size,
            timeout: self.timeout,
        };
        crate::grammar::register_all(grammars, limits).map_err(|e| e.to_string())?;
        if let Some(name) = &self.kind_name {
            match Format::from_name(name) {
                Some(f) => self.kind = Some(f),
                None => {
                    return Err(format!(
                        "unknown format: {name} (one of {})",
                        Format::known_names().join(" ")
                    ))
                }
            }
        }
        crate::load::set_limits(limits);
        Ok(())
    }
}

/// Read `argv`, without the program's name. An option [`OPTIONS`]
/// does not list is refused (ADR-006).
pub fn parse_args(argv: impl IntoIterator<Item = std::ffi::OsString>) -> Result<Parsed, String> {
    let mut args = Args {
        files: Vec::new(),
        kind_name: None,
        kind: None,
        grammars: Vec::new(),
        opts: crate::app::Options::default(),
        mouse: true,
        max_size: crate::load::Limits::DEFAULT.max_size,
        timeout: crate::load::Limits::DEFAULT.timeout,
    };
    let mut it = argv.into_iter();
    let mut only_files = false;
    while let Some(arg) = it.next() {
        let s = arg.to_string_lossy().into_owned();
        if only_files || !s.starts_with('-') || s == "-" {
            args.files.push(std::path::PathBuf::from(arg));
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
        if lookup(&name).is_none() {
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
            "-h" => return Ok(Parsed::Print(summary())),
            "--help" => return Ok(Parsed::Print(reference())),
            "-V" | "--version" => return Ok(Parsed::Print(format!("aless {}\n", VERSION))),
            "--generate" => return generate(&value()?).map(Parsed::Print),
            "-k" | "--kind" | "--format" => args.kind_name = Some(value()?),
            "--grammar" | "--grammar-expr" => {
                // Raw, not through `value()`: FILE is a path, and a path's
                // bytes need not be UTF-8. Attached (`--grammar=NAME=FILE`),
                // the value is what follows the argument's own first `=`.
                let v = match inline.take() {
                    Some(_) => crate::grammar::after_eq(&arg),
                    None => it.next().ok_or_else(|| format!("{name} needs a value"))?,
                };
                args.grammars
                    .push(crate::grammar::Definition::parse_os(&name, &v)?);
            }
            "--max-size" => args.max_size = crate::load::parse_size(&value()?)?,
            "--timeout" => args.timeout = crate::load::parse_timeout(&value()?)?,
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

    /// The help is read in a terminal `WIDTH` wide.
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
        for placeholder in ["{formats}", "{version}"] {
            assert!(!reference.contains(placeholder), "{placeholder}");
        }
    }

    /// The reference names every format aless reads, with every extension
    /// that implies it.
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
    }

    /// The man page and the completions are ASCII, so no encoding a
    /// shell or a formatter reads them in can change them.
    #[test]
    fn the_generated_files_are_ascii() {
        for what in GENERATE {
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

    /// A man page sets code as it is, indented, so code fits a terminal
    /// there too.
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
        assert!(!html.contains("{version}"));
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
            html_text("see KEYS and EXIT STATUS. Under INPUT OPTIONS -k"),
            "see <a href=\"#keys\">Keys</a> and <a href=\"#exit-status\">Exit status</a>. \
             Under <a href=\"#input-options\">Input options</a> \
             <a href=\"#opt-kind\"><code>-k</code></a>"
        );
        assert_eq!(html_text("KEYSTONE NO_KEYS"), "KEYSTONE NO_KEYS");
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

    fn parse(argv: &[&str]) -> Result<Parsed, String> {
        parse_args(argv.iter().map(std::ffi::OsString::from))
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
            Ok(Parsed::Run(args)) => assert_eq!(args.files, [std::path::PathBuf::from("--nope")]),
            _ => panic!("-- --nope"),
        }
    }

    #[test]
    fn help_version_and_generate_print() {
        let print = |argv: &[&str]| match parse(argv) {
            Ok(Parsed::Print(text)) => text,
            _ => panic!("{argv:?} prints nothing"),
        };
        assert_eq!(print(&["-h"]), summary());
        assert_eq!(print(&["--help"]), reference());
        assert_eq!(print(&["-V"]), format!("aless {}\n", VERSION));
        assert_eq!(print(&["--generate", "man"]), man_page());
        assert_eq!(print(&["--generate=complete-fish"]), complete_fish());
        let Err(e) = parse(&["--generate", "nope"]) else {
            panic!("--generate nope");
        };
        assert!(e.starts_with("--generate writes man, "), "{e}");
    }
}
