//! Custom grammars: ABNF text given on the command line (`--grammar
//! NAME=FILE`, `--grammar-expr NAME=ABNF`), compiled once at startup by the
//! tabnas ABNF compiler and kept in a process-wide registry that
//! [`Format::Custom`](crate::load::Format::Custom) indexes.
//!
//! A registered grammar is a format like any built-in one: it has a name,
//! it is detected from a file's extension or whole file name (`x.hosts`,
//! `/etc/hosts`), `-k NAME` selects it, and its parser is made afresh for
//! every parse from the compiled spec, as the built-in grammars' are. What
//! a parse builds is whatever the grammar says: with `; @object` and
//! `; @array` annotations a JSON value of strings, without them the
//! compiler's `{rule, src, kids}` tree. Either is a value the document
//! model takes as it takes any grammar's, so the viewer and every headless
//! operation work on it unchanged.
//!
//! The registry is filled from the command line before any load, and
//! looked up by name from then on. It only grows: a name registered twice
//! resolves to the later grammar, as a repeated option would be expected
//! to. Tests register grammars of their own under names of their own.

use std::fmt;
use std::path::PathBuf;
use std::sync::{mpsc, RwLock};
use std::time::Duration;

use serde_json::{json, Map, Value};
use tabnas::Tabnas;
use tabnas_abnf::{AbnfConvertOptions, Element, GrammarSpec, Kind};

use crate::load::{self, Limits, LoadError};

/// The index of a custom grammar in the registry: what `Format::Custom`
/// carries. Only [`register`] makes one, so every id names a grammar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CustomId(u16);

/// Where a grammar's text came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// `--grammar NAME=FILE`: the file, as given.
    File(PathBuf),
    /// `--grammar-expr NAME=ABNF`: the text itself.
    Inline(String),
}

impl Source {
    /// The option that gives a grammar this way.
    pub fn option(&self) -> &'static str {
        match self {
            Source::File(_) => "--grammar",
            Source::Inline(_) => "--grammar-expr",
        }
    }

    /// The file, as a report names it; `None` for inline text.
    pub fn file(&self) -> Option<String> {
        match self {
            Source::File(p) => Some(p.display().to_string()),
            Source::Inline(_) => None,
        }
    }
}

/// A grammar as the command line defines it, before it is compiled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Definition {
    /// The names a file must match, as given; the first is the format's
    /// name. Matching is case-insensitive.
    pub names: Vec<String>,
    pub source: Source,
}

