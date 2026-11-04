---
title: Choose the format aless reads
description: Name the format for a file whose name does not say which format it holds, or for standard input, and read a tab again as another format.
group: Files and formats
order: 11
---
aless takes a file's format from its extension. A file whose extension no format claims opens as plain text. aless reads plain text as an array of the file's lines. When the name is wrong or missing, name the format yourself, with `-k` on the command line or `:format` inside aless.

## Name the format of a file

Give `-k` the format:

<!-- test: skip deployment.conf and events.log stand for files of your own, and the site publishes neither -->
```sh
aless -k yaml deployment.conf
aless -k jsonl events.log
```

`-k` applies to every file on the command line, so one `-k` covers a whole list of files. `--kind` and `--format` are the same option under longer names. A format's extension works as its name, so `-k yml` reads YAML too. For more information about every format and its extensions, see [Formats](/reference/formats.html).

## Read a tab again as another format

Inside aless, `:format FORMAT` reads the active tab's text again as FORMAT, the format you name. Use `:format` for a file whose extension misled aless. For example, type `:format yaml` on a `.conf` file that holds YAML.

A `:format` that fails leaves the document as it was. `!` then shows that attempt's report. aless keeps that report for `!` until the next reload or the next `:format`.

To open a file as the format you name, use `:open PATH FORMAT`, where PATH is the file's path.

## Read a format with no extension

Two formats have no extension of their own: a semantic version and an arithmetic expression. Naming the format is the only way to read either. A version piped in opens as its parts:

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

The status bar names the format, `semver`.

## Read standard input

aless reads standard input when standard input isn't a terminal. It reads the input as JSON unless `-k` says otherwise. Although the document comes from standard input, the keys you press still come from the terminal. Pipe YAML in and name the format:

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

Standard input opens in a tab of its own. aless names that tab `(stdin)`. For example, `curl -s https://api.example/items | aless` opens the response in such a tab. A `-` among the files stands for standard input, so `aless notes.json -` opens both.

## Open a grammar as a document

A grammar in ABNF, EBNF, or GBNF opens as the grammar spec its compiler writes. That spec is the options and rules a tabnas parser is made from. aless takes a grammar's notation from its extension: `.abnf` for ABNF, `.ebnf` for EBNF, or `.gbnf` for GBNF. Open a grammar as you would any file:

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

The grammar's own rules are under the top-level `rule`. To read a file with the grammar, rather than read the grammar itself, name the grammar with `--grammar`. For more information, see [Read a format aless does not know](/how-to/grammar.html).

## Recover from the wrong format

A file read as the wrong format opens as the parser's report. The report gives the line and column where the parse stopped. This command reads a YAML file as JSON:

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

The parse stopped at line 1, column 1, on the `o` of `openapi`. To read the tab again as the right format, type `:format yaml`. For more information about reading the report, see [Find out why a file does not parse](/how-to/errors.html).
