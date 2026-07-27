# Changelog

All notable changes to carve-rs are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

- BREAKING: rename `Emoji` AST nodes to `Symbol`, the `emoji` render option to
  `symbols`, and the CLI flag `--emoji` to `--symbol`; symbol shortcodes now
  require a leading word boundary, require an ASCII alphanumeric first name
  character, and support trailing attrs via an HTML `<span>` wrapper.

### Fixed

- **A run of 2+ hyphens now decomposes into em/en dashes with no leftover
  literal hyphen** (canonical djot allocation, matching carve-js / carve-php /
  the executable-spec oracle). The old fixed longest-match table (capped at six
  hyphens, `------` -> em em, applied greedily) left a stray literal hyphen at
  N = 7, 13, ... (N is 1 mod 6) and mis-allocated at N = 8, 10, 13 - e.g. seven
  hyphens rendered as `——-` (two em plus a hyphen) instead of the canonical
  `—––` (one em plus two en). Allocation is now: all em when divisible by 3, all
  en when divisible by 2, otherwise as many em-dashes as fit with the remainder
  as en-dashes, where a remainder of 1 trades one em for two en. The Markdown,
  plain-text and ANSI renderers share the same per-position scan as the HTML
  path, so all outputs stay byte-parity; the arrow operators (`->`, `<-`, ...)
  still win at their own position, so `<-->` is two arrows and `------->` is a
  seven-hyphen run followed by a literal `>`.
- **An indented `{attr}` line and an indented image + `^ caption` pair now stay
  literal** (strict column-0 rule, `docs/divergence-from-djot.md` §11). A
  block-attribute line above its container's content column no longer attaches to
  the following block, and an indented image + caption no longer forms a
  `<figure>`; both fold as literal paragraph text, matching carve-php and
  carve-js. A flush-left `{attr}` line or image caption still fires unchanged.
