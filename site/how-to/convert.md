---
title: Convert a file to another format
description: Write a document, or the records in it, as CSV, YAML, TOML, a Markdown table or any other format aless can write, streamed as it reads.
order: 7
---
`--render FORMAT` writes the value aless read as another format. Every format it reads can be written as every format it has a writer for: `csv`, `ini`, `json`, `json5`, `jsonc`, `jsonic`, `jsonl`, `markdown`, `toml`, `xml`, `yaml` and `zon`. The examples convert [`books.json`](/examples/books.json), a list of three books.

## Records as CSV

The elements of the root array are the rows, and the first row's members are the columns:

```console
$ aless --render csv books.json 2>/dev/null
"title","author","year","tags"
"A Book of Examples","A. N. Author","1999","[""examples"",""reference""]"
"Shelves and How to Fill Them","B. Bookworm","2012","[""furniture""]"
"The Index","C. Cataloguer","2021","[]"
```

Every field is quoted, a nested array or object is written as its JSON text, and the lines end in CRLF. For records deeper in a document, start at them with `--path`, as in `aless --render csv --path .response.items api.json`.

## A document as YAML, TOML or a table

The same records as YAML:

```console
$ aless --render yaml books.json 2>/dev/null
- "title": "A Book of Examples"
  "author": "A. N. Author"
  "year": 1999
  "tags":
    - "examples"
    - "reference"
- "title": "Shelves and How to Fill Them"
  "author": "B. Bookworm"
  "year": 2012
  "tags":
    - "furniture"
- "title": "The Index"
  "author": "C. Cataloguer"
  "year": 2021
  "tags": []
```

One book as TOML, and all of them as a Markdown table:

```console
$ aless --render toml --path '.[0]' books.json 2>/dev/null
"title" = "A Book of Examples"
"author" = "A. N. Author"
"year" = 1999
"tags" = [ "examples", "reference" ]
$ aless --render markdown books.json 2>/dev/null
| title | author | year | tags |
| --- | --- | --- | --- |
| A Book of Examples | A. N. Author | 1999 | ["examples","reference"] |
| Shelves and How to Fill Them | B. Bookworm | 2012 | ["furniture"] |
| The Index | C. Cataloguer | 2021 | [] |
```

Strings and keys are quoted wherever the format quotes, so nothing reads back as another kind of value. A shape a format cannot hold, such as a null in TOML, fails with the `INPUT_INVALID` code and the status 1 rather than being written wrongly.

## Read what was not kept

Each format leaves something out, and aless says what on standard error when a conversion succeeds, which is why the examples above send standard error to `/dev/null`. Look at it on its own:

```console
$ aless --render yaml books.json 2>&1 >/dev/null
{
  "warning": {
    "kind": "loss",
    "message": "the document was written as yaml, which does not keep everything a document can hold",
    "file": "books.json",
    "render": "yaml",
    "loss": ["Comments are not kept.","Anchors and aliases are not kept: an alias is written as a copy of the value it names.","Tags are not kept.","Styles are not kept: every string and key is written double-quoted, and every collection in block style.","A stream of several documents is written as one document, a sequence of them."]
  }
}
```

The status is 0 all the same. CSV's list matters most: every value is written as text, so a number read back from CSV is a string:

```console
$ aless --render csv books.json 2>/dev/null > books.csv; aless --json --compact --path '.[0]' books.csv
{"title":"A Book of Examples","author":"A. N. Author","year":"1999","tags":"[\"examples\",\"reference\"]"}
```

## Large inputs

A conversion streams: JSON Lines, CSV and TSV are read a record at a time, whatever their size, and most other formats write their records as the parse goes. `--render json` keeps each number as the source spelled it, where `--json` writes the 64-bit value. [Large inputs, streaming and limits](/explanation/large-inputs.html) has the details, and [Reshape a document with a program](/how-to/alchemy.html) goes further than conversion, choosing and renaming the fields on the way.
