# Handover: the streaming plan's steps 10, 12 and 13 in Rust, CSV's native route, and the records wave, 2026-10-22 and 2026-10-23

This session ran on the maintainer's machine on 2026-10-22 and
2026-10-23. It continued alchemy's streaming plan, which
[`docs/streaming-plan.md`](https://github.com/tabnas/alchemy/blob/main/docs/streaming-plan.md)
in tabnas/alchemy records step by step in its section 11. It began at
step 10's fourth item, the hosts reading a file through a reader. It
built CSV's native route and is now releasing it, the records wave.

aless is the host the plan serves, so its handover is kept here. The
fleet's earlier page, parser's
[`doc/handover/README.md`](https://github.com/tabnas/parser/blob/be46d32f76d6c52b641c80167d94267b5c96da05/doc/handover/README.md),
was last written on 2026-10-09. tabnas/parser#318 (`2d64381`) removed
it on the maintainer's instruction. What was still open on it is under
"Carried over".

## The maintainer's rulings

On 2026-10-22:

1. **CSV: profile first.** Profile csv's transducer part under alchemy's
   evaluator, tune it, measure it again, then decide.
2. **Prune, then release.** alchemy-cli's `translate` and
   `run --grammar` prune the engine's tree as aless's export does. Then
   aless 0.4.9 and alchemy-cli 0.1.16 are released by workflow.

On 2026-10-23:

3. **Step 12: JSON Lines now, CSV later.** JSON Lines leaves the line
   source for its part. CSV keeps the line source until its part has a
   native route, which is a step of its own.
4. **A quiet writer: accept it, and document it.** Take JSON Lines from
   a writer to standard input that goes quiet past `--timeout`. The
   records in the window not yet parsed are not written.
5. **Step 13 in transduce.** Rust retires `capability::INCREMENTAL` and
   the rule-event inference. `ParserSource` keeps its walk, and aless's
   `Plan` stays while CSV keeps its line source.
6. **Releases wait for the next wave.** Ruling 11 replaced this one.
7. **Remove only the merged worktree sets.** This is done; "Local state"
   says what is left.
8. **Next: CSV's native route**, measured before it is built.
9. **CSV's route is `transduce-records`.** It is a generic library
   function in alchemy that transduce runs natively in Rust, as
   `transduce-tree` is run, and csv's part becomes its spec.
   - aless reads CSV and TSV through it, and step 12 then finishes for
     CSV.
   - TypeScript and Go run the library's alchemy until they have
     natives of their own.
10. **csv tags its empty cells.** csv's grammar tags its two empty-cell
    alternates with the group `empty`, in Rust, TypeScript and Go. Then
    the spec need not match an alternate's fields.
11. **The release wave now.** Once the open pull requests merge, csv,
    alchemy, transduce 0.3.0 (breaking: steps 12 and 13) and alchemy-cli
    are released by workflow. Then aless reads CSV and TSV through the
    part and drops `Plan` and `line_format`, and aless 0.4.10 is
    released.
12. **transduce's grammars are dev-dependencies.** tabnas-json and
    tabnas-csv move to `[dev-dependencies]` in transduce's Rust crate.

## What landed

Every pull request below is merged.

| Step | Pull request | Commit | What |
|---|---|---|---|
| 10 | rjrodger/aless#89 | `506710f` | A JSON or JSONC file exported through its part is read a window at a time. |
| 10 | tabnas/alchemy-cli#53 | `1f427f8` | `run`, `run --grammar` and `translate` read a file through a reader. |
| 10 | tabnas/alchemy-cli#54 | `fd8803d` | Every route through a part prunes the engine's tree (ruling 2). |
| 10 | tabnas/transduce#61 | `a751bc9` | `docs/BENCH.md` adds "From a reader". |
| 10 | tabnas/alchemy#102 | `0f788b3` | The plan's record of step 10. |
| release | rjrodger/aless#90 | `f85a482` | aless 0.4.9; crates.io, the Homebrew tap and the GitHub Release were checked. |
| release | tabnas/alchemy-cli#55 | `6982256` | alchemy-cli 0.1.16. |
| 12 | rjrodger/aless#91 | `9c17786` | JSON Lines is read through tabnas-jsonl's part, a window at a time. |
| 12 | tabnas/transduce#62 | `5c5fe2d` | `LinesSource` reads CSV alone; `LineFormat::Jsonl` is gone (breaking). |
| 12 | tabnas/render#25 | `5fb900a` | The end-to-end bench reads JSON through its part. |
| 12 | tabnas/alchemy#103 | `bc975e8` | The CSV profile, and step 12's record. |
| 13 | tabnas/alchemy-cli#56 | `ad5aefb` | `ParserSource` is no longer told its grammar's name. |
| 13 | tabnas/transduce#63 | `629e4a7` | The inference is retired in Rust (breaking: `SourceMode`, `Prune`, `ParserSource::grammar`, `mode`, `unverified`). |
| 13 | rjrodger/aless#92 | `0aadf25` | The matrix's last `.grammar` call goes, with the comments on the inference. |
| 13 | tabnas/render#26 | `91ca069` | The renderers are measured again, through JSON's part. |
| 13 | tabnas/transduce#64 | `a408064` | Three broken rustdoc links are fixed; `cargo doc` now runs clean with warnings denied. |
| 13 | tabnas/alchemy#104 | `76a3282` | Step 13's record. |
| records | tabnas/alchemy#105 | `3b90e29` | The library's `transduce-records` in all three runtimes. Rust's lowering hands it to `Routers::transduce_records`. |
| records | tabnas/csv#108 | `a024e7a` | csv's part is the spec, and the grammar tags its empty cells `empty` (ruling 10). |
| records | tabnas/transduce#65 | `d935469` | `TransduceRecords`, the native. 198 of 199 documents stream as the walk. |
| records | tabnas/alchemy#106 | `9f5bce0` | `transduce.tsv` rows for csv's part as csv ships it. |
| records | tabnas/alchemy-cli#57 | `75d0108` | The native and the library agree on 848 documents, and refuse at the same bounds. |
| 12 | tabnas/transduce#66 | `6a43c75` | `LinesSource`, `LineFormat` and `DEFAULT_CHUNK_BYTES` go (breaking). |
| docs | tabnas/parser#318 | `2d64381` | parser's old handover page goes. |

**Released:** aless 0.4.9 and alchemy-cli 0.1.16.

**Unreleased on `main`**, now going out in the records wave (ruling 11):

- csv: #108, as 0.6.9.
- alchemy: #105 and #106, as 0.2.15.
- transduce: #62 to #66, and #67 once merged, as 0.3.0, a breaking
  release.
- alchemy-cli: #56 and #57.
- aless: #91 and #92. `CHANGELOG.md` lists #91 under Unreleased.
- render: changes to docs and benches only, so it is not released.

## Measurements

### From a reader

From transduce's `docs/BENCH.md`, aless `--render csv`, 2026-10-22:

| Input | Route | Time | Peak memory |
|---|---|---|---|
| JSON, 21.6 MB | read whole | 23.7 s | 136 MiB |
| JSON, 21.6 MB | from a reader | 23.6 s | 12 MiB |
| JSON Lines, 21.6 MB | `LinesSource` | 19.4 s | 11 MiB |
| JSON Lines, 21.6 MB | its part, from a reader | 23.2 s | 12 MiB |
| CSV, 26.9 MB | `LinesSource` | 27.1 s | 17 MiB |
| CSV, 26.9 MB | its interpreted part | 112.3 s | 12 MiB |

### CSV's part under alchemy's evaluator

For 100,000 rows, through alchemy-cli's `run --grammar csv`, release
build:

| What runs | Time |
|---|---|
| `LinesSource` | about 3.9 s |
| the native `transduce-tree` floor | 5.5 s |
| an empty alchemy step | 11.2 s |
| csv's part | 16.7 s |

- **Interning the evaluator's names saved 3% to 4%.**
- **The part needs two scans.** The language can join two vectors but
  cannot flatten one, and a record's missing cells come out as a vector
  of pairs.

### Step 13

From transduce's `cargo bench`, 20,000 records, a two-vCPU Neoverse-N1:

| What runs | Time |
|---|---|
| the bare parse | 1.22 s |
| JSON's part | 1.90 s |
| text to table rows through the part | 1.87 s |
| text to table rows through the retired adapter, same machine | 1.58 s |

The part is about a fifth slower than the adapter it replaced. The hosts
have paid that since step 9.

### A native route for CSV

transduce's `docs/BENCH.md` ("Records, natively") has these. A probe
wrote csv's part natively in Rust, the state machine `TransduceRecords`
now is. It reads over `ParserEventSource` from a file, pruned, into a
table. Each figure is the median of three interleaved runs:

| Route | 100,000 rows | 700,000 rows (26.9 MB) | Peak |
|---|---|---|---|
| `LinesSource` | 3.93 s | 27.8 s | 12.3 MB |
| native records | 5.25 s | 36.9 s | 6.8 MB |
| `transduce-tree` floor | 5.35 s | 37.6 s | 6.8 MB |

- **The native route costs a third more than the line source** in time
  and holds half its memory. csv's interpreted part takes four times as
  long.
- **The extra third is the source's pass building, not CSV's logic.**
  At 100,000 rows, counting instructions:

  | What runs | Time | Instructions |
  |---|---|---|
  | the line source | 3.93 s | 24.0 G |
  | the native route | 5.25 s | 32.6 G |
  | the engine alone | 3.75 s | 22.7 G |
  | `ParserEventSource` into a sink that keeps nothing | 5.00 s | 31.5 G |

  In a profile, the engine's notification and transduce's pass building
  take 23% of the run, and allocation about 29%.
- **A header that names a column twice streams both cells**, as csv's
  part does, where the walk keeps the last. A host's tree contract
  refuses it. It is the one document of 199 where the native and the
  walk differ.

## CSV's native route (rulings 9 and 10)

`transduce-records spec input` is in alchemy's `stdlib/transduce.alc`.
It is two chained scans, like csv's old part. The spec is a record of:

- `:document`, `:record` and `:cell`: rule names;
- `:empty`: a group;
- `:fill`: what an empty cell holds, and a header cell a record lacks;
- `:header`: whether the first record names the columns;
- `:unnamed`: the prefix of a name past the header's, followed by the
  cell's position.

csv's part is now:

```
def csv-transduce [input]
  transduce-records
    record
      entry :document "csv"
      entry :record "record"
      entry :cell "val"
      entry :empty "empty"
      entry :header true
      entry :unnamed "field~"
    input
```

- **The Rust lowering recognises the library's two scans** over the
  host's parser events. It asks the routers for a native,
  `Routers::transduce_records`, whose default declines, as it does for
  `transduce-tree`. transduce's `TransduceRouters` offers
  `TransduceRecords`.
- **The native holds the header's names to `max_metadata_bytes`.** It
  uses the measure the library's scan state is held to: 16 bytes a node
  plus each string's bytes. The state is `[phase names cells]`, so the
  native and the library refuse at the same point. alchemy-cli's test
  checks every bound from 8 to 220.
- **A header cell that isn't a string** names its column as the csv
  grammar names it.
- **`shared::records`** (in alchemy) holds `RecordsSpec`, the state's
  measure and refusal, and `records_name`. `shared::number` holds
  `number_text` and its kin, so the native writes a number as
  `scalar-text` does.

## In progress: the records wave, then aless

Ruling 11 is being carried out. The wave's order comes from admin's
`node tasks/dep-order.js --edges`: csv and alchemy, then transduce
(which needs both), then alchemy-cli. render needs no release, because
its only runtime dependency is alchemy's shared types and 0.2.15 is
compatible.

1. **transduce: tabnas/transduce#67, open** (ruling 12). The branch is
   `claude/grammars-as-dev-deps` in
   `~/Projects/tabnas-worktrees/csv-records/transduce`.
   - tabnas-json and tabnas-csv move to `[dev-dependencies]`.
   - Its first CI run failed the fleet's port-deps test (ADR-24),
     because the ports now differ. TypeScript and Go still parse each
     line with those grammars in their `LinesSource`.
   - `d78a962` adds `tools/port-deps.json`, which records both
     differences with their reason and their repair: step 12 in the
     ports. CI is running again.
   - After it merges, admin's `docs/deps` may need
     `node tasks/port-deps.js --write --root ~/Projects/tabnas`.
2. **csv 0.6.9: tabnas/csv#109, open.** It was bumped with publish.sh's
   step 2, and no dependency moves.
3. **alchemy 0.2.15: tabnas/alchemy#107, open.** Go's requires move to
   json v0.5.19 and parser v0.12.15, and `go test ./...` passes on
   them.
4. **Then dispatch both:**
   `gh workflow run release.yml -R tabnas/<repo> --ref main -f go=true`.
   Check npm, the Go proxy, crates.io and the `go/vX` Release.
5. **transduce 0.3.0**, once csv and alchemy are published: bump with
   step 2 (it moves Go's requires), then a pull request, then dispatch.
6. **alchemy-cli 0.1.17**, the same way.
7. **aless:**
   - CSV and TSV go through csv's part. Each joins all three part
     lists: `translate::transducer`, `export::shipped` and
     `export::transducer`.
   - `Plan`, `line_format`, the use of `LinesSource`, the alarm's
     `lines` flag and the `--max-size` exemption's wording for the line
     source all go. `streams_as_read` stays.
   - Tests and docs follow. Then the release: a bump pull request,
     `release.yml` with `tag=dry-run`, then `tag=v0.4.10`.
   - Before release, probe a CSV with a repeated header name through
     `--render csv` beside `--json`. The part streams both cells, so the
     tree contract must refuse it, as it does for jsonic.
8. **alchemy's plan record** for rulings 8 to 12 and the measurements
   above is on `claude/plan-csv-records` (`18c6d3c`), in
   `~/Projects/tabnas-worktrees/csv-records/alchemy-plan`. It needs
   #106, #57, #66 and the wave added before its pull request opens.

## Carried over

- **TypeScript and Go:**
  - Item 7 of step 10 comes first: their engines read a reader a window
    at a time. Which goes first is not yet asked. Their steps 12 and 13
    follow, with natives of their own for `transduce-records`.
  - Their step 12 retires their `LinesSource`. Then csv and json leave
    their runtime dependencies, and transduce's two port-deps entries
    go.
  - transduce's shared fixtures keep the incremental rows for them, and
    the Rust runner skips them.
  - Step 12 dropped the JSON Lines line-source rows that the ports
    still run. Putting them back is offered and not yet done.
- **Item 8 of step 10:** the fleet's other plugins declare `lex.window`,
  yaml last.
- **Lighter passes:** parser#258 (open: "Reduce the per-rule cost:
  allocation and cloning per step"). The profile above is its latest
  measurement, and that work narrows every part's cost.
- **admin's citations of parser's removed page.** Pinning them to
  `be46d32` is offered and not yet done.
- **From parser's earlier page (2026-10-09):**
  - aless#29 (housekeeping) is still open.
  - Not checked since that page:
    - two engines from `tabnas_json::make()` cannot be merged in Rust;
    - chess-view is released by a `web/v*` tag that only the maintainer
      can push;
    - the engine's loose TypeScript types.
  - Its aless 0.1.0 release steps were done long ago.

## Local state, on the maintainer's machine

- **`~/Projects/tabnas-worktrees/release-records/`** holds the wave.
  csv and alchemy are worktrees on their release branches; the rest are
  symlinks to the main checkouts.
- **`~/Projects/tabnas-worktrees/csv-records/`** holds the route's
  worktrees. The ones for #105 to #108, #57 and #66 are merged, and
  transduce is on #67's branch. alchemy-plan holds the plan record.
- **`~/Projects/tabnas-worktrees/csv-native/`** holds the probe:
  transduce on `claude/csv-native-probe`, its example uncommitted. The
  native has landed, so it can go once the wave is out. Its inputs came
  from transduce's `tests/support` `records_csv`;
  `csv_native_probe gen ROWS FILE` makes them again.
- **`~/Projects/tabnas-worktrees/evaluator-tune/`** holds the interning
  experiment: one commit on `claude/tune-rule-record`, never pushed.
- **`~/Projects/tabnas-worktrees/fixes/`** is not this session's and was
  left alone.
- **`~/Projects/tabnas-worktrees/handover/`** holds this page's branch.

## Working method that held up

- **A port still runs a row that Rust's code has retired.**
  transduce's shared fixtures run in three harnesses: Rust, TypeScript
  and Go. A change that retires Rust code keeps the rows the ports still
  run, and the Rust runner skips them by mode. A new mode goes into all
  three harnesses at once. Go's run is also how step 13 found that a
  part's source holds each token's source text, quotes included, to
  `max_scalar_bytes`.
- **A change across repositories is tested together, and lands in
  order.** alchemy's `downstream.yml` clones each sibling at the pull
  request's branch name when that repository has one, but sibling CI
  clones `main`. So a fixture that must pass before and after a sibling
  merges is written to hold in both states. alchemy#105's row 38 reads
  the trailing cell by `comma` being among its groups, which holds with
  and without csv's `empty` tag.
- **Moving one port's dependency fails ADR-24's port-deps test** until
  `tools/port-deps.json` records the difference, with its reason and
  its repair.
- **Give a squash merge its subject.** csv#108 had one commit, and its
  squash took that commit's "WIP:" headline, not the pull request's
  title. Pass `--subject` and `--body-file` to `gh pr merge --squash`.
- **Measure a renamed or retired bench group against `main` on the
  same machine.** Run `main`'s groups in a detached worktree. BENCH's
  2026-09-27 numbers came from another machine.
- **`RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`** catches stale
  intra-doc links that the gates miss.
- **Check that a removed worktree is merged.** Before removing one,
  check that it is clean and that its `HEAD` is its merged pull
  request's head. Then use `git worktree remove` from its main checkout.
