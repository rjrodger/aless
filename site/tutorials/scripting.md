---
title: Ask a file questions from a script
description: Use aless without its viewer, as a script or an agent does, to outline a file, read the value at a path, find nodes, turn a line and column into a path, and read an error.
order: 2
---
In this tutorial we put questions to the file from [the first tutorial](/tutorials/viewer.html), but from the command line, the way a script or an agent asks them. Every answer is JSON on standard output, and aless never stops to wait for a key. You need that file in your current directory:

```sh
curl -LO https://aless.tabnas.dev/examples/bookshelf-openapi.yaml
```

## What is in the file

Ask for an outline, one level deep:

```console
$ aless --paths --depth 1 bookshelf-openapi.yaml
{
  "file": "bookshelf-openapi.yaml",
  "format": "yaml",
  "path": ".",
  "entries": [
    {"path":".","kind":"object","line":1,"col":1,"length":6},
    {"path":".openapi","kind":"string","line":1,"col":1,"value":"3.0.0"},
    {"path":".info","kind":"object","line":2,"col":1,"length":4},
    {"path":".servers","kind":"array","line":10,"col":1,"length":1},
    {"path":".tags","kind":"array","line":13,"col":1,"length":2},
    {"path":".paths","kind":"object","line":18,"col":1,"length":6},
    {"path":".components","kind":"object","line":196,"col":1,"length":3}
  ],
  "total": 7,
  "limit": 200,
  "truncated": false
}
```

`--paths` lists the node it starts at and the nodes below it, an entry each, and `--depth 1` stops one level down. An entry gives the node's path, its kind, the line and column where it starts in the file, and a container's length or a scalar's value. `total` is how many entries there were, and `truncated` says whether `--limit` cut the list short.

There are six entries under `paths`. Start the outline there to list them:

```console
$ aless --paths --depth 1 --path .paths bookshelf-openapi.yaml
{
  "file": "bookshelf-openapi.yaml",
  "format": "yaml",
  "path": ".paths",
  "entries": [
    {"path":".paths","kind":"object","line":18,"col":1,"length":6},
    {"path":".paths.\"/api/shelf\"","kind":"object","line":19,"col":3,"length":2},
    {"path":".paths.\"/api/shelf/{shelf_id}\"","kind":"object","line":55,"col":3,"length":4},
    {"path":".paths.\"/api/shelf/{shelf_id}/book\"","kind":"object","line":99,"col":3,"length":3},
    {"path":".paths.\"/api/shelf/{shelf_id}/book/{book_id}\"","kind":"object","line":134,"col":3,"length":3},
    {"path":".paths.\"/api/author\"","kind":"object","line":162,"col":3,"length":1},
    {"path":".paths.\"/api/author/{author_id}\"","kind":"object","line":176,"col":3,"length":1}
  ],
  "total": 7,
  "limit": 200,
  "truncated": false
}
```

A key that is not a plain word is quoted in a path, as jq quotes it.

## Read a value

A path from an entry goes straight back into `--path`, and `--json` prints the value there:

```console
$ aless --json --path '.paths."/api/shelf/{shelf_id}/book".get.summary' bookshelf-openapi.yaml
"The books on a shelf"
```

Quote a path for the shell, which would otherwise read its brackets and braces as patterns of its own. A container comes out as indented JSON:

```console
$ aless --json --path '.components.schemas.Book' bookshelf-openapi.yaml
{
  "type": "object",
  "required": [
    "title",
    "authorId"
  ],
  "properties": {
    "id": {
      "type": "string",
      "readOnly": true
    },
    "title": {
      "type": "string",
      "example": "A Book of Examples"
    },
    "authorId": {
      "type": "string"
    },
    "published": {
      "type": "integer",
      "description": "The year it was published",
      "example": 1999
    },
    "tags": {
      "type": "array",
      "items": {
        "type": "string"
      }
    }
  }
}
```

The file is YAML and the answer is JSON. aless reads every format it knows into one kind of tree, and prints that tree as JSON whatever it was read from.

## Find a node

`--find` searches each node's `"key": value` text with a regular expression, as the viewer's `/` does. Find the operations, and keep the first three:

```console
$ aless --find operationId --limit 3 bookshelf-openapi.yaml
{
  "file": "bookshelf-openapi.yaml",
  "format": "yaml",
  "path": ".",
  "pattern": "operationId",
  "matches": [
    {"path":".paths.\"/api/shelf\".get.operationId","kind":"string","line":22,"col":7,"value":"listShelves"},
    {"path":".paths.\"/api/shelf\".post.operationId","kind":"string","line":37,"col":7,"value":"createShelf"},
    {"path":".paths.\"/api/shelf/{shelf_id}\".get.operationId","kind":"string","line":60,"col":7,"value":"getShelf"}
  ],
  "total": 11,
  "limit": 3,
  "truncated": true
}
```

`total` is 11 and `truncated` is true, so eight more matched. `--limit 0` lists them all.

## Turn a position into a path

Linters and validators report a line and a column. `--where --at` names the node at one:

```console
$ aless --where --at 104:7 bookshelf-openapi.yaml
{
  "file": "bookshelf-openapi.yaml",
  "format": "yaml",
  "path": ".paths.\"/api/shelf/{shelf_id}/book\".get.operationId",
  "kind": "string",
  "line": 104,
  "col": 7,
  "value": "listBooks"
}
```

That is the node the search found in the first tutorial. The question also goes the other way: `--where --path` gives the line and column a path starts at.

## Read an error

Now ask for a path that is not there, and print the exit status after it:

```console
$ aless --json --path .paths.nope bookshelf-openapi.yaml; echo $?
{
  "error": {
    "kind": "not_found",
    "file": "bookshelf-openapi.yaml",
    "format": "yaml",
    "path": ".paths.nope",
    "message": "no .paths.nope in bookshelf-openapi.yaml: .paths has no key \"nope\"",
    "nearest": {"path":".paths","kind":"object","line":18,"col":1,"length":6},
    "keys": ["/api/shelf","/api/shelf/{shelf_id}","/api/shelf/{shelf_id}/book","/api/shelf/{shelf_id}/book/{book_id}","/api/author","/api/author/{author_id}"]
  }
}
4
```

The error is a JSON object on standard error, and standard output is empty. The status says which kind of error it is (4 is a path or position that names nothing), and the object holds what a caller needs to try again: the deepest node the path reached, as `nearest`, and that node's first keys. So a script checks the status first, and reads standard output only when it is 0.

## Next

That is the whole loop: outline, narrow, read, and read the errors. [Get values out of a file in a script](/how-to/extract.html) has more recipes, and the [command line reference](/reference/command-line.html#output) has every output's fields. To hand all of this to an agent, see [Give an agent aless](/how-to/agents.html).
