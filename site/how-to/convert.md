---
title: Convert a file to another format
description: Write a document, or the records in it, as CSV, YAML, TOML, a Markdown table or any other format aless can write, streamed as it reads.
order: 7
---
`--render FORMAT` writes the value aless read as another format. Every format it reads can be written as every format it has a writer for that can hold it: `csv`, `ini`, `json`, `json5`, `jsonc`, `jsonic`, `jsonl`, `markdown`, `toml`, `xml`, `yaml`, `zon`, `feed` (an Atom feed) and `expr` take any document, and `css`, `proto`, `pgn` and `semver` only their own kind ([below](#formats-that-hold-only-their-own-kind)). The examples convert [`books.json`](/examples/books.json), a list of three books.

## Records as CSV

The elements of the root array are the rows, and the first row's members are the columns:

```console
$ aless --render csv books.json 2>/dev/null
"title","author","year","tags"
"A Book of Examples","A. N. Author","1999","[""examples"",""reference""]"
"Shelves and How to Fill Them","B. Bookworm","2012","[""furniture""]"
"The Index","C. Cataloguer","2021","[]"
```

Every field is quoted, a nested array or object is written as its JSON text, and the lines end in CRLF. A value that is not an array is written as one row, and a row of another kind than the first has a field only where the first row's columns find one. For records deeper in a document, start at them with `--path`, as in `aless --render csv --path .response.items api.json`.

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
| A Book of Examples | A. N. Author | 1999 | \["examples","reference"\] |
| Shelves and How to Fill Them | B. Bookworm | 2012 | \["furniture"\] |
| The Index | C. Cataloguer | 2021 | \[\] |
```

Strings and keys are quoted wherever the format quotes, and a character that would start Markdown's inline markup is escaped, so nothing reads back as another kind of value.

## What a format cannot hold

A conversion does not fail because the target format has no way to say something. Each format declares a convention for what it cannot hold, and aless writes by it: a null in TOML is left out, NaN and the infinities are `null` in JSON and their names in CSV, and XML writes any document as the element tree its embedding declares, keeping each value's kind in a `type` attribute:

```console
$ aless --render xml --path '.[2]' books.json 2>/dev/null
<document type="object"><member name="title">The Index</member><member name="author">C. Cataloguer</member><member type="number" name="year">2021</member><member type="array" name="tags"/></document>
```

A document whose root a format cannot have is wrapped. A TOML or INI document is a table, so an array or a scalar is written as the one member of a table, named `items`:

```console
$ aless --render toml --path '.[0].tags' books.json 2>/dev/null
"items" = [ "examples", "reference" ]
```

`--key` names that member instead:

```console
$ aless --render toml --key tags --path '.[0].tags' books.json 2>/dev/null
"tags" = [ "examples", "reference" ]
```

JSON Lines and the record formats wrap the other way: a value that is not an array is written as the one element of an array. An Atom feed writes any document through the feed's embedding, each member or element an entry that carries its value, and an expression writes one as it is, an array whose first element is an operator in infix.

## Formats that hold only their own kind

Four formats have no convention for a document of another kind. CSS, `.proto` and PGN write only the tree their own documents read as, a stylesheet's syntax tree, a FileDescriptorProto or a database of games, so a conversion into one from anything else is refused before the input is read, with the status 2 and the route that can write one:

```console
$ aless --render proto --compact books.json; echo $?
{"error":{"kind":"usage","message":"--render proto: proto writes a proto-descriptor tree, the tree its own documents read as, and books.json is not one; a program that makes one can write it: --alchemy FILE --render proto"}}
2
```

That route is a [program](/how-to/alchemy.html) whose output is the tree the format reads as. A semantic version takes any document that is a version, an object with a `major`, a `minor` and a `patch`, and refuses any other with the code `TARGET_VALUE_UNREPRESENTABLE` and the status 1, before writing anything.

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

The status is 0 all the same. When a conversion wraps the root or reshapes the value between a tree and a table, the note's `adapters` names each step (`wrap-object`, `wrap-array`, `the inferred table` or `records`), and their sentences follow the format's own in `loss`. CSV's list matters most: every value is written as text, so a number read back from CSV is a string:

```console
$ aless --render csv books.json 2>/dev/null > books.csv; aless --json --compact --path '.[0]' books.csv
{"title":"A Book of Examples","author":"A. N. Author","year":"1999","tags":"[\"examples\",\"reference\"]"}
```

## Large inputs

A conversion streams: JSON Lines, CSV and TSV are read a record at a time, whatever their size, and most other formats write their records as the parse goes. `--render json` keeps each number as the source spelled it, where `--json` writes the 64-bit value. [Large inputs, streaming and limits](/explanation/large-inputs.html) has the details, and [Reshape a document with a program](/how-to/alchemy.html) goes further than conversion, choosing and renaming the fields on the way.
