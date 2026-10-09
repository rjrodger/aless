---
title: a terminal viewer for JSON, YAML, TOML, CSV, XML and more
description: aless shows JSON, YAML, TOML, CSV, XML, INI, Markdown and more as one tree in a jless-style terminal viewer, and prints JSON for scripts and agents.
---
<div class="hero">
<h1>aless</h1>
<p class="tagline">{{tagline}}.</p>
</div>

```screen
▽ {
    openapi: "3.0.0"
  ▷ info: (4) {title: "Bookshelf demo API", version: "1.0.0", …}
  ▷ servers: (1) [{…}]
  ▷ tags: (2) [{…}, {…}]
  ▽ paths: {
    ▶ "/api/shelf": (2) {get: {…}, post: {…}}
    ▷ "/api/shelf/{shelf_id}": (4) {parameters: […], get: {…}, put: {…}, …}
    ▷ "/api/shelf/{shelf_id}/book": (3) {parameters: […], get: {…}, …}
    ▷ "/api/shelf/{shelf_id}/book/{book_id}": (3) {parameters: […], get: {…}, …}
    ▷ "/api/author": (1) {get: {…}}
 bookshelf-openapi.yaml .paths["/api/shelf"]             yaml · watching · 19:3
```

aless opens a file in a terminal as a tree you can fold, move through with the keys of [jless](https://jless.io), search, and copy paths and values from. It reads JSON, JSON5, JSONC, JSON Lines, jsonic, YAML, TOML, INI, CSV, TSV, XML, RSS and Atom, ZON, Markdown and plain text into the same tree, and any other format an ABNF grammar describes. It watches the files it shows, and when one changes on disk it reloads it and keeps your place.

Without a terminal, aless prints JSON instead of drawing a screen: an outline of a file, the value at a path, the path at a line and column, the nodes a search finds, a report on whether files parse, or the whole document in another format. That is the interface for shell scripts, CI jobs and AI agents, and it never waits for a key.

```console
$ aless --json --path '.info.title' bookshelf-openapi.yaml
"Bookshelf demo API"
```

## Install

With Homebrew, on macOS or Linux, or with a Rust toolchain:

```sh
brew install rjrodger/tap/aless
cargo install --locked aless
```

[Install aless](/how-to/install.html) has the installers for Linux, macOS and Windows, the prebuilt archives, and how to check what you downloaded.

## The documentation

<div class="quadrants">
<div>

### [Tutorials](/tutorials/index.html)

Two lessons from the start: [the viewer](/tutorials/viewer.html), and [the command line](/tutorials/scripting.html) a script or an agent uses.

</div>
<div>

### [How-to guides](/how-to/index.html)

Recipes for a task you already have: [extract a value](/how-to/extract.html), [convert a file](/how-to/convert.html), [check files in CI](/how-to/check.html), [read a format of your own](/how-to/grammar.html).

</div>
<div>

### [Reference](/reference/index.html)

Every [option and output](/reference/command-line.html), and every [key](/reference/keys.html), written from the aless binary itself.

</div>
<div>

### [Explanation](/explanation/index.html)

Why aless works as it does: [one tree for every format](/explanation/one-tree.html), [where values came from](/explanation/positions.html), and what [large inputs](/explanation/large-inputs.html) cost.

</div>
</div>

## About

aless is a young project with one maintainer, and 0.1.0 was its first release. Its interface follows jless, by Paul Julius Martinez, and its [notices](https://github.com/rjrodger/aless/blob/main/THIRD_PARTY_NOTICES.md) credit it; [aless and jless](/explanation/jless.html) says where the two differ. Every format is read through the parsers of [tabnas](https://tabnas.dev), a family of parsers in TypeScript, Go and Rust. aless is MIT licensed, and its source, releases and issues are on [GitHub](https://github.com/rjrodger/aless).
