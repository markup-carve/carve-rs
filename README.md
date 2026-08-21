# carve-rs

[![CI](https://github.com/markup-carve/carve-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/markup-carve/carve-rs/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

Rust parser and renderer (HTML, Markdown, plain text, ANSI) for the [Carve](https://github.com/markup-carve/carve) markup language.

> Carve is a post-Markdown lightweight markup language with visual mnemonics and human-centered design. See the [language site](https://markup-carve.github.io/carve/) for the spec.

Implements **Carve spec 0.1** (see [Versioning & Changelog](https://markup-carve.github.io/carve/versioning)).

## Install

```sh
cargo add carve-lang
```

The crate is published on crates.io as **`carve-lang`** (the name `carve` was
taken), but it is imported as `carve` and its CLI binary is `carve`:

```rust
let html = carve::to_html("# Hello /Carve/");
```

HTML migration is available through `html_to_ast` and `html_to_carve`. Both
return the value plus ordered loss diagnostics and accept safe, semantic, and
trusted-roundtrip policies. The CLI equivalent is:

```sh
carve migrate --from html --mode safe --report report.json input.html
```

A `<math>` element is read for the TeX it already carries: a `<semantics>`
annotation declaring `application/x-tex`, `text/x-tex` or `LaTeX`, else
`alttext` with the assumption reported. There is no MathML-to-TeX converter
here by decision (carve#1210 D6), so an element carrying neither is dropped
with a warning in `safe` and `semantic` and preserved verbatim in
`roundtrip` - its children are a token stream, and concatenating them reads
`<mfrac><mn>1</mn><mn>2</mn></mfrac>` as `12` rather than as one half.

`--adapter word` and `--adapter google-docs` add one recognition the `generic`
default does not risk: footnote-shaped HTML. A word processor writes a note as
a body anchor and a definition block that link to each other, and none of them
uses the `doc-noteref` / `doc-endnotes` roles a Carve engine writes, so under
`generic` a note arrives as a literal link beside an orphaned list. Under those
two adapters the pair is matched through the fragment each anchor addresses and
written back as a footnote reference and definition, whatever the ids are
called - Word's `_ftnref1`/`_ftn1`, Google Docs' `ftnt_ref1`/`ftnt1`,
LibreOffice's `sdfootnote1anc`/`sdfootnote1sym` and Pandoc's `fnref1`/`fn1` all
pair by the same rule. Back-links, the marker anchors they sit on, and the rule
separating the notes from the body are generated navigation and are dropped. A
reference whose target is missing stays a link, and a definition nothing
references stays ordinary content rather than becoming a definition that renders
as nothing. Name the adapter only for input you know came from that editor: on
arbitrary HTML a mutually linked anchor pair is not proof of a footnote, which
is why `generic` stays out.

Markdown migration is `markdown_to_ast` and `markdown_to_carve`, or
`carve migrate --from markdown input.md`. It parses the source to a tree and
writes it canonically, so the output is the document rather than the author's
spelling: a setext heading comes back as `#`, an indented code block as a
fence. There is no mode or report, because nothing is dropped - the
`--mode`/`--adapter`/`--report` options belong to HTML.

Djot migration is `djot_to_carve`, or `carve migrate --from djot input.dj`. It
rewrites the delimiters that differ between the two languages, and like
Markdown it has no mode or report.

## Status

The crate passes every `.crv` / `.html` pair currently checked into its
`tests/spec` submodule. The conformance test suite includes an all-pairs gate so
new corpus pairs fail CI until the parser and renderer support them.

| Pair | Construct | Status |
|---|---|---|
| 01-emphasis | `/italic/`, `*bold*`, `_underline_`, `~strike~`, `=hl=`, `{^super^}`, `{,sub,}`, `/*bi*/` | passing |
| 02-headings | `# H1` … `#### H4` | passing |
| 03-links | `[text](url)` | passing |
| 04-images | `![alt](src)` (inline + block) | passing |
| 05-lists | unordered (`- item`) | passing |
| 06-task-lists | `- [ ] todo`, `- [x] done` | passing |
| 11-fenced-code | ` ``` ` blocks with language tag | passing |
| 12-inline-code | `` `code` `` | passing |
| 07-blockquote-with-attribution | `> quote` + `^ Attribution` caption | passing |
| 08-image-with-caption | `![…](…)` + `^ caption` | passing |
| 09–10 | tables, rowspan/colspan | passing |
| 13 | admonitions (`::: note`) | passing |
| 14 | abbreviations (`*[ABBR]:`) | passing |
| 15 | `@mentions`, `#tags` | passing |
| 16 | inline extensions (`:type[…]`) | passing |
| 17 | attribute blocks (`{#id .class}`) | passing |
| 18 | YAML frontmatter | passing |
| 318-composite-figures | `::: figure` groups: panels, group caption on the closer, `Figure 2a` crossrefs | passing |

## Library use

```rust
let html = carve::to_html("# Hello\n\nThis is /italic/ and *bold*.");
assert!(html.contains("<h1>Hello</h1>"));
```

For lower-level access, `carve::parse` returns a typed `Document` AST and `carve::render_html` walks it to HTML:

```rust
let doc = carve::parse(source);
// inspect or transform doc.children …
let html = carve::render_html(&doc);
```

Besides HTML, the crate renders the same AST to Markdown, plain text, and
ANSI-styled text via `carve::to_markdown`, `carve::to_plain_text`, and
`carve::to_ansi` (each with a matching `render_*` function for a parsed
`Document`).

### Linting

`carve::lint_carve` reports silent degradations - places where a document parses
and renders without error, but something the author wrote does not reach the
output. It returns a `Vec<LintWarning>`, each carrying a stable `rule` id shared
with carve-js and carve-php, a message, a 1-based line and column, and byte
offsets into the source you passed.

```rust
let warnings = carve::lint_carve("`c`{kbd}\n");
assert_eq!(warnings[0].rule, "semantic-attribute-outside-span");
```

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

Both are tier-aware. `abbr`, `time` and `kbd` are reserved in core; `samp`,
`var`, `cite` and `dfn` only become elements once the `SemanticSpan` extension
is registered, and until then they are ordinary attributes whose value reaches
the output intact. Pass the same `Options` you render with so the diagnostics
describe the output you will actually get:

```rust
let warnings = carve::lint_carve_with_options(source, &options);
```

`cite` on a block quote is a valid HTML URL attribute and is deliberately not
reported.

### Section wrappers

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

### Heading id transforms

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

### ProseMirror / Tiptap

The AST converts to a ProseMirror document and back, so a Tiptap editor and this
engine can share one stored document:

```rust
let doc = carve::parse(source);
let editor = carve::to_prosemirror(&doc);
let back = carve::from_prosemirror(&editor.json)?;
```

Node and mark names come from the map carve-grammars publishes, vendored under
`resources/` with the commit it was copied from, rather than restated here - the
same arrangement carve-php uses. Every name in the conversion is read from it;
none is written out, and a test fails if one is.

The editor model is smaller than Carve's AST, so `to_prosemirror` reports what
it could not carry rather than losing it quietly:

```rust
let editor = carve::to_prosemirror(&doc);
if !editor.dropped.is_empty() || !editor.degraded.is_empty() {
    // `dropped` - the content is gone.
    // `degraded` - the node type is gone, its text survives: a soft break
    //   becomes a space, an escape becomes the character it escaped.
}
```

An application that stores what the editor returns should assert both are empty
before saving. Going the other way, a ProseMirror name the map does not know is
an **error**, not a skip: an editor that grew a node type nobody mapped is
exactly where a quiet skip destroys the most content.

On the shared corpus, 791 documents report nothing lost and round-trip to
byte-identical HTML; 215 report what they lost. The spec's
[format bridges](https://markup-carve.github.io/carve/format-bridges) page has
the reasoning behind the arrangement.

## Untrusted input

The normative hardening is always on and needs no configuration: dangerous URL
schemes are blanked, event-handler attributes like `onclick` are dropped, and the
bidi override/isolate characters behind Trojan Source are removed from rendered
text.

Raw passthrough is the deliberate exception. A ` ```=html ` block or a
`` `…`{=html} `` span renders **verbatim** by design, so it is the one thing input
you did not author has to switch off:

```rust
let options = carve::Options::new()
    .with_raw_html(false)                        // escape =html, do not emit it
    .with_profile(carve::Profile::comment());    // full | article | comment | minimal

let html = carve::try_to_html_with_options(untrusted, &options)?;
```

Use the `try_*` entry points here, not `to_html_with_options`. The infallible
wrappers are `try_…().unwrap_or_default()`, so a profile rejection - input past
`max_length`, or a denied construct when the profile's action is `Error` - comes
back as an **empty string**, which a caller cannot tell from a document that
legitimately rendered to nothing.

`Profile` also carries a link policy; pair it with
`Options::with_profile_base_host` so the policy can tell internal links from
external ones.

An untrusted **AST payload** is bounded the same way, through the same profile:

```rust
let doc = carve::from_json(untrusted_payload)?;
let prepared = carve::prepare_document_for_render(
    doc,
    &options,
    carve::Mode::Interactive,
    true,
)?;
let html = carve::render_html_with_options(&prepared, &options)?;
```

`prepare_document_for_render` is where the profile applies on this path, and the
caps there are sized from the payload's measured length rather than from the
`srcByteLength` it carries - that number arrives inside the payload, so a hostile
tree could otherwise claim to have come from nothing and render anything, or
claim a gigabyte and widen its own expansion budget. `Document::source_len`
still reports the claim as written; `untrusted_input_len()` and
`expansion_budget_len()` report what may be trusted to size a cap.

Runnable version of all of the above, including what a rejection looks like:
`cargo run --example untrusted_input`. Full recipe, defaults and threat model:
[Security](https://markup-carve.github.io/carve/security).

## Extensions

Nine semantic inline names are built in and need no registration: `abbr`,
`cite`, `dfn`, `kbd`, `samp`, `var`, `time`, `code`, and `mark`.
`:name[content]{attrs}` remains an ordinary inline-extension AST node and maps
to the same-named HTML element; unknown names retain the generic
`<span class="ext-name">` fallback. Plain and ANSI render only the content.

The same registry has compact span-attribute sugar: `[Ctrl]{kbd}` and
`[HTML]{abbr="HyperText Markup Language"}`. Attributes can combine, for example
`[CSS]{dfn abbr="Cascading Style Sheets"}`; non-semantic attributes remain on
one outer span.
`:cite[…]` is distinct from bibliographic `[@key]` citations, and `:abbr[…]`
does not declare an automatic abbreviation.

Opt-in extensions implement `CarveExtension` and are passed through `Options`.
An extension can add inline/block matchers, run `after_parse` and
`before_render` AST transforms, and override renderers for extension nodes such
as `:kbd[Ctrl]`.

```rust
use carve::{CarveExtension, InlineExtension, Options, RenderContext};

struct Kbd;

impl CarveExtension for Kbd {
    fn name(&self) -> &'static str {
        "kbd"
    }

    fn render_inline_extension(
        &self,
        node: &InlineExtension,
        ctx: &RenderContext<'_>,
    ) -> Option<String> {
        (node.name == "kbd").then(|| {
            format!("<kbd>{}</kbd>", ctx.render_inlines(&node.children))
        })
    }
}

let kbd = Kbd;
let options = Options::new().with_extension(&kbd);
let html = carve::to_html_with_options("Press :kbd[Ctrl].", &options);
assert_eq!(html, "<p>Press <kbd>Ctrl</kbd>.</p>");
```

Locale-specific quote glyphs are also an opt-in extension:

```rust
use carve::{Options, SmartQuotes};

let quotes = SmartQuotes::new("de");
let options = Options::new().with_extension(&quotes);
let html = carve::to_html_with_options("\"Hallo\"", &options);
assert_eq!(html, "<p>„Hallo“</p>");
```

### Built-in extensions

The crate ships the same opt-in extensions as carve-js: `Autolink`,
`ExternalLinks`, `HeadingPermalinks`, `TableOfContents`, `Wikilinks`,
`TabNormalize`, `FencedRender` (with a Mermaid preset), `MathBlock`, `Spoiler`,
`Details`, and `ListTable`.

#### `Details`

`Details` renders `::: details` admonitions as the HTML5 `<details>/<summary>`
disclosure widget instead of the default `<div class="details">`. The quoted
title becomes the `<summary>` (a title-less block falls back to
`<summary>Details</summary>`); the title is flattened to escaped plain text.
Block attributes on the opener (`{#faq open}`) carry onto the `<details>` tag in
source order (the auto `details` class is dropped - the tag is already the
styling hook):

```rust
use carve::{Details, Options};

let ext = Details::new();
let opts = Options::new().with_extension(&ext);
let src = "::: details \"More info\"\nHidden _here_.\n:::";
assert_eq!(
    carve::to_html_with_options(src, &opts),
    "<details>\n  <summary>More info</summary>\n  <p>Hidden <u>here</u>.</p>\n</details>"
);
```

Without the extension, `::: details` stays a plain `<div class="details">`.

#### `Spoiler`

Hidden / blurred content revealed on interaction (the standard `spoiler` role).

- **Inline** `:spoiler[text]` → `<span class="spoiler">text</span>` (without the
  extension: generic `<span class="ext-spoiler">`).
- **Block** `::: spoiler "Title"` → `<details class="spoiler">` disclosure
  (native, accessible); title-less → `<summary>Spoiler</summary>` (without the
  extension: `<div class="spoiler">`).

```rust
use carve::{Spoiler, Options};

let ext = Spoiler::new();
let opts = Options::new().with_extension(&ext);
assert_eq!(
    carve::to_html_with_options("Plot: :spoiler[the butler did it].", &opts),
    "<p>Plot: <span class=\"spoiler\">the butler did it</span>.</p>"
);
```

Author attributes merge onto the marker (spoiler base class first) with the
always-on attribute hardening (`on*` / `srcdoc` / `formaction` stripped,
dangerous values neutralized), so a `{onclick="…"}` can never reach the output.

Carve emits only the marker; the blur / collapse + reveal is the host's CSS/JS.
Three host looks over the same markup (hover never reveals - it would spoil by
accident; content stays in the DOM for screen readers):

- inline `:spoiler[text]` → `<span class="spoiler">` styled as a **blur**;
- a generic `{.spoiler}` block div → `<div class="spoiler">` styled as a
  **blurred panel that keeps its space**, revealing on click;
- `::: spoiler` → `<details class="spoiler">` left as a **native collapse**
  (summary only, expands on click - no JS, fully accessible).

A `.masked` variant gives a credit-card / PIN look (`:spoiler[1234]{.masked}`).

```css
/* Inline: blurred until clicked. */
span.spoiler { filter: blur(.3em); cursor: pointer; border-radius: 3px; padding: 0 .15em;
  background: rgba(127, 127, 127, .14); user-select: none; transition: filter .2s; }
span.spoiler.revealed { filter: none; background: transparent; user-select: text; }
/* Credit-card / PIN variant ({.masked}): every char a dot. */
span.spoiler.masked { filter: none; -webkit-text-security: disc; }
span.spoiler.masked.revealed { -webkit-text-security: none; }
/* Block as a blurred panel that keeps its space (a generic {.spoiler} div). */
div.spoiler { filter: blur(.4em); cursor: pointer; border-radius: 8px; padding: 10px 14px;
  border-left: 3px solid #e0af68; user-select: none; transition: filter .25s; }
div.spoiler.revealed { filter: none; cursor: auto; user-select: text; }
/* Block as a native collapse (::: spoiler): summary only until clicked. */
details.spoiler { border-left: 4px solid #e0af68; border-radius: 8px; padding: 6px 14px; }
details.spoiler > summary { color: #e0af68; cursor: pointer; list-style: none; }
details.spoiler > summary::before { content: "👁 "; }
details.spoiler > summary::after { content: " (click to reveal)"; font-weight: 400; }
details.spoiler[open] > summary::after { content: ""; }
```

```js
// The two blur forms (inline span, block div) reveal on click / Enter / Space.
for (const el of document.querySelectorAll('span.spoiler, div.spoiler')) {
  el.tabIndex = 0; el.setAttribute('role', 'button');
  el.setAttribute('aria-label', 'Spoiler, activate to reveal');
  const toggle = () => el.classList.toggle('revealed');
  el.addEventListener('click', toggle);
  el.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); toggle(); }
  });
}
// `::: spoiler` → <details> is a native disclosure - it collapses/expands on its own.
```

#### `FencedRender`

Generic client-rendered fenced-block factory; Mermaid is one preset of it. It
claims fenced code blocks by language word and emits one hydration element; the
body is passed through verbatim. One factory covers Mermaid, D2, Graphviz,
WaveDrom, ABC, Vega-Lite, Chart.js, etc.

- **text mode** (Mermaid/D2/Graphviz/WaveDrom/ABC): escapes `&` and `<`, keeps
  `>` for arrow syntax.
- **json mode** (Vega-Lite/Chart.js): body verbatim inside
  `<script type="application/json">`, with `</` rewritten to `<\/`.

```rust
use carve::{FencedRender, Options};

let ext = FencedRender::d2();
let opts = Options::new().with_extension(&ext);
assert_eq!(
    carve::to_html_with_options("``` d2\na -> b\n```", &opts),
    "<pre class=\"d2\">a -> b</pre>"
);
```

Presets: `FencedRender::mermaid()`, `d2()`, `graphviz()` (claims `dot` +
`graphviz`), `wavedrom()`, `abc()`, `plantuml()` (claims `plantuml` + `puml`),
`vega_lite()`, `chart()`; or
`FencedRender::with_options` for a custom language set, `cssClass`, `tag`, or
content mode. `FencedRender::presets()` returns every preset as a `Vec` to
register in a loop (it claims every preset fence word, so register only those
whose client library you load if that matters). Author attributes
on the fence are copied onto the wrapper with the always-on hardening (`on*` /
`srcdoc` / `formaction` stripped, dangerous values neutralized), so a
`{onclick="…"}` fence can never reach the output.

> **Note:** PlantUML payload vs Mermaid. Both hydrate fully offline (load the
> file locally, no CDN). `@plantuml/core` is roughly **~2 MB gzipped** - about
> double Mermaid's **~0.95 MB** - because it bundles Graphviz (`viz.js`, ~0.6 MB
> gz) to lay out class / component / deployment diagrams; `plantuml.js` itself
> is ~1.4 MB gz. Sequence diagrams render to SVG without the layout engine, so a
> sequence-only page is lighter. Load PlantUML only on pages that use the UML
> types Mermaid cannot draw (use case, component, deployment, timing); prefer
> Mermaid where it suffices. (Sizes are the shipped browser builds, not npm's
> `unpackedSize`, which is dominated by source maps and inverts the comparison.)

> **Note:** json mode emits a `<script type="application/json">`. If you
> sanitize the HTML *after* converting, that inert script is usually stripped -
> whitelist `<script type="application/json">` in your sanitizer, or render the
> config in **text mode** so it rides in a `<pre>` as escaped text (read from
> `textContent`):
>
> ```rust
> use carve::{ContentMode, FencedRender, FencedRenderOptions, Options};
> // Text mode: config rides in <pre class="chart"> as escaped text and survives
> // HTML sanitizing (the json preset's <script> wrapper would be stripped).
> let chart = FencedRender::with_options(FencedRenderOptions::new(
>     vec!["chart".into()], Some("chart".into()), None, ContentMode::Text,
> ));
> let opts = Options::new().with_extension(&chart);
> ```

#### `MathBlock`

`MathBlock` renders a fenced code block tagged `math` (a ` ``` math ` fence) as
`<div class="math display">\[ … \]</div>`, the GFM-style block form of Carve's
core `$$` display math. The body is HTML-escaped and wrapped in `\[ … \]` for a
client-side math engine (KaTeX/MathJax). Non-`math` code blocks defer to the
core renderer.

```rust
use carve::{MathBlock, Options};

let ext = MathBlock::new();
let opts = Options::new().with_extension(&ext);
assert_eq!(
    carve::to_html_with_options("``` math\nx^2\n```", &opts),
    "<div class=\"math display\">\\[x^2\\]</div>"
);
```

A `{#eq .big key=val}` block-attribute line above the fence merges onto the
`<div>` exactly as core display `$$` math carries its attributes - the
`math display` base class ahead of author classes, then id and other attributes
in source order (class-first):

```text
{#eq .big data-ref=x}
``` math
x^2
```
→ <div class="math display big" id="eq" data-ref="x">\[x^2\]</div>
```

Attributes get the always-on hardening every element gets (`is_dangerous_attr_name`
strips `on*` / `srcdoc` / `formaction`; `sanitize_attr_value` neutralizes
dangerous URL / `expression()` values), so a `{onclick="…"}` on a fence can
never reach the output. This mirrors how core inline `` $`…` `` / display
`` $$`…` `` math carry their `{...}` attributes.

#### `ListTable`

`ListTable` (a Tier-3 extension) renders a `::: list-table` block authored as a
nested list into a real HTML `<table>`, so cells can hold full block content
(paragraphs, lists, code) that the native pipe-table syntax cannot express. The
outer list items are rows and the inner list items are cells:

```rust
use carve::{ListTable, Options};

let ext = ListTable::new();
let opts = Options::new().with_extension(&ext);
let src = "::: list-table \"Cap\"\n- - A\n  - B\n:::";
assert_eq!(
    carve::to_html_with_options(src, &opts),
    "<table>\n  <caption>Cap</caption>\n  <tbody>\n    <tr><td>A</td><td>B</td></tr>\n  </tbody>\n</table>"
);
```

The quoted title becomes the `<caption>`. `{header-rows=N}` / `{header-cols=N}`
block attributes on the PRECEDING line promote rows to `<thead>`/`<th>` and the
first N cells of each row to row-header `<th>`. A cell whose sole content is a
lone `^` merges with the cell above (rowspan) and a lone `<` merges with the cell
to the left (colspan), matching Carve's native pipe-table continuation markers,
so the output matches the equivalent pipe table. A cell carrying its own
attributes (`-{.x} ^`) is never a span marker - its `^`/`<` stays literal and the
attribute carries onto the `<td>`/`<th>`. A `list-table` that cannot be rendered
as a table (no usable nested list, or a row with no cell list) is left as the
default `<div class="list-table">` so content is never silently dropped.

See [docs/extensions.md](docs/extensions.md) for the full reference.

## CLI

The crate ships a `carve` binary that reads Carve source from a file or stdin
and writes the rendered output to stdout. HTML is the default; pass a format
flag for Markdown, plain text, or ANSI-colored terminal output.

Prebuilt binaries are attached to every release for macOS (Apple silicon and
Intel), Linux (glibc and musl) and Windows, so the CLI does not require a Rust
toolchain:

```bash
brew install markup-carve/carve/carve       # macOS and Linux

# or take the archive for your platform straight from the release page and put
# `carve` on your PATH: https://github.com/markup-carve/carve-rs/releases
```

Each archive ships a `.sha256` sidecar next to it. From source instead:

```bash
cargo install carve-lang                    # from crates.io
cargo install --path .                      # from a checkout
```

Then:

```bash
carve README.crv > README.html      # HTML (default, interactive)
carve --static README.crv           # self-contained HTML (print / PDF / archival)
carve --markdown README.crv         # Markdown
carve --plain README.crv            # plain text
carve --ansi README.crv             # ANSI-colored terminal text
echo '# Hello' | carve              # render from stdin
carve merge base.crv ours.crv theirs.crv # structural three-way merge
```

The library exports `merge_ast`, `merge_ast_with_resolver`, `create_ast_patch`,
and `apply_ast_patch` for the same workflow over typed `Document` values. A
resolver can select base, ours, theirs, or a JSON-encoded replacement for each
conflict. `ast_patch_to_json` and `ast_patch_from_json` exchange the same
`{op,path,value}` wire format as the JS and PHP engines. The merge combines
independent field edits, insertions, deletions, and moves, while unresolved
ambiguous edits are returned as JSON-Pointer conflicts. Derived position
metadata is intentionally regenerated after serialization.

Other options:

```bash
carve --mention-url '/users/{name}' --tag-url '/topics/{name}' social.crv
carve --symbol 'rocket=🚀' --symbol 'tada=🎉' symbols.crv
carve --no-raw-html untrusted.crv   # escape =html raw blocks/spans
carve --safe --profile comment untrusted.crv   # and restrict which constructs are allowed
carve --help
```

`--html` / `--markdown` (`--md`) / `--plain` (`--plain-text`) / `--ansi` select
the format (last one wins). `--mention-url` / `--tag-url` build HTML links and
apply to HTML output only. `--no-raw-html` (alias `--safe`) escapes `=html` raw
blocks and spans instead of emitting them verbatim, which is the safe choice when
rendering untrusted input; it composes with every format and with `--profile`.
`--profile NAME` (`full` | `article` | `comment` | `minimal`) restricts which
constructs are allowed at all and caps input length, and `--profile-base-host`
gives its link policy a host to judge internal vs external links against; see
[Untrusted input](#untrusted-input). `--static` (vs the default `--interactive`) renders
self-contained HTML: interactive constructs flatten (a `::: details` becomes an
expanded `<section>`) and client-script visuals (mermaid / chart / math) degrade
to source. Pass `--extensions` to enable the bundled interactive extensions
(details, spoiler, mermaid, chart, math) so `--static` has something to flatten;
without it the CLI parses those words as plain containers. Supplying build
renderers for the diagrams/math requires the library API (`Options::with_mode` +
`with_renderers`); see `docs/extensions.md` and `examples/static_mode.rs`.

## Building from source

```bash
git clone https://github.com/markup-carve/carve-rs
cd carve-rs
git submodule update --init   # pulls the spec corpus
cargo test
```

The spec corpus lives in `tests/spec/` as a git submodule of [`markup-carve/carve`](https://github.com/markup-carve/carve). Running `cargo test` without initializing the submodule will fail with a clear error message.

## For bindings that pin this engine

carve-rb, carve-py, carve-wasm and carve-go each pin a carve-rs revision - three
as a Cargo git dependency (the crate publishes as `carve-lang`, not `carve`), one
as a revision file beside a prebuilt wasm. `tools/check-engine-pin.py` is the
single reader for both shapes: it asserts the pin names a real commit that is an
ancestor of `main`, that a lockfile agrees with its manifest, and - optionally -
that a committed artifact matches the digest recorded beside its revision. The
lag behind `main` is reported as a number and never gates; `--max-age-days`
gates on age instead.

See [docs/engine-pin-guard.md](docs/engine-pin-guard.md) for the CI snippet each
binding should use and for what the guard cannot see.

## Design

- **Linear-time** parsing: block lexer reads line by line, inline scanner does a single linear pass with no backtracking.
- **Zero dependencies** in the runtime crate. Tests use only `std`.
- **Conformance via corpus**: every supported construct has a `.crv` / `.html` pair in the upstream spec. The Rust output must match the JS reference byte-for-byte (after trimming).

See `src/parse.rs` for the parser and `src/render.rs` for the renderer. The AST in `src/ast.rs` mirrors the shape of [`carve-js`'s `ast.ts`](https://github.com/markup-carve/carve-js/blob/main/src/ast.ts).

## License

MIT — see [LICENSE](LICENSE).
