---
title: Choose the format aless reads
description: Open a file whose name does not tell its format, or standard input, as the format you name.
group: In the viewer
order: 6
---
aless takes a file's format from its extension, and reads a file whose extension no format claims as plain text, an array of its lines. When the name is wrong or missing, you name the format.

## Name the format of a file

Give `-k` (or `--kind`, or `--format`) a format:

```sh
aless -k yaml deployment.conf
aless -k jsonl events.log
```

`-k` applies to every file on the command line. Inside aless, `:format yaml` reads the focused tab again as YAML, and `:open PATH FORMAT` opens a file as the format you name.

Two formats have no extension of their own, so `-k` is the only way to read them: a semantic version, read without the line break its file ends with, and an arithmetic expression, each operation an array of its operator and its terms. Piped in, a version opens as its parts:

```sh
printf '1.4.0-rc.1\n' | aless -k semver
```

```screen
▼ {
    major: 1
    minor: 4
    patch: 0
  ▽ prerelease: [
      "rc"
      1
 (stdin) .                                                         semver · 1:1
```

## Open a grammar as a document

A grammar in ABNF, EBNF or GBNF (`.abnf`, `.ebnf`, `.gbnf`) opens as the grammar spec its compiler writes, the options and rules a tabnas parser is made from:

```sh
aless hosts.abnf
```

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

## Read standard input

aless reads standard input when it is not a terminal, as JSON unless `-k` says otherwise, and takes its keys from the terminal:

```sh
printf 'name: aless\ntags: [viewer, cli]\n' | aless -k yaml
```

```screen
▼ {
    name: "aless"
  ▽ tags: [
      "viewer"
      "cli"

 (stdin) .                                                           yaml · 1:1
```

`-` stands for standard input where a command takes files. Piped into the viewer, as in `curl -s https://api.example/items | aless`, the response opens in a tab of its own. On Windows the viewer needs a console to read keys from, so give it a file there rather than a pipe.

## When the format is wrong

A file read as the wrong format opens as the parser's report, with the line and column where the parse stopped; `!` shows the whole report, and `:format yaml` reads the tab again as the right format:

```sh
aless --kind json bookshelf-openapi.yaml
```

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
```

The [format list](/reference/command-line.html#input-and-formats) gives every format aless reads, with the extensions that imply each one. A format it has no parser for can still be read with a grammar of your own: [Read a format aless does not know](/how-to/grammar.html).
