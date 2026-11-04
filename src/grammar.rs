//! Custom grammars: ABNF from `--grammar` and `--grammar-expr`, compiled once
//! at startup into a process-wide registry that
//! [`Format::Custom`](crate::load::Format::Custom) indexes. The registry only
//! grows, so tests register grammars under names of their own.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::PathBuf;
use std::sync::{mpsc, RwLock};
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};
use tabnas::Tabnas;
use tabnas_abnf::{AbnfConvertOptions, Element, GrammarSpec, Kind};

use crate::load::{self, Limits, LoadError};

/// Only [`register`] makes one, so every id names a grammar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CustomId(u16);

/// Where a grammar's text came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    File(PathBuf),
    Inline(String),
}

impl Source {
    pub fn option(&self) -> &'static str {
        match self {
            Source::File(_) => "--grammar",
            Source::Inline(_) => "--grammar-expr",
        }
    }

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
    /// The first is the format's name; matching is case-insensitive.
    pub names: Vec<String>,
    pub source: Source,
}

impl Definition {
    /// `NAME[,NAME…]=FILE` or `NAME[,NAME…]=ABNF`: the first `=` ends the
    /// names, as ABNF text contains `=` itself.
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
            if !seen
                .iter()
                .any(|s: &String| s.to_lowercase() == name.to_lowercase())
            {
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

    /// [`Definition::parse`], with FILE keeping bytes that are not UTF-8.
    pub fn parse_os(option: &str, value: &OsStr) -> Result<Definition, String> {
        let mut def = Definition::parse(option, &value.to_string_lossy())?;
        if let Source::File(path) = &mut def.source {
            *path = PathBuf::from(after_eq(value));
        }
        Ok(def)
    }
}

/// What follows the first `=`, bytes as given: `=` is ASCII, so the raw value
/// and its lossy text cut at the same place.
pub fn after_eq(raw: &OsStr) -> OsString {
    let bytes = raw.as_encoded_bytes();
    let at = bytes
        .iter()
        .position(|&b| b == b'=')
        .map_or(bytes.len(), |i| i + 1);
    // SAFETY: the cut is just after an ASCII `=`, and the encoding allows a
    // split before or after any non-empty valid UTF-8 substring.
    unsafe { OsStr::from_encoded_bytes_unchecked(&bytes[at..]) }.to_owned()
}

/// Turn off the engine's JSON lexing defaults for a grammar of plain text, as
/// "What aless sets" in tests/fixtures/grammars/README.md lists them.
pub(crate) fn plain_text_options(spec: &mut GrammarSpec) {
    fn object<'a>(map: &'a mut Map<String, Value>, key: &str) -> &'a mut Map<String, Value> {
        let value = map.entry(key).or_insert_with(|| json!({}));
        if !value.is_object() {
            *value = json!({});
        }
        value.as_object_mut().expect("made an object above")
    }
    // A token the grammar names appears quoted anywhere in its spec (`"#NR"`).
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

/// A grammar's text, compiled, which a test can check without registering it.
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

/// The largest count a repetition may carry (`1*1024word`). The compiler writes
/// out every copy, so a count in the millions takes gigabytes and minutes.
pub const MAX_REPEAT: usize = 1024;

/// The most rules a grammar's repetitions may have the compiler write, as
/// [`repeat_rules`] counts them. Only `--timeout` bounds the cost of other shapes.
pub const MAX_REPEAT_TOTAL: usize = 1024;

/// The element nesting the compiler walks (tabnas-bnf's MAX_ELEMENT_DEPTH, which
/// tabnas-abnf does not re-export); [`repeat_rules`] rewrites no deeper grammar.
const COMPILER_ELEMENT_DEPTH: usize = 128;

/// Compile ABNF text, bounding its repetitions first and installing the result
/// once to be sure it takes. `word_keywords` makes `"nameserver"` match whole
/// words only, not the front of a longer one.
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
    spec.install(&mut Tabnas::new())
        .map_err(|e| one_line(&e.to_string()))?;
    Ok(Compiled { spec })
}

/// The engine colours its diagnostics and the regex crate's run to several lines.
fn one_line(message: &str) -> String {
    load::escape_controls(&load::strip_ansi(message))
}

