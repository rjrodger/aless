# aless

A [jless](https://jless.io)-style terminal viewer for **every format the
[tabnas](https://github.com/tabnas) parsers read** — JSON, JSON Lines,
jsonic, JSONC, JSON5, YAML, TOML, INI, CSV, TSV, XML, ZON, Markdown and
RSS/Atom feeds, and any text format you describe with an ABNF grammar
([Custom grammars](#custom-grammars)) — with **tabs** for several files
at once and a **watch mode** that reloads a file when it changes while
**keeping your place**.
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
aless --grammar hosts=hosts.abnf /etc/hosts   # a format of your own, from an ABNF grammar
aless --no-watch big.json            # do not reload on change
aless --mode line --line-numbers x.json
aless                                # explore the current directory; Enter opens a file
aless examples/solardemo-1.0.0-openapi-3.0.0.yaml   # an OpenAPI spec to try (see examples/)
```

| Option | Effect |
|---|---|
| `-k`, `--kind FORMAT` | parse every input as FORMAT instead of by extension; FORMAT may be a `--grammar` NAME |
| `--grammar NAME=FILE` | read files whose extension or whole name is NAME with the ABNF grammar in FILE; `NAME,NAME2=FILE` gives it two names; repeatable ([Custom grammars](#custom-grammars)) |
| `--grammar-expr NAME=ABNF` | the same, with the grammar text on the command line |
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
| `--timeout SECONDS` | stop a parse, or a `--grammar` compile, that runs longer than this (`2.5`, `90s`, `2m`; default none) |

`NO_COLOR` in the environment also disables colour.

## Scripts and agents

aless also runs without a screen. Give it any option from the table
below except those the viewer shares (`--depth`, `-k`, the `--grammar`
options and the limits), or let its standard output be something other
than a terminal (a pipe, a file, an agent's tool call), and it prints
JSON instead of starting the viewer. It
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
aless --render csv --path .items orders.json       # the records as CSV, streamed
aless --alchemy export.alc response.json           # a program over the document, streamed
```

| Option | Effect |
|---|---|
| `--json` | the document as JSON (the default), or the value at the start |
| `--paths` | an entry for the start and each node below it |
| `--find REGEX` | the entries of the nodes whose `"key": value` text matches: the viewer's search, so smart case (`REGEX/s` matches case) and `[ ] { }` literal unless escaped |
| `--where` | the start's entry |
| `--check` | parse every input and report on each |
| `--render csv\|json` | the records at the start as CSV, or the value there as JSON, streamed as the input is read ([Exporting](#exporting)) |
| `--alchemy FILE`, `--alchemy-expr TEXT` | run an alchemy program over the input and stream what it exports; `--render` names the renderer for a table or JSON events ([Programs](#programs)) |
| `--explain` | with `--alchemy`: the program's plan report as JSON, and no run |
| `--path PATH` | start at PATH instead of the root |
| `--at LINE[:COL]` | start at the node at that source position |
| `--depth N` | `--paths` and `--find` go at most N levels below the start |
| `--limit N` | at most N entries (default 200, 0 for all) |
| `--compact` | JSON on one line |
| `-k`, `--kind FORMAT` | parse as FORMAT; standard input is JSON unless this says otherwise |
| `--grammar NAME=FILE`, `--grammar-expr NAME=ABNF` | a format of your own, from an ABNF grammar ([Custom grammars](#custom-grammars)) |
| `--max-size SIZE` | refuse an input larger than SIZE (default `64M`; `0` for no limit) |
| `--timeout SECONDS` | stop a parse, or a `--grammar` compile, that runs longer than this (default none) |

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
| 1 | the input did not parse; with `--check`, some input failed and the report says which; with `--render` or `--alchemy`, the input or its records will not do (`INPUT_INVALID` and the other input, protocol and target codes) | `parse`, `transduce` |
| 2 | bad usage: an unknown option, a bad path, no input, a directory, a `--grammar` or an `--alchemy` program that does not compile, or the viewer without a terminal | `usage`, `alchemy` |
| 3 | an input, or an `--alchemy` program file, could not be read, or the output could not be written (`OUTPUT_FAILED`) | `io`, `transduce` |
| 4 | `--path` or `--at` names nothing | `not_found` |
| 5 | an input, a `--grammar` file or an `--alchemy` program file is larger than `--max-size`; with `--render` or `--alchemy`, over a limit of the transducer's (`RESOURCE_LIMIT_EXCEEDED`) | `too_large`, `transduce` |
| 6 | a parse, or a `--grammar` compile, ran longer than `--timeout` | `timeout` |

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
saying how long it took. A `usage` error has only `kind` and `message`,
except for a `--grammar` that does not compile, which adds the `grammar`
name and, when it came from a file, the `file`. A grammar file that
cannot be read is an `io` error, one over `--max-size` a `too_large`
error and a compile past `--timeout` a `timeout` error, each with the
fields of that kind — `file` the grammar file (`null` for
`--grammar-expr`), `format` `null`, and `size`, `limit` or `seconds` as
above — plus `grammar` ([Custom grammars](#custom-grammars)).
A document nested deeper than aless parses fails as a `parse` error with
the code `too_deep`: past about 1,000 levels, or sooner where the grammar
has a limit of its own (127 levels for JSON, JSONL, JSONic, JSON5, YAML,
TOML, INI and ZON, 256 for XML, 512 for JSONC). For a grammar from the
command line the 1,000 levels are measured on the value it built, once
the parse is done, and the error then has no `line` ([Custom
grammars](#custom-grammars)).

**Large inputs.** An input is read whole, and parsed whole, before
anything is printed: the tabnas grammars parse complete documents, so
there is no streaming, and the first byte of output comes when the parse
ends. That costs memory, about 80 bytes per byte of input, and time (see
[Performance](#performance)), which is why inputs over `--max-size` are
refused, before a file is read or as soon as standard input passes the
limit, and why `--timeout` exists: at around a megabyte a second, a
large input can take minutes, so a caller with a deadline of its own
should pass a shorter one, and get an error it can read rather than a
kill. Pipes are safe both ways: standard input can be a pipe or a file,
and a reader that stops early (`aless --json big.json | head`) ends
aless quietly with status 0, though the parse has already been paid for.
To take part of a large document, `--path` and `--depth` keep the output
small; the input is still parsed in full. `--render` and `--alchemy` are
the exceptions: they stream, as the next sections say.

### Exporting

`--render csv` writes a document's records as CSV, and `--render json`
writes the document as JSON text, both **streamed**: each record is
written as it is read, through the [tabnas
transducer](https://github.com/tabnas/transduce) and its
[renderers](https://github.com/tabnas/render), rather than after the
whole document has been built.

```bash
aless --render csv data.jsonl                                   # one line, one row
aless --render csv --path .response.payload.deep.records response.json
aless --render json big.yaml                                    # --json, streamed
aless -k jsonl --render csv < events.log                        # stdin, a record at a time
```

The **rows** are the elements of the root array, or of the array at
`--path`; for JSON Lines the lines, for CSV and TSV the records. The
**columns** are the members of the first row, in its order (a later row's
extra members are dropped; one it lacks is an empty field). Rows that are
all scalars make one column, `value`; a row that is neither an object nor
a scalar, or of the other kind than the first, fails the export with its
path. A root that is not an array fails with a message that says to give
`--path` to one. The CSV is the standard profile: every field quoted with
`"` doubled, records ending in CRLF, a header row, `null` and a missing
member both written as the empty string (so the two cannot be told apart:
this default export is lossy there, as the profile is), a nested container
written as compact JSON text in its cell, and a number as the source
spelled it where the source provides its lexeme (the JSON family, YAML,
ZON and JSON Lines). An empty array exports as nothing. `--render json`
takes `--compact` and `--indent` as `--json` does. Two things a stream
cannot do, since the input is read once, front to back: `--at` is not
accepted, and `[-1]` on an array (counting from the end) is a usage error,
though on an object it is the key `-1`, as everywhere. A document that
repeats a key on the exported path after the first was taken (`{"rows":
[…], "rows": […]}`) fails with `DUPLICATE_MEMBER` rather than export a
different value from the one `--json`, which keeps the last, would give.

```
$ aless --render csv --path .response.payload.deep.records response.json
"id","person","account"
"123","{""name"":""Alice""}","{""balance"":50.25}"
"456","{""name"":""Bob""}","{""balance"":72}"
```

**Errors** keep their shape. A failure of the transducer's is
`"kind": "transduce"`, with the transducer's `code` (`INPUT_INVALID`,
`RESOURCE_LIMIT_EXCEEDED`, `OUTPUT_FAILED`, …), its `message`, the
`file` and `format`, then `path`, `limit` (`{name, value}`), `line` and
`col` when the failure has them, and `output`: `"partial"` when some of
the result had been written before the failure (a stream cannot take bytes
back; the renderer writes whole records, and holds its output until the
end when it can), else `"none"`. The status follows the code, as the table
above says. aless's own limits report as they do for a parse, plus that
`output` field: nesting past its cap is a `parse` error with the code
`too_deep`, a run past `--timeout` a `timeout` error (the deadline covers
the whole run, the writing of a parsed value included), and a grammar
that panicked a `parse` error with the code `grammar`. A `--path` that
names nothing is `not_found`, its `nearest` entry without a source
position.

```
$ aless --render csv broken.json
{
  "error": {
    "kind": "transduce",
    "file": "broken.json",
    "format": "json",
    "code": "INPUT_INVALID",
    "message": "unexpected: unexpected character(s): }",
    "line": 2,
    "col": 8,
    "output": "none"
  }
}
```

**Streaming, honestly.** JSON Lines, CSV and TSV with the rows at the
root are read from the file a record (or a chunk of records) at a time,
so the file is never in memory whole and `--max-size` does not apply to
it; the transducer's own limits per record do (a line over
`max_record_bytes`, 64 MB, fails). Every other format is parsed whole by
the tabnas engine, within `--max-size`, and the note above about memory
per input byte stands. What differs is when the output starts: for the
JSON family, jsonic, YAML, ZON and Markdown the records are streamed out
as the parse proceeds, and the exported array is not kept behind them
(except in YAML, jsonic and Markdown, whose grammars may still refer to
it); for the rest they are streamed out after the parse, from the value
it built. Where one of those grammars refuses to stream a particular
document part-way (a jsonic implicit list whose first element is a
container, a YAML stream of several documents or a `<<` merge key, a
repeated member the grammar merges), aless falls back once to parsing it
whole and streaming its value, provided nothing has been written yet;
otherwise the refusal is reported with `output: "partial"`. `--timeout`
stops either kind at the deadline, with `output` saying whether records
had already been written.

`--render` on its own is the default export. Programs that select,
project and reshape on the way through are the next section's.

### Programs

`--alchemy FILE` runs a program in the
[alchemy](https://github.com/tabnas/alchemy) language over the input and
streams what it exports; `--alchemy-expr TEXT` takes the program on the
command line. A program's `export` receives the document as a stream of
JSON events and answers a text, a table or JSON events: aless writes a
text as it is, renders a table as CSV (`--render json` for JSON records,
one object per row keyed by the column labels) and JSON events as JSON
(`--render csv` for a table of them). The JSON a program renders is
compact, one document on one line. The program does the selecting, so
`--path` and `--at` are not accepted, and neither is any other output
option; `--render` given for a program that renders its own text is a
usage error. `--explain` prints the program's plan report as one JSON
object instead of running it (the chain of calls, the protocols, what is
retained and under which limits, the ordering contract, the renderer, the
guarantee and its qualification; `--compact` puts it on one line), and
reads no input. The
[language reference](https://github.com/tabnas/alchemy/blob/main/docs/language.md)
has the language; the worked example of the transducer's design, a table
bound by the document's own metadata, is
[`tests/fixtures/programs/export.alc`](tests/fixtures/programs/export.alc).

```bash
aless --alchemy export.alc response.json                  # the table the program binds, as CSV
aless --alchemy export.alc --render json response.json    # the same rows as JSON records
aless --alchemy-expr 'def export [input] input' data.yaml # the document, as JSON
aless -k jsonl --alchemy filter.alc < events.log          # stdin, a record at a time
aless --alchemy export.alc --explain                      # the plan; no run
```

```
$ aless --alchemy tests/fixtures/programs/export.alc tests/fixtures/records.json
"Identifier","Full name","Balance"
"123","Alice","50.25"
"456","Bob","72"
```

The input reaches the program the way an export reaches its renderer
([Streaming, honestly](#exporting)): JSON Lines, CSV and TSV a record at
a time, a verified grammar's events as the parse proceeds, with the parse
pruned under the rows the program's plan names, and every other grammar's
value after its parse; the same fallback when a grammar refuses to
stream, the same `--timeout` and depth caps, and the transducer's default
limits, which a failure names. What the run holds is what the program
retains (`--explain` says what, and under which limit), so a table over a
JSON Lines file of any size runs in bounded memory.

**Errors.** A program that does not compile (it does not parse, does not
type check, uses a stream twice, or cannot be shown to stream:
`DSL_PARSE_ERROR`, `DSL_TYPE_ERROR`, `STREAM_REUSED`,
`STREAMABILITY_UNKNOWN`) is the command's mistake, `"kind": "alchemy"`
with status 2: the language's `code`, its `message` (the finer code leads
it: `unbalanced`, `unknown_name`, `arity`, `protocol_mismatch`, …), `file`
(the program's path, or `--alchemy-expr`), `format` `null`, `line` and
`col` in the program when the failure has them, and `output`. A failure
of the program's own met once the input is open (a `match` no case takes,
a `--render` that does not fit what it exports) has the same shape plus
`input`, the document's name. A program file that cannot be read is an
`io` error and one over `--max-size` a `too_large` error, `file` the
program's and `format` `null`. Everything else reports as an export's
failure does: the transducer's codes as `transduce`
(`INPUT_ORDER_VIOLATION` when a row arrives before the metadata the
program binds its columns to, `RESOURCE_LIMIT_EXCEEDED` naming the
`limit`, `PROTOCOL_ORDER_ERROR`, …), with `output` saying whether anything
had been written, and aless's own limits as `parse`/`too_deep` and
`timeout`, with `output` too.

```
$ aless --alchemy-expr 'def export [input] (nope input)' data.json
{
  "error": {
    "kind": "alchemy",
    "file": "--alchemy-expr",
    "format": null,
    "code": "DSL_TYPE_ERROR",
    "message": "unknown_name: nope is not defined",
    "line": 1,
    "col": 21,
    "output": "none"
  }
}
```

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
their **source order**. Any other text format can be given a grammar of
its own, named on the command line: the next section.

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
the grammar then reads that extension.

What the grammar builds is the document. With `; @object a b` and
`; @array` comments on its rules (the annotation syntax is
tabnas/abnf's, and [its guide](https://github.com/tabnas/abnf/blob/main/ts/doc/guide.md)
explains it) the value is JSON of strings: one member per part of the
rule that produces a value, nested where that part's own rule is
annotated. Without annotations it is the compiler's parse tree, a
`{"rule": …, "src": …, "kids": […]}` node per rule. Either way the
viewer, `--json`, `--paths`, `--find`, `--where`, `--check` and
`--render` work on it as on any format; `--render` parses the input
whole before it streams, since no grammar from the command line is one
the transducer has verified. An empty file is `null`; one holding only
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
{"file":"tests/fixtures/grammars/hosts","format":"hosts","path":"[1].names[0]","kind":"string","line":5,"col":17,"value":"workstation.example.com"}
$ aless --grammar hosts=tests/fixtures/grammars/hosts.abnf --render csv tests/fixtures/grammars/hosts | head -3
"address","names"
"127.0.0.1","[""localhost""]"
"127.0.1.1","[""workstation.example.com"",""workstation""]"
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
levels is `too_deep` too, with no `line`. A parse takes about 9 KB of
memory a line and some 80 µs (300,000 lines of `hosts`: 25 to 45 s and
2.7 GB in a release build, the spread being how busy the machine was;
86,000 lines: 5 s, 0.8 GB), so `--timeout` and `--max-size` matter as
for any format.

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
  the depth well before that. Most stop sooner of their own accord,
  with the same error: JSON, JSONL, JSONic, JSON5, YAML, TOML, INI and
  ZON at 127 levels, XML at 256 open elements, JSONC at 512 levels.
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
  most for large inputs.

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

`tabnas-transduce` and `tabnas-render`, behind `--render`, come the same
way, as do `tabnas-abnf` and `tabnas-bnf`, the ABNF compiler behind
`--grammar`. The other dependencies: crossterm (terminal), notify (file
watching), regex (search), unicode-width (layout), serde_json (JSON
output), arboard (clipboard, optional).

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
| `export` | `--render`: records as CSV or the document as JSON, streamed through the tabnas transducer and renderers; the source plumbing `--alchemy` runs on |
| `alchemy` | `--alchemy`: a program in the alchemy language compiled, explained, and run over the input through `export`'s plumbing |
| `fmt` | text of keys and values, previews, JSON output, path formats |
| `grammar` | custom grammars: `--grammar` and `--grammar-expr` parsed, ABNF compiled once, the registry `Format::Custom` indexes |
| `headless` | the agent interface: paths, listings, search, positions, checks, JSON errors |
| `load` | format detection; the tabnas grammars; errors with positions; the size, depth and time limits; text fallback |
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
