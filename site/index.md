---
title: a terminal viewer and a format converter, for people, scripts and agents
description: aless shows JSON, YAML, TOML, CSV, XML, INI, Markdown and more as one tree in a jless-style terminal viewer, and without a terminal prints JSON, writes any of those formats as any other, and runs programs over them, for scripts and agents.
---
<section class="intro">
<img class="emblem" src="/mark.svg" width="96" height="96" alt="">
<span class="word">aless</span>
<h1>Every format as one tree: a viewer in a terminal, and a converter and query tool for scripts and agents</h1>
<p class="lede">aless reads JSON, YAML, TOML, CSV, XML, INI, Markdown and a dozen formats more into one kind of tree. In a terminal it is a viewer with jless's keys, with tabs and a watch mode that keeps your place. In a shell script, a pipe or an agent's tool call it prints JSON, writes any of those formats as any other, and runs programs over them.</p>
<p class="meta">aless {{version}} · MIT licensed · <a href="https://github.com/rjrodger/aless">github.com/rjrodger/aless</a></p>
</section>

<div class="faces">
<div class="face">
<p class="kind">In a terminal</p>
<h2>A viewer for files of any of those formats</h2>
<p>Open a file and it is a tree. Fold it, move through it with the keys of jless, search it with a regular expression, and copy a value or its path. Several files open in tabs, a directory opens as an explorer, and a conversion can sit in a pane beside the document. aless watches every file it shows: when one changes on disk it reloads it and keeps your place.</p>

```screen
▽ {
    openapi: "3.0.0"
  ▷ info: (4) {title: "Bookshelf demo API", …}
  ▷ servers: (1) [{…}]
  ▷ tags: (2) [{…}, {…}]
  ▽ paths: {
    ▶ "/api/shelf": (2) {get: {…}, post: {…}}
    ▷ "/api/shelf/{shelf_id}": (4) {parameters: […], …}
    ▷ "/api/shelf/{shelf_id}/book": (3) {parameters: […], …}
    ▷ "/api/shelf/{shelf_id}/book/{book_id}": (3) {…}
    ▷ "/api/author": (1) {get: {…}}
…f-openapi.yaml .paths["/api/shelf"] yaml · watching · 19:3
```

<p class="more"><a href="/tutorials/viewer.html">Look around a file in the viewer</a>, the first tutorial.</p>
</div>
<div class="face">
<p class="kind">In a script, a pipe or an agent's tool call</p>
<h2>A query tool and a converter for scripts and agents</h2>
<p>Without a terminal, aless draws nothing and never waits for a key. It prints an outline of a file, the value at a path, the path at a line and column, the nodes a search finds, or a report on whether files parse, as JSON on standard output. It writes any format it reads as any format it can write, a record at a time where the format's reader allows, and standard output is then that format's text. It runs programs that choose and reshape values on the way through. A query answers with one JSON value and a conversion with one document, every error is one JSON object on standard error, and the exit status says which.</p>

```console
$ aless --json --path '.info.title' bookshelf-openapi.yaml
"Bookshelf demo API"
$ aless --render yaml --path '.[0]' books.json 2>/dev/null
"title": "A Book of Examples"
"author": "A. N. Author"
"year": 1999
"tags":
  - "examples"
  - "reference"
```

<p class="more"><a href="/tutorials/scripting.html">Ask a file questions from a script</a>, the second tutorial.</p>
</div>
</div>

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

## What it reads and writes

Every format is read into the same tree, so the viewer, the paths, the search and the outputs are the same for all of them. A format aless has no parser for can be read with a grammar of your own, written in ABNF ([Read a format aless does not know](/how-to/grammar.html)).

<p class="chips"><span class="chips-label">Reads</span> <span class="chip">json</span> <span class="chip">jsonl</span> <span class="chip">jsonic</span> <span class="chip">jsonc</span> <span class="chip">json5</span> <span class="chip">yaml</span> <span class="chip">toml</span> <span class="chip">ini</span> <span class="chip">csv</span> <span class="chip">tsv</span> <span class="chip">xml</span> <span class="chip">zon</span> <span class="chip">markdown</span> <span class="chip">rss</span> <span class="chip">atom</span> <span class="chip">css</span> <span class="chip">proto</span> <span class="chip">pgn</span> <span class="chip">expr</span> <span class="chip">semver</span> <span class="chip">text</span> <a class="chip" href="/how-to/grammar.html">any ABNF grammar</a></p>
<p class="chips"><span class="chips-label">Writes</span> <span class="chip">json</span> <span class="chip">jsonl</span> <span class="chip">jsonic</span> <span class="chip">jsonc</span> <span class="chip">json5</span> <span class="chip">yaml</span> <span class="chip">toml</span> <span class="chip">ini</span> <span class="chip">csv</span> <span class="chip">xml</span> <span class="chip">zon</span> <span class="chip">markdown</span> <span class="chip">feed</span> <span class="chip">expr</span> <a class="chip" href="/how-to/convert.html#formats-that-hold-only-their-own-kind">css, proto, pgn and semver from their own kind</a></p>

[Convert a file to another format](/how-to/convert.html) shows a conversion and what each format declares it cannot keep.

## The documentation

<div class="cards">
<div class="card">
<h3><a href="/tutorials/index.html">Tutorials</a></h3>
<p>Two lessons from the start, one for each face: <a href="/tutorials/viewer.html">the viewer</a>, and <a href="/tutorials/scripting.html">the command line</a> a script or an agent uses.</p>
</div>
<div class="card">
<h3><a href="/how-to/index.html">How-to guides</a></h3>
<p>One task each, in the viewer and from the command line: <a href="/how-to/search.html">search a document</a>, <a href="/how-to/watch.html">follow a file as it changes</a>, <a href="/how-to/extract.html">get a value out in a script</a>, <a href="/how-to/convert.html">convert a file</a>, <a href="/how-to/check.html">check that files parse</a>, <a href="/how-to/agents.html">give an agent aless</a>.</p>
</div>
<div class="card">
<h3><a href="/reference/index.html">Reference</a></h3>
<p>Every <a href="/reference/command-line.html">option, output, error and exit status</a>, every <a href="/reference/keys.html">key</a>, and every <a href="/reference/releases.html">release</a>, written from the aless binary and its changelog.</p>
</div>
<div class="card">
<h3><a href="/explanation/index.html">Explanation</a></h3>
<p>Why aless works as it does: <a href="/explanation/one-tree.html">one tree for every format</a>, <a href="/explanation/command-line.html">why the command line answers in JSON</a>, <a href="/explanation/positions.html">where values came from</a>, and what <a href="/explanation/large-inputs.html">large inputs</a> cost.</p>
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
