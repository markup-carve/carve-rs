# Rendering behavior

Two rendering choices this engine exposes: how sections are wrapped, and how heading ids are derived.

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

The table covers Latin, IPA, combining marks, Cyrillic, punctuation and currency
- not Greek, CJK or Arabic. `Fold` keeps what it cannot map, so a CJK heading
still has a usable, unique anchor; `Strict` drops it, so the id is guaranteed to
match `[0-9A-Za-z-]` and a heading in an uncovered script can end up with very
little left. Pick `Strict` only when a pure-ASCII fragment matters more than the
anchor's meaning.

Both transforms apply to the id index as well as the rendered attribute, so
`</#id>` cross-references and implicit `[Heading][]` references resolve against
the ids the option actually produced.

---

[Back to the README](../README.md)
