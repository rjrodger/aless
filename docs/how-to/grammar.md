---
title: Read a format aless does not know
description: Describe a line-oriented text format in ABNF, and open its files in the viewer as you would a built-in format.
group: Files and formats
order: 13
---
A text format aless has no parser for can be read with a grammar you write in [ABNF](https://github.com/tabnas/abnf), the notation of RFC 5234. You name the grammar on the command line, and aless reads with it every file whose extension, or whole name, is that name. This guide reads `/etc/hosts`. Download the grammar and a sample hosts file:

<!-- test: skip it downloads the files the site publishes, which the capture below reads from tests/fixtures/grammars/ -->
```sh
curl -LO https://aless.tabnas.dev/examples/hosts.abnf
curl -LO https://aless.tabnas.dev/examples/hosts
```

## Read a file with a grammar

The grammar describes a hosts file:

```console
$ cat hosts.abnf
; /etc/hosts: an address and the host names it answers to, one per line.
; Comments start with # and blank lines are skipped, in every grammar here.
hosts   = *( entry %x0A / %x0A ) [ entry ]   ; @array
entry   = address names                      ; @object address names
address = word
names   = 1*word                             ; @array
word    = ( TX )
```

`--grammar NAME=FILE` ties it to the name `hosts`, so the file named `hosts` is read with it:

```sh
aless --grammar hosts=hosts.abnf hosts
```

aless shows the file as an array of entries, each an object of its address and its names, and the status bar names the format after the grammar:

<!-- screen: aless --grammar hosts=hosts.abnf hosts
size: 80x20 -->
```screen
▼ [
  ▽ {
      address: "127.0.0.1"
    ▽ names: [
        "localhost"
  ▽ {
      address: "127.0.1.1"
    ▽ names: [
        "workstation.example.com"
        "workstation"
  ▽ {
      address: "10.0.0.5"
    ▽ names: [
        "fileserver"
        "files"
        "nas"
  ▽ {
      address: "192.168.1.20"
 hosts .                                                 hosts · watching · 4:1
```

The comments `; @array` and `; @object address names` say what each rule builds: the file is an array of entries, and an entry is an object of its address and its names. Without them the value is the compiler's parse tree, a `{"rule", "src", "kids"}` node for each rule, which works but reads less well. The [tabnas/abnf guide](https://github.com/tabnas/abnf/blob/main/ts/doc/guide.md) explains the annotations.

## Use it as any format

Once named, the grammar is a format like any other. `aless --grammar hosts=hosts.abnf /etc/hosts` opens the real file, and everything the viewer does works on it: folding, search, the copy keys and the [source view](/how-to/display.html#data-mode-and-line-mode). The status bar gives each node's line and column in the file, and a change to the file reloads the tab with your place kept ([Follow a file as it changes](/how-to/watch.html)).

`NAME,NAME2=FILE` gives a grammar two names, `--grammar` repeats for several grammars, and `--grammar-expr NAME=ABNF` takes the grammar's text on the command line instead of from a file. Standard input has no name, so give it `-k NAME`: `crontab -l | aless --grammar crontab=crontab-user.abnf -k crontab`.

## Write your own

A grammar reads its file as plain text. `#` starts a comment to the end of a line, spaces and tabs between tokens are skipped, and a word is the token `TX`, so `::1` and `root:x:0:0` are one word each until the grammar names a literal that splits them, such as `":"`. `NR`, `ST` and `VL` bring numbers, quoted strings, and `true`, `false` and `null` back, for a format that has them.

aless keeps a library of grammars, for `/etc/hosts`, `/etc/crontab` and a user's crontab, `/etc/passwd`, `/etc/group`, `/etc/fstab`, `/etc/resolv.conf` and `KEY=value` files, each with a sample. Its [README](https://github.com/rjrodger/aless/tree/main/tests/fixtures/grammars) is the guide to writing one: how a line-oriented grammar names its newlines, where the tokens fit, and what the compiler refuses.

A grammar that does not compile is refused before any input is read, with the compiler's message on one line of standard error, and the exit status 2. An input the grammar does not accept opens as a parse error like any other, with the grammar's name as its format ([Find out why a file does not parse](/how-to/errors.html)). A grammar's parse takes about 2 KB of memory and 60 µs a line, so `--max-size` and `--timeout` matter for it as for any format, and the [reference](/reference/command-line.html#custom-grammars) has the limits it runs under.