/// Refuse a count over [`MAX_REPEAT`], named first wherever it is, or more than
/// [`MAX_REPEAT_TOTAL`] rules in all. The walk keeps its own stack, as the
/// nesting is unbounded until the compiler checks it.
fn check_repeats(grammar: &tabnas_abnf::Grammar) -> Result<(), String> {
    let mut stack: Vec<(&Element, usize)> = grammar
        .productions
        .iter()
        .flat_map(|p| p.alts.iter().flatten())
        .map(|el| (el, 1))
        .collect();
    let mut deepest = 0;
    while let Some((el, depth)) = stack.pop() {
        deepest = deepest.max(depth);
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
                stack.push((inner, depth + 1));
            }
            Kind::Opt { inner } | Kind::Star { inner, .. } | Kind::Plus { inner } => {
                stack.push((inner, depth + 1))
            }
            Kind::Group { alts } => stack.extend(alts.iter().flatten().map(|el| (el, depth + 1))),
            _ => {}
        }
    }
    let rules = repeat_rules(grammar, deepest);
    if rules > MAX_REPEAT_TOTAL {
        return Err(format!(
            "abnf: the repetitions would have the compiler write {rules} rules, more than aless \
             compiles (at most {MAX_REPEAT_TOTAL}): it writes a rule or two for every copy a \
             repetition may make past its minimum, and writes a rule's repetitions again into \
             every alternative that starts with that rule, so write `*word` or `1*word` for a \
             run of any length"
        ));
    }
    Ok(())
}

/// About how many rules the compiler writes for the repetitions, counted on
/// its own left-recursion rewrite (`eliminate_left_recursion`), which copies a
/// rule an alternative starts with into it once per alternative of that rule.
/// A grammar deeper than [`COMPILER_ELEMENT_DEPTH`] is counted as written.
fn repeat_rules(grammar: &tabnas_abnf::Grammar, deepest: usize) -> usize {
    let rules: std::collections::HashSet<&str> = grammar
        .productions
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    let rewritten = (deepest <= COMPILER_ELEMENT_DEPTH)
        .then(|| tabnas_abnf::eliminate_left_recursion(grammar).ok())
        .flatten();
    let productions = rewritten
        .as_ref()
        .map_or(&grammar.productions, |g| &g.productions);
    productions
        .iter()
        .flat_map(|p| &p.alts)
        .fold(0usize, |sum, alt| {
            sum.saturating_add(alternative_repeat_rules(alt, &rules))
        })
}

/// Counts the rest of an alternative twice after an optional group ending in a
/// terminal (`[ X "@" ] Y`), which the compiler may split into `X "@" Y` and Y.
fn alternative_repeat_rules(alt: &[Element], rules: &std::collections::HashSet<&str>) -> usize {
    let probe_prefix = |el: &Element| match &el.kind {
        Kind::Opt { inner } => matches!(&inner.kind, Kind::Group { alts }
            if alts.len() == 1
                && alts[0].len() >= 2
                && alts[0].last().is_some_and(Element::is_terminal)),
        _ => false,
    };
    let mut total = 0usize;
    let mut twice = false;
    for (i, el) in alt.iter().enumerate() {
        let n = element_repeat_rules(el, rules);
        total = total.saturating_add(if twice { n.saturating_mul(2) } else { n });
        twice = twice || (i + 1 < alt.len() && probe_prefix(el));
    }
    total
}

/// As the compiler desugars `m*nX`: one rule, two for each copy past `m` (an
/// optional and its group), and a step for every copy when X is not a
/// terminal; `m*X` writes one for its tail. X is written once, so nesting adds.
fn element_repeat_rules(el: &Element, rules: &std::collections::HashSet<&str>) -> usize {
    let terminal = |el: &Element| match &el.kind {
        Kind::Term { .. } | Kind::Regex { .. } | Kind::Token { .. } | Kind::Prose { .. } => true,
        // A name that is no rule of the grammar is an engine token (`TX`).
        Kind::Ref { name, .. } => !rules.contains(name.as_str()),
        _ => false,
    };
    let mut total = 0usize;
    let mut stack = vec![el];
    while let Some(el) = stack.pop() {
        match &el.kind {
            Kind::Rep { min, max, inner } => {
                let (chain, beyond) = match max {
                    Some(max) => {
                        let beyond = max.saturating_sub(*min);
                        (beyond.saturating_mul(2), beyond)
                    }
                    None => (1, 1),
                };
                let steps = if terminal(inner) {
                    0
                } else {
                    min.saturating_add(beyond)
                };
                total = total
                    .saturating_add(1)
                    .saturating_add(chain)
                    .saturating_add(steps);
                stack.push(inner);
            }
            Kind::Opt { inner } | Kind::Star { inner, .. } | Kind::Plus { inner } => {
                stack.push(inner)
            }
            Kind::Group { alts } => stack.extend(alts.iter().flatten()),
            _ => {}
        }
    }
    total
}

