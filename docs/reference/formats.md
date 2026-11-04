---
title: Formats
description: Every format aless reads, the extensions that imply each, what each becomes in the tree, where each format's positions are exact, and how deep each can nest.
order: 3
---
aless takes a file's format from its extension. A grammar named NAME with `--grammar` claims a file whose extension or whole name is NAME. A file whose extension no format claims is read as `text`. Standard input is read as `json` unless `-k` names another format.

`-k FORMAT` names the format of every input, whatever its extension says. FORMAT is a format's name, such as `yaml`, or one of its extensions, such as `yml`. `-k` is also written [`--kind`](/reference/command-line.html#opt-kind).

Every format is read into one kind of tree. The tree holds objects, arrays, strings, numbers, `true`, `false`, and `null`. An object keeps its keys in source order. For more information, see [One tree for every format](/explanation/one-tree.html).

## Formats, extensions, and trees

Each row names a format, the extensions that imply it, and what a file of that format becomes:

| Format | Extensions | Becomes |
|---|---|---|
| `json` | `json` `geojson` `har` `jsonld` `webmanifest` | its value |
| `jsonl` | `jsonl` `ndjson` | an array of the values on its lines |
| `jsonic` | `jsonic` | its value |
| `jsonc` | `jsonc` | its value |
| `json5` | `json5` | its value |
| `yaml` | `yaml` `yml` | its value, or an array of its documents when it holds several |
| `toml` | `toml` | an object of its keys and tables |
| `ini` | `ini` `cfg` `conf` `cnf` | an object of its sections, each an object of its keys, with the keys before the first section beside them |
| `csv` | `csv` | an array of records, each an object keyed by the header row |
| `tsv` | `tsv` `tab` | as `csv`, with tabs between the fields |
| `xml` | `xml` `svg` `xhtml` `xsd` `xsl` `xslt` `plist` | element records, each its `name`, `localName`, `attributes`, and `children` |
| `zon` | `zon` | its value |
| `markdown` | `md` `markdown` | its syntax tree: a `type` and `children` for each block and inline node |
| `feed` | `rss` `atom` | one Atom-shaped object, for RSS and Atom alike |
| `css` | `css` | its syntax tree: a `type` for each rule, declaration, comment, and at-rule |
| `proto` | `proto` | its `FileDescriptorProto`, as `protoc` writes one in JSON |
| `pgn` | `pgn` | an array of games, each its `tags`, `moves`, and `result` |
| `c` | `c` `h` | its concrete syntax tree |
| `abnf` | `abnf` | the grammar spec its compiler writes |
| `ebnf` | `ebnf` | the grammar spec its compiler writes |
| `gbnf` | `gbnf` | the grammar spec its compiler writes |
| `expr` | none: `-k expr` | its value, each operation an array of its operator and its terms |
| `semver` | none: `-k semver` | its `major`, `minor`, `patch`, `prerelease`, and `build` |
| `text` | `txt` `text` `log`, and any file no format claims | an array of its lines |
| a `--grammar` NAME | NAME as an extension, or as a whole filename | what the grammar's annotations build |

`-k FORMAT`, `:format FORMAT`, and `:open PATH FORMAT` each take a format's name or one of its extensions, so `-k yml` reads YAML, and `-k md` reads Markdown. In `:open PATH FORMAT`, PATH is the file to open.

### Trees in detail

aless accepts a trailing comma in a `jsonc` file, as editors do.

An `ini` value is a string, apart from `true`, `false`, and `null`. Every `csv` or `tsv` value is a string. Text in an XML element is a string among its `children`.

Each node of a C file's tree has a `kind`, a `span`, its `children`, and its `trivia`. A node's `trivia` are its comments. Each token has its name as `tname`, its text as `src`, and its `span`. A node also names its parts, such as `operand`, `cond`, and `declaredName`. Each named part repeats a subtree that `children` holds.

aless reads a C file as C23. It follows no `#include`, so a type that a header declares is unknown. aless therefore keeps the declaration `size_t x;` as its tokens.

