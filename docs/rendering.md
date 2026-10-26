# Rendering behavior

Three rendering choices this engine exposes: how sections are wrapped, how
heading ids are derived, and whether the Markdown target carries the containers
it cannot spell.

## Section wrappers


A top-level heading is wrapped, along with the content following it up to the
next same-or-shallower heading, in a `<section>` carrying the heading's id (spec
PART 9 §13). Only the id moves - `{#install .featured}` gives
`<section id="install"><h2 class="featured">` - and a heading inside a
blockquote, div, or list item is not wrapped at all.

`with_sections(false)` renders headings flat, with the id back on the `<h*>`:

```rust
use carve::{to_html_with_options, Options};

let html = to_html_with_options("# A\n\np\n", &Options::new().with_sections(false));
assert_eq!(html, "<h1 id=\"A\">A</h1>\n<p>p</p>");
```

This exists for sites whose CSS or JS assumes rendered blocks are direct
children of the content container - the `.stack > * + *` spacing idiom,
`:first-child`, `nth-child()` counting, DOM child walks - all of which stop
matching once a wrapper sits in between. It is the one output change that
breaks a document whose *source* migrated cleanly.

Nothing else changes when it is off: ids, collision dedup, `</#id>`
cross-references, implicit `[Heading][]` references and heading numbering all
resolve against the slug rather than the element carrying it. The endnotes
`<section role="doc-endnotes">` is a separate construct and is still emitted.
The option is HTML-only - no other target emits `<section>`.

## Heading id transforms


An auto-generated heading id keeps the heading's own characters and its case:
`# Über uns` is `Über-uns`. Two OPT-IN, orthogonal transforms are available, and
both match carve-js and carve-php byte for byte:

```rust
use carve::{AsciiHeadingIds, Options};

let options = Options::new()
    .with_lowercase_heading_ids(true)
    .with_ascii_heading_ids(AsciiHeadingIds::Fold);
```

`with_lowercase_heading_ids` folds the kept characters per code point.
`with_ascii_heading_ids` transliterates them for URL and CSS-fragment
portability, through the same 903-entry table the other two engines carry:

| source | default | `Fold` | `Strict` |
| --- | --- | --- | --- |
| `Grüße` | `Grüße` | `Grusse` | `Grusse` |
| `Œuvre æsop` | `Œuvre-æsop` | `OEuvre-aesop` | `OEuvre-aesop` |
| `Ωmega` | `Ωmega` | `Ωmega` | `mega` |

The table covers Latin, IPA, combining marks, Cyrillic, punctuation and
currency - not Greek, CJK or Arabic. `Fold` keeps what it cannot map, so a CJK
heading still has a usable, unique anchor; `Strict` drops it, so the id is guaranteed to
match `[0-9A-Za-z-]` and a heading in an uncovered script can end up with very
little left. Pick `Strict` only when a pure-ASCII fragment matters more than the
anchor's meaning.

Both transforms apply to the id index as well as the rendered attribute, so
`</#id>` cross-references and implicit `[Heading][]` references resolve against
the ids the option actually produced.

---

[Back to the README](../README.md)

## Carrying a container through Markdown

A container the Markdown target writes as its children alone - a tab set, an
admonition, a named div, a columns block, a disclosure, a spoiler, a composite
figure group, a line block, a hard-break block - leaves nothing in the file that
says it was there, so an import cannot return it however good it gets. PART 11
§10s (CARVE-P11-063) defines an opt-in mode that brackets each one with an HTML
comment holding the container's Carve opener and closer verbatim, and the import
reverses it.

`Options::with_roundtrip(true)` (the public `roundtrip: true` field) or
`--roundtrip` enables the container comments for Markdown output. The same
option enables preservation attributes for HTML output, and no other setting
switches either target's carriers. It is off by default. A Markdown renderer
with raw HTML turned off shows the comments as text. Enabling it adds comment
lines without moving the existing output.

