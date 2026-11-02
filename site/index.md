---
title: a jless-style terminal viewer for every data format
description: aless shows JSON, YAML, TOML, CSV, XML, INI, Markdown and more as one tree in a jless-style terminal viewer, with tabs, a file explorer and a watch mode that keeps your place.
---
<section class="intro">
<img class="logo logo-light" src="/logo.svg" alt="aless" width="1490" height="430"><img class="logo logo-dark" src="/logo-dark.svg" alt="aless" width="1490" height="430">
<h1>Every format as one tree, in a viewer for the terminal</h1>
<p class="lede">aless reads JSON, YAML, TOML, CSV, XML, INI, Markdown and a dozen formats more into one kind of tree, and shows it in a terminal with the keys of jless. Several files open in tabs, a directory opens as an explorer, and aless watches every file it shows: when one changes on disk it reloads it and keeps your place.</p>
<p class="meta">aless {{version}} · MIT licensed · <a href="https://github.com/rjrodger/aless">github.com/rjrodger/aless</a></p>
</section>

## A viewer for files of any of those formats

Open a file and it is a tree. Fold it, move through it with the keys of jless, search it with a regular expression, and copy a value or its path. Here an OpenAPI document in YAML, opened with its containers folded two levels down:

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

aless is a viewer and nothing else. It draws on a terminal and reads keys from it, and without a terminal it refuses to start. To read one of these formats in a script, or write a document as another format, there is [dmog](https://github.com/tabnas/dmog), which translates any format aless reads into any other.

[Look around a file in the viewer](/tutorials/viewer.html), the tutorial, takes ten minutes.

## Install

aless is one binary with nothing beside it. Each release is built for Linux, macOS and Windows, on x86_64 and on aarch64, and [Install aless](/how-to/install.html) says how to check what you downloaded.

| Where | Command |
|---|---|
| Homebrew, on macOS or Linux | `brew install rjrodger/tap/aless` |
| A Rust toolchain | `cargo install --locked aless` |
| cargo binstall | `cargo binstall aless` |

The installers put the newest release's binary in `~/.cargo/bin`. On Linux or macOS:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/rjrodger/aless/releases/latest/download/aless-installer.sh | sh
```

On Windows, in PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/rjrodger/aless/releases/latest/download/aless-installer.ps1 | iex"
```

## What it reads

Every format is read into the same tree, so the viewer, the paths you copy, the search and the source positions are the same for all of them. A format aless has no parser for can be read with a grammar of your own, written in ABNF ([Read a format aless does not know](/how-to/grammar.html)).

<p class="chips"><span class="chips-label">Reads</span> <span class="chip">json</span> <span class="chip">jsonl</span> <span class="chip">jsonic</span> <span class="chip">jsonc</span> <span class="chip">json5</span> <span class="chip">yaml</span> <span class="chip">toml</span> <span class="chip">ini</span> <span class="chip">csv</span> <span class="chip">tsv</span> <span class="chip">xml</span> <span class="chip">zon</span> <span class="chip">markdown</span> <span class="chip">rss</span> <span class="chip">atom</span> <span class="chip">css</span> <span class="chip">proto</span> <span class="chip">pgn</span> <span class="chip">c</span> <span class="chip">abnf</span> <span class="chip">ebnf</span> <span class="chip">gbnf</span> <span class="chip">expr</span> <span class="chip">semver</span> <span class="chip">text</span> <a class="chip" href="/how-to/grammar.html">any ABNF grammar</a></p>
## The documentation

<div class="cards">
<div class="card">
<h3><a href="/tutorials/index.html">Tutorial</a></h3>
<p>One lesson from the start: <a href="/tutorials/viewer.html">look around a file in the viewer</a>, move through it, fold it, search it and copy from it, then change it on disk and watch aless keep your place.</p>
</div>
<div class="card">
<h3><a href="/how-to/index.html">How-to guides</a></h3>
<p>One task each: <a href="/how-to/search.html">search a document</a>, <a href="/how-to/watch.html">follow a file as it changes</a>, <a href="/how-to/tabs.html">open several files in tabs</a>, <a href="/how-to/explore.html">browse a directory</a>, <a href="/how-to/grammar.html">read a format aless does not know</a>.</p>
</div>
<div class="card">
<h3><a href="/reference/index.html">Reference</a></h3>
<p>Every <a href="/reference/command-line.html">option, format, limit and exit status</a>, every <a href="/reference/keys.html">key</a>, and every <a href="/reference/releases.html">release</a>, written from the aless binary and its changelog.</p>
</div>
<div class="card">
<h3><a href="/explanation/index.html">Explanation</a></h3>
<p>Why aless works as it does: <a href="/explanation/one-tree.html">one tree for every format</a>, <a href="/explanation/positions.html">where values came from</a>, <a href="/explanation/reloading.html">how a reload keeps your place</a>, and what <a href="/explanation/large-inputs.html">large inputs</a> cost.</p>
</div>
</div>

## The project

<div class="cards">
<div class="card">
<h3>Who writes it</h3>
<p>aless is written by <a href="https://richardrodger.com">Richard Rodger</a>, as a <a href="https://tabnas.dev">tabnas</a> project. Every format is read through the tabnas parsers, a family of parsers in TypeScript, Go and Rust, so a new format is a grammar rather than a feature.</p>
</div>
<div class="card">
<h3>What it owes jless</h3>
<p>The viewer's interface is <a href="https://jless.io">jless</a>'s, by Paul Julius Martinez: the key map, the data and line modes, the collapsed previews, the search and the copy commands. aless is an independent implementation, its <a href="https://github.com/rjrodger/aless/blob/main/THIRD_PARTY_NOTICES.md">notices</a> credit jless as the design it follows, and <a href="/explanation/jless.html">aless and jless</a> says where the two differ.</p>
</div>
<div class="card">
<h3>Licence and source</h3>
<p>MIT. The source, the releases and the issues are on <a href="https://github.com/rjrodger/aless">GitHub</a>, the crate is on <a href="https://crates.io/crates/aless">crates.io</a>, and <a href="/reference/releases.html">Releases</a> lists what each release changed.</p>
</div>
</div>
