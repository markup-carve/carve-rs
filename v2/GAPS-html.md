# Spec gaps: resolve stage and HTML renderer

Gaps found while building `v2/src/resolve/` and `v2/src/html/` from the 0.2
specification. Each entry: nearest clause, minimal input, the readings, what
the oracle does, what this engine does.

## HTML-001 C2-H-115 counts 17 reserved names, the table has 18

Nearest clause: C2-H-115, table of C2-H-114.

The rows that stay or are new: `data-marker`, `data-carve-explicit-id`,
`data-carve-generated-id`, `data-carve-ref`, `data-carve-ref-collapsed`,
`data-carve-ref-shortcut`, `data-carve-autolink`,
`data-carve-footnote-label`, `data-carve-inline-footnote`, `data-carve-raw`,
`data-carve-abbreviations`, `data-carve-label`, `data-carve-tight`,
`data-carve-xref`, `data-carve-caption-number`, `data-carve-div`,
`data-carve-line-block`, `data-carve-src`. That is 18, not 17.

Readings: (a) the count is wrong and all 18 are reserved; (b) one row is not
meant to be reserved (`data-carve-src` is "reserved, never written").

This engine: all 18 are reserved (C2-H-116 drops an authored one under
`roundtrip`).

## HTML-002 C2-I-084 matches a heading on its folded source, the resolver has no source

Nearest clause: C2-I-084, C2-A-109.

```carve
# A *bold* title

[A *bold* title][] and [A bold title][]
```

Both references resolve in the oracle. `resolve(doc, opts)` gets the tree
only, and the tree holds no source text of the heading (`pos` offsets point
into a source the stage does not see). Readings: (a) the resolve stage needs
the source; (b) the match is against a canonical respelling of the heading.

This engine matches the folded plain text and a canonical respelling (bare
delimiters, one-backtick code spans), which covers the example. A heading
written with braced forms or escapes matches only by its plain text.

## HTML-003 C2-H-170 against C2-A-025: caption before or after the children

