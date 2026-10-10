---
title: Large inputs, streaming and limits
description: What aless costs in time and memory, which work streams and which reads a whole document, and the limits that keep a large or hostile input in check.
order: 5
---
aless parses with a general rule engine, and that sets its costs. They are worth knowing before you point it at a large file, or let it read input you did not write.

## A whole document at a time

Most of what aless does reads the whole input and parses the whole of it before it prints anything: the viewer, `--json`, `--paths`, `--find`, `--where` and `--check`. The grammars parse complete documents, so the first byte of output comes when the parse ends. That parse runs at about a megabyte a second, and holds about 40 bytes of memory for each byte of input while it runs: on one machine, a 4.4 MB JSON document loaded in about three and a half seconds and a 60 MB one in 45, at a peak of 2.1 GB. Moving around the tree afterwards is fast whatever its size, because the engine is no longer involved.

`--path` and `--depth` make an answer small, not the parse. A pipe that stops reading early, as `head` does, ends aless without an error.

## Streaming

`--render` and `--alchemy` work differently, because their output is a stream of records rather than one answer. JSON Lines, CSV and TSV are read a record at a time, so a file of any size passes through in the memory of one record. For the JSON family, jsonic, YAML, ZON and Markdown the records are written as the parse proceeds, and for most of them the records already written are not kept; the other formats are parsed whole and then streamed. A grammar that cannot stream a particular document, such as a YAML file holding several documents, is read whole instead, as long as nothing has been written yet. So is a JSON Lines file with a record that repeats a key, which its stream would hold twice; standard input cannot be read twice, so there that record fails the run.

A stream cannot take back what it wrote. When one fails part-way, what it had written stays on standard output, every record whole and none in part, and the error says `"output": "partial"`.

## The limits

Three limits keep an input from taking the machine down, whether it is large, slow or written to hurt:

| Limit | Default | Refuses |
|---|---|---|
| `--max-size` | 64 MB | an input over the size, before a file is read, or as soon as standard input passes it |
| depth | about 1,000 levels, lower in most grammars | a document nested deeper, with the `too_deep` code |
| `--timeout` | none | a parse past the time, with how far it got |

The depth limit is not optional. Some grammars would recurse until the stack ran out and take the process with them, and slow down with the square of the depth before that, so most grammars stop sooner of their own accord: JSON, JSON Lines, JSON5, jsonic, YAML, TOML, INI and ZON at 127 levels, XML at 256 open elements and JSONC at 512 levels. The time limit has no default, since how long a parse should take depends on the machine; a program with a deadline of its own should pass one a little shorter, and get an error it can read rather than a kill.

None of the limits makes parsing a hostile input free: they bound what one can cost. Raise `--max-size` for a large file you trust, and keep it for input you do not. The [reference](/reference/command-line.html#large-inputs-and-streaming) states every limit exactly.