/// [`compile`] on a thread of its own, within `timeout`. A compile cannot be
/// interrupted, so past the deadline the thread is left to the process's exit.
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
        Err(_) => return compile_here(source, timeout),
    };
    settle(outcome)
}

/// [`compile`] on this thread, when no other can be had. It cannot be stopped,
/// so it is timed: one ending past `timeout`, even with an error, is a `timeout`.
fn compile_here(source: &str, timeout: Option<Duration>) -> Result<Compiled, LoadError> {
    let start = Instant::now();
    let outcome = load::catch_grammar(|| compile(source));
    let took = start.elapsed();
    if let Some(limit) = timeout.filter(|t| took >= *t) {
        return Err(LoadError::compile_finished_late(limit, took));
    }
    settle(outcome)
}

fn settle(outcome: Result<Result<Compiled, String>, String>) -> Result<Compiled, LoadError> {
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
    pub name: &'static str,
    /// Every name, lower-cased by Unicode's rules (`--help`'s "in any case").
    pub matches: Vec<String>,
    pub source: String,
    pub origin: Source,
    pub compiled: Compiled,
}

impl Grammar {
    pub fn parser(&self) -> Result<Tabnas, String> {
        self.compiled.parser()
    }

    pub fn matches(&self, name: &str) -> bool {
        self.matches.contains(&name.to_lowercase())
    }
}

/// Why a grammar could not be registered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GrammarError {
    pub option: &'static str,
    pub grammar: String,
    pub file: Option<String>,
    /// Reading the file (`io`, `too_large`), the compile's time (`timeout`), or
    /// the compiler's refusal (`grammar`), which is a mistake in the command.
    pub error: LoadError,
    pub size: Option<u64>,
    pub limits: Limits,
}

impl GrammarError {
    /// The grammar does not compile, as against an unreadable file or a slow compile.
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

/// Each grammar is leaked once, so its name can be `&'static str` as the
/// built-in [`Format`](crate::load::Format) names are.
static REGISTRY: RwLock<Vec<&'static Grammar>> = RwLock::new(Vec::new());

fn registry() -> std::sync::RwLockReadGuard<'static, Vec<&'static Grammar>> {
    REGISTRY.read().unwrap_or_else(|e| e.into_inner())
}

/// Read within the size limit and compile within the time limit, as any input.
/// The error is boxed: it carries a whole report.
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
        matches: def.names.iter().map(|n| n.to_lowercase()).collect(),
        source,
        origin: def.source,
        compiled,
    }));
    all.push(grammar);
    Ok(CustomId(id))
}

pub fn register_all(
    defs: impl IntoIterator<Item = Definition>,
    limits: Limits,
) -> Result<Vec<CustomId>, Box<GrammarError>> {
    defs.into_iter().map(|d| register(d, limits)).collect()
}

pub fn get(id: CustomId) -> Option<&'static Grammar> {
    registry().get(usize::from(id.0)).copied()
}

/// The last registered grammar a name, an extension or a whole file name selects.
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

pub fn registered() -> Vec<(CustomId, &'static Grammar)> {
    registry()
        .iter()
        .enumerate()
        .map(|(i, g)| (CustomId(i as u16), *g))
        .collect()
}

