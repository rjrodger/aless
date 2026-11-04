# Architecture Decision Record

The register of the fundamental decisions for aless: the choices the
rest of the repository is built on, which a contributor, person or
agent, does not reverse without a new entry here.

An entry belongs here when reversing it would change what aless is,
rather than how one part of it works. Ordinary design choices live in
the code and in [`docs/`](docs/). Each entry states the decision, the
context that forced it, the consequences accepted in exchange, and how
it is enforced. Entries are append-only and numbered in order. A
decision that no longer holds keeps its heading, and its status says
what superseded it, so the citations of it still resolve.

| ADR | Decision | Status |
|-----|----------|--------|
| [ADR-001](#adr-001-aless-is-a-terminal-viewer) | aless is a terminal viewer | Accepted |
| [ADR-002](#adr-002-every-format-is-read-through-a-tabnas-grammar) | Every format is read through a tabnas grammar | Accepted |
| [ADR-003](#adr-003-the-interface-is-jlesss-unless-the-readme-says-otherwise) | The interface is jless's, unless the README says otherwise | Accepted |
| [ADR-004](#adr-004-correctness-and-uniformity-come-before-performance) | Correctness and uniformity come before performance | Accepted |
| [ADR-005](#adr-005-dependencies-are-published-crates-pinned-by-the-lock) | Dependencies are published crates, pinned by the lock | Accepted |
| [ADR-006](#adr-006-the-reference-is-written-from-the-option-table) | The reference is written from the option table | Accepted |

## ADR-001: aless is a terminal viewer

**Status:** Accepted, 2026-11-02.

**Decision.** aless draws a document on a terminal and reads keys from
it. Without a terminal to draw on (standard output a pipe or a file, no
terminal to open, or `TERM=dumb`) it refuses to start, before reading
any input, with one line on standard error and the exit status 2.
Nothing but `-h`, `--help`, `-V` and `--generate` writes to standard
output. The JSON it writes is `:w FILE` and what the copy keys copy.

**Context.** A program that is both a viewer and a command-line
processor carries two contracts, and each limits the other: an option a
script depends on cannot change for the viewer's sake, and a viewer
feature has to say what it does without a screen.

**Consequences.** A feature that runs without a terminal, writes a
document to standard output, queries one or transforms one does not
belong in aless. The exit statuses are three: 0 the viewer was quit, 1
the terminal failed while the viewer ran, 2 it could not start.

**Enforcement.** `tests/binary.rs` runs the built binary without a
terminal and holds it to the refusal and an empty standard output;
`admin/scripts/pty-smoke.py` does the same in a pseudo-terminal, and
CI's last step does it on all three platforms.

## ADR-002: Every format is read through a tabnas grammar

**Status:** Accepted.

**Decision.** aless has no parser of its own. Every format, a custom
grammar from the command line included, is read by the tabnas engine
with a grammar, and everything above the parser (the tree, the key map,
search, paths, positions, errors) is written once for all of them.

**Context.** A viewer for many formats either grows one reader per
format, each with its own errors and gaps, or reads all of them through
one model and gives up what the model does not hold.

**Consequences.** A new format is a grammar crate, not a feature. The
model holds what every format has in common, so comments, anchors and
tags are not in the tree; the source view shows them. The engine is
slower than a parser written for one format.

**Enforcement.** `tests/formats.rs` loads a fixture of every format
through its grammar, and `tests/grammars.rs` holds every grammar of the
library to its sample's expected tree.

## ADR-003: The interface is jless's, unless the README says otherwise

**Status:** Accepted.

**Decision.** The key map, data and line modes, collapsed previews,
search and the copy commands behave as jless's do. A new binding goes on
a key jless leaves free, and every difference is listed in the README's
"Deviations from jless".

**Context.** A reader who knows jless can use aless at once only while
no key of jless's means something else here.

**Consequences.** Tabs, watching, the explorer and the source view live
on keys jless does not use. A behaviour that departs from jless's needs
its line in the README before it ships.

**Enforcement.** Review, against the README's list; `render` and `tab`
tests pin drawing and navigation.

## ADR-004: Correctness and uniformity come before performance

**Status:** Accepted, 2026-10-24.

**Decision.** Where a correct, uniform design and a faster one differ,
aless takes the correct one, and measures the cost.

**Context.** A viewer that shows a value wrongly, or a format that takes
a route of its own, fails the reader in a way speed does not repay.

**Consequences.** A change that costs time states the measurement in
its pull request. A shortcut that holds for one format and not the
others is not taken.

**Enforcement.** Review.

## ADR-005: Dependencies are published crates, pinned by the lock

**Status:** Accepted.

**Decision.** Every dependency comes from crates.io, at the version
`Cargo.lock` pins. tabnas code is never vendored or copied here.

**Context.** A release that builds from a path or a branch cannot be
rebuilt by anyone else, and a copied parser drifts from the one the
grammar's own repository tests.

**Consequences.** A change that needs an unreleased tabnas fix waits for
its release; `cargo update -p NAME` moves a pin.

**Enforcement.** CI builds with `--locked`, and `cargo package --locked`
builds the crate from exactly the files it publishes.

## ADR-006: The reference is written from the option table

**Status:** Accepted.

**Decision.** Every option is in one table, `cli::OPTIONS`. The parser
refuses any option the table does not list, and `-h`, `--help`, the man
page, the completions and the site's command-line reference are all
written from it, never by hand.

**Context.** A reference kept beside the code by hand disagrees with it
the first time an option changes.

**Consequences.** A new option goes into the table with its summary and
detail. `man/aless.1` and `completions/` are committed output of
`admin/scripts/generate.sh`.

**Enforcement.** `every_option_in_the_table_is_read` in `src/cli.rs`,
`the_generated_files_are_current` in `tests/binary.rs`, and
`the_reference_is_the_binarys` in `tests/site.rs`.
