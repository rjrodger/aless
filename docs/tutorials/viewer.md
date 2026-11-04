---
title: Look around a file in the viewer
description: Open an API description in aless, move through it, fold it, search it, copy a path, and then change the file on disk and watch aless keep your place.
order: 1
---
In this tutorial we open an API description in aless and look around it with the keyboard. The walk takes about 10 minutes.

You need aless installed. For more information, see [Install aless](/how-to/install.html). You also need a terminal at least 80 columns wide.

## Get the file

The file is a small OpenAPI document in YAML. It was written for these pages. Download it into an empty directory:

<!-- test: skip it downloads the file the site publishes, which the screens on this page read from examples/ -->
```sh
curl -LO https://aless.tabnas.dev/examples/bookshelf-openapi.yaml
```

Or open [`bookshelf-openapi.yaml`](/examples/bookshelf-openapi.yaml) in your browser and save it into the same directory.

## Open the file

To open the file, give aless the file's name:

```sh
aless bookshelf-openapi.yaml
```

aless reads the file and draws it as a tree. Every container starts unfolded:

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

The reversed row at the foot of the screen is the status bar. From left to right, the status bar names the file, the path of the focused node, the format, `watching`, and the line and column where the focused node starts in the file. The path here is `.`, the root of the document.

The row below the status bar is the status line. It holds messages and the search. The status line is empty for now.

The focused row's marker is the solid `▼`. The other markers are the hollow `▽`, so the solid marker shows you where the focus is.

## Move and fold

The keys are those of [jless](https://jless.io), because aless is modelled on it, so if you know jless, you know the keys already. The link goes to the jless site, outside this documentation. Paul Julius Martinez wrote the jless viewer.

To move down two rows, press `j` twice. The focus lands on `info`. The status bar follows the focus: `.info`, at line 2, column 1.

To fold `info`, press `h`. `info` folds into one row. The row shows a preview of what `info` holds:

<!-- screen: aless bookshelf-openapi.yaml
keys: j j h
rows: 3 -->
```screen
  ▶ info: (4) {title: "Bookshelf demo API", version: "1.0.0", …}
```

In the folded row, `(4)` is how many members `info` has: `title`, `version`, `description`, and `license`.

`l` unfolds `info` again. `Space` toggles the fold, so one key does both.

`c` folds the focused node and every sibling it has. Press `c`, and the whole document fits on the screen:

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

`J` and `K` move from sibling to sibling. To reach `paths`, press `J` three times. To unfold `paths`, press `l`:

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

The status bar now says `.paths`, at line 18.

`j` and `k` move a row at a time. `g` and `G` go to the first and the last row.

## Search

To find `listBooks`, press `/`, type `listBooks`, and press `Enter`. aless unfolds whatever hides the first match and moves the focus to it:

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

The path is too long for the status bar, so aless cuts the path at the left. The status line shows the pattern and the count, `[1/1]`: the first of one match. `n` and `N` go to the next and the previous match.

The pattern is a regular expression. It ignores case unless it holds a capital letter, as `listBooks` does.

## Copy the path

To copy the focused node's path, press `y`, then `p`. The path, `.paths["/api/shelf/{shelf_id}/book"].get.operationId`, goes to the clipboard. The status line names where the path went: `Copied path to the clipboard`.

aless writes the path as jless does. `yq` copies the path as jq writes it, ready for a jq filter. `yy` copies the value rather than its path.

Over SSH, where aless can reach no clipboard, aless copies through the terminal instead. The status line then names `the terminal clipboard (OSC 52)`.

## Try line mode and the source view

To switch to [line mode](/how-to/display.html#data-mode-and-line-mode), press `m`. Line mode shows the document as JSON prints it, with its quotes, commas, and closing brackets:

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

To go back, press `m` again.

To switch to the [source view](/how-to/display.html#data-mode-and-line-mode), press `s`. The source view shows the text aless read from the file. The focused node's line carries a marker:

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

The marker is on line 104, where the status bar says the focused node starts. To go back to the tree, press `s` again.

## Change the file

aless watches every file it shows. When the file changes, aless reloads it.

To see the reload, leave aless open. In a second terminal, edit the file:

1. Open `bookshelf-openapi.yaml` in your editor.
2. Change `The books on a shelf` to `Every book on a shelf, in order`.
3. Save the file.

Within a second, aless reloads the file:

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

The new summary is there. The focus is still on `listBooks`, because aless found the node again by its path. An edit above the node or around it doesn't move you. That's the whole loop: you read, something rewrites the file, and you go on reading where you were!

To quit, press `q`.

## Go further

For more information about every key, see [Keys](/reference/keys.html). `F1` shows the same keys inside aless. The how-to guides go further on each part of this walk:

- [Search a document](/how-to/search.html)
- [Copy a value or its path](/how-to/copy.html)
- [Choose how the viewer draws a document](/how-to/display.html)
- [Follow a file as it changes](/how-to/watch.html)

For more information about why the same keys work on a TOML file or a CSV file, see [One tree for every format](/explanation/one-tree.html).
