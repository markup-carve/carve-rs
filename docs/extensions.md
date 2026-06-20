# Extensions

Opt-in extensions implement `CarveExtension` and are passed through `Options`.
This page documents the built-in extensions in more depth than the README. See
the README's `## Extensions` section for the general extension model and the
short list of all built-ins.

## ListTable

`ListTable` is a Tier-3 extension that renders a `::: list-table` block authored
as a nested list into a real HTML `<table>`. Unlike the native pipe-table syntax
(whose cells are inline-only), a list-table cell can hold full block content -
paragraphs, lists, code blocks, nested tables - because each cell is a list item.

Register it like any other extension:

```rust
use carve::{ListTable, Options};

let ext = ListTable::new();
let opts = Options::new().with_extension(&ext);
let html = carve::to_html_with_options("::: list-table\n- - A\n  - B\n:::", &opts);
assert_eq!(
    html,
    "<table>\n  <tbody>\n    <tr><td>A</td><td>B</td></tr>\n  </tbody>\n</table>"
);
```

### Authoring model

The block is an outer list where each outer item is a row and each inner item is
a cell:

```
::: list-table
- - Row 1, cell 1
  - Row 1, cell 2
- - Row 2, cell 1
  - Row 2, cell 2
:::
```

### Caption

The caption comes from the quoted title on the opener (Carve parses the title;
it is not a `{caption=...}` attribute):

```
::: list-table "Quarterly results"
- - A
  - B
:::
```

renders `<caption>Quarterly results</caption>`. The title is flattened to
escaped plain text.

### Header rows and columns

`{header-rows=N}` and `{header-cols=N}` go on the line PRECEDING the opener (a
trailing `{...}` on the `:::` line would make the whole block literal in Carve):

```
{header-rows=1}
::: list-table
- - Region
  - Q1
- - EMEA
  - 10
:::
```

`header-rows=N` promotes the first N rows to `<thead>` with `<th>` cells.
`header-cols=N` promotes the first N cells of every row to row-header `<th>`.
The two combine. The boolean form `{header-rows}` (no value) means the first
row, the common "this table has a header row" case, so `=1` is rarely needed;
`{header-cols}` likewise promotes the first column. An absent attribute means no
header.

### Block cells

A cell whose only content is a single attribute-free paragraph collapses to
inline content (`<td>text</td>`), matching tight list-item rendering. A
multi-block cell keeps its wrappers:

```
::: list-table
- - A
  - Strong quarter.

    - new logos
    - renewals
:::
```

renders the second cell as `<td><p>Strong quarter.</p>\n<ul>...</ul></td>`.

### Row and column spans

A cell whose sole content is a lone `^` merges with the cell above it (rowspan);
a lone `<` merges with the cell to its left (colspan). These are the SAME
continuation markers Carve's native pipe tables use, and the resulting `<table>`
matches what the equivalent pipe table would produce:

```
{header-rows=1}
::: list-table "Sales"
- - Region
  - Q1
  - Q2
- - EMEA
  - 10
  - 12
- - ^
  - 14
  - 16
- - Total
  - <
  - <
:::
```

EMEA's cell gets `rowspan="2"` (it plus the `^` below); "Total" gets
`colspan="3"` (it plus the two `<`).

A `^`/`<` with nothing to merge into (a `^` in the first row, a leading `<`)
becomes an empty cell rather than being dropped - again matching pipe tables.

### Escaping a marker

A cell carrying its own attributes is never a span marker: its `^`/`<` content is
then literal (the same escape pipe tables use), and the cell's attributes carry
onto the `<td>`/`<th>`:

```
::: list-table
- - A
  -{.note} ^
:::
```

renders `<td class="note">^</td>`.

### Header / body rowspan clamp

An HTML cell cannot reliably span from `<thead>` into `<tbody>`. A `^` in a body
row whose origin sits in the header rows therefore finds no valid origin and
degrades to an empty cell.

### Ragged rows and table attributes

Short rows are padded with empty cells to the widest effective row, so no content
is dropped and the grid stays rectangular. A preceding attribute line carries id
and sibling classes onto the `<table>` tag; the structural `header-rows` /
`header-cols` keys are consumed and dropped.

### Deferring (never drop content)

When a `list-table` cannot be rendered as a table - its sole child is not a list,
or a row was authored as a plain paragraph (`- not-a-cell-row`) with no inner
cell list - the whole block degrades to the default `<div class="list-table">`
holding the literal nested list. The defer decision is made on the pristine AST
before any rewrite, so a deferred render is byte-identical to the plain block and
nothing is silently lost.

Without the extension registered, `::: list-table` always stays the default
`<div class="list-table">`.
