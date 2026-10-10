# Changelog

Every notable change to aless, newest first. The format is
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions
follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

A release's notes on GitHub are its section here: the pull request that
bumps the version renames `Unreleased` to the version and its date
([RELEASING.md](RELEASING.md)).

## Unreleased

### Added

- Five formats more, read by the crates that carry their translation
  parts (admin ADR-27): CSS (`.css`, tabnas-css), Protocol Buffers'
  `.proto` files (`.proto`, tabnas-proto, read as their
  FileDescriptorProto in the JSON form protoc writes), PGN chess games
  (`.pgn`, tabnas-chess, named `pgn` as its manifest names it),
  arithmetic expressions (`-k expr`, tabnas-expr, each operation an array
  of its operator and its terms) and semantic versions (`-k semver`,
  tabnas-semver, read without the line break a file holding one ends
  with). A `.css`, `.proto` or `.pgn` file, read as plain text before,
  is read as its format.
- `--render` writes each of them, and Atom feeds through tabnas-feed
  0.6.16's render: `css`, `proto`, `pgn`, `expr`, `semver` and `feed`.
  An expression takes any document as it is and a feed any through its
  embedding. CSS, `.proto` and PGN write only the tree their own
  documents read as, and refuse any other document before reading it,
  as a `usage` error that names the route that can write one, a program
  that makes that tree (`--alchemy FILE --render proto`); a version takes
  only a tree that is a version, and refuses any other with
  `TARGET_VALUE_UNREPRESENTABLE` before writing anything.

### Changed

- `--render` given a format's extension rather than its name
  (`--render rss`, `--render yml`) names the format's render
  (`--render feed`), where it said the format had none.
- `--render` reads JSON Lines, CSV and TSV a record at a time under
  `--path` too, as it does at the root: the path's value is
  taken from the records as they pass, and the rest of the file is read
  the same way to its end, so `--max-size` does not apply below the root
  either. What is written is what reading the file whole writes; a record
  the grammar refuses after the value was written fails the run with
  `output: "partial"`, as a refusal at the root does.

### Fixed

- A JSON Lines record that repeats a member (`{"a":1,"a":2}`), which its
  stream holds twice, was refused with `DUPLICATE_MEMBER` at the root
  though nothing had been written. A file is now read again whole, within
  `--max-size`, and its value written, the last of the two as `--json`
  reads it, at the root and below it. Standard input, which cannot be read
  again, keeps the refusal, with `output: "none"`.

## [0.2.0] - 2026-10-10

### Changed

- `--render` composes every conversion through alchemy's `translate`
  (admin ADR-27), from the parts each format's crate declares: what a
  format cannot hold is written by the convention its manifest declares,
  never refused. A null in TOML is left out; NaN and the infinities are
  `null` in `--render json`, as in `--json`, and their names in CSV; a
  root TOML or INI cannot have is the one member of a table, and one JSON
  Lines, CSV or Markdown cannot have the one element of an array; a
  Markdown document with no table is the empty table; XML writes any
  document as the element tree its embedding declares.
- `--render csv` and every records format take rows of every kind: an
  array row's cells are its positions, a scalar row is one cell named
  `value`, and a row of another kind than the first has a cell where the
  first row's columns find one. A table of no columns is the empty
  document.
- The loss warning names every step that ran in `adapters`; `adapter`
  still names the one between a tree and a table. JSON declares its one
  loss, so `--render json` writes the warning too.

### Added

- `--key NAME`: the member a root is written under when the format's
  document must be a table (`items` by default).

## [0.1.1] - 2026-10-09

The first release with prebuilt binaries. 0.1.0 went to crates.io alone,
published by hand; 0.1.1 is the same program, released the whole way: the
archives for Linux, macOS and Windows with their checksums and attestations,
the installers, the Homebrew tap, and crates.io by trusted publishing. What
aless does is under 0.1.0, below, and at
[aless.tabnas.dev](https://aless.tabnas.dev).

## [0.1.0] - 2026-10-09

The first release.

### Added

- A [jless](https://jless.io)-style terminal viewer for JSON, JSON Lines,
  jsonic, JSONC, JSON5, YAML, TOML, INI, CSV, TSV, XML, ZON, Markdown and
  RSS/Atom feeds, with tabs, panes, a file explorer, regex search, and a
  watch mode that reloads a changed file and keeps your place.
- Custom grammars: any text format described in ABNF (`--grammar`,
  `--grammar-expr`).
- Without a screen, JSON for scripts and agents: `--json`, `--paths`,
  `--find`, `--where` and `--check`, each with source positions.
- `--help`, the whole reference an agent needs to drive aless: every
  option, what each output prints, every error kind with its fields, the
  exit statuses, paths, positions, formats and limits. `-h` is a summary.
- `--generate`: the man page, completions for bash, zsh, fish and
  PowerShell, and the Agent Skill, from the binary itself. Each shell's
  completions load from a file where the shell looks, or from its
  startup file.
- `--render` to CSV, JSON, or any format whose tabnas crate carries a
  render, and `--alchemy` programs over any input.
- Releases: binaries for Linux (x86_64 and aarch64, glibc and static
  musl), macOS (x86_64 and aarch64) and Windows (x86_64 and aarch64), shell
  and PowerShell installers, a Homebrew tap, `cargo install` and
  `cargo binstall`, with checksums, a CycloneDX SBOM and build-provenance
  attestations. Each archive carries the man page and the completions,
  and the Homebrew formula installs them where Homebrew's own formulas
  put theirs.
- A homepage, [aless.tabnas.dev](https://aless.tabnas.dev): tutorials,
  how-to guides, the reference and explanations, with the reference
  written from the binary and every example's output checked against it.

[0.1.1]: https://github.com/rjrodger/aless/releases/tag/v0.1.1
[0.1.0]: https://crates.io/crates/aless/0.1.0
