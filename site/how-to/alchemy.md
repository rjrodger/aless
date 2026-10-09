---
title: Reshape a document with a program
description: Run a program in the alchemy language over a document, to choose, rename and arrange its values as a table, written as CSV, JSON or any format aless writes.
order: 10
---
`--render` converts a whole document. When you want a table made of some of its values, under names you choose, a program does it. Programs are written in [alchemy](https://github.com/tabnas/alchemy), a small streaming language, and `--alchemy FILE` runs one over the input. The examples use [`records.json`](/examples/records.json), an API response whose rows are deep inside it, with the columns described in the response itself, and two programs, [`export.alc`](/examples/export.alc) and [`table.alc`](/examples/table.alc).

## Run a program

`export.alc` binds a table to the response's own metadata and writes it as CSV:

```console
$ aless --alchemy export.alc records.json
"Identifier","Full name","Balance"
"123","Alice","50.25"
"456","Bob","72"
```

The column names, `Identifier`, `Full name` and `Balance`, come from the document, and each column's values from the path the metadata gives for it.

## Choose the output format

`table.alc` builds the same table and leaves its writing to aless, so `--render` chooses the format:

```console
$ aless --alchemy table.alc --render json records.json 2>/dev/null
[{"Identifier":123,"Full name":"Alice","Balance":50.25},{"Identifier":456,"Full name":"Bob","Balance":72}]
$ aless --alchemy table.alc --render yaml records.json 2>/dev/null
- "Identifier": 123
  "Full name": "Alice"
  "Balance": 50.25
- "Identifier": 456
  "Full name": "Bob"
  "Balance": 72
```

A table is written as CSV unless `--render` says otherwise. A program that writes its own text, as `export.alc` does, refuses `--render`.

## Write a short program inline

`--alchemy-expr` takes the program on the command line. The shortest exports the document as it came:

```console
$ aless --alchemy-expr 'def export [input] input' books.json
[{"title":"A Book of Examples","author":"A. N. Author","year":1999,"tags":["examples","reference"]},{"title":"Shelves and How to Fill Them","author":"B. Bookworm","year":2012,"tags":["furniture"]},{"title":"The Index","author":"C. Cataloguer","year":2021,"tags":[]}]
```

## See what a program will do

`--explain` prints a program's plan instead of running it, and reads no input: the chain of calls, the protocols between them, and what the program holds in memory, under which limit.

```console
$ aless --alchemy table.alc --explain | head -12
{
  "entry": "export",
  "finite": false,
  "chain": [
    "table-from-json"
  ],
  "output": "TableRows/1",
  "passes": 1,
  "protocol": [
    "JsonEvents/1",
    "TableRows/1",
    "Text"
```

A program streams: it holds what its plan says it retains, here the column descriptions and one row at a time, so a table over a large input runs in little memory. The program does the choosing, so `--path` and `--at` do not go with `--alchemy`. The [language reference](https://github.com/tabnas/alchemy/blob/main/docs/language.md) has the language, and the [command line reference](/reference/command-line.html#errors) the errors a program can raise.
