# How aless is put together

This page lists the modules, the invariants a change keeps, and the
facts the code depends on that are too long for a comment. The
decisions behind them are in `ADR.md`.

## The terminal and the library

`src/main.rs` is the only file that touches the terminal. It reaches
the terminal through crossterm. `src/main.rs` is also the only file
with platform-specific code. Everything in the library is
terminal-free, so a test drives the whole application without a
screen, as `tests/app_flow.rs` does. Everything in the library is also
unit tested.

Without a terminal, the binary refuses to start before it reads any
input, so a pipe left open, or a FIFO nobody writes, cannot hold a
viewer that cannot start. Grammar files count as input.
`--print-json` is the one exception (ADR-007): `cli::print_json` reads
its file and writes the JSON `:w` writes, and `src/main.rs` prints that
JSON without looking for a terminal.

The modules are these:

| Module | What it holds |
|---|---|
| `cli` | Every option, in one table. The parser that reads the command line from that table. The reference `--help` prints, the summary `-h` prints, the man page, and the shells' completions. |
| `load` | Format detection. The tabnas grammars. The grammar notations compiled to their specs. Errors with positions. The size, depth, and time limits. The plain-text fallback. |
| `grammar` | Custom grammars: the reading of `--grammar` and `--grammar-expr`, the ABNF compiled once, and the registry that `Format::Custom` indexes. |
| `doc` | The parsed value as a pre-order arena. The visible rows. Paths. Folding. |
| `prov` | Source positions. The module finds them by aligning the token stream with the tree. |
| `fmt` | The text of keys and values, previews, the JSON `:w` writes, and the path forms. |
| `search` | jless-style search patterns. |
| `tab` | One open document: its focus, scroll, mode, search, navigation, and the reload that keeps your place. |
| `explorer` | Directory trees as documents. The module lists each directory as it is unfolded. |
| `app` | The tabs, the modes, the source view, the key map and `KEY_MAP`, the command line, watch scheduling, and copying. |
| `highlight` | The source view's colour, from tabnas-lsp's semantic tokens. The module makes those tokens off the viewer's thread. |
| `render` | The screen as ratatui widgets. Every width, measured as the terminal places text. |
| `watch` | The notify file watcher. |
| `clip` | The clipboard, and OSC 52. |

## Tests

Tests live in two places: next to the code under `#[cfg(test)]`, and in
`tests/`. The files under `tests/` are these:

| File | What it holds |
|---|---|
| `formats.rs` | Every fixture loads through its grammar. Each fixture's positions fall on the right lines. |
| `grammars.rs` | Every grammar of the library, held to its sample's expected tree. |
| `app_flow.rs` | The application, run without a terminal against real files. |
| `binary.rs` | The built binary: what it prints about itself, the generated files, and its refusal without a terminal. |
| `site.rs` | The documentation site, built and checked. |
| `render_memory.rs` | Drawing a long value costs a screenful, not the value. |

Fixtures are under `tests/fixtures/`. Every format has at least one
fixture. A reload test writes to `std::env::temp_dir()`. A change to
how something is drawn updates its `render` test, and a change to
navigation updates its `tab` test. A new binding goes on a key jless
leaves free.

## Grammars and depth

In the tabnas engine, an alternate that hands control to another rule
either pushes a child rule or replaces the current rule. A push (`p`)
opens a frame for something the tree nests. A replace (`r`) re-enters
the current rule in the same frame, for the next item of a sequence.
Every `*A`, `1*A`, and `m*A` compiles to a replace loop, so a file's
length adds no depth. `MAX_RULE_DEPTH` caps the open rules, as a
nesting guard. The built-in grammars and a custom one share that guard.
The observable is the engine's rule depth: `load` reads that depth as
`ctx.rule_stack.len()`. The test
`a_custom_grammars_repetition_is_not_nesting` holds a long `hosts` file
to the depth a single line needs.

A custom grammar builds its value in the compiler's shape: an object
and a `kids` array for each level. The rule stack does not count that
shape in the same units, so the nesting of a custom grammar's value is
measured on the value instead. `MAX_VALUE_DEPTH` caps that nesting.

The compiler has bounds of its own, beyond those the reference lists.
The emitter writes a repetition out to at most 8,192 copies. The
left-recursion pass adds at most 512 alternatives to one rule, and at
most 131,072 elements to a grammar. A cycle of rules that start with one
another meets the bound of 512 alternatives. The test
`a_grammar_whose_alternatives_multiply_opens_as_its_refusal` opens such
a cycle as the compiler's refusal.

`MAX_REPEAT` and `MAX_REPEAT_TOTAL` bound what a grammar's repetition
counts expand to. The compiler writes `min` copies of an element, then
one nested optional rule for each copy up to `max`. A count too large
for an integer saturates rather than failing to parse.

The figures that follow were measured in a release build when the
bounds were set:

- The repetition `10000000"a"` reached 5 GB.
- The repetition `1*1000"a"` compiles in about a second, and
  `1*5000"a"` in about 35 seconds.
- A grammar of 1,000 rules compiles in under half a second.
- A grammar of 3,000 rules adds about 20 seconds to each parse.

Grammars in use stay in the hundreds of rules: RFC 3986's URI grammar
writes 113 rules.

