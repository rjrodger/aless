---
title: a jless-style terminal viewer for every data format
description: aless shows JSON, YAML, TOML, CSV, XML, INI, Markdown, and more as one tree in a jless-style terminal viewer, with tabs, a file explorer, and a watch mode that keeps your place.
---
<section class="intro">
<img class="logo logo-light" src="/logo-light.svg" alt="aless" width="1490" height="430" loading="lazy"><img class="logo logo-dark" src="/logo-dark.svg" alt="aless" width="1490" height="430" loading="lazy">
<h1>A terminal viewer for plain text data files</h1>
<p class="lede">View JSON (and variants), YAML, TOML, CSV, XML, Markdown, and
many more formats directly in your terminal. Review the tree
structure of the data format directly with your keyboard. If the file
changes, <code>aless</code> reloads and keeps your place. Great for reviewing
generated OpenAPI specs, XML documents, CSV files and other large
mucky things.</p>
<p>And a quick shout out to <a href="https://jless.io">jless</a>, which was the inspiration for this project.</p>
<p class="actions"><a class="button primary" href="/tutorials/viewer.html">Take the tutorial</a> <a class="button" href="/how-to/install.html">Install aless</a></p>
<p class="meta">aless {{version}} · MIT licensed · <a href="https://github.com/rjrodger/aless">github.com/rjrodger/aless</a></p>
</section>

This screen shows an OpenAPI document in YAML. The command is `aless --depth 2 bookshelf-openapi.yaml`, so only the first two levels start open:

<!-- screen: aless --depth 2 bookshelf-openapi.yaml
size: 80x18 -->
```screen
▼ {
    openapi: "3.0.0"
  ▽ info: {
      title: "Bookshelf demo API"
      version: "1.0.0"
      description: "A small API for keeping books on shelves, written for aless…
    ▷ license: (1) {name: "MIT"}
  ▽ servers: [
    ▷ (2) {url: "https://bookshelf.example/v1", …}
  ▽ tags: [
    ▷ (2) {name: "shelf", description: "Shelves, and the books on them"}
    ▷ (2) {name: "author", description: "The people who wrote the books"}
  ▽ paths: {
    ▷ "/api/shelf": (2) {get: {…}, post: {…}}
    ▷ "/api/shelf/{shelf_id}": (4) {parameters: […], get: {…}, put: {…}, …}
    ▷ "/api/shelf/{shelf_id}/book": (3) {parameters: […], get: {…}, …}
 bookshelf-openapi.yaml .                                 yaml · watching · 1:1
```

The tutorial, [Look around a file in the viewer](/tutorials/viewer.html), walks through that document. The tutorial takes about 10 minutes.

When a file changes, aless marks what changed and keeps the version it replaced. Here a save changed a year in [`books.json`](/examples/books.json) and removed a tag:

<!-- screen: aless books.json
edit: books.json: "year": 1999,\n    "tags": ["examples", "reference"] => "year": 2001,\n    "tags": ["examples"]
size: 80x13 -->
```screen
▼ [
└─▽ {
  │   title: "A Book of Examples"
  │   author: "A. N. Author"
  ├─~ year: 2001
  └─▽ tags: [ −1
        "examples"
── 2 changes in version 2 · [ older · d next change ────────────────────────────
  ~ [0].year     1999 → 2001
  − [0].tags[1]  "reference"
 14:00:05 ○──────────────────────────────────────────────────● 14:02:11  2 of 2
 books.json .                                             json · watching · 1:1
Reloaded books.json: 1 changed, 1 removed
```

A `~` marks a changed value, and `−1` counts a removed one. The timeline above the status bar has a dot for each version. For more information, see [Compare the versions of a file](/how-to/versions.html).

## Install

aless is one binary. Each release has builds for Linux, macOS, and Windows. Each platform gets an x86_64 build and an aarch64 build. With Homebrew or Cargo, one command installs aless:

| With | Command |
|---|---|
| Homebrew, on macOS or Linux | `brew install rjrodger/tap/aless` |
| A Rust toolchain | `cargo install --locked aless` |
| `cargo binstall` | `cargo binstall aless` |

