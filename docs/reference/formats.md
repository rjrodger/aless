---
title: Formats
description: Every format aless reads, the extensions that imply each, what each becomes in the tree, where its positions are exact, and how deep it may nest.
order: 3
---
aless takes a file's format from its extension, or from its whole name when a `--grammar` is named for it, and `-k FORMAT` names one for every input ([`--kind`](/reference/command-line.html#opt-kind)). A file whose extension no format claims is read as `text`. Standard input is read as `json` unless `-k` says otherwise.

Every format is read into one kind of tree: objects with their keys in source order, arrays, strings, numbers, `true`, `false` and `null`. [One tree for every format](/explanation/one-tree.html) says why.

## The formats

| Format | Extensions | Becomes |
|---|---|---|
| `json` | `json` `geojson` `har` `jsonld` `webmanifest` | its value |
| `jsonl` | `jsonl` `ndjson` | an array of the values on its lines |
| `jsonic` | `jsonic` | its value |
| `jsonc` | `jsonc` | its value; a trailing comma is accepted, as editors accept it |
| `json5` | `json5` | its value |
| `yaml` | `yaml` `yml` | its value; a file of several documents is an array of them |
| `toml` | `toml` | an object of its keys and tables |
| `ini` | `ini` `cfg` `conf` `cnf` | an object of its sections, each an object of its keys, with the keys before the first section beside them; a value is a string, apart from `true`, `false` and `null` |
| `csv` | `csv` | an array of records, each an object keyed by the header row, every value a string |
| `tsv` | `tsv` `tab` | as `csv`, its fields separated by tabs |
| `xml` | `xml` `svg` `xhtml` `xsd` `xsl` `xslt` `plist` | element records, each its `name`, `localName`, `attributes` and `children`, with text as strings among the children |
| `zon` | `zon` | its value |
| `markdown` | `md` `markdown` | its syntax tree: a `type` and `children` for each block and inline node |
| `feed` | `rss` `atom` | one Atom-shaped object, whether the feed was RSS or Atom |
| `css` | `css` | its syntax tree: a `type` for each rule, declaration, comment and at-rule |
| `proto` | `proto` | its FileDescriptorProto, as protoc writes one in JSON |
| `pgn` | `pgn` | an array of games, each its `tags`, its `moves` and its `result` |
| `c` | `c` `h` | its concrete syntax tree, read as C23 without following an `#include` |
| `abnf` | `abnf` | the grammar spec its compiler writes |
| `ebnf` | `ebnf` | the grammar spec its compiler writes |
| `gbnf` | `gbnf` | the grammar spec its compiler writes |
| `expr` | none: `-k expr` | its value, each operation an array of its operator and its terms |
| `semver` | none: `-k semver` | its `major`, `minor`, `patch`, `prerelease` and `build` |
| `text` | `txt` `text` `log`, and any file no format claims | an array of its lines |
| a `--grammar` NAME | NAME as an extension, or as a whole filename | what the grammar's annotations build ([Read a format aless does not know](/how-to/grammar.html)) |

A format's name, or one of its extensions, is what `-k`, `:format` and `:open PATH FORMAT` take: `-k yml` reads YAML and `-k md` Markdown.

### C, the grammar notations, expressions and versions

A C file's tree gives each node a `kind`, a `span`, its `children` and its `trivia` (comments), and each token a name (`tname`), its text (`src`) and its `span`. A node also names its parts (`operand`, `cond`, `declaredName`), which repeat subtrees its `children` hold. A type a header declares is unknown without the header, so `size_t x;` is kept as its tokens.

ABNF, EBNF and GBNF compile to one kind of spec, the tree a tabnas parser is made from: its `options` (the fixed tokens it reads, its start rule) and its `rule`s, each with its `open` and `close` alternates, and the rules and tokens the compiler adds.

An expression is jsonic with arithmetic, so `1+2*3` is `["+", 1, ["*", 2, 3]]`. A version is its parts: `1.4.0-rc.1+build.5` is `{"major": 1, "minor": 4, "patch": 0, "prerelease": ["rc", 1], "build": ["build", "5"]}`, read without the line break its file ends with.

## Numbers

A number in the tree is a 64-bit float, and keeps its source's spelling wherever JSON spells it the same way. `1.0`, `1e21` and an integer beyond 2^53 are shown, searched, copied and written by `:w` as the source writes them. A spelling JSON has no syntax for, such as `0x1F` in JSON5, `1_000` in TOML or `+1`, gives the value it stands for, `31`, `1000` and `1`, and the source view (`s`) shows the spelling.

## Source positions

The status bar shows the line and column where the focused node starts, and the source view opens there. How exact that is depends on the format:

| Format | Positions |
|---|---|
| the JSON family, `toml`, `ini`, `csv`, `tsv`, `zon` | exact, for every key and value |
| `c` | exact, for every token, each time the tree repeats it; none for what the reader adds (a node's `kind`, a token's `tname`, the numbers of a `span`) |
| `yaml`, `xml`, `markdown`, `feed`, `css`, `pgn`, `expr` | a best effort: most values are placed, and a value built from several tokens is not |
| `proto` | reliable for its first values, the imports, and a best effort after them |
| `abnf`, `ebnf`, `gbnf` | none: the compiler keeps no places |
| a `--grammar` NAME | exact for a value that is one token's text, such as a `TX` word, and none for one built from several |

A node with no position shows none, rather than its parent's. [Where each value came from](/explanation/positions.html) says how the positions are found.

## Nesting limits

A document nested deeper than its format reads opens as a parse error with the code `too_deep`. Every parse stops at 3,000 open rules, about 1,000 levels, and most formats stop sooner.

| Format | Stops at |
|---|---|
| `json`, `jsonl`, `jsonic`, `json5`, `yaml`, `toml`, `ini`, `zon` | 127 levels |
| `expr` | 127 levels, or an expression of 127 operations |
| `xml` | 256 open elements |
| `jsonc` | 512 levels |
| `css` | 768 open rules |
| `proto` | braces 100 deep, counted before the file is parsed |
| `c` | an expression of about 127 operations, 1,024 open rules, or a syntax tree 256 levels deep |
| `abnf`, `ebnf`, `gbnf` | groups and elements nested 128 deep (127 are read) |
| `markdown` | a tree 205 levels deep, past which it stops nesting, without an error |
| `feed` | 256 open elements, as XML |
| `pgn`, `csv`, `tsv` | 3,000 open rules, about 1,000 levels |
| a `--grammar` NAME | 3,000 open rules, or a value nested over 1,000 levels |

A C syntax tree repeats subtrees under the members that name them, so each level of a chain of subscripts or member accesses doubles the tree. aless stops before writing out a tree of more than 256 values for each byte of the file (a million at least, and 33,554,432 at most), or of more than 2,048 bytes of names and strings for each byte (8 MB at least, and 256 MB at most).

The other limits, `--max-size` and `--timeout`, hold for every format: [Large inputs and limits](/reference/command-line.html#large-inputs-and-limits).