impl Definition {
    /// Read an option's value: `NAME[,NAME…]=FILE` for `--grammar`,
    /// `NAME[,NAME…]=ABNF` for `--grammar-expr`. The first `=` separates
    /// the names from the rest, which for `--grammar-expr` is the grammar
    /// text and contains `=` itself.
    pub fn parse(option: &str, value: &str) -> Result<Definition, String> {
        let inline = option == "--grammar-expr";
        let what = if inline { "ABNF" } else { "FILE" };
        let Some((names, rest)) = value.split_once('=') else {
            return Err(format!(
                "{option} needs NAME={what}, not {value:?} (NAME is the extension or file name \
                 the grammar reads; NAME,NAME2={what} shares one grammar)"
            ));
        };
        let mut seen = Vec::new();
        for name in names.split(',') {
            let name = name.trim();
            if name.is_empty() {
                return Err(format!(
                    "{option} {value:?}: a name is missing before the `=`"
                ));
            }
            if name
                .chars()
                .any(|c| c.is_whitespace() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"'))
            {
                return Err(format!(
                    "{option} {value:?}: {name:?} cannot be a file's extension or name"
                ));
            }
            if !seen.iter().any(|s: &String| s.eq_ignore_ascii_case(name)) {
                seen.push(name.to_string());
            }
        }
        if rest.trim().is_empty() {
            return Err(format!("{option} {value:?}: nothing after the `=`"));
        }
        let source = if inline {
            Source::Inline(rest.to_string())
        } else {
            Source::File(PathBuf::from(rest))
        };
        Ok(Definition {
            names: seen,
            source,
        })
    }
}

/// The engine's options for a grammar of plain text, merged into the
/// compiled spec's `options` before it is installed.
///
/// The compiler emits the tokens the grammar names and nothing else, and a
/// fresh engine's defaults for everything else are JSON's: `{ } [ ] : ,`
/// are tokens, `//` and `/* */` open comments, digits make numbers, quotes
/// make strings and `true` `false` `null` are values. Every one of those
/// breaks plain text (`::1` would be three tokens, `//server/share` a
/// comment, `0` a number where the grammar asked for a word), so they are
/// turned off, and the grammar's own tokens define the language: a word
/// (`TX`) runs to a space, tab, newline, `#` or one of the grammar's
/// literals, and a literal the grammar names is a token whatever it is
/// (`":"` splits `root:x:0:0`). What stays: `#` comments to the end of the
/// line, since every format this is for has them, and the number, string
/// and value lexers where the grammar names `NR`, `ST` or `VL`. A carriage
/// return becomes a space character, so a CRLF file parses as an LF file
/// does: the lexer skips the `\r` and reaches the `\n`, which the grammar
/// takes as its `%x0A` token (a fixed token is tried before a line end is
/// skipped, which is why a grammar needs no option to see its newlines).
/// Measured against tabnas-abnf 0.4.16 with the fixture grammars under
/// `tests/fixtures/grammars/`, whose README explains each setting.
pub(crate) fn plain_text_options(spec: &mut GrammarSpec) {
    /// The object at `key`, made one if it is missing or something else.
    fn object<'a>(map: &'a mut Map<String, Value>, key: &str) -> &'a mut Map<String, Value> {
        let value = map.entry(key).or_insert_with(|| json!({}));
        if !value.is_object() {
            *value = json!({});
        }
        value.as_object_mut().expect("made an object above")
    }
    // The token names the grammar uses, wherever they appear in the
    // emitted document (an alternate's `s`, a token set): `"#NR"` says the
    // grammar wants numbers.
    let document = spec.to_value().to_string();
    let names = |token: &str| document.contains(&format!("\"#{token}\""));
    let options = &mut spec.options;
    let tokens = object(object(options, "fixed"), "token");
    // A grammar with a production of one of these names keeps its token.
    for name in ["#OB", "#CB", "#OS", "#CS", "#CL", "#CA"] {
        tokens.entry(name).or_insert(Value::Null);
    }
    let defs = object(object(options, "comment"), "def");
    for name in ["slash", "multi"] {
        defs.entry(name).or_insert(Value::Null);
    }
    for (token, option) in [("NR", "number"), ("ST", "string"), ("VL", "value")] {
        if !names(token) {
            object(options, option).insert("lex".into(), json!(false));
        }
    }
    object(options, "space").insert("chars".into(), json!(" \t\r"));
}

/// A grammar's text, compiled: what [`register`] keeps, and what a test of
/// a grammar file checks without registering it.
pub struct Compiled {
    spec: GrammarSpec,
}

impl fmt::Debug for Compiled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Compiled({} rules)", self.spec.rule.len())
    }
}

impl Compiled {
    /// A fresh engine with the grammar installed.
    pub fn parser(&self) -> Result<Tabnas, String> {
        let mut parser = Tabnas::new();
        self.spec.install(&mut parser).map_err(|e| e.to_string())?;
        Ok(parser)
    }
}

/// The largest count a repetition may carry (`1024"a"`, `1*1024word`).
/// The compiler writes out every copy: `min` copies of the element, then
/// one nested optional rule per copy up to `max`, so a count in the
/// millions takes gigabytes and minutes before any input is read
/// (`10000000"a"` reached 5 GB; `99999999999999999999999"a"` saturates to
/// 2^64 copies). Measured: `1*1000"a"` compiles in 1 s, `1*5000"a"` in
/// 35 s. Real grammars stay in the hundreds (`1*255`).
pub const MAX_REPEAT: usize = 1024;

/// Compile ABNF text: the compiler's parse, a check of the repetition
/// counts against [`MAX_REPEAT`], then its conversion, with the start rule
/// its default (the grammar's first production) and `word_keywords` on, so
/// a quoted keyword (`"nameserver"`) matches whole words and not the front
/// of a longer one; then the engine options of [`plain_text_options`]; and
/// the result installed once on a fresh engine to be sure it takes. The
/// error is the compiler's own message, or the engine's, on one line and
/// without its colour codes.
pub fn compile(source: &str) -> Result<Compiled, String> {
    let options = AbnfConvertOptions {
        word_keywords: true,
        ..AbnfConvertOptions::default()
    };
    let grammar = tabnas_abnf::parse_abnf(source).map_err(|e| one_line(&e.to_string()))?;
    check_repeats(&grammar)?;
    let mut spec = tabnas_abnf::emit_grammar_spec(&grammar, Some(&options))
        .map_err(|e| one_line(&e.to_string()))?;
    plain_text_options(&mut spec);
    let compiled = Compiled { spec };
    compiled.parser().map_err(|e| one_line(&e))?;
    Ok(compiled)
}

