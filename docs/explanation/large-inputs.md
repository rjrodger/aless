---
title: Large inputs and limits
description: What a parse costs in time and memory, and why aless sets limits that stop a large or hostile input before it takes the machine down.
order: 4
---
aless parses with a general rule engine, and the engine sets what a parse costs. Know those costs before you point aless at a large file, or before you let aless read input you did not write.

## What a parse costs

aless reads and parses the whole input before it draws anything. The grammars parse complete documents, so a tab shows nothing until its parse ends.

A parse runs at about 1 MB a second. On one machine, for example, a 4.4 MB JSON document loaded in about 3.5 seconds. On the same machine, a 60 MB JSON document loaded in 45 seconds. While a parse runs, it holds about 40 bytes of memory for each byte of input. At its peak, the parse of that 60 MB document held 2.1 GB of memory.

Once the parse ends, moving around the tree is fast whatever the tree's size, because the engine is no longer involved. Folding, the search, and drawing all work on the tree. That work stays fast because the tree is much smaller than the parse that built it. `--depth` folds the tree aless draws, not the parse.

Before aless reads a large file, it says so on the terminal. A reload of a large watched file holds the viewer for as long as the parse takes.

## Why aless sets limits

An input can be large, slow to parse, or written to hurt. A viewer that reads whatever it is given can be made to fill the machine's memory or never finish. So aless limits what one input can cost:

- `--max-size` refuses an input larger than its value before aless reads the input. The limit is 64 MB unless you set it. The refusal names the `--max-size` value that would let aless read the input.
- A depth limit refuses a document nested deeper than its format reads. The refusal carries the code `too_deep`.
- `--timeout` stops a parse that runs longer than its value. The limit is off unless you set it. The tab says how far the parse got.

You cannot switch the depth limit off, because without the limit some grammars would recurse until the stack ran out and the process crashed. Well before that point, the same grammars slow down with the square of the depth. So every parse stops at a fixed depth. Most formats stop sooner, at a depth of their own.

A C file has one more limit. Its syntax tree repeats subtrees under the members that name them, so a chain of member accesses can double the tree at each step. aless stops before it writes out a tree out of all proportion to the file. The formats reference gives each format's limit. For more information, see [Formats](/reference/formats.html#nesting-limits).

The time limit has no default, because a reasonable time for a parse depends on the machine. Set a limit where a file might take minutes. A grammar's compiler is the one reader aless cannot stop part way. So past the limit, aless stops waiting for the compile and reports a `timeout` instead.

None of the limits makes a hostile input free to parse. They limit what one input can cost. Raise `--max-size` for a large file you trust, and keep the limit for input you do not trust. The command-line reference states every limit exactly. For more information, see [Command line](/reference/command-line.html#large-inputs-and-limits).
