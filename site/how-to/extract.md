---
title: Get values out of a file in a script
description: Print the value at a path as JSON, in any of the formats aless reads, for jq, a script or an agent to use.
order: 6
---
aless prints a value as JSON whatever format it read it from, so the same commands work on JSON, YAML, TOML, CSV and the rest. The examples read [`books.json`](/examples/books.json), a list of three books.

## Print the value at a path

`--json --path` prints the value at a path:

```console
$ aless --json --compact --path '.[1]' books.json
{"title":"Shelves and How to Fill Them","author":"B. Bookworm","year":2012,"tags":["furniture"]}
```

`--compact` puts it on one line; without it the value is indented two spaces a level, or as many as `--indent` says. Paths are jq's, and `.[-1]` counts from the end:

```console
$ aless --json --path '.[-1].title' books.json
"The Index"
```

A string comes out as a JSON string, quotes and all. Pipe it into `jq -r .` for the bare text.

## Use the path syntax you already have

JSONPath and JSON Pointer paths work too, so a path copied from another tool does not need rewriting:

```console
$ aless --json --path '$[0].tags[1]' books.json
"reference"
$ aless --json --path /0/author books.json
"A. N. Author"
```

Wildcards, slices and recursive descent are jq's work rather than aless's. Hand jq the whole document for those:

```sh
aless --json config.yaml | jq '.services[] | .image'
```

When standard output is a pipe, `--json` is the default, so `aless config.yaml | jq` works without it.

## Find where a value is

`--where --path` gives the line and column a value starts at, for an error message or an editor:

```console
$ aless --where --path '.[2].year' books.json
{
  "file": "books.json",
  "format": "json",
  "path": ".[2].year",
  "kind": "number",
  "line": 17,
  "col": 5,
  "value": 2021
}
```

## Handle a path that is not there

A path that names nothing exits with the status 4 and prints a `not_found` error on standard error, with the nearest node the path did reach. Check the status before you read standard output:

```sh
if title=$(aless --json --path '.[5].title' books.json); then
  echo "found $title"
else
  echo "no fifth book"
fi
```

The [command line reference](/reference/command-line.html#exit-status) lists every status, and [Ask a file questions from a script](/tutorials/scripting.html) walks through the outputs one at a time.
