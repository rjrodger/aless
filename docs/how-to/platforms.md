---
title: Run aless on Linux, macOS, and Windows
description: Learn what the viewer needs from a terminal on each platform, what differs on Windows, and what to do over SSH, in a container, or inside tmux.
group: Setting up
order: 3
---
aless runs on Linux, macOS, and Windows. Its tests run on all three. aless needs a terminal that understands VT escape sequences, and a font that has the fold markers. Without a terminal it refuses to start.

## Fold markers in ASCII

The viewer draws a container's state with the fold markers `▼ ▽ ▶ ▷`. A font without those glyphs shows boxes in their place. `--ascii` draws `v` and `>` instead:

```sh
aless --ascii books.json
```

<!-- screen: aless --ascii books.json
keys: j h
size: 80x8 -->
```screen
v [
  > (4) {title: "A Book of Examples", author: "A. N. Author", year: 1999, …}
  v {
      title: "Shelves and How to Fill Them"
      author: "B. Bookworm"
      year: 2012
 books.json [0]                                           json · watching · 2:3
```

In the capture, `v` marks the open containers and `>` the folded book. Inside aless, `:set ascii` does what `--ascii` does. `:set noascii` goes back to `▼ ▽ ▶ ▷`.

## Windows

On Windows, use Windows Terminal, or another console that understands VT escape sequences, because aless needs them on every platform. `Ctrl+Z` doesn't suspend aless on Windows. The key suspends aless only on Unix. The installer puts `aless` in `~\.cargo\bin`. The completions for PowerShell load from your profile. For more information, see [Set up completions and the man page](/how-to/completions.html).

## Clipboard over SSH, in a container, and in tmux

The copy commands use the system clipboard where there is one. Over SSH or in a container there is none, so aless sends the text through the terminal instead. aless wraps the text in the OSC 52 escape sequence. The terminal on your own machine then puts the text on your clipboard. Inside tmux, the sequence reaches that terminal when the tmux option `set-clipboard` lets it through. A print command starts with `p`. It shows the text on screen instead of copying it. For more information about the terminals that take the sequence, and about the print commands, see [Copy a value or its path](/how-to/copy.html).

## Colour and the mouse

`--no-color`, or `NO_COLOR` set in the environment, draws without colour.

The viewer takes the mouse's wheel and clicks. `--no-mouse` leaves the mouse to the terminal, for the terminal's own selection and scrolling.

## Refusal to start

aless refuses to start on a terminal that can't draw the viewer at all, or with a pipe on standard output. `TERM=dumb` names such a terminal. In both cases aless says why on standard error and exits with the status 2.

`--print-json FILE` is the one exception: it prints the tree of FILE as JSON without the viewer, so it needs no terminal. For more information, see [Save a document as JSON](/how-to/json.html#print-the-json-from-the-shell).

## File watching

aless watches a file through the platform's own file watcher. That file watcher is inotify on Linux, FSEvents on macOS, and ReadDirectoryChangesW on Windows. aless watches the file's directory, so it sees a save that renames a new file over the old one. While the viewer is idle, aless also checks each watched file's size and modification time about every half second. That check catches what a watcher misses, for example on a network share.

A directory's modification time moves only as often as its file system's clock ticks. The tick is about 16 ms on NTFS and up to 2 seconds on FAT. So for 2 seconds after the [explorer](/how-to/explore.html) reads a directory with a new modification time, the explorer reads that directory again on every check.

For more information about the environment variables aless reads, see [Command line](/reference/command-line.html#environment). For more information about the builds for each platform, see [Install aless](/how-to/install.html).
