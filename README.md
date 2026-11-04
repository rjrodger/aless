# aless

A [jless](https://jless.io)-style terminal viewer for every format the
[tabnas](https://github.com/tabnas) parsers read: JSON, JSON Lines,
jsonic, JSONC, JSON5, YAML, TOML, INI, CSV, TSV, XML, ZON, Markdown,
RSS and Atom feeds, CSS, `.proto` files, PGN chess games, C source,
grammars in ABNF, EBNF and GBNF, expressions and semantic versions, and
any text format you describe with an ABNF grammar. Every format is read
into one tree, which you fold, search and copy from with jless's keys.
Several files open in tabs, a directory opens as an explorer, and when a
file changes on disk aless reads it again and keeps your place.

aless is a viewer: it draws on a terminal and reads keys from it, and
without one it refuses to start. It is written in Rust, and runs on
Linux, macOS and Windows.

**The documentation is at [aless.tabnas.dev](https://aless.tabnas.dev)**:
a tutorial, how-to guides, the whole reference, and the reasons behind
the design.

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

## Thanks to jless

aless is inspired by [jless](https://jless.io), Paul Julius Martinez's
terminal viewer for JSON and YAML ([its
source](https://github.com/PaulJuliusMartinez/jless)). jless set out how
a large document reads in a terminal: a tree you fold and unfold with
vim's keys, a data mode that drops the punctuation, a preview of what a
folded node holds, and a path copied in two keystrokes. aless keeps that
interface whole, and carries it to every format the tabnas parsers read,
adding tabs, a file explorer, and a reload that keeps your place.
jless's licence is in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md),
and [aless and jless](https://aless.tabnas.dev/explanation/jless.html)
says where the two differ.

## Install

| Where | Command |
|---|---|
| Linux, macOS | `curl --proto '=https' --tlsv1.2 -LsSf https://github.com/rjrodger/aless/releases/latest/download/aless-installer.sh \| sh` |
| Windows | `powershell -ExecutionPolicy Bypass -c "irm https://github.com/rjrodger/aless/releases/latest/download/aless-installer.ps1 \| iex"` |
| Homebrew, on macOS or Linux | `brew install rjrodger/tap/aless` |
| A Rust toolchain, 1.88 or newer | `cargo install --locked aless`, or `cargo binstall aless` for the prebuilt binary |

Every release is on [GitHub
Releases](https://github.com/rjrodger/aless/releases), built for Linux,
macOS and Windows, on x86_64 and on aarch64. [Install
aless](https://aless.tabnas.dev/how-to/install.html) says how to check
what you downloaded, and [Set up completions and the man
page](https://aless.tabnas.dev/how-to/completions.html) how to have your
shell complete aless's options.

## Use it

```sh
aless data.json                          # one file
aless config.toml deploy.yaml data.csv   # several files, a tab each
curl -s https://api.example/items | aless   # standard input, read as JSON
aless -k yaml settings.conf              # a file whose extension misleads
aless --grammar hosts=hosts.abnf /etc/hosts   # a format of your own
aless                                    # explore the current directory
```

The keys are jless's: `j` and `k` move, `h` and `l` fold and unfold,
`/` searches, `yp` copies the focused node's path, and `q` closes the
tab. `F1`
inside aless lists every key and command. The
[tutorial](https://aless.tabnas.dev/tutorials/viewer.html) takes ten
minutes, and `aless --help` prints the whole command line reference.

## Deviations from jless

- Several files open in tabs rather than one document, so `q` closes
  the active tab, and quits only when it was the last.
- The search matches each node's own line-mode text (`"key": value`),
  so a pattern cannot span two rows; jless searches the whole document
  as one line.
- Row numbers count the rows on screen, in data mode as in line mode;
  jless numbers the lines of the pretty-printed document.
- `J` and `K` stop at the last sibling, rather than tracking a depth
  asked for earlier.
- A file's format comes from its extension or from `--kind`, where
  jless's `--json` and `--yaml` name it.

Everything aless adds (tabs, watching, the explorer, the source view)
is on a key jless leaves free.

## Develop it

[AGENTS.md](https://github.com/rjrodger/aless/blob/main/AGENTS.md) is
the guide for anyone working on aless, person or agent.
[docs/contributing/](https://github.com/rjrodger/aless/tree/main/docs/contributing)
has the detail: the
gates CI runs and how to run them, the design and its invariants, and how
the documentation site is built. [RELEASING.md](RELEASING.md) is how a
release is cut, and [PACKAGING.md](PACKAGING.md) what a packager needs.

## Licence

MIT: see [LICENSE](LICENSE). jless is MIT too; its notice is in
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