/// A compiler's message as a report carries it: the engine colours its
/// diagnostics and the regex crate's run to several lines, where a
/// `message` is one line of plain text. Escapes are stripped, control
/// characters shown escaped (`\n`).
fn one_line(message: &str) -> String {
    load::escape_controls(&load::strip_ansi(message))
}

/// Refuse a repetition count over [`MAX_REPEAT`] before the compiler
/// writes the copies out. The elements are walked with a stack of their
/// own; the compiler bounds their nesting itself, later.
fn check_repeats(grammar: &tabnas_abnf::Grammar) -> Result<(), String> {
    let mut stack: Vec<&Element> = grammar
        .productions
        .iter()
        .flat_map(|p| p.alts.iter().flatten())
        .collect();
    while let Some(el) = stack.pop() {
        match &el.kind {
            Kind::Rep { min, max, inner } => {
                let count = max.unwrap_or(0).max(*min);
                if count > MAX_REPEAT {
                    let at = match el.sp.and_then(|sp| sp.r.zip(sp.c)) {
                        Some((row, col)) => format!(" at line {row}, column {col}"),
                        None => String::new(),
                    };
                    return Err(format!(
                        "abnf: a repetition count of {count}{at} is more than aless compiles \
                         (at most {MAX_REPEAT}): the compiler writes out every copy, so write \
                         `*word` or `1*word` for a run of any length"
                    ));
                }
                stack.push(inner);
            }
            Kind::Opt { inner } | Kind::Star { inner, .. } | Kind::Plus { inner } => {
                stack.push(inner)
            }
            Kind::Group { alts } => stack.extend(alts.iter().flatten()),
            _ => {}
        }
    }
    Ok(())
}

/// [`compile`] on a thread of its own, within `timeout`. A compile cannot
/// be interrupted, so past the deadline the thread is left to itself (the
/// process exits on the error, which ends it) and the error says so. A
/// panic in the compiler is a `grammar` error, as one in a parse is.
fn compile_within(source: &str, timeout: Option<Duration>) -> Result<Compiled, LoadError> {
    let (tx, rx) = mpsc::channel();
    let text = source.to_string();
    let spawned = std::thread::Builder::new()
        .name("aless-grammar".into())
        .stack_size(load::PARSE_STACK)
        .spawn(move || {
            let _ = tx.send(load::catch_grammar(|| compile(&text)));
        });
    let outcome = match spawned {
        // A time too far off to reach is no limit.
        Ok(_) => match timeout.filter(|t| std::time::Instant::now().checked_add(*t).is_some()) {
            Some(limit) => rx
                .recv_timeout(limit)
                .map_err(|_| LoadError::compile_timed_out(limit))?,
            None => rx
                .recv()
                .map_err(|_| LoadError::tagged("grammar", "the compiler gave no answer"))?,
        },
        // No thread to be had: compile on this one, with no deadline.
        Err(_) => load::catch_grammar(|| compile(source)),
    };
    match outcome {
        Ok(Ok(compiled)) => Ok(compiled),
        Ok(Err(message)) => Err(LoadError::tagged("grammar", message)),
        Err(panic) => Err(LoadError::tagged(
            "grammar",
            format!("the compiler failed: {panic}"),
        )),
    }
}

/// A registered grammar.
pub struct Grammar {
    /// The format's name: the first name given.
    pub name: &'static str,
    /// Every name given, lower-cased: what a file's extension or whole
    /// name is compared with.
    pub matches: Vec<String>,
    /// The ABNF text.
    pub source: String,
    pub origin: Source,
    pub compiled: Compiled,
}

impl Grammar {
    /// A fresh engine with the grammar installed, as every parse gets one.
    pub fn parser(&self) -> Result<Tabnas, String> {
        self.compiled.parser()
    }

    /// Whether `name` (an extension, or a whole file name) is one of this
    /// grammar's, case-insensitively.
    pub fn matches(&self, name: &str) -> bool {
        self.matches.contains(&name.to_ascii_lowercase())
    }
}

