//! Colour for text: a pane showing its text colours each token as the
//! text's grammar lexed it, through tabnas-lsp's semantic tokens, the ones
//! its language server sends an editor.
//!
//! Colouring parses the whole text and keeps its lex trace, which at its
//! peak holds a few hundred times the text: 270 MB and a second of a
//! release build for 0.9 MB of JSON. So it runs off the viewer's thread:
//! one worker takes the texts in turn, each on a thread of its own with
//! the stack a parse needs, under the parse timeout and aless's depth cap,
//! and only up to [`MAX_BYTES`]. A text no pane shows any more is skipped,
//! or called off. The text shows uncoloured until its colours come in. A grammar the registry
//! marks as lexing speculatively (its trace holds tokens the parse took
//! back) is never coloured, and neither is plain text.

use std::collections::{HashMap, HashSet, VecDeque};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use tabnas::Tabnas;
use tabnas_lsp::{Entry, Registry, Span, TokenType};

use crate::load::{self, CatchGuard, Deadline, Format};

/// The longest text coloured: 512 KiB, which peaks near 150 MB while its
/// colours are made.
pub const MAX_BYTES: usize = 512 << 10;

/// The longest one text's colours may take, or the parse timeout when
/// that is shorter: colours are worth a wait, not a worker held by a slow
/// grammar while other texts queue. A parse stopped here colours what it
/// had lexed.
pub const MAX_TIME: Duration = Duration::from_secs(10);

/// What a text is lexed by: a format's grammar, or alchemy's for a
/// program.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Grammar {
    Format(Format),
    Alchemy,
}

impl Grammar {
    /// The registry's entry for this grammar, when it has one: a format's
    /// language id is its name. A custom grammar's name is the user's, so
    /// it is never looked up.
    fn entry(self, registry: &Registry) -> Option<&Entry> {
        match self {
            Grammar::Format(f) if !f.is_custom() => registry.entry(f.name()),
            _ => None,
        }
    }

    /// Whether a text lexed by this grammar can be coloured: it has a
    /// grammar, and `registry` does not mark its lex stream speculative.
    fn colourable_by(self, registry: &Registry) -> bool {
        self != Grammar::Format(Format::Text) && self.entry(registry).is_none_or(Entry::is_clean)
    }

    /// [`Grammar::colourable_by`] the registry tabnas-lsp bundles.
    pub fn colourable(self) -> bool {
        self.colourable_by(Registry::bundled())
    }

    fn parser(self) -> Option<Tabnas> {
        match self {
            Grammar::Format(format) => load::make_parser(format).ok().flatten(),
            Grammar::Alchemy => Some(tabnas_alchemy::grammar::make()),
        }
    }
}

/// The most colourings left running, called off or out of time and still
/// making their parsers: with that many, a text shown waits in the queue
/// until one ends, while the worker takes in newer texts and skips hidden
/// ones. Each holds a thread with the parse stack and a grammar's install,
/// so a run of slow grammars hidden in turn cannot pile them up.
const MAX_LEFT: usize = 2;

/// How long past a text's deadline the worker still waits for its
/// colours: a parse stops itself at the deadline, and what it had lexed
/// is still worth colouring once its spans are made.
const GRACE: Duration = Duration::from_secs(1);

/// A text's colours, line by line as [`load::lines`] splits it: each
/// line's runs, in order and not overlapping.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Painted {
    lines: Vec<Vec<Run>>,
}

/// A coloured byte range of one line, and what the grammar lexed there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Run {
    pub start: usize,
    pub end: usize,
    pub kind: TokenType,
}

impl Painted {
    /// Line `n`'s runs.
    pub fn line(&self, n: usize) -> &[Run] {
        self.lines.get(n).map_or(&[], Vec::as_slice)
    }

    /// lsp's spans over `text[shift..]`, none of which crosses a line, as
    /// runs of `text`'s lines.
    fn from_spans(text: &str, shift: usize, spans: &[Span]) -> Painted {
        let starts: Vec<usize> = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(i, _)| i + 1))
            .collect();
        let mut lines = vec![Vec::new(); starts.len()];
        for span in spans {
            let (start, end) = (span.start + shift, span.end + shift);
            let n = starts.partition_point(|&s| s <= start) - 1;
            let base = starts[n];
            lines[n].push(Run {
                start: start - base,
                end: end - base,
                kind: span.kind,
            });
        }
        Painted { lines }
    }
}

