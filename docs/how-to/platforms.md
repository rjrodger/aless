---
title: Run aless on Linux, macOS and Windows
description: What the viewer needs from a terminal on each platform, what differs on Windows, and what to do over SSH, in a container or inside tmux.
group: Setting up
order: 3
---
aless runs on Linux, macOS and Windows, and its tests run on all three. It needs a terminal that understands VT escape sequences, and a font that has its fold markers. Without a terminal it refuses to start.

## The fold markers

The viewer draws a container's state with `▼ ▽ ▶ ▷`, and a font without those glyphs shows boxes in their place. `--ascii` draws `v` and `>` instead:

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

`:set ascii` does the same inside aless, and `:set noascii` goes back.

## Windows

Use Windows Terminal, or another console that understands VT sequences. `C-z`, which suspends aless, is Unix only. The installer puts `aless` in `~\.cargo\bin`, and PowerShell's completions load from your profile ([Set up completions and the man page](/how-to/completions.html)).

## Over SSH, in a container and in tmux

The copy commands use the system clipboard where there is one. Over SSH or in a container, where there is none, aless sends the text through the terminal with the OSC 52 escape sequence, and the terminal on your own machine puts it on your clipboard. Inside tmux, that works when tmux's `set-clipboard` option lets the sequence through. [Copy a value or its path](/how-to/copy.html) lists the terminals that take it, and the `p` keys that print the text on screen instead.

## Colour and the mouse

`--no-color`, or `NO_COLOR` set in the environment, draws without colour. The viewer takes the mouse's wheel and clicks; `--no-mouse` leaves the mouse to the terminal, for its own selection and scrolling. A terminal that cannot draw the viewer at all, `TERM=dumb`, is refused as a pipe on standard output is: aless says so on standard error and exits with the status 2.

## Watching files

aless watches a file through the platform's own file watcher: inotify on Linux, FSEvents on macOS and ReadDirectoryChangesW on Windows. It watches the file's directory, so a save that renames a new file over the old one is seen too. While the viewer is idle, aless also checks each watched file's size and modification time about every half second, which catches what a watcher misses, on a network share among other places.

A directory's modification time moves only as often as its file system's clock ticks, 16 ms or so on NTFS and up to two seconds on FAT. So for two seconds after the [file explorer](/how-to/explore.html) reads a directory with a new modification time, it reads that directory again on every check.

The [reference](/reference/command-line.html#environment) lists the environment variables aless reads, and [Install aless](/how-to/install.html) the builds for each platform.
