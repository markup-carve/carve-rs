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

Client-script extensions (mermaid, chart, math) cannot produce their image
inside the engine. A static render therefore accepts a **renderers** map of
boxed closures keyed by extension. When the needed renderer is absent, the
static path falls back to source - never blank.

```rust
use carve::{Mode, Options, StaticRenderers};
let opts = Options::new()
    .with_mode(Mode::Static)
    .with_renderers(StaticRenderers {
        // src -> SVG / <img> for mermaid diagrams
        mermaid: Some(Box::new(|src: &str| pre_render_mermaid(src))),
        // config src -> SVG / <img> for charts
        chart: Some(Box::new(|src: &str| pre_render_chart(src))),
        // (tex, display) -> MathML / HTML for server-side math
        math: Some(Box::new(|tex: &str, display: bool| ssr_math(tex, display))),
    });
```

`mermaid` / `chart` are `Box<dyn Fn(&str) -> String>` (`DiagramRenderer`);
`math` is `Box<dyn Fn(&str, bool) -> String>` (`MathRenderer`), where the `bool`
is `true` for display math. Renderer output is trusted and emitted verbatim.

### Per-extension static output

carve-rs ships Details, Spoiler, FencedRender (mermaid / chart presets) and
MathBlock. It has no Tabs / CodeGroup extension - those exist in carve-js and
carve-php only - so the labeled-section flattening for tab groups is provided by
the **core caption floor**: an unconsumed grouping `[label]` renders as a
`<p class="div-label">` caption in every target (the floor that also covers a
bare labeled `:::` div).

| Extension | Interactive HTML | Static HTML |
| --- | --- | --- |
| Details (`::: details`) | `<details><summary>T</summary>…</details>` | `<section class="details"><h3 class="details-title">T</h3>…</section>` (a `[label]` follows the title as a `<p class="div-label">`) |
| Spoiler inline (`:spoiler[x]`) | `<span class="spoiler">x</span>` | `<span class="spoiler spoiler-revealed">x</span>` |
| Spoiler block (`::: spoiler`) | `<details class="spoiler"><summary>T</summary>…</details>` | `<section class="spoiler spoiler-revealed"><h3 class="spoiler-title">T</h3>…</section>` |
| FencedRender mermaid | `<pre class="mermaid">…</pre>` (client-hydration) | `renderers.mermaid` output, else `<pre class="mermaid"><code class="language-mermaid">…\n</code></pre>` (source, fence attrs preserved) |
| FencedRender chart | `<div class="chart"><script type="application/json">…</script></div>` | `renderers.chart` output, else `<pre class="chart"><code class="language-chart">…\n</code></pre>` (no `<script>`) |
| FencedRender other presets (d2, graphviz, …) | `<pre class="lang">…</pre>` | always source `<pre><code>` (no build renderer) |
| MathBlock (` ```math `) | `<div class="math display">\[…\]</div>` | `renderers.math(src, true)` output inside the div, else the same `\[…\]` source |
| Core inline / display math (`$…$` / `$$…$$`) | `<span class="math {inline,display}">\(…\)</span>` | `renderers.math(src, display)` output inside the span, else the same `\(…\)` / `\[…\]` source |

In static HTML the resolution order per node is: the extension's static path if
defined; else its ordinary renderer (correct for already-static extensions like
ListTable, which need no static path); else the core caption floor for any
unconsumed grouping `[label]`. No authored token is ever dropped.

> Note on cross-impl parity: this branch follows the carve-js shapes
> (`<section class="details">`, `spoiler-revealed`). carve-php degrades Details
> / Spoiler natively (it keeps the `<details>`/`<summary>` disclosure in static
> mode rather than flattening to a `<section>`); the two will be reconciled when
> the spec PR lands.

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
