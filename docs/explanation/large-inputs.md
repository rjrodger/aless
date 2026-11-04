---
title: Large inputs and limits
description: What aless costs in time and memory, and why it has limits that stop a large or hostile input before it takes the machine down.
order: 4
---
aless parses with a general rule engine, and that sets its costs. Know them before you point it at a large file, or let it read input you did not write.

## A whole document at a time

aless reads the whole input, and parses the whole of it, before it draws anything. The grammars parse complete documents, so a tab shows nothing until its parse ends. A parse runs at about a megabyte a second, and holds about 40 bytes of memory for each byte of input while it runs. On one machine, a 4.4 MB JSON document loaded in about three and a half seconds, and a 60 MB one in 45 seconds, at a peak of 2.1 GB.

Moving around the tree afterwards is fast whatever its size, because the engine is no longer involved: folding, the search and drawing work on the tree, which is much smaller than the parse that built it. `--depth` folds the tree that is drawn, not the parse. Before a large file is read, aless says so on the terminal, and a reload of a large watched file holds the viewer for as long as the parse takes.

## Why there are limits

An input can be large, slow to parse, or written to hurt, and a viewer that reads whatever it is given can be made to fill the machine's memory or never finish. So aless sets bounds on what one input may cost:

- `--max-size`, 64 MB unless you set it, refuses a larger input before it is read, and names the size that would read it.
- A depth limit refuses a document nested deeper than its format reads, with the code `too_deep`.
- `--timeout`, off unless you set it, stops a parse that runs too long, and the tab says how far it got.

The depth limit cannot be switched off. Some grammars would recurse until the stack ran out and took the process with them, and slow down with the square of the depth well before that, so every parse stops at a fixed depth, and most formats stop sooner of their own accord. A C file has one more bound: its syntax tree repeats subtrees under the members that name them, so a chain of member accesses can double the tree at each step, and aless stops before writing out a tree out of all proportion to the file. [Formats](/reference/formats.html#nesting-limits) gives each format's limit.

The time limit has no default, since how long a parse should take depends on the machine. Set one where a file might take minutes. A grammar's compiler is the one reader it cannot stop part way, so past the limit aless stops waiting for the compile and reports a `timeout` instead.

None of the limits makes a hostile input free to parse: they bound what one can cost. Raise `--max-size` for a large file you trust, and keep it for input you do not. The [command line reference](/reference/command-line.html#large-inputs-and-limits) states every limit exactly.
