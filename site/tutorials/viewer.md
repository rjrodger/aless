---
title: Look around a file in the viewer
description: Open a file in aless, move through it, fold it, search it and copy a path, then change it on disk and see aless keep your place.
order: 1
---
In this tutorial we open an API description in aless and find our way around it with the keyboard. It takes about ten minutes. You need aless installed ([Install aless](/how-to/install.html)) and a terminal at least 80 columns wide.

## Get the file

The file is a small, made-up OpenAPI document in YAML. Download it into an empty directory:

```sh
curl -LO https://aless.tabnas.dev/examples/bookshelf-openapi.yaml
```

Any other way of saving [the file](/examples/bookshelf-openapi.yaml) there does as well.

## Open it

Open the file:

```sh
aless bookshelf-openapi.yaml
```

aless reads it and shows it as a tree, every container expanded:

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

The last row is the status bar. It names the file, then the path of the focused node (`.`, the root), the format, `watching`, and the line and column in the file where the focused node starts. The focused row's marker is filled in (`▼`), and every other marker is hollow (`▽`).

## Move and fold

Press `j` twice. The focus moves down two rows to `info`, and the status bar follows it: `.info`, which starts at line 2, column 1.

Press `h`. `info` folds into one row, with a preview of what it holds:

```screen
  ▶ info: (4) {title: "Bookshelf demo API", version: "1.0.0", …}
```

`(4)` is how many members it has. `l` unfolds it again, and `Space` toggles it either way. Now press `c`, which folds the focused node and all its siblings, and the whole document fits on the screen:

```screen
▽ {
    openapi: "3.0.0"
  ▶ info: (4) {title: "Bookshelf demo API", version: "1.0.0", …}
  ▷ servers: (1) [{…}]
  ▷ tags: (2) [{…}, {…}]
  ▷ paths: (6) {"/api/shelf": {…}, "/api/shelf/{shelf_id}": {…}, …}
  ▷ components: (3) {parameters: {…}, responses: {…}, schemas: {…}}
```

`J` and `K` move between siblings. Press `J` three times to reach `paths`, then `l` to unfold it:

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

`j` and `k` move one row at a time, and `g` and `G` go to the first and the last row.

## Search

Press `/`, type `listBooks`, and press `Enter`. aless unfolds whatever hides the first match and moves the focus to it:

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

The path is too long for the status bar, so it is cut at the left, and the row below shows the search: the first of one match. `n` and `N` go to the next and the previous match. The pattern is a regular expression, and it ignores case unless it holds a capital letter (this one does).

## Copy the path

Press `y`, then `p`. aless copies the focused node's path, `.paths["/api/shelf/{shelf_id}/book"].get.operationId`, and says where it put it:

```screen
Copied path to the clipboard
```

Over SSH, or anywhere else no clipboard can be reached, it copies through the terminal instead, and says `the terminal clipboard (OSC 52)`. The path is in jq's syntax, so jq takes it, and so does aless's own `--path`, as the [next tutorial](/tutorials/scripting.html) shows. `yy` copies the value rather than its path.

## Two more ways to look

Press `m` for line mode, which shows the document as JSON prints it, with quoted keys, commas and closing brackets:

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
```

Press `m` again to go back. Now press `s` for the source, the file as it is on disk, with the focused node's line marked:

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
```

Press `s` again to return to the tree.

## Change the file

aless watches the file it shows. Leave it open, and in a second terminal open the file in your editor. Change `The books on a shelf` to `Every book on a shelf, in order`, and save. Within a second, aless reloads it:

```screen
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

The new summary is there, and the focus is still on `listBooks`. aless found it again by its path, so an edit above it or around it does not move you. Press `q` to quit.

## Next

That is the viewer's daily work: move, fold, search, copy. [Keys](/reference/keys.html) lists every key, as `F1` shows them inside aless. The [next tutorial](/tutorials/scripting.html) asks the same file questions from the command line, without opening the viewer at all.
