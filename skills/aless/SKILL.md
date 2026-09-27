---
name: aless
description: Read, query, validate and export structured files with the aless command-line tool, without its terminal viewer. Formats are JSON, JSON Lines, JSON5, JSONC, jsonic, YAML, TOML, INI, CSV, TSV, XML, ZON, Markdown and RSS/Atom, plus any line-oriented text format described by an ABNF grammar given on the command line (/etc/hosts, crontabs, passwd, fstab). Use it to outline a large or unfamiliar file, to print the value at a path as JSON, and to find which path and source line a key or value is at. It also maps a line:col from a linter, test or stack trace to the structural path it points into. It converts any of those formats to JSON for jq, exports the records in one as CSV (streamed, so JSON Lines and CSV of any size), and checks that files parse, reporting the parser's exact error position and hint.
---

# aless, headless

aless is a jless-style terminal viewer for people. For you it is a CLI
that prints JSON: every run reads one file (or `--check` reads several),
prints one JSON value on standard output and exits 0, or prints
`{"error": {…}}` on standard error and exits non-zero.

## Rules

1. **Always pass an output option**: `--json`, `--paths`, `--find`,
   `--where`, `--check` or `--render`. aless also prints JSON whenever standard output
   is not a terminal, but an explicit option guarantees it. The viewer is
   never what you want. Without a terminal it refuses with exit status 2;
   in a pseudo-terminal it would wait for keys.
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
for `--grammar-expr`); a grammar file that cannot be read exits 3
(`io`, with the same `grammar` and `file`); an input the grammar
rejects is a `parse` error, exit 1, with `format` the grammar's name
and the `line` and `col` it stopped at. `--render csv` works on the
records (the input is parsed whole first).

## Output

An **entry** describes one node:

```json
{"path":".spec.replicas","kind":"number","line":12,"col":3,"value":3}
```

- `path` is in jq syntax. Pass it back to `--path` unchanged.
- `kind` is one of object, array, string, number, boolean or null.
- `line` and `col` count from 1 and point where the node starts: at its
  key if it has one, else at its value. They are exact for the JSON
  family, TOML, INI, CSV and ZON, best-effort for YAML, XML and Markdown,
  and `null` when unknown.
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

`file` is the path as given, or `-` for standard input.
`truncated: true` means `total` exceeded `--limit` (default 200). Raise
it (`--limit 0` is unlimited), or narrow with `--path` or `--depth`.

Exit statuses, and the `error.kind` that goes with each:

| Exit | Error kind | Meaning |
|---|---|---|
| 0 | none | success |
| 1 | `parse`, `transduce` | the input did not parse; with `--check`, a file failed; with `--render`, the input or its records will not do |
| 2 | `usage` | bad option or path syntax, no input, a directory, a `--grammar` that does not compile, or no terminal for the viewer |
| 3 | `io`, `transduce` | the file could not be read, or standard output could not be written |
| 4 | `not_found` | `--path` or `--at` named nothing |
| 5 | `too_large`, `transduce` | the input is larger than `--max-size` (default 64M); with `--render`, over a limit of the transducer's |
| 6 | `timeout` | the parse ran longer than `--timeout` (default none) |

A `not_found` error carries `nearest`, the entry of the deepest node the
path reached. When that node is an object it also carries `keys`, its
first keys. Use them to correct the path.

A `transduce` error comes from `--render`: `code` is the transducer's
(`INPUT_INVALID`, `RESOURCE_LIMIT_EXCEEDED`, `OUTPUT_FAILED`, …), and it
carries `message`, `file`, `format`, then `path`, `limit` (`{name,
value}`), `line` and `col` when the failure has them, and `output`:
`"partial"` if some of the result had already been written (a stream
cannot take it back), else `"none"`. `INPUT_INVALID` with a message that
names `--path` means the value is not an array of records: point `--path`
at one. `DUPLICATE_MEMBER` means a key on the exported path is repeated
in the document: `--json` keeps the last value, a stream cannot, so it
refuses rather than export a different one. `[-1]` on an array is a usage
error under `--render` (a stream cannot count from the end); on an object
it is the key `-1`. A `--render` run stopped by `--timeout` or by nesting
reports `timeout` or `parse`/`too_deep` as any parse does, plus `output`.

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
  prints anything, using about 80 bytes of memory per byte of input: 13 MB
  takes about 1 GB and some seconds. Inputs over `--max-size` (default
  64M) fail with exit 5, and the `hint` says what size would read them.
  Raise the limit only if the machine has the memory; `--path` and
  `--depth` shrink the output, not the parse.
- A document nested deeper than aless reads fails with code `too_deep`
  rather than crashing: past about 1,000 levels, or sooner where the
  grammar has a limit of its own: 127 for JSON, JSONL, JSONic, JSON5,
  YAML, TOML, INI and ZON, 256 for XML, 512 for JSONC.
- `--render` streams: JSON Lines, CSV and TSV are read a record at a
  time, whatever their size, and `--max-size` does not apply to them.
  Every other format is still parsed whole (and read within `--max-size`);
  the records leave as the parse proceeds for the JSON family, YAML and
  ZON, and after it for the rest. So for a huge export, prefer JSON Lines
  or CSV input, or convert once with `--render json`.
- A parse runs at about a megabyte a second, so a big file can outlast
  your command runner. If the runner has a timeout, pass `--timeout` a
  few seconds shorter (`--timeout 50` under a 60 s limit). A slow parse
  then ends with a `timeout` error, exit 6, showing how far it got,
  instead of being killed without a word.
- `aless --help` has the full option list. It opens with this interface.
