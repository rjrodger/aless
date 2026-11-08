# aless

aless is a terminal viewer in the style of [jless](https://jless.io).
It reads every format the [tabnas](https://github.com/tabnas) parsers
read. Those formats are JSON, JSON Lines, jsonic, JSONC, JSON5, YAML,
TOML, INI, CSV, TSV, XML, ZON, Markdown, RSS and Atom feeds, CSS,
`.proto` files, PGN chess games, C source, grammars (ABNF, EBNF, and
GBNF), expressions, and semantic versions. aless also reads any text
format you describe with an ABNF grammar.

aless reads every format into one tree. You fold, search, and copy
from that tree with the same keys as jless. Several files open at
once, each in a tab of its own. A directory opens as an
[explorer](https://aless.tabnas.dev/how-to/explore.html). When a file
changes on disk, aless reads the file again and keeps your place.

aless is a viewer: it draws on a terminal and reads keys from the
terminal, so without one it refuses to start. It is written in Rust.
It runs on Linux, macOS, and Windows.

The documentation is at [aless.tabnas.dev](https://aless.tabnas.dev).
It holds a tutorial, how-to guides, the whole reference, and the
reasons behind the design.

Here is an OpenAPI document in YAML, opened with only its first two
levels unfolded:

<!-- screen: aless --depth 2 bookshelf-openapi.yaml
size: 80x18 -->
```screen
▼ {
    openapi: "3.0.0"
  ▽ info: {
      title: "Bookshelf demo API"
      version: "1.0.0"
      description: "A small API for keeping books on shelves, written for aless…
    ▷ license: (1) {name: "MIT"}
  ▽ servers: [
    ▷ (2) {url: "https://bookshelf.example/v1", …}
  ▽ tags: [
    ▷ (2) {name: "shelf", description: "Shelves, and the books on them"}
    ▷ (2) {name: "author", description: "The people who wrote the books"}
  ▽ paths: {
    ▷ "/api/shelf": (2) {get: {…}, post: {…}}
    ▷ "/api/shelf/{shelf_id}": (4) {parameters: […], get: {…}, put: {…}, …}
    ▷ "/api/shelf/{shelf_id}/book": (3) {parameters: […], get: {…}, …}
 bookshelf-openapi.yaml .                                 yaml · watching · 1:1
```

aless draws each folded node with a preview of what the node holds.

## Thanks to jless

[jless](https://jless.io) is the inspiration for aless. jless is Paul
Julius Martinez's terminal viewer for JSON and YAML. Its source is on
[GitHub](https://github.com/PaulJuliusMartinez/jless). jless showed
how a large document reads in a terminal: a tree you fold and unfold
with vim keys, a
[data mode](https://aless.tabnas.dev/how-to/display.html#data-mode-and-line-mode)
that drops the punctuation, a preview of what a folded node holds, and
a path copied in two keystrokes.

aless keeps that interface whole. It uses that interface for every
format the tabnas parsers read. It adds tabs, an explorer, and a
reload that keeps your place.

The licence of jless is in
[`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).

## Install

| You have | Run |
|---|---|
| Linux or macOS | `curl --proto '=https' --tlsv1.2 -LsSf https://github.com/rjrodger/aless/releases/latest/download/aless-installer.sh \| sh` |
| Windows | `powershell -ExecutionPolicy Bypass -c "irm https://github.com/rjrodger/aless/releases/latest/download/aless-installer.ps1 \| iex"` |
| Homebrew, on macOS or Linux | `brew install rjrodger/tap/aless` |
| A Rust toolchain, 1.88 or newer | `cargo install --locked aless`, or `cargo binstall aless` for the prebuilt binary |

Every release is on [GitHub
Releases](https://github.com/rjrodger/aless/releases). A release is
built for Linux, macOS, and Windows, on x86_64 and on aarch64. For
more information about checking what you downloaded, see [Install
aless](https://aless.tabnas.dev/how-to/install.html). For more
information about having your shell complete the options aless takes,
see [Set up completions and the man
page](https://aless.tabnas.dev/how-to/completions.html).

## Use it

Here are some ways to start aless:

```sh
aless data.json                          # one file
aless config.toml deploy.yaml data.csv   # several files, a tab each
curl -s https://api.example/items | aless   # standard input, read as JSON
aless -k yaml settings.conf              # a file whose extension misleads
aless --grammar hosts=hosts.abnf /etc/hosts   # a format of your own
aless                                    # explore the current directory
```

The keys are the ones jless uses: `j` and `k` move, `h` and `l` fold
and unfold, `/` searches, `yp` copies the focused node's path, and `q`
closes the tab. Inside aless, `F1` lists every key and command.

The tutorial, [Look around a file in the
viewer](https://aless.tabnas.dev/tutorials/viewer.html), takes about
10 minutes. `aless --help` prints the whole command-line reference.

## Deviations from jless

aless differs from jless in these ways:

- Several files open in tabs, so `q` closes the active tab. `q` quits
  aless only when that tab is the last. jless opens one document.
- The search matches each node's own
  [line-mode](https://aless.tabnas.dev/how-to/display.html#data-mode-and-line-mode)
  text (`"key": value`), so a pattern cannot span two rows. jless
  searches the whole document as one line.
- Row numbers count the rows on screen, in data mode as in line mode.
  jless numbers the lines of the pretty-printed document.
- The `J` and `K` keys stop at the last sibling, rather than tracking
  a depth asked for earlier.
- The format of a file comes from its extension or from `--kind`. In
  jless, the `--json` and `--yaml` options name the format.

Everything aless adds is on a key jless leaves free. The additions are
tabs, watching and the versions of a watched file, the explorer, and
the [source
view](https://aless.tabnas.dev/how-to/display.html#data-mode-and-line-mode).
For more information, see
[aless and jless](https://aless.tabnas.dev/explanation/jless.html).

## Develop it

[`AGENTS.md`](https://github.com/rjrodger/aless/blob/main/AGENTS.md)
is the guide for anyone working on aless, whether a person or an
agent. The pages under
[`docs/contributing/`](https://github.com/rjrodger/aless/tree/main/docs/contributing)
hold the detail: the gates CI runs and how to run them, the design and
its invariants, and how the documentation site is built.
For more information about the steps of a release, see
[Releasing aless](RELEASING.md). For more information about what a
packager needs, see [Packaging aless](PACKAGING.md).

## Licence

aless is under the MIT licence: see [`LICENSE`](LICENSE). jless is
under the MIT licence too. The notice for jless is in
[`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).
