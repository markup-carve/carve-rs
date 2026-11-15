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