/// Colour `text` as `grammar` lexes it, on this thread: the worker's
/// work. The parse stops at `timeout` or past aless's depth cap, and the
/// tokens lexed by then are coloured. A text too long to colour, one
/// without a grammar, and a grammar that panics colour nothing.
pub fn paint(grammar: Grammar, text: &str, timeout: Option<Duration>) -> Option<Painted> {
    paint_unless(grammar, text, timeout, None)
}

/// [`paint`], stopped as it would be at its deadline once `stop` is raised.
fn paint_unless(
    grammar: Grammar,
    text: &str,
    timeout: Option<Duration>,
    stop: Option<Arc<AtomicBool>>,
) -> Option<Painted> {
    if text.len() > MAX_BYTES || !grammar.colourable() {
        return None;
    }
    // The time starts before the parser is made, as the loader's does:
    // installing a custom grammar can itself take seconds.
    let deadline = match stop {
        Some(stop) => Deadline::after(timeout).map(|d| d.or_cancelled(stop)),
        None => Deadline::after(timeout),
    };
    let mut parser = grammar.parser()?;
    let _stopped = load::guard(&mut parser, deadline, load::MAX_RULE_DEPTH, || {});
    // The loader parses a text without its byte-order mark, and so does
    // this; the runs still count from the text's first byte.
    let (body, shift) = match text.strip_prefix('\u{feff}') {
        Some(body) => (body, '\u{feff}'.len_utf8()),
        None => (text, 0),
    };
    let overrides = grammar
        .entry(Registry::bundled())
        .and_then(Entry::overrides);
    let _guard = CatchGuard::enter();
    catch_unwind(AssertUnwindSafe(|| {
        let highlight = tabnas_lsp::highlight(parser, body, overrides);
        Painted::from_spans(text, shift, &highlight.spans)
    }))
    .ok()
}

/// A painting's key: the tab, its generation and the grammar. A tab's
/// generation moves with every text it takes, so a key names one text.
pub type Key = (u64, u64, Grammar);

/// Colours made off the viewer's thread, by one worker for every pane,
/// which takes the texts in turn. It starts with the first text asked for.
#[derive(Default)]
pub struct Painter {
    /// Where texts go and colours come back.
    worker: Option<Worker>,
    /// No worker could be started, so nothing is coloured.
    failed: bool,
    /// The newest painting of each tab: the key it was made for, and its
    /// colours, `None` for a text that is not coloured.
    done: HashMap<u64, (Key, Option<Arc<Painted>>)>,
    /// The painting each tab waits for.
    asked: HashMap<u64, Key>,
    /// What the panes show, shared with the worker.
    shown: Arc<Mutex<Shown>>,
    /// The colouring threads running, the one waited for and those left
    /// behind, shared with the worker.
    running: Arc<AtomicUsize>,
    /// How long one text's parse may take.
    timeout: Option<Duration>,
}

struct Worker {
    jobs: Sender<Job>,
    done: Receiver<(Key, Made)>,
}

/// What the panes show, as the painter and its worker share it: the
/// worker skips a job whose text is not among `keys` when its turn comes,
/// and the painter calls off the one `running` once its text leaves them.
#[derive(Default)]
struct Shown {
    /// The texts the panes show now.
    keys: HashSet<Key>,
    /// The text the worker is colouring, and the flag that stops it.
    running: Option<(Key, Arc<AtomicBool>)>,
}

/// What the worker made of a job.
enum Made {
    /// The text's colours: `None` for a text that is not coloured.
    Colours(Option<Painted>),
    /// Nothing: no pane showed the text when its turn came, or it was
    /// called off once none did.
    Skipped,
}

struct Job {
    key: Key,
    text: String,
    timeout: Option<Duration>,
}

impl Painter {
    /// A painter whose parses stop after `timeout`, or [`MAX_TIME`] when
    /// that is sooner or there is none.
    pub fn new(timeout: Option<Duration>) -> Painter {
        Painter {
            timeout: Some(timeout.map_or(MAX_TIME, |t| t.min(MAX_TIME))),
            ..Painter::default()
        }
    }

