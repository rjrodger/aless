# Examples

Files to open with aless. The documentation site publishes each of them
under `/examples/`, and its pages open them.

| File | What it is |
|---|---|
| [`bookshelf-openapi.yaml`](bookshelf-openapi.yaml) | An OpenAPI 3.0 description of a small, made-up bookshelf API, which the tutorial walks through |
| [`books.json`](books.json) | Three made-up book records, for the guides to search, copy from and break |
| [`broken.json`](broken.json) | A JSON document with a comma too many, for the guide to a file that does not parse |

```sh
aless examples/bookshelf-openapi.yaml
aless examples            # browse them in the explorer
```

The bookshelf API was written for this repository, and no service
serves it. It holds quoted keys (`'200'`), `$ref` strings, `allOf`,
`>-` folded block scalars and a line of only spaces, and
`tests/formats.rs` checks its structure and a source position. It is
checked out byte for byte on every platform (`-text` in
`.gitattributes`).

Every capture of the viewer on the site's pages is drawn by
`tests/site.rs` from these files, with the sample hosts file and its
grammar from `tests/fixtures/grammars/` beside them, and the test fails
when aless no longer draws what a page shows
(`docs/contributing/site.md`).
