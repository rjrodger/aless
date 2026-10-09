# Examples

Files to open with aless.

| File | What it is |
|---|---|
| [`bookshelf-openapi.yaml`](bookshelf-openapi.yaml) | An OpenAPI 3.0 definition of a small, made-up bookshelf API |
| [`books.json`](books.json) | Three made-up book records, for the guides to extract and convert |
| [`broken.json`](broken.json) | A JSON document with a trailing comma, for the guides to show an error |

```bash
aless examples/bookshelf-openapi.yaml
aless examples            # browse them in the explorer
```

The bookshelf API was written for this repository, and no service
serves it. It exercises quoted keys (`'200'`), `$ref` strings, `allOf`,
`>-` folded block scalars and a whitespace-only line, and
`tests/formats.rs` checks its structure and a source position. It is
checked out byte for byte on every platform (`-text` in
`.gitattributes`).

The documentation site publishes every file here but this one under
`/examples/` (`https://aless.tabnas.dev/examples/books.json`), and its
pages run their examples against them: `tests/site.rs` fails when a
page's output no longer matches what aless prints for these files.
