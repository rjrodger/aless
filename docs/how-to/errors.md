---
title: Find out why a file does not parse
description: Read the report aless shows for a file that does not parse, see it full size, find the line in the source view, and get the document back.
group: Files and formats
order: 13
---
A file that doesn't parse still opens in a tab. In place of the tree, the tab shows the parser's report: where the parse stopped and what the parser met there. The example on this page is [`broken.json`](/examples/broken.json). Its array ends in a comma too many. Open the file:

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

The first line names the error. `unexpected` is its code, `tabnas` says that the parser raised it, and the message says what the parser met.

The `-->` line gives the file, the line, and the column. The excerpt shows the lines around that place. A caret under the line marks the character where the parse stopped. Here that character is the `]` that closes the array, straight after the comma too many.

The paragraph after the excerpt is the grammar's hint. The `--internal` line is the parser's own state. Include it in a report to the grammar's authors.

The status bar carries the report's short form: a `!`, the line and column, and the message.

## See the report full size

To see the whole report over the screen, press `!` or type `:error`. To scroll the report, press `j` and `k`. To pan a line wider than the screen, press `h` and `l`. Any other key goes back.

The [source view](/how-to/display.html#data-mode-and-line-mode) marks the line where the parse stopped. To see the source view, press `s`:

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

The `!` after the line number is the mark.

aless draws the report safely, whatever the file holds. In the message and in the file's name, aless escapes a control character, so the newline that an unclosed string runs into appears as `\n`. In the quoted lines, aless draws a control character as a picture one column wide, such as `␛`, so that the caret still lines up. Where a quoted line is longer than 160 characters, as in a minified file, aless cuts the line to a window around the error.

## Get the document back

Fix the file and save it. aless is watching the file, so it reads the file again when you save. To read the tab again as the right format when the file opened as the wrong one, type `:format FORMAT`. FORMAT is the name of a format, such as `yaml`. For more information, see [Choose the format aless reads](/how-to/formats.html).

When a file parsed once and then breaks while you edit it, aless keeps the file's last good document on screen until the next save that parses. Until then, aless docks the report beneath the document. For more information, see [Follow a file as it changes](/how-to/watch.html).

A file nested too deeply, too large, or too slow to parse opens the same way. The report then carries the code `too_deep`, a size, or a `timeout`. For more information about each format's nesting limit, see [Formats](/reference/formats.html#nesting-limits). For more information about the size and time limits, see [Large inputs and limits](/explanation/large-inputs.html).
