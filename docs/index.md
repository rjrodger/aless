---
title: a jless-style terminal viewer for every data format
description: aless shows JSON, YAML, TOML, CSV, XML, INI, Markdown and more as one tree in a jless-style terminal viewer, with tabs, a file explorer and a watch mode that keeps your place.
---
<section class="intro">
<img class="logo logo-light" src="/logo-light.svg" alt="aless" width="1490" height="430" loading="lazy"><img class="logo logo-dark" src="/logo-dark.svg" alt="aless" width="1490" height="430" loading="lazy">
<h1>Every format as one tree, in a viewer for the terminal</h1>
<p class="lede">aless reads JSON, YAML, TOML, CSV, XML, Markdown and many formats more into one kind of tree, and shows it in your terminal with the keys of jless. Several files open in tabs, and a directory opens as an explorer. When a file changes on disk, aless reads it again and keeps your place.</p>
<p class="actions"><a class="button primary" href="/tutorials/viewer.html">Take the tutorial</a> <a class="button" href="/how-to/install.html">Install aless</a></p>
<p class="meta">aless {{version}} · MIT licensed · <a href="https://github.com/rjrodger/aless">github.com/rjrodger/aless</a></p>
</section>

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

An OpenAPI document in YAML, opened with `aless --depth 2 bookshelf-openapi.yaml`, so that only its first two levels start open. The [tutorial](/tutorials/viewer.html) walks through it in ten minutes.

## Install

aless is one binary. Each release is built for Linux, macOS and Windows, on x86_64 and on aarch64, and [Install aless](/how-to/install.html) says how to check what you downloaded.

| Where | Command |
|---|---|
| Homebrew, on macOS or Linux | `brew install rjrodger/tap/aless` |
| A Rust toolchain | `cargo install --locked aless` |
| cargo binstall | `cargo binstall aless` |

The installer puts the newest release's binary in `~/.cargo/bin`. On Linux or macOS:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/rjrodger/aless/releases/latest/download/aless-installer.sh | sh
```

On Windows, in PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/rjrodger/aless/releases/latest/download/aless-installer.ps1 | iex"
```

## What it reads

Every format is read into the same tree, so the keys, the search, the paths you copy and the source positions work the same way on all of them. A format aless has no parser for can be read with a grammar of your own, written in ABNF.

<p class="chips"><span class="chips-label">Reads</span> <span class="chip">json</span> <span class="chip">jsonl</span> <span class="chip">jsonic</span> <span class="chip">jsonc</span> <span class="chip">json5</span> <span class="chip">yaml</span> <span class="chip">toml</span> <span class="chip">ini</span> <span class="chip">csv</span> <span class="chip">tsv</span> <span class="chip">xml</span> <span class="chip">zon</span> <span class="chip">markdown</span> <span class="chip">feed</span> <span class="chip">css</span> <span class="chip">proto</span> <span class="chip">pgn</span> <span class="chip">c</span> <span class="chip">abnf</span> <span class="chip">ebnf</span> <span class="chip">gbnf</span> <span class="chip">expr</span> <span class="chip">semver</span> <span class="chip">text</span> <a class="chip" href="/how-to/grammar.html">any ABNF grammar</a></p>

[Formats](/reference/formats.html) lists the extensions that imply each one, and what each becomes in the tree.

## The documentation

<div class="cards">
<a class="card" href="/tutorials/index.html">
<h3>Tutorial</h3>
<p>One lesson from the start: open a file, move through it, fold it, search it and copy from it, then change it on disk and watch aless keep your place.</p>
</a>
<a class="card" href="/how-to/index.html">
<h3>How-to guides</h3>
<p>One task each: search a document, follow a file as it changes, open several files in tabs, browse a directory, find why a file does not parse, read a format of your own.</p>
</a>
<a class="card" href="/reference/index.html">
<h3>Reference</h3>
<p>Every option, key, format and limit, and every release, written from the aless binary and its changelog.</p>
</a>
<a class="card" href="/explanation/index.html">
<h3>Explanation</h3>
<p>Why aless works as it does: one tree for every format, where each value came from, how a reload keeps your place, and what large inputs cost.</p>
</a>
</div>

## The project

<div class="cards">
<div class="card">
<h3>Who writes it</h3>
<p>aless is written by <a href="https://richardrodger.com">Richard Rodger</a>, as a <a href="https://tabnas.dev">tabnas</a> project. Every format is read through the tabnas parsers, a family of parsers in TypeScript, Go and Rust, so a new format is a grammar rather than a feature.</p>
</div>
<div class="card">
<h3>What it owes jless</h3>
<p>aless is inspired by <a href="https://jless.io">jless</a>, by Paul Julius Martinez, and its interface is jless's: the key map, the data and line modes, the folded previews, the search and the copy commands. aless is an independent implementation, its <a href="https://github.com/rjrodger/aless/blob/main/THIRD_PARTY_NOTICES.md">notices</a> credit jless as the design it follows, and <a href="/explanation/jless.html">aless and jless</a> says where the two differ.</p>
</div>
<div class="card">
<h3>Licence and source</h3>
<p>MIT. The source, the releases and the issues are on <a href="https://github.com/rjrodger/aless">GitHub</a>, the crate is on <a href="https://crates.io/crates/aless">crates.io</a>, and <a href="/reference/releases.html">Releases</a> lists what each release changed.</p>
</div>
</div>