```rust
use carve::{markdown_to_carve, to_markdown_with_options, Options};

let options = Options::default().with_roundtrip(true);
let source = "::: note\nAn admonition body.\n:::\n";
let markdown = to_markdown_with_options(source, &options);
assert_eq!(
    markdown,
    "<!-- carve: ::: note -->\nAn admonition body.\n\n<!-- carve: ::: -->\n"
);
assert_eq!(markdown_to_carve(&markdown), source);
```

The payload is Carve source, spelled by the canonical writer rather than a second
time by this target, so the opener is the one `carve fmt` writes. An attributed
container is two Carve lines and so takes two markers: PART 4 is strict that an
opener line carries no inline `{...}` attributes, so the attributes travel on the
line above and that line takes a marker of its own.

A container inside a list item or a block quote takes its markers at the host's
content column or behind its `>`, and the import restores it in that host. A
composite figure group's caption line takes a marker of its own directly after
the closer's, and the import replaces the caption's rendered paragraph with it.

### Attribute lines, definition lists and captions

PART 11 §10t (CARVE-P11-067) carries three things that are not containers, with
the same marker, escape and damaged-set rule.

- An attribute line this target drops: a heading's authored `#id` or the
  attributes of a paragraph, block quote, list or table take one marker directly
  above the block. A generated heading id takes none.
- A definition list: each term takes a marker holding its term line, each
  definition a `:` marker above its first block, and one marker with an empty
  payload closes the list. Only the outermost definition list is carried.
- A caption: a block quote's, table's, image's, code listing's or equation's
  `^` line takes a marker directly above the paragraph this target writes for it.

Input:

```carve
{#start}
# Roundtrip Test

:: Carve
: A markup language.

> Stay hungry.
^ Steve Jobs
```

Output with `--markdown --roundtrip`:

```markdown
<!-- carve: {#start} -->
# Roundtrip Test

<!-- carve: :: Carve -->
**Carve**

<!-- carve: : -->
A markup language.

<!-- carve: -->

> Stay hungry.

<!-- carve: ^ Steve Jobs -->
Steve Jobs
```

Attributes on a list item's marker line and on an inline construct are not
carried.

### What is not carried

- **A container this target already spells.** An attributes-only opener is read as
  a paragraph and a list table is written as a pipe table, so neither is
  element-less and neither takes a marker.
- **A container inside a table cell.** This target flattens a cell to one line,
  so there is no line for a marker to stand on.
- **A definition list nested in a definition.** It degrades inside the outer
  list, which is carried.
- **A marker-shaped line in verbatim content.** A fenced or indented code
  block's payload is verbatim, so a page documenting this mode holds
  marker-shaped lines that record nothing. They are left exactly as written.

### The escape

A `-->` in the payload would end the comment, so it is written `--\>`, and a
backslash the payload already carries before such a `>` grows by one. Nothing else
is escaped: the payload is Carve source, and Carve's own escape is the one the
reader already has.

### A damaged set is never guessed

A marker deleted, two reordered, a set left unbalanced or a marker separated
from the block it stands above imports as ordinary Markdown, comments and all,
with exactly one `carrier-markers-damaged` row on the migration report at
`degraded` fidelity and `fallback` confidence. Nothing is partially
reconstructed.

## Roundtrip export

HTML spells less than a Carve document holds. A `*` bullet list and a `-` one
render the same `<ul>`, `***` and `---` the same `<hr>`, and a reference link
and a direct one the same `<a href>`: once a render has collapsed those, no
import can tell them apart. PART 10 §13 (CARVE-P10-015) defines one opt-in that
adds the carriers §13a names and changes nothing else.

