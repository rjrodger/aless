---
title: Check that files parse
description: Find out which of a set of files do not parse, where and why, in a terminal or a CI job.
order: 8
---
`--check` parses every file it is given and reports on each one, with the status 0 when all of them parse and 1 when any does not. The examples use [`books.json`](/examples/books.json), and [`broken.json`](/examples/broken.json), which has a trailing comma.

## Check some files

Name them:

```console
$ aless --check bookshelf-openapi.yaml books.json
{
  "ok": true,
  "files": [
    {"file":"bookshelf-openapi.yaml","format":"yaml","ok":true,"error":null},
    {"file":"books.json","format":"json","ok":true,"error":null}
  ]
}
```

Each file is read as its extension says, as anywhere else in aless, so one command checks JSON, YAML and TOML together.

## Read a failure

A file that does not parse carries the same error object a run on it alone would print, with the parser's code, the line and column, a hint, and the report the viewer shows:

```console
$ aless --check --compact books.json broken.json; echo $?
{"ok":false,"files":[{"file":"books.json","format":"json","ok":true,"error":null},{"file":"broken.json","format":"json","ok":false,"error":{"kind":"parse","file":"broken.json","format":"json","code":"unexpected","message":"unexpected character(s): ]","line":3,"col":36,"hint":"The character(s) ] do not match any rule alternative active at\nthis position.","source_line":"  \"tags\": [\"examples\", \"reference\",]","report":"[tabnas/unexpected]: unexpected character(s): ]\n  --> broken.json:3:36\n  1 | {\n  2 |   \"title\": \"A Book of Examples\",\n  3 |   \"tags\": [\"examples\", \"reference\",]\n                                         ^ unexpected character(s): ]\n  4 | }\n  5 | \n\n  The character(s) ] do not match any rule alternative active at\n  this position.\n\n  --internal: tag=-; rule=val~o; token=#CS; plugins=--"}}]}
1
```

The report is on standard output with the status 1, unlike other errors, which go to standard error.

## Check every file in a repository

In a CI job, hand it the files git tracks:

```sh
git ls-files -z '*.json' '*.yaml' '*.yml' '*.toml' | xargs -0 -r aless --check --
```

`-z` and `xargs -0` keep each name whole, spaces included, and `--` ends aless's options, so a name that starts with `-` is read as a file. `-r` runs nothing when no file matches; without it, aless would run with no file and read standard input.

The job fails when a file does not parse, and the report says which. Through `xargs` the status is 123 rather than aless's 1, and any status but 0 fails the job. A file that is too large or too slow fails too, with its own error: `--max-size` (64 MB unless you set it) and `--timeout` set the bounds, and the [reference](/reference/command-line.html#errors) gives each error's fields. A format of your own is checked with its grammar named, as in `aless --grammar hosts=hosts.abnf --check hosts`.

To see the same report in a terminal, open the file in the viewer: a file that does not parse shows the parser's report, with a caret under the place it stopped.
