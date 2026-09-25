# Examples

Files to open with aless.

| File | What it is |
|---|---|
| [`solardemo-1.0.0-openapi-3.0.0.yaml`](solardemo-1.0.0-openapi-3.0.0.yaml) | The OpenAPI 3.0 definition of the Solar System demo API |

```bash
aless examples/solardemo-1.0.0-openapi-3.0.0.yaml
aless examples            # browse them in the explorer
```

The OpenAPI definition is copied unchanged from
[`voxgig-sdk/voxgig-solardemo-sdk`](https://github.com/voxgig-sdk/voxgig-solardemo-sdk/blob/main/.sdk/def/solardemo-1.0.0-openapi-3.0.0.yaml)
(MIT License, Copyright (c) 2026 Voxgig). It exercises quoted keys
(`'200'`), `$ref` strings, `>-` folded block scalars and a whitespace-only
line, and the test suite checks that the YAML grammar reads it as a
reference YAML parser does.
