//! Colour for text: a pane showing its text colours each token as the
//! text's grammar lexed it, through tabnas-lsp's semantic tokens, the ones
//! its language server sends an editor.
//!
//! Colouring parses the whole text and keeps its lex trace, which at its
//! peak holds a few hundred times the text: 270 MB and a second of a
//! release build for 0.9 MB of JSON. So it runs off the viewer's thread,
//! on one worker with the stack a parse needs, under the parse timeout
//! and aless's depth cap, and only on text up to [`MAX_BYTES`]. The text
//! shows uncoloured until its colours come in. A grammar the registry
//! marks as lexing speculatively (its trace holds tokens the parse took
//! back) is never coloured, and neither is plain text.

use std::collections::{HashMap, HashSet, VecDeque};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

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
    if text.len() > MAX_BYTES || !grammar.colourable() {
        return None;
    }
    // The time starts before the parser is made, as the loader's does:
    // installing a custom grammar can itself take seconds.
    let deadline = Deadline::after(timeout);
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

/// Colours made off the viewer's thread, by one worker for every pane.
/// It starts with the first text asked for.
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
    /// The texts the panes show now, shared with the worker, which skips
    /// a job for any other when its turn comes.
    wanted: Arc<Mutex<HashSet<Key>>>,
    /// How long one text's parse may take.
    timeout: Option<Duration>,
}

struct Worker {
    jobs: Sender<Job>,
    done: Receiver<(Key, Made)>,
}

/// What the worker made of a job.
enum Made {
    /// The text's colours: `None` for a text that is not coloured.
    Colours(Option<Painted>),
    /// Nothing: no pane showed the text when its turn came.
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
    /// making. A job for any other is skipped when its turn comes, so a tab
    /// left or closed never holds up the one in view.
    pub fn want(&mut self, keys: impl IntoIterator<Item = Key>) {
        let mut wanted = self.wanted.lock().unwrap_or_else(PoisonError::into_inner);
        wanted.clear();
        wanted.extend(keys);
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
                    let wanted = self.wanted.clone();
                    move || work(queued, painted, wanted)
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
        let until = std::time::Instant::now() + limit;
        while self.pending() && std::time::Instant::now() < until {
            if !self.collect() {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
}

/// The worker: paint each text asked for, in order, skipping one that a
/// newer text for the same tab has replaced and one no pane shows any
/// more. It ends when the painter is dropped.
fn work(jobs: Receiver<Job>, done: Sender<(Key, Made)>, wanted: Arc<Mutex<HashSet<Key>>>) {
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
        let shown = wanted
            .lock()
            .map_or(true, |wanted| wanted.contains(&job.key));
        let made = if shown {
            Made::Colours(paint(job.key.2, &job.text, job.timeout))
        } else {
            Made::Skipped
        };
        if done.send((job.key, made)).is_err() {
            return;
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
}
