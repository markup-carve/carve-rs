# Changelog

All notable changes to carve-rs are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- **An implicit `[Heading][]` reference no longer resolves into a blockquote**
  (#410, spec PART 11 R1). Quoted text names the quoted document's headings,
  not this one's, so a heading with a blockquote ancestor is declined from the
  implicit-reference index - in either nesting order, a quote inside a div and
  a div inside a quote alike.

  The cause was one index serving two lookups. A `</#id>` crossref DOES resolve
  into quoted material, because it addresses a heading by id rather than by
  wording, and the implicit path shared that index and inherited its inclusion.
  The quoted ids are now recorded, and only the reference lookup declines them;
  crossrefs are unchanged, which the tests pin.

  Found by the combinatorial check in markup-carve/carve#452: the corpus
  covered implicit references and covered headings in blockquotes, and never
  put both in one document, so three engines declined and this one resolved
  with every suite green.

### Added

- **`Options::with_sections(false)` renders headings without the `<section>`
  wrapper** (markup-carve/carve#427, spec PART 9 §13). The id goes back on the
  `<h*>` alongside its other attributes, and the blocks that would have been
  section children stay as siblings. Default unchanged, so existing output is
  byte-identical.

  The wrapper is the one output change that breaks a site whose source migrated
  cleanly: CSS and JS assuming rendered blocks are direct children of the
  content container stop matching once a `<section>` sits in between. The
  endnotes `<section role="doc-endnotes">` is a different construct and is
  still emitted.

### Fixed

- **A profile's `admonition` deny list now matches only the eight Tier-1
  callout kinds, not every named fence** (markup-carve/carve#431). The
  renderer already drew this line - `::: sidebar` has always rendered a plain
  `<div>`, not `<aside>` - but `canonical_block_type` classified every
  `::: kind` fence as `admonition` for profile purposes regardless of kind, so
  `deny_block(["admonition"])` silently stripped `::: sidebar` and any other
  non-Tier-1 fence too. Only `note`, `tip`, `warning`, `danger`, `info`,
  `success`, `example` and `quote` are callouts now; any other named fence
  (`::: sidebar`, `::: details` before its extension rewrite applies, etc.) is
  classified as `div`.

  `deny_block(["div"])` still catches every admonition, Tier-1 or not,
  through the existing supertype rule - so this narrows `admonition` alone
  without opening a gap. A host that wants the old blanket behavior back
  denies both `["admonition", "div"]`.

  This is a profile-classification change only: the published AST is
  unaffected, and `::: sidebar` still serializes as `{"type":"admonition",
  "kind":"sidebar"}`, matching carve-js and `resources/ast-schema.json`.
  carve-php made the matching change in markup-carve/carve-php#513.

- **An unwrapped heading no longer puts its id before the author's attributes**
  (spec PART 10 §1). This engine wrote the id first in every case, so
  `{a=b .c}` on a heading inside a blockquote rendered
  `<h1 id="Auto" a="b" class="c">` where carve-js and carve-php both render
  `<h1 a="b" class="c" id="Auto">`. Authored attributes now keep their source
  order and a generated id joins at the end; an id the author wrote stays in
  its authored slot.

  All three engines disagreed here and none could be wrong, because the only
  way to reach the code was a heading inside a container and no corpus case
  gave such a heading attributes. carve-js is canonical. The `sections` switch
  is what forced the question: with it off every heading takes that path.

  `data-source-line` stays last. This engine stamps it as an ordinary
  key-value at parse time, so it rides along in the authored run; it is a
  render annotation rather than an authored attribute, and the generated id
  belongs before it (`<h2 id="Nested" data-source-line="4">`).

- **An auto heading slug no longer collides with an explicit `{#id}`.**
  `{#API-2}` on one heading plus a later `# API` emitted `id="API-2"` twice -
  invalid HTML, where every `#API-2` anchor resolves to the first match and the
  second heading is unreachable. A heading's own explicit id was reserved but
  never RECORDED as explicit, so the guard that skips a claimed id could not see
  it. The cross-reference index carried a third copy of the numbering rule with
  no skip at all, so `</#api-2>` resolved to the right id carrying the WRONG
  heading's title; it now agrees with the renderer (#335).

- **A parenthesised destination gets the Unicode-whitespace rule too.** The
  balanced-parens scan is a separate path from the plain one and did not carry
  the check added for carve#404, so the rule depended on whether the URL
  happened to contain a parenthesis: `[x](<NBSP>https://e.com)` was rejected
  while `[x](<NBSP>https://e.com/a(b))` linked with the invisible character in
  its href. This is also what made carve-rs look like it treated dangerous
  schemes specially - `javascript:alert(1)` is parenthesised, so it reached the
  unchecked path (carve#407).

- **A link label's closing `]` is found past an editorial comment.** The scan
  already skipped code spans, because a `]` inside one is content. An editorial
  comment holds literal content too and was not skipped, so `[{#a]b#}](u)`
  ended the label at the comment's bracket and formed no link - with no
  spelling that worked, since `{# ... #}` resolves no escapes and `\]` puts a
  real backslash in the comment. Applied to BOTH the scanner and the
  precomputed bracket table, which have to agree (carve#403).

- **Unicode whitespace ends a link destination, in both forms.**
  `unicode_url_char` is "any non-whitespace, non-ASCII Unicode character" with
  no qualifier, but the byte scans tested ASCII whitespace only - so a narrow
  no-break space passed for an ordinary destination character. An inline
  destination carrying one now forms no link, and a reference definition's
  destination ends there. Zero-width characters (U+200B, U+FEFF) are not
  whitespace and stay: the test is the Unicode White_Space property, not "is
  invisible" (carve#404).

- **The Markdown target emits the heading id a cross-reference needs.** It
  re-derived every heading's id by slugging the heading text, so it never knew
  about the `-N` suffix the core adds to a duplicate heading. A reference to
  `Setup-2` then matched no heading, which cost it BOTH halves at once: the
  heading lost its `{#id}` suffix, and `render_link` degraded the reference to
  bare text because it drops a fragment link whose target it does not know. Ids
  now carry the same disambiguation the core uses (carve#352).

- **A cross-reference no longer feeds the heading slug it sits in.** By the time
  the Markdown target runs, resolution has turned `</#a>` into a link carrying
  the target heading's text, so `# A </#a>` slugged as `A-A` and every id
  derived there disagreed with the one the core assigned before resolution. The
  core's own `plain_inlines` already skips a resolved cross-reference; this
  copy did not (carve#352).

- **A continuation line that reads as a list marker is aligned and escaped,
  not under-indented.** The canonical writer special-cased such a line to a
  fixed two-space indent instead of the marker-width indent every other
  continuation gets, which happened to work only while the marker was wider
  than two columns. Aligning it lets the existing escaping do its job, and
  matches carve-js and carve-php (carve#352).

- **Removed a dead negative cache in the comment-closer lookahead.** The
  lookahead already answers from a width to last-index map, so the per-width
  "no closer from here onward" cache in front of it could never change an
  outcome - and its hit condition is unreachable anyway: it needs a second
  opener of the same width after a proven-no-closer point, but a second line of
  the same width IS the closer for the first. Found while investigating a patch
  coverage miss on the same code in carve-php, where those lines were the whole
  gap.

  The perf test around it was also weakened: its `< 2.0` per-byte bound sat
  exactly at the boundary for this defect, so a version that rescans to end of
  input per opener PASSED it - taking 162 seconds instead of 0.6 to do so. The
  bound is now 1.2 (measured: 0.73 with the index) plus a wall-clock ceiling, so
  a reintroduced rescan fails instead of merely crawling.

- **A `%%%` comment opener with trailing text no longer leaks the comment body
  and drops the next block.** `%%% html` was not accepted as a fence line, so
  the `%%` line-comment rule ate the opener, the body rendered as an ordinary
  paragraph, and the following `%%%` opened an unterminated block that swallowed
  the rest of the document. A comment fence is now a delimiter plus an
  insignificant tail: only the leading run of `%` is structural, so `%%% TODO`
  opens and `%%% end` closes. Percent fences carry no info string - a raw block
  is a code fence with `=FORMAT` - so `%%% html` is a comment and its body stays
  hidden.

  An opener with no matching closer ahead now opens nothing and degrades to a
  line comment, so following blocks still render. The closer also matches on
  **exact** delimiter length now: `%%%%` no longer closes a `%%%` block, which
  is what the spec always required and what carve-js does. The opener's tail is
  kept as the body's first line so the writer round-trips it; a closer's tail is
  dropped (carve#463, PART 9 §28).

- **A blank line inside a marker-line item no longer ends its sub-list**
  (carve-rs#301). `- - A` opens a sub-list on the marker line; when its first
  item is loose, the sibling marker after the blank sits at the sub-list's own
  column, which is shallower than the indented block above it but still inside
  the item. The collector compared that line's indent against the first
  collected block's indent rather than against the item's content column, so it
  ended the block there and the sibling started a second list - splitting one
  list in two and flipping the following item from loose to tight.

  Reached through `carve fmt`, which emits a blank the source did not have: both
  PART 11 §1 invariants broke at once, so formatting a document changed what it
  rendered as. carve-js and carve-php read all of these as one list.

### Changed

- **BREAKING (AST): an escaped character is now its own node.** `\-` parses to
  `InlineNode::EscapedText` instead of being folded into the surrounding text.
  Consumers reading `Text` see the run split at each escape; the character
  itself is unchanged, and every renderer's output is the same except Markdown
  (below).

  The backslash carries intent the character does not: `\-\-` was written
  precisely so a downstream processor with smart punctuation on would not read
  an en dash. Flattening it lost that, and this engine emitted the trigger bare
  where carve-php reproduced the escape (carve#350). `escaped_text` is in the
  inline vocabulary in the spec's profiles.md.

- **Markdown output reproduces the author's escapes** (PART 11 §7 M2).
  `A \" B \-\- C` now renders as `A \" B \-\- C` rather than `A " B -- C`.
  A document that escapes nothing gains no backslashes.

### Changed

- **BREAKING (AST): a line block is now its own node type.** `::: |` parses to
  `BlockNode::LineBlock` instead of a `Div` carrying a `.line-block` class.
  Consumers that matched on the class have to match on the variant instead.

  The class could not express the construct: inside a `::: |` fence every
  newline is a hard break, while a plain div an author gave that class keeps
  soft breaks. With only the class to go on, the writer could not tell the two
  apart, emitted the generic `:::` form, and a formatted line block re-parsed as
  an ordinary div - one of the four constructs breaking
  `parse(fmt(x)) == parse(x)` (carve#359). It also brings carve-rs in line with
  the block vocabulary in the spec's profiles.md, which lists `line_block`, and
  with carve-php and carve-js, which now both have the node.

  **Rendered output is unchanged** in every target: the HTML is still
  `<div class="line-block">`, with the structural class trailing the author's
  own attributes (`{.foo #v}` renders `class="foo line-block" id="v"`).

### Fixed

- **`carve fmt` no longer changes a table's alignment.** A cell's alignment
  marker is the first byte of its content, but the parser re-indexed the raw
  cell at `[1]` for a header cell - where the `=` was already stripped - and so
  read the byte after the marker. A header cell beginning with an escaped marker
  (`|=<\< Note |`) came out centred where carve-js and carve-php read it as
  left. Latent until the writer started emitting that shape, at which point the
  formatter corrupted the document it formatted.

- **Tables are written in the native header form** (`=` cells plus per-cell
  alignment markers) instead of a GFM delimiter row, closing the table half of
  carve#359. A delimiter row's alignment applies to the whole column while the
  AST records it per cell, so an aligned header over unaligned body cells came
  back with every body cell aligned. The two header shapes with no native
  spelling (a promoted span marker, a header cell carrying attributes) keep the
  delimiter row, now emitted bare. Output is byte-identical to carve-js.
- **`autolink` and `admonition` are deniable by name** (carve#362). Both folded
  into `link` / `div` before the profile's allow/deny check, so naming them was
  a silent no-op - a host restricting untrusted input could deny autolinks, get
  no error and no violation, and still emit them. They stay covered by the
  broader name: denying `link` still strips autolinks and denying `div` still
  strips admonitions, so no profile written against the broad name is widened.

- **The canonical writer reproduces a line block as a line block** (carve#359).
  It emitted a bare `:::` plus a `.line-block` class, and resolved the indent
  placeholder to a literal non-breaking space - which re-parses as text rather
  than as indentation, so the text node came back different even though the
  emitted bytes looked identical. `::: |` and its leading spaces now round-trip
  byte for byte.

- **A leading attribute line reaches a line block.** `{#verse}` before a `::: |`
  fence was dropped: the block-attribute merge fell through to a catch-all arm
  that silently ignored the node.

### Fixed

- **The Markdown renderer no longer de-escapes underscores inside verbatim
  content.** The intraword-underscore cleanup matched a literal `\_` anywhere in
  the assembled document, so a backslash the author wrote was rewritten along
  with the escapes the renderer added: `` `a\_b` `` came back as `` `a_b` ``, and
  the same happened in fenced code blocks, link destinations, image sources and
  escaped raw HTML. Each of those dropped a byte the parser had kept - a code
  span does not process escapes, so its content carries the backslash literally.
  The cleanup now decides on a sentinel only the text escaper emits, so it sees
  exactly the escapes the renderer wrote (carve-js#400).

- **The Markdown renderer no longer escapes intraword underscores.**
  `company_id` came out as `company\_id`, but CommonMark does not honour an
  intraword underscore - it renders literally either way - so the escape
  protected nothing and only littered identifiers in output meant to be read
  and searched. An asterisk is not symmetric here (`a*b*c` does emphasise), so
  `*` stays escaped everywhere; only `_` narrows.

  Ships together with the same change in carve-php and carve-js, and the three
  engines were compared byte-for-byte on the Markdown target.

### Fixed

- **`carve fmt` no longer rewrites the author's smart typography** (carve#339).
  Formatting normalized `...` to the ellipsis glyph, `--` to an en dash and `"`
  to curly quotes in the author's own source. The Carve renderer now splits text
  into literal runs and smart-typography runs: literals still go through the
  escaper, and a smart run is emitted exactly as typed, so it re-derives to the
  same glyph on the next parse instead of being frozen as one.

  Every other target is unchanged - HTML, Markdown, plain text and ANSI keep
  resolving smart typography at render time. `to_html(fmt(x)) == to_html(x)` and
  `fmt` idempotency both still hold, and `fmt` output is now byte-identical to
  carve-js and carve-php across the full transform matrix.

  Simply dropping the smart pass was not enough: the escaper is doing double
  duty, protecting block markers (a literal `>` at the start of a line must stay
  escaped or it re-parses as a blockquote) as well as escaping punctuation smart
  typography owns. Splitting resolves that conflict. An escape sequence passes
  through verbatim, since it is already valid source - re-escaping it doubled
  the backslash, and unescaping it dropped a non-breaking `\ ` entirely.

## [0.1.1] - 2026-07-27

- Advance the spec corpus to carve `9c5f53a` (categories 143-162: definition-list
  openers, strict column-0, the dash-run ladder, unresolved footnote-ref
  attributes, tight-item trailing text, and the list-looseness pins) and cover
  them in the corpus test. Fix a `carve fmt` bug the new corpus exposed: a tight
  list item with more than one child (for example text after a fenced block,
  category 162) was joined with blank lines and loosened on re-parse. A tight
  item now joins its blocks with a single newline, keeping the blank only
  adjacent to a nested list child.
- BREAKING: rename `Emoji` AST nodes to `Symbol`, the `emoji` render option to
  `symbols`, and the CLI flag `--emoji` to `--symbol`; symbol shortcodes now
  require a leading word boundary, require an ASCII alphanumeric first name
  character, and support trailing attrs via an HTML `<span>` wrapper.

### Fixed

- **Trailing text after a closed block in a TIGHT list item now renders BARE,
  and a blank line that separates a block from trailing text now loosens the
  item** (matching carve-js and the executable-spec oracle). Previously text
  that followed a fenced code block, `:::` div, admonition, or table inside a
  tight list item was wrongly wrapped in a `<p>` (e.g. `- item` then an indented
  fence then `tail` rendered `<p>tail</p>` instead of a bare `tail`); a tight
  item never wraps any of its paragraphs, so all direct-child paragraphs now
  render as bare inlines. Separately, a blank line between a block and following
  trailing text (`- item` / fence / blank / `tail`) now marks the item loose,
  wrapping its leading and trailing text in `<p>` (§17 L1); a blank before a
  single sub-block with no trailing text stays tight (the compact-block rule),
  and a blank followed by another sub-block opener stays tight. As a side effect
  the checkbox of a loose task item now sits OUTSIDE its first paragraph
  (`<input ...> <p>b</p>`), also matching carve-js and the oracle.
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

- **SVG `img` fence** (Tier-3, opt-in, off by default) (#254, #263): an
  `` ```img `` block renders a sanitized SVG instead of showing the source.
  Sandbox by default - the sanitized SVG is encoded into a `data:image/svg+xml`
  `<img>` the browser isolates (no script, no fetch, no DOM access); a host may
  opt into a live inline `<svg>` for `currentColor` / CSS theming. When no
  `{alt=…}` is given, the alt text falls back to the SVG's `<title>`.
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
- A table continuation row must close with a pipe (#259).
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
