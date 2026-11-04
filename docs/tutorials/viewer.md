---
title: Look around a file in the viewer
description: Open an API description in aless, move through it, fold it, search it and copy a path, then change it on disk and watch aless keep your place.
order: 1
---
In this tutorial we open an API description in aless and find our way around it with the keyboard. It takes about ten minutes. You need aless installed ([Install aless](/how-to/install.html)) and a terminal at least 80 columns wide.

## Get the file

The file is a small OpenAPI document in YAML, written for these pages. Download it into an empty directory:

```sh
curl -LO https://aless.tabnas.dev/examples/bookshelf-openapi.yaml
```

Saving [the file](/examples/bookshelf-openapi.yaml) from a browser does as well.

## Open it

Give aless the file's name:

```sh
aless bookshelf-openapi.yaml
```

aless reads the file and shows it as a tree, with every container unfolded:

<!-- screen: aless bookshelf-openapi.yaml
size: 80x14 -->
```screen
▼ {
    openapi: "3.0.0"
  ▽ info: {
      title: "Bookshelf demo API"
      version: "1.0.0"
      description: "A small API for keeping books on shelves, written for aless…
    ▽ license: {
        name: "MIT"
  ▽ servers: [
    ▽ {
        url: "https://bookshelf.example/v1"
        description: "An example server"
 bookshelf-openapi.yaml .                                 yaml · watching · 1:1
```

The reversed row at the foot of the screen is the status bar, and the row below it, empty here, holds messages and the search. The status bar names the file, then the path of the focused node (`.`, the root of the document), the format, `watching`, and the line and column where the focused node starts in the file. The focused row's marker is solid (`▼`), and the others are hollow (`▽`).

## Move and fold

