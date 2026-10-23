# Handover: the streaming plan's steps 10, 12 and 13 in Rust, and CSV's native route begun, 2026-10-22 and 2026-10-23

This session ran on the maintainer's machine on 2026-10-22 and
2026-10-23. It continued alchemy's streaming plan, which
[`docs/streaming-plan.md`](https://github.com/tabnas/alchemy/blob/main/docs/streaming-plan.md)
in tabnas/alchemy records step by step in its section 11. It began at
step 10's fourth item, the hosts reading a file through a reader, and
ended partway into CSV's native route.

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
6. **Releases wait for the next wave.** That means no transduce 0.3.0,
   aless 0.4.10 or alchemy-cli release yet.
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

**Released:** aless 0.4.9 and alchemy-cli 0.1.16.

**Unreleased on `main`**, held for the next wave (ruling 6):

- transduce: #62, #63 and #64, which make 0.3.0, a breaking release.
- aless: #91 and #92. `CHANGELOG.md` lists #91 under Unreleased.
- alchemy-cli: #56.
- render and alchemy: changes to docs and benches only.

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

A scratch probe wrote csv's part natively in Rust. It reads over
`ParserEventSource` from a file, pruned, into a table. Each figure is
the median of three interleaved runs:

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
- **The probe agreed with the walk on 12 of 13 documents.** The 13th
  has a header naming one column twice. The probe streams both cells, as
  csv's part does, and a host's tree contract refuses it.

## In progress: CSV's native route

### The design (rulings 9 and 10)

`transduce-records spec input` goes in alchemy's `stdlib/transduce.alc`.
It is two chained scans, like csv's part. The spec is a record of:

- `:document`, `:record` and `:cell`: rule names;
- `:empty`: a group;
- `:fill`: what an empty cell holds, and a header cell a record lacks;
- `:header`: whether the first record names the columns;
- `:unnamed`: the prefix of a name past the header's, followed by the
  cell's position.

csv's part becomes:

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

The Rust lowering recognises the library's two scans over the host's
parser events. It asks the routers for a native,
`Routers::transduce_records`, whose default declines, as it does for
`transduce-tree`.

The native must hold the header's names to `max_metadata_bytes`. It
uses the measure the library's scan state is held to: 16 bytes a node
plus each string's bytes. The state is `[phase names cells]`, so the
native and the library refuse at the same point.

### Where it stands

Sibling CI clones `main`, so each change lands only after the change it
needs.

1. **alchemy: code-complete, pushed, no pull request yet.** The work is on
   `claude/transduce-records` in
   `~/Projects/tabnas-worktrees/csv-records/alchemy`: seven commits, the
   last `1ed5a06`.
   - The library's sixteen definitions are in `stdlib/transduce.alc`.
     `node ts/embed-grammar.js` copied them into `rs/`, `go/` and `ts/`.
   - Their signatures are in all three checkers, and in the three
     runtimes' name lists (TypeScript's count is now 60).
   - `docs/language.md` has the prose, and an `alchemy`/`core` pair for
     each definition. `pipe.tsv` has a row for each definition, and
     `check.tsv` has the explain report and two refusals.
   - A header cell that isn't a string names its column as the csv
     grammar names it: `null`, `true` or `false`, and a number as
     `scalar-text` writes it.
   - Rust:
     - `rs/src/shared/records.rs` holds `RecordsSpec` and its contract
       pass for pass, the state's measure and refusal, and
       `records_name`.
     - `is_json_number`, `shortest_number` and `number_text` moved to
       `rs/src/shared/number.rs`, so transduce's native writes a number
       as `scalar-text` does. They are re-exported at their old paths.
     - `Routers::transduce_records` declines by default.
     - In `lower.rs`: `records_spec`, `native_records` (the library's two
       scans over the host's parser events), and the hook in `events()`.
       It holds the spec's state to `max_metadata_bytes` and is tried
       before `transduce-tree`.
     - Three tests: the spec read and refused; the native offered,
       declined and off; the library's scans by hand over csv-shaped
       passes.
   - Rust's lib and spec tests and clippy pass. TypeScript passes 548
     of 548, and Go passes.
   - Next: alchemy's full gate (`ci/rust/run.sh`), then the pull
     request. Its `downstream.yml` runs alchemy-cli's gates against it.
2. **transduce:** `TransduceRecords` in Rust, offered by
   `TransduceRouters` and held to the walk over csv's documents. The
   probe's code in
   `~/Projects/tabnas-worktrees/csv-native/transduce/rs/examples/csv_native_probe.rs`
   is a start.
3. **csv:**
   - Tag `elem`'s two empty-cell alternates with the group `empty`, in
     both rule sets (strict and relaxed), in `rs/src/lib.rs`,
     `ts/src/csv.ts` and `go/csv.go`.
   - Rewrite `alchemy/transduce.alc` as the spec; `npm run embed` in
     `ts/` copies it.
   - The part needs alchemy's `transduce-records`, so alchemy is
     released first.
4. **alchemy-cli:** a differential test holding the native to the
   library over csv's corpus, as `transduce_tree_native_and_interpreted_agree`
   does.
5. **The release wave, then aless.**
   - aless reads CSV and TSV through the part.
   - Step 12 then finishes for CSV:
     - transduce retires `LinesSource` and `LineFormat`; TypeScript and
       Go keep theirs.
     - aless drops `Plan`, `line_format`, and the line source's part in
       the `--max-size` exemptions.
6. **alchemy's plan record** for rulings 7 to 10 and the measurements
   above.

## Carried over

- **The releases** that ruling 6 holds.
- **TypeScript and Go:**
  - Item 7 of step 10 comes first: their engines read a reader a window
    at a time. Which goes first is not yet asked. Their steps 12 and 13
    follow.
  - transduce's shared fixtures keep the incremental rows for them, and
    the Rust runner skips them.
  - Step 12 dropped the JSON Lines line-source rows that the ports
    still run. Putting them back is offered and not yet done.
- **Item 8 of step 10:** the fleet's other plugins declare `lex.window`,
  yaml last.
- **Lighter passes:** parser#258 (open: "Reduce the per-rule cost:
  allocation and cloning per step"). The profile above is its latest
  measurement, and that work narrows every part's cost.
- **From parser's earlier page (2026-10-09):**
  - aless#29 (housekeeping) is still open.
  - Not checked since that page:
    - two engines from `tabnas_json::make()` cannot be merged in Rust;
    - chess-view is released by a `web/v*` tag that only the maintainer
      can push;
    - the engine's loose TypeScript types.
  - Its aless 0.1.0 release steps were done long ago.

## Local state, on the maintainer's machine

- **`~/Projects/tabnas-worktrees/csv-records/`** holds the work in
  progress. alchemy, csv, alchemy-cli and transduce are real worktrees,
  each on `claude/transduce-records` from `main`. The rest are symlinks
  to the main checkouts.
- **`~/Projects/tabnas-worktrees/csv-native/`** holds the probe:
  transduce on `claude/csv-native-probe`, its example uncommitted.
  Remove it once the native lands. Its inputs came from transduce's
  `tests/support` `records_csv`; `csv_native_probe gen ROWS FILE` makes
  them again.
- **`~/Projects/tabnas-worktrees/evaluator-tune/`** holds the interning
  experiment: one commit on `claude/tune-rule-record`, never pushed.
- **`~/Projects/tabnas-worktrees/fixes/`** is not this session's and was
  left alone.
- **`~/Projects/tabnas-worktrees/handover/`** holds this page's branch.
- **Every other set was removed** on 2026-10-23, once its pull requests
  had merged (ruling 7).

## Working method that held up

- **A port still runs a row that Rust's code has retired.**
  transduce's shared fixtures run in three harnesses: Rust, TypeScript
  and Go. A change that retires Rust code keeps the rows the ports still
  run, and the Rust runner skips them by mode. A new mode goes into all
  three harnesses at once. Go's run is also how step 13 found that a
  part's source holds each token's source text, quotes included, to
  `max_scalar_bytes`.
- **Measure a renamed or retired bench group against `main` on the
  same machine.** Run `main`'s groups in a detached worktree. BENCH's
  2026-09-27 numbers came from another machine.
- **`RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`** catches stale
  intra-doc links that the gates miss.
- **Check that a removed worktree is merged.** Before removing one,
  check that it is clean and that its `HEAD` is its merged pull
  request's head. Then use `git worktree remove` from its main checkout.
