---
name: aless
description: Read, query, validate and export structured files with the aless command-line tool, without its terminal viewer. Formats are JSON, JSON Lines, JSON5, JSONC, jsonic, YAML, TOML, INI, CSV, TSV, XML, ZON, Markdown and RSS/Atom, plus any line-oriented text format described by an ABNF grammar given on the command line (/etc/hosts, crontabs, passwd, fstab). Use it to outline a large or unfamiliar file, to print the value at a path as JSON, and to find which path and source line a key or value is at. It also maps a line:col from a linter, test or stack trace to the structural path it points into. It converts any of those formats to JSON for jq, exports the records in one as CSV (streamed, so JSON Lines and CSV of any size), writes any of them as any format that has a render (YAML, TOML, INI, XML, ZON, JSON Lines, a Markdown table, JSON, CSV), runs a program in the alchemy streaming language over one (select, project, reshape and render on the way through), and checks that files parse, reporting the parser's exact error position and hint.
---

# aless, headless

aless is a jless-style terminal viewer for people. For you it is a CLI
that prints JSON: every run reads one file (or `--check` reads several),
prints one JSON value on standard output and exits 0, or prints
`{"error": {…}}` on standard error and exits non-zero.

## Rules

1. **Always pass an output option**: `--json`, `--paths`, `--find`,
   `--where`, `--check`, `--render` or `--alchemy`. aless also prints JSON whenever standard output
   is not a terminal, but an explicit option guarantees it. The viewer is
   never what you want. Without a terminal it refuses with exit status 2;
   in a pseudo-terminal it would wait for keys. Never pass `--panes`: it
   opens the viewer, and `--render` or `--alchemy` given with it choose
   the viewer's output pane instead of printing.
2. **Name the file as an argument.** Standard input works, but it is
   parsed as JSON unless `-k FORMAT` says otherwise (`-k yaml`,
   `-k toml`, …).
3. **Check the exit status before reading standard output.** Only 0
   means standard output holds the answer (with `--check`, 1 still
   comes with a full report).
4. **Quote paths** for the shell: `--path '.items[0]'`, since the shell
   would glob `[0]`.
5. **Start small on a big or unknown file.** Use `--paths --depth 1`
   first, not `--json` on the whole document.

## Recipes

Outline a file, then drill in:

```bash
aless --paths --depth 1 config.yaml
aless --paths --depth 1 --path '.spec.template' config.yaml
```

Take a value, as JSON. Any format comes out as JSON, so jq can take over:

```bash
aless --json --path '.spec.containers[0]' deploy.yaml
aless --json deploy.yaml | jq '.spec.containers[].image'
```

Find keys or values. The pattern is a regex matched against each node's
`"key": value` text. Case is ignored unless the pattern has a capital
letter, or ends with `/s`. `[ ] { }` match themselves unless escaped.

```bash
aless --find '"image":' deploy.yaml      # every image key, with path and line
aless --find 'nginx' deploy.yaml         # any key or value mentioning nginx
aless --find ': 80$' --path .spec deploy.yaml   # every value 80 under .spec
```

