# aless

A [jless](https://jless.io)-style terminal viewer for **every format the
[tabnas](https://github.com/tabnas) parsers read** — JSON, JSON Lines,
jsonic, JSONC, JSON5, YAML, TOML, INI, CSV, TSV, XML, ZON, Markdown and
RSS/Atom feeds — with **tabs** for several files at once and a **watch
mode** that reloads a file when it changes while **keeping your place**.
Without a screen it prints JSON instead, for scripts and agents: outlines,
values by path, search hits and parse errors, each with its source
position ([Scripts and agents](#scripts-and-agents)). Pure Rust; runs on
Linux, macOS and Windows.

```
▼ {
  ▽ store: {
      name: "corner shop"
      open: true
    ▽ books: [
      ▷ (3) {title: "SICP", price: 42.5, tags: […]}
      ▽ {
          title: "TAPL"
          price: 55
        ▽ tags: [
            "types"
    ▷ counts: (2) {fiction: 12, science: 7}
    version: 3
 nested.json .store.books[1].title                json · watching · 7:8
/TAPL  [1/1]
```

aless reproduces jless's interface — the key map, data and line modes,
collapsed previews, regex search, the yank commands — and adds what the
AQL [aless](https://github.com/voxgig-boru/aless) pioneered: tabs, watching,
and a reload that re-anchors the view. Both are credited in
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). Deviations from jless are
listed [below](#deviations-from-jless).

## Install

Rust 1.86 or newer and git:

```bash
cargo install --locked --git https://github.com/rjrodger/aless aless
```

The tabnas crates are not on crates.io yet, so `Cargo.toml` takes them
straight from GitHub, pinned by `Cargo.lock` (see [Dependencies](#dependencies));
`--locked` makes `cargo install` honour those pins instead of resolving
each repository's current head.
Build from a checkout with `cargo build --release`; the binary is
`target/release/aless`.

## Usage

```bash
aless data.json                      # one file
aless config.toml deploy.yaml a.csv  # several files, one tab each
curl -s https://api.example/x | aless        # stdin (JSON unless --kind says otherwise)
aless --kind jsonic notes.txt        # force a format
aless --no-watch big.json            # do not reload on change
aless --mode line --line-numbers x.json
aless                                # explore the current directory; Enter opens a file
aless examples/solardemo-1.0.0-openapi-3.0.0.yaml   # an OpenAPI spec to try (see examples/)
```

| Option | Effect |
|---|---|
| `-k`, `--kind FORMAT` | parse every input as FORMAT instead of by extension |
| `--no-watch` | do not reload files when they change |
| `-m`, `--mode data\|line` | start in data (default) or line mode |
| `--depth N` | fold containers deeper than N levels at start |
| `-n` / `-N`, `-r` / `-R` | absolute / relative line numbers on / off |
| `--scrolloff N` | rows kept around the focus when scrolling (default 3) |
| `--indent N` | indentation per level (default 2) |
| `--hidden` | show dot-files in the explorer |
| `--ascii` | ASCII fold markers (`v`, `>`) instead of `▼ ▽ ▶ ▷` |
| `--no-color`, `--no-mouse` | plain output; no mouse capture |
| `--max-size SIZE` | refuse an input larger than SIZE (default `64M`; `K`, `M`, `G`; `0` for no limit); see [Performance](#performance) |
| `--timeout SECONDS` | stop a parse that runs longer than this (`2.5`, `90s`, `2m`; default none) |

`NO_COLOR` in the environment also disables colour.

## Scripts and agents

aless also runs without a screen. Give it any option from the table
below except `--depth` and `-k`, which the viewer shares, or let its
standard output be something other than a terminal (a pipe, a file, an
agent's tool call), and it prints JSON instead of starting the viewer. It
never waits for keys: when the viewer cannot start, aless says so at
once, before reading any input, and exits with status 2.

```bash
aless config.yaml | jq .spec                       # any format in, JSON out
aless --paths --depth 1 big.json                   # an outline: what is in it
aless --json --path '.spec.containers[0]' deploy.yaml
aless --find '"image":' deploy.yaml                # nodes whose text matches
aless --where --at 42:7 deploy.yaml                # the value a linter's 42:7 is in
aless --where --path .spec.replicas deploy.yaml    # the line a path is on
aless --check $(git ls-files '*.yaml' '*.toml')    # does everything parse?
```

| Option | Effect |
|---|---|
| `--json` | the document as JSON (the default), or the value at the start |
| `--paths` | an entry for the start and each node below it |
| `--find REGEX` | the entries of the nodes whose `"key": value` text matches: the viewer's search, so smart case (`REGEX/s` matches case) and `[ ] { }` literal unless escaped |
| `--where` | the start's entry |
| `--check` | parse every input and report on each |
| `--path PATH` | start at PATH instead of the root |
| `--at LINE[:COL]` | start at the node at that source position |
| `--depth N` | `--paths` and `--find` go at most N levels below the start |
| `--limit N` | at most N entries (default 200, 0 for all) |
| `--compact` | JSON on one line |
| `-k`, `--kind FORMAT` | parse as FORMAT; standard input is JSON unless this says otherwise |
| `--max-size SIZE` | refuse an input larger than SIZE (default `64M`; `0` for no limit) |
| `--timeout SECONDS` | stop a parse that runs longer than this (default none) |

**Paths** are jq's syntax, which every output prints, so a path can go
straight back in: `.`, `.a.b[0]`, `."odd key"`, `.["a.b"]`, and `[-1]`
for a last item. `a.b[0]` without the dot, JSONPath's `$.a['b'][0]` and
JSON Pointer's `/a/b/0` work too. Wildcards, slices and recursive descent
are jq's work: pipe `--json` into jq for those. Quote a path for the
shell, whose globbing would take `[0]`.

**An entry** is one node:

```json
{"path":".store.books[1].title","kind":"string","line":7,"col":8,"value":"TAPL"}
```

- `kind` is jq's name for the type: object, array, string, number,
  boolean or null.
- `line` and `col` count from 1 and give where the node starts in the
  source: at its key when it has one, else at its value. They are exact
  for the JSON family, TOML, INI, CSV and ZON, best-effort for YAML, XML
  and Markdown, and `null` when unknown.
- A container has `length`, its item count; a scalar has `value`. A
  string over 200 characters is cut to 200, with `"truncated": true` and
  its full `length`. Numbers are 64-bit floats, so an integer beyond
  2^53 loses precision; NaN and the infinities, which JSON cannot hold,
  are `"NaN"`, `"Infinity"` and `"-Infinity"` in an entry and `null` in
  `--json` output.

A listing puts one entry per line and says what it left out:

```
$ aless --paths --depth 1 nested.json
{
  "file": "nested.json",
  "format": "json",
  "path": ".",
  "entries": [
    {"path":".","kind":"object","line":1,"col":1,"length":2},
    {"path":".store","kind":"object","line":2,"col":3,"length":4},
    {"path":".version","kind":"number","line":11,"col":3,"value":3}
  ],
  "total": 3,
  "limit": 200,
  "truncated": false
}
```

`file` is the path as given, or `-` for standard input, and `path` is
where the listing starts. `--find` prints the same with `pattern` and
`matches`; `--where` prints one entry with `file` and `format`; `--check`
prints `{"ok", "files": [{"file", "format", "ok", "error"}]}`.

**Errors** are JSON on standard error, and standard output stays empty:

```
$ aless bad.json
{
  "error": {
    "kind": "parse",
    "file": "bad.json",
    "format": "json",
    "code": "unexpected",
    "message": "unexpected end of input",
    "line": 2,
    "col": 1,
    "hint": "The document ends before it is complete: look for an unclosed\nbracket, brace or string, or a missing value at the end.",
    "source_line": "",
    "report": "[tabnas/unexpected]: unexpected end of input\n  --> bad.json:2:1\n  1 | {\"a\": 1, \"b\": \n  2 | \n      ^ unexpected end of input\n…"
  }
}
```

| Exit | Meaning | Error `kind` |
|---|---|---|
| 0 | success: standard output holds the answer | |
| 1 | the input did not parse; with `--check`, some input failed and the report says which | `parse` |
| 2 | bad usage: an unknown option, a bad path, no input, a directory, or the viewer without a terminal | `usage` |
| 3 | an input could not be read, or the output could not be written | `io` |
| 4 | `--path` or `--at` names nothing | `not_found` |
| 5 | an input is larger than `--max-size` | `too_large` |
| 6 | a parse ran longer than `--timeout` | `timeout` |

A `parse` or `io` error has `file`, `format`, `code` (the grammar's error
code, or `io`), `message`, `line`, `col`, `hint`, `source_line` and
`report`, the whole report the viewer shows, uncoloured; a field that
does not apply is `null` (`file` too, when it was standard output that
could not be written). A `not_found` error has the `path` or `at` it
was given, the entry of the `nearest` node the path did reach, and that
node's first `keys` when it is an object. A `too_large` error has the
fields of an `io` one plus the input's `size` (`null` for standard input,
which is read no further than the limit) and the `limit`, in bytes, and
its `hint` names the `--max-size` that would read it. A `timeout` error
has the fields of a `parse` one, its `line` and `col` showing how far the
parse got, plus the time limit in `seconds`; a parse that finished, but
late, fails the same way, with `line` and `col` `null` and a `hint`
saying how long it took. A `usage` error has only `kind` and `message`.
A document nested deeper than aless parses (about 1,000 levels) fails as
a `parse` error with the code `too_deep`.

**Large inputs.** An input is read whole, and parsed whole, before
anything is printed: the tabnas grammars parse complete documents, so
there is no streaming, and the first byte of output comes when the parse
ends. That costs memory, about 80 bytes per byte of input, and time (see
[Performance](#performance)), which is why inputs over `--max-size` are
refused, before a file is read or as soon as standard input passes the
limit, and why `--timeout` exists: a slow grammar can take minutes over
a document of modest size, so a caller with a deadline of its own should
pass a shorter one, and get an error it can read rather than a kill.
Pipes are safe both ways: standard input can be a pipe or a file,
and a reader that stops early (`aless --json big.json | head`) ends
aless quietly with status 0, though the parse has already been paid for.
To take part of a large document, `--path` and `--depth` keep the output
small; the input is still parsed in full.

These shapes are a contract: fields may be added, but none is renamed,
removed or given a new meaning. [`skills/aless/SKILL.md`](skills/aless/SKILL.md)
is an Agent Skill that teaches an agent all of this (copy the
`skills/aless` directory into `~/.claude/skills/`, or wherever your agent
loads skills from), and `aless --help` opens with it.

## Formats

The format comes from the file extension; `--kind`, `:open PATH FORMAT`
and `:format FORMAT` override it. Anything unrecognised is shown as plain
text, one line per row, so every file is viewable.

| Format | Extensions | Parser |
|---|---|---|
| json | json, geojson, har, jsonld, webmanifest | tabnas-json |
| jsonl | jsonl, ndjson | tabnas-jsonl |
| jsonic | jsonic | tabnas-jsonic |
| jsonc | jsonc | tabnas-jsonc |
| json5 | json5 | tabnas-json5 |
| yaml | yaml, yml | tabnas-yaml |
| toml | toml | tabnas-toml |
| ini | ini, cfg, conf, cnf | tabnas-ini |
| csv, tsv | csv; tsv, tab | tabnas-csv |
| xml | xml, svg, xhtml, xsd, xsl, xslt, plist | tabnas-xml |
| zon | zon | tabnas-zon |
| markdown | md, markdown (shown as its AST) | tabnas-markdown |
| feed | rss, atom (normalised to an Atom shape) | tabnas-feed |
| text | txt, text, log, anything else | — |

CSV and TSV show a list of records keyed by the header row. Map keys keep
their **source order**.

## Keys

The jless key map, with counts (`3j`, `2J`, `5g`) where jless takes them.

| Keys | Action |
|---|---|
| `j` `k` / `↓` `↑` / `C-n` `C-p` / `Enter` `Backspace` | move down / up |
| `h` / `←` | collapse an expanded container, else go to the parent |
| `l` / `→` | expand a collapsed container, else step to the first child |
| `H` | go to the parent without collapsing |
| `J` `K` | next / previous sibling |
| `w` `b` | forward / back to the next change in depth |
| `0` `^` / `$` | first / last sibling |
| `g` `G` / `Home` `End` | first / last row; `Ng` or `NG` goes to row N |
| `C-f` `C-b` / `PgDn` `PgUp` | a page down / up |
| `C-d` `C-u` | half a page down / up (a count sets and remembers the distance) |
| `C-e` `C-y` | scroll one row, dragging the focus only when it would leave the window |
| `zz` `zt` `zb` | focused row to the centre / top / bottom |
| `.` `,` `;` | scroll a long value right / left / to its end and back |
| `<` `>` | less / more indentation |
| `Space` | toggle the focused container |
| `c` `C` | collapse the focused node and its siblings (`C`: and everything inside them) |
| `e` `E` | expand the focused node and its siblings (`E`: deeply) |
| `m` | switch between data mode and line mode |
| `%` | in line mode, jump between a container's opening and closing brackets |
| `/pat` `?pat` | search forward / backward (regex, smart case, `/s` forces case, `[ ] { }` are literal) |
| `n` `N` | next / previous match, wrapping (the status shows `[i/N]` and `W` after a wrap) |
| `*` `#` | search for the focused key forward / backward |
| `yy` `yv` `ys` `yk` `yp` `yb` `yq` | copy the pretty value / one-line value / string contents / key / path `.a[0].b` / `["a"][0]["b"]` / jq path |
| `pp` `pv` `ps` `pk` `pP` `pb` `pq` | the same, printed on screen |
| `:` | a command (below) |
| `F1` / `:help` | the key map |
| `q` | close the tab (closing the last one quits); `C-c` quits |

Extensions on keys jless leaves free:

| Keys | Action |
|---|---|
| `Tab` `Shift-Tab` | next / previous tab |
| `W` | toggle watching on the tab |
| `r` | reload the tab now |
| `s` | show the raw source, scrolled to the focused node's line (`s` or `Esc` returns) |
| `!` | show the tab's parse error report in full (`h` `l` pan a long line) |
| `C-z` | suspend (Unix) |

Mouse: the wheel scrolls, a click focuses a row (`--no-mouse` to leave the
mouse to the terminal).

### Commands

`:open PATH [FORMAT]` (`:e`) · `:q` / `:close` · `:qa` / `:quit` / `:exit` ·
`:tab N` · `:tabnext` / `:tabprev` · `:watch [on|off]` · `:reload` ·
`:format FORMAT` · `:mode data|line` · `:depth N` · `:expand` / `:collapse` ·
`:N` / `:line N` · `:source` · `:error` · `:w[!] FILE` (writes the document as JSON) ·
`:set number|nonumber|number!|relativenumber|norelativenumber|relativenumber!|so=N|indent=N|watch|nowatch|ascii` ·
`:help`.

## File explorer

A directory opens as a tree in the same viewer:

```bash
aless .            # explore the current directory (plain `aless` does the same)
aless ~/projects   # any directory; :open DIR and :explore DIR work inside too
```

Directories are collapsible containers, files are leaves showing their
size and the format their extension implies. Everything the tree already
does applies: `j`/`k`, `l`/`h`, `Space`, `c`/`e`, `/name` to search the
listed names, `n`/`N`, counts. Directories are listed as you expand them,
one level ahead so a collapsed directory's preview shows its count and
first names, and never more than 2000 directories in one explorer.

| Keys | Action |
|---|---|
| `Enter` | open the file under the cursor in a new tab; on a directory, toggle it |
| `-` | go up: the parent directory becomes the root (folds and focus are kept) |
| `:cd DIR` | change the root, relative to the current one |
| `:explore [DIR]` | open another directory in a new tab |
| `:set hidden` / `nohidden` / `hidden!` | show dot-files (`--hidden` at start) |
| `yp` | copy the entry's filesystem path |

An explorer tab watches like a file tab: a file added or removed shows up
on the next tick, with the folds and the focus kept.

## Parse errors

A file that does not parse shows the report the tabnas engine renders for
it, the same text its own tools print:

```
[tabnas/unexpected]: unexpected character(s): ,
  --> data.json:3:14
  1 | {
  2 |   "a": 1,
  3 |   "b": [1, 2,,]
                   ^ unexpected character(s): ,
  4 | }

  The character(s) , do not match any rule alternative active at
  this position.
```

The header names the grammar and the error code, the `-->` line gives the
file, line and column, the excerpt marks the offending token with a caret,
and the grammar's hint follows; the full report adds the grammar's link and
the engine's diagnostics line. The colours are the engine's.

The report is made safe to draw, since its colour codes are obeyed. Control
characters in the message, the hint and the file's name, such as the
newline an unterminated string runs into, are shown escaped (`\n`). In the
quoted source lines they are shown as one-column pictures (`␛`), so the
caret still lines up. A quoted line longer than 160 characters, as in a
minified file, is cut to a window around the error, and the caret stops
at the end of its line.

- A file that has never parsed shows its report in place of the tree.
- A watched file that breaks after loading keeps its last good document on
  screen, with the report docked beneath it; the next save that parses
  clears it.
- `!` or `:error` shows the whole report in a scrollable overlay; `h` and
  `l` pan a line wider than the screen. `s` shows the raw source with the
  failing line marked.
- A `:format` that fails leaves the document as it was, and `!` shows that
  attempt's report until the next reload or `:format`.
- The status bar carries the short form (`!3:14: unexpected character(s): ,`)
  and the tab strip an `!`.

## Watching, reloading and keeping your place

Every file tab watches its file (`--no-watch`, `W` or `:watch off` to opt
out; the status bar says `watching`). Changes arrive through the platform
file watcher (inotify, FSEvents, ReadDirectoryChangesW — on the file's
directory, so an editor that saves by writing a temporary file and renaming
it is seen), debounced so a save that takes several writes reloads once;
a poll of the file's size and modification time on every half-second tick
catches anything the watcher misses.

A reload re-anchors the view instead of resetting it:

1. Folds are remembered **by path**: a container that is still there keeps
   its expanded or collapsed state; new containers start expanded.
2. The focused node is found again by path. If that path is gone, the
   focus goes to its nearest surviving ancestor and, within that ancestor,
   to the node **closest to the old source line** — so a renamed key or a
   reordered entry keeps the cursor where you were reading.
3. The focused row stays on the same screen row.

A file that fails to parse keeps the previous document and shows the
engine's report docked beneath it (see [Parse errors](#parse-errors)); the
source view (`s`) opens at the failing line. A file that
disappears keeps its document and marks the tab `✗`; when it comes back it
reloads. A file that does not exist yet can be opened all the same: the
tab waits for it.

### Source positions

aless knows the line and column each node came from, shown as `12:8` in
the status bar and used by the source view and by the reload fallback.
The tabnas engine reports every token it lexes with its position, and
aless aligns that stream with the parsed tree — keys and leaf values in
document order against value-bearing tokens in source order, with a
bounded lookahead — so no grammar has to record provenance itself. The
alignment is exact for the JSON family, TOML, INI, CSV/TSV and ZON, and
best-effort where a grammar synthesises values (Markdown's AST, XML's
element records, YAML): a node the alignment cannot place shows no
position and inherits none.

## Modes

**Data mode** (default) is the streamlined view: keys unquoted when they
are identifiers, no commas, no closing brackets, collapsed containers shown
as `(N) {first: items, …}` previews. **Line mode** (`m`) shows every
line of a pretty-printed rendering — quoted keys, commas, closing brackets
that `%` jumps between. **Source view** (`s`) shows the file as it is on
disk with line numbers, the focused node's line highlighted.

## Copying

`y` commands use the system clipboard (the `clipboard` feature, on by
default, via `arboard`). When no clipboard is reachable — over SSH, in a
container — aless falls back to the terminal's OSC 52 sequence, which
xterm, kitty, WezTerm, iTerm2, Alacritty, foot and Windows Terminal turn
into a clipboard write; the status line says which happened. `p`
commands print instead, so the text can be read or copied by the
terminal.

## Performance

Parsing is the tabnas engine's, and it is a general rule engine rather
than a hand-written JSON parser: on one machine a 5 MB JSON document
(240 thousand nodes) loads in about two seconds and a 47 MB one (2.4
million nodes) in about 35 seconds; on a slower one, 13 MB took 10
seconds and 66 MB 68 seconds. Memory peaks at about 80 bytes per source
byte while the parse runs (13 MB peaked at 1.0 GB, 66 MB at 5.1 GB); the
steady state afterwards is much smaller.

Three limits keep a large, slow or hostile input from taking the machine
down:

- **Size.** An input over `--max-size` (64 MB unless set; `0` removes the
  limit) is refused with a report that names the size that would read
  it.
- **Depth.** Nesting deeper than about 1,000 levels stops the parse with
  a `too_deep` error. Some grammars would otherwise recurse until the
  stack ran out and end the process, and slow down with the square of
  the depth well before that; the JSON grammar stops at 127 levels of
  its own accord. The parse runs on a thread with a 64 MB stack, whatever
  the platform gives the main thread.
- **Time.** A parse that runs past `--timeout` stops with a `timeout`
  error showing how far it got. aless looks at the time between every two
  steps of the parser, but cannot cut a step short: one very long string
  is read to its end first, and a parse that finishes after the limit
  fails all the same. There is no default, since how long a parse should
  take depends on the machine; set one where time matters. It matters
  most for TOML: a document of many tables parses in time that grows with
  the square of its length (4,000 `[[tables]]`, 300 KB, take some 20
  seconds).

The parse blocks the interface, so a large file shows a `loading…` notice
before the screen is taken over, and a reload of a large watched file
pauses the viewer for as long as its parse takes. Navigation, folding and
search are independent of the engine and stay fast: rebuilding the rows
of a 2.4-million-node document takes about a second, and a search over it
under a second.

## Platforms

Linux, macOS and Windows are all first-class: CI builds and tests on the
three. Terminal handling is crossterm's; the Unicode fold markers need a
font that has them (`--ascii` otherwise). On Windows use Windows Terminal
or another VT-capable console; piping into aless on Windows requires a
console to read keys from, so prefer a file argument there. `C-z`
suspend is Unix-only.

## Deviations from jless

- Multiple files open in tabs rather than one document; `q` closes the
  current tab and quits only when it was the last.
- Numbers are shown as the parsed value (`1e+21`, `55`), not the source
  spelling, because tabnas values are doubles; the source view has the
  spelling.
- Search runs over each node's own line-mode text (`"key": value`), so a
  pattern cannot span rows.
- `J`/`K` stop at the last sibling instead of tracking a desired depth.
- jless's `--json` and `--yaml` name the input format; here that comes
  from the extension or `--kind`, and `--json` asks for JSON output (see
  [Scripts and agents](#scripts-and-agents)).

## Dependencies

The engine (`tabnas`) and the grammar crates are pre-release and live on
GitHub, so `Cargo.toml` names each repository with `git = …` and
`Cargo.lock` pins the commit. Each grammar's own manifest refers to its
siblings by relative path (`tabnas = { path = "../../parser/rs" }`, the
tabnas development model); inside a git checkout cargo reads that as
"the package of that name in this same repository", which does not exist,
so a `[patch."https://github.com/tabnas/<grammar>"]` table per repository
supplies each sibling from its own repository. When the crates are
published, the `git` entries become version requirements and the patch
tables go; nothing else changes.

The other dependencies: crossterm (terminal), notify (file watching),
regex (search), unicode-width (layout), serde_json (JSON output),
arboard (clipboard, optional).

## Development

```bash
cargo fmt --all --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked                   # unit tests, fixture loading, headless app runs, the agent interface
python3 scripts/pty-smoke.py          # unix: drives the built binary in a pseudo-terminal
```

Module map — `src/main.rs` is the only file that touches the terminal;
the library is terminal-free and unit tested:

| Module | Role |
|---|---|
| `doc` | the parsed value as a pre-order arena; visible rows; paths; folding |
| `explorer` | directory trees as documents, listed lazily |
| `fmt` | text of keys and values, previews, JSON output, path formats |
| `headless` | the agent interface: paths, listings, search, positions, checks, JSON errors |
| `load` | format detection; the tabnas grammars; errors with positions; text fallback |
| `prov` | source positions by aligning the token stream with the tree |
| `search` | jless-style search patterns |
| `tab` | one open document: focus, scroll, mode, search, navigation, reload re-anchoring |
| `app` | tabs, modes, the key map, the command line, watch scheduling, yank |
| `render` | the screen as styled lines |
| `watch` | the notify file watcher |
| `clip` | clipboard and OSC 52 |

## License

MIT — see [LICENSE](LICENSE). jless and the AQL aless are MIT too; their
notices are in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
