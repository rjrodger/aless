---
title: Choose the format aless reads
description: Read a file whose name does not tell its format, or standard input, as the format you name.
order: 3
---
aless takes a file's format from its extension, and reads a file whose extension no format claims as plain text, an array of its lines. When the name is wrong or missing, you name the format.

## Name the format of a file

Give `-k` (or `--kind`, or `--format`) a format:

```sh
aless -k yaml deployment.conf
aless -k jsonl --paths --depth 1 events.log
```

`-k` applies to every file on the command line. In the viewer, `:format yaml` reads the focused tab again as YAML, and `:open PATH FORMAT` opens a file as the format you name.

Two formats have no extension of their own, so `-k` is the only way to read them: a semantic version, read without the line break its file ends with, and an arithmetic expression, each operation an array of its operator and its terms:

```console
$ printf '1.4.0-rc.1\n' | aless -k semver --json --compact
{"major":1,"minor":4,"patch":0,"prerelease":["rc",1],"build":[]}
$ printf 'total: 2*(3+4)\n' | aless -k expr --json --compact
{"total":["*",2,["(",["+",3,4]]]}
```

## Read standard input

aless reads standard input when it is not a terminal, as JSON unless `-k` says otherwise:

```console
$ printf 'name: aless\ntags: [viewer, cli]\n' | aless -k yaml --json --compact
{"name":"aless","tags":["viewer","cli"]}
```

`-` stands for standard input where a command takes files. Piped into the viewer, as in `curl -s https://api.example/items | aless`, the response opens in a tab of its own. On Windows the viewer needs a console to read keys from, so give it a file there rather than a pipe.

## When the format is wrong

A file read as the wrong format fails where the parse stops, with the status 1:

```console
$ aless --kind json --json --compact bookshelf-openapi.yaml; echo $?
{"error":{"kind":"parse","file":"bookshelf-openapi.yaml","format":"json","code":"unexpected","message":"unexpected character(s): o","line":1,"col":1,"hint":"The character(s) o do not match any rule alternative active at\nthis position.","source_line":"openapi: 3.0.0","report":"[tabnas/unexpected]: unexpected character(s): o\n  --> bookshelf-openapi.yaml:1:1\n  1 | openapi: 3.0.0\n      ^ unexpected character(s): o\n  2 | info:\n  3 |   title: Bookshelf demo API\n\n  The character(s) o do not match any rule alternative active at\n  this position.\n\n  --internal: tag=-; rule=val~o; token=#BD~unexpected; plugins=--"}}
1
```

The [format list](/reference/command-line.html#input-and-formats) gives every format aless reads, with the extensions that imply each one. A format it has no parser for can still be read with a grammar of your own: [Read a format aless does not know](/how-to/grammar.html).