Map a position (a linter's `deploy.yaml:42:7`) to the path it is in, or
a path to its line:

```bash
aless --where --at 42:7 deploy.yaml
aless --where --path '.spec.replicas' deploy.yaml
```

Inside the text, `--at` answers the node starting last at or before the
position on its line, the innermost of several starting there (a line
alone, or a column before the line's first node, that first node; on a
line with no node of its own, a comment or a closing bracket, the last
node before the line, or the document's first node when none is).
Outside the text it names nothing: a line past the last line, or a
column past the end of its line, where a line's text excludes its
terminator (LF or CRLF), a trailing terminator starts no line (`"a\n"`
has one), and an empty line has no column inside it. That is exit 4,
`not_found`, and its `nearest` is the node a position inside the text
would have answered.

Check that files parse, and see why one does not:

```bash
aless --check $(git ls-files '*.yaml' '*.yml' '*.toml' '*.json')
```

Each failing file carries `line`, `col`, `code`, `message`, `hint`,
`source_line` and `report`, the parser's full error report with the
lines around the error. Fix the file at `line:col` and check again.

Export the records in a file as CSV, or a whole document as JSON,
streamed as the file is read:

```bash
aless --render csv data.jsonl                                   # one line, one row
aless --render csv --path .response.payload.deep.records api.json
aless --render json big.yaml                                    # --json, streamed
aless -k jsonl --render csv < events.log                        # stdin, a record at a time
```

The rows are the elements of the array at `--path` (the root when no
path is given); for JSON Lines the lines, for CSV and TSV the records.
The columns are the first row's members, in its order; a member a later
row lacks is an empty field, as is `null`; a nested value is compact JSON
text in its cell. Every field is quoted, records end in CRLF, and a header
row comes first. Use `--paths --depth 2` first to find the array to
export. Standard output is CSV bytes, not JSON, when the status is 0.

Write any of those formats as any format that has a render, streamed the
same way: `--render yaml`, `toml`, `ini`, `xml`, `zon`, `jsonl`,
`markdown` (a table of the records), `json5`, `jsonc`, `jsonic`, `json`
or `csv`:

```bash
aless --render yaml data.csv                    # the records as YAML
aless --render toml --path .spec deploy.json    # the value at a path, as TOML
aless --render markdown --path .rows api.json   # records as a Markdown table
aless --render csv table.md                     # a Markdown table's rows as CSV
```

Each format is written in its always-quoted profile (every YAML string
and key double-quoted, every TOML key quoted and every table inline, ZON
field names as `.@"name"`), so nothing reads back as another kind;
numbers as the source spelled them, `.inf` and `.nan` where a format
spells them. A source whose shape the target cannot carry (a null in
TOML, a root that is not an array in JSON Lines, no table in Markdown)
fails typed with `INPUT_INVALID` and a message that says why. On success
standard output holds the document alone, and standard error holds
`{"warning": {"kind": "loss", "message", "file", "render", "loss"}}`,
where `loss` lists what a document written this way does not keep
(YAML: comments, anchors and aliases, tags, styles, several documents;
CSV: types and the null/missing difference) and `adapter` names the
inferred table or `records` when one ran between the shapes; JSON
declares no loss and prints nothing. Exit status 0 is success whatever
standard error holds. A member the input repeats (`{"a":1,"a":2}`) is
written once, with the last value as `--json` reads it, when nothing had
been written yet; otherwise the run fails with `DUPLICATE_MEMBER` and
`output: "partial"`. `--render` with a format that has no render (`rss`)
is a usage error that lists the ones that do.

Run a program in the [alchemy](https://github.com/tabnas/alchemy)
streaming language over a file: select, project, reshape and render on
the way through, in bounded memory. A program's `export` takes the
document as JSON events and answers a text (written as it is), a table
(CSV, or JSON records with `--render json`) or JSON events (JSON, or a
table with `--render csv`); `--render` with any format that has a render
writes the program's table or events as that format (`--render yaml`
writes a table as a sequence of mappings). The program does the selecting, so no
`--path`, `--at` or other output option goes with it. `--explain` prints
the program's plan as JSON (its chain, protocols, what it retains and
under which limit) and reads no input: run it first on a program you did
not write. The plan is the program's, so its `renderer` is the program's
own default, whatever `--render` names; a `--render` the program refuses
is the same usage error with `--explain` as without it.

```bash
aless --alchemy export.alc api.json                       # a table the program renders, as CSV
aless --alchemy table.alc --render json api.json          # the same rows as JSON records
aless --alchemy-expr 'def export [input] input' data.yaml # the document as JSON
aless -k jsonl --alchemy filter.alc < events.log          # stdin, a record at a time
aless --alchemy export.alc --explain                      # the plan; no run
```

The [language reference](https://github.com/tabnas/alchemy/blob/main/docs/language.md)
has the language; `tests/fixtures/programs/export.alc` in the aless
repository is the worked example, a table bound by the document's own
metadata and rendered by the program itself as CSV, and `table.alc`
beside it binds the same table for aless to render, so it is the one that
takes `--render`. A program that does not compile exits 2 with `{"error":
{"kind": "alchemy", "code", "message", "file", "line", "col", …}}`, the
`code` the language's (`DSL_PARSE_ERROR`, `DSL_TYPE_ERROR`,
`STREAM_REUSED`, `STREAMABILITY_UNKNOWN`), the `message` led by a finer
code (`unknown_name`, `arity`, `protocol_mismatch`, …) and `line:col` in
the program; fix the program there. The input's failures are `transduce`
errors as under `--render` (`INPUT_ORDER_VIOLATION`: a row came before
the metadata the program binds its columns to).

Read a file in a format aless has no parser for, with an ABNF grammar
(the syntax is tabnas/abnf's; `; @object` and `; @array` comments on a
rule say what it builds, and the values are the source text, as
strings). Ready-made grammars for `/etc/hosts`, crontabs, `/etc/passwd`,
`/etc/group`, `/etc/fstab`, `/etc/resolv.conf` and `KEY=value` files are
under `tests/fixtures/grammars/` in the aless repository, with a guide to
writing one:

```bash
aless --grammar hosts=hosts.abnf --json /etc/hosts          # files named `hosts` or `*.hosts`
aless --grammar crontab=crontab.abnf --paths --depth 1 /etc/crontab
crontab -l | aless --grammar crontab-user=crontab-user.abnf -k crontab-user --json
aless --grammar-expr 'kv=settings = *entry ; @array
entry = key "=" value ; @object key value
key = ( TX )
value = ( TX )' --json --compact settings.kv                  # the grammar inline
```

NAME (`hosts`) is the format's name, `format` in every output and what
`-k` takes, and what a file's extension or whole file name must be,
case-insensitively; `NAME,NAME2=FILE` gives one grammar two names; both
options repeat. A grammar that does not compile exits 2 before any
input is read, with `{"error": {"kind": "usage", "message": "--grammar
hosts: …", "grammar": "hosts", "file": "hosts.abnf"}}` (`file` absent
for `--grammar-expr`; a repetition count over 1,024, or repetitions that
would have the compiler write more than 1,024 rules, is refused the same
way: `1*255word` writes 509, and a rule's repetitions are written again
into every alternative that starts with that rule); a grammar
file that cannot be read exits 3 (`io`), one over `--max-size` exits 5
(`too_large`, with `size` and `limit`), and a compile past `--timeout`
exits 6 (`timeout`, with `seconds`), each with that kind's fields
(`file` the grammar file, `format` null) plus `grammar`; an input the
grammar rejects is a `parse` error, exit 1, with `format` the grammar's
name and the `line` and `col` it stopped at. `--render csv` works on
the records (the input is parsed whole first).

## Output

An **entry** describes one node:

```json
{"path":".spec.replicas","kind":"number","line":12,"col":3,"value":3}
```

- `path` is in jq syntax. Pass it back to `--path` unchanged.
- `kind` is one of object, array, string, number, boolean or null.
- `line` and `col` count from 1, columns in characters, and point where
  the node starts: at its key if it has one, else at its value. They are
  exact for the JSON family, TOML, INI, CSV and ZON, best-effort for
  YAML, XML and Markdown, and `null` when unknown.
- A container has `length`, its item count. A scalar has `value`.
  Strings over 200 characters are cut, and get `"truncated": true` and
  their full `length`.

What each option prints:

| Option | Shape |
|---|---|
| `--json` | the value itself |
| `--paths` | `{file, format, path, entries: [entry…], total, limit, truncated}` |
| `--find RE` | `{file, format, path, pattern, matches: [entry…], total, limit, truncated}` |
| `--where` | `{file, format, …entry}` |
| `--check` | `{ok, files: [{file, format, ok, error}]}` |
| `--render csv` | CSV text: a header row, then one record per row, all fields quoted, CRLF |
| `--render json` | the value itself, streamed |
| `--render yaml` | the value as one YAML document, streamed; `{"warning": {"kind": "loss", …}}` on standard error |
| `--alchemy FILE` | what the program exports: text as it is, a table as CSV (`--render json`: JSON records), JSON events as JSON |
| `--alchemy FILE --explain` | `{entry, output, protocol, chain, retention, …}`, the program's plan report |

`file` is the path as given, or `-` for standard input.
`truncated: true` means `total` exceeded `--limit` (default 200). Raise
it (`--limit 0` is unlimited), or narrow with `--path` or `--depth`.

Exit statuses, and the `error.kind` that goes with each:

| Exit | Error kind | Meaning |
|---|---|---|
| 0 | none | success |
| 1 | `parse`, `transduce` | the input did not parse; with `--check`, a file failed; with `--render` or `--alchemy`, the input or its records will not do |
| 2 | `usage`, `alchemy` | bad option or path syntax, no input, a directory, a `--grammar` or an `--alchemy` program that does not compile, or no terminal for the viewer |
| 3 | `io`, `transduce` | the file, or an `--alchemy` program file, could not be read, or standard output could not be written |
| 4 | `not_found` | `--path` or `--at` named nothing |
| 5 | `too_large`, `transduce` | the input, a `--grammar` file or an `--alchemy` program file is larger than `--max-size` (default 64M); with `--render` or `--alchemy`, over a limit of the transducer's, a program's output over `--max-output` (default 1G) among them |
| 6 | `timeout` | the parse, or a `--grammar` compile, ran longer than `--timeout` (default none), or the input was still being read when it passed; with `--render` or `--alchemy`, the whole run |

A `not_found` error carries `nearest`, the entry of the deepest node the
path reached, or for `--at` of the node a position inside the text would
have answered (`null` when no node has a position). When that node is an
object it also carries `keys`, its first keys. Use them to correct the
path.

A `transduce` error comes from `--render`: `code` is the transducer's
(`INPUT_INVALID`, `RESOURCE_LIMIT_EXCEEDED`, `OUTPUT_FAILED`, …), and it
carries `message`, `file`, `format`, then `path`, `limit` (`{name,
value}`), `line` and `col` when the failure has them, and `output`:
`"partial"` if some of the result had already been written (a stream
cannot take it back), else `"none"`. What was written ends at the end of
a record (a CSV row, a value directly inside the root JSON array or
object, a bare number there ending at the comma after it, a JSON Lines
line): every record whole when the failure came is
on standard output before the error is reported, none held back in a
buffer; a record half written is dropped, unless it is longer than 16 MB,
which is written as it comes (a JSON one to the end of one of its own
values). A program's own text is written an item at a time, and a `json`
or `csv` render in the program a record at a time; another format's
render (`--render yaml`) may stop inside a record. `INPUT_INVALID` with a
message that names `--path` means the value is not an array of records:
point `--path` at one. `DUPLICATE_MEMBER` means a key on the exported
path is repeated
in the document: `--json` keeps the last value, a stream cannot, so it
refuses rather than export a different one. `[-1]` on an array is a usage
error under `--render` (a stream cannot count from the end); on an object
it is the key `-1`. A `--render` run stopped by `--timeout` or by nesting
reports `timeout` or `parse`/`too_deep` as any parse does, plus `output`;
a record-at-a-time read (JSON Lines, CSV, TSV) stopped by `--timeout`
names the line the record it was reading starts on, with `col` null,
unless the deadline passed while standard input was still being read,
a writer slow or silent: then `line` and `col` are both null, as for
any input still being read when the time ran out.
An error met while `--render yaml` was writing (a `transduce`,
`too_deep` or `timeout` error) also carries `loss`, the sentences its
warning gives on success.

An `alchemy` error is the program's own: `code` is the language's,
`message` opens with the finer code, `file` is the program's path (or
`--alchemy-expr`), `format` is null, and `line` and `col` are in the
program when it has a position. A transducer limit met while the plan is
built (`RESOURCE_LIMIT_EXCEEDED` naming `max_plan_steps`) is the
program's too: a `transduce` error with status 5, `file` the program's,
`format` null and no `input`. Once the input is open, where a failure
came from decides whose it is. One from the program's sink is the
program's when its code is the language's (a `match` no case takes, the
evaluator's `recursion`: `alchemy`, status 2) or when it has a position,
whatever its code (`fail` refusing a record: the transducer's code, kind
and status, `INPUT_INVALID` as `transduce`, status 1), and is placed the
same way, `file`, `line` and `col` the program's, `format` null, plus
`input`, the document's name, since the events a program reads carry no
positions of the input's; fix the program there. One from the program's
sink with neither (a renderer's `MISSING_VALUE` over the rows the program
built) and every failure of the source's, whatever its code (a grammar's
refusal to stream part-way after output has left, `STREAMABILITY_UNKNOWN`
with `output: "partial"`), are the input's: a `transduce` error, or
`timeout`/`too_deep`, as under `--render`, with `output`. `--timeout`
covers the parse and the program together, so a program slow on one item
stops at it, with `line` and `col` null (a timeout raised in the
program's work on an item has no input position; one raised in the parse
shows how far the parse got). A program that writes more than
`--max-output` (default 1G, `0` for none) stops with a `transduce` error,
`RESOURCE_LIMIT_EXCEEDED` with `limit.name` `max_output_bytes`, status 5,
and a `hint` naming the option.

