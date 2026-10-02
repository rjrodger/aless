# Fleet audit table, 2026-10-02

One row per tabnas repository from the port audit this session ran on 2026-10-01 and 2026-10-02. The columns are the CI result on main, the number of shared fixture files and which runtimes run them, the divergences recorded and whether an executable register holds them, the count of major or worse gaps per runtime, and a risk grade. The rows describe main as it was then. Since then bnf released 0.1.23, and the Go engine requirement moved to v0.12.8 in twenty repositories.

| repo | kind | CI main | shared fixtures | run by ts/go/rs | divergences recorded / registered | major+ gaps (go / rs / all) | risk |
|---|---|---|---|---|---|---|---|
| json | COMPONENT | green | 3 | y/y/y | 3 / no | 0 / 0 / 1 | low |
| jsonl | COMPONENT | green | 5 | y/y/y | 3 / no | 0 / 0 / 0 | low |
| jsonic | COMPONENT | green | 68 | y/y/y | 4 / yes | 0 / 0 / 0 | low |
| jsonc | COMPONENT | green | 12 | y/y/y | 2 / no | 0 / 0 / 0 | low |
| json5 | COMPONENT | green | 128 | y/y/y | 5 / yes | 1 / 0 / 0 | low |
| yaml | COMPONENT | green | 28 | y/y/y | 7 / no | 1 / 0 / 0 | medium |
| toml | COMPONENT | green | 17 | y/y/y | 7 / yes | 1 / 0 / 0 | low |
| ini | COMPONENT | green | 39 | y/y/y | 6 / no | 1 / 0 / 0 | low |
| csv | COMPONENT | green | 143 | y/y/y | 1 / no | 0 / 0 / 0 | low |
| xml | COMPONENT | green | 11 | y/y/y | 5 / no | 1 / 0 / 1 | low |
| zon | COMPONENT | green | 12 | y/y/y | 6 / no | 0 / 0 / 0 | low |
| markdown | COMPONENT | green | 22 | y/y/y | 2 / no | 0 / 0 / 0 | low |
| feed | COMPONENT | green | 8 | y/y/y | 0 / yes | 0 / 0 / 0 | low |
| abnf | COMPONENT | green | 7 | y/y/y | 6 / no | 0 / 1 / 0 | low |
| bnf | COMPONENT | green | 0 | n/n/n | 7 / yes | 2 / 0 / 1 | medium |
| ebnf | COMPONENT | green | 0 | n/n/n | 5 / no | 1 / 0 / 1 | low |
| gbnf | COMPONENT | green | 9 | y/y/y | 5 / no | 4 / 0 / 0 | low |
| hoover | COMPONENT | green | 7 | y/y/y | 2 / no | 0 / 0 / 1 | low |
| css | COMPONENT | green | 10 | y/y/y | 5 / yes | 0 / 1 / 0 | low |
| c | COMPONENT | green | 38 | y/y/y | 3 / yes | 0 / 2 / 2 | medium |
| proto | COMPONENT | green | 15 | y/y/y | 7 / yes | 1 / 1 / 0 | low |
| semver | COMPONENT | green | 7 | y/y/y | 1 / no | 0 / 1 / 0 | low |
| chess | COMPONENT | green | 10 | y/y/y | 0 / no | 0 / 1 / 0 | low |
| expr | COMPONENT | green | 41 | y/y/y | 3 / no | 1 / 0 / 2 | medium |
| directive | COMPONENT | green | 8 | y/y/y | 9 / no | 0 / 0 / 1 | low |
| path | COMPONENT | green | 4 | y/y/y | 2 / no | 0 / 1 / 1 | low |
| multisource | COMPONENT | green | 7 | y/y/y | 4 / no | 0 / 0 / 1 | low |
| debug | COMPONENT | green | 3 | y/y/y | 15 / no | 1 / 0 / 1 | medium |
| railroad | TOOL | green | 2 | y/y/y | 0 / no | 0 / 0 / 1 | low |
| support | TOOL | green | 10 | y/y/y | 0 / yes | 0 / 0 / 0 | low |
| jsonic-cli | TOOL | mixed | 6 | y/y/y | 12 / yes | 0 / 1 / 1 | medium |
| lsp | TOOL | green | 2 | y/y/y | 1 / no | 0 / 0 / 1 | low |
| transduce | TOOL | green | 6 | y/y/y | 4 / no | 4 / 0 / 1 | high |
| render | TOOL | green | 5 | y/y/y | 0 / no | 1 / 0 / 3 | high |
| alchemy | COMPONENT | green | 4 | y/y/y | 0 / no | 2 / 0 / 3 | high |
| skills | TOOL | green | 0 | n/n/n | 0 / no | 0 / 2 / 1 | low |
| mcp | TOOL | green | 0 | n/n/n | 0 / no | 0 / 0 / 1 | low |

