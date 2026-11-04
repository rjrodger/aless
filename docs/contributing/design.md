# How aless is put together

The modules, the invariants a change keeps, and the facts the code
depends on that are too long for a comment. The decisions behind them
are in `ADR.md`.

## The terminal and the library

`src/main.rs` is the only file that touches the terminal (crossterm) and
the only one with platform-specific code. Everything in the library is
terminal-free and unit tested, so a test drives the whole application
without a screen (`tests/app_flow.rs`). Without a terminal the binary
refuses to start before it reads any input, grammar files included, so a
pipe left open or a FIFO nobody writes cannot hold a viewer that cannot
start.

| Module | What it holds |
|---|---|
| `cli` | every option, in one table; the parser that reads the command line from it; the reference `--help` prints, the summary `-h` prints, the man page and the shells' completions |
| `load` | format detection; the tabnas grammars; the grammar notations compiled to their specs; errors with positions; the size, depth and time limits; the plain-text fallback |
| `grammar` | custom grammars: `--grammar` and `--grammar-expr` read, ABNF compiled once, and the registry `Format::Custom` indexes |
| `doc` | the parsed value as a pre-order arena; the visible rows; paths; folding |
| `prov` | source positions, by aligning the token stream with the tree |
| `fmt` | the text of keys and values, previews, the JSON `:w` writes, the path forms |
| `search` | jless-style search patterns |
| `tab` | one open document: focus, scroll, mode, search, navigation, and the reload that keeps your place |
| `explorer` | directory trees as documents, listed as they are unfolded |
| `app` | the tabs, the modes, the source view, the key map and `KEY_MAP`, the command line, watch scheduling, copying |
| `highlight` | the source view's colour: tabnas-lsp's semantic tokens, made off the viewer's thread |
| `render` | the screen as ratatui widgets, every width measured as the terminal places text |
| `watch` | the notify file watcher |
| `clip` | the clipboard, and OSC 52 |

## Tests

Tests live next to the code (`#[cfg(test)]`) and in `tests/`:

| File | Holds |
|---|---|
| `formats.rs` | every fixture loads through its grammar, with positions on the right lines |
| `grammars.rs` | every grammar of the library to its sample's expected tree |
| `app_flow.rs` | the application run without a terminal against real files |
| `binary.rs` | the built binary: what it prints about itself, the generated files, its refusal without a terminal |
| `site.rs` | the documentation site, built and checked |
| `render_memory.rs` | drawing a long value costs a screenful, not the value |

Fixtures are under `tests/fixtures/`, one for each format at least. A
reload test writes to `std::env::temp_dir()`. A change to how something
is drawn updates its `render` test, and a change to navigation its `tab`
test. A new binding goes on a key jless leaves free.

## Grammars and depth

In the tabnas engine an alternate that hands control to another rule
either pushes a child rule (`p`), opening a frame for something the tree
nests, or replaces the current rule (`r`), re-entering it in the same
frame for the next item of a sequence. Every `*A`, `1*A` and `m*A`
compiles to a replace loop, so a file's length adds no depth. The cap of
open rules, `MAX_RULE_DEPTH`, is a nesting guard shared by the built-in
grammars and a custom one. The observable is the engine's rule depth,
which `load` reads as `ctx.rule_stack.len()`:
`a_custom_grammars_repetition_is_not_nesting` holds a long `hosts` file
to the depth a single line needs.

A custom grammar builds its value in the compiler's shape, an object and
a `kids` array a level, which the rule stack does not count in the same
units, so its nesting is measured on the value instead
(`MAX_VALUE_DEPTH`).

The compiler's own bounds, beyond those the reference lists: the emitter
writes a repetition out to 8,192 copies at most, and the left-recursion
pass adds at most 512 alternatives to one rule and 131,072 elements to a
grammar. A cycle of rules that start with one another meets the first of
those, and `a_grammar_whose_alternatives_multiply_opens_as_its_refusal`
opens it as the compiler's refusal.

`MAX_REPEAT` and `MAX_REPEAT_TOTAL` bound what a grammar's repetition
counts expand to. The compiler writes `min` copies of an element, then
one nested optional rule for each copy up to `max`, and a count too large
for an integer saturates rather than failing to parse. Measured in a
release build when the bounds were set: `10000000"a"` reached 5 GB,
`1*1000"a"` compiles in about a second and `1*5000"a"` in about 35;
1,000 rules compile in under half a second, and 3,000 add about 20
seconds to each parse. Real grammars stay in the hundreds: RFC 3986's URI
grammar writes 113 rules.

