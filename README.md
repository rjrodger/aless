# aless

A [jless](https://jless.io)-style terminal viewer for **every format the
[tabnas](https://github.com/tabnas) parsers read** — JSON, JSON Lines,
jsonic, JSONC, JSON5, YAML, TOML, INI, CSV, TSV, XML, ZON, Markdown and
RSS/Atom feeds — with **tabs** for several files at once and a **watch
mode** that reloads a file when it changes while **keeping your place**.
Pure Rust; runs on Linux, macOS and Windows.

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
cargo install --git https://github.com/rjrodger/aless aless
```

The tabnas crates are not on crates.io yet, so `Cargo.toml` takes them
straight from GitHub, pinned by `Cargo.lock` (see [Dependencies](#dependencies)).
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
aless                                # a welcome tab; :open files from inside
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
| `--ascii` | ASCII fold markers (`v`, `>`) instead of `▼ ▽ ▶ ▷` |
| `--no-color`, `--no-mouse` | plain output; no mouse capture |

`NO_COLOR` in the environment also disables colour.

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
| `C-z` | suspend (Unix) |

Mouse: the wheel scrolls, a click focuses a row (`--no-mouse` to leave the
mouse to the terminal).

### Commands

`:open PATH [FORMAT]` (`:e`) · `:q` / `:close` · `:qa` / `:quit` / `:exit` ·
`:tab N` · `:tabnext` / `:tabprev` · `:watch [on|off]` · `:reload` ·
`:format FORMAT` · `:mode data|line` · `:depth N` · `:expand` / `:collapse` ·
`:N` / `:line N` · `:source` · `:w[!] FILE` (writes the document as JSON) ·
`:set number|nonumber|number!|relativenumber|norelativenumber|relativenumber!|so=N|indent=N|watch|nowatch|ascii` ·
`:help`.

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

A file that fails to parse keeps the previous document and shows the error
with its line and column (`!3:14: unexpected …` in the status bar, `!` on
the tab); the source view (`s`) opens at the failing line. A file that
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
than a hand-written JSON parser: on this machine a 5 MB JSON document
(240 thousand nodes) loads in about two seconds and a 47 MB one (2.4
million nodes) in about 35 seconds, with a peak of roughly 50 bytes of
memory per source byte while the parse runs; the steady state afterwards
is much smaller. The parse blocks the interface, so a large file shows a
`loading…` notice before the screen is taken over, and a reload of a
large watched file pauses the viewer for as long as its parse takes.
Navigation, folding and search are independent of the engine and stay
fast: rebuilding the rows of a 2.4-million-node document takes about a
second, and a search over it under a second.

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
- No `--yaml`/`--json` flags: formats come from extensions or `--kind`.

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
regex (search), unicode-width (layout), serde_json (number formatting),
arboard (clipboard, optional).

## Development

```bash
cargo fmt --all --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked                   # unit tests, fixture loading, headless app runs
python3 scripts/pty-smoke.py          # unix: drives the built binary in a pseudo-terminal
```

Module map — `src/main.rs` is the only file that touches the terminal;
the library is terminal-free and unit tested:

| Module | Role |
|---|---|
| `doc` | the parsed value as a pre-order arena; visible rows; paths; folding |
| `fmt` | text of keys and values, previews, JSON output, path formats |
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
