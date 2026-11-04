---
title: Find out why a file does not parse
description: Read the report aless shows for a file that does not parse, see it full size, find the line in the source, and get the document back.
group: Files and formats
order: 12
---
A file that does not parse still opens in a tab, which shows the parser's report in place of the tree. The example opens [`broken.json`](/examples/broken.json), whose array ends in a comma too many:

```sh
aless broken.json
```

<!-- screen: aless broken.json
size: 80x16 -->
```screen
[tabnas/unexpected]: unexpected character(s): ]
  --> broken.json:3:36
  1 | {
  2 |   "title": "A Book of Examples",
  3 |   "tags": ["examples", "reference",]
                                         ^ unexpected character(s): ]
  4 | }
  5 |

  The character(s) ] do not match any rule alternative active at
  this position.

  --internal: tag=-; rule=val~o; token=#CS; plugins=--

 broken.json .               !3:36: unexpected character(s): ]  json · watching
broken.json: 3:36: unexpected character(s): ]
```

## Read the report

The first line names the error: `unexpected` is its code, `tabnas` says the parser raised it, and the message says what the parser met. The `-->` line gives the file, the line and the column. The excerpt shows the lines around that place, with a caret under the character where the parse stopped, and the paragraph after it is the grammar's hint. The `--internal` line is the parser's own state, for a report to the grammar's authors.

The status bar carries the short form: a `!`, the line and column, and the message.

## See it full size

`!`, or `:error`, shows the whole report over the screen. `j` and `k` scroll it, `h` and `l` pan a line wider than the screen, and any other key goes back. `s` shows the source text, with the line the parse stopped at marked:

<!-- screen: aless broken.json
keys: s
size: 80x8 -->
```screen
1  {
2    "title": "A Book of Examples",
3!   "tags": ["examples", "reference",]
4  }


 broken.json  (source)       !3:36: unexpected character(s): ]  json · watching
```

The report is drawn safely, whatever the file holds. A control character in the message or in the file's name, such as the newline an unclosed string runs into, is shown escaped as `\n`. In the quoted lines it is shown as a picture one column wide, such as `␛`, so that the caret still lines up. A quoted line longer than 160 characters, as in a minified file, is cut to a window around the error.

## Get the document back

Fix the file and save it: aless is watching it, and reads it again. When the file opened as the wrong format, `:format FORMAT` reads the tab again as the right one ([Choose the format aless reads](/how-to/formats.html)).

A file that parsed once and breaks while you edit it keeps its last good document on screen, with the report docked beneath it, until the next save that parses ([Follow a file as it changes](/how-to/watch.html)).

A file nested too deeply, too large or too slow to parse opens the same way, with the code `too_deep`, a size, or a `timeout`. [Formats](/reference/formats.html#nesting-limits) gives each format's nesting limit, and [Large inputs and limits](/explanation/large-inputs.html) the others.
