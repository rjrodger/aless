---
title: Read a format aless does not know
description: Describe a line-oriented text format in ABNF, name the grammar on the command line, and open its files in the viewer as you open the files of a built-in format.
group: Files and formats
order: 14
---
When you give aless a grammar in ABNF, aless reads a text format it has no built-in grammar for. ABNF is the notation of RFC 5234. For more information, see [tabnas/abnf](https://github.com/tabnas/abnf) on GitHub. When you name the grammar on the command line, aless reads with the grammar every file whose extension, or whole name, is that name. This guide reads `/etc/hosts`. Download the grammar and a sample hosts file:

<!-- test: skip a download of the two files the site publishes, which the capture on this page reads from tests/fixtures/grammars/ instead -->
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

`--grammar NAME=FILE` names the grammar in FILE as NAME. Here FILE is `hosts.abnf` and NAME is `hosts`, so aless reads the file named `hosts` with that grammar:

```sh
aless --grammar hosts=hosts.abnf hosts
```

aless shows the file as an array of entries. The status bar names the format after the grammar:

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

The comments `; @array` and `; @object address names` say what each rule builds, so the file is an array of entries and each entry is an object of its address and its names. Without the comments, the value is the compiler's parse tree: a `{"rule", "src", "kids"}` node for each rule. The tree works, but it reads less well. For more information about annotations such as `; @array`, see the [tabnas/abnf guide](https://github.com/tabnas/abnf/blob/main/ts/doc/guide.md) on GitHub.

## Use the grammar as any other format

Once named, the grammar is a format like any other. The command `aless --grammar hosts=hosts.abnf /etc/hosts` opens the real file. Everything the viewer does works on a file the grammar reads: folding, search, the copy commands, and the [source view](/how-to/display.html#data-mode-and-line-mode). The status bar gives each node's line and column in the file. A change to the file reloads the tab without losing your place. For more information, see [Follow a file as it changes](/how-to/watch.html).

`NAME,NAME2=FILE` gives a grammar two names. To name several grammars, repeat `--grammar`. `--grammar-expr NAME=ABNF` takes the grammar from the command line instead of from a file. In that option, ABNF is the grammar's text itself. Standard input has no name, so give the grammar's name with `-k NAME`, as in `crontab -l | aless --grammar crontab=crontab-user.abnf -k crontab`.

## Write a grammar of your own

A grammar reads its input as plain text. In the input, `#` starts a comment that runs to the end of the line. A grammar skips spaces and tab characters between tokens. For example, the rule `entry = address names` in `hosts.abnf` has no token for the space between an address and its names. A word is the token `TX`, so `::1` and `root:x:0:0` are one word each. A word stays whole until the grammar names a literal that splits it, such as `":"`. Three more tokens are for a format that has numbers, quoted strings, or the values `true`, `false`, and `null`. `NR` brings back numbers, `ST` brings back quoted strings, and `VL` brings back `true`, `false`, and `null`.

aless keeps a library of grammars, for `/etc/hosts`, `/etc/crontab`, a user's crontab, `/etc/passwd`, `/etc/group`, `/etc/fstab`, `/etc/resolv.conf`, and `KEY=value` files. Each grammar comes with a sample. The [library's README](https://github.com/rjrodger/aless/tree/main/tests/fixtures/grammars) on GitHub is the guide to writing a grammar. It says how a line-oriented grammar names its newlines, where the tokens fit, and what the compiler refuses.

Before reading any input, aless refuses a grammar that doesn't compile. aless prints the compiler's message on one line of standard error and exits with the status 2. An input the grammar doesn't accept opens as a parse error like any other. The tab's format is the grammar's name, as it is for an input the grammar accepts. For more information, see [Find out why a file does not parse](/how-to/errors.html).

A grammar's parse takes about 2 KB of memory and 60 µs a line, so `--max-size` and `--timeout` matter for a grammar as for any format. The command-line reference gives the limits a grammar runs under. For more information, see [Command line](/reference/command-line.html#custom-grammars).
