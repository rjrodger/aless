# Changelog

Every notable change to aless, newest first. The format is
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions
follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

A release's notes on GitHub are its section here: the pull request that
bumps the version renames `Unreleased` to the version and its date
([RELEASING.md](RELEASING.md)).

## [Unreleased]

### Changed

- `whole`, in `--render`'s loss note and in an error carrying `loss`, is
  given for nine more of the formats read whole before anything is
  written, where it was given for TOML and INI alone: feeds, `.proto`,
  PGN, expressions, versions, C and the three grammar notations now say
  why in their manifests (tabnas-feed 0.6.18, tabnas-proto 0.6.11,
  tabnas-chess 0.1.15, tabnas-expr 0.5.19, tabnas-semver 0.0.11, tabnas-c
  0.5.15, tabnas-abnf 0.4.24, tabnas-ebnf 0.1.19 and tabnas-gbnf 0.1.20).
  A C file, for one, is read whole because a conditional group is folded
  into one node only once the whole file is parsed.

## [0.4.0] - 2026-10-13

### Added

- C source (`.c`, `.h`, tabnas-c 0.5.14), read as its concrete syntax
  tree, preprocessor directives parsed into it, and written by
  `--render c` (admin ADR-27, phase 4's second batch). The tree is the
  one tabnas-c realizes from its arena on the parse's thread: each node
  its `kind`, `span`, `children` and `trivia`, each token its `tname`,
  `src` and `span`, and the members a node names its parts by (`operand`,
  `cond`), which repeat subtrees its `children` hold. Like CSS, `.proto`
  and PGN, `--render c` writes only that tree, and refuses another
  document before reading it with the `usage` error that names the
  program route. A C file's tokens are placed where its tree says, not
  by aligning values, and what the reader adds has no position. Its
  reader's limits are `too_deep` errors: tabnas-expr's 127 operations in
  an expression and 1,024 open rules, and tabnas-c's tree of 256 levels,
  with no position where the parse stopped between two tokens. A `.c` or
  `.h` file, read as plain text before, is read as C.
- A C tree is refused as `too_deep`, before it is written out, when it
  would hold more than 256 values a byte of the file written out (at
  least a million, and never more than 33,554,432, so that a long comment
  cannot raise the bound a short chain is held to), or more than 2,048
  bytes of names and strings a byte (at least 8 MB, and never more than
  256 MB, so that a long name in a subtree it repeats is held as well):
  its reader shares the subtrees it repeats, so a chain of subscripts or
  member accesses doubles the tree written out at each level, and 40
  member accesses in one expression would otherwise hold more values
  than a machine can.
- Grammars in ABNF (`.abnf`, tabnas-abnf 0.4.23), EBNF (`.ebnf`,
  tabnas-ebnf 0.1.18) and GBNF (`.gbnf`, tabnas-gbnf 0.1.19), each read
  as the grammar spec its compiler writes, `options` and `rule`s with
  the rules the compiler adds, and written by `--render abnf`, `ebnf` and
  `gbnf` (admin ADR-27, phase 4's second batch). The three write one
  schema, so a grammar in one notation is written in another; any other
  document is the `usage` error that names the program route, and a spec
  the target notation cannot say (an action, a terminal none of its
  forms matches, in EBNF a counted repetition nested past the 128 levels
  its own reader reads, so ABNF's `*65"a"` and longer) is
  `TARGET_VALUE_UNREPRESENTABLE` before anything is written. A grammar's
  compiler keeps no places, so no node of its spec has a position, and
  the compiler's own limits are `too_deep` errors: groups and options
  nested about 500 levels in ABNF, 130 in EBNF and 128 in GBNF, and
  elements nested 128 levels. The compile runs on the parse's thread, or
  within `--timeout` on a thread of its own, which aless stops waiting
  for when the time is up, and does not start once it is; while a
  compile it stopped waiting for runs on, no other starts, and a later
  grammar in the run is the same `timeout` at once. `aless --check` on a
  grammar file now says whether it compiles; such a file, read as plain
  text before, is read as its spec. `--grammar` reads its grammar as
  before.

### Changed

- tabnas-expr 0.5.18. `--render expr` refuses a tree its reader would
  read back as another, with `TARGET_VALUE_UNREPRESENTABLE` naming the
  object's path where it meets it: a list led by an object whose `src`
  the reader takes for the list's operator, where that is no default
  operator's text, as a C syntax tree's lists are. 0.5.17 wrote such a
  list, and it read back with its leading object reduced to that text.
- tabnas-abnf 0.4.23, from 0.4.20, the compiler behind `--grammar` too.
- tabnas-bnf 0.1.30, the compiler the three notations and `--grammar`
  share. A cycle of rules whose alternatives start with one another,
  which its left-recursion pass multiplied until memory ran out (twenty
  such rules, 0.7 KB), is refused before anything is built, past 512
  alternatives added to one rule, at once and in every route; so is a
  grammar the pass would grow past 131,072 elements, a chain of 5,000
  rules each led by the next among them. A grammar of 10,000 rules
  compiles in under two seconds, where it took ten. What the compiler
  writes and holds is bounded too (3,500,000 units), so a grammar whose
  spec would pass that is refused rather than built.
- tabnas-feed 0.6.17, a version move.
- tabnas-proto 0.6.10. A `.proto` file is read through the crate's own
  descriptor tree (`to_descriptor_value`), so a descriptor's members now
  come in the order the crate's other runtimes give them: the lists every
  descriptor has first, then what a statement places (the syntax, the
  package, a field's `proto3Optional`) in the order of the statements.
  The output can differ from 0.3.0's in member order, and the package,
  held after the lists, is past where positions are aligned, so it has
  none. A number the reader reads as NaN (a digit separator, `1_0`) stays
  NaN: `--json` writes it as null, as before, an entry gives `"NaN"`,
  and `--render yaml` writes `.nan`. The refusals are as they were.
  `--render proto` writes a backtick literal as itself, where 0.6.8
  wrote one holding a line break as an aggregate its own reader refused,
  and refuses a string no spelling of a `.proto` file holds with
  `TARGET_VALUE_UNREPRESENTABLE`, before writing anything.

## [0.3.0] - 2026-10-10

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
- `whole` in `--render`'s loss note and in an error carrying `loss`: why
  a TOML or INI source is read whole before anything is written, in its
  format's own sentence (alchemy 0.2.6, tabnas-toml 0.5.17, tabnas-ini 0.5.19).

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
- `--render` and `--alchemy` write an XML document's records as the parse
  proceeds, as they do the JSON family's (tabnas-transduce 0.2.7,
  tabnas-xml 0.7.17), where they waited for the parse to end: a failure
  part way leaves every record before it written, with `output: "partial"`.

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
