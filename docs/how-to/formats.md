---
title: Choose the format aless reads
description: Open a file whose name does not tell its format, or standard input, as the format you name, and read a tab again as another format.
group: Files and formats
order: 11
---
aless takes a file's format from its extension, and reads a file whose extension no format claims as plain text, an array of its lines. When the name is wrong or missing, you name the format.

## Name the format of a file

Give `-k` (or `--kind`, or `--format`) a format:

```sh
aless -k yaml deployment.conf
aless -k jsonl events.log
```

`-k` applies to every file on the command line. A format's extension works as its name, so `-k yml` reads YAML too. [Formats](/reference/formats.html) lists every format and its extensions.

## Read a tab again as another format

Inside aless, `:format FORMAT` parses the focused tab's text again as FORMAT, for a file whose extension misled aless: `:format yaml` on a `.conf` file that holds YAML. `:open PATH FORMAT` opens a file as the format you name. A `:format` that fails leaves the document as it was, and `!` shows that attempt's report until the next reload or `:format`.

## Read a format with no extension

Two formats have no extension of their own, so a name is the only way to read them: a semantic version, and an arithmetic expression. Piped in, a version opens as its parts:

```sh
printf '1.4.0-rc.1\n' | aless -k semver
```

<!-- screen: printf '1.4.0-rc.1\n' | aless -k semver
size: 80x10 -->
```screen
▼ {
    major: 1
    minor: 4
    patch: 0
  ▽ prerelease: [
      "rc"
      1
    build: []
 (stdin) .                                                         semver · 1:1
```

## Read standard input

aless reads standard input when it is not a terminal, as JSON unless `-k` says otherwise, and takes its keys from the terminal:

```sh
printf 'name: aless\ntags: [viewer, terminal]\n' | aless -k yaml
```

<!-- screen: printf 'name: aless\ntags: [viewer, terminal]\n' | aless -k yaml
size: 80x8 -->
```screen
▼ {
    name: "aless"
  ▽ tags: [
      "viewer"
      "terminal"

 (stdin) .                                                           yaml · 1:1
```

A `-` among the files stands for standard input, so `aless notes.json -` opens both. A response piped in, as in `curl -s https://api.example/items | aless`, opens in a tab of its own, named `(stdin)`.

## Open a grammar as a document

A grammar in ABNF, EBNF or GBNF (`.abnf`, `.ebnf`, `.gbnf`) opens as the grammar spec its compiler writes, the options and rules a tabnas parser is made from:

```sh
aless hosts.abnf
```

<!-- screen: aless hosts.abnf
size: 80x14 -->
```screen
▼ {
  ▽ options: {
    ▽ fixed: {
      ▽ token: {
          "#T": "\n"
    ▽ rule: {
        start: "__start__"
    ▽ lex: {
        empty: true
  ▽ rule: {
    ▽ hosts: {
      ▽ open: [
 hosts.abnf .                                                   abnf · watching
```

To read a file with the grammar rather than read the grammar, name it with `--grammar` ([Read a format aless does not know](/how-to/grammar.html)).

## When the format is wrong

A file read as the wrong format opens as the parser's report, with the line and column where the parse stopped:

```sh
aless --kind json bookshelf-openapi.yaml
```

<!-- screen: aless --kind json bookshelf-openapi.yaml
size: 80x16 -->
```screen
[tabnas/unexpected]: unexpected character(s): o
  --> bookshelf-openapi.yaml:1:1
  1 | openapi: 3.0.0
      ^ unexpected character(s): o
  2 | info:
  3 |   title: Bookshelf demo API

  The character(s) o do not match any rule alternative active at
  this position.

  --internal: tag=-; rule=val~o; token=#BD~unexpected; plugins=--



 bookshelf-openapi.yaml .     !1:1: unexpected character(s): o  json · watching
bookshelf-openapi.yaml: 1:1: unexpected character(s): o
```

`:format yaml` reads the tab again as the right format. [Find out why a file does not parse](/how-to/errors.html) says how to read the report.
