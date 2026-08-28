# Extensions

Opt-in extensions implement `CarveExtension` and are passed through `Options`.
This page documents the built-in extensions in more depth than the README. See
the README's `## Extensions` section for the general extension model and the
short list of all built-ins.

## Bibliography (Citations + CSL-JSON)

The `Citations` extension (Tier-2) parses `[@key]` citations and renders a
references list from in-document `[@key]:` definitions. Attaching an external
**CSL-JSON pool** with `with_bibliography` turns on the Tier-3 Bibliography
behavior (spec §6, issue #199): keys resolve against in-document defs first,
then the pool, and the in-text citations plus the references list gain
footnote-style back-links.

The extension does no file I/O or JSON parsing - the host resolves the
front-matter `bibliography:` path, parses the CSL-JSON, and passes the entries
in as plain `CslEntry` values:

```rust
use carve::{Citations, CslDate, CslEntry, CslName, Options};

let pool = vec![CslEntry {
    id: "smith2020".to_string(),
    author: Some(vec![CslName {
        family: Some("Smith".to_string()),
        given: Some("John".to_string()),
        literal: None,
    }]),
    issued: Some(CslDate { date_parts: Some(vec![vec![2020]]), literal: None }),
    title: Some("A Study".to_string()),
}];
let ext = Citations::new().with_bibliography(pool);
let opts = Options::new().with_extension(&ext);
let html = carve::to_html_with_options("See [@smith2020].", &opts);
// in-text: <a id="cite-smith2020-1" href="#ref-smith2020">1</a>
// entry:   <li id="ref-smith2020">Smith, John (2020). A Study.
//            <a href="#cite-smith2020-1" class="ref-backref">↩</a></li>
```

A CSL-JSON entry renders with the minimal fixed template
`Family, Given (Year). Title.` (authors joined with `; `, any missing field and
its separator omitted, trailing period). The entry text is plain - HTML-escaped,
never re-parsed as Carve. In-document `[@key]:` definitions keep their inline
rendering and win over a pool entry with the same key. Back-links appear only
when a pool is supplied; plain Tier-2 citations are byte-identical to before.
Resolving an arbitrary `.csl` style is out of scope (a renderer-plugin point).

### The definition line in the tree

A `[@key]: {author="Smith" year="2020"} entry` line is a `citation_definition`
block node (PART 12 §18), produced by `parse_with_options` whenever the
extension is on:

```json
{
  "type": "citation_definition",
  "key": "smith2020",
  "children": [{ "type": "text", "value": "Smith, J. (2020). A Study. Pub." }],
  "attrs": { "keyValues": { "author": "Smith", "year": "2020" } },
  "pos": { "startLine": 3, "endLine": 3, "startColumn": 1, "endColumn": 78 }
}
```

`key` is the citation key without the `@` - the same string `citation.key`
carries at the use site. `children` is the entry's INLINE content: a footnote
body holds blocks and this does not, which is why the node is shaped after the
link reference definition. `attrs` holds the leading metadata block when the
line carries one, and is absent otherwise.

The node renders nothing where it sits on every target; the entry's text
renders in the references list below. Tier-2: with the extension off the line
is ordinary paragraph text and no `citation_definition` is produced.

The generated citation ids (`cite-{key}-{n}` use-site anchors, `ref-{key}`
reference entries) are deduplicated against the document id namespace
(extensions contract §2.6): when an explicit `{#id}` attribute or a generated
heading id already uses a name, the citation id takes the next free suffix
(`ref-foo-2`) instead of emitting a duplicate DOM id, and every href /
back-link follows. Custom extensions can reserve their own generated ids in
the same namespace through `RenderContext::unique_id`.

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

## Glossary

`Glossary` is a Tier-3 extension (#91) that renders a `::: glossary` definition
list as a `<dl class="glossary">` whose terms carry linkable `gloss-{slug}` ids,
and links every `:term[word]` use to the matching entry. It reuses existing
syntax (the definition list and the `:name[…]` inline form), so there is no new
markup. Off by default; register it explicitly.

```rust
use carve::{Glossary, Options};

let ext = Glossary::new();
let opts = Options::new().with_extension(&ext);
let src = "Use :term[HTTP].\n\n::: glossary\n:: HTTP\n:  HyperText Transfer Protocol.\n:::";
let html = carve::to_html_with_options(src, &opts);
assert!(html.contains("<a href=\"#gloss-http\" class=\"term\">HTTP</a>"));
assert!(html.contains("<dt id=\"gloss-http\">HTTP</dt>"));
```

- The id slug is the heading-id slug of the term's plain text, lowercased
  (`HTTP` -> `gloss-http`); `:term[word]` slugs its own bracket text the same
  way, so the two sides meet without a separate key.
- The `<dl>` renders in source order (no sort); on a duplicate slug the first
  entry wins the id. A single-paragraph definition collapses to inline content.
- `:term[word]` with no matching entry degrades to `<span class="term">word</span>`
  (no link). With the extension off it is the generic `<span class="ext-term">`.
- Authored attributes carry through: a preceding `{#id .class}` line lands on the
  (first) `<dl>` (`glossary` stays the leading class); inline `:term[x]{.c #i}`
  attributes ride on the output, and a duplicate author `href` is dropped so the
  resolved link has exactly one. Non-definition-list content inside the block is
  preserved in place; a `::: glossary` nested in a blockquote / list / div is
  found too.

## Index

`Index` is a Tier-3 extension (#91) that collects invisible `:index[term]`
markers into a `::: index` block - a sorted `<ul class="index">` with one
back-link per occurrence. Off by default; register it explicitly. Pairs with but
is independent of `Glossary`.

```rust
use carve::{Index, Options};

let ext = Index::new();
let opts = Options::new().with_extension(&ext);
let src = "A :index[parser] here.\n\n::: index\n:::";
let html = carve::to_html_with_options(src, &opts);
assert!(html.contains("<span id=\"idx-parser-1\" class=\"index-term\"></span>"));
assert!(html.contains("<a href=\"#idx-parser-1\" class=\"index-backref\">"));
```

- Each body `:index[term]` emits an empty `<span id="idx-{slug}-{n}"
  class="index-term">` anchor target (`n` is that slug's 1-based occurrence in
  document order). A span, not an `<a>`, so a marker inside a link label never
  nests one anchor in another.
- `::: index` renders `<ul class="index">`, one `<li>` per distinct slug sorted
  by Unicode codepoint, each with a `↩` back-link per occurrence. With no markers
  it stays the plain `<div class="index">`; authored content inside the block is
  preserved before the list, and a preceding `{#id .class}` line lands on the
  `<ul>`.
- Only body markers are indexed: a `:index` inside deferred content (a footnote
  definition) renders inert (`<span class="index-term">`, no id) so a back-link
  never dangles. With the extension off, `:index[term]` is the generic
  `<span class="ext-index">term</span>`.

## HeadingNumbers

`HeadingNumbers` is a Tier-3 extension (#198) that auto-numbers sections and
rewrites auto-filled `</#id>` cross-references to "Section 1.2 - Title". Render
policy, not source semantics: off by default, no new syntax. Register it
explicitly.

```rust
use carve::{HeadingNumbers, Options};

let ext = HeadingNumbers::new();
let opts = Options::new().with_extension(&ext);
let src = "# Parsing\n\nSee </#Parsing>.";
let html = carve::to_html_with_options(src, &opts);
assert!(html.contains("<span class=\"section-number\">1</span> Parsing"));
assert!(html.contains("<a href=\"#Parsing\">Section 1 - Parsing</a>"));
```

- Numbers headings gap-free (`1`, `1.1`, `1.2`) in document order and prepends a
  `<span class="section-number">` inside each `<h*>`; the id is unchanged. Skips
  blockquote-quoted and `{.unnumbered}` headings (the class from a preceding
  attribute line). `min_level` (default 1) sets the top numbered level - set 2
  when `#` is the doc title.
- Rewrites only `</#id>`-origin links - identified by the non-rendered
  `Link::from_crossref` flag set during cross-reference resolution - so ordinary
  `[text](#id)` links and implicit `[label][]` references keep their text.
  `HeadingNumbersOptions::crossref` (`Number` / `NumberTitle` (default) /
  `Title`) and `label` (default `"Section"`) tune the output.
- With the extension off, headings and cross-references render unchanged. The
  `from_crossref` flag is metadata only - it changes no rendered output, so the
  conformance corpus is unaffected.

## Static rendering mode

A render carries a **mode** - a render option, not document syntax (see the
[normative extensions contract](https://markup-carve.github.io/carve/extensions)
§2.5 and the [graceful-degradation page](https://markup-carve.github.io/carve/graceful-degradation)):

- `Mode::Interactive` (default) - online HTML; extensions render their
  interactive form (live `<details>` disclosures, mermaid via a client script,
  KaTeX-ready math).
- `Mode::Static` - self-contained HTML for a medium that cannot interact or run
  client scripts (print, PDF source, archival HTML). Interactive constructs
  flatten and client-script visuals degrade to a build renderer's output or to
  source.

```rust
use carve::{Mode, Options};
let opts = Options::new().with_mode(Mode::Static);
```

`Mode` is a closed enum, so an unknown mode value (a future `"print"` /
`"email"` preset) is unrepresentable - the spec's "MUST reject an unknown mode"
is satisfied by construction. Omitting the mode leaves `Mode::Interactive`, so
existing callers are unaffected. The mode only affects the **HTML** renderer;
the Markdown, plain-text and ANSI renderers are inherently static and reach the
same end by flattening containers and keeping client-script blocks as source.

The CLI exposes this as `carve --static file.crv` (and `--interactive`, the
default). A runnable end-to-end demo lives in `examples/static_mode.rs`
(`cargo run --example static_mode`): it renders one document interactive, static
with source fallback, and static with build renderers.

### The renderers map

Client-script extensions (mermaid, chart, plantuml, math, …) cannot produce
their image inside the engine. A static render therefore accepts a **renderers**
map. It is **open**: a diagram renderer is keyed by the fence's css class, so a
custom `FencedRender` instance is static-capable with no change to the type.
When the needed renderer is absent, the static path falls back to source - never
blank.

```rust
use carve::{Mode, Options, StaticRenderers};
let opts = Options::new()
    .with_mode(Mode::Static)
    .with_renderers(
        StaticRenderers::new()
            // src -> SVG / <img>, keyed by fence css class
            .diagram("mermaid", |src: &str| pre_render_mermaid(src))
            .diagram("chart", |src: &str| pre_render_chart(src))
            // a custom fence word works the same way, no engine change:
            .diagram("myuml", |src: &str| pre_render_myuml(src))
            // (tex, display) -> MathML / HTML for server-side math
            .math(|tex: &str, display: bool| ssr_math(tex, display)),
    );
```

Diagram renderers are `Box<dyn Fn(&str) -> String>` (`DiagramRenderer`) held in
an open `diagrams: HashMap<String, _>` keyed by css class; `math` is
`Box<dyn Fn(&str, bool) -> String>` (`MathRenderer`), where the `bool` is `true`
for display math. Renderer output is trusted and emitted verbatim.

### Per-extension static output

carve-rs ships **24 extension modules under 32 registry keys**. The list is not
maintained by hand here - it is `carve::extensions::registry::keys()`, and
`tests/the_extension_list_in_the_docs_is_the_registry.rs` fails if this block
and the registry disagree:

<!-- registry-keys: derived from carve::extensions::registry::keys() -->

```
autolink, citations, code-callouts, code-group, color-swatch, details,
external-links, fenced-render, fenced-render-abc, fenced-render-chart,
fenced-render-d2, fenced-render-graphviz, fenced-render-plantuml,
fenced-render-vega-lite, fenced-render-wavedrom, glossary, heading-level-shift,
heading-numbers, heading-permalinks, heading-reference, img-fence, index,
list-table, math-block, semantic-span, smart-quotes, spoiler, tab-normalize,
table-of-contents, tabs, toc, wikilinks
```

Six of them render differently under `Mode::Static` - the six that ask the
render context for the mode (`RenderContext::is_static`): **Details, Spoiler,
Tabs, CodeGroup, FencedRender and MathBlock**. Every other extension in the list
renders identically in both modes, which is resolution step 2 below.

**Tabs and CodeGroup are two of the six, and each flattens its own group.** This
paragraph used to say carve-rs had no Tabs or CodeGroup extension, and used that
to explain the flattening as the work of the core caption floor. Both halves
were false: `src/extensions/tabs.rs` and `src/extensions/code_group.rs` have
been on `main` since #906, each carries a static arm of its own, and a
REGISTERED Tabs or CodeGroup therefore never reaches the floor. What the floor
covers is a grouping `[label]` no registered extension consumed - resolution
step 3, not a stand-in for a missing extension. Measured on one document, with
the extension registered and without it:

`:::: tabs` with `Tabs` registered, `Mode::Static`:

```html
<div class="tabs" role="group" aria-label="Tabs">
  <section class="tabs-panel">
  <h3 class="tabs-label">Rust</h3>
<p>rust body</p>
  </section>
</div>
```

the same document with no extension registered, `Mode::Static` - the core
caption floor:

```html
<div class="tabs">
  <div class="tab">
    <p class="div-label">Rust</p>
    <p>rust body</p>
  </div>
</div>
```

| Extension | Interactive HTML | Static HTML |
| --- | --- | --- |
| Details (`::: details`) | `<details><summary>T</summary>…</details>` | `<details open><summary>T</summary>…</details>` - the disclosure is KEPT and forced open, not flattened to a `<section>`. A `[label]` is ignored, which is what the spec's title-vs-label table says for details: it has no group to name |
| Tabs (`:::: tabs`) | `<div class="tabs" role="group">` holding an `<input type="radio" class="tabs-radio">` and `<label class="tabs-label">` per tab, then a `<div class="tabs-panel" role="group" aria-label="…">` per panel (`TabsMode::Css`); `role="tablist"` with `<button type="button">` under `TabsMode::Aria` | `<section class="tabs-panel"><h3 class="tabs-label">…</h3>…</section>` per panel, no radios and neither mode - there is no interaction left to describe |
| CodeGroup (`:::: code-group`) | the same shape with `code-group`, `code-group-radio`, `code-group-label` and `code-group-panel` | `<section class="code-group-panel"><h3 class="code-group-label">…</h3><pre><code>…</code></pre></section>` per panel |
| Spoiler inline (`:spoiler[x]`) | `<span class="spoiler">x</span>` | `<span class="spoiler spoiler-revealed">x</span>` |
| Spoiler block (`::: spoiler`) | `<details class="spoiler"><summary>T</summary>…</details>` | `<section class="spoiler spoiler-revealed"><h3 class="spoiler-title">T</h3>…</section>` |
| FencedRender mermaid | `<pre class="mermaid">…</pre>` (client-hydration) | `renderers.mermaid` output, else `<pre class="mermaid"><code class="language-mermaid">…\n</code></pre>` (source, fence attrs preserved) |
| FencedRender chart | `<div class="chart"><script type="application/json">…</script></div>` | `renderers.chart` output, else `<pre class="chart"><code class="language-chart">…\n</code></pre>` (no `<script>`) |
| FencedRender other presets / custom (d2, graphviz, plantuml, `myuml`, …) | `<pre class="lang">…</pre>` | `renderers.diagram("lang")` output if supplied (keyed by css class), else source `<pre><code>` |
| MathBlock (` ```math `) | `<div class="math display">\[…\]</div>` | `renderers.math(src, true)` output inside the div, else the same `\[…\]` source |
| Core inline / display math (`$…$` / `$$…$$`) | `<span class="math {inline,display}">\(…\)</span>` | `renderers.math(src, display)` output inside the span, else the same `\(…\)` / `\[…\]` source |

In static HTML the resolution order per node is: the extension's static path if
defined; else its ordinary renderer (correct for already-static extensions like
ListTable, which need no static path); else the core caption floor for any
unconsumed grouping `[label]`. No authored token is dropped that some other
mode would have kept - static output is never lossier than interactive output
for the same registration.

> Note on cross-impl parity: the Spoiler shapes follow carve-js
> (`spoiler-revealed`). Details does NOT: carve-rs keeps the
> `<details>`/`<summary>` disclosure and forces `open`, which is what carve-php
> does, and it has done so since the static-mode PR (#143) - the sentence that
> used to claim a `<section class="details">` here was never true of this
> engine.

## CodeCallouts

`CodeCallouts` is a Tier-2 extension (#88) for AsciiDoc-style annotations inside
fenced code. A `<n>` (ASCII digits) that is the last non-whitespace content on a
fenced-code line renders as `<b class="callout" data-callout="n">n</b>` (only the
marker is not HTML-escaped); a host hides it from copy with CSS. An immediately
following paragraph whose every soft-break line is `<n> text` binds as
`<ol class="callouts">` with one `<li value="n">` per line (the explicit `value`
matches the bubble, so non-sequential numbers stay aligned).

```rust
use carve::{CodeCallouts, Options};

let ext = CodeCallouts::new();
let options = Options::new().with_extension(&ext);
```

- The list binds only when the code block has at least one marker and every
  following line is a `<n> text` item; otherwise the `<n>` stay literal.
- Markers render independent of any list; only the trailing `<n>` per line is a
  marker. Authored block attributes ride onto the `<ol>` (`callouts` leading
  class).
- Off by default, optional-corpus pinned. HTML-only: non-HTML targets keep the
  `<n>` literal (source-faithful). Byte-identical to carve-js.

## Built-in extensions

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


