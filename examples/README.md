# Examples

This directory holds files to open with aless. The documentation site
publishes each of those files under `/examples/`. The site's pages open
the same files.

| File | What it is | What it is for |
|---|---|---|
| [`bookshelf-openapi.yaml`](bookshelf-openapi.yaml) | An OpenAPI 3.0 description of a small, made-up bookshelf API | The tutorial walks through it. |
| [`books.json`](books.json) | Three made-up book records | The guides search it, copy from it, and break it. |
| [`broken.json`](broken.json) | A JSON document with a comma too many | The guide to a file that does not parse uses it as its example. |

To open the files, run aless from the repository's root:

```sh
aless examples/bookshelf-openapi.yaml
aless examples            # browse them in the explorer
```

`aless examples` opens the whole directory in the
[explorer](https://aless.tabnas.dev/how-to/explore.html).

No service serves the bookshelf API: it was written for this
repository. `bookshelf-openapi.yaml` holds quoted keys (`'200'`),
`$ref` strings, `allOf`, `>-` folded block scalars, and a line of only
spaces.
`tests/formats.rs` checks its structure and a source position. Git
checks the file out byte for byte on every platform, because
`.gitattributes` marks it `-text`.

`tests/site.rs` draws every capture of the viewer on the site's pages
from the files in this directory. The sample hosts file and its
grammar sit beside those files for every capture. Both come from
`tests/fixtures/grammars/`. The test fails when aless no longer draws
what a page shows. For more information, see
[The documentation site](../docs/contributing/site.md).
