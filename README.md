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

## Status

The crate passes every `.crv` / `.html` pair currently checked into its
`tests/spec` submodule. The conformance test suite includes an all-pairs gate so
new corpus pairs fail CI until the parser and renderer support them.

| Pair | Construct | Status |
|---|---|---|
| 01-emphasis | `/italic/`, `*bold*`, `_underline_`, `~strike~`, `^super^`, `,sub,`, `=hl=`, `/*bi*/` | passing |
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

## Extensions

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
`graphviz`), `wavedrom()`, `abc()`, `vega_lite()`, `chart()`; or
`FencedRender::with_options` for a custom language set, `cssClass`, `tag`, or
content mode. `FencedRender::presets()` returns every preset as a `Vec` to
register in a loop (it claims every preset fence word, so register only those
whose client library you load if that matters). Author attributes
on the fence are copied onto the wrapper with the always-on hardening (`on*` /
`srcdoc` / `formaction` stripped, dangerous values neutralized), so a
`{onclick="…"}` fence can never reach the output.

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
flag for Markdown, plain text, or ANSI-colored terminal output:

```bash
cargo install --path .

carve README.crv > README.html      # HTML (default, interactive)
carve --static README.crv           # self-contained HTML (print / PDF / archival)
carve --markdown README.crv         # Markdown
carve --plain README.crv            # plain text
carve --ansi README.crv             # ANSI-colored terminal text
echo '# Hello' | carve              # render from stdin
```

Other options:

```bash
carve --mention-url '/users/{name}' --tag-url '/topics/{name}' social.crv
carve --emoji 'rocket=🚀' --emoji 'tada=🎉' emoji.crv
carve --no-raw-html untrusted.crv   # escape =html raw blocks/spans
carve --help
```

`--html` / `--markdown` (`--md`) / `--plain` (`--plain-text`) / `--ansi` select
the format (last one wins). `--mention-url` / `--tag-url` build HTML links and
apply to HTML output only. `--no-raw-html` (alias `--safe`) escapes `=html` raw
blocks and spans instead of emitting them verbatim, which is the safe choice when
rendering untrusted input; it composes with every format and with `--profile`. `--static` (vs the default `--interactive`) renders
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

## Design

- **Linear-time** parsing: block lexer reads line by line, inline scanner does a single linear pass with no backtracking.
- **Zero dependencies** in the runtime crate. Tests use only `std`.
- **Conformance via corpus**: every supported construct has a `.crv` / `.html` pair in the upstream spec. The Rust output must match the JS reference byte-for-byte (after trimming).

See `src/parse.rs` for the parser and `src/render.rs` for the renderer. The AST in `src/ast.rs` mirrors the shape of [`carve-js`'s `ast.ts`](https://github.com/markup-carve/carve-js/blob/main/src/ast.ts).

## License

MIT — see [LICENSE](LICENSE).