The keys are those of [jless](https://jless.io), by Paul Julius Martinez, the viewer aless is modelled on. If you know jless, you know them already.

Press `j` twice. The focus moves down two rows to `info`, and the status bar follows it: `.info`, at line 2, column 1. Now press `h`, and `info` folds into one row, with a preview of what it holds:

<!-- screen: aless bookshelf-openapi.yaml
keys: j j h
rows: 3 -->
```screen
  ▶ info: (4) {title: "Bookshelf demo API", version: "1.0.0", …}
```

`(4)` is how many members `info` has. `l` unfolds it again, and `Space` toggles it either way. Press `c`, which folds the focused node and every sibling it has, and the whole document fits on the screen:

<!-- screen: aless bookshelf-openapi.yaml
keys: j j h c
size: 80x10 -->
```screen
▽ {
    openapi: "3.0.0"
  ▶ info: (4) {title: "Bookshelf demo API", version: "1.0.0", …}
  ▷ servers: (1) [{…}]
  ▷ tags: (2) [{…}, {…}]
  ▷ paths: (6) {"/api/shelf": {…}, "/api/shelf/{shelf_id}": {…}, …}
  ▷ components: (3) {parameters: {…}, responses: {…}, schemas: {…}}

 bookshelf-openapi.yaml .info                             yaml · watching · 2:1
```

`J` and `K` move from sibling to sibling. Press `J` three times to reach `paths`, then `l` to unfold it:

<!-- screen: aless bookshelf-openapi.yaml
keys: j j h c J J J l
size: 80x14 -->
```screen
▽ {
    openapi: "3.0.0"
  ▷ info: (4) {title: "Bookshelf demo API", version: "1.0.0", …}
  ▷ servers: (1) [{…}]
  ▷ tags: (2) [{…}, {…}]
  ▼ paths: {
    ▽ "/api/shelf": {
      ▽ get: {
        ▽ tags: [
            "shelf"
          operationId: "listShelves"
          summary: "List the shelves"
 bookshelf-openapi.yaml .paths                           yaml · watching · 18:1
```

`j` and `k` move a row at a time, and `g` and `G` go to the first and the last row.

## Search

Press `/`, type `listBooks`, and press `Enter`. aless unfolds whatever hides the first match, and moves the focus to it:

<!-- screen: aless bookshelf-openapi.yaml
keys: j j h c J J J l / listBooks Enter
size: 80x14 -->
```screen
              $ref: "#/components/responses/NotFound"
    ▽ "/api/shelf/{shelf_id}/book": {
      ▽ parameters: [
        ▽ {
            $ref: "#/components/parameters/ShelfId"
      ▽ get: {
        ▽ tags: [
            "shelf"
          operationId: "listBooks"
          summary: "The books on a shelf"
        ▽ parameters: [
          ▽ {
…l .paths["/api/shelf/{shelf_id}/book"].get.operationId yaml · watching · 104:7
/listBooks  [1/1]
```

The path is too long for the status bar, so it is cut at the left. The row below it shows the pattern, and that this is the first of one match. `n` and `N` go to the next and the previous match. The pattern is a regular expression, and it ignores case unless it holds a capital letter, as this one does.

## Copy the path

Press `y`, then `p`. aless copies the focused node's path, `.paths["/api/shelf/{shelf_id}/book"].get.operationId`, and the status line says where it put it: `Copied path to the clipboard`.

The path is in jless's syntax. `yq` copies it as jq writes it, ready for a jq filter, and `yy` copies the value instead of its path. Over SSH, where no clipboard can be reached, aless copies through the terminal, and the status line names `the terminal clipboard (OSC 52)`.

## Two more ways to look

Press `m` for line mode, which shows the document as JSON prints it, with its quotes, commas and closing brackets:

<!-- screen: aless bookshelf-openapi.yaml
keys: j j h c J J J l / listBooks Enter m
size: 80x14 -->
```screen
        ▽ {
            "$ref": "#/components/parameters/ShelfId"
          }
        ],
      ▽ "get": {
        ▽ "tags": [
            "shelf"
          ],
          "operationId": "listBooks",
          "summary": "The books on a shelf",
        ▽ "parameters": [
          ▽ {
…s["/api/shelf/{shelf_id}/book"].get.operationId yaml · line · watching · 104:7
/listBooks  [1/1]
```

Press `m` again to go back. Now press `s` for the source view, the text aless read from the file, with the focused node's line marked:

<!-- screen: aless bookshelf-openapi.yaml
keys: j j h c J J J l / listBooks Enter s
size: 80x14 -->
```screen
 98            $ref: '#/components/responses/NotFound'
 99    /api/shelf/{shelf_id}/book:
100      parameters:
101        - $ref: '#/components/parameters/ShelfId'
102      get:
103        tags: [shelf]
104▶       operationId: listBooks
105        summary: The books on a shelf
106        parameters:
107          - $ref: '#/components/parameters/Limit'
108        responses:
109          '200':
 bookshelf-openapi.yaml  (source)                       yaml · watching · 104:7
/listBooks  [1/1]
```

Press `s` again to go back to the tree.

## Change the file

aless watches every file it shows. Leave it open, and open the file in your editor in a second terminal. Change `The books on a shelf` to `Every book on a shelf, in order`, and save. Within a second, aless reads the file again:

<!-- screen: aless bookshelf-openapi.yaml
keys: j j h c J J J l / listBooks Enter
edit: bookshelf-openapi.yaml: The books on a shelf => Every book on a shelf, in order
size: 80x14 -->
```screen
              $ref: "#/components/responses/NotFound"
    ▽ "/api/shelf/{shelf_id}/book": {
      ▽ parameters: [
        ▽ {
            $ref: "#/components/parameters/ShelfId"
      ▽ get: {
        ▽ tags: [
            "shelf"
          operationId: "listBooks"
          summary: "Every book on a shelf, in order"
        ▽ parameters: [
          ▽ {
…l .paths["/api/shelf/{shelf_id}/book"].get.operationId yaml · watching · 104:7
Reloaded bookshelf-openapi.yaml (328 nodes)
```

The new summary is there, and the focus is still on `listBooks`. aless found the node again by its path, so an edit above it or around it does not move you. That is the whole loop: you read, something rewrites the file, and you go on reading where you were!

Press `q` to quit.

## Next

[Keys](/reference/keys.html) lists every key, as `F1` shows them inside aless. The how-to guides go further on each part of this walk: [Search a document](/how-to/search.html), [Copy a value or its path](/how-to/copy.html), [Choose how the viewer draws a document](/how-to/display.html) and [Follow a file as it changes](/how-to/watch.html). [One tree for every format](/explanation/one-tree.html) says why the same keys work on a TOML file or a CSV file.