ABNF, EBNF, and GBNF compile to one kind of spec. The spec is the tree that a tabnas parser is made from. Its `options` name the start rule and the fixed tokens the parser reads. Its `rule`s each hold their `open` and `close` alternates. The spec also holds the rules and tokens the compiler adds.

An expression is jsonic with arithmetic, so `1+2*3` is `["+", 1, ["*", 2, 3]]`.

A version is its parts. The version `1.4.0-rc.1+build.5` is `{"major": 1, "minor": 4, "patch": 0, "prerelease": ["rc", 1], "build": ["build", "5"]}`. A line break at the end of the file is not read as part of the version.

A custom grammar's tree is what its annotations build. For more information, see [Read a format aless does not know](/how-to/grammar.html).

## Numbers

A number in the tree is a 64-bit float. The number keeps its source's spelling wherever JSON spells it the same way. The spellings `1.0` and `1e21`, and an integer beyond 2^53, are shown, searched, copied, and written by `:w` as the source writes them. A spelling that JSON has no syntax for gives the value it stands for. For example, `0x1F` in JSON5 gives `31`, `1_000` in TOML gives `1000`, and `+1` gives `1`. The [source view](/how-to/display.html#data-mode-and-line-mode) shows the spelling. Pressing `s` opens the source view.

## Source positions

The status bar shows the line and column where the focused node starts. The source view opens at that line. How exact the position is depends on the format:

| Format | Positions |
|---|---|
| the JSON family, `toml`, `ini`, `csv`, `tsv`, `zon` | exact, for every key and value |
| `c` | exact for every token, each time the tree repeats it, and none for what aless adds |
| `yaml`, `xml`, `markdown`, `feed`, `css`, `pgn`, `expr` | a best effort: most values are placed, and a value built from several tokens is not |
| `proto` | reliable for the imports, which are its first values, and a best effort after them |
| `abnf`, `ebnf`, `gbnf` | none: the compiler keeps no places |
| a `--grammar` NAME | exact for a value that is one token's text, such as a `TX` word, and none for one built from several |

A node with no position shows none, rather than its parent's. The values aless adds to a C tree are a node's `kind`, a token's `tname`, and the numbers of a `span`. These values have no position. For more information, see [Where each value came from](/explanation/positions.html).

## Nesting limits

A document nested deeper than its format reads opens as a parse error with the code `too_deep`. Every parse stops at 3,000 open rules, which is about 1,000 levels. Most formats stop sooner:

| Format | Stops at |
|---|---|
| `json`, `jsonl`, `jsonic`, `json5`, `yaml`, `toml`, `ini`, `zon` | 127 levels |
| `expr` | 127 levels, or an expression of 127 operations |
| `xml` | 256 open elements |
| `jsonc` | 512 levels |
| `css` | 768 open rules |
| `proto` | braces 100 deep, counted before the file is parsed |
| `c` | an expression of about 127 operations, 1,024 open rules, or a syntax tree 256 levels deep |
| `abnf`, `ebnf`, `gbnf` | groups and elements nested 128 deep, of which 127 are read |
| `markdown` | a tree 205 levels deep, past which nesting stops without an error |
| `feed` | 256 open elements, as XML |
| `pgn`, `csv`, `tsv` | 3,000 open rules, about 1,000 levels |
| a `--grammar` NAME | 3,000 open rules, or a value nested over 1,000 levels |

A C syntax tree repeats subtrees under the members that name them, so each level of a chain of subscripts or member accesses doubles the tree. aless stops before writing out a C tree of more than 256 values for each byte of the file. That limit is 1,048,576 values at least, and 33,554,432 at most. aless also stops before writing out a C tree of more than 2,048 bytes of names and strings for each byte of the file. That limit is 8 MB at least, and 256 MB at most.

The other limits, `--max-size` and `--timeout`, hold for every format. For more information, see [Command line](/reference/command-line.html#large-inputs-and-limits).
