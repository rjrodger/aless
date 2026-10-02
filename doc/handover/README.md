# Handover: aless and the tabnas fleet, 2026-10-02

Session `session_0159wM7D8uUWqobPsiuXUWkz` (claude.ai/code), 2026-10-01 to
2026-10-02. It reviewed aless and the completeness of the tabnas ports,
then worked through the follow-ups the maintainer chose. It stopped twice
on a usage limit. It stopped for good at 20:40 UTC on 2026-10-02, when the
maintainer asked for this page. Every state below was read at that time.
The next session can start from this page rather than from the transcript.

Two companion files sit beside this one:

- [`review-2026-10-02.md`](review-2026-10-02.md): the review report, with
  every finding and how it was verified. A section on checks of a private
  repository is left out of this public copy.
- [`fleet-table.md`](fleet-table.md): one row per tabnas repository from
  the port audit, every major gap in full with the verdict of its second
  check, and the release tags of each current version.

No work exists only in the session's container. Everything is pushed to a
branch with a PR, and nothing is left uncommitted.

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
5. "Write a handover doc and push to aless", and later "stop and write a
   handover doc in aless": this page.

Those answers are the authority for the open items below. Quote them to
any subagent you give the work to (see "A pitfall to avoid").

## Done

| Item | Outcome |
|---|---|
| aless #37 | Merged (17a5e34). |
| aless #28 | Rewritten: the crates.io move waits on releases of twelve crates whose unreleased main aless calls, and on a transduce release against csv 0.6.0. yaml no longer blocks. |
| parser #260 | Closed: the fleet gate passed for Go and TypeScript at 599e026 (result in the issue). |
| parser issues | Filed #266 (Go records the same bad token twice under relex with recovery) and #267 (Go gives an Err-only custom bad token the code `""`). |
| bnf 0.1.23 | Released. The maintainer merged #96 (the version bump, carrying the engine move) and dispatched `release.yml` (run 37023279722, success). npm's gitHead, `ts/v0.1.23` and `go/v0.1.23` all name ef1b6e0, and crates.io has 0.1.23. There is no `rs/v0.1.23` tag. #95 was closed as superseded by #96. |
| Go engine at v0.12.8 | Merged in 20 repositories: bnf (#96), csv #88, feed #68, hoover #60, ini #89, json #93, json5 #84, jsonc #74, jsonic #100, jsonic-cli #44, jsonl #38, lsp #31, markdown #80, proto #55, railroad #57, semver #29, toml #87, xml #77, yaml #111, zon #86. The maintainer added the fixes three of them needed: json's tracked lockfile (f699544), the TypeScript peer floor in proto (7168b81) and semver (6f57b66), and lsp's CI (b14777b). |

## Open PRs from this session

All are ready for review. "Not independently reviewed" means the second,
adversarial review planned for that change did not run. The gates listed
in each PR, and a test shown to fail first, are its verification. "Codex"
is the review bot `chatgpt-codex-connector`. None of its open findings
below has been answered or verified yet; each needs a check, a fix or a
reply, and then its thread resolved.

| PR | What it does | CI at 20:40 UTC | What it needs |
|---|---|---|---|
| [rjrodger/aless#38](https://github.com/rjrodger/aless/pull/38) | CSV and TSV fields sit at their values, never on the header line. The second commit, aa5843d, aligns records by column. | Green, mergeable. | Codex's four findings on the first commit are fixed in aa5843d, answered and resolved. aa5843d is not independently reviewed, and Codex has not reviewed it. |
| [rjrodger/aless#40](https://github.com/rjrodger/aless/pull/40) | The rest of #15: a lines-mode timeout names its line, with tests and docs. | Green, mergeable. | One Codex P2 at `skills/aless/SKILL.md:273`. The skill promises a line for every record-at-a-time timeout, but a deadline that expires while standard input is blocked gives a null `line` and `col`. The README already says so, and the skill should too. |
| [rjrodger/aless#41](https://github.com/rjrodger/aless/pull/41) | This handover. | Green. | Codex's three findings are answered by the commit that carries this page. |
| [tabnas/parser#268](https://github.com/tabnas/parser/pull/268) | Go: a null definition in map-form options deletes it (#240). | **prose red**; the rest green. | The prose gate: see "Clearing the prose gate". One Codex P2 at `go/utility.go:1500`: `match.value.<name>: false` now becomes a deletion marker, but TypeScript's validator rejects `false` for `match.value`. Remove the bool case and tighten the Go validator, after checking TypeScript. Not independently reviewed, and one existing test had pinned the old reader contract and was updated. |
| [tabnas/parser#269](https://github.com/tabnas/parser/pull/269) | Go: Number options without a Sep keep the separator (#241). | Green on every check. | Reworked at ec4915c, see "The #241 fix". Review, then merge. |
| [tabnas/parser#270](https://github.com/tabnas/parser/pull/270) | Go: Derive carries each custom matcher once (#242). | Green. | Not independently reviewed. |
| [tabnas/parser#271](https://github.com/tabnas/parser/pull/271) | Go: under `line.single` a CRLF is one line token that advances the row (#265). | **prose red**; the rest green. | The prose gate. Not independently reviewed. csv's Go suite passes against it. A shared `lex-line-single.tsv` fixture would pin all three runtimes, which is left as a follow-up. |
| [tabnas/parser#272](https://github.com/tabnas/parser/pull/272) | Registers #263 and #218 per ADR-14. | Green. | Four Codex P2s on `DIVERGENCE.md`, each asking for a registered row: a multi-character block-comment end (line 895); an end-less comment whose body holds the word `undefined`, where TypeScript answers `unexpected` (line 854); U+2028 or U+2029 also in `line.chars`, where Rust answers `unprintable` (line 784); and a custom `string.escape` mapping of a non-ASCII character, which Go misses because it looks up the first UTF-8 byte (line 808). #218's repair direction is written as "ruling pending". |
| [tabnas/parser#274](https://github.com/tabnas/parser/pull/274) | Handover engine patches 1 and 2, in progress. | **rust red.** 11792ed fixed a misplaced clippy allow, and the job then failed on three `clippy::format_collect` errors in `rs/tests/linear_time_test.rs`. fleet-pr was still running; the rest is green. | See "Stopped part way". |
| [tabnas/css#56](https://github.com/tabnas/css/pull/56) | Go: string lexing off, as TypeScript has it; 65 stray-quote rows in `test/spec/quotes.tsv`, run by all three runtimes. | Green. | Afterwards parser's `DIVERGENCE.md`, `go/options.go` and `go/empty_chars_test.go` should stop citing css as a live empty-`Chars` site. |
| [tabnas/toml#89](https://github.com/tabnas/toml/pull/89) | Go: a redefined key raises `toml_key_conflict`, as TypeScript and Rust do; shared `key-conflict.tsv`. | Green, mergeable. | Codex P1 at `test/conformance.tsv:127`: the new Go baseline, 280 of 509, was measured against the published parser v0.12.7, while CI runs against parser main as a sibling. Re-measure every runtime as CI does. Codex P2 at `go/refs.go:159`: `[[a.b]]` then `[[a.b.c]]` is a valid nested array of tables, which Go rejects with `toml_key_conflict` because it calls `tableAt(DEFINE)`. Use `arrayAt` when `table_array` is set, and add the case to the shared fixtures. Two decisions are also in the PR: TypeScript accepts four TOML-invalid documents, which Go now follows; and the position of a conflict, TypeScript 1:1, Go and Rust at the key, is registered with TypeScript moving. |

## The Go engine at v0.12.8: the last twelve

The same move as the merged twenty. The parser requirement in every
`go.mod` that names it moved to v0.12.8, `go mod tidy` changed nothing
else, and `go build`, `go vet` and `GOWORK=off go test ./...` passed
against the published module. Each is open on the branch
`claude/deps-go-engine-v0.12.8`, green, with no review findings:

| PR | Beyond the go.mod move |
|---|---|
| [abnf#105](https://github.com/tabnas/abnf/pull/105) | Peer floor `>=0.12.7` raised to `>=0.12.8`; AGENTS.md line 864 says the two move together. |
| [ebnf#52](https://github.com/tabnas/ebnf/pull/52), [gbnf#55](https://github.com/tabnas/gbnf/pull/55) | Peer floor raised to `>=0.12.8`, as for abnf and as the maintainer did on proto and semver. |
| [support#44](https://github.com/tabnas/support/pull/44) | `ts/package-lock.json`'s `@tabnas/parser` entry moved to 0.12.8 (`npm update --package-lock-only`), as `enginepin.test.js` requires; `npm test` 187 of 187. |
| [c#58](https://github.com/tabnas/c/pull/58), [chess#45](https://github.com/tabnas/chess/pull/45), [css#57](https://github.com/tabnas/css/pull/57), [debug#70](https://github.com/tabnas/debug/pull/70), [directive#65](https://github.com/tabnas/directive/pull/65), [expr#79](https://github.com/tabnas/expr/pull/79), [multisource#69](https://github.com/tabnas/multisource/pull/69), [path#60](https://github.com/tabnas/path/pull/60) | Nothing: the peer range is `>=0`. |

## The #241 fix

parser#269 first made `NumberOptions.Sep` a `*string`. That broke every
module that names the field: json and jsonic write `Sep: ""`, and bnf's
options serializer passes `o.Sep` as a string, so the fleet, gate and
rust jobs could not compile json or bnf.

ec4915c follows the direction issue #241 itself gives instead. `Sep`
stays a string. An empty `Sep` is "not supplied" and keeps the default
`_` or the base value, as `StringOptions.Chars` already documents. The
new constant `NumberSepNone`, the NUL character, switches the separator
off, and a serialized `"sep": null` or `"sep": ""` arrives as it. The Go
fleet gate passed all 29 suites locally, and CI is green on every check,
including the json and jsonic token-stream parity.

What follows once an engine release carries it:

- json and jsonic should write `Sep: tabnas.NumberSepNone`. Until then
  their separator is on. They still reject `1_000`, because their number
  `Exclude` reads the raw source with a strict regex, but they pay a
  separator comparison per number byte.
- bnf's `go/options_data.go` writes `sep` only when it is non-empty, so it
  already drops an "off" separator, and it would write `NumberSepNone` as
  a NUL. It should write `"sep": null` for `NumberSepNone`.
- csv sets only `Lex`. With number lexing on, Go csv now reads `1_000` as
  1000, as TypeScript csv does.

## Clearing the prose gate

parser's `prose` job fails whenever a gated page's Vale alert counts
change. `ts/scripts/vale-counts.cjs` compares the counts recorded in
`.vale.ini` and `doc/STYLE-GUIDE.md` with a live Vale run, and any
difference, up or down, fails. #268 adds six alerts, in `Google.Passive`,
`Google.Contractions`, `Google.Parens` and `Google.Semicolons`. #271 adds
eleven, in those and in `Google.Acronyms`, `Google.Headings` and
`Google.Colons`.

Vale is not installed by default. CI pins 3.22.0:

```bash
curl -sSfL -o vale.tar.gz https://github.com/errata-ai/vale/releases/download/v3.22.0/vale_3.22.0_Linux_64-bit.tar.gz
tar xzf vale.tar.gz vale && export PATH="$PWD:$PATH"
cd <parser worktree> && vale sync && node ts/scripts/vale-counts.cjs
```

Rewrite the added prose so it adds no alerts: no parentheses, semicolons
or passive voice, and no phrase Google would contract. Then, if the count
still moved, run `node ts/scripts/vale-counts.cjs --write` and commit
`.vale.ini` and `doc/STYLE-GUIDE.md` with the change. ec4915c on #269 is
the example: its new prose adds nothing and drops one parenthesis, so the
count went from 1714 to 1713. `node --test ts/test/docs.test.js` checks
the house rules Vale cannot.

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
  repository was changed. The release table in `fleet-table.md` shows the
  gap: each is a crate only, with no `ts/v` or `go/v` tag and nothing on
  npm. The plan's main points:
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

**parser: handover engine patches 1 and 2**, now
[tabnas/parser#274](https://github.com/tabnas/parser/pull/274), towards
#251. It follows the parser's own `doc/handover/README.md`, "Landing
order" step 1:

- 4535a89 applies patch 1 and 8aa930e applies patch 2, with the
  `ParseMode` conflict resolved as that handover prescribes.
- f109c0b is unreviewed work on the open findings: Rust's
  `continuations()` at a fetched bad token follows TypeScript, and
  `test/spec/bad-token.tsv` gains `continuations` rows. Go answers three
  cases differently, which the file's header records.
- 11792ed fixes the clippy failure f109c0b introduced.
- CI's rust job still fails, on `clippy::format_collect` at lines 67,
  117 and 121 of `rs/tests/linear_time_test.rs`, which patch 2 adds.
  Each builds a `String` with `map(|..| format!(..)).collect()`. Build
  it with `fold` and `write!`, as clippy suggests. A local `cargo clippy
  --all-targets --all-features -- -D warnings` on Rust 1.97 did not
  report it, so check with `ci/rust/run.sh`, which CI runs.

Codex reviewed f109c0b and left two findings, both unanswered:

- P1 at `test/spec/bad-token.tsv:52`: the file's header names Go's
  different answers under relex with recovery and at the
  `maxRecoveries` cap, but no group in `test/spec/divergent.tsv` or
  `DIVERGENCE.md` registers them, so the parity gate cannot see either
  side change. Register them per ADR-14, or fix Go (#266 covers the
  first).
- P2 at `rs/src/parser.rs:1977`: for a built-in lexer fault, the new
  branch hands lex subscribers a mutable `#BD` token, then recovers
  from, or fails with, the original error, ignoring what a subscriber
  changed. TypeScript runs subscribers before `parse_alts` reads the
  token, so the mutated token should take the normal token path.

`cargo test` and `go test ./...` pass on it locally. Still to do: fix the
clippy errors, run the TypeScript suite, finish the open findings the parser handover lists for
Engine 1 and 2, review f109c0b, and pass the fleet gate. After that come
the downstream Rust test PRs for css, toml and yaml in the same window,
then patch 3b, in which a merge of a removed default fixed token follows
TypeScript, in Rust and Go.

## Decisions waiting on the maintainer

1. #218's repair direction (parser#272 registers it as pending).
2. #244: the issue asks Rust to match Go, but TypeScript copies
   decorations before re-running plugins, as Rust does, which ADR-13 makes
   canonical; the yaml Rust plugin's guard is what breaks.
3. toml#89: whether Go should follow TypeScript in accepting four
   TOML-invalid documents, and which runtime moves on the conflict
   position.
4. The two divergences measured but not registered (parser#272's body):
   Rust cuts a multi-character block-comment end one character short;
   Go counts no row for U+2028 inside a multi-line string under json5's
   line configuration. Codex's first finding on #272 asks for the first.
5. tabnas/json's `go/debugtest/go.mod` carries a committed `replace` of
   the engine to `../../../parser/go`, which the fleet's guidance says is
   never committed.
6. The release-path questions above.

The #241 API question is gone: #269 no longer changes the field's type.

## A pitfall to avoid

The background agents this session started read the latest user message
as the whole request. Early on, that message was "Fix the CSV/TSV
first-field position in aless and open a PR; review the PRs and issues,
ask me about them". Many agents treated the approved work as unauthorised
and refused it: 20 of 31 engine bumps, the bnf release, and the transduce
and alchemy release paths. The session then did those by hand. At the
end it happened again: 9 of 22 read-only status agents read "stop and
write a handover doc in aless" as addressed to them and returned
placeholders. When work is handed to subagents, quote the maintainer's
answers above word for word in the prompt, say the task is part of them,
or do the work directly.

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
  --runtime go|ts`, and its prose gate needs Vale, as above.
- The toolchain here was Node 22 (packages declare `>=24`; suites ran), Go
  1.24.7 and Rust 1.97. Check yours: a machine's inventory is not the
  repository's.
- No check-in is scheduled. The last one fired at 20:05 UTC and was not
  re-armed, because the maintainer asked to stop.
