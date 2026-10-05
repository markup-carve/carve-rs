# Linting


`carve::lint_carve` reports silent degradations - places where a document parses
and renders without error, but something the author wrote does not reach the
output. It returns a `Vec<LintWarning>`, each carrying a stable `rule` id shared
with carve-js and carve-php, a message, a 1-based line and column, and byte
offsets into the source you passed.

```rust
let warnings = carve::lint_carve("`c`{kbd}\n");
assert_eq!(warnings[0].rule, "semantic-attribute-outside-span");
```

The same check from the command line, which is what a CI gate wants:

```bash
carve lint docs/*.crv
```

```
docs/guide.crv:3:1 unattached-block-attribute — This block attribute reaches no block: ...
```

Exit codes are the interface, and the three-way split is deliberate: **0**
clean, **1** findings, **2** a file could not be read. Collapsing 2 into 1 would
report an unreadable path as a lint failure; collapsing it into 0 would pass a
build whose documents were never opened. A bad path is reported and skipped, so
one missing file in a glob still lets every other document be checked.

Reads stdin with no path, or with `-`, and reports under `<stdin>`.
`--extensions` enables the bundled extensions. For citations, use
`carve lint --extension citations docs/*.crv`. The command refuses render-only
flags with exit 2 rather than silently ignoring them.

The line format and the exit codes match carve-js's `carve lint` exactly, so a
script that parses one CLI parses the other. The `rule` id is shared across
engines by contract; the message PROSE is not - the same trigger reports the
same id everywhere, worded for each engine.

Default checks include `unattached-block-attribute` for attributes that reach
no block, and these Markdown/Djot habits in literal text:

| Rule | Source | Carve spelling |
| --- | --- | --- |
| `markdown-strong-double-star` | `**bold**` | `*bold*` |
| `markdown-strikethrough-double-tilde` | `~~gone~~` | `~gone~` |
| `djot-superscript-caret` | `^sup^` | `{^sup^}` |
| `djot-plus-bullet` | `+ item` | `- item` |

A plus line consumed by a table continuation does not trigger the bullet rule.

The compact semantic span attribute rules (spec PART 9 §10):

| rule | fires on |
| --- | --- |
| `semantic-attribute-value-ignored` | a value on a reserved name that only selects a wrapper: `[x]{kbd="V"}` renders `<kbd>x</kbd>` and `V` reaches no output |
| `semantic-attribute-outside-span` | a reserved name anywhere other than an ordinary `[content]{attrs}` span, where it stays a raw attribute: `` `c`{kbd} `` renders `<code kbd="">c</code>` |

The composite-figure rules (spec PART 9 §4c):

| rule | fires on |
| --- | --- |
| `figure-group-opener-metadata` | a `::: figure` opener carrying a quoted title or a `[label]`, which stays a generic container - the group has no title or label slot |
| `figure-group-nested` | a bare `::: figure` opener inside an open group's body, which stays a generic container - groups do not nest |
| `figure-group-panel-number` | a `#` placeholder in a PANEL caption, which stays literal - panels are not sequence units |

Both semantic span attribute rules are tier-aware. `abbr`, `time` and `kbd` are
reserved in core; `samp`, `var`, `cite` and `dfn` only become elements once the
`SemanticSpan` extension is registered, and until then they are ordinary
attributes whose value reaches the output intact. Pass the same `Options` you
render with so the diagnostics describe the output you will actually get:

```rust
let warnings = carve::lint_carve_with_options(source, &options);
```

`cite` on a block quote is a valid HTML URL attribute and is deliberately not
reported.

The in-document reference rules:

| rule | fires on |
| --- | --- |
| `broken-crossref` | a `</#id>` cross-reference with no matching heading or numbered caption id; it renders as literal text. When the id exists on an element a cross-reference cannot reach (a paragraph, a span, an uncaptioned table), the message names that element and suggests `[text](#id)`. Ids match case-sensitively; a case-only miss names the real id |
| `unresolved-reference-link` | a `[text][label]` or `[text][]` reference with no matching definition (or, for `[text][]`, heading). Labels match case-sensitively; a case-only miss names the real label or heading text |
| `broken-fragment-link` | a `[text](#id)` link, inline or through a reference definition, whose fragment matches no id in the rendered HTML |

`broken-fragment-link` reads ids off the rendered output, so heading slugs,
footnote ids and ids in raw HTML count, while an id quoted in a code block, an
HTML comment or another attribute's value does not. Fragments match
case-sensitively and a case-only near miss is named. A `:~:` text directive is
ignored, a percent-encoded fragment is decoded, and `#`, `#top` and links into
other files are skipped. With `citations`, `#ref-…` and `#cite-…` are skipped;
with any other extension except `semantic-span` the rule stays silent, since
lint cannot know which ids that extension generates.

With citations enabled, `references-placement-in-container` reports a
`::: references` marker inside a container. The marker renders as a div there;
the generated list keeps its document position.

---

[Back to the README](../README.md)