## Major and blocker gaps by repository

Each entry reads repository [severity/runtime/confidence]. Every one of the 58 is given in full, as the audit recorded it. An adversarial check re-derived 24 of them from primary sources, and its verdict follows each of those. The other 34 were not checked a second time. The review report, section 4, says what changed after the audit.

- json [major/all/high]: No executable cross-runtime divergence register (ADR-14). The three behavioural asymmetries (exponent overflow, integer-like key order, 127-level depth/cancel) are recorded only in AGENTS.md prose and in single-runtime tests; TS pins nothing for its side of the 1e999 case, so the asymmetry is asserted from one end only and a TS change to reject would pass every gate here. **Refuted by the check:** The load-bearing part of the claim is false. TypeScript DOES pin its side of the overflow (and depth) asymmetry, executably and in CI: ts/test/conformance.test.js's "i_* : same accept/reject and same value as JSON.parse" test asserts, for every implementation-defined case in the pinned nst/JSONTestSuite corpus, that this package's accept/reject verdict equals JSON.parse's.
- json5 [major/go/high]: Go reports `unprintable` where TypeScript and Rust report `unexpected` for a string literal holding both an ES5.1-forbidden escape (`\1`..`\9`, `\0<digit>`, `\u{`) and a raw control character: jsonic's Go-only `jsonic$unprintable` pre-scan matcher runs ahead of the plugin's string check. Not in DIVERGENCE.md, no fixture row; the code is the contract. *Confirmed by the check and re-graded minor.*
- yaml [major/go/high]: An alias used as an implicit key with the colon directly after it parses differently in Go: '*al: a' gives {"*al":"a"} in TS and Rust but [null,"a"] in Go, and '- {*al: 1}' parses in TS/Rust and is refused in Go. A value difference, not in DIVERGENCE.md, no fixture row, repair direction undecided (TS's own reading may not be what YAML means for an alias key). *Confirmed by the check.*
- ini [major/go/high]: Parse's shared default instance is not safe for concurrent use although the docs say it is: declaredSections is a closure variable per instance (go/ini.go:600 `var declaredSections map[string]bool`, reset at :608 in @ini-bo, read :617, written :649), so concurrent Parse calls race and `section.duplicate: error` can fire or miss across documents; the Rust port moved this state to the parse context (rs/AGENTS.md 'Per-parse state lives on the context', pinned by concurrent_parses_do_not_share_declared_sections in rs/tests/ini_test.rs:705). Open as issue #81 with a -race reproduction (7 data races). *Confirmed by the check.*
- toml [major/go/high]: The Go port does not raise toml_key_conflict: a key redefined as a table or array-of-tables is accepted and the earlier value silently replaced, where TypeScript and Rust diagnose it. Nine BurntSushi invalid documents (go 261/261/0 vs ts and rust 278/278/0 in test/conformance.tsv). The recorded reason - the Go engine turns every panic in a grammar action into `internal` - is no longer true of the engine toml pins. *Confirmed by the check.*
- xml [major/all/high]: No executable divergence register (ADR-14). The five result differences above live in prose in rs/README.md and rs/AGENTS.md and are pinned, where at all, by Rust-only in-language tests; nothing in the TS or Go suites asserts them, and go/doc/concepts.md:99-101 states the ports 'produce identical parse results for the shared conformance fixtures' without mentioning the astral-entity or depth differences. *Confirmed by the check.*
- xml [major/go/high]: Go disagrees with TypeScript on an `<!ENTITY>` declaration whose name starts outside the BMP: Go records it and resolves a later `&name;`; TS (and Rust, mirroring TS) skips the declaration and raises `undeclared_entity`. A three-way result difference with no fixture, no register row, and no mention in go/doc. rs/AGENTS.md names TS as the defective side (XML 1.0 [4] admits #x10000-#xEFFFF), so the repair direction is TS first, then Rust; until then Go is the port out of step with the canonical result. *Confirmed by the check and re-graded minor.*
- abnf [major/rs/high]: Long single rules are quadratic in the Rust engine (10-60x slower than TS), so the conformance sweep's RFC5322.abnf sits on the 60 s budget line and the exact-set known-gaps register flaps with GitHub runner speed; mitigated with a 'budget-timing' row kind that waives one outcome, root cause in parser/rs Rule::accept_child_node's clone, not in this crate **Refuted by the check:** Not a port gap by the stated test (TypeScript has a behaviour or surface that Rust lacks today on main). Every runtime rejects RFC5322.abnf with the same message; the only difference is wall-clock time in abnf's UNOPTIMISED Rust test profile, and in a release build the Rust compiler is faster than TypeScript on this very file (6.52 s vs node's 13 s, per abnf#81's own measurement).
- bnf [blocker/all/high]: The replace-loop compiler (#80, merged 2026-09-28) is on main but unreleased; the last release 0.1.22 (2026-09-26) predates it, so every npm and Go consumer (abnf, ebnf, gbnf, semver, proto) and the crates.io crate still compile *A/1*A/m*A as push chains, the shape the fleet rule forbids and that trips aless's 3,000-rule depth guard *Confirmed by the check and re-graded major.*
- bnf [major/go/high]: optionsToData drops CommentDef lex, eatline and suffix (and silently drops a function-valued Suffix), so a spec passed through SpecToData/SpecToJSON/ToRecognitionSpec/ToPureSpec loads as a grammar accepting a different language; TS cloneData and Rust clone options whole *Confirmed by the check.*
- bnf [major/go/high]: Go has no byte-for-byte oracle against the TypeScript emitter; parity is asserted only by the downstream front-end suites and a one-off c.gbnf comparison recorded in prose, so a Go emit divergence surfaces only if a front-end test happens to exercise it
- ebnf [major/all/high]: No shared fixtures and no executable cross-runtime register: parity rests on a Rust-only recorded oracle and hand-mirrored unit tests, and the repo declares no error codes to pin with ERROR:<code> rows *Confirmed by the check.*
- ebnf [major/go/high]: The Go front-end is a hand-written recursive-descent scanner, not the tabnas rule table TS and Rust use, so it exports no ebnfRules equivalent and its lexical diagnostics are hand-written; a defect in the canonical meta-grammar does not reproduce in Go and vice versa *Confirmed by the check and re-graded minor.*
- gbnf [major/go/high]: No renderer and no gbnf-check command: renderGbnf (IR to GBNF text, the ABNF-to-GBNF bridge) and ts/src/cli.ts have no Go counterpart; Rust has both (render_gbnf, cli::run). *Confirmed by the check.*
- gbnf [major/go/high]: No TypeScript oracle or differential parity gate for Go; parity is claimed from hand-mirrored accept/reject tables, and the Go columns of DIVERGENCE.md are asserted nowhere. *Confirmed by the check.*
- gbnf [major/go/high]: Live corpus is compile-only in Go: the 77 schema-generated grammars are compiled but no sample is graded accept or reject, where TS and Rust grade every case in both directions.
- gbnf [major/go/high]: A surrogate-pair escape decodes to two U+FFFD instead of one astral character, so a grammar spelling an emoji as 😀 accepts different strings in Go than in TS and Rust.
- hoover [major/all/high]: The two open divergences are not in an executable register run by every suite (ADR-14: 'every unrepaired divergence lives in a register both runtimes execute; a prose-only divergence record is a defect'). They are pinned in Rust only; the TypeScript and Go suites would not notice if either side changed. *Confirmed by the check and re-graded minor.*
- css [major/rs/high]: The engine-based Rust port on main is published nowhere: crates.io tabnas-css 0.5.9 is the previous standalone implementation, released under the same version number, so the next release must bump to 0.5.10 before the real crate can ship. *Confirmed by the check and re-graded minor.*
- c [major/rs/high]: The Rust corpus match is 63/100 exact; 37 Csmith seeds lose the tokens of every prefix-operator-only brace-initializer item (DIVERGENCE.md section 3), so the 'concrete' tree is not concrete there. *Confirmed by the check.*
- c [major/all/high]: A ternary as a whole declaration initializer builds a self-containing node in the canonical; the Go port kills the host process (uncatchable stack overflow) where TS throws and Rust returns cancel. *Confirmed by the check.*
- c [major/rs/medium]: Two open Rust-only resource defects: a one-line top-level call `f(1);` allocates until the process aborts (legacy structurer), and 10,000 init-declarators cost 37 s and 4 GB at constant rule depth.
- c [major/all/medium]: Canonical design defects shared by all three runtimes: each binary_expression holds its operands twice (children and left/right) so the serialized tree doubles per operator, and three small hostile shapes take super-linear time (exponential rendering, exponential nested parameter lists under extended, quadratic #if folding).
- proto [major/go/high]: Go diverges from canonical on four registered inputs, all unrepaired and all in go/build_descriptor.go: jsNumber strips the sign before choosing a base so `-0x10` becomes -16 (line 208); toInt accepts a digit separator so `1_0` becomes 10 (line 183); a Go map keeps a field option named `__proto__`; buildGroup discards proto3Optional so a proto3 optional group gets no `_g` oneof (line 451). Go is the only port with parity defects it owns. *Confirmed by the check and re-graded minor.*
- proto [major/rs/high]: Parse time is quadratic in the number of elements inside one repetition (fields in a message, names in a reserved list, definitions in a file): 400 fields take 10.1 s in a debug build against 0.129 s in TypeScript, 800 fields 46.6 s. Every answer matches, so it is not a divergence, but an ordinary large .proto is 100x slower in the Rust port. The cause is in the engine (Rule::accept_child_node cloning the parent's accumulator in parser/rs/src/rule.rs), not in this crate, and deliberately no ratio test pins it. **Refuted by the check:** The claim describes an engine defect that no longer exists on main. The cause it names, Rule::accept_child_node holding a second Arc handle on the parent's own accumulator, was removed from the engine on 2026-09-21/22 (parser PR #193 "perf(rs): let a buried rule release its self-referential child node" and the #195 fix "flatten a repetition by handle"); issue parser#195, which was filed FROM proto's table (same 50..800-field numbers), is closed as completed.
- semver [major/rs/high]: The repository tells Rust users the crate is unpublished and to clone three to five sibling repositories, but tabnas-semver 0.0.5 is on crates.io and rs/v0.0.5 is tagged; the Rust install documentation is wrong for every consumer. *Confirmed by the check and re-graded minor.*
- chess [major/rs/high]: No CI gate runs the Rust suite, although the crate is published to crates.io at every release. The Rust port is tested only when someone runs `make test-rs` by hand. *Confirmed by the check.*
- expr [major/go/high]: A dangling operator (`1+`, `-`, `a:1+`, `1+2+`) parses to a self-referential *ListRef (the top node holds itself in the missing-operand slot) where TypeScript returns a finite tree; a consumer walking the value, Simplify, or json.Marshal loops or overflows. Not recorded in DIVERGENCE.md and pinned by no fixture; the stamped C library carries a cycle detector as a workaround. *Confirmed by the check.*
- expr [major/ts/high]: With an evaluator and a ternary operator, `(1?2:3)+1` evaluates to 4 in TypeScript and 3 in Go and Rust: the ternary after-close writes the evaluated value along the r.prev chain onto ctx.NORULE, whose node then seeds the val for the `1`. Go guards the write with != jsonic.NoRule. Per ADR-13 the repair is in TypeScript; nothing pins it (evaluate-math.tsv has no ternary; ternary-paren-preval.tsv runs without an evaluator). *Confirmed by the check.*
- expr [major/all/medium]: A ternary directly inside a map or list with an evaluator is wrong in all three runtimes, differently in each: `a:1?2:3` -> TS TypeError 'Cannot create property a on number 2', Go `{}`, Rust `2.0`; `[1?2:3, 4]` -> TS TypeError, Go `[]`, Rust `2.0`. Reported as 'Related, separate' in #62 and has no issue of its own (only #62 and #64 are open).
- directive [major/all/high]: Open plugin defect #54: for a close-token directive the plugin inserts a {s: CLOSE, b: 1} close alternate into each open rule (val) with no value action, so over a GrammarSpec-defined host grammar (values set by @value$ in alternate actions) a closed body like add<[1,2,3]> reaches the action undefined and yields 0 instead of 6. Same code in all three runtimes; only the Go C library carries a workaround.
- path [major/rs/high]: Child path is derived on the child side and keyed on the parent rule NAME: a host whose pair/elem pushes a rule not named val gets no path/key/index on that child; TS and Go set it on any child. Also, a host that registers @val-bo/prepend, @map-bo/prepend, @list-bo/prepend or @elem-ao/prepend itself, or a /replace reference for one of those phases, silently drops Path's hook.
- path [major/all/high]: No executable divergence register (ADR-14): the two behavioural divergences above live only in rs/AGENTS.md and rs/README.md prose; there is no DIVERGENCE.md, no test/spec/divergent.tsv, and no gate in any suite. The meta.path.base string corner also states no repair direction (ADR-13).
- multisource [major/all/high]: No executable divergence register (ADR-14): the four recorded rows are pinned only on the Rust side; the Go cells (js source is raw text; 4000-chain gives {"end":1}; alias falls to raw text; merge once per key) and the TS cells have no test keeping them true, so a repair or regression in Go or TS goes unnoticed.
- debug [major/go/high]: Go Abnf() does not render the bnf#80 repeat loop. For every grammar compiled by current abnf/ebnf/gbnf (every `*A`), Go emits the loop as one of its own alternatives, `r-gen1-star-A = [ r-gen1-star-A / r-gen1-star-A-alt0 ]`, and the recompiled grammar rejects inputs the original accepts. TS (#66) and Rust (#63) both render `*A` / `*( a b )` / `1*A`.
- debug [major/all/high]: The published 0.3.9 of every runtime (npm, go/v0.3.9, crates.io tabnas-debug) predates the repeat-loop rendering: release run 20 shipped e47934b on 2026-09-26; #63 (Rust) merged 2026-09-29 and #66 (TypeScript) 2026-10-01. Against grammars compiled by the current bnf (bnf#80, pinned by parser 0.12.8's fleet), the published abnf() in all three runtimes emits the wrong loop rendering.
- railroad [major/all/high]: CLI grammar mode is a fixed table in the ports: Go resolves only json / @tabnas/json / tabnas/json / github.com/tabnas/json/go (go/cmd/tabnas-railroad/main.go:242) and Rust only json / @tabnas/json / tabnas/json / tabnas-json / tabnas_json (rs/src/cli.rs:223), where the TS CLI `require`s any module and honours a `#export` suffix (ts/src/bin/tabnas-railroad-cli.ts:43; AGENTS.md 'The CLI'). railroad's stated role is the dev tool every grammar repo uses to regenerate its diagrams, and only the TS CLI can do that for a grammar other than json. Render mode is fully general in all three.
- jsonic-cli [blocker/rs/high]: The Rust crate has never been published: tabnas-jsonic-cli is absent from crates.io, so the Rust port reaches no user and every release.yml run on main fails on its crates job. ADR-21 makes the first publish a manual maintainer step (scoped token, then trusted publisher).
- jsonic-cli [major/ts/high]: The canonical command exits 0 after a failure (parse error, missing --file, unresolvable -p), so a shell cannot script against it; both ports exit 1. ADR-13 repair direction is TypeScript; unrepaired on main and no issue is open for it.
- lsp [major/all/high]: Published 0.1.3 predates the Rust port and the latest pipeline fix: main is 46 commits ahead of ts/v0.1.3 = go/v0.1.3 (7e73b38, 2026-09-23), including the whole Rust port (PR #21, 2026-09-28: rs/ full pipeline + `tabnas-lsp-gen --runtime rust`) and PR #30 (TOML strings and alchemy `:name` keywords coloured; touched ts/src/core.js, go/semantic.go, rs/src/semantic.rs and the fixture). npm @tabnas/lsp 0.1.3 (gitHead 7e73b38) therefore ships no Rust generator target and the pre-#30 semantic tokens; go/v0.1.3 likewise. A 0.1.4 release is owed.
- render [blocker/go/high]: The Go port cannot be built or tested from published inputs: go/go.mod requires github.com/tabnas/transduce/go v0.1.0, a tag transduce does not have, and the committed go/go.sum has no transduce line at all, so the module only ever resolved through an uncommitted go.work over the sibling checkout (go/go.mod:5-7 says so). Fix is upstream (transduce must release go/v0.1.0) but render's main stays unbuildable against the proxy until then.
- render [major/all/high]: No CI gate exists for the TypeScript or Go port. The only workflow is rust.yml, path-filtered to rs/**, test/spec/**, ci/rust/**; the TS (f5f65f4) and Go (a17caaf) port merges produced no workflow run. The Makefile still builds and tests rs/ only. Both ports' parity with the 325 shared rows is therefore unverified by any gate.
- render [major/ts/high]: The TS port depends on @tabnas/transduce, which is on neither npm nor any tag: peerDependency ">=0" and devDependency "*" cannot resolve from the registry, so a clean `npm install && npm test` has no published input to run against. The TS suite was not run in this audit (npm install forbidden) and runs in no workflow, so TS parity rests on code reading only. engines.node ">=24" also exceeds this machine's node 22.
- render [major/all/high]: Nothing but the Rust crate is published, and it was published from a commit that predates both ports and the shared fixtures. The only tag is rs/v0.1.0 at cebb197 (PR #1 merge, 2026-09-27, Rust-only); crates.io has tabnas-render 0.1.0; there is no @tabnas/render on npm, no go/v tag, no GitHub Release, and no release.yml although the kind is TOOL (ADR-20 requires one). 0.1.0 is spent on crates.io, so the ports can only ship under a bump.
- transduce [blocker/go/high]: The Go port does not compile against the modules its own go.mod declares: go/lines.go:263 and go/harness_test.go:102 call tabnascsv.Make, which exists only from csv/go 0.6.0, while go.mod requires github.com/tabnas/csv/go v0.5.11. `go test ./...` at main fails at build.
- transduce [blocker/go/high]: The incremental source — the point of the crate — is compiled only with the build tag tabnas_nodecell. A default build has IncrementalGrammars() == [] and refuses every SourceMode Incremental and LinesSource.RunIncremental(jsonl) run with STREAMABILITY_UNKNOWN before the parse; 30 of 113 Go tests (incremental_test.go, incremental_source_test.go) and 152 of the 250 shared fixture rows do not run. The stated reason — 'a released Go engine does not have NodeCell/SetNode yet' — is no longer true: parser go/v0.12.8 ships go/nodecell.go; go.mod pins v0.12.7, which does not.
- transduce [major/go/high]: With the adapter built and engine 0.12.8, the published yaml/go v0.5.15 that go.mod requires fails two tests: a YAML key that is a mapping is refused ('opened a container inside a map before announcing the member's key') instead of streaming as the walk — the behaviour PR #20 pinned 'since tabnas/yaml#107'. yaml/go 0.5.16 (published) fixes it; go.mod does not require it.
- transduce [major/go/high]: Divergences are handled as a by-name skip list in the Go test, not an executable register (ADR-14), and the comment claims DIVERGENCE.md records them when it does not; both skips are now stale (the rows pass with yaml 0.5.16 and csv 0.6.0), so the skip list hides closed divergences instead of failing loudly.
- transduce [major/all/high]: No CI gates ts/ or go/: the only workflow is rust.yml, path-filtered to rs/**, test/**, ci/rust/**; the Go-port merge commit ran nothing but CodeQL; the Makefile runs only Rust. Two of three runtimes are unverified anywhere but a developer's machine, and the TypeScript port has never been shown green by any record on this machine either (npm install was out of scope).
- skills [major/rs/high]: The five skills describe the tabnas fleet as a two-runtime (TypeScript + Go) system and never mention Rust, rs/, cargo or crates.io, although every COMPONENT and three TOOLs now carry an rs/ crate and the fleet's `make test` runs the Rust suite. An agent following build-a-plugin or test-a-grammar scaffolds and verifies without the third runtime.
- skills [major/rs/high]: upgrade-a-plugin and build-a-plugin teach 'three version constants' (ts/package.json, ts/src VERSION, go VERSION). The fleet's release version is also declared in rs/Cargo.toml, rs/src/lib.rs and the generated rs/Cargo.lock; a bump done per the skill leaves rs/ stale and turns rust.yml red (ci/rust/run.sh runs --locked).
- skills [major/all/high]: upgrade-a-plugin tells the agent that DIVERGENCE.md 'is the single record of result differences'. Since ADR-14 a divergence must also be REGISTERED in test/spec/divergent.tsv (a column per runtime, asserted by every suite), and a gate requires every `### ` heading in DIVERGENCE.md to be a register group or a declared exemption; prose alone now fails that gate.
- alchemy [blocker/go/high]: The Go module cannot be built or tested against published modules: it requires github.com/tabnas/transduce/go v0.1.0 and github.com/tabnas/render/go v0.1.0, tags that do not exist; go.mod itself says they are 'not yet released, resolved through a go.work over the sibling checkouts until they are'.
- alchemy [major/all/high]: Neither port has any CI gate: .github/workflows holds only rust.yml, whose path filter excludes ts/ and go/, so the merge of the Go port (2435be8) ran no test workflow at all, and no run on GitHub has ever executed npm test or go test for this repo.
- alchemy [major/go/high]: `alchemy run` and the run.tsv fixture rows depend on transduce's `tabnas_nodecell` build tag; a default build refuses `run` with STREAMABILITY_UNKNOWN before reading the document, and TestSpecRun skips every row that reads a document, so the Go run path is unverified in a default `go test ./...`.
- alchemy [major/ts/high]: @tabnas/alchemy is unpublished and cannot be installed or tested from the registry: its peer/dev dependencies @tabnas/transduce and @tabnas/render are on npm nowhere (and engines.node >=24, typescript 7.x, @types/node 26.x).
- alchemy [major/all/high]: Every guide still describes a Rust-only repository, one day after both ports merged: README says 'Rust only for now' and its Layout table lists only rs/; the Makefile says 'TypeScript and Go ports are not started yet' and has no ts/go targets; test/AGENTS.md says 'Today the Rust crate is the only runtime' and 'TypeScript and Go: not yet'; AGENTS.md's Repository map and 'Verify your work' cover only rs/ and cargo.
- mcp [major/ts/medium]: Bundled contract data lags the engine: data/error-codes.json carries engine version 0.12.2 while the parser sibling is 0.12.8, and data/DIVERGENCE.md differs from the parser's by 90 lines. The staleness gate regenerates from the sibling and asserts byte equality, so `npm test` in the fleet layout fails today and CI (which clones parser at main) will fail on the next push to main. Published 0.1.16 was built against parser 0.12.2.

## Release-tag agreement for each repository's current version

Measured again on 2026-10-02 at 20:30 UTC against each repository's main. The version is read from `ts/package.json`, or `rs/Cargo.toml` where that has none. Each tag column gives the commit the `ts/v`, `go/v` or `rs/v` tag for that version names, the last column the `gitHead` npm records for it, and `-` means the tag or package does not exist. transduce, render and alchemy are published only as crates, so their other columns are empty. bnf 0.1.23 is on crates.io without an `rs/v` tag.

```
repo         version  ts-tag    go-tag    rs-tag    npm-gitHead note
parser       0.12.8   599e026   599e026   599e026   599e026    
json         0.5.11   edb661e   edb661e   edb661e   edb661e    
jsonl        0.1.10   d3e8ff8   d3e8ff8   d3e8ff8   d3e8ff8    
jsonic       0.7.2    58034df   58034df   58034df   58034df    
jsonc        0.5.8    b2f4317   b2f4317   b2f4317   b2f4317    
json5        0.5.9    468f484   468f484   468f484   468f484    
yaml         0.5.16   fd48f97   fd48f97   -         fd48f97     no rs tag;
toml         0.5.9    21affbf   21affbf   21affbf   21affbf    
ini          0.5.12   2b7f6c7   2b7f6c7   2b7f6c7   2b7f6c7    
csv          0.6.0    2ba7fd8   2ba7fd8   -         2ba7fd8     no rs tag;
xml          0.7.10   5cf2b86   5cf2b86   5cf2b86   5cf2b86    
zon          0.5.10   f39f20e   f39f20e   f39f20e   f39f20e    
markdown     0.7.6    3d36f54   3d36f54   3d36f54   3d36f54    
feed         0.6.10   2903ab8   2903ab8   2903ab8   2903ab8    
abnf         0.4.16   503bfaf   503bfaf   503bfaf   503bfaf    
bnf          0.1.23   ef1b6e0   ef1b6e0   -         ef1b6e0     no rs tag;
ebnf         0.1.10   b147997   b147997   b147997   b147997    
gbnf         0.1.13   07e2df4   07e2df4   07e2df4   07e2df4    
lsp          0.1.3    7e73b38   7e73b38   bf749e6   7e73b38     rs tag differs;
hoover       0.3.10   ed3417a   ed3417a   ed3417a   ed3417a    
css          0.5.9    482a557   482a557   482a557   482a557    
c            0.5.9    d7ad9b5   d7ad9b5   d7ad9b5   d7ad9b5    
proto        0.6.2    ffb89e1   ffb89e1   ffb89e1   ffb89e1    
semver       0.0.5    f4aed5d   f4aed5d   f4aed5d   f4aed5d    
chess        0.1.9    28f0548   28f0548   28f0548   28f0548    
expr         0.5.11   2d22719   2d22719   2d22719   2d22719    
directive    0.5.9    52f9619   52f9619   52f9619   52f9619    
path         0.3.9    acfa470   acfa470   acfa470   acfa470    
multisource  0.5.9    5f27caf   5f27caf   5f27caf   5f27caf    
debug        0.3.9    e47934b   e47934b   e47934b   e47934b    
railroad     0.3.8    75c3ac5   75c3ac5   75c3ac5   75c3ac5    
support      0.3.5    5424add   5424add   5424add   5424add    
jsonic-cli   0.5.9    da88285   da88285   -         da88285     no rs tag;
transduce    0.1.0    -         -         b81a495   -           no ts tag; no go tag; not on npm;
render       0.1.0    -         -         cebb197   -           no ts tag; no go tag; not on npm;
alchemy      0.1.1    -         -         32a83a7   -           no ts tag; no go tag; not on npm;
```
