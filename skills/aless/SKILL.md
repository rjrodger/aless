---
name: aless
description: Read, query and validate structured files with the aless command-line tool, without its terminal viewer. Formats are JSON, JSON Lines, JSON5, JSONC, jsonic, YAML, TOML, INI, CSV, TSV, XML, ZON, Markdown, RSS/Atom, CSS, Protocol Buffers .proto files, PGN chess games, C source, grammars in ABNF, EBNF and GBNF, arithmetic expressions and semantic versions, plus any line-oriented text format described by an ABNF grammar given on the command line (/etc/hosts, crontabs, passwd, fstab). Use it to outline a large or unfamiliar file, to print the value at a path as JSON, and to find which path and source line a key or value is at. It also maps a line:col from a linter, test or stack trace to the structural path it points into. It converts any of those formats to JSON for jq, and checks that files parse, reporting the parser's exact error position and hint.
---

# aless, headless

aless is a jless-style terminal viewer for people. For you it is a CLI
that prints JSON: every run reads one file (or `--check` reads several),
prints one JSON value on standard output and exits 0, or prints
`{"error": {…}}` on standard error and exits non-zero. Standard output
is then empty, with one exception: a failed `--check` still prints its
report there (exit 1).

## Rules

1. **Always pass an output option**: `--json`, `--paths`, `--find`,
   `--where` or `--check`. aless also prints JSON whenever standard output
   is not a terminal, or `TERM=dumb` is set, but an explicit option
   guarantees it. The viewer is never what you want. Without a terminal
   it refuses with exit status 2 and a `usage` error; in a
   pseudo-terminal it would wait for keys.
2. **Name the file as an argument.** Standard input works, but it is
   parsed as JSON unless `-k FORMAT` (or `--format FORMAT`) says
   otherwise (`-k yaml`, `-k toml`, …).
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

Each failing file's `error` is the error a run on that file alone would
print. A file that does not parse carries `line`, `col`, `code`,
`message`, `hint`, `source_line` and `report`, the parser's full error
report with the lines around the error: fix the file at `line:col` and
check again. A file that cannot be read is an `io` error, and a
directory a `usage` error, `{kind, message}` and nothing more.

What each format reads as: an expression (`-k expr`) is read as
S-expressions, `1+2*3` as `["+",1,["*",2,3]]`; a version (`-k semver`)
as `{major, minor, patch, prerelease, build}`, its file's one line break
not part of it; a `.proto` file as its FileDescriptorProto, as protoc
writes one in JSON; a C file (`.c`, `.h`) as its concrete syntax tree,
each node `kind`, `span`, `children` and `trivia`, each token `tname`,
`src` and `span`, with members such as `operand` that repeat subtrees
`children` holds. C is read as plain C23 following no `#include`, so a
header's typedef is not known for a type. A grammar (`.abnf`, `.ebnf`,
`.gbnf`) is read as the grammar spec its compiler writes, `{options,
rule, v, meta}`, each rule its `open` and `close` alternates, with no
positions; `--check` on one says whether it compiles. To read a file
with a grammar, name it with `--grammar`.

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
name and the `line` and `col` it stopped at.

## Output

An **entry** describes one node:

```json
{"path":".spec.replicas","kind":"number","line":12,"col":3,"value":3}
```