Nearest clauses: C2-A-025 ("Resolution visits `title`, then `children`, then
`caption`") and C2-H-170 ("a node before the nodes inside it: the caption of a
table takes its number before an equation in a cell of that table").

Readings: (a) the walk order of C2-A-025 holds for footnote numbers and ids,
and caption numbers are counted when their owner is visited; (b) C2-A-025
holds for everything and C2-H-170 is wrong.

This engine: (a). A caption number is counted at its owner (pre-order); the
footnote references inside the caption are numbered after the children.

## HTML-006 C2-H-147 `ascii` transliteration is host-defined

The host table is not specified. This engine decomposes (NFKD) and keeps the
ASCII characters, then applies C2-H-146 again and lowers. Outside conformance
by the clause's own words.

## HTML-007 C2-H-165/C2-H-166: a list item past the nesting bound

Nearest clauses: C2-H-165 ("Any other block that holds blocks is written as
those blocks in their degraded forms"), C2-H-166.

```carve
- a
  - d *e*
    - f
```

With `profile: minimal` (`max_nesting` 2) the inner list stands at the bound
and stays, and its item is outside. Readings: (a) the item is replaced by its
degraded blocks, which puts `<p>` directly into `<ul>`; (b) the item keeps its
`<li>` and its blocks are degraded.

Oracle: (b), `<li>d e` then `<p>- f</p>`. This engine: (b).

## HTML-008 C2-H-114 keeps `data-carve-abbreviations` on `<template>` but places none

Nearest clause: C2-H-114. No clause says where the template stands (end of the
document, the place of the first definition, each definition) or how a line is
spelled. The oracle writes no template under `roundtrip`. This engine writes
none.

## HTML-009 C2-H-168: static display math and its final line break

```carve
$$`E=mc^2`
```

With `mode: static` the clause says "`<pre><code class="language-math">`
around its source". The oracle writes the value and a line break, as a code
block value has one (C2-A-045). This engine follows the oracle.

## HTML-010 C2-H-175 and C2-H-161: an `abbr` value the filter empties

```carve
[g]{abbr="javascript:x"}
```

C2-H-175 omits `title` when the derived value is empty; C2-H-161 empties an
authored `abbr` value that names a denied scheme. Readings: (a) the emptiness
is tested after the filter, no `title`; (b) before it, `title=""`. Oracle: (b).
This engine: (b).

## HTML-011 C2-H-141: key order when a definition's attributes merge

```carve
[x][r]{k=2}

{k=1 t=3}
[r]: /u
```

C2-H-141 says the reference's key wins, not where it stands. C2-A-035 (merge of
attribute blocks) would keep `k` at its first place: `k="2" t="3"`. The oracle
writes the definition's other keys first and then the reference's keys:
`t="3" k="2"`. This engine follows the oracle.

## HTML-012 C2-H-064: a table panel of a figure group

C2-H-064: "Each panel in it is `<figure class="carve-figure-panel">`", and
C2-A-084 makes a `table` a panel too. A table panel is a `<table>`, not a
`<figure>`. The oracle writes it as a plain `<table>` without the panel class.
This engine does the same.

## HTML-013 C2-H-060: the single body takes the head rows too

```carve
{footer-rows=1}
|= h |= h2 |
| a | b |
| c | ^ |
```

"The table is then written with one `<tbody>` that holds every row." Read
literally that includes the head rows, though only the body/foot boundary is
crossed. The oracle puts the head rows into the one `<tbody>` as well (their
cells keep `scope="col"`). This engine follows the oracle.

## HTML-014 C2-H-139: an empty key value in the text of an unresolved reference

```carve
[a][nope]{e=""}
```

C2-H-139 fixes the order of the brace block but not the spelling of a value.
The oracle writes `{e}` for an empty value and quotes a value with a space
(`k="v w"`). This engine does the same; a value with other characters outside
letters, digits and `-_.:/` is quoted.

## HTML-015 C2-H-094/C2-H-165: notes whose marks a profile degrades

```carve
a[^1]

[^1]: N
```

With `profile: minimal` the reference degrades to the text `[^1]`. Whether the
footnote section is still written is not stated: C2-H-094 speaks of "numbered
footnotes", and the number is a tree fact. The oracle writes no section. This
engine writes a note only when its mark type (`footnote_ref` or
`inline_footnote`) is inside the profile, and no section when no note is left.

## HTML-016 C2-H-165: the value of a degraded code or raw block

"the value for a code or raw block" - the value ends with a line break
(C2-A-045). The oracle writes `<p>code</p>`, without the break. This engine
follows the oracle.

## HTML-017 ast-results lists `numberPrefix` as a result, C2-H-151 makes it an input

Nearest clauses: `data/ast-results.json` (`math`: `number`, `numberPrefix`),
C2-H-151 ("a display `math` node that carries `numberPrefix` takes the next
number"), C2-A-102.

`numberPrefix` has no source spelling and nothing derives it; resolution reads
it to pick the sequence. Readings: (a) only `number` is a result and the data
file lists one field too many; (b) resolution derives the prefix from some
source the spec does not name. The oracle keeps a supplied `numberPrefix` and
adds `number`. This engine: (a). `tests/resolve_examples.rs` and
`tests/oracle_extra.rs` do not strip `numberPrefix` before resolving.

## HTML-018 C2-H-100 has no form for a mention with a `label`

Nearest clauses: C2-H-100 (`<strong>@NAME</strong>`), C2-A-101 ("an optional
visible `label`").

```json
{"type": "mention", "kind": "user", "target": "core", "label": "Core Team"}
```

Readings: (a) the label replaces `@NAME`; (b) `@NAME` stays and the label is
not written. The oracle writes `<strong>Core Team</strong>`. This engine
follows the oracle.