pub fn names() -> Vec<&'static str> {
    registry().iter().map(|g| g.name).collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::load::Format;

    /// `key = value` lines; `#` comments are the engine's, and skipped.
    pub(crate) const KV: &str = "\
settings = *entry            ; @array
entry    = key \"=\" val      ; @object key val
key      = ( TX )
val      = ( TX / NR )
";

    #[cfg(unix)]
    #[test]
    fn a_file_keeps_path_bytes_that_are_not_utf8() {
        use std::os::unix::ffi::{OsStrExt, OsStringExt};
        let raw = OsStr::from_bytes(b"hosts=g\xff/x=y.abnf");
        let d = Definition::parse_os("--grammar", raw).unwrap();
        assert_eq!(d.names, ["hosts"]);
        let file = PathBuf::from(OsString::from_vec(b"g\xff/x=y.abnf".to_vec()));
        assert_eq!(d.source, Source::File(file));
        let d = Definition::parse_os("--grammar-expr", OsStr::new("kv=doc = *TX\n")).unwrap();
        assert_eq!(d.source, Source::Inline("doc = *TX\n".into()));
        // The attached form cuts at the argument's first `=`.
        assert_eq!(
            after_eq(OsStr::from_bytes(b"--grammar=hosts=g\xff.abnf")).as_bytes(),
            b"hosts=g\xff.abnf"
        );
    }

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
        let d = Definition::parse("--grammar", "Ärger,ÄRGER=x.abnf").unwrap();
        assert_eq!(d.names, ["Ärger"], "in any case, not only ASCII");

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

    /// Measured: a change in the compiler's or the engine's lexing shows here.
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
        const FIELDS: &str = "doc = a \":\" b   ; @object a b\na = ( TX )\nb = ( TX )\n";
        let c = compile(FIELDS).unwrap();
        let value = c.parser().unwrap().parse("root:x").unwrap().to_json();
        assert_eq!(value, json!({"a": "root", "b": "x"}));
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
        // With `\r` not a space, the lexer skips `\r\n` whole as a line end
        // before the grammar's `\n` is tried, and the first line never ends.
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
        let umlaut = register(inline("Impl-Ärger-a", KV), Limits::NONE).unwrap();
        assert_eq!(lookup("impl-ärger-a"), Some(umlaut));
        assert_eq!(lookup("IMPL-ÄRGER-A"), Some(umlaut));
        assert_eq!(
            Format::from_name("impl-ärger-a"),
            Some(Format::Custom(umlaut))
        );
        assert_eq!(
            Format::detect(std::path::Path::new("x.IMPL-ÄRGER-A")),
            Format::Custom(umlaut)
        );
        assert_eq!(Format::Custom(umlaut).name(), "Impl-Ärger-a");
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

    /// A count too large for the grammar's number type saturates, and is refused too.
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

    #[test]
    fn repetition_rules_are_bounded_in_total() {
        // Under a time limit, so that an unbounded compile fails in seconds.
        let many = inline(
            "impl-rep-total",
            &format!("doc ={}\n", " 1*1024\"a\"".repeat(16)),
        );
        let limits = Limits {
            max_size: None,
            timeout: Some(Duration::from_secs(10)),
        };
        let e = register(many, limits).unwrap_err();
        assert!(
            e.is_usage() && e.error.code == "grammar",
            "{}",
            e.error.message
        );
        // 16 x (1 + 2 x 1023).
        assert!(
            e.error
                .message
                .contains("the repetitions would have the compiler write 32752 rules"),
            "{}",
            e.error.message
        );
        assert!(
            e.error.message.contains("at most 1024") && e.error.message.contains("`*word`"),
            "{}",
            e.error.message
        );
        assert_eq!(lookup("impl-rep-total"), None);
        // A rule an alternative starts with is written into it, once per rule: 3 x 599.
        let e = compile("doc = s \"x\"\ns = r \"x\"\nr = 1*300\"q\"\n").unwrap_err();
        assert!(e.contains("write 1797 rules"), "{e}");
        // Named alone, or after the start of an alternative, it is not.
        for text in [
            "doc = r\nr = 1*300\"q\"\n",
            "doc = \"x\" r r\nr = 1*300\"q\"\n",
        ] {
            compile(text).unwrap_or_else(|e| panic!("{text:?}: {e}"));
        }
        // The rest is written once per alternative of the rule it starts with.
        let e = compile("doc = r 1*300\"x\"\nr = \"a\" / \"b\"\n").unwrap_err();
        assert!(e.contains("write 1198 rules"), "{e}");
        // 599 inside the group, and 1 + 2 x 299 + 300 outside: a step per copy.
        let e = compile("doc = 1*300(1*300\"a\" \"x\")\n").unwrap_err();
        assert!(e.contains("write 1498 rules"), "{e}");
        // What follows an optional prefix the compiler may branch on is doubled.
        let e = compile("doc = [\"a\" \"@\"] 1*300\"a\"\n").unwrap_err();
        assert!(e.contains("write 1198 rules"), "{e}");
        let e = compile("doc = 1024\"a\" 1024\"a\" 5000\"a\"\n").unwrap_err();
        assert!(e.contains("a repetition count of 5000"), "{e}");
        let e = compile("doc = 5000\"a\" 1024\"a\" 1024\"a\"\n").unwrap_err();
        assert!(e.contains("a repetition count of 5000"), "{e}");
        // Copies of a terminal up to the minimum write no rules.
        compile("doc = 1024\"a\" 1024\"b\" 1024\"c\"\n").unwrap();
    }

    /// [`repeat_rules`] against what the compiler writes, for each way it writes
    /// a repetition: the compiler adds at most a few rules of the grammar's own.
    #[test]
    fn repetition_rules_are_counted_as_the_compiler_writes_them() {
        for text in [
            "doc = 1*40\"a\"\n",
            "doc = 1*40TX\n",
            "doc = 3*40\"a\" *\"b\" 4*\"c\"\n",
            "doc = r \"x\"\nr = 1*40\"q\"\n",
            "doc = s \"x\"\ns = r \"x\"\nr = 1*40\"q\"\n",
            "doc = r 1*40\"x\"\nr = \"a\" / \"b\" / \"c\"\n",
            "doc = 1*20(1*20\"a\" \"x\")\n",
            "doc = 40r\nr = \"a\" \"b\"\n",
            "doc = 40(40\"a\")\n",
            "doc = [\"a\" \"@\"] 1*40\"a\"\n",
            "doc = 0*20\"x\" doc 1*20\"y\" / \"z\"\n",
            "a = b \"x\" / \"c\"\nb = a 1*20\"y\" / \"d\"\n",
        ] {
            let grammar = tabnas_abnf::parse_abnf(text).unwrap();
            let counted = repeat_rules(&grammar, 1);
            let written = compile(text)
                .unwrap_or_else(|e| panic!("{text:?}: {e}"))
                .spec
                .rule
                .len();
            assert!(
                counted <= written && written <= counted + 16,
                "{text:?}: counted {counted}, the compiler wrote {written}"
            );
        }
        let chain =
            tabnas_abnf::parse_abnf("doc = s \"x\"\ns = r \"x\"\nr = 1*300\"q\"\n").unwrap();
        assert_eq!(repeat_rules(&chain, 1), 1797);
        assert_eq!(repeat_rules(&chain, COMPILER_ELEMENT_DEPTH + 1), 599);
        let deep = format!(
            "doc = {}1*3\"a\"{}\n",
            "(".repeat(COMPILER_ELEMENT_DEPTH),
            ")".repeat(COMPILER_ELEMENT_DEPTH)
        );
        let e = compile(&deep).unwrap_err();
        assert!(e.contains("more than 128 deep"), "{e}");
    }

    #[test]
    fn a_compile_past_the_time_limit_is_a_timeout() {
        // `1*300"a"` takes the compiler well over a millisecond.
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
        assert!(
            e.error
                .hint
                .starts_with("The compiler cannot be interrupted, so aless stopped waiting"),
            "{}",
            e.error.hint
        );
        assert!(e.error.plain_report().starts_with("[aless/timeout]"));
        assert_eq!(lookup("impl-slow-a"), None);
        let limits = Limits {
            max_size: None,
            timeout: Some(Duration::from_secs(600)),
        };
        assert!(register(inline("impl-slow-b", "doc = 1*20\"a\"\n"), limits).is_ok());
    }

    #[test]
    fn a_compile_on_this_thread_past_the_time_limit_is_a_timeout() {
        let slow = "doc = 1*300\"a\"\n";
        let e = compile_here(slow, Some(Duration::from_millis(1)))
            .err()
            .unwrap();
        assert!(e.is_timeout(), "{}", e.message);
        assert_eq!(
            e.message,
            "timeout: the grammar took longer than 0.001 s to compile"
        );
        assert!(
            e.hint.starts_with(
                "No thread could be started for the compiler, so it ran to its end on \
                 aless's own and took "
            ),
            "{}",
            e.hint
        );
        assert!(!e.hint.contains("stopped waiting"), "{}", e.hint);
        // A late refusal is late too: the time is the answer, not the error.
        let late_error = "doc = 1*300\"a\"\nbad = 99999999999999999999999\"a\"\n";
        let e = compile_here(late_error, Some(Duration::from_nanos(1)))
            .err()
            .unwrap();
        assert!(e.is_timeout(), "{}", e.message);
        assert!(compile_here("doc = 1*20\"a\"\n", Some(Duration::from_secs(600))).is_ok());
        assert!(compile_here("doc = 1*20\"a\"\n", None).is_ok());
        let e = compile_here("doc = \"a\n", Some(Duration::from_secs(600)))
            .err()
            .unwrap();
        assert_eq!(e.code, "grammar");
    }

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