- `path` is in jq syntax (a root array's items are `.[0]`, `.[1]`, …).
  Pass it back to `--path`, or to jq, unchanged.
- `kind` is one of object, array, string, number, boolean or null.
- `line` and `col` count from 1, columns in characters, and point where
  the node starts: at its key if it has one, else at its value. They are
  exact for the JSON family, TOML, INI, CSV and ZON, and for C's tokens
  (from the places its tree records; what the reader adds, a kind, a
  token's name, a span's numbers, has none), best-effort for YAML, XML,
  Markdown, CSS, PGN and expressions, loose for a `.proto` file's
  descriptor, and `null` when unknown, as throughout a grammar's spec.
- A container has `length`, its item count. A scalar has `value`.
  Strings over 200 characters are cut, and get `"truncated": true` and
  their full `length`.

What each option prints:

| Option | Shape |
|---|---|
| `--json` | the value itself, indented 2 spaces a level (`--indent N` sets it, `--compact` puts it on one line) |
| `--paths` | `{file, format, path, entries: [entry…], total, limit, truncated}` |
| `--find RE` | `{file, format, path, pattern, matches: [entry…], total, limit, truncated}` |
| `--where` | `{file, format, …entry}` |
| `--check` | `{ok, files: [{file, format, ok, error}]}` |

`file` is the path as given, or `-` for standard input.
`truncated: true` means `total` exceeded `--limit` (default 200). Raise
it (`--limit 0` is unlimited), or narrow with `--path` or `--depth`.

Exit statuses, and the `error.kind` that goes with each:

| Exit | Error kind | Meaning |
|---|---|---|
| 0 | none | success |
| 1 | `parse` | the input did not parse; with `--check`, a file failed |
| 2 | `usage` | bad option or path syntax, no input, a directory, a `--grammar` that does not compile, or no terminal for the viewer |
| 3 | `io` | the file or a `--grammar` file could not be read, or standard output could not be written |
| 4 | `not_found` | `--path` or `--at` named nothing |
| 5 | `too_large` | the input or a `--grammar` file is larger than `--max-size` (default 64M) |
| 6 | `timeout` | the parse, or a `--grammar` compile, ran longer than `--timeout` (default none; `--timeout 0` is none too), or the input was still being read when it passed |

A `usage` error is `{kind, message}` and nothing more (a `--grammar`
that does not compile adds `grammar`, and `file` when it came from one),
the viewer's refusal without a terminal included. `--compact` puts any
error on one line.

A `not_found` error carries `nearest`, the entry of the deepest node the
path reached, or for `--at` of the node a position inside the text would
have answered (`null` when no node has a position), and `keys`: that
node's first keys when it is an object, else `null`. Use them to correct
the path.

## Paths

jq syntax: `.`, `.a.b`, `.a[0]`, `.a[-1]` (the last item), `."odd key"`,
`.["a.b"]`, and `.[0]` for a root array's first item. Also accepted:
`a.b[0]` and `[0]` without the leading dot, `$.a['b'][0]` and JSON
Pointer `/a/b/0`. There are no wildcards, slices or recursive descent.
For those, pipe `--json` into jq.

## Caveats

- Numbers are 64-bit floats: integers beyond 2^53 lose precision in
  `--json` and in entries, so compare large IDs as text with `--find`
  rather than as numbers.
- `--json` writes NaN and the infinities as `null`. Entries write them
  as `"NaN"`, `"Infinity"` and `"-Infinity"`, with kind `number`.
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
  YAML, TOML, INI, ZON and expressions (and 127 operations in one
  expression), 256 for XML, 512 for JSONC, 768 open rules for CSS,
  braces 100 deep for a `.proto` file, counted before it is parsed, and
  C's own: 127 operations in an expression (a call's arguments among
  them), 1,024 open rules and a syntax tree 256 levels deep, these with
  no `line` when the parse stopped between tokens; and a C tree of more
  than 256 values a byte of the file written out (a million at least,
  33,554,432 at most), or of more than 2,048 bytes of names and strings
  a byte (8 MB at least, 256 MB at most), which its reader's repeated
  subtrees make of a chain of subscripts or member accesses a few levels
  deep, also with no `line`; and a grammar in ABNF, EBNF or GBNF at its compiler's limits,
  groups and options nested about 500 levels in ABNF, 130 in EBNF and 128
  in GBNF (no `line`), and elements nested 128 levels (the rule's
  `line`); a grammar the compiler's left-recursion pass would grow past
  512 alternatives added to one rule or 131,072 elements added to the
  grammar is code `compile`, at the rule. A grammar's compile cannot be
  stopped part way: past
  `--timeout` aless stops waiting for it (`timeout`, exit 6), and a later
  grammar in the same run is the same `timeout` at once while that
  compile still runs, as is one whose time is up before its compile
  starts. A `--grammar`
  grammar's nesting is measured on the value it built, after the parse
  (the error then has no `line`); a file of any length parses at the
  depth of one line, at about 2 KB of memory and 60 µs a line.
- A parse runs at about a megabyte a second, so a big file can outlast
  your command runner. If the runner has a timeout, pass `--timeout` a
  few seconds shorter (`--timeout 50` under a 60 s limit). A slow parse
  then ends with a `timeout` error, exit 6, showing how far it got,
  instead of being killed without a word. There is no limit by default,
  and `--timeout 0` asks for none.
- To write a document as another format, or to run a program over one,
  use the `dmog` command (tabnas/dmog): `dmog translate --from FORMAT
  --to FORMAT FILE`, `dmog run PROGRAM FILE`. aless reads, and prints
  JSON.
- `aless --help` is the full reference: every option, what each output
  prints, every error kind with its fields, the exit statuses, paths,
  positions, formats and limits. It opens with this interface; `aless -h`
  is a summary of the options. `aless --generate skill` prints this skill,
  so an agent can be given it wherever aless is installed.
