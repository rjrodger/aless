---
title: Read a format aless does not know
description: Describe a line-oriented text format in ABNF, and open its files in the viewer like any built-in format.
group: In the viewer
order: 8
---
A text format aless has no parser for can be read with a grammar you write in [ABNF](https://github.com/tabnas/abnf), the notation of RFC 5234. You name the grammar on the command line, and aless reads with it the files whose extension, or whole filename, is that name. This guide uses `/etc/hosts`; download the grammar and a sample hosts file:

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

The `; @array` and `; @object address names` comments say what each rule builds: the file is an array of entries, and an entry an object of its address and its names. Without them the value is the compiler's parse tree, a `{"rule", "src", "kids"}` node for each rule, which works but is harder to read. The [tabnas/abnf guide](https://github.com/tabnas/abnf/blob/main/ts/doc/guide.md) explains the annotations.

## Use it everywhere a format goes

Once named, the grammar is a format like any other. The viewer opens the real file with it, `aless --grammar hosts=hosts.abnf /etc/hosts`, and everything the viewer does works on the result: the folds, the search, the copy keys and the source view (`s`) work as on JSON, the status bar gives each node's line and column in the file, and a change to the file reloads the tab with your place kept ([Follow a file as it changes](/how-to/watch.html)).

The grammar file is a document of its own as well: `aless hosts.abnf` opens the spec it compiles to, the options and rules a parser is made from ([Choose the format aless reads](/how-to/formats.html)).

`NAME,NAME2=FILE` gives a grammar two names, `--grammar` repeats for several grammars, and `--grammar-expr NAME=ABNF` takes the grammar's text on the command line instead of from a file. Standard input has no name, so give it `-k NAME`: `crontab -l | aless --grammar crontab=crontab.abnf -k crontab`.

## Write your own

A grammar reads its file as plain text. `#` starts a comment to the end of a line, spaces and tabs between tokens are skipped, and a word is the token `TX`, so `::1` and `root:x:0:0` are one word each until the grammar names a literal that splits them (`":"`). `NR`, `ST` and `VL` bring numbers, quoted strings and `true`, `false` and `null` back when a format has them.

aless keeps a library of grammars, for `/etc/hosts`, crontabs, `/etc/passwd`, `/etc/group`, `/etc/fstab`, `/etc/resolv.conf` and `KEY=value` files, each with a sample. Its [README](https://github.com/rjrodger/aless/tree/main/tests/fixtures/grammars) is the guide to writing one: how a line-oriented grammar names its newlines, where the tokens fit, and what the compiler refuses.

A grammar that does not compile is refused before any input is read, with the compiler's message on one line of standard error and the status 2. An input it does not accept opens as a parse error like any other, with the grammar's name as its format. The [reference](/reference/command-line.html#custom-grammars) has the limits a grammar runs under.
