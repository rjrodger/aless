# Handover: aless and the tabnas fleet, 2026-10-02

Session `session_0159wM7D8uUWqobPsiuXUWkz` (claude.ai/code), 2026-10-01 to
2026-10-02. It reviewed aless and the completeness of the tabnas ports,
then worked through the follow-ups the maintainer chose. It stopped twice
on a usage limit; this page records where each piece stands, so the next
session can start from it rather than from the transcript.

Two companion files sit beside this one:

- [`review-2026-10-02.md`](review-2026-10-02.md): the review report, with
  every finding and how it was verified. A section on checks of a private
  repository is left out of this public copy.
- [`fleet-table.md`](fleet-table.md): one row per tabnas repository from
  the port audit: CI, parity gates, divergences, publication, verdict.

`patches/` holds the one piece of unfinished work that exists nowhere
else (see "Stopped part way").

## What the maintainer asked for

1. Review the state of aless and the completeness of the tabnas parser
   ports, adding tabnas repositories with push permission as needed.
2. Check the GitHub issues and PRs.
3. Fix the CSV/TSV first-field position in aless and open a PR; review
   the PRs and issues and ask about them.
4. The answers to those questions, word for word as chosen:
   - aless #37: "Merge #37, then a follow-up PR".
   - aless follow-ups: "--at past the end exits 4", "Document --at
     snapping instead" (both: outside the text exits 4, inside it the
     snapping is documented), "Rewrite #28 with the real blocker", "Fix
     #17's renderer-buffer loss".
   - parser: "PR with handover patches 1 and 2", "Register 263, 265, 218
     and file the Go findings", "Fix Go 240, 241, 242 and 265", "Close
     #260".
   - fleet: "bnf release 0.1.23", "toml and css Go fixes", "go.mod engine
     to 0.12.8 fleet-wide", "transduce/render/alchemy release path".
5. "Write a handover doc and push to aless": this page.

Those answers are the authority for the open items below. Quote them to
any subagent you give the work to (see "A pitfall to avoid").

## Done