    /// The colours for `key`, once they are in.
    pub fn get(&self, key: Key) -> Option<Arc<Painted>> {
        match self.done.get(&key.0) {
            Some((k, painted)) if *k == key => painted.clone(),
            _ => None,
        }
    }

    /// Whether `key` is painted or on its way.
    pub fn has(&self, key: Key) -> bool {
        self.done.get(&key.0).is_some_and(|(k, _)| *k == key)
            || self.asked.get(&key.0) == Some(&key)
    }

    /// Ask for the colours of `text`, the text `key` names, unless they
    /// are in or on their way. A text that cannot be coloured is settled
    /// at once, as uncoloured.
    pub fn ask(&mut self, key: Key, text: &str) {
        if self.has(key) {
            return;
        }
        if text.len() > MAX_BYTES || !key.2.colourable() {
            self.done.insert(key.0, (key, None));
            return;
        }
        let timeout = self.timeout;
        let sent = self.worker().is_some_and(|worker| {
            let job = Job {
                key,
                text: text.to_string(),
                timeout,
            };
            worker.jobs.send(job).is_ok()
        });
        if sent {
            self.asked.insert(key.0, key);
        } else {
            self.done.insert(key.0, (key, None));
        }
    }

    /// Take in the colours that have come: true when any did. Colours for
    /// a text its tab has since replaced are dropped. A worker that has
    /// gone leaves every text it had uncoloured, and nothing more is
    /// asked of it.
    pub fn collect(&mut self) -> bool {
        let Some(worker) = &self.worker else {
            return false;
        };
        let mut any = false;
        loop {
            match worker.done.try_recv() {
                Ok((key, made)) => {
                    if self.asked.get(&key.0) == Some(&key) {
                        self.asked.remove(&key.0);
                        // A skipped text is asked for again if it shows.
                        if let Made::Colours(painted) = made {
                            self.done.insert(key.0, (key, painted.map(Arc::new)));
                        }
                        any = true;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    for (id, key) in self.asked.drain() {
                        self.done.insert(id, (key, None));
                    }
                    self.worker = None;
                    self.failed = true;
                    break;
                }
            }
        }
        any
    }

    /// Name the texts the panes show now, the ones whose colours are worth
    /// making. A job for any other is skipped when its turn comes, and the
    /// one being coloured is called off, so a tab left or closed never
    /// holds up the one in view.
    pub fn want(&mut self, keys: impl IntoIterator<Item = Key>) {
        let mut shown = self.shown.lock().unwrap_or_else(PoisonError::into_inner);
        shown.keys.clear();
        shown.keys.extend(keys);
        if let Some((key, stop)) = &shown.running {
            if !shown.keys.contains(key) {
                stop.store(true, Ordering::Relaxed);
            }
        }
    }

    /// Whether colours are on their way.
    pub fn pending(&self) -> bool {
        !self.asked.is_empty()
    }

    /// Forget the paintings of tabs `live` no longer names.
    pub fn retain(&mut self, live: impl Fn(u64) -> bool) {
        self.done.retain(|id, _| live(*id));
        self.asked.retain(|id, _| live(*id));
    }

    /// The worker, started on first use.
    fn worker(&mut self) -> Option<&Worker> {
        if self.worker.is_none() && !self.failed {
            let (jobs, queued) = mpsc::channel();
            let (painted, done) = mpsc::channel();
            let spawned = std::thread::Builder::new()
                .name("aless-paint".into())
                .stack_size(load::PARSE_STACK)
                .spawn({
                    let (shown, running) = (self.shown.clone(), self.running.clone());
                    move || work(queued, painted, shown, running)
                });
            match spawned {
                Ok(_) => self.worker = Some(Worker { jobs, done }),
                Err(_) => self.failed = true,
            }
        }
        self.worker.as_ref()
    }

    /// Wait up to `limit` for every text asked for, taking in its colours.
    #[cfg(test)]
    pub fn settle(&mut self, limit: Duration) {
        let until = Instant::now() + limit;
        while self.pending() && Instant::now() < until {
            if !self.collect() {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
}

/// The worker: paint each text asked for, in order, skipping one that a
/// newer text for the same tab has replaced and one no pane shows any
/// more, and stopping one whose text leaves the panes while it is
/// coloured. It ends when the painter is dropped.
fn work(
    jobs: Receiver<Job>,
    done: Sender<(Key, Made)>,
    shown: Arc<Mutex<Shown>>,
    running: Arc<AtomicUsize>,
) {
    let lock = || shown.lock().unwrap_or_else(PoisonError::into_inner);
    let mut queue: VecDeque<Job> = VecDeque::new();
    loop {
        if queue.is_empty() {
            match jobs.recv() {
                Ok(job) => queue.push_back(job),
                Err(_) => return,
            }
        }
        queue.extend(jobs.try_iter());
        let Some(job) = queue.pop_front() else {
            continue;
        };
        if queue.iter().any(|later| later.key.0 == job.key.0) {
            continue;
        }
        let stop = Arc::new(AtomicBool::new(false));
        let (in_view, busy) = {
            let mut shown = lock();
            let in_view = shown.keys.contains(&job.key);
            let busy = running.load(Ordering::SeqCst) >= MAX_LEFT;
            if in_view && !busy {
                shown.running = Some((job.key, stop.clone()));
            }
            (in_view, busy)
        };
        if in_view && busy {
            // Held until a thread left behind ends, taking in newer texts,
            // which may replace it, meanwhile.
            queue.push_front(job);
            match jobs.recv_timeout(Duration::from_millis(20)) {
                Ok(newer) => queue.push_back(newer),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            continue;
        }
        let key = job.key;
        let made = if in_view {
            // A deadline to stop it by, even for a painter made without one.
            let timeout = job.timeout.or(Some(MAX_TIME));
            let (text, flag) = (job.text, stop.clone());
            let colour = move || paint_unless(key.2, &text, timeout, Some(flag));
            let colours = apart(colour, &stop, timeout, &running);
            lock().running = None;
            match colours {
                Some(painted) if !stop.load(Ordering::Relaxed) => Made::Colours(painted),
                _ => Made::Skipped,
            }
        } else {
            Made::Skipped
        };
        if done.send((key, made)).is_err() {
            return;
        }
    }
}

/// Run `colour` on a thread of its own and wait for it: its colours, or
/// `Some(None)` for none. The wait ends early, and the thread is left to
/// finish alone, when `stop` is raised (`None`), or once `limit` and
/// [`GRACE`] have passed (`Some(None)`, as for a parse out of time). The
/// flag and the deadline stop a parse at its next step, but making the
/// parser comes first, seconds for a large custom grammar, and neither
/// reaches it. So a text hidden or out of time never holds the worker, and
/// the next text starts at once, whatever is left running; a thread left
/// behind ends once its parser is made, its parse stopping at the first
/// step. `running` counts the threads. Without a thread to be had, the
/// work runs here.
fn apart(
    colour: impl FnOnce() -> Option<Painted> + Send + 'static,
    stop: &AtomicBool,
    limit: Option<Duration>,
    running: &Arc<AtomicUsize>,
) -> Option<Option<Painted>> {
    /// A thread's place in the count, given up however it ends.
    struct Place(Arc<AtomicUsize>);
    impl Drop for Place {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::SeqCst);
        }
    }
    if stop.load(Ordering::Relaxed) {
        return None;
    }
    let until = limit.and_then(|limit| Instant::now().checked_add(limit.saturating_add(GRACE)));
    // The work is taken by whichever thread runs it: a spawn that fails
    // drops its closure unrun, and the work with it otherwise.
    let slot = Arc::new(Mutex::new(Some(colour)));
    let taken = slot.clone();
    let (sent, got) = mpsc::channel();
    running.fetch_add(1, Ordering::SeqCst);
    let place = Place(running.clone());
    let spawned = std::thread::Builder::new()
        .name("aless-paint-job".into())
        .stack_size(load::PARSE_STACK)
        .spawn(move || {
            let _place = place;
            let colour = taken.lock().unwrap_or_else(PoisonError::into_inner).take();
            if let Some(colour) = colour {
                let _ = sent.send(colour());
            }
        });
    if spawned.is_err() {
        let colour = slot.lock().unwrap_or_else(PoisonError::into_inner).take();
        return Some(colour.and_then(|colour| colour()));
    }
    loop {
        match got.recv_timeout(Duration::from_millis(10)) {
            Ok(painted) => return Some(painted),
            Err(RecvTimeoutError::Timeout) if stop.load(Ordering::Relaxed) => return None,
            Err(RecvTimeoutError::Timeout)
                if until.is_some_and(|until| Instant::now() >= until) =>
            {
                return Some(None)
            }
            Err(RecvTimeoutError::Timeout) => {}
            // The thread ended without colours: a panic outside the grammar.
            Err(RecvTimeoutError::Disconnected) => return Some(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(p: &Painted, text: &str, n: usize) -> Vec<(String, &'static str)> {
        let line = crate::load::lines(text)[n];
        p.line(n)
            .iter()
            .map(|r| (line[r.start..r.end].to_string(), r.kind.name()))
            .collect()
    }

    #[test]
    fn a_text_is_coloured_by_what_its_grammar_lexed() {
        let text = "{\"a\": 1,\n \"b\": [true, \"x\"]}\n";
        let p = paint(Grammar::Format(Format::Json), text, None).unwrap();
        let first = kinds(&p, text, 0);
        assert!(first.contains(&("\"a\"".into(), "string")), "{first:?}");
        assert!(first.contains(&("1".into(), "number")), "{first:?}");
        let second = kinds(&p, text, 1);
        assert!(second.contains(&("true".into(), "keyword")), "{second:?}");
        assert!(second.contains(&("\"x\"".into(), "string")), "{second:?}");
        // A program is lexed by alchemy's grammar.
        let program = "; a note\ndef export [input] input\n";
        let p = paint(Grammar::Alchemy, program, None).unwrap();
        assert_eq!(kinds(&p, program, 0), [("; a note".into(), "comment")]);
    }

    /// TOML's strings, values and quoted keys alike, and alchemy's `:name`
    /// keywords are coloured (tabnas/lsp#28): the TOML lexer reports a
    /// string at its end, and the token after it hid it, and alchemy's
    /// keyword token had no type at all.
    #[test]
    fn toml_strings_and_alchemy_keywords_are_coloured() {
        let toml = "a = \"x\"\n\"q k\" = 'lit'\nb = [\"y\"]\n";
        let p = paint(Grammar::Format(Format::Toml), toml, None).unwrap();
        assert!(
            kinds(&p, toml, 0).contains(&("\"x\"".into(), "string")),
            "{:?}",
            kinds(&p, toml, 0)
        );
        let second = kinds(&p, toml, 1);
        assert!(second.contains(&("\"q k\"".into(), "string")), "{second:?}");
        assert!(second.contains(&("'lit'".into(), "string")), "{second:?}");
        assert!(kinds(&p, toml, 2).contains(&("\"y\"".into(), "string")));
        let program = "def export [input] (match input case :none 1 case _ 2)\n";
        let p = paint(Grammar::Alchemy, program, None).unwrap();
        assert!(
            kinds(&p, program, 0).contains(&(":none".into(), "keyword")),
            "{:?}",
            kinds(&p, program, 0)
        );
    }

    #[test]
    fn runs_count_from_the_first_byte_and_stay_in_their_line() {
        // A byte-order mark is not parsed, but the runs count it; a line
        // ending in CRLF keeps its runs short of the CR.
        let text = "\u{feff}a: 1\r\nb: two\r\n";
        let p = paint(Grammar::Format(Format::Yaml), text, None).unwrap();
        let first = kinds(&p, text, 0);
        assert!(first.contains(&("1".into(), "number")), "{first:?}");
        let second = kinds(&p, text, 1);
        assert!(second.contains(&("two".into(), "string")), "{second:?}");
        for (n, line) in crate::load::lines(text).iter().enumerate() {
            assert!(p.line(n).iter().all(|r| r.end <= line.len() + 1));
        }
    }

    /// A JSON5 string continued across a CRLF is coloured as across an LF:
    /// the parser reads it as the loader does, and the runs are those of
    /// the LF text, line for line, none taking a line's CR.
    #[test]
    fn json5_colours_a_continuation_across_crlf() {
        let lf = "{\n  a: 'x\\\n  y',\n  b: [1, \"p\\\nq\"],\n  c: true\n}\n";
        let crlf = lf.replace('\n', "\r\n");
        let one = paint(Grammar::Format(Format::Json5), lf, None).unwrap();
        let two = paint(Grammar::Format(Format::Json5), &crlf, None).unwrap();
        for n in 0..crate::load::lines(lf).len() {
            assert_eq!(kinds(&two, &crlf, n), kinds(&one, lf, n), "line {n}");
        }
        assert!(kinds(&two, &crlf, 5).contains(&("true".into(), "keyword")));
    }

    #[test]
    fn what_is_never_coloured() {
        let too_long = " ".repeat(MAX_BYTES + 1);
        assert_eq!(paint(Grammar::Format(Format::Json), &too_long, None), None);
        assert_eq!(paint(Grammar::Format(Format::Text), "a", None), None);
        let registry = Registry::from_json(
            r#"{"count": 1, "entries": [
                {"name": "@tabnas/json", "languageId": "json", "lexStream": "speculative"}
            ]}"#,
        )
        .unwrap();
        assert!(!Grammar::Format(Format::Json).colourable_by(&registry));
        assert!(
            Grammar::Format(Format::Yaml).colourable_by(&registry),
            "not listed"
        );
        assert!(Grammar::Alchemy.colourable_by(&registry));
        assert!(
            Grammar::Format(Format::Json).colourable(),
            "the bundled entry is clean"
        );
    }

    #[test]
    fn a_parse_out_of_time_colours_what_it_lexed() {
        let text = format!("[{}]", vec!["1"; 2000].join(","));
        let p = paint(
            Grammar::Format(Format::Json),
            &text,
            Some(Duration::from_nanos(1)),
        )
        .unwrap();
        assert!(
            p.line(0).len() < 4000,
            "stopped early: {} runs",
            p.line(0).len()
        );
    }

    #[test]
    fn the_painter_keeps_the_newest_text_of_each_tab() {
        let json = Grammar::Format(Format::Json);
        let mut painter = Painter::new(None);
        painter.want([(1, 0, json), (1, 1, json)]);
        painter.ask((1, 0, json), "[1]");
        painter.ask((1, 1, json), "[\"x\"]");
        painter.ask((2, 0, Grammar::Format(Format::Text)), "plain");
        assert!(
            painter.has((2, 0, Grammar::Format(Format::Text))),
            "settled at once"
        );
        painter.settle(Duration::from_secs(30));
        assert!(!painter.pending());
        assert!(painter.get((1, 0, json)).is_none(), "replaced");
        let p = painter.get((1, 1, json)).unwrap();
        assert_eq!(p.line(0)[1].kind, TokenType::String);
        assert!(painter.get((2, 0, Grammar::Format(Format::Text))).is_none());
        painter.retain(|id| id != 1);
        assert!(painter.get((1, 1, json)).is_none(), "its tab is gone");
    }

    /// A job whose text no pane shows by its turn is skipped, so a tab left
    /// or closed never holds up the one in view; shown again, the text is
    /// asked for again and coloured.
    #[test]
    fn a_text_no_pane_shows_is_skipped() {
        let json = Grammar::Format(Format::Json);
        let (left, shown) = ((1, 0, json), (2, 0, json));
        let mut painter = Painter::new(None);
        painter.want([shown]);
        painter.ask(left, "[1]");
        painter.ask(shown, "[2]");
        painter.settle(Duration::from_secs(30));
        assert!(!painter.pending(), "the skipped job is not waited for");
        assert!(painter.get(shown).is_some(), "the text in view is coloured");
        assert!(!painter.has(left), "the other is not");
        painter.want([left, shown]);
        painter.ask(left, "[1]");
        painter.settle(Duration::from_secs(30));
        assert!(painter.get(left).is_some(), "shown again, it is coloured");
    }

    /// A text that leaves the panes while it is coloured is called off:
    /// its parse stops, and it is skipped rather than coloured in part.
    #[test]
    fn a_text_hidden_while_it_is_coloured_is_called_off() {
        let key = (1, 0, Grammar::Format(Format::Json));
        // Long enough that its parse is still running when it is hidden.
        let text = format!("[{}]", vec!["1"; 200_000].join(","));
        assert!(text.len() <= MAX_BYTES);
        let mut painter = Painter::new(None);
        painter.want([key]);
        painter.ask(key, &text);
        let until = Instant::now() + Duration::from_secs(30);
        while painter.shown.lock().unwrap().running.is_none() {
            assert!(Instant::now() < until, "never started");
            std::thread::sleep(Duration::from_millis(1));
        }
        painter.want([]);
        painter.settle(Duration::from_secs(30));
        assert!(!painter.pending());
        assert!(!painter.has(key), "called off, not coloured");
    }

    /// With as many colourings left running as allowed, a shown text waits
    /// in the queue for one to end rather than start another, while a
    /// hidden one is skipped at once; once there is room, it is coloured.
    #[test]
    fn a_shown_text_waits_while_too_many_are_left_running() {
        let json = Grammar::Format(Format::Json);
        let (shown, hidden) = ((1, 0, json), (2, 0, json));
        let mut painter = Painter::new(None);
        painter.running.store(MAX_LEFT, Ordering::SeqCst);
        painter.want([shown]);
        painter.ask(hidden, "[1]");
        painter.ask(shown, "[2]");
        let until = Instant::now() + Duration::from_secs(30);
        while painter.has(hidden) {
            assert!(Instant::now() < until, "the hidden text was never skipped");
            painter.collect();
            std::thread::sleep(Duration::from_millis(5));
        }
        std::thread::sleep(Duration::from_millis(100));
        painter.collect();
        assert!(painter.pending(), "the shown text waits");
        assert!(painter.get(shown).is_none(), "and is not coloured yet");
        painter.running.store(0, Ordering::SeqCst);
        painter.settle(Duration::from_secs(30));
        assert!(painter.get(shown).is_some(), "coloured once there is room");
    }

    /// Work no flag reaches, as making a large grammar's parser is, is left
    /// to finish on its own thread once called off or out of time, and the
    /// worker goes on: the next text starts at once, however many are left
    /// running, and the count of them falls as they end.
    #[test]
    fn work_called_off_or_out_of_time_is_left_to_finish_alone() {
        let running = Arc::new(AtomicUsize::new(0));
        // Work that ends when released, as an install ends when done, and
        // after twenty seconds should nothing release it.
        let stuck = || {
            let (release, held) = mpsc::channel::<()>();
            let failsafe = release.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(20));
                let _ = failsafe.send(());
            });
            (release, move || {
                let _ = held.recv();
                None
            })
        };
        let mut releases = Vec::new();
        // Two texts hidden while their grammars install.
        for _ in 0..2 {
            let stop = Arc::new(AtomicBool::new(false));
            let raise = {
                let stop = stop.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(20));
                    stop.store(true, Ordering::Relaxed);
                })
            };
            let (release, work) = stuck();
            let began = Instant::now();
            assert!(apart(work, &stop, None, &running).is_none(), "called off");
            assert!(began.elapsed() < Duration::from_secs(10), "not waited for");
            raise.join().unwrap();
            releases.push(release);
        }
        assert_eq!(running.load(Ordering::SeqCst), 2, "both left running");
        // A text still shown whose install outlasts its limit.
        let calm = AtomicBool::new(false);
        let (release, work) = stuck();
        let began = Instant::now();
        let colours = apart(work, &calm, Some(Duration::from_millis(10)), &running);
        assert_eq!(colours, Some(None), "given up as out of time");
        assert!(began.elapsed() < GRACE + Duration::from_secs(5));
        releases.push(release);
        assert_eq!(running.load(Ordering::SeqCst), 3);
        // The next text starts at once, whatever is left running.
        let began = Instant::now();
        let colours = apart(|| Some(Painted::default()), &calm, None, &running);
        assert_eq!(colours, Some(Some(Painted::default())));
        assert!(began.elapsed() < Duration::from_secs(5), "not held");
        // Called off before it starts, it never starts.
        let stop = AtomicBool::new(true);
        let colours = apart(|| unreachable!("it started"), &stop, None, &running);
        assert!(colours.is_none());
        // Each thread left ends once its work does.
        for release in releases {
            release.send(()).unwrap();
        }
        let until = Instant::now() + Duration::from_secs(30);
        while running.load(Ordering::SeqCst) > 0 {
            assert!(Instant::now() < until, "they never ended");
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
