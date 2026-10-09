# Examples

Files to open with aless.

| File | What it is |
|---|---|
| [`bookshelf-openapi.yaml`](bookshelf-openapi.yaml) | An OpenAPI 3.0 definition of a small, made-up bookshelf API |

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