A panic inside the compiler, as inside a parse, is reported as a
`grammar` error through `load::catch_grammar` (`compile_within`).

`compile_notation` compiles ABNF with `marks` and `builtins` on and
recognition off, and EBNF and GBNF with `builtins` on; each result is
lowered with `tabnas_abnf::to_pure_spec`, so the spec matches what each
crate's TypeScript compiler writes.

## Formats

### C's tree

tabnas-c's reader repeats a node's subtrees under the members that name them, so the tree written out can grow faster than the file. The caps (`C_TREE_PER_BYTE`, `C_TREE_FLOOR`, `C_TREE_CEILING` for values; `C_TREE_BYTES_PER_BYTE`, `C_TREE_BYTES_FLOOR`, `C_TREE_BYTES_CEILING` for the bytes of names and strings) were set from tabnas-c's corpus of 100 Csmith programs, read with GCC extensions on: they hold 0.9 to 58 values a source byte (median 32), and 5.5 to 342 bytes of text a byte (median 197). The value cap is over four times the corpus maximum, the bytes cap about six times. The value ceiling is what a 64 MB array of single digits, the default `--max-size`, makes in another format; the bytes ceiling stops a long name repeated under deep member accesses, which stays under the value ceiling while its text runs to terabytes. The hint `Deep::Repeated` gives spells the numbers out, so a change to a cap edits it too.

### `.proto` files (`read_as`)

The descriptor's members follow the order the crate's other runtimes write: the lists every descriptor has, then what statements set, in statement order. A number the reader reads as NaN (a digit separator, `1_0`) stays NaN, which JSON writes as null. A file with no `syntax` line is proto2.

### Rule history (`make_parser`)

Every grammar keeps `RULE_HISTORY` rules of history unless it sets its own: CSS sets 1, and `get_or_insert` keeps it. `tabnas_proto::make()` sets the rewind depth its grammar needs, since it backtracks across whole statements.

## Positions

The engine reports each token with its place, and a parsed value carries
none; `prov` aligns the two, as the
[positions explanation](../explanation/positions.md) tells a reader.
Two shapes are aligned another way.

### Records keyed by a header row (`align_records`)

A CSV or TSV record carries no keys in the token stream: its keys are the header's cells, lexed once before any value. `capture_cells` keeps the `#CA` separators and `#LN` line ends; a line is the run of tokens between line ends, the first line with a token is the header, and each later one is a record, each field placed at the first token of the cell its column names. No key is looked for by name and no bracket at all, since tabnas-csv makes the root array and the records, so a lone `[` is a value. A line that does not spell its record (a cell past the header's columns, which tabnas-csv names `field~2`) is matched by value over that line's tokens alone. An empty or missing cell stays unplaced; a blank line makes no record, and a line of spaces is a record of its own.

### C's tree (`align_spans`)

Each token node carries its `span`, a `line` and a 1-based `col` in characters, so no alignment is needed. A token node with an empty `src` is the reader's placeholder for a token a path dropped, and takes no place.

## Numbers

A number keeps its source's spelling where the spelling is JSON's.
`fmt::spelling` decides, `prov::TokVal::Num` carries the spelling in a
thin box (so a token stays the size it is without one), and
`take_spelling` hands it to the node placed at the token only when the
node holds that very number, bit for bit. `Doc::spellings` holds them
by node, empty for most documents, and `fmt::value_text` is where every
written value goes: the row, the preview, search and the JSON `:w` and
the copy keys write. Under a best-effort alignment (YAML, XML, Markdown)
a misplaced match could show another spelling of the same float.

## Colour and drawing

### The source view's colour

The source view is coloured (`highlight`) from tabnas-lsp's semantic tokens, made off the viewer's thread. Colouring a text of `MAX_BYTES` peaks near 150 MB; 0.9 MB of JSON takes about 270 MB and a second in a release build. The bundled registry finds proto, css, c and expr under their package's name, and pgn and semver under their own `languageId`. On a line that ends in CRLF, `Painted::from_spans` stops the runs short of the CR.

### Widths (`render`)

`cols`, `clip` and `Line::fit_cols` walk grapheme clusters lazily and stop once they have their answer, so a value of millions of characters is measured and clipped without holding anything a character; `coloured` builds runs only for what shows. `tests/render_memory.rs` holds it. ratatui's buffer draws a zero-width cluster nowhere, which `walk` allows for.

### The explorer

`Explorer::fs_path` uses a path segment no listing knows as typed.