/// Why a grammar could not be registered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GrammarError {
    /// The option that named the grammar.
    pub option: &'static str,
    /// The format's name.
    pub grammar: String,
    /// The grammar file, as given; `None` for `--grammar-expr`.
    pub file: Option<String>,
    /// What went wrong: reading the file (`io`, `too_large`), the compile
    /// running past its time (`timeout`), or the compiler's refusal
    /// (`grammar`), which is a mistake in the command.
    pub error: LoadError,
    /// The grammar file's size, when it is a regular file.
    pub size: Option<u64>,
    /// The limits the grammar was read and compiled under.
    pub limits: Limits,
}

impl GrammarError {
    /// Whether the command asked for what cannot be done (the grammar does
    /// not compile), as against the file not being readable or the compile
    /// not finishing in time.
    pub fn is_usage(&self) -> bool {
        !self.error.is_io() && !self.error.is_too_large() && !self.error.is_timeout()
    }
}

impl fmt::Display for GrammarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {}: {}",
            self.option, self.grammar, self.error.message
        )
    }
}

impl std::error::Error for GrammarError {}

/// The grammars registered so far. Each is leaked once, so the name a
/// [`Format`](crate::load::Format) reports can be `&'static str` as the
/// built-in names are.
static REGISTRY: RwLock<Vec<&'static Grammar>> = RwLock::new(Vec::new());

fn registry() -> std::sync::RwLockReadGuard<'static, Vec<&'static Grammar>> {
    REGISTRY.read().unwrap_or_else(|e| e.into_inner())
}

/// Register a grammar: read its file (within the size limit, as any input
/// is), compile it (within the time limit, as any parse runs), and give it
/// a place in the registry. A later grammar with one of the same names is
/// the one that name resolves to. (The error is boxed: it carries a whole
/// report.)
pub fn register(def: Definition, limits: Limits) -> Result<CustomId, Box<GrammarError>> {
    let name = def.names.first().cloned().unwrap_or_default();
    let size = match &def.source {
        Source::File(path) => std::fs::metadata(path)
            .ok()
            .filter(|m| m.is_file())
            .map(|m| m.len()),
        Source::Inline(_) => None,
    };
    let fail = |error: LoadError| {
        Box::new(GrammarError {
            option: def.source.option(),
            grammar: name.clone(),
            file: def.source.file(),
            error,
            size,
            limits,
        })
    };
    let source = match &def.source {
        Source::File(path) => load::read_path_within(path, limits.max_size).map_err(|e| {
            let mut e = if e.is_too_large() {
                // Worded for a grammar file: no parse of it is coming.
                LoadError::grammar_too_large(size, limits.max_size.unwrap_or(0))
                    .with_origin(&load::origin_of(path))
            } else {
                e
            };
            e.message = format!("{}: {}", path.display(), e.message);
            fail(e)
        })?,
        Source::Inline(text) => text.clone(),
    };
    let compiled = compile_within(&source, limits.timeout).map_err(fail)?;
    let mut all = REGISTRY.write().unwrap_or_else(|e| e.into_inner());
    let id = u16::try_from(all.len()).map_err(|_| {
        fail(LoadError::tagged(
            "grammar",
            format!("no room for another grammar: {} are registered", all.len()),
        ))
    })?;
    let grammar: &'static Grammar = Box::leak(Box::new(Grammar {
        name: Box::leak(name.into_boxed_str()),
        matches: def.names.iter().map(|n| n.to_ascii_lowercase()).collect(),
        source,
        origin: def.source,
        compiled,
    }));
    all.push(grammar);
    Ok(CustomId(id))
}

/// Register every definition in turn, stopping at the first that fails.
pub fn register_all(
    defs: impl IntoIterator<Item = Definition>,
    limits: Limits,
) -> Result<Vec<CustomId>, Box<GrammarError>> {
    defs.into_iter().map(|d| register(d, limits)).collect()
}

/// The grammar an id names.
pub fn get(id: CustomId) -> Option<&'static Grammar> {
    registry().get(usize::from(id.0)).copied()
}

/// The grammar a name (a format name, a file's extension or its whole
/// name) selects, case-insensitively: the last registered that has it.
pub fn lookup(name: &str) -> Option<CustomId> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    registry()
        .iter()
        .rposition(|g| g.matches(name))
        .map(|i| CustomId(i as u16))
}