`Options::with_roundtrip(true)` turns it on (the field is `roundtrip: true`),
`--roundtrip` does on the CLI, and the TARGET decides the carrier: HTML writes
the §13a attributes, Markdown writes the carrier comments [Carrying a container
through Markdown](#carrying-a-container-through-markdown) describes. It is OFF
by default, and enabling it neither removes nor reorders an attribute the
default bytes carry.

```rust
use carve::{html_to_carve, to_html_with_options, HtmlImportOptions, Options};

let options = Options::default().with_roundtrip(true);
let source = "* star\n\n***\n";
let html = to_html_with_options(source, &options);
assert!(html.contains(r#"<ul data-marker="*">"#));
assert!(html.contains(r#"<hr data-char="*">"#));

let back = html_to_carve(&html, &HtmlImportOptions::default()).unwrap();
assert_eq!(back.value, source);
```

### The carriers

| Attribute | Rides | Value |
| --- | --- | --- |
| `data-marker` | `<ul>` | the authored bullet, only where it is not `-` |
| `data-char` | `<hr>` | the authored break character, only where it is not `-` |
| `data-carve-explicit-id` | `<section>`, `<h1>`-`<h6>` | presence: the id was authored |
| `data-carve-id-slot` | `<section>` | the authored id's index among the heading's attributes, beside `data-carve-explicit-id`; omitted at 0 |
| `data-carve-ext` | `<section>` | `section`: the wrapper's id is generated |
| `data-carve-tight` | `<ul>`, `<ol>` | `true`: the list is tight although an item's paragraph keeps its `<p>` for its own attributes |
| `data-carve-ref` | `<a>`, `<img>` | the reference label it resolved by |
| `data-carve-autolink` | `<a>` | presence: authored as an autolink |
| `data-carve-footnote-label` | `<a>`, `<li>` | the authored footnote label |
| `data-carve-inline-footnote` | `<li>` | presence: the endnote was an inline footnote |
| `data-carve-col-widths` | `<table>` | the separator row's dash widths, comma-joined; absent for a table with no separator row or a width outside 1-1000 |
| `data-carve-label` | `<pre>` | the code block's authored label, the text between the fence label's brackets |
| `data-carve-raw` | `<span>` | the raw inline's format; the span holds its content as escaped text |
| `data-carve-escaped` | `<span>` | presence; the span holds the one escaped character |
| `data-carve-abbreviations` | `<template>` | presence; the template holds the document's top-level abbreviation definitions as escaped text, one `*[ABBR]: expansion` per line |

The prefix is `data-carve-`. Released carve-php versions wrote `data-djot-*`,
which the HTML importer still reads as a deprecated alias so their exports keep
round-tripping. A reader CONSUMES these names and never writes one back out as
an authored attribute, in any import mode.

The importer validates each value before it rebuilds anything. An id slot is a
run of decimal digits, and one past the heading's last attribute places the id
last. `data-carve-tight` reads `true` or `false`, either one taken over what the
items' `<p>` elements imply. On a section, `data-carve-ext` reads `section`
only. Any other value is not a carrier: it is dropped with an
`attribute-dropped` row and nothing is rebuilt from it.

Where the explicit-id carrier is read, the imported heading's `attrs.order`
records the id's slot. An import records no attribute order otherwise.

Two carriers on one element that contradict each other restore neither meaning.
`data-carve-autolink` beside a valid `data-carve-ref` imports as an inline link,
`data-carve-ext="section"` beside `data-carve-explicit-id` as an unmarked
section, and `data-carve-escaped` beside `data-carve-raw` as a plain span. The
pair is reported as one `attribute-dropped` warning, and neither name comes back
as an authored attribute. A footnote endnote carrying both
`data-carve-footnote-label` and `data-carve-inline-footnote` follows the same
rule.

An ordered list's authored marker is its delimiter, which PART 10 §12's
`data-delim` writes on every render, so `data-marker` rides the `<ul>` alone
here. Task markers use `[ ]` and `[_]` for open, `[/]` for progress, `[!]` for blocked, `[x]` or `[X]` for done, `[-]` for dropped, `[>]` for deferred, and `[?]` for uncertain. Only done is checked. A task list's `task-list` class and its items' `data-task-state` are §11's
and are written with or without this option.

### What it guarantees

Recovery of the named distinctions, not byte-identical source.

- Raw BLOCKS are not carried and not promised. A raw block whose format the
  target cannot write is dropped with the option on as it is with it off, and
  the `raw-format-dropped` row is reported in both cases.
- A carried raw inline is still reported. `data-carve-raw` lets an import
  recover the node, but the target did not write the format, so a raw inline
  in a format other than HTML reports `raw-format-dropped` with the option on
  as it does with it off.
- The reference form is not recovered. `[r][]` and `[r][r]` both write
  `data-carve-ref="r"`, so an import returns one spelling for both.
- Verbatim source is not carried. Fence length, fence character and opener
  padding are not in the tree, and `data-carve-src` is not written. The importer
  still consumes that name, in both spellings, so it never comes back as an
  authored attribute.
- A generated section id is told apart only where the wrapper carries
  `data-carve-ext="section"`. An unmarked wrapper is compared against the
  default slug, as before.

### An authored attribute of a carrier's own name

The carrier wins and the authored attribute is dropped, because emitting both
would be two attributes of one name. The drop is reported rather than silent:
`to_html_with_options_and_report` returns a `carrier-attribute-dropped` row
with the displaced name in `attribute` and no `format`.

`data-carve-src` is reserved without a carrier. Every importer consumes the
name, so roundtrip export drops an authored one from any element and reports the
same row, with no `pos`. Default export passes it through untouched.

### Export is not an import mode

Roundtrip export adds information to OUTPUT. The HTML importer's `roundtrip`
mode decides reconstruction and raw-HTML preservation of INPUT, and finding a
carrier in a document is not a reason to trust that document. Enabling export
never enables a trusted import, never widens an importer's mode and never
relaxes a destination check.

## Carve reference source

The source writer retains a reference label when its definition matches the
link or image destination and title at document level. Parsed nested definitions
are hoisted to that level by the reader. If a reference has no matching
definition or its fields cannot be spelled faithfully, `render_carve` returns `SourceUnspellable` rather than
silently writing an inline link. AST decoding still accepts the reference.
This follows the field-loss rule in spec PART 11 section 1d and the reference
fields in PART 12 section 3.

## Carve table alignment

`Options::default().with_align_tables(true)` enables aligned tables in
`try_to_carve_with_options`. For an existing `Document`, use
`render_carve_with_options(&doc, &CarveWriterOptions::default().with_align_tables(true))`.
The default writer keeps canonical tables unaligned.

## Mention and tag resolvers

`Options::with_mention_url` and `with_tag_url` remain available for route
templates. Application lookups can instead use `with_mention_resolver` and
`with_tag_resolver`. A resolver receives `SocialLinkResolverInput` with the
exact parsed name, node kind, attributes, and optional opaque context. It
returns `Ok(Some(destination))`, `Ok(None)` for unresolved, or `Err`.

A configured resolver is authoritative for its kind. Unresolved values and
errors render the ordinary inert span and never fall back to a template. A
denied destination from either a resolver or a URL template also renders the
inert span. Resolver errors are not otherwise exposed. A returned destination
is complete and is not encoded a second time.

## Straight quotes with smart typography



Set `Options { smart_typography: SmartTypographyMode::QuotesSource,
..Options::default() }` to emit quote and apostrophe source runs while keeping
other smart substitutions enabled. The setting applies to HTML, Markdown,
plain text, and ANSI. `Glyph` remains the default; `Source` still emits every
source run. Parsing and heading IDs are unchanged; typed curly quotes and
escapes keep their existing behavior. The CLI accepts
`--smart-typography quotes-source`.

## Whitespace and annotation offsets


An escaped space produces a `non_breaking_space` node. Preserved line-block
columns use the same node. Text and verbatim values keep literal Unicode,
including U+E000. HTML renders generated spaces as `&nbsp;`; Markdown uses
U+00A0, and plain text and ANSI use ordinary spaces.

Annotation offsets count Unicode codepoints in the contract's fixed field
order. They do not depend on JSON property insertion order or source positions.
The shared annotation fixture covers image alt text, math, breaks and reversed
ranges.

Table body partitions and source attributes are documented in
[Table source metadata](table-source-metadata.md).
