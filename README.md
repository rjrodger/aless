# aless

A [jless](https://jless.io)-style terminal viewer for **every format the
[tabnas](https://github.com/tabnas) parsers read**: JSON, JSON Lines,
jsonic, JSONC, JSON5, YAML, TOML, INI, CSV, TSV, XML, ZON, Markdown,
RSS/Atom feeds, CSS, `.proto` files, PGN chess games, C source,
grammars in ABNF, EBNF and GBNF, expressions and semantic versions, and
any text format you describe with an ABNF grammar
([Custom grammars](#custom-grammars)), with **tabs** for several files
at once and a **watch mode** that reloads a file when it changes while
**keeping your place**.
Without a screen it prints JSON instead, for scripts and agents: outlines,
values by path, search hits and parse errors, each with its source
position ([Scripts and agents](#scripts-and-agents)). Pure Rust; runs on
Linux, macOS and Windows.

**Documentation: [aless.tabnas.dev](https://aless.tabnas.dev)**, with
tutorials, how-to guides, the whole reference, and the reasons behind the
design.

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

aless reproduces jless's interface (the key map, data and line modes,
collapsed previews, regex search, the yank commands) and adds tabs,
watching, and a reload that re-anchors the view. jless is credited in
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). Deviations from jless are
listed [below](#deviations-from-jless).

## Install

Every release is on [GitHub
Releases](https://github.com/rjrodger/aless/releases), with these ways
in:

| | |
|---|---|
| Linux, macOS | `curl --proto '=https' --tlsv1.2 -LsSf https://github.com/rjrodger/aless/releases/latest/download/aless-installer.sh \| sh` |
| Windows | `powershell -ExecutionPolicy Bypass -c "irm https://github.com/rjrodger/aless/releases/latest/download/aless-installer.ps1 \| iex"` |
| Homebrew (macOS, Linux) | `brew install rjrodger/tap/aless` |
| Rust toolchain | `cargo install --locked aless`, or `cargo binstall aless` for the prebuilt binary |

The installers put `aless` in `~/.cargo/bin`.

The prebuilt binaries cover:
- Linux, x86_64 and aarch64: built against glibc 2.35, or static (musl),
  which runs anywhere; the shell installer picks for you;
- macOS, x86_64 and aarch64;
- Windows, x86_64 and aarch64.

Each archive has a `.sha256` beside it, and its build provenance checks
with `gh attestation verify FILE --repo rjrodger/aless`. Beside the
binary it carries the man page, [`man/aless.1`](man/aless.1), and the
shells' completions, under [`completions/`](completions/): bash
(`aless.bash`), zsh (`_aless`), fish (`aless.fish`) and PowerShell
(`_aless.ps1`).

Homebrew installs the man page, and the bash, zsh, fish and PowerShell
completions in its own directories for them; its
[shell completion guide](https://docs.brew.sh/Shell-Completion) says how
a shell reads those (for zsh, `eval "$(brew shellenv)"` before
`compinit`). For any other install, `aless --generate` prints each file
from the binary itself, to save where the shell looks:

```bash
aless --generate man > ~/.local/share/man/man1/aless.1
aless --generate complete-bash > ~/.local/share/bash-completion/completions/aless
aless --generate complete-zsh > ~/.zfunc/_aless          # a directory on $fpath
aless --generate complete-fish > ~/.config/fish/completions/aless.fish
```

or to load from the shell's startup file each time it starts:

```bash
eval "$(aless --generate complete-bash)"         # ~/.bashrc
eval "$(aless --generate complete-zsh)"          # ~/.zshrc, after compinit
aless --generate complete-fish | source          # ~/.config/fish/config.fish
```

```powershell
aless --generate complete-powershell | Add-Content $PROFILE
```

For the newest commit on `main` rather than a release, build from git
(Rust 1.88 or newer):

```bash
cargo install --locked --git https://github.com/rjrodger/aless aless
```

`--locked` keeps the dependency versions `Cargo.lock` pins (see
[Dependencies](#dependencies)). From a checkout, `cargo build --release`
builds `target/release/aless`.

For package maintainers: [PACKAGING.md](PACKAGING.md). For making a
release: [RELEASING.md](RELEASING.md).

## Usage

```bash
aless data.json                      # one file
aless config.toml deploy.yaml a.csv  # several files, one tab each
curl -s https://api.example/x | aless        # stdin (JSON unless --kind says otherwise)
aless --kind jsonic notes.txt        # force a format
aless --grammar hosts=hosts.abnf /etc/hosts   # a format of your own, from an ABNF grammar
aless --no-watch big.json            # do not reload on change
aless --mode line --line-numbers x.json
aless                                # explore the current directory; Enter opens a file
aless examples/bookshelf-openapi.yaml   # an OpenAPI spec to try (see examples/)
```

| Option | Effect |
|---|---|
| `-k`, `--kind FORMAT` | parse every input as FORMAT instead of by extension; FORMAT may be a `--grammar` NAME; `--format` is another name for it |
| `--grammar NAME=FILE` | read files whose extension or whole name is NAME with the ABNF grammar in FILE; `NAME,NAME2=FILE` gives it two names; repeatable ([Custom grammars](#custom-grammars)) |
| `--grammar-expr NAME=ABNF` | the same, with the grammar text on the command line |
| `--no-watch` / `--watch` | do not reload files when they change / reload them (the default) |
| `-m`, `--mode data\|line` | start in data (default) or line mode |
| `--depth N` | fold containers deeper than N levels at start |
| `-n` / `-N`, `-r` / `-R` | absolute / relative line numbers on / off |
| `--scrolloff N` | rows kept around the focus when scrolling (default 3) |
| `--indent N` | indentation per level (default 2), JSON output's too |
| `--hidden` | show dot-files in the explorer |
| `--ascii` | ASCII fold markers (`v`, `>`) instead of `▼ ▽ ▶ ▷` |
| `--no-color` (or `--no-colour`), `--no-mouse` | plain output; no mouse capture |
| `--max-size SIZE` | refuse an input larger than SIZE (default `64M`; `K`, `M`, `G`; `0` for no limit); see [Performance](#performance) |
| `--timeout SECONDS` | stop a parse, or a `--grammar` compile, that runs longer than this (`2.5`, `90s`, `2m`; default none, `0` for no limit); on standard input, without a screen, it runs from the start, so waiting on the input counts |
| `-h` / `--help` | a summary of the options / the whole reference: every option, what each output prints, the errors and exit statuses, paths, positions, formats and limits |
| `-V`, `--version` | the version |
| `--generate WHAT` | print the man page (`man`), a shell's completions (`complete-bash`, `complete-zsh`, `complete-fish`, `complete-powershell`) or the Agent Skill (`skill`) |

`NO_COLOR` in the environment also disables colour.

## Scripts and agents

aless also runs without a screen. Give it any option from the table
below except those the viewer shares (`--depth`, `--indent`, `-k`, the
`--grammar` options, `--max-size` and `--timeout`), or let its standard
output be something other than a terminal (a pipe, a file, an agent's
tool call), or set `TERM=dumb`, and it prints JSON instead of starting
the viewer. It
never waits for keys: when the viewer cannot start, aless says so at
once, before reading any input, with a `usage` error, and exits with
status 2.

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
| `--compact` | JSON on one line, an error's too |
| `--indent N` | `--json` indented N spaces a level (default 2) |
| `-k`, `--kind FORMAT` | parse as FORMAT; standard input is JSON unless this says otherwise; `--format` is another name for it |
| `--grammar NAME=FILE`, `--grammar-expr NAME=ABNF` | a format of your own, from an ABNF grammar ([Custom grammars](#custom-grammars)) |
| `--max-size SIZE` | refuse an input larger than SIZE (default `64M`; `0` for no limit) |
| `--timeout SECONDS` | stop a parse, or a `--grammar` compile, that runs longer than this (default none; `0` for no limit); on standard input the time runs from the start, so waiting on the input counts |
| `TERM=dumb` | in the environment, not an option: JSON output, as when standard output is not a terminal, since such a terminal cannot draw the viewer |

**Paths** are jq's syntax, which every output prints, so a path can go
straight back in: `.`, `.a.b[0]`, `."odd key"`, `.["a.b"]`, `.[0]` for
a root array's first item and `.[-1]` for its last. `a.b[0]` without the
dot (and so `[0]` for `.[0]`), JSONPath's `$.a['b'][0]` and JSON
Pointer's `/a/b/0` work too. Wildcards, slices and recursive descent
are jq's work: pipe `--json` into jq for those. Quote a path for the
shell, whose globbing would take `[0]`.

**An entry** is one node:

```json
{"path":".store.books[1].title","kind":"string","line":7,"col":8,"value":"TAPL"}
```

- `kind` is jq's name for the type: object, array, string, number,
  boolean or null.
- `line` and `col` count from 1, columns in characters, and give where
  the node starts in the source: at its key when it has one, else at its
  value. They are exact for the JSON family, TOML, INI, CSV and ZON, and
  for C's tokens, from the places its syntax tree records; best-effort
  for YAML, XML, Markdown, CSS, PGN and expressions, loose for a `.proto`
  file's descriptor, and `null` when unknown, as throughout a grammar's
  spec, which its compiler writes.
- A container has `length`, its item count; a scalar has `value`. A
  string over 200 characters is cut to 200, with `"truncated": true` and
  its full `length`. Numbers are 64-bit floats, so an integer beyond
  2^53 loses precision; NaN and the infinities, which JSON cannot hold,
  are `"NaN"`, `"Infinity"` and `"-Infinity"` in an entry and `null` in
  `--json` output.

Inside the text, `--at` answers the node starting last at or before the
position on its line, the innermost of several starting there (a line
alone, or a column before the line's first node, that first node; on a
line with no node of its own, a comment or a closing bracket, the last
node before the line, or the document's first node when none is).
Outside the text it names nothing: a line past the last line, or a
column past the end of its line, where a line's text excludes its
terminator (LF or CRLF), a trailing terminator starts no line (`"a\n"`
has one), and an empty line has no column inside it.

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
prints `{"ok", "files": [{"file", "format", "ok", "error"}]}`, where a
failing file's `error` is the object a run on that file alone would
print (below): a parse error's fields for a file that does not parse,
`kind` and `message` alone for a directory.

**Errors** are JSON on standard error. Standard output then stays empty,
with one exception: a failed `--check` still prints its report there,
with status 1. A parse error:

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
| 2 | bad usage: an unknown option, a bad path, no input, a directory, a `--grammar` that does not compile, or the viewer without a terminal | `usage` |
| 3 | an input or a `--grammar` file could not be read, or the output could not be written | `io` |
| 4 | `--path` or `--at` names nothing | `not_found` |
| 5 | an input or a `--grammar` file is larger than `--max-size` | `too_large` |
| 6 | a parse, or a `--grammar` compile, ran longer than `--timeout`, or the input was still being read when it passed | `timeout` |

A `parse` or `io` error has `file`, `format`, `code` (the grammar's error
code, or `io`), `message`, `line`, `col`, `hint`, `source_line` and
`report`, the whole report the viewer shows, uncoloured; a field that
does not apply is `null` (`file` too, when it was standard output that
could not be written). A `not_found` error has the `path` or `at` it
was given, the entry of the `nearest` node, the deepest node the path
did reach or, for `--at`, the node a position inside the text would have
answered (`null` when no node has a position), and `keys`, that node's
first keys when it is an object and `null` otherwise. A `too_large`
error has the
fields of an `io` one plus the input's `size` (`null` for standard input,
which is read no further than the limit) and the `limit`, in bytes, and
its `hint` names the `--max-size` that would read it. A `timeout` error
has the fields of a `parse` one, its `line` and `col` showing how far the
parse got, plus the time limit in `seconds`; a parse that finished, but
late, fails the same way, with `line` and `col` `null` and a `hint`
saying how long it took. On standard input the time runs from
the start of the run, so waiting on it counts: an input still being read
when it passes (a writer that is slow, or sends nothing, or never closes
its end) fails as `timeout` too, with `line` and `col` `null` and a
message saying the input was still being read. On a file it runs from
the start of the parse, so each file `--check` reads gets the whole of
it. A `usage` error has only `kind` and `message`, the viewer's refusal
without a terminal among them,
except for a `--grammar` that does not compile, which adds the `grammar`
name and, when it came from a file, the `file`. A grammar file that
cannot be read is an `io` error, one over `--max-size` a `too_large`
error and a compile past `--timeout` a `timeout` error, each with the
fields of that kind, `file` the grammar file (`null` for
`--grammar-expr`), `format` `null`, and `size`, `limit` or `seconds` as
above, plus `grammar` ([Custom grammars](#custom-grammars)).
A document nested deeper than aless parses fails as a `parse` error with
the code `too_deep`: past about 1,000 levels, or sooner where the grammar
has a limit of its own (127 levels for JSON, JSONL, JSONic, JSON5, YAML,
TOML, INI, ZON and expressions, an expression of 127 operations too, 256
for XML, 512 for JSONC, 768 open rules for CSS, braces 100 deep for a
`.proto` file, counted before it is parsed, C's own: an expression of
127 operations, 1,024 open rules and a syntax tree 256 levels deep, and a
tree of more than 256 values a byte of the file written out (33,554,432
at most, however long the file) or of more than 2,048 bytes of names and
strings a byte (256 MB at most), which its reader's repeated subtrees
make of a few levels of nesting, and an ABNF,
EBNF or GBNF grammar's compiler's: groups and options about 500 deep in
ABNF, 130 in EBNF and 128 in GBNF, and elements nested 128 deep in any of
them). For a grammar from the
command line the 1,000 levels are measured on the value it built, once
the parse is done, and the error then has no `line` ([Custom
grammars](#custom-grammars)).

**Large inputs.** An input is read whole, and parsed whole, before
anything is printed: the tabnas grammars parse complete documents, so
there is no streaming, and the first byte of output comes when the parse
ends. That costs memory, about 40 bytes per byte of input, and time (see
[Performance](#performance)), which is why inputs over `--max-size` are
refused, before a file is read or as soon as standard input passes the
limit, and why `--timeout` exists: at around a megabyte a second, a
large input can take minutes, so a caller with a deadline of its own
should pass a shorter one, and get an error it can read rather than a
kill. Pipes are safe both ways: standard input can be a pipe or a file,
and a reader that stops early (`aless --json big.json | head`) ends
aless without an error, with status 0, though the parse has already been
paid for.
To take part of a large document, `--path` and `--depth` keep the output
small; the input is still parsed in full. Writing a document as another
format, and running a program over one, is the `dmog` command's work
([tabnas/dmog](https://github.com/tabnas/dmog), data mogrify): `dmog
translate --from yaml --to toml deploy.yaml`, `dmog run export.alc
response.json`. aless reads, shows and prints JSON.

## Formats

The format comes from the file extension; `--kind`, `:open PATH FORMAT`
and `:format FORMAT` override it. Anything unrecognised is shown as plain
text, one line per row, so every file is viewable.

| Format | Extensions | Parser |
|---|---|---|
| json | json, geojson, har, jsonld, webmanifest | tabnas-json |
| jsonl | jsonl, ndjson | tabnas-jsonl |
| jsonic | jsonic | tabnas-jsonic |
| jsonc | jsonc (trailing commas accepted, as editors accept them) | tabnas-jsonc |
| json5 | json5 | tabnas-json5 |
| yaml | yaml, yml | tabnas-yaml |
| toml | toml | tabnas-toml |
| ini | ini, cfg, conf, cnf | tabnas-ini |
| csv, tsv | csv; tsv, tab | tabnas-csv |
| xml | xml, svg, xhtml, xsd, xsl, xslt, plist | tabnas-xml |
| zon | zon | tabnas-zon |
| markdown | md, markdown (shown as its AST) | tabnas-markdown |
| feed | rss, atom (normalised to an Atom shape) | tabnas-feed |
| css | css (its syntax tree) | tabnas-css |
| proto | proto (its FileDescriptorProto, as protoc writes one in JSON) | tabnas-proto |
| pgn | pgn (chess games, each its tags and moves) | tabnas-chess |
| c | c, h (its concrete syntax tree, every token with its place) | tabnas-c |
| abnf | abnf (the grammar spec its compiler writes) | tabnas-abnf |
| ebnf | ebnf (the grammar spec its compiler writes) | tabnas-ebnf |
| gbnf | gbnf (the grammar spec its compiler writes) | tabnas-gbnf |
| expr | none, `-k expr` (jsonic with arithmetic, an operation the list of its operator and terms) | tabnas-expr |
| semver | none, `-k semver` (a version: its major, minor, patch, prerelease and build) | tabnas-semver |
| text | txt, text, log, anything else | none |

CSV and TSV show a list of records keyed by the header row. Map keys keep
their **source order**. A `.proto` file shows the FileDescriptorProto its
crate builds, not the syntax tree its grammar parses; an expression shows
its operations as S-expressions (`1+2*3` is `["+", 1, ["*", 2, 3]]`), as
the crate's fixtures write them; and a version is read without the line
break its file ends with, which the grammar, taking no whitespace, would
refuse. A C file shows its concrete syntax tree, preprocessor directives
parsed into it: each node its `kind`, `span`, `children` and `trivia`
(comments), each token its name (`tname`), text (`src`) and span, and the
members a node names its parts by (`operand`, `cond`, `declaredName`),
which repeat subtrees its `children` hold. It is read as plain C23,
following no `#include`, so a typedef a header declares is not known for
a type: `size_t x;` is kept as its tokens, with no declaration's
structure. A grammar in ABNF, EBNF or GBNF shows the grammar spec its
compiler writes, the tree a tabnas parser is made from: its `options`
(the fixed tokens it lexes, its start rule) and its `rule`s, each its
`open` and `close` alternates, with the helper rules and tokens the
compiler adds. The three notations compile to one kind of spec, and the
compiler keeps no places, so no node of a spec has a position. To read a file
with a grammar, rather than read the grammar, name it with `--grammar`.
Any other text format can be given a grammar of its own, named on the
command line: the next section.

## Custom grammars

A text format aless has no parser for can be read with a grammar you
write in [ABNF](https://github.com/tabnas/abnf) (RFC 5234, compiled by
tabnas/abnf). The command line ties the grammar to a name, and a file
whose extension or whole file name is that name is read with it:

```bash
aless --grammar hosts=hosts.abnf /etc/hosts              # the whole file name is `hosts`
aless --grammar hosts=hosts.abnf --json backup.hosts     # or the extension
aless --grammar crontab,cron=crontab.abnf /etc/crontab jobs.cron   # two names, one grammar
crontab -l | aless --grammar-expr 'jobs=…' -k jobs       # the text inline; -k for standard input
```

`--grammar NAME=FILE` reads the grammar from FILE (within `--max-size`);
`--grammar-expr NAME=ABNF` takes the text itself, everything after the
first `=`. Both repeat, for several grammars at once. NAME is the
format's name, as `--paths` and `--check` report it and as `-k` and
`:format` take it, and what the extension or whole name must be, in any
case. A name that is a built-in format's (`json`, `conf`) is allowed:
the grammar then reads that extension. A grammar file is a document of
its own as well: `aless hosts.abnf` shows the grammar spec it compiles
to, and `aless --check hosts.abnf` says whether it compiles, with the
compiler's message where it does not ([Formats](#formats)).

What the grammar builds is the document. With `; @object a b` and
`; @array` comments on its rules (the annotation syntax is
tabnas/abnf's, and [its guide](https://github.com/tabnas/abnf/blob/main/ts/doc/guide.md)
explains it) the value is JSON of strings: one member per part of the
rule that produces a value, nested where that part's own rule is
annotated. Without annotations it is the compiler's parse tree, a
`{"rule": …, "src": …, "kids": […]}` node per rule. Either way the
viewer, `--json`, `--paths`, `--find`, `--where` and `--check` work on
it as on any format. An empty file is `null`; one holding only
comments and blank lines is whatever the grammar builds from nothing,
`[]` for the grammars below.

The whole of `hosts.abnf`, from the grammar library under
[`tests/fixtures/grammars/`](tests/fixtures/grammars/):

```abnf
; /etc/hosts: an address and the host names it answers to, one per line.
; Comments start with # and blank lines are skipped, in every grammar here.
hosts   = *( entry %x0A / %x0A ) [ entry ]   ; @array
entry   = address names                      ; @object address names
address = word
names   = 1*word                             ; @array
word    = ( TX )
```

Run on the library's own sample (`tests/fixtures/grammars/hosts`, which
is detected by its whole name; standard input needs `-k`):

```
$ head -5 tests/fixtures/grammars/hosts
# /etc/hosts: static table lookup for hostnames.
# See hosts(5) for details.

127.0.0.1       localhost
127.0.1.1       workstation.example.com workstation
$ head -5 tests/fixtures/grammars/hosts | aless --grammar hosts=tests/fixtures/grammars/hosts.abnf -k hosts --json --compact
[{"address":"127.0.0.1","names":["localhost"]},{"address":"127.0.1.1","names":["workstation.example.com","workstation"]}]
$ aless --grammar hosts=tests/fixtures/grammars/hosts.abnf --where --at 5:17 --compact tests/fixtures/grammars/hosts
{"file":"tests/fixtures/grammars/hosts","format":"hosts","path":".[1].names[0]","kind":"string","line":5,"col":17,"value":"workstation.example.com"}
```

(`tests/agent.rs` runs those commands and holds their output to this.)

The library holds a grammar for each of `/etc/hosts`, `/etc/crontab` and
a user's crontab, `/etc/passwd`, `/etc/group`, `/etc/fstab`,
`/etc/resolv.conf` and shell-style `KEY=value` files, each with a sample
and the JSON it parses to, and [its README](tests/fixtures/grammars/README.md)
is the guide to writing one: how a line-oriented grammar names its
newlines (`%x0A`), where the engine's tokens `TX` (a word), `NR`, `ST`
and `VL` fit, and what the compiler refuses.

**Plain text.** A grammar reads its file as plain text, not as the JSON
the tabnas engine lexes by default: `{ } [ ] : ,` are ordinary
characters, `//` and `/* */` do not open comments, digits and quotes are
ordinary characters and `true` is a word, so `::1`, `root:x:0:0`, `0,30`
and `//server/share` are each one word (`TX`) until the grammar names a
literal (`":"` in a `passwd` grammar splits `root:x:0:0` into fields) or
a token class (`NR`, `ST` and `VL` bring numbers, quoted strings and
`true`/`false`/`null` back). `#` starts a comment to the end of the
line; spaces, tabs and a carriage return before a newline are skipped
between tokens, so a CRLF file reads as an LF file does; and a quoted
keyword (`"nameserver"`) matches whole words only. The engine settings
behind this are listed at the end of the library's README.

**Errors.** A grammar that does not compile is a usage error, raised
before any input is read: status 2, and without a screen
`{"error": {"kind": "usage", "message": "--grammar hosts: <the
compiler's message>", "grammar": "hosts", "file": "hosts.abnf"}}`
(`file` omitted for `--grammar-expr`); the compiler's message is one
line, without its colour codes. A repetition count over 1,024
(`2000"a"`, `1*5000word`) is refused the same way, since the compiler
writes out every copy and a count in the millions would take gigabytes
before any input was read; and so is a grammar whose repetitions would
have the compiler write more than 1,024 rules. It writes two for every
copy past a repetition's minimum (`1*255word` is 509 rules) and none for
the copies of a terminal up to it (`1024"a"`), one more for every copy
of a rule or a group, and a rule's repetitions again into every
alternative that starts with that rule (`doc = r "x"` writes `r`'s
twice). Rules cost time faster than they add up, in the compile and
again at the start of every parse, which assembles the grammar afresh:
in a release build 1,000 rules add about a second to each parse, and
2,000 add nine. That limit bounds what repetitions cost and nothing
else. Other shapes can make a grammar slow to compile, rules of several
alternatives that start with one another above all, since each copies
the other's alternatives, and `--timeout` is what bounds a compile. A grammar file that cannot be read is an `io`
error, status 3; one over `--max-size` is `too_large`, status 5, with
its `size` and the `limit`; and the compile is held to `--timeout` as a
parse is, with a `timeout` error, status 6, and the `seconds`. It runs
on a thread of its own, which cannot be interrupted, so past the limit
aless stops waiting for it and exits; when no thread can be started
(the process is out of threads or memory), it runs on aless's own
thread to its end instead, and one that ended past the limit is refused
all the same, its hint saying how long it took. Each has the fields of that kind (`file` the
grammar file, `format` `null`) plus `grammar`. An input the grammar
does not accept is a `parse` error like any other, with `format` the
grammar's name and the line and column the parse stopped at.

**Limits.** A grammar from the command line runs under the limits the
built-in grammars do. Its rule stack may reach 3,000 open rules, and a
repetition adds none: the compiler writes `*entry` as a loop that
re-enters its rule in one frame, so a file of any length costs the depth
of one line, and only the grammar's own recursion nests. Past the cap the
parse fails as `too_deep`, naming the open rules. Nesting is measured on
the value the grammar built as well, once the parse is done: over 1,000
levels is `too_deep` too, with no `line`. A parse takes about 2 KB of
memory a line and some 60 µs (300,000 lines of `hosts`: 18 s and 0.5 GB
in a release build; 86,000 lines: 5 s and 0.16 GB), so `--timeout` and
`--max-size` matter as for any format. That is with the engine's rule
history bounded, as it is for every parse aless runs: a rule reaches
back through `prev` three rules at most (`options.rule.history` set to
3), which is the deepest walk any grammar of the tabnas fleet makes, and
the value is the one the whole history gives. With the whole history
kept, every line's rules stayed reachable until the parse ended, and the
same 300,000 lines took 2.7 GB.

**Source positions** come from the token alignment every format has
([Source positions](#source-positions)): a value that is one token's
text (a `TX` word, `*` included) is placed exactly; one assembled from
several tokens or characters (a `1*DIGIT` rule, a group of several
parts, `gecos = *( word / " " )`) is not, and shows no position of its
own. `--where` at a position inside such a value answers, as on any
line, the last placed node before it: the field before it, or the
record.

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
| `s` | show the raw source, scrolled to the focused node's line and coloured by its grammar (`s` or `Esc` returns; `:` and `r` work there) |
| `!` | show the tab's parse error report in full (`h` `l` pan a long line) |
| `C-z` | suspend (Unix) |

Mouse: the wheel scrolls, and a click focuses a row (`--no-mouse` to
leave the mouse to the terminal).

The source view is coloured by what the tab's grammar lexed, with the
semantic tokens tabnas-lsp's language server sends an editor: strings
green, numbers magenta, keywords yellow, keys and names blue, comments
grey. The colours are made off the viewer's thread, and the text shows
plain until they come in. Text over 512 KiB stays plain, since colouring
parses the whole text and holds a few hundred times its size while it
does; so does plain text, a grammar in ABNF, EBNF or GBNF, which its
compiler lexes with a parser of its own, and a grammar the language
server's registry marks as lexing speculatively. Colouring one text stops
after ten seconds, or the parse timeout when that is shorter, and colours
what it had lexed. Without colour (`--no-color`, `NO_COLOR`) no text is
lexed for it.

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
on the next tick, with the folds and the focus kept. A directory's
modification time moves only as often as its file system's clock ticks,
16 ms or so on NTFS and up to two seconds on FAT, and the time is the
file system's own, which on a network share is its server's and need not
agree with this machine's. So for two seconds after a directory is read
with a new time, timed by this machine's own clock whatever that time
says, it is read again and compared on each tick, and a change in the
same tick still shows.

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
file watcher (inotify, FSEvents, ReadDirectoryChangesW, on the file's
directory, so an editor that saves by writing a temporary file and renaming
it is seen), debounced so a save that takes several writes reloads once;
a poll of the file's size and modification time on every half-second tick
catches anything the watcher misses.

A reload re-anchors the view instead of resetting it:

1. Folds are remembered **by path**: a container that is still there keeps
   its expanded or collapsed state; new containers start expanded.
2. The focused node is found again by path. If that path is gone, the
   focus goes to its nearest surviving ancestor and, within that ancestor,
   to the node **closest to the old source line**, so a renamed key or a
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
aless aligns that stream with the parsed tree (keys and leaf values in
document order against value-bearing tokens in source order, with a
bounded lookahead), so no grammar has to record provenance itself. The
alignment is exact for the JSON family, TOML, INI, CSV/TSV and ZON, and
best-effort where a grammar synthesises values (Markdown's AST, XML's
element records, YAML, CSS's and PGN's trees, an expression's operators):
a node the alignment cannot place shows no position and inherits none. A
`.proto` file's descriptor is derived rather than read, its statements
regrouped, so only its first values are placed reliably. A C syntax tree
records where each token is, so a token and its text are placed exactly
where it says, each time the tree repeats it, and what the reader adds (a
node's kind, a token's name, a span's own numbers) has no position. A
grammar's spec is written by its compiler, which keeps no places, so no
node of it has a position, and `--where` finds none.

## Modes

**Data mode** (default) is the streamlined view: keys unquoted when they
are identifiers, no commas, no closing brackets, collapsed containers shown
as `(N) {first: items, …}` previews. **Line mode** (`m`) shows every
line of a pretty-printed rendering: quoted keys, commas, closing brackets
that `%` jumps between. **Source view** (`s`) shows the file as it is on
disk with line numbers, the focused node's line highlighted.

## Copying

`y` commands use the system clipboard (the `clipboard` feature, on by
default, via `arboard`). When no clipboard is reachable, over SSH or in a
container, aless falls back to the terminal's OSC 52 sequence, which
xterm, kitty, WezTerm, iTerm2, Alacritty, foot and Windows Terminal turn
into a clipboard write; the status line says which happened. `p`
commands print instead, so the text can be read or copied by the
terminal.

## Performance

Parsing is the tabnas engine's, and it is a general rule engine rather
than a hand-written JSON parser: on one machine (a cloud VM, release
build) a 4.4 MB JSON document of records (315 thousand nodes) loads in
about three and a half seconds, a 12 MB one in nine and a 60 MB one (4.2
million nodes) in 45 (the median of three runs each). Memory peaks at
about 40 bytes per source byte while the parse runs (12 MB peaked at
0.42 GB, 60 MB at 2.1 GB); the steady state afterwards is much smaller.
Each rule's history is kept three steps deep ([Custom
grammars](#custom-grammars)), where the engine by default keeps every
rule until its container closes: with the whole history, the same
documents took 10 and 67 seconds and more than twice the memory, and a
flat array of 1.5 million numbers took 2.8 GB where it now takes 0.33
GB.

Three limits keep a large, slow or hostile input from taking the machine
down:

- **Size.** An input over `--max-size` (64 MB unless set; `0` removes the
  limit) is refused with a report that names the size that would read
  it.
- **Depth.** Nesting deeper than about 1,000 levels stops the parse with
  a `too_deep` error. Some grammars would otherwise recurse until the
  stack ran out and end the process, and slow down with the square of
  the depth well before that. Most stop sooner of their own accord,
  with the same error: JSON, JSONL, JSONic, JSON5, YAML, TOML, INI, ZON
  and expressions at 127 levels (an expression at 127 operations, too),
  XML at 256 open elements, JSONC at 512 levels, CSS at 768 open rules,
  a `.proto` file's braces at 100, counted before it is parsed, and C at
  its reader's limits (an expression of 127 operations, 1,024 open
  rules, a syntax tree 256 levels deep). A C tree also stops at 256
  values a byte of the file written out (a million at least, and
  33,554,432 at most, so no padding beside a chain raises it), and at
  2,048 bytes of names and strings a byte (8 MB at least, and 256 MB at
  most, so a long name in a repeated subtree is held too): its reader
  repeats a node's subtrees under the members that name them, so each
  level of a chain of subscripts or member accesses doubles it. A
  grammar in ABNF, EBNF or GBNF stops at its compiler's limits: groups
  and options nested past 2,048 open rules in ABNF (about 500 levels),
  520 in EBNF (about 130) and 512 in GBNF (about 128), and elements
  nested past 128 levels in any of them.
  The parse runs on a thread with a 64 MB stack, whatever the platform
  gives the main thread. A grammar from the command line runs under the
  same cap, and its nesting is measured on its value as well, once the
  parse is done ([Custom grammars](#custom-grammars)).
- **Time.** A parse that runs past `--timeout` stops with a `timeout`
  error showing how far it got. aless looks at the time between every two
  steps of the parser, but cannot cut a step short: one very long string
  is read to its end first, and a parse that finishes after the limit
  fails all the same. There is no default, since how long a parse should
  take depends on the machine; set one where time matters. Every format
  parses in time that grows in step with the input, at half a megabyte to
  a megabyte and a half a second on one machine (40,000 TOML
  `[[tables]]`, 1.6 MB, took under three seconds), so the limit matters
  most for large inputs. A grammar in ABNF, EBNF or GBNF is the
  exception: its compiler, which cannot be stopped part way, takes time
  that grows faster than its rules (10,000 rules, 300 KB, take under two
  seconds, 40,000 about fifteen), as a grammar from the command line
  does, and refuses, before building anything, a grammar its
  left-recursion pass would grow past 512 alternatives added to one
  rule, which a cycle of rules whose alternatives start with one another
  multiplies, or past 131,072 elements added to the whole grammar, which
  a long chain of rules led by one another copies. Past `--timeout`
  aless stops waiting for the compile and reports the `timeout`, and
  while that compile runs on it starts no other grammar's, which reports
  the same `timeout` at once, nor any once the time is up.

The parse blocks the interface, so a large file shows a `loading…` notice
before the screen is taken over, and a reload of a large watched file
pauses the viewer for as long as its parse takes. Navigation, folding and
search are independent of the engine and stay fast: rebuilding the rows
of a 2.4-million-node document takes about a second, and a search over it
under a second.

## Platforms

Linux, macOS and Windows are all first-class: CI builds and tests on the
three. Terminal handling is crossterm's and drawing ratatui's; the Unicode
fold markers need a font that has them (`--ascii` otherwise). On Windows
use Windows Terminal or another VT-capable console; piping into aless on
Windows requires a console to read keys from, so prefer a file argument
there. `C-z` suspend is Unix-only.

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

The engine (`tabnas`, the `tabnas-parser` package) and the grammar
crates come from crates.io, each a version requirement in `Cargo.toml`
pinned by `Cargo.lock`. `tabnas-abnf`, `tabnas-ebnf`, `tabnas-gbnf` and
the `tabnas-bnf` they share, the grammar notations' compilers (ABNF's
also behind `--grammar`), and `tabnas-lsp`, whose semantic tokens colour
the source view, come the same way. The other dependencies: crossterm
(terminal), ratatui (drawing), notify (file watching), regex (search),
unicode-width (layout), serde_json (JSON output), arboard (clipboard,
optional).

## Development

```bash
cargo fmt --all --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked                   # unit tests, fixture loading, headless app runs, the agent interface, the site
python3 scripts/pty-smoke.py          # unix: drives the built binary in a pseudo-terminal
cargo package --locked                # the crate builds from exactly the files it publishes
scripts/generate.sh                   # man/ and completions/, after an option or the help changes
```

Every option is in one table, `cli::OPTIONS`, and the parser reads no
option it does not list. `-h`, `--help`, the man page and the completions
are all written from it, and the committed `man/aless.1` and
`completions/` are what `scripts/generate.sh` writes: `tests/agent.rs`
fails when they differ from what the binary writes.

The documentation site is Markdown under `site/`, built by
`tests/site.rs`, which also runs every example on its pages and checks
their links; `ALESS_SITE_OUT=DIR cargo test --test site` writes the site
into `DIR`. `AGENTS.md` says how its pages are written.

Module map: `src/main.rs` is the only file that touches the terminal;
the library is terminal-free and unit tested:

| Module | Role |
|---|---|
| `doc` | the parsed value as a pre-order arena; visible rows; paths; folding |
| `explorer` | directory trees as documents, listed lazily |
| `fmt` | text of keys and values, previews, JSON output, path formats |
| `grammar` | custom grammars: `--grammar` and `--grammar-expr` parsed, ABNF compiled once, the registry `Format::Custom` indexes |
| `headless` | the agent interface: paths, listings, search, positions, checks, JSON errors |
| `cli` | every option, in one table the parser reads; the reference `--help` prints, the summary of `-h`, the man page and the shells' completions (`--generate`) |
| `load` | format detection; the tabnas grammars; the grammar notations' documents compiled to their specs; errors with positions; the size, depth and time limits; text fallback |
| `prov` | source positions by aligning the token stream with the tree |
| `search` | jless-style search patterns |
| `tab` | one open document: focus, scroll, mode, search, navigation, reload re-anchoring |
| `app` | tabs, modes, the source view, the key map, the command line, watch scheduling, yank |
| `highlight` | colour for the source view: tabnas-lsp's semantic tokens, made by a worker off the viewer's thread |
| `render` | the screen as ratatui widgets, every width measured as the terminal places text |
| `watch` | the notify file watcher |
| `clip` | clipboard and OSC 52 |

## License

MIT: see [LICENSE](LICENSE). jless is MIT too; its notice is in
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