| Item | Outcome |
|---|---|
| aless #37 | Merged (17a5e34). |
| aless #28 | Rewritten: the crates.io move waits on releases of twelve crates whose unreleased main aless calls, and on a transduce release against csv 0.6.0. yaml no longer blocks. |
| parser #260 | Closed: the fleet gate passed for Go and TypeScript at 599e026 (result in the issue). |
| parser issues | Filed #266 (Go records the same bad token twice under relex with recovery) and #267 (Go gives an Err-only custom bad token the code `""`). |
| bnf 0.1.23 | Released. The maintainer merged #96 (the version bump, carrying the engine move) and dispatched `release.yml` (run 37023279722, success). npm's gitHead, `ts/v0.1.23` and `go/v0.1.23` all name ef1b6e0; crates.io has 0.1.23. #95 was closed as superseded by #96. |
| Go engine at v0.12.8 | Merged in 20 repositories: bnf (#96), csv #88, feed #68, hoover #60, ini #89, json #93, json5 #84, jsonc #74, jsonic #100, jsonic-cli #44, jsonl #38, lsp #31, markdown #80, proto #55, railroad #57, semver #29, toml #87, xml #77, yaml #111, zon #86. The maintainer added the fixes three of them needed: json's tracked lockfile (f699544), the TypeScript peer floor in proto (7168b81) and semver (6f57b66), and lsp's CI (b14777b). |

## Open PRs from this session

All are ready for review. "Not independently reviewed" means the second,
adversarial review planned for that change did not run before the usage
limit; the gates listed in each PR and a test shown to fail first are its
verification.

| PR | What it does | State and what it needs |
|---|---|---|
| [rjrodger/aless#38](https://github.com/rjrodger/aless/pull/38) | CSV and TSV fields sit at their values, never on the header line. Second commit aa5843d aligns records by column. | Codex's four findings on the first commit are fixed in aa5843d, answered and resolved. Merges cleanly with main. aa5843d is not independently reviewed. |
| [rjrodger/aless#40](https://github.com/rjrodger/aless/pull/40) | The rest of #15: a lines-mode timeout names its line; tests and docs. | Main's #39 moved the same lockfile pins, so the branch merged main and kept main's `Cargo.lock` (39f4f3e). What remains is the test assertions and the README and skill sentences. |
| [tabnas/parser#268](https://github.com/tabnas/parser/pull/268) | Go: a null definition in map-form options deletes it (#240). | Not independently reviewed. One existing test had pinned the old reader contract and was updated; confirm that is wanted. |
| [tabnas/parser#269](https://github.com/tabnas/parser/pull/269) | Go: Number options without a Sep keep the separator (#241). | **API change**: `NumberOptions.Sep` becomes `*string`. json (`go/json.go:80`) and jsonic (`go/jsonic.go:289` and tests) write `Sep: ""` and must write `Sep: tabnas.String("")` when they take the engine release carrying it. Maintainer decision. |
| [tabnas/parser#270](https://github.com/tabnas/parser/pull/270) | Go: Derive carries each custom matcher once (#242). | Not independently reviewed. |
| [tabnas/parser#271](https://github.com/tabnas/parser/pull/271) | Go: under `line.single` a CRLF is one line token that advances the row (#265). | Not independently reviewed. csv's Go suite passes against it. A shared `lex-line-single.tsv` fixture would pin all three runtimes; left as a follow-up. |
| [tabnas/parser#272](https://github.com/tabnas/parser/pull/272) | Registers #263 (two groups: a Rust row at a raw U+2028, Go columns after an escaped non-ASCII character) and #218 (a block comment with no end, three ways) per ADR-14. | #218's repair direction is written as "ruling pending". Two more splits were measured but not registered (in the PR body). |
| [tabnas/css#56](https://github.com/tabnas/css/pull/56) | Go: string lexing off, as TypeScript has it; 65 stray-quote rows in `test/spec/quotes.tsv`, run by all three runtimes. | The Go css fix ruled into css#53, which merged without it. Afterwards parser's `DIVERGENCE.md`, `go/options.go` and `go/empty_chars_test.go` should stop citing css as a live empty-`Chars` site. |
| [tabnas/toml#89](https://github.com/tabnas/toml/pull/89) | Go: a redefined key raises `toml_key_conflict`, as TypeScript and Rust do; shared `key-conflict.tsv`; Go rejects 280 of 509 invalid corpus documents (was 261). | Two decisions in the PR: TypeScript accepts four TOML-invalid documents, which Go now follows; and the position of a conflict (TypeScript 1:1, Go and Rust at the key) is registered with TypeScript moving. |

## The Go engine at v0.12.8: the last twelve

The same move as the merged twenty. The parser requirement in every
`go.mod` that names it moved to v0.12.8, `go mod tidy` changed nothing
else, and `go build`, `go vet` and `GOWORK=off go test ./...` passed
against the published module. Each is open, on the branch
`claude/deps-go-engine-v0.12.8`:

| PR | Beyond the go.mod move |
|---|---|
| [abnf#105](https://github.com/tabnas/abnf/pull/105) | Peer floor `>=0.12.7` raised to `>=0.12.8`; AGENTS.md line 864 says the two move together. |
| [ebnf#52](https://github.com/tabnas/ebnf/pull/52), [gbnf#55](https://github.com/tabnas/gbnf/pull/55) | Peer floor raised to `>=0.12.8`, as for abnf and as the maintainer did on proto and semver. |
| [support#44](https://github.com/tabnas/support/pull/44) | `ts/package-lock.json`'s `@tabnas/parser` entry moved to 0.12.8 (`npm update --package-lock-only`), as `enginepin.test.js` requires; `npm test` 187 of 187. |
| [c#58](https://github.com/tabnas/c/pull/58), [chess#45](https://github.com/tabnas/chess/pull/45), [css#57](https://github.com/tabnas/css/pull/57), [debug#70](https://github.com/tabnas/debug/pull/70), [directive#65](https://github.com/tabnas/directive/pull/65), [expr#79](https://github.com/tabnas/expr/pull/79), [multisource#69](https://github.com/tabnas/multisource/pull/69), [path#60](https://github.com/tabnas/path/pull/60) | Nothing: the peer range is `>=0`. |

## Not started

- **aless: `--at` outside the text answers `not_found` (exit 4).** Today
  `node_at` (`src/headless.rs`) snaps to the last positioned node at or
  before any position, so a position past the end of the file answers
  the last node with exit 0. Asked: a line after the last line, or a
  column past the end of its line, gives the `not_found` error with
  `nearest`. A position inside the text that falls between nodes keeps
  snapping, and that is documented in the README's "Scripts and agents"
  section, `--help` (WITHOUT A SCREEN) and `skills/aless/SKILL.md`, with
  tests in `src/headless.rs` and `tests/agent.rs`.
- **aless: #17's renderer-buffer loss.** A failure part way through a
  streaming run (a line-by-line read, with or without `--render`) drops
  up to 32 KB of complete, already-rendered records held in the
  renderer's buffer. On a slow stdin with `--timeout` the error says
  `output: none` though a record had parsed. Asked: every complete record
  reaches stdout before the error, never a partial one, on the plain and
  the `--render` paths, with a slow-stdin test in `tests/agent.rs`.
- **transduce, render, alchemy: the release path.** A plan was made and no
  repository was changed. Its main points:
  - All three now carry `ts/package.json` and a Go module, so the fleet's
    standard `release.yml` applies. The two shapes admin #97 offers both
    assumed a Rust-only repository, which is no longer the case.
  - Each first npm publish has to be manual before trusted publishing can
    be set up for the package.
  - go.mod moves to parser v0.12.8, csv v0.6.0 and yaml v0.5.16. render
    and alchemy need transduce's and then render's `go/v` tags first.
  - Decisions for the maintainer: transduce's `tabnas_nodecell` build tag
    (30 of 113 Go tests skip without it); the first release numbers, since
    0.1.0, 0.1.0 and 0.1.1 already exist as crates; whether alchemy stays
    a COMPONENT, which requires a stamped C library.
  - The plan's details touch the private administration repository and
    are not reproduced here. See admin #97 and #102.

## Stopped part way

**parser: handover engine patches 1 and 2** (parser
`doc/handover/README.md`, "Landing order" step 1). In a worktree from main,
patch 1 was applied as 4535a89 and patch 2 as 8aa930e, with the
`ParseMode` conflict resolved as the handover prescribes. Work on the open
findings had begun, uncommitted, in six files. None of it was reviewed or
gated. It is saved here:

- `patches/parser-engine-1-and-2-committed.patch`: the two commits
  (`git am`).
- `patches/parser-engine-1-and-2-uncommitted.patch`: the work in progress
  on top (`git apply`).

Resume: apply both to a branch from parser main, finish the open findings
the handover lists for Engine 1 and 2, run all three runtimes, and open
the PR, which closes #251. The original patches remain on parser main
under `doc/handover/patches/`. After that come the downstream Rust test
PRs for css, toml and yaml in the same window, then patch 3b (a merge of
a removed default fixed token follows TypeScript, in Rust and Go).

## Decisions waiting on the maintainer

1. #241's API change (parser#269): accept `*string`, or another shape.
2. #218's repair direction (parser#272 registers it as pending).
3. #244: the issue asks Rust to match Go, but TypeScript copies
   decorations before re-running plugins, as Rust does, which ADR-13 makes
   canonical; the yaml Rust plugin's guard is what breaks.
4. toml#89: whether Go should follow TypeScript in accepting four
   TOML-invalid documents, and which runtime moves on the conflict
   position.
5. The two divergences measured but not registered (parser#272's body):
   Rust cuts a multi-character block-comment end one character short;
   Go counts no row for U+2028 inside a multi-line string under json5's
   line configuration.
6. tabnas/json's `go/debugtest/go.mod` carries a committed `replace` of
   the engine to `../../../parser/go`, which the fleet's guidance says is
   never committed.
7. The release-path questions above.

## A pitfall to avoid

The background agents this session started saw only the latest user
message ("Fix the CSV/TSV first-field position in aless and open a PR;
review the PRs and issues, ask me about them") as the user's request. Many
of them treated the approved work as unauthorised and refused it: 20 of
31 engine bumps, the bnf release, and the transduce and alchemy release
paths. The session then did those by hand. When work is handed to
subagents, quote the maintainer's answers above word for word in the
prompt, or do the work directly.

## How the work was set up

The setup was in the session's container, which is gone with it. Recreate
it as follows:

- Every tabnas repository cloned as a sibling under one directory
  (`/home/user/<name>` here), so the grammar crates' `../../parser/rs`
  path dependencies resolve. Shallow clones are enough for reading; fetch
  before branching.
- One git worktree per change under a separate directory
  (`/home/user/wt/<repo>-<topic>`), with a symlink `wt/parser` to the
  engine checkout for the path dependencies, and `CARGO_TARGET_DIR` set
  per repository.
- Gates: aless's four (`cargo fmt --all --check`, `cargo clippy
  --all-targets --locked -- -D warnings`, `cargo test --locked`, `python3
  scripts/pty-smoke.py target/debug/aless`); for the fleet, each
  repository's AGENTS.md. The parser fleet gate is `ci/fleet/run-fleet.sh
  --runtime go|ts`.
- Toolchain here was Node 22 (packages declare `>=24`; suites ran), Go
  1.24.7 and Rust 1.97. Check yours: a machine's inventory is not the
  repository's.
