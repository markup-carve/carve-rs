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
figure group - leaves nothing in the file that says it was there, so an import
cannot return it however good it gets. PART 11 §10s (CARVE-P11-063) defines an
opt-in mode that brackets each one with an HTML comment holding the container's
Carve opener and closer verbatim, and the import reverses it.

`Options::with_carry_markers` turns it on, `--carry-markers` does on the CLI, and
both apply to Markdown output only. The mode is OFF by default, because a
Markdown renderer with raw HTML turned off shows the comment as text. With it off
the emitted bytes are the ones this target emits today: the mode only ADDS comment
lines and moves none.

```rust
use carve::{markdown_to_carve, to_markdown_with_options, Options};

let options = Options::default().with_carry_markers(true);
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

### What is not carried

- **A container this target already spells.** An attributes-only opener is read as
  a paragraph and a list table is written as a pipe table, so neither is
  element-less and neither takes a marker.
- **A container inside a host that prefixes its lines.** In a list item, a block
  quote or a table cell the comment would land at the host's content column or
  behind its `>`, where the import does not read it, so writing one would look
  like a carry and not be. The container degrades there exactly as it does with
  the mode off. Reading a prefixed marker is an owed follow-up.
- **A composite figure group's own caption.** The caption slot hangs outside the
  closing fence, so its Markdown fallback follows the closer and the round trip
  does not restore it.
- **A marker-shaped line inside a fenced code block.** A code block's payload is
  verbatim content, so a page documenting this mode holds marker-shaped lines that
  record no container. They are left exactly as written.

### The escape

A `-->` in the payload would end the comment, so it is written `--\>`, and a
backslash the payload already carries before such a `>` grows by one. Nothing else
is escaped: the payload is Carve source, and Carve's own escape is the one the
reader already has.

### A damaged set is never guessed

A marker deleted, two reordered or a set left unbalanced imports as ordinary
Markdown, comments and all, with exactly one `carrier-markers-damaged` row on the
migration report at `degraded` fidelity and `fallback` confidence. No container is
partially reconstructed.