## Paths

jq syntax: `.`, `.a.b`, `.a[0]`, `.a[-1]` (the last item), `."odd key"`,
`.["a.b"]`. Also accepted: `a.b[0]`, `$.a['b'][0]` and JSON Pointer
`/a/b/0`. There are no wildcards, slices or recursive descent. For those,
pipe `--json` into jq.

## Caveats

- Numbers are 64-bit floats: integers beyond 2^53 lose precision, so
  compare large IDs as text with `--find` rather than as numbers.
- `--json` writes NaN and the infinities as `null`. Entries write them as
  `"NaN"`, `"Infinity"` and `"-Infinity"`, with kind `number`.
- Unknown extensions are read as plain text: an array of lines. Use `-k`
  to name the format, or `--grammar` to give the format one.
- Big files are costly. aless reads and parses the whole input before it
  prints anything, using about 40 bytes of memory per byte of input: 12 MB
  takes about 0.4 GB and some seconds. Inputs over `--max-size` (default
  64M) fail with exit 5, and the `hint` says what size would read them.
  Raise the limit only if the machine has the memory; `--path` and
  `--depth` shrink the output, not the parse.
- A document nested deeper than aless reads fails with code `too_deep`
  rather than crashing: past about 1,000 levels, or sooner where the
  grammar has a limit of its own: 127 for JSON, JSONL, JSONic, JSON5,
  YAML, TOML, INI and ZON, 256 for XML, 512 for JSONC. A `--grammar`
  grammar's nesting is measured on the value it built, after the parse
  (the error then has no `line`); a file of any length parses at the
  depth of one line, at about 2 KB of memory and 60 µs a line.
- `--render` streams: JSON Lines, CSV and TSV are read a record at a
  time, whatever their size, and `--max-size` does not apply to them.
  Every other format is still parsed whole (and read within `--max-size`);
  the records leave as the parse proceeds for the JSON family, jsonic,
  YAML, ZON and Markdown, and after it for the rest. A document one of
  those grammars refuses to stream part-way (a jsonic implicit list, a
  YAML `---` stream or `<<` merge key) is parsed whole and exported all
  the same, when nothing has been written yet. So for a huge export,
  prefer JSON Lines or CSV input, or convert once with `--render json`.
- `--alchemy` streams the same way, and holds only what the program
  retains (`--explain` says what, and under which limit): a table over a
  JSON Lines file of any size runs in bounded memory. The JSON a program
  renders is compact, one line.
- A parse runs at about a megabyte a second, so a big file can outlast
  your command runner. If the runner has a timeout, pass `--timeout` a
  few seconds shorter (`--timeout 50` under a 60 s limit). A slow parse
  then ends with a `timeout` error, exit 6, showing how far it got,
  instead of being killed without a word.
- `aless --help` has the full option list. It opens with this interface.