/// Every registered grammar, in registration order.
pub fn registered() -> Vec<(CustomId, &'static Grammar)> {
    registry()
        .iter()
        .enumerate()
        .map(|(i, g)| (CustomId(i as u16), *g))
        .collect()
}

/// The format names of the registered grammars, in registration order.
pub fn names() -> Vec<&'static str> {
    registry().iter().map(|g| g.name).collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::load::Format;

    /// `key = value` lines, one object each; `#` comments are the engine's
    /// and are skipped.
    pub(crate) const KV: &str = "\
settings = *entry            ; @array
entry    = key \"=\" val      ; @object key val
key      = ( TX )
val      = ( TX / NR )
";

    fn inline(names: &str, text: &str) -> Definition {
        Definition::parse("--grammar-expr", &format!("{names}={text}")).unwrap()
    }

    fn parse_json(id: CustomId, input: &str) -> serde_json::Value {
        let g = get(id).unwrap();
        let parser = g.parser().unwrap();
        let value = parser
            .parse(input)
            .unwrap_or_else(|e| panic!("{}: {e}", g.name));
        value.to_json()
    }

    #[test]
    fn definitions_parse_names_and_the_first_equals_sign() {
        let d = Definition::parse("--grammar", "hosts=hosts.abnf").unwrap();
        assert_eq!(d.names, ["hosts"]);
        assert_eq!(d.source, Source::File(PathBuf::from("hosts.abnf")));
        assert_eq!(d.source.option(), "--grammar");
        assert_eq!(d.source.file(), Some("hosts.abnf".to_string()));

        let d = Definition::parse("--grammar", "hosts, hostsfile ,Hosts=etc/hosts.abnf").unwrap();
        assert_eq!(d.names, ["hosts", "hostsfile"], "repeats are dropped");

        // Everything after the first `=` is the text, `=` included.
        let d = Definition::parse("--grammar-expr", "kv=doc = *TX\n").unwrap();
        assert_eq!(d.names, ["kv"]);
        assert_eq!(d.source, Source::Inline("doc = *TX\n".to_string()));
        assert_eq!(d.source.option(), "--grammar-expr");
        assert_eq!(d.source.file(), None);

        for bad in [
            "hosts",
            "=x.abnf",
            "hosts=",
            "hosts= ",
            ",hosts=x.abnf",
            "a b=x.abnf",
            "a/b=x.abnf",
        ] {
            let e = Definition::parse("--grammar", bad).unwrap_err();
            assert!(e.starts_with("--grammar"), "{bad:?}: {e}");
        }
        let e = Definition::parse("--grammar-expr", "kv").unwrap_err();
        assert!(e.contains("NAME=ABNF"), "{e}");
    }

    /// The engine's JSON defaults are off for a grammar of plain text: its
    /// punctuation, its `//` comments, and the number, string and value
    /// tokens the grammar does not name. `#` comments stay. (Measured, so a
    /// change in the compiler or the engine shows up here.)
    #[test]
    fn plain_text_has_the_grammars_tokens_only() {
        const WORDS: &str = "doc = *word   ; @array\nword = ( TX )\n";
        let c = compile(WORDS).unwrap();
        let words = |src: &str| c.parser().unwrap().parse(src).unwrap().to_json();
        assert_eq!(
            words("::1 root:x:0:0 0,30 [a] {b} //server/share # c\ntrue 0 17 \"q\" 'r'"),
            json!([
                "::1",
                "root:x:0:0",
                "0,30",
                "[a]",
                "{b}",
                "//server/share",
                "true",
                "0",
                "17",
                "\"q\"",
                "'r'"
            ])
        );
        // The classes come back when the grammar names them: a quoted string
        // is one token, quotes and spaces included.
        const TYPED: &str = "\
doc  = *( word / num / str / val )   ; @array
word = ( TX )
num  = ( NR )
str  = ( ST )
val  = ( VL )
";
        let c = compile(TYPED).unwrap();
        let value = c
            .parser()
            .unwrap()
            .parse("a 17 \"q r\" true")
            .unwrap()
            .to_json();
        assert_eq!(value, json!(["a", "17", "\"q r\"", "true"]));
        // A literal the grammar names is a token, and cuts a word.
        const FIELDS: &str = "doc = a \":\" b   ; @object a b\na = ( TX )\nb = ( TX )\n";
        let c = compile(FIELDS).unwrap();
        let value = c.parser().unwrap().parse("root:x").unwrap().to_json();
        assert_eq!(value, json!({"a": "root", "b": "x"}));
        // A keyword matches whole words only.
        const KW: &str = "doc = 1*( kw / word )   ; @array\nkw = \"default\" %x20\nword = ( TX )\n";
        let c = compile(KW).unwrap();
        let value = c
            .parser()
            .unwrap()
            .parse("default defaults")
            .unwrap()
            .to_json();
        assert_eq!(value, json!(["default ", "defaults"]));
    }

    /// A line-oriented grammar: the newline it names is a token, so a
    /// repetition stops at the line's end, blank lines and comments cost
    /// nothing, and a CRLF file reads as an LF file.
    #[test]
    fn line_oriented_grammars_read_lf_and_crlf_alike() {
        const HOSTS: &str = "\
hosts   = *( host %x0A / %x0A ) [ host ]   ; @array
host    = address names                    ; @object address names
address = word
names   = 1*word                           ; @array
word    = ( TX )
";
        let want = json!([
            {"address": "127.0.0.1", "names": ["localhost", "myhost"]},
            {"address": "::1", "names": ["localhost"]}
        ]);
        let lf = "# hosts\n\n127.0.0.1 localhost myhost\n\n::1 localhost # v6\n";
        let crlf = lf.replace('\n', "\r\n");
        let c = compile(HOSTS).unwrap();
        for src in [lf, crlf.as_str(), lf.trim_end()] {
            let value = c
                .parser()
                .unwrap()
                .parse(src)
                .unwrap_or_else(|e| panic!("{src:?}: {e}"));
            assert_eq!(value.to_json(), want, "{src:?}");
        }
        // Without the carriage return as a space, the lexer skips `\r\n`
        // whole as a line end before the grammar's `\n` is tried, and the
        // first line never ends.
        let mut spec = tabnas_abnf::abnf_convert(HOSTS, None).unwrap();
        plain_text_options(&mut spec);
        spec.options.insert("space".into(), json!({"chars": " \t"}));
        let mut parser = Tabnas::new();
        spec.install(&mut parser).unwrap();
        let e = parser.parse(&crlf).unwrap_err();
        assert_eq!(e.row, 6, "{e}");
    }

    #[test]
    fn a_grammar_compiles_once_and_parses_to_the_value_it_builds() {
        let id = register(inline("impl-kv-a", KV), Limits::NONE).unwrap();
        let g = get(id).unwrap();
        assert_eq!(g.name, "impl-kv-a");
        assert_eq!(
            parse_json(id, "a=1\nb = two\n# note\nc=3\n"),
            json!([
                {"key": "a", "val": "1"},
                {"key": "b", "val": "two"},
                {"key": "c", "val": "3"}
            ])
        );
        // Without annotations, the compiler's tree.
        let id = register(inline("impl-tree-a", "pair = TX \"=\" TX\n"), Limits::NONE).unwrap();
        let v = parse_json(id, "a=b");
        assert_eq!(v["rule"], "pair");
        assert_eq!(v["src"], "a=b");
        assert_eq!(v["kids"], json!([]));
    }

    #[test]
    fn names_resolve_case_insensitively_and_the_latest_wins() {
        let first = register(inline("impl-hosts-a,impl-hostsfile-a", KV), Limits::NONE).unwrap();
        assert_eq!(lookup("impl-hosts-a"), Some(first));
        assert_eq!(lookup("IMPL-HOSTSFILE-A"), Some(first));
        assert_eq!(lookup(" impl-hosts-a "), Some(first));
        assert_eq!(lookup("impl-nope-a"), None);
        assert_eq!(lookup(""), None);
        let second = register(inline("impl-hostsfile-a", KV), Limits::NONE).unwrap();
        assert_ne!(first, second);
        assert_eq!(lookup("impl-hostsfile-a"), Some(second));
        assert_eq!(lookup("impl-hosts-a"), Some(first));
        assert_eq!(get(first).unwrap().name, "impl-hosts-a");
        assert_eq!(get(second).unwrap().name, "impl-hostsfile-a");
        assert!(names().contains(&"impl-hosts-a"));
        assert!(registered().iter().any(|(id, _)| *id == second));
        // The format names list a name once, however often it is registered.
        register(inline("impl-dup-a", KV), Limits::NONE).unwrap();
        register(inline("IMPL-DUP-A", KV), Limits::NONE).unwrap();
        let dup = |list: &[&str]| {
            list.iter()
                .filter(|n| n.eq_ignore_ascii_case("impl-dup-a"))
                .count()
        };
        assert_eq!(dup(&names()), 2);
        assert_eq!(dup(&Format::known_names()), 1);

        // Through Format: the name, the extension, the whole file name.
        assert_eq!(
            Format::from_name("impl-hosts-a"),
            Some(Format::Custom(first))
        );
        assert_eq!(Format::Custom(first).name(), "impl-hosts-a");
        assert_eq!(Format::Custom(first).to_string(), "impl-hosts-a");
        assert_eq!(
            Format::from_extension("Impl-Hosts-A"),
            Some(Format::Custom(first))
        );
        assert_eq!(
            Format::detect(std::path::Path::new("/etc/impl-hosts-a")),
            Format::Custom(first)
        );
        assert_eq!(
            Format::detect(std::path::Path::new("dir/x.impl-hostsfile-a")),
            Format::Custom(second)
        );
        assert_eq!(
            Format::detect(std::path::Path::new("dir/impl-hosts-a.bak")),
            Format::Text
        );
    }

    #[test]
    fn a_custom_name_shadows_a_built_in() {
        // `webmanifest` is a JSON extension nothing else here relies on.
        assert_eq!(Format::from_extension("webmanifest"), Some(Format::Json));
        let id = register(inline("webmanifest", KV), Limits::NONE).unwrap();
        assert_eq!(
            Format::from_extension("webmanifest"),
            Some(Format::Custom(id))
        );
        assert_eq!(Format::from_name("webmanifest"), Some(Format::Custom(id)));
        assert_eq!(
            Format::detect(std::path::Path::new("app.webmanifest")),
            Format::Custom(id)
        );
        // The built-ins are untouched.
        assert_eq!(Format::from_name("json"), Some(Format::Json));
        assert!(!Format::ALL.iter().any(|f| matches!(f, Format::Custom(_))));
    }

    #[test]
    fn a_grammar_that_does_not_compile_is_the_commands_mistake() {
        let e = register(inline("impl-bad-a", "doc = undefined_rule\n"), Limits::NONE).unwrap_err();
        assert_eq!(e.option, "--grammar-expr");
        assert_eq!(e.grammar, "impl-bad-a");
        assert_eq!(e.file, None);
        assert!(e.is_usage());
        assert_eq!(e.error.code, "grammar");
        assert!(e.error.message.starts_with("abnf"), "{}", e.error.message);
        assert!(
            e.to_string().starts_with("--grammar-expr impl-bad-a: abnf"),
            "{e}"
        );
        assert_eq!(lookup("impl-bad-a"), None, "nothing is registered");
        // A grammar file that cannot be read is an io error.
        let d = Definition::parse("--grammar", "impl-missing-a=/nonexistent/x.abnf").unwrap();
        let e = register(d, Limits::NONE).unwrap_err();
        assert_eq!(e.option, "--grammar");
        assert_eq!(e.file, Some("/nonexistent/x.abnf".to_string()));
        assert!(!e.is_usage());
        assert!(e.error.is_io());
        assert!(
            e.to_string()
                .starts_with("--grammar impl-missing-a: /nonexistent/x.abnf: "),
            "{e}"
        );
        // Or too large for --max-size.
        let dir = std::env::temp_dir().join(format!("aless-grammar-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("big.abnf");
        std::fs::write(&file, KV).unwrap();
        let d = Definition::parse("--grammar", &format!("impl-big-a={}", file.display())).unwrap();
        let e = register(
            d.clone(),
            Limits {
                max_size: Some(8),
                timeout: None,
            },
        )
        .unwrap_err();
        assert!(e.error.is_too_large() && !e.is_usage());
        assert_eq!(e.size, Some(KV.len() as u64));
        assert_eq!(e.limits.max_size, Some(8));
        assert!(
            e.error.message.ends_with(&format!(
                "the grammar file is {} B, over the 8 B limit",
                KV.len()
            )),
            "{}",
            e.error.message
        );
        assert!(
            e.error
                .hint
                .starts_with("A grammar file over --max-size is not read"),
            "{}",
            e.error.hint
        );
        assert!(
            e.error.plain_report().contains("--> "),
            "{}",
            e.error.plain_report()
        );
        // And within it, a file registers.
        let id = register(
            d,
            Limits {
                max_size: Some(1 << 20),
                timeout: None,
            },
        )
        .unwrap();
        assert_eq!(get(id).unwrap().origin, Source::File(file.clone()));
        assert_eq!(get(id).unwrap().source, KV);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A repetition count the compiler would write out in the millions is
    /// refused before it starts, as the command's mistake; one within
    /// [`MAX_REPEAT`] compiles. A count too large for the grammar's
    /// number type saturates, and is refused the same way.
    #[test]
    fn repetition_counts_are_bounded() {
        for (text, count) in [
            ("doc = 10000000\"a\"\n", "10000000".to_string()),
            ("doc = *99999999\"a\"\n", "99999999".to_string()),
            ("doc = 1*1000000000\"a\"\n", "1000000000".to_string()),
            (
                "doc = 99999999999999999999999\"a\"\n",
                usize::MAX.to_string(),
            ),
            ("doc = word\nword = ( 2000TX )\n", "2000".to_string()),
            ("doc = 1025\"a\"\n", "1025".to_string()),
        ] {
            let e = compile(text).unwrap_err();
            assert!(
                e.starts_with(&format!("abnf: a repetition count of {count}")),
                "{text:?}: {e}"
            );
            assert!(e.contains("at most 1024") && e.contains("`*word`"), "{e}");
        }
        for text in [
            "doc = 1024\"a\"\n",
            "doc = 1*255\"a\"\n",
            "doc = *word\nword = ( TX )\n",
        ] {
            compile(text).unwrap_or_else(|e| panic!("{text:?}: {e}"));
        }
        let e = register(inline("impl-rep-a", "doc = 5000\"a\"\n"), Limits::NONE).unwrap_err();
        assert!(e.is_usage());
        assert_eq!(e.error.code, "grammar");
        assert!(
            e.to_string()
                .starts_with("--grammar-expr impl-rep-a: abnf: a repetition count of 5000"),
            "{e}"
        );
    }

    /// The compile runs within the time limit a parse would, on a thread
    /// of its own: past it, the grammar is a `timeout` error, and the
    /// compiler is left to finish on its own.
    #[test]
    fn a_compile_past_the_time_limit_is_a_timeout() {
        // Some 300 nested optionals take the compiler well over a
        // millisecond (`1*1000"a"` takes about a second in a release build).
        let slow = inline("impl-slow-a", "doc = 1*300\"a\"\n");
        let limits = Limits {
            max_size: None,
            timeout: Some(Duration::from_millis(1)),
        };
        let e = register(slow, limits).unwrap_err();
        assert!(!e.is_usage());
        assert!(e.error.is_timeout(), "{}", e.error.message);
        assert_eq!(e.limits, limits);
        assert_eq!(
            e.to_string(),
            "--grammar-expr impl-slow-a: timeout: the grammar took longer than 0.001 s to compile"
        );
        assert!(
            e.error.hint.contains("--timeout 0 for no limit"),
            "{}",
            e.error.hint
        );
        assert!(e.error.plain_report().starts_with("[aless/timeout]"));
        assert_eq!(lookup("impl-slow-a"), None);
        // With time enough, a grammar registers under a limit.
        let limits = Limits {
            max_size: None,
            timeout: Some(Duration::from_secs(600)),
        };
        assert!(register(inline("impl-slow-b", "doc = 1*20\"a\"\n"), limits).is_ok());
    }

    /// A compiler message is one line of plain text: the engine's colour
    /// codes are stripped and control characters shown escaped, as a
    /// report's `message` is for a parse error.
    #[test]
    fn compiler_messages_are_one_plain_line() {
        for text in ["doc = *\n", "doc = \"a\u{1}b\"\n", "doc = %x7E-21\n"] {
            let e = compile(text).unwrap_err();
            assert!(!e.contains('\u{1b}'), "{text:?}: {e:?}");
            assert!(!e.chars().any(char::is_control), "{text:?}: {e:?}");
            assert!(e.starts_with("abnf: "), "{text:?}: {e}");
        }
        let e = compile("doc = *\n").unwrap_err();
        assert!(
            e.contains("[tabnas/unexpected]: unexpected character(s): *"),
            "{e}"
        );
        let e = compile("doc = %x7E-21\n").unwrap_err();
        assert!(e.contains("regex parse error:\\n"), "{e}");
    }
}
