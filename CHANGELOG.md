# Changelog

All notable changes to carve-rs are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - YYYY-MM-DD

Initial release of **carve-rs**, a zero-dependency Rust crate and CLI for the
[Carve](https://github.com/markup-carve/carve) markup language. The crate is
byte-conformant with the carve-js reference implementation against the shared
spec corpus.

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

- `Bibliography` - CSL-JSON pool with cite-ordered `<ol>` and back-links
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
- `--emoji` for custom emoji token expansion
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

[Unreleased]: https://github.com/markup-carve/carve-rs/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/markup-carve/carve-rs/releases/tag/v0.1.0