`compile_within` reports a panic inside the compiler as a `grammar`
error, through `load::catch_grammar`. A panic inside a parse is reported
the same way.

`compile_notation` compiles ABNF with `marks` and `builtins` on and
recognition off. It compiles EBNF and GBNF with `builtins` on. It lowers
each result with `tabnas_abnf::to_pure_spec`, so the spec matches what
each crate's TypeScript compiler writes.

## Formats

### C's tree

tabnas-c's reader repeats a node's subtrees under the members that name
them, so the tree written out can grow faster than the file. The caps on
that tree were set from tabnas-c's corpus of 100 Csmith programs. The
corpus was read with GCC extensions on. For each cap, the table gives
its constants, the range and the median the corpus holds for each source
byte, and how far the cap sits over the corpus maximum:

| Cap | Constants | Corpus range for each source byte | Median | Cap over the corpus maximum |
|---|---|---|---|---|
| Values | `C_TREE_PER_BYTE`, `C_TREE_FLOOR`, `C_TREE_CEILING` | 0.9 to 58 values | 32 | Over four times |
| Bytes of names and strings | `C_TREE_BYTES_PER_BYTE`, `C_TREE_BYTES_FLOOR`, `C_TREE_BYTES_CEILING` | 5.5 to 342 bytes | 197 | About six times |

The value ceiling is what a 64 MB array of single digits makes in
another format. That size, 64 MB, is the default `--max-size`. The
bytes ceiling stops a long name repeated under deep member accesses,
because such a tree stays under the value ceiling while its text runs
to terabytes. The hint that `Deep::Repeated` gives spells the numbers
out, so a change to a cap edits the hint too.

### `.proto` files (`read_as`)

The descriptor's members follow the order the crate's other runtimes
write. First come the lists that every descriptor has, then what
statements set, in statement order. A number that the reader reads as
NaN stays NaN, so JSON writes that number as `null`. The reader reads a
number with a digit separator, such as `1_0`, as NaN. A file with no
`syntax` line is proto2.

### Rule history (`make_parser`)

Every grammar keeps `RULE_HISTORY` rules of history unless it sets its
own. CSS sets one rule of history. `get_or_insert` keeps that setting.
`tabnas_proto::make()` sets the rewind depth its grammar needs, because
that grammar backtracks across whole statements.

## Positions

The engine reports each token with its place, but a parsed value
carries no place. `prov` aligns the two, as
[Where each value came from](../explanation/positions.md) tells a
reader. `prov` aligns two shapes another way.

### Records keyed by a header row (`align_records`)

A CSV or TSV record carries no keys in the token stream, because its
keys are the header's cells. The header is lexed once, before any
value. `capture_cells` keeps the `#CA` separators and the `#LN` line
ends. A line is the run of tokens between line ends. The first line with
a token is the header, and each later line is a record. The alignment
places each field at the first token of the cell its column names. The
alignment looks for no key by name, and for no bracket at all, because
tabnas-csv makes the root array and the records. A lone `[` is
therefore a value.

For a line that does not spell its record, the alignment matches by
value over that line's tokens alone. A line with a cell past the
header's columns is such a line. tabnas-csv names that cell `field~2`.

An empty or missing cell stays unplaced. A blank line makes no record,
and a line of spaces is a record of its own.

### C's tree (`align_spans`)

Each token node carries its `span`, so no alignment is needed. A `span`
holds a `line` and a `col`. The `col` is 1-based and counted in
characters. A token node with an empty `src` is the tabnas-c reader's
placeholder for a token a path dropped. Such a node takes no place.

## Numbers

A number keeps its source's spelling where the spelling is JSON's.
`fmt::spelling` decides whether a number keeps its spelling.
`prov::TokVal::Num` carries the spelling in a thin box, so a token
stays the size it is without a spelling. `take_spelling` hands the
spelling to the node placed at the token, but only when the node holds
that same number, bit for bit. `Doc::spellings` holds the spellings by
node. For most documents, `Doc::spellings` is empty.
`fmt::value_text` is where every written value goes: the row, the
preview, search, and the JSON that `:w` and the copy commands write. The
alignment of YAML, XML, and Markdown is a best effort. Under such an
alignment, a misplaced match could show another spelling of the same
float.

## Colour and drawing

### The source view's colour

`highlight` colours the source view from tabnas-lsp's semantic tokens.
`highlight` makes the tokens off the viewer's thread. Colouring a text of
`MAX_BYTES` peaks near 150 MB. In a release build, colouring 0.9 MB
of JSON takes about 270 MB and a second. The bundled registry finds
`proto`, `css`, `c`, and `expr` under their package's name, and `pgn`
and `semver` under their own `languageId`. On a line that ends in CRLF,
`Painted::from_spans` stops the runs short of the CR.

### Widths (`render`)

`cols`, `clip`, and `Line::fit_cols` walk grapheme clusters lazily, and
stop once they have their answer. These functions therefore measure and
clip a value of millions of characters without holding anything for
each character. `coloured` builds runs only for what shows.
`tests/render_memory.rs` holds that behaviour. ratatui's buffer draws a
zero-width cluster nowhere, so `walk` allows for a zero-width cluster.

### The explorer

When no listing contains a path segment, `Explorer::fs_path` uses that
segment as it was typed.
