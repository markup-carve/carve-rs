# Changelog

All notable changes to carve-rs are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Entries for 0.1.5 and earlier are archived in
[CHANGELOG-0.1.md](https://github.com/markup-carve/carve-rs/blob/main/CHANGELOG-0.1.md),
which the published crate does not carry.

## [Unreleased]

### Changed

- Match the pinned extension attribute-order rule for spoiler wrappers with and without an authored class slot (#1981).
- Escaped spaces and preserved line-block columns use `non_breaking_space` nodes. Literal U+E000 remains literal in every output; stored trees using the old marker need source reparsing to recover authored characters (#1981).
- Annotation offsets use a fixed codepoint projection independent of JSON key order, including image alt text, math, breaks and generated spaces (#1981).


## [0.1.7] - 2026-09-25

### Breaking

- A `footnote_ref` spells its target as `label` on the wire, and a reference carrying no target is refused (#1849, #1853).
- A citation item spells its own mode on the wire: `Citation` grows `mode`, and `CitationGroup::integral` becomes a method (#1860).
- A display equation carries its label and its number (#1862).
- `RenderLoss.format` is optional, and a checked render result carries totals by loss code (#1859, #1868).
- `Document` implements `Drop` so it tears a deep tree down iteratively. Field moves, by-value destructuring and struct update syntax no longer compile: borrow the fields, or `std::mem::take` a mutable document (#1887, #1916).
- The raw-keep report names a non-handler injection sink `injection-sink`, the spelling the spec and the other two engines use, not `active-content` (#1881, #1883).
- `HtmlImportOptions.max_depth` cannot raise HTML import past the 128-level `MAX_HTML_IMPORT_DEPTH` ceiling; a lower value still narrows it (#1907).
- Markdown conversion gains fallible `try_markdown_to_ast`, `try_markdown_to_carve` and `try_migrate_markdown`. The older entry points panic where excessive nesting or the canonical writer prevents conversion, rather than returning a partial result (#1875, #1877, #1884).
- The CLI rejects a non-UTF-8 argument as a usage error naming the argument, exit 2, instead of panicking with exit 101 (#1834, #1835).
- The HTML importer reads an empty mark carrying attributes as an empty span holding them, reversing the choice #1734 made (#1807).
- A link tail holds no whitespace before its closing parenthesis, so `[t](a )` reads as prose (#1809).
- A generated-content `:::` kind parses as a directive rather than an admonition (#1871, markup-carve/carve#2225).
- Every empty block container renders one blank HTML body line (#1841, markup-carve/carve#2184).

### Fixes

- A node pulled in by a sliced include reports positions in its own file's coordinates, and list items, table rows and cells, definition terms and citation items carry `pos.file` too (#1783, #1786).
- A reference definition reads its destination through the same `link_destination` production as an inline tail, with the trailing attribute block read after the destination and the title (#1792).
- A destination that reaches whitespace with an unclosed parenthesis is refused instead of publishing an unbalanced `href` (#1794).
- A form feed is content in the bold-italic guard, and a bare emphasis delimiter is refused against a tab as against a space for `/`, `*`, `_`, `~` and `=` (#1810, #1811).
- The combined token's trailing attribute block is written on the outer `<strong>`, which the HTML renderer used to drop (#1814).
- A `::` line with an empty term and trailing whitespace folds into an open description body (#1815).
- A code fence in a list item or a description body interrupts the open paragraph only when a closer is written in that body at the content column, and a fence line the item's lead paragraph absorbs still opens a span to the end of the item (#1816, #1821).
- An unresolved reference's literal source keeps its no-break spaces visible, and a heading reference key keeps a no-break space (#1823).
- A fenced code block inside a description body carries a position, so the description and the list around it reach the fence's closer (#1837).
- A blank in a sibling sub-list no longer loosens the outer item (#1847).
- The parser and Markdown importer backlog: a reference label holds an opening `[` as content, a `%%` comment inside a bare emphasis run consumes the rest of the line, a caption's number placeholder is the first `#` that does not begin a tag, a nested item's bare `:::` run opens its div in that item, a code span on an indented continuation line begins at its backtick run, and an unclosed inline HTML opener is bounded at the end of its block so the later blocks survive (#1827).
- A quoted fence or container title is read as the source spells it, with no escape mechanism, so an authored backslash survives and an escaped quote cannot extend the slot (#1946, #1949).
- The canonical writer escapes a caret beside a brace only where dropping the escape would change the reread tree, so `{^`, `^}` and `x{^y` are written as the source spells them (#1950, #1955).
- A `%%` line comment drops trailing ASCII space and tab from its content and consumes one leading space or tab as the separator, in the block, inline and verse readers alike, and the canonical writer emits nothing after the marker for a comment that is empty once trimmed. A `%%%` block body keeps its bytes, and a no-break space or a vertical tab survives (#1951, #1955).
- The Markdown writer keeps the `**`/`*` spelling of an emphasis that follows an escaped marker (#1831, #1836).
- A list keeps its tightness on the Markdown target, and the tight-item separator rule reaches an ATX heading, a fenced code block and a GFM table as well as the two spellings CARVE-P11-047 names (#1900, #1911, #1914, #1925).
- A tight item's sibling quotes and tables are kept apart, since a quote absorbs a following quote or table rather than being interrupted by it (#1924, #1934).
- A task item's continuation lines are padded to the item's content column rather than past its checkbox, so a heading, fence, table or nested list below the first paragraph reaches the output (#1912, #1929, #1938).
- A block below a nested list stays out of the item above it instead of arriving as lazy continuation (#1930, #1940).
- A placed glossary writes both of its tags at one column, and a placed reference list indents its items and closing tag to their container's depth (#1906, #1936, #1942).
- `carve lint` reports `references-placement-in-container` for a nested references marker when citations are enabled (#1935, #1939).
- A placement a container cannot carry out is refused rather than splicing an endnotes section into a block quote, and a bare placement carrier survives a profile that denies nothing it uses (#1894, #1922, #1927).
- A title or a `[label]` keeps a body-less container through the profile filter, as a caption keeps a figure group (#1897, #1915).
- `LinkPolicy` reads the host a browser reads (#1870).
- A denied-scheme destination is imported as its content (#1872, markup-carve/carve#2255).
- A raw-kept element reports its refused attributes, and a `style` in kept bytes is read through the refusal policy so the row carries the code, severity and owner every other refused attribute gets (#1873, #1892, #1913).
- A `style` declaration's `url(...)` arguments are read off the same comment-stripped, escape-decoded text the renderer acts on, so the report cannot describe a danger the renderer does not leave behind (#1921, #1943).
- An unspellable attribute name is reported in the ruled words (#1882, #1889).
- The directive-as-admonition substitution is reported as a degradation (#1879, #1885).
- A directive's quoted title survives parse, wire, writer and render, is walked where an admonition's is walked, and is visited at the eleven further traversals that skipped it (#1874, #1880, #1895, #1908, #1909, #1919, #1941).
- The ProseMirror bridge carries a container title's inline words through its flatten, and writes a degraded composite figure's group caption as a trailing paragraph inside the `carveDiv`, which `from_prosemirror` accepts (#1785, #1944, #1947).
- The BBCode importer spells the four formatting tags the way the Carve writer would, escapes the inline constructs a post's own text forms beside the tags it converted, and keeps an empty quote as a `>` block instead of dropping it, with no trailing space on a blank line inside a quote (#1813, #1825, #1952, #1954).
- The autolink extension decodes backslash escapes in a bare URL (#1842, #1843).
- The Markdown importer keeps an ordered task item's marker as the text it was written as, reads a box for a bullet task item whose label is also defined, and reads a box only where the cmark-gfm tasklist extension reaches it (#1886, #1888, #1891, #1899, #1902, #1920).
- An ordered task item's lost checkbox is reported as `structure-unspellable` in one wording at both the Markdown and the HTML entry point (#1890, #1904, #1928, #1948).
- HTML import keeps multiline elements and code inside a table cell on one Carve row, reporting the loss where content must be flattened (#1910, #1931).
- The plain-text escaper freezes a hash after an ampersand, so numeric-reference text cannot become a Carve tag (#1841).

### Improvements

- `to_ast_envelope_json` and `from_ast_envelope_json` read and write the versioned AST interchange envelope, and `AstEnvelopeError` separates a newer contract, an unimplemented required extension and a foreign vocabulary from a tree that would not decode. A major too large for a machine integer is a version refusal too, the schema pattern matching a major of any width (#1963, #1966).
- A spanning table cell publishes its resolved extent, and a rowspan crossing a row-group boundary renders in one body group (#1850, #1854, markup-carve/carve#2224).
- JSON AST interchange carries ruby base and annotation pairs through rendering, profiles and canonical source output, with the flattened form reported as `ruby-flattened` and accepted by `--allow-loss ruby-flattened`; it also preserves the optional line-end ranges on line blocks (#1859, #1868).
- A small-caps span is carried through every target (#1864).
- `block_extension` takes the wire name CARVE-P12-055 gives it, and `block_extension`, `directive` and `ruby` join the canonical type vocabulary so `is_type_allowed` stops answering "allowed" for a name it does not know (#1856, #1867).
- The ProseMirror bridge builds the four types carve-grammars named for it (#1893).
- `ast::dispose_blocks` and `ast::dispose_inlines` drop a detached block or inline tree with the same iterative teardown `Document` uses. Ordinary `drop` on detached nodes, and the derived `Clone`, `Debug` and equality traits, remain recursive (#1917, #1918, #1926).

## [0.1.6] - 2026-09-18

### Added

- The CLI accepts repeatable `--extension` registry keys, tabs and citation
  modes, section-wrapper opt-out, and source-line annotations
  (markup-carve/carve-rs#1755).
- `Options` accepts authoritative mention and tag resolver callbacks with node
  attributes and opaque host context (markup-carve/carve-rs#1682).
- `IncludeResolver` gains a defaulted `unresolved_id`, and `FileSystemResolver`
  answers it with where a missing target would be, so a host watches the file the
  author meant rather than the directive's spelling (markup-carve/carve-rs#1688).
- `carve --json` publishes the include dependency list, so a host can watch every
  file a document reads (markup-carve/carve-rs#1678).
- The extension registry publishes each fenced-render preset's static-renderer
  key, so a binding can list the diagram classes it will be asked for
  (markup-carve/carve-rs#1679).
- `from_prosemirror_with_report` returns the document with `dropped` and
  `degraded` maps, for what the editor payload lost on the way back
  (markup-carve/carve-rs#1759).

### Changed

- The ProseMirror bridge carries block source positions through `carvePos`, so
  collected link and footnote definitions retain their authored order and
  placement (markup-carve/carve-rs#1694).
- A mention or tag template that produces a denied URL now renders the inert
  span instead of an anchor with an empty `href` (markup-carve/carve-rs#1682).
- The Carve writer keeps the native `|=` header form for a table whose header
  spans trail its real header cells (`|= A |= B | < |`), instead of a GFM
  delimiter row. A leading span, a real cell after a header span, or a trailing
  rowspan still uses the delimiter row. The HTML importer, which renders through
  the writer, produces the native form too (markup-carve/carve-rs#1658).
- **Breaking:** Migration reports advance to schema version 2: `Carried` is renamed to
  `Preserved`, `Normalized` distinguishes semantics-preserving rewrites, and
  Markdown, Djot, and BBCode now emit a conservative `Dropped`/`Fallback`
  finding because those paths do not yet expose construct-level fidelity.
  `MigrationReport` also gains `mode` and `adapter`. `ElementUnwrapped` moves
  from carried to degraded, opaque `raw-preserved` HTML moves to degraded,
  truncated diagnostics become dropped/fallback, and HTML `--check-loss` now
  passes preserved/normalized findings and fails degraded/dropped ones. The CLI
  emits the same report for every importer (markup-carve/carve-rs#1584).

- Add processor-level file inclusion (`{{ path }}`, PART 9 §19 rules I1-I11):
  `expand_includes` expands directives with a host-supplied `IncludeResolver`,
  with section and line-range selection, heading-level shifts, cross-file id and
  footnote-label renaming, cycle, depth and byte budgets, and dependency
  reporting. The core parser opens no file: with no resolver the directive stays
  literal. `FileSystemResolver` sits behind the default-on `fs` feature, so a
  build with that feature off carries no file-opening code. The CLI gains
  `--include-root` and `carve flatten`, which writes the document back with
  every include expanded (markup-carve/carve-rs#241).
- **Breaking:** `IncludeResolver::resolve` returns
  `Result<IncludeResolved, IncludeDenial>` instead of `Option<IncludeResolved>`,
  so a refusal says which class it is and `IncludeDependency` carries it. To
  migrate, map `None` to `Err(IncludeDenial::NotFound)` or
  `Err(IncludeDenial::Unresolved)`, which is what every refusal meant before,
  and `Some(x)` to `Ok(x)`; a hand-built `IncludeDependency` adds
  `denial: None` (markup-carve/carve-rs#1648).
- **Breaking:** the `substitution` node carries its halves as `old` and `new`,
  two arrays of inline nodes, in place of the `oldText` and `newText` strings;
  `CriticSubstitute` changes the same way. An empty half is an empty array, and
  ingest refuses the old fields. Resolution and positions now reach both halves
  (markup-carve/carve-rs#1752, markup-carve/carve#2095).

### Fixed

- The ProseMirror bridge names a stock Tiptap mention by its `id`, falling back
  to `label` when `id` is missing or `null`, and never writes
  `mentionSuggestionChar`. A different `label` is reported as degraded, and a
  name with no Carve spelling, such as `Lea Thompson`, is written as the escaped
  text the editor showed and reported as degraded, since the name survives as
  that text (markup-carve/carve-rs#1759, markup-carve/carve-rs#1770).
- `render_carve` returns `SourceUnspellable` for a mention or tag whose name the
  grammar rejects, such as one with a space, an apostrophe, an outer or doubled
  dot, or a non-ASCII letter. It used to delete those characters and write a
  different name (markup-carve/carve-rs#1762).
- The ProseMirror bridge drops a mention's or tag's own attribute, such as
  `data-team`, and reports it under `dropped`, instead of losing it with an
  empty report or handing the writer a tree it refuses. On a mention written as
  text the attribute is reported as dropped, since nothing carries it.
  `render_carve` still refuses such a tree built by an API caller
  (markup-carve/carve-rs#1763, markup-carve/carve-rs#1770,
  markup-carve/carve-php#2167).
- The ProseMirror bridge drops a mention or tag that carries neither an `id` nor
  a `label` and reports it under `dropped`, keyed on the node kind, no field
  having held a name. It used to write a bare `@` or `#`, a character the
  payload never carried. `render_carve` still refuses such a tree built by an
  API caller (markup-carve/carve-rs#1773, markup-carve/carve-php#2176).
- An opener of an emphasis kind already open is content, bare or forced, and a
  braced inline of another kind starts its own scope for that rule and for the
  closer search (markup-carve/carve-rs#1741, markup-carve/carve-rs#1747).
- A substitution splits only at a `~>` outside code, math, an inline literal, a
  comment or an escape, and each half is inline content
  (markup-carve/carve-rs#1742).
- A code span's closer is searched for across the rest of the block, so a
  forced or editorial closer inside the span is code
  (markup-carve/carve-rs#1740).
- An unclosed code span ended at a forced or editorial closer drops the line
  break before it, except in a line block (markup-carve/carve-rs#1748).
- `render_carve` writes an empty delimited comment between two touching
  backtick runs, such as two adjacent code spans, which it used to merge into
  one span (markup-carve/carve-rs#1743).
- `render_carve` returns `SourceUnspellable` for a table row whose every cell is
  blank, and the HTML importer drops such a row with a `structure-unspellable`
  diagnostic, keeping the rest of the table (markup-carve/carve-rs#1735).
- The HTML and Markdown importers keep the title of a link or image that names
  no destination, on the span that replaces it (markup-carve/carve-rs#1738).
- The ProseMirror bridge reports a substitution whose halves hold more than
  text, and an emphasis inside one of its own kind, as degraded instead of
  returning a different tree (markup-carve/carve-rs#1752,
  markup-carve/carve-rs#1747).
- `render_carve` returns `SourceUnspellable` for a mention or a tag that carries
  attributes, where it used to drop them (markup-carve/carve-rs#1737).
- A link, image or span after a backtick an earlier construct used up is read,
  where the paragraph's later brackets used to stay literal
  (markup-carve/carve-rs#1733).
- The HTML importer drops an inline element the HTML left empty, such as
  `<strong></strong>`, which it wrote as delimiters that read back as text
  (markup-carve/carve-rs#1719).
- `render_carve` returns `SourceUnspellable` for an emphasis, insertion or
  deletion inside one of its own kind in the same scope, and the HTML importer
  unwraps the inner one with a `structure-unspellable` diagnostic
  (markup-carve/carve-rs#1725, markup-carve/carve-rs#1741).
- `render_carve` returns `SourceUnspellable` for a mention or tag directly
  against a word character (markup-carve/carve-rs#1729).
- `render_carve` writes a hard break in a table cell as one space, which kept
  the row from ending; the HTML importer reports it as `structure-unspellable`
  (markup-carve/carve-rs#1714).
- `render_carve` escapes the colon of a trailing `:name` before a link, span,
  note reference or citation, which read back as an inline extension
  (markup-carve/carve-rs#1726).
- The Markdown importer writes a link with an empty destination as its text and
  such an image as its alt text (markup-carve/carve-rs#1717).
- `render_carve` returns `SourceUnspellable` for an empty code span its backtick
  run cannot end, and the HTML importer drops such a span with a
  `structure-unspellable` diagnostic (markup-carve/carve-rs#1705).
- The HTML importer also drops an emphasis, insertion or deletion that held only
  such a span, which it wrote as bare delimiters (markup-carve/carve-rs#1718).
- `render_carve` escapes a caret that ends a text node before a link, span, note
  reference or citation, which read back as an inline note
  (markup-carve/carve-rs#1710).
- The HTML importer drops the space after a `<br>`, so its output is a fixed
  point of `carve fmt` (markup-carve/carve-rs#1706).
- An emphasis ending in a hard break keeps its closer in the Carve writer and the
  HTML importer (markup-carve/carve-rs#1702).
- An include directive closes at the first `}}` outside a quoted run, so a quoted
  value may carry the pair (markup-carve/carve-rs#1614).
- A relative include root is refused rather than resolved against the process
  working directory, and the byte budget charges a target that breaks it and
  publishes the total read (markup-carve/carve-rs#1612, markup-carve/carve-rs#1617).
- The Carve writer keeps a block image, a merged run and a line comment in the
  tight item that hosts them instead of letting a re-parse pull them into a
  sub-list (markup-carve/carve-rs#1602, markup-carve/carve-rs#1596).
- An admonition title, a definition term, a figure caption and a container label
  move emphasis padding outside the delimiters in Markdown too
  (markup-carve/carve-rs#1616).
- `render_carve` writes a frontmatter block as the author wrote it. It rebuilt the
  block from the parsed key/value map, which dropped a JSON or TOML block and
  key-sorted a YAML one behind a bare `---` (markup-carve/carve-rs#1695).
- Writing a paragraph that holds many include directives no longer costs the
  square of its length (markup-carve/carve-rs#1698).
- The Markdown writer no longer escapes a hash right after a task box, which no
  reader takes for a heading (markup-carve/carve-rs#1691).
- A comment whose content opens with `%` is written back with its full opener run
  instead of a shorter marker and a stray character (markup-carve/carve-rs#1591).
- A raw-HTML profile error points the author at Carve markup, not Djot
  (markup-carve/carve-rs#1637).
- A footnote written into a description body gets the same per-marker body floor
  as a nested note: an opener or plain continuation at that floor belongs to the
  note, a column-0 tail is top-level, and a list in the body starts at its marker
  (markup-carve/carve-rs#1577, markup-carve/carve-rs#1580,
  markup-carve/carve-rs#1583).
- The Markdown writer escapes a hash where the emitted line would read it as a
  heading: one opening a line inside a paragraph, and a heading's own trailing
  run, which a CommonMark reader takes for the closing sequence and drops
  (markup-carve/carve-rs#1681, markup-carve/carve-rs#1687).
- Markdown emphasis survives seams it used to lose: two adjacent delimiter runs
  stay apart, an escaped marker no longer lengthens the run beside it, a run
  that cannot flank falls back to inline HTML, and a literal tilde is escaped
  because GFM reads it as a delimiter (markup-carve/carve-rs#1624,
  markup-carve/carve-rs#1677, markup-carve/carve-rs#1620,
  markup-carve/carve-rs#1641).
- A bare emphasis closer no longer reaches inside a braced inline, a link
  destination or an autolink, and the fast layout path answers as the parser
  does (markup-carve/carve-rs#1638, markup-carve/carve-rs#1665,
  markup-carve/carve-rs#1671).
- The Carve writer braces an emphasis a re-parse would read differently: one
  whose bare opener the writer had just refused, and an italic whose content
  opens and closes with a strong marker (markup-carve/carve-rs#1656,
  markup-carve/carve-rs#1673).
- An underscore pair in text is escaped where the emitted BLOCK would pair it
  rather than the line alone, and a same-strength nesting stays readable
  (markup-carve/carve-rs#1653, markup-carve/carve-rs#1659,
  markup-carve/carve-rs#1642).
- A forced-span closer strips the unclosed verbatim run it ends, so a span
  holding one space renders an empty code element as the other engines do
  (markup-carve/carve-rs#1684).
- An inline include leaves ONE text run, spanned in the host file, instead of
  the adjacent text nodes PART 12 §1a forbids, and an included child is parsed
  with the caller's extensions (markup-carve/carve-rs#1683,
  markup-carve/carve-rs#1651).
- The ProseMirror bridge reports the mark run two adjacent spans come back as,
  and keeps two links with different destinations apart instead of merging them
  and losing the second (markup-carve/carve-rs#1667,
  markup-carve/carve-rs#1675).
- An empty code span survives a ProseMirror round trip. A mark cannot span zero
  characters, so the span used to leave the document with both loss maps empty
  (markup-carve/carve-rs#1689).
- Markdown and Djot import keep what they used to drop: inline HTML, attributed
  and unpaired raw HTML, structural Djot constructs, and fidelity and confidence
  on every diagnostic (markup-carve/carve-rs#1594, markup-carve/carve-rs#1603,
  markup-carve/carve-rs#1582, markup-carve/carve-rs#1588).
- The Markdown renderer moves emphasis padding outside the delimiters, so content that begins or ends with whitespace still reads as emphasis rather than literal text; content that is only whitespace falls back to inline HTML (markup-carve/carve-rs#1599, markup-carve/carve-js#1683).
- A nested note ends at a definition below its own body floor, so a trailing
  line further down belongs to the surviving ancestor rather than reaching a
  floor that has already closed (markup-carve/carve-rs#1575, markup-carve/carve#1971,
  markup-carve/carve#1918).
- A trailing line after a consumed definition in a stack of nested notes is
  placed by column-reach. A note nested one column shy of its host's body column
  keeps a residual marker indent, so a line below the inner note's own content
  column falls to the reachable ancestor note instead of the innermost one,
  matching carve-js (markup-carve/carve-rs#1573, markup-carve/carve#1946,
  markup-carve/carve-php#1895).
- A description item following a nested closed fence opens a new item instead of
  being absorbed into the one above, matching the other engines
  (markup-carve/carve-rs#1572, markup-carve/carve#1970). `carve fmt` no longer inserts a separating blank
  before that item, converging its `carve` output with carve-js and carve-php.

[Unreleased]: https://github.com/markup-carve/carve-rs/compare/0.1.7...HEAD
[0.1.7]: https://github.com/markup-carve/carve-rs/compare/0.1.6...0.1.7
[0.1.6]: https://github.com/markup-carve/carve-rs/compare/0.1.5...0.1.6