- **An indented `::: |` line block or `::: \` hard-break block now stays
  literal** (strict column-0 rule, `docs/divergence-from-djot.md` §11). A colon
  fence recognized only at its container's content column (column 0 at the top
  level); an opener above that column no longer fires, so the whole run folds to
  paragraph text. The line-block and hard-break checks were missing the
  content-column gate the plain `:::` div / admonition path already had. A
  flush-left line block or hard-break block still opens unchanged. Matches
  carve-js.
- **A blank line inside an outer list item before an attached paragraph now
  loosens the outer item** even when a nested sublist precedes the blank. `- a`
  / `  - b` / blank / `   > q` renders the outer item loose
  (`<li><p>a</p>…</li>`), matching carve-js: the sublist collection swallowed the
  blank, so the outer item was left tight. Only an attached paragraph loosens; a
  flush-to-content-column block quote or content that nests into the inner item
  (corpus 142) leaves the outer item tight.
- **An under-indented definition line still attaches as a `<dd>`** (carve#295,
  PART 9 §24 C3). A `:  def` line below the term's content column - including at
  column 0 - now attaches to the open definition list inside a list item instead
  of orphaning to a top-level paragraph. A definition marker is a lenient
  exception to the column-0-exits rule: only a blank line before it (or a
  first-class opener such as a new `:: term` or a block opener) ends the entry.
  Lazy body text after the below-content definition folds into it, matching
  carve-php / carve-js.
- **A complete `{…}` line trailing a non-attribute brace no longer drops the
  line.** A line like `{k=v}{+i+}` (a valid attribute block immediately followed
  by critic markup or an empty/other non-attribute brace) was mis-read by the
  multi-line block-attribute joiner, which stripped the outer braces and parsed
  the interior `}{` as an unquoted value (`k="v}{+i+"`), swallowing the whole
  line to empty output. It now stays literal (`<p>{k=v}<ins>i</ins></p>`),
  matching the reference. The multi-line join now only applies to a block that
  genuinely continues onto later lines (`{#id` then `.foo}`).
- **Colon-fence `:::` obeys the content column in list items** (carve#295, PART 9
  §24 C3). A `:::` container (admonition / div / line block) below a list item's
  content column now folds as lazy paragraph text instead of nesting - the last
  block-opener kind still missing the content-column gate that quote / heading /
  table / def-list already had.
- **Definition lists and tables are first-class block openers in list items**
  (carve#295, PART 9 §24 C3). A `:: ` def-list term now interrupts at a list
  item's column 0 and nests at its content column, uniform with quote/heading/
  fence. A table row below or above the content column folds as lazy paragraph
  text instead of wrongly nesting, and an indented table row is a paragraph, not
  a table. Above-content lazy continuation lines no longer keep a residual indent.
- **Post-blank list continuation follows the content-column model** (carve#295,
  PART 9 §24 C3). A block opener or sublist marker must reach the parent item's
  content_column (`- `=2, `1. `=3, `10. `=4) to belong to the item: below it,
  a line after a blank ends the item and parses at document level (with no blank
  it lazily continues the item paragraph); at it, the opener nests; above it, the
  opener folds in as lazy paragraph text. The continuation boundary was keyed to
  a fixed `base_indent + 2`, so an ordered item's deeper body column was misjudged
  and a below-content block opener wrongly nested. Now aligned with the spec, the
  executable oracle, and carve-js/carve-php.
- **Never pad all-space verbatim content in `carve fmt`.** A verbatim span whose
  content is entirely spaces was padded by the serializer even though the parser
  correctly leaves it unstripped, so every fmt pass grew the span by two spaces
  (`` ` ` `` → `` `   ` `` → `` `     ` ``) and broke both formatter guarantees,
  `to_html(fmt(x)) == to_html(x)` and `fmt(fmt(x)) == fmt(x)`. Code spans, the
  inline literal and math were all affected. The serializer now pads exactly
  where the parser strips, and the closed and unclosed verbatim paths share one
  `strip_verbatim_padding` helper so they cannot drift apart.

### Added

- **Inline literal** via the `` !`…` `` prefix (#245): a `!` immediately before a
  verbatim backtick span renders its content as escaped prose with no `<code>`
  wrapper, so notation that collides with the bare emphasis delimiters (phonemic
  `/kaet/`, glob patterns, paths) needs no per-character escaping. Mirrors the
  `$`-math prefix; a trailing `{…}` is the ordinary attribute block.
- **PlantUML preset** and an open static-renderers map, so a build-time diagram
  renderer can be plugged in (#243); the CLI registers every `FencedRender`
  diagram preset under `--extensions` (#252).
- **Opt-in source-line tracking** for editor scroll-sync (#224), with nested
  blocks and list items stamped (#235).

### Fixed

- Static diagram output is wrapped in a uniform `<div>` (#246); `FencedRender`
  and `MathBlock` degrade to source rather than raw HTML in non-HTML renderers
  (#247).
- The definition prepass tracks list content columns (#248); a fence opener
  strips markers and the closer strips blockquote-only (#250).
- A fenced-code delimiter sits at its container's content column (#244).
- A sublist marker at the content column interrupts a continuation paragraph
  (#238).
- A trailing backslash at end of input is a hard break; a bare same-level `#`
  continues a heading (#234).
- A thematic break is a contiguous column-zero run only (#233).
- Definition-list djot parity: a blank line may separate a term from its
  definition (#229); terms fold continuation lines like a heading (#228);
  descriptions support the `:  +` first-block form (#227) and lazy continuation
  (#225); definition and footnote bodies continue like list items (#221).
- Untrusted comment/minimal presets are capped with a default `max_length`
  (#223).

### Changed

- Eliminated three quadratic scans in inline parsing, so pathological input
  parses in near-linear time (#220).
- The formatter preserves the authored list marker (#237) and keeps verbatim
  content byte-exact through document normalization (#231).

## [0.1.0] - YYYY-MM-DD

Initial release of **carve-rs**, a zero-dependency Rust crate and CLI for the
[Carve](https://github.com/markup-carve/carve) markup language. The crate is
byte-conformant with the carve-js reference implementation against the shared
spec corpus. Published on crates.io as **`carve-lang`** (the name `carve` was
already taken); the library is still imported as `use carve` and the CLI binary
is still `carve`.

### Core parsing and rendering

- Linear-time block lexer (line-by-line) and single-pass inline scanner with no
  backtracking; zero runtime dependencies
- Full Tier-1 feature set: headings (H1-H6), paragraphs, emphasis (`/italic/`,
  `*bold*`, `_underline_`, `~strikethrough~`, `^super^`, `,sub,`, `=highlight=`,
  `/*bold-italic*/`), blockquotes with attribution captions, unordered and ordered
  lists, task lists, tables (with colspan/rowspan), inline code and fenced code
  blocks, images (inline and block with captions), horizontal rules, hard breaks,
  YAML frontmatter, admonitions (`::: note`/`tip`/`warning`/`danger`), abbreviations
  (`*[ABBR]:`), mentions (`@user`), hashtags (`#tag`), display and inline math
  (`$$`/`` $` ``), inline extensions (`:type[...]`), attribute blocks (`{#id .class
  key=val}`), raw HTML passthrough (`=html`), comment lines (`%%`), and reference
  links/images
- Inline footnotes (`^[...]`) and block footnote definitions
- Editorial / critic markup (`{+ +}` insert, `{- -}` delete,
  `{~ old~>new ~}` substitute, `{= =}` highlight, `{# #}` comment)
- Smart typography: curly quotes, em/en dashes, ellipsis
- HTML renderer (`carve::to_html` / `carve::render_html`)
- Markdown renderer (`carve::to_markdown`), plain-text renderer
  (`carve::to_plain_text`), ANSI-colored renderer (`carve::to_ansi`)
- Static render mode (`Options::with_mode(Mode::Static)`) for self-contained
  HTML without client-side scripts; interactive constructs degrade gracefully
- Reference definitions collected inside list items and containers

### Extension API

- `CarveExtension` trait with inline/block matchers, `after_parse` and
  `before_render` AST transforms, and per-node renderer overrides
- `Options::with_extension` and `Options::with_extensions` for composing
  extension sets; `Options::with_mode` and `Options::with_renderers` for static
  build-time rendering

### Tier-2 opt-in extensions

- `MathBlock` - fenced ` ```math ` block as `<div class="math display">` with
  author-attribute passthrough; mirrors core `$$` display math
- `citations` - `[@key]` reference citations with typed locators, explicit
  suffixes, and integral/group-level markers (§22); wired into `--extensions`
  CLI bundle
- `CodeCallouts` - annotated callout markers inside fenced code blocks; wired
  into `--extensions` CLI bundle

### Tier-3 opt-in extensions

- citations `bibliography` option - supplying a CSL-JSON pool renders a
  cite-ordered `<ol>` with back-links (a citations capability, not a standalone
  extension)
- `Glossary` + `Index` - `::: glossary` / `:term[word]` and `:index[term]` /
  `::: index` with back-links
- `HeadingNumbers` - section auto-numbering and numbered cross-references
- `ColorSwatch` - `:color[value]` inline chip; CSS named-color validation;
  configurable position, shape, tint; auto-contrast label variant
- `Spoiler` - `:spoiler[text]` inline and `::: spoiler` native `<details>`
- `Details` - `::: details "Title"` as HTML5 `<details>/<summary>`
- `FencedRender` - client-render factory with presets for Mermaid, D2, Graphviz
  (`dot`/`graphviz`), WaveDrom, ABC, Vega-Lite, Chart.js; text and json content
  modes; `FencedRender::presets()` helper
- `ListTable` - `::: list-table` nested-list-to-table with header-rows/cols and
  span markers
- `TableOfContents`, `HeadingPermalinks`, `Autolink`, `ExternalLinks`,
  `Wikilinks`, `TabNormalize` - standard document-enhancement extensions

### CLI

- `carve` binary reading Carve from a file or stdin, writing rendered output to
  stdout
- `--html` (default) / `--markdown` (`--md`) / `--plain` / `--ansi` format
  selection
- `--static` for self-contained HTML (interactive constructs flattened; compose
  with `--extensions` to enable the bundled interactive extension set)
- `--no-raw-html` (`--safe`) escapes `=html` raw blocks/spans for untrusted input
- `--extensions` enables the bundled interactive extension bundle (details,
  spoiler, mermaid, chart, math, citations, code-callouts)
- `--mention-url` and `--tag-url` for `@mention` and `#tag` link templates
- `--symbol` for custom symbol shortcode expansion (e.g. emoji)
- `carve fmt` - canonical formatter (semantic-preserving, `-w` in-place,
  `--check` CI gate)

### Security (always-on, §25-§26)

- URL scheme denylist covering `javascript:`, `data:`, `vbscript:`, and OS
  protocol-handler schemes
- Dangerous attribute stripping (`on*`, `srcdoc`, `formaction`) on all elements
- CSS `expression()` and `url()` neutralization in style attributes
- Trojan-Source hardening: NFC normalization of heading/footnote ids (via
  dependency-free `unicode_nfc.rs`); bidi and zero-width Unicode control
  characters stripped from text and code content (§26)
- Uniform nesting depth cap of 200
- Char-boundary panic guard in container-prefix stripping (crash-DoS fix)

[Unreleased]: https://github.com/markup-carve/carve-rs/compare/0.1.0...HEAD
[0.1.0]: https://github.com/markup-carve/carve-rs/releases/tag/0.1.0