An installer script puts the newest release's binary in `~/.cargo/bin`. On Linux or macOS, run:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/rjrodger/aless/releases/latest/download/aless-installer.sh | sh
```

On Windows, in PowerShell, run:

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/rjrodger/aless/releases/latest/download/aless-installer.ps1 | iex"
```

For more information about checking what you downloaded, see [Install aless](/how-to/install.html).

## Formats

aless reads every format into the same tree, so the keys, the search, the paths you copy, and the source positions work the same way on every format. aless can also read a format it has no built-in grammar for, with a custom grammar of your own. You write the custom grammar in ABNF.

<p class="chips"><span class="chips-label">Reads</span> <span class="chip">json</span> <span class="chip">jsonl</span> <span class="chip">jsonic</span> <span class="chip">jsonc</span> <span class="chip">json5</span> <span class="chip">yaml</span> <span class="chip">toml</span> <span class="chip">ini</span> <span class="chip">csv</span> <span class="chip">tsv</span> <span class="chip">xml</span> <span class="chip">zon</span> <span class="chip">markdown</span> <span class="chip">feed</span> <span class="chip">css</span> <span class="chip">proto</span> <span class="chip">pgn</span> <span class="chip">c</span> <span class="chip">abnf</span> <span class="chip">ebnf</span> <span class="chip">gbnf</span> <span class="chip">expr</span> <span class="chip">semver</span> <span class="chip">text</span> <a class="chip" href="/how-to/grammar.html">any ABNF grammar</a></p>

For more information about the extensions that imply each format, and about what each format becomes in the tree, see [Formats](/reference/formats.html).

## Documentation

<div class="cards">
<a class="card" href="/tutorials/">
<h3>Tutorial</h3>
<p>The tutorial is one lesson from the start: open a file, move through it, fold it, search it, and copy from it. Then change the file on disk, and watch aless keep your place.</p>
</a>
<a class="card" href="/how-to/">
<h3>How-to guides</h3>
<p>Each guide solves one task: search a document, follow a file as it changes, open several files in tabs, browse a directory, find out why a file does not parse, or read a format of your own.</p>
</a>
<a class="card" href="/reference/">
<h3>Reference</h3>
<p>The reference holds every option, key, format, and limit, and every release. It is written from the aless binary and its changelog.</p>
</a>
<a class="card" href="/explanation/">
<h3>Explanation</h3>
<p>The explanation pages say why aless works as it does: why every format becomes one tree, where each value came from, how a reload keeps your place, and what large inputs cost.</p>
</a>
</div>

## Project

<div class="cards">
<div class="card">
<h3>Who writes it</h3>
<p><a href="https://richardrodger.com">Richard Rodger</a> writes aless. aless is a <a href="https://tabnas.dev">tabnas</a> project. The tabnas parsers are a family of parsers in TypeScript, Go, and Rust. aless reads every format through the tabnas parsers, so a new format is a grammar rather than a feature.</p>
</div>
<div class="card">
<h3>What it owes jless</h3>
<p><a href="https://jless.io">jless</a> inspired aless. Paul Julius Martinez wrote jless. aless has the interface of jless: the key map, the <a href="/how-to/display.html#data-mode-and-line-mode">data mode and line mode</a>, the folded previews, the search, and the copy commands. aless is an independent implementation. Its <a href="https://github.com/rjrodger/aless/blob/main/THIRD_PARTY_NOTICES.md">notices</a> credit jless as the design it follows. For more information about where aless differs from jless, see <a href="/explanation/jless.html">aless and jless</a>.</p>
</div>
<div class="card">
<h3>Licence and source</h3>
<p>aless is MIT licensed. Its source, its releases, and its issues are on <a href="https://github.com/rjrodger/aless">GitHub</a>. The crate is on <a href="https://crates.io/crates/aless">crates.io</a>. For more information about what each release changed, see <a href="/reference/releases.html">Releases</a>.</p>
</div>
</div>
