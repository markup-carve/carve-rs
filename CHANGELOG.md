# Changelog

All notable changes to carve-rs are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- **The index back-link says where it goes** (markup-carve/carve#1469). A `↩`
  with no accessible name is announced as "leftwards arrow with hook", or
  skipped - and an index entry has one per occurrence, so a reader met a row of
  identical unnamed arrows. The k-th back-link is now named `Back to {term} {k}`
  and shows `↩<sup>k</sup>`, mirroring PART 9 §16's footnote rule.

### Changed

- **A tab set, a code group and a rendered diagram carry an accessible name**
  (markup-carve/carve#1468). Each tab was already named by its own `<label>` and
  the GROUP was anonymous; a diagram fence emitted its source with no role, so a
  reader heard the markup as prose. `Tabs` and `CodeGroup` write `role="group"`
  plus a name (`Aria` mode keeps `role="tablist"`), and `FencedRender` writes
  `role="img"` plus a `label` defaulting to the fence word. An `aria-label`,
  `aria-labelledby` or `role` the author wrote always wins, matched
  ASCII-case-insensitively, and the engine's attributes are recorded at the END
  of the author's attribute order - without that they rendered in map order,
  which is alphabetical, and the three engines disagreed byte-for-byte on a
  shape the optional corpus pins.

- **One `labels` map localizes every engine-written string.** `label_default`
  grows `indexBackref`, `tabsGroup` and `codeGroup`, and the extensions that
  write those strings read them through the new `RenderContext::label`, so a
  German document sets `labels` once instead of finding several call sites and
  silently missing one. An option set on the extension still wins for that
  instance. PART 9 §16a already required this - "an extension MUST NOT require
  the host to configure the same text twice".

### Added

- **ASCII-folding for auto-generated heading ids**, opt-in
  (markup-carve/carve-rs#1159, spec PART 9 §12).
  `Options::with_ascii_heading_ids` takes `AsciiHeadingIds::Fold` - transliterate
  what the table covers and keep the rest, so `Grüße` is `Grusse` and a CJK
  heading keeps a usable anchor - or `AsciiHeadingIds::Strict`, which also drops
  the residue for an id guaranteed to match `[0-9A-Za-z-]`. Orthogonal to
  `with_lowercase_heading_ids`, and off by default. The transliteration table is
  the one carve-js and carve-php already carry, ported rather than authored, so
  the ids are byte-identical across the three engines; `Strict` is what
  carve-php's extension does and `Fold` is carve-js's default. Zero dependencies:
  the table is baked, bounded and auditable.

### Changed

- **A colon container's body is parsed from a worklist, not down the stack**
  (markup-carve/carve-rs#1165). Nesting costs heap instead of host stack, so a
  document at the 200-level cap parses in 128KiB where it needed 384KiB
  (release; 1024KiB to 256KiB in debug) and `to_html` needs 256KiB where it
  needed 384KiB. This matters on wasm, where the host owns the stack and an
  overflow takes the module rather than the call
  (markup-carve/carve-wasm#48). No output changes.
- **A row is a row, in every table section** (markup-carve/carve#1459, PART 10
  §7). `<thead>` and `<tfoot>` now write one row per line, as `<tbody>` always
  did. Nothing renders differently - whitespace between rows in table context is
  not rendered - but the emitted HTML is consistent and diffs read cleanly. All
  three table paths move: the block renderer, the layout fast path and the
  list-table extension.
- **A table cell's marker run ends at a space** (markup-carve/carve#1259, PART 9
  §5 T11). The kind marker `=`, the alignment run and the attribute block are
  one run, and a cell carrying any of them must follow it with a space; without
  one there is no run and every character of it is content. `|=hot= |` is the
  highlight its author wrote rather than a header cell holding `hot=`, `|=a |`
  is a data cell, and `|{#x}=R|` is literal text. The run is atomic, so a
  rejected alignment run takes the `=` with it. A cell with no run is unchanged,
  and the canonical writer already pads every cell, so a formatted document
  needs no migration.
- **A column's alignment defaults come from the header section**, not from row
  0 (markup-carve/carve#1259, PART 9 §5 T9). A `|=` cell below the header run is
  a row header - it heads its row, not its column - so it declares no column
  default. The two readings agreed until the clause above let a rejected marker
  run demote a row that used to be all-header, at which point a body cell
  inherited alignment from a row header, where carve-js and the spec's oracle
  inherit nothing.

### Added

- **A `labels` render option carries the strings the engine writes itself**
  (markup-carve/carve#1456, PART 9 §16a). One key today, `footnoteBacklink`,
  defaulting to `Back to reference`, set with `Options::with_label`. Values are
  text and are escaped where they land, unlike the raw `symbols` map.

### Fixed

- **The footnote backlink has an accessible name** (markup-carve/carve#1455,
  PART 9 §16). `role="doc-backlink"` was right and the name was the `↩` glyph,
  so a screen reader announced its Unicode name or skipped the link. The name is
  now the label plus what the link visibly says: `Back to reference` for a lone
  backlink, `Back to reference 2` for the second of several.

### Fixed

- **A hyphen run that opens a word after whitespace is a flag, not a dash**
  (markup-carve/carve#1443, PART 9 §8). `git log --oneline` and
  `--force-with-lease` keep their hyphens; every other position converts as
  before, including `pages 1--10` and a trailing `text --`.

### Changed

- **Core parsing avoids per-line copies in definition prepasses.** The link
  definition scan borrows unchanged lines, documents without footnote syntax
  skip the footnote-definition scan, and ordinary documents reject the
  colon-ladder specialization before building its line index. Inline parsing
  appends ordinary ASCII prose in runs and sizes its reusable buffers from the
  input. On the shared 49 KiB Tier-1 benchmark the prepass changes alone remove
  about 3,800 allocations per parse; together the changes improve end-to-end
  throughput by 11–21% in interleaved local trials.
- **A vertical table-cell marker requires a horizontal partner.** Lone `^` and
  `v` prefixes remain visible content; paired two-axis runs are unchanged.
- **`Figure::target` is now `Box<FigureTarget>`** (#1119). Breaking, for callers
  that construct or match a figure through the public AST. `Figure` embedded a
  whole `Table` or `CodeBlock`, which set `BlockNode` at 472 bytes against 264
  for the next largest variant; it is 272 bytes now. Every recursive walk moves
  those by value, so a nesting level costs less in stack across parse, clone,
  drop, render and serialize (markup-carve/carve-wasm#44).

### Fixed

- **An all-blank raw payload remains distinct from an absent payload.** One
  blank line between raw fences produces one newline, matching the general
  payload-preservation rule (markup-carve/carve#1414, corpus 372).
- **Depth limits now guard every recursive input and output path** (#1119,
  #1120, #1121, #1124). HTML inline rendering and extension re-entry share the
  live render budget, AST JSON encoding has a fallible `try_to_json` entry
  point used by merge and patch, and the render ceiling is reachable by a
  valid wire document while remaining above every parser-produced tree.
- **The block parser returns its large node through a box** (#1119), removing a
  `BlockNode`-sized return slot from each recursive parser frame.
- **Definitions collected at a list item's content column close its paragraph**
  (markup-carve/carve#1376). A following line below that column no longer uses
  the comment-only continuation path; bare-dot items use the bullet column.
- **Parsing a document at the nesting cap costs far less native stack**
  (markup-carve/carve-wasm#44). `promote_block_images` is a worklist instead of
  a recursion, and the over-cap degrade moved out of `parse_blocks`' frame.
  Measured on corpus document 182, parsing dropped from 347 KiB of stack to
  103 KiB, which is what a host with a small stack - a wasm module's 1 MiB, for
  instance - had been running out of. No output changed.
- **A structural link title and an authored `title` attribute both survive the
  ProseMirror bridge** (#1115). Links, images and reference definitions now put
  the quoted structural title in `carveLinkTitle`, leaving `title` and
  `carveAttrOrder` to carry `{title=...}` without either value winning the same
  field. Older payloads using the overloaded `title` field remain readable.
- **A structural title and an authored `title` attribute no longer swap places
  across the ProseMirror round trip** (#1105, follows #1110). The two spellings
  share one wire field, and `carveAttrOrder` - the record of which one the
  author typed - was written untruthfully and never read, so `[a]: /u "T"` came
  back as `[a]: /u "T" {title=T}` and `[z](safe.html){title="T"}` as
  `[z](safe.html "T")`. Links, images and reference definitions all.
- **The Carve writer does not turn a generated heading id into source inside a
  footnote definition** (#1105). The redundancy test walked the document body
  only, so `[^a]: # h` was written back as `[^a]: {#h}` over an indented `# h`,
  an attribute line the author never wrote.

## [0.1.3] - 2026-08-18

### Security

- **A list-valued URL attribute is probed at every candidate, not at its head**
  (PART 9 §25, markup-carve/carve#1320, #1068). The value probe read only the
  leading scheme, which vouches for the whole value only where the whole value
  is one URL, so `srcset="safe.png 1x, javascript:alert(1) 2x"` passed on its
  second entry. `srcset`, `ping`, `imagesrcset` and `archive` are now split and
  every candidate is read.

### Added

- **Composite figures: a bare `::: figure` container is one figure of ordered
  panels** (PART 9 §4c, markup-carve/carve#1122, #986, #1008). Its captionable
  children are the panels; the `^ ` caption after the CLOSING fence captions the
  group, which is one numbering unit, so `</#id>` on a panel resolves as
  "Figure 2a". HTML renders `carve-figure-group` / `carve-figure-panel`, the
  other targets degrade per PART 11 §10g, AST JSON gains the additive
  `figure_group` node (PART 12 §16), and the HTML importer reads the shape back.
  The ProseMirror bridge has no editor node for it and degrades to the generic
  container, reporting the grouping. `carve lint` reports an opener carrying a
  title or `[label]`, a nested group, and a panel-caption placeholder.
- **Delimited inline comments, `{% … %}`** (PART 9 §21a,
  markup-carve/carve#1239, #1010, #1019). A behavior change: `foo {% bar %} baz`
  used to render its braces and now renders `foo  baz`. The first `%}` closes;
  an unterminated opener stays literal and verbatim contexts stay opaque. The
  AST and canonical writer keep the spelling with `delimited: true`, the
  ProseMirror bridge carries the flag both ways, and the new
  `braced-comment-in-a-template-source` lint rule mitigates Liquid, Nunjucks or
  Twig source reaching the parser as text.
- **A ProseMirror/Tiptap bridge: `to_prosemirror`, `from_prosemirror`,
  `ProseMirrorDoc`, `ProseMirrorError`** (#993). Outbound reports `dropped` and
  `degraded`; inbound treats an unknown node name as an error rather than a
  skip. Node and mark names come from the carve-grammars map vendored under
  `resources/`. On the shared corpus 791 documents round-trip to byte-identical
  HTML and 215 report what they lost.
- **The bridge keeps the attribute run the author typed, and carries a mark with
  no content** (markup-carve/carve-grammars#240, #1030, #1034). An attribute an
  editor added is appended rather than dropped, a generated heading id is no
  longer written back as authored, an admonition's kind is no longer duplicated
  as a class, the `code` mark takes `id`/`class`/`carveAttrOrder`, and an empty
  mark rides the `carveEmptyMark` atom instead of being deleted in silence.
  Value quoting and class interleaving are not recoverable and are not faked.
- **`lint_carve` / `lint_carve_with_options` and `LintWarning`**
  (markup-carve/carve#1131, markup-carve/carve#1132, #972, #974, #979). Two
  rules, `semantic-attribute-value-ignored` and
  `semantic-attribute-outside-span`, with ids and messages matching carve-js'
  `lintCarve` and values quoted the way the renderer emits them, cut at 120
  characters. Both are tier-aware, which is why the `_with_options` form exists.
  Neither is a rendering change. `start`/`end` are BYTE offsets, not the
  codepoint offsets `Pos` carries.
- **A bibliography definition line is a node:
  `BlockNode::CitationDefinition`, `citation_definition` on the wire** (PART 12
  §18, markup-carve/carve#1279, #1031). The Citations extension used to collect
  the line and drop the paragraph, so it was not in the tree and an AST round
  trip deleted it. Rendered output is unchanged.
- **`Table::row_groups`, and the HTML importer states one where a reader cannot
  derive it** (PART 12 §15, markup-carve/carve#1210, #1003). The field is
  emitted only where the stated partition and the usual head/body derivation
  disagree; a partition the field cannot describe is refused with
  `table-degraded` rather than described wrongly. `structure-unspellable` is a
  new import diagnostic code, and §15's summing MUST is enforced in `from_json`,
  where an untrusted payload can contradict its rows.
- **An unattached block attribute is dropped in the container it was written in,
  and the drop is reported** (PART 9 §15 A4, markup-carve/carve#1281, #1038).
  A floating attribute is scoped to its container, so `> q` / `> {.k}` publishes
  the quote and drops the set. Dropping silently is the one thing it may not do:
  the new `unattached-block-attribute` lint rule reports it once per stacked
  run, from a record the parser keeps rather than by re-deriving the rule.
- **The `{:TAG}` language attribute** (markup-carve/carve#1114, #934). `[x]{:fr}`
  is exact sugar for `{lang=fr}` on spans and block attribute lines alike, `{:}`
  desugars to `lang=""`, and a malformed tag leaves the block literal. Shipped
  without an entry when it landed; recorded here.
- **`extensions::semantic_span::SemanticSpan`** (PART 9 §10, #948). The four
  names core does not reserve - `samp`, `var`, `cite`, `dfn` - under the same
  rules core uses for `abbr`, `time` and `kbd`, plus `:name[…]` for all seven as
  a soft-deprecated spelling scheduled for removal in 0.2.
- **`SmartQuotes` extension** (#914), matching carve-php's locale configuration:
  20 locale sets, exact-locale then language fallback, English for an unknown
  locale, chainable per-quote overrides. Apostrophes stay U+2019.
- **`CodeGroup` (#903), `HeadingReference` (#905), `Tabs` (#906) and
  `HeadingLevelShift` (#898) extensions**, ported from carve-js and carve-php,
  closing the last extension gaps between the three engines. Each degrades to a
  headed `<section>` per panel in static mode.
- **`djot_to_carve`, a Djot to Carve migration** (#916, #926, #929, #998), and
  **`carve migrate --from djot` to run it** (#936). Rewrites only the inline
  delimiters that mean different things in the two languages, converts an
  intraword `_x_` to the braced `{/case/}`, escapes a `#` so Djot prose does not
  become a Carve tag and an at-sign so it does not become a mention (#1063), and
  freezes a braced run whose opener never closes on its line. `migrate` is also
  listed in `-h` for the first time.
- **An AST-first HTML importer and migration CLI** (#902), **a Markdown importer
  parsed to an AST** (#931), **structural AST merge and patch APIs** with
  `carve merge [--json]` (#893), **a name-keyed registry of the built-in
  extensions** (#923), and **structural short captions in AST JSON** (#921).
- **`ALL`, `as_str` and `from_name` on the four HTML import report
  vocabularies.** The wire spellings were a private table inside the binary and
  are now the enums' own. No emitted byte changes.

The HTML importer gained a mapping for most of what it used to unwrap or drop
in silence. Each of these was a loss the reader was never told about:

- **A table's own `<caption>`** (#985) - what pandoc emits for every captioned
  table, walked into and discarded.
- **`colspan` and `rowspan`, as the continuation cells Carve already has**
  (markup-carve/carve#1210, #1000), with four further defects on that path: a
  rowspan crossing its row group, one leaving the derived head, `rowspan="0"`,
  and unbounded spans. A row shorter than the spans reaching into it reports the
  cell it invents, and a second `<caption>` says which one was kept.
- **A table's sections and rows keep the attributes they have a slot for**
  (#1006, port of markup-carve/carve-js#1096); a `<thead>`, `<tfoot>` or empty
  section that carries some is reported by name, and **a `<colgroup>` now says
  it was dropped** rather than vanishing.
- **A `<math>` element imports as the TeX it carries** (markup-carve/carve#1210
  D6, #1001, #1009): an `<annotation>` with a TeX encoding, else `alttext` with
  the new `encoding-assumed` info, else dropped in `safe`/`semantic` and raw in
  `roundtrip`. It used to unwrap, so one half arrived as `12`.
- **A definition list** (#989), **`<details>/<summary>` as a `::: details`
  admonition** and **`<q>` as the marks it renders as** (#994), **`<ol type>` as
  a numbering style and `<ins>` as `{+ +}`** (#992), **`<figure>` from another
  producer and a blockquote's `cite`** (#1033), and **word-processor
  footnote-shaped HTML as real footnotes** in the `word` and `google-docs`
  adapters (markup-carve/carve#1210, #1012).
- **The seven semantic elements as the span attribute that spells them**
  (PART 9 §10, markup-carve/carve#1140, #973), and **an authored table-cell
  `scope`** (#953). A `scope` that merely restates the positional default is
  still dropped.
- **Every attribute the language can hold, `aria-*` and unknown names included**
  (maintainer ruling on markup-carve/carve-php#1337, #1065). The keep list is a
  refusal list now, derived from `is_dangerous_attr_name` so the importer and
  the renderer cannot drift, plus `srcset` and any name the writer's identifier
  rule would silently rewrite. Every site the widening reaches names what it
  could not carry, so a reported loss does not become a silent one.
- **A bare-text `<li>` imports TIGHT** (markup-carve/carve#1210, #1016). Only a
  direct `<p>` votes, and a mixed list stays loose the way CommonMark resolves
  it.
- **A diagnostic's `path` is rooted at the imported fragment**
  (markup-carve/carve#1257, #1015). `<html>`, `<head>` and `<body>` contribute
  no segment and no sibling position, and both import budgets stop being spent
  on them: `<p>x</p>` needed `max_nodes` 5 and now needs 2.

### Changed

- **At a container's content column, a block ends the paragraph it sits under**
  (PART 1 S4, PART 9 §24 C3, markup-carve/carve#1357, #1093). What the line
  RENDERS is not a parameter and the question is asked of the BLOCK, not of the
  line's spelling. Same reading as carve-php, arrived at independently.
- **A flatten preserves the boundary it dissolves** (PART 11 §1b,
  markup-carve/carve#1325, #1094). A producer flattening block content into an
  inline-only slot - a caption, a fence title, a table cell, alt text, a
  definition term - emits a separator between two former siblings that each
  contribute a token.
- **A line block hardens a soft break at every depth** (PART 9 §23,
  markup-carve/carve#1351, #1090). The conversion ran on a stanza's top-level
  nodes only, so a closed inline construct spanning a boundary kept the bare
  newline.
- **The Markdown target's authored escape narrows on the line** (PART 11 §8b,
  markup-carve/carve#1322, #1069), and **a line's content position is read after
  the container prefix** (markup-carve/carve#1331, markup-carve/carve#1330,
  #1074). `C\# is a language` is written `C# is a language`, and `> \# heading`
  does not come back as a heading. The ATX probe that decided this was quadratic
  on a line of adjacent hashes (12.19 to 0.29 us/byte at 100k).
- **The Markdown target escapes `<` only where it would open markup** (PART 11
  §8a M1e, markup-carve/carve#1148, #951) - a backslash before an ASCII letter,
  `/`, `!` or `?` - and **leaves a bare ampersand alone** (#881), since an
  entity in Markdown text decodes to a character and a character cannot open a
  tag.
- **A table cell's attribute block binds after the kind and alignment markers**
  (PART 9 §5 T10, markup-carve/carve#1226, #996). `|={.total} Total |` sets the
  cell's attributes, where the braces used to reach the output as text, and an
  attributed HEADER cell has a spelling at all for the first time. This
  REINTERPRETS one released spelling: `|{#x}< content |` is no longer aligned,
  the `<` is content, and the output does not move. `carve fmt` rewrites the old
  form.
- **Every table cell pads its content in the canonical form** (#999).
  `|=Heading|` is written `|= Heading |`; the prefix still touches the opening
  pipe, and an empty cell takes a single space.
- **Every `<th>` carries a `scope`** (PART 10 §T9, markup-carve/carve#1159):
  `col` in the head-row run, `row` below it. An authored `scope` replaces the
  emitted one, matched case-insensitively.
- **The semantic-span registry splits by tier, and leftover attributes ride the
  element** (PART 9 §9, markup-carve/carve#1162, #946). `:kbd[Ctrl+C]` is the
  generic `ext-kbd` span in a core render, core keeps `abbr`, `time` and `kbd`
  as attributes, and `[x]{#k .key kbd}` is `<kbd id="k" class="key">x</kbd>`
  with no wrapper.
- **`code` and `mark` leave the built-in semantic registry** (#943,
  markup-carve/carve#1146), because Carve already has an inline spelling for
  both. `:code[*b*]` and `` `*b*` `` used to produce one tag with two content
  models and nothing reported the switch.
- **Compact semantic span attributes are portable core syntax** (#928), and
  **the nine-name semantic inline registry is spec- and corpus-pinned across all
  engines** (#925).
- **A referenced abbreviation definition splits by target** (PART 11 §10f,
  markup-carve/carve#1185, #971). Plain text and the terminal drop the
  `*[TERM]: expansion` line and print `TERM (expansion)` at each occurrence;
  Markdown keeps both, because that spelling is PHP Markdown Extra's own.
- **Bidi control characters are stripped from presentation targets** (#883), so
  Trojan-Source reordering cannot survive into plain-text, ANSI or Markdown
  output, and **plain-text and ANSI preserve list structure** (#884).
- **A tab and four spaces resolve to the same column inside a list item**
  (#892).

### Fixed

Container boundaries, and what a line at a content column does. These are one
family:

- **A block that leaves no open paragraph ends the container it was written
  in, wherever it was written** (PART 1 S4, markup-carve/carve#1280, #1035,
  #1057). Eight kinds of marker-line content folded the next flush-left line in
  where `> # H` already ended the quote, and a marker-line attribute no longer
  pulls a line in to have something to attach to.
- **Prose after a block in an item reopens the item's paragraph**
  (markup-carve/carve#1370, #1100). `- | a |` / `  b` / `tail` answered one line
  two ways, depending on whether the item's first block sat on the marker line.
- **A block quote's `+` marker attaches ONE block, the way a list item's already
  did** (PART 9 §17 L3, markup-carve/carve#1290, #1036), measured by parsing one
  block rather than by a second line scan. The extent probe then had to read a
  closer, and to take the attribute run with it (#1037).
- **A block-attribute line after a `+` continuation marker is an attribute
  block** (markup-carve/carve#1238, #1020, #1028), **a block-attribute line
  before a NESTED LIST reaches that list** rather than vanishing, and **a
  flush-left attribute line is not paragraph text** (#1013).
- **A wrapped attribute block behaves like the single-line one**, at the top
  level, in a definition body and in a block quote (§15 A5, #1046, #1058). It
  used to fold into the open paragraph as literal text, and inside a quote it
  attached forward and out of the container.
- **A comment fence hides its body wherever it sits, not only at column 0**
  (PART 9 §24 S1 and §28, markup-carve/carve#1311, #1052, #1061, #1064), and
  **a definition inside a QUOTED comment fence registers nothing too**
  (markup-carve/carve#1341, #1081) - `> %%%` / `> [r]: /url` / `> %%%` used to
  register the label. A list marker at the item's content column inside a body
  no longer severs the chunk, and the opener gate asks which live column the
  fence reaches rather than comparing against the innermost one.
- **A definition's column is reached by composing the strips** (PART 9 §24 C5,
  markup-carve/carve#1368, #1099), so a definition behind five of the 62
  quote/list prefixes registers instead of reading as lazy paragraph text, and
  **the column question is asked inside the quote, and again at each depth**
  (#1089).
- **The definition pre-pass asks the block parser whether a paragraph is open**
  (#1048). A line was kept as lazy paragraph text and consumed as a footnote
  definition at once, so the author's words disappeared and an endnote nobody
  wrote appeared. The probe budget that bounds it is priced in bytes, because a
  probe costs what it parses (#1073).
- **A container's extent reaches the definition it hosted** (#1108). The
  prepass' invisible placeholder was written at the wrong column behind an
  alternating prefix, so `list` and `list_item` ended a line early in the AST -
  8 corpus documents where carve-js and carve-php agreed with each other. The
  tree was never wrong, only the span.
- **`collect_definition_body` was quadratic** (#1093). It rebuilt and re-parsed
  the body once per lazy line, so a paragraph continued lazily under a `:  `
  body cost 22.9 seconds at 32 KB; it is now 35 ms. Pre-existing and invisible
  to the corpus, which has no document long enough.

Rows, cells, tabs and verse:

- **A verbatim run in a table row is opened by N backticks and closed by exactly
  N**, on the row and across a `+` continuation (PART 9 §22,
  markup-carve/carve#1284, #1041, #1044, #1059). A parity toggle per backtick
  CHARACTER meant one backtick worked, two did not, three worked again; the cell
  is now assembled before it is parsed and the run's WIDTH crosses the boundary.
- **A table row's escaped closing pipe is an escape** (markup-carve/carve#1293,
  #1045). `| a b \|` published a `<br>` where the author wrote a pipe.
- **A table's HEAD row resolves its continuation cells.** `<` and `^` rendered
  empty `<th>` cells, losing the span and gaining a column the table does not
  have; a resolved continuation is transparent to the head/body split too.
- **A trailing tab on a line that takes no content is dropped** (PART 2, #1040,
  #1042). A fence closer padded with a tab was swallowed as body text, a fence
  opener ending in a tab opened nothing, and `---` plus a tab was refused as a
  frontmatter opener while still reading as a thematic break.
- **A definition term's continuation line drops its trailing whitespace**
  (markup-carve/carve#926, #1043).
- **A verse comment is decided at the block layer, and a verse hard break is
  bare only where the newline gives it back** (markup-carve/carve#1333,
  markup-carve/carve#1334, markup-carve/carve#1340, #1077). A comment-only body
  line left to the inline parser could be claimed by §21's verbatim exclusion,
  publishing the comment's own text. The comment then had to keep its text
  wherever its boundary ended up, since counting top-level `hard_break` nodes
  missed two of the three spellings a boundary has (#1086).

References, notes and inline content:

- **A footnote inside an unresolved reference is not a reference**
  (markup-carve/carve#1198, PART 9R R2, #978). Footnotes were numbered before
  the reference was known to have resolved, so a discarded link text published
  an endnote with a backlink to an anchor no element carries. 23 documents
  changed; the AST's `number` moves with the rendering.
- **A reference inside an inline note, a critic insertion or a critic deletion
  resolves** (PART 9 §16, markup-carve/carve#1203, #983). §16 disables FOOTNOTE
  recognition, not reference resolution.
- **A reference tail no longer seals its own link text**
  (markup-carve/carve#1196). `[t[x][r2]][r]` now agrees with `[t[x][r2]](/u)`.
- **A footnote reference no longer crosses a source newline** (#917), and **an
  abbreviation expands inside a span** (markup-carve/carve#1151, #955), which
  `[HTML]{.x}` and `[HTML]{kbd}` silently dropped.
- **A heading's math contributes its text to the derived id** (#1032). All three
  text flatteners swallowed `InlineNode::Math` through their catch-all.
- **An authored `abbr` wins on the Markdown, ANSI and plain targets**
  (markup-carve/carve#1176, #958), per the markup-carve/carve#1127 ruling the
  HTML renderer already honored.
- **A math span's base class keeps the class slot in place** (PART 10 §1,
  markup-carve/carve#1164, #956), and **the `ext-NAME` base class joins the
  author's class slot** (#949) instead of jumping ahead of the id.

Canonical form and the non-HTML targets:

- **`carve fmt` no longer escapes into a run the reader reads raw** (PART 11 §2,
  markup-carve/carve#1197, markup-carve/carve#1206, #981). An alt text, an
  admonition label, a div label, a code-fence label and a footnote id each grew
  a backslash per pass, so `fmt(fmt(x)) == fmt(x)` failed from the second pass
  on and two of them said something new each time.
- **`carve fmt` no longer escapes a caret before a bracket run that opens no
  note** (PART 11 §2, markup-carve/carve#1191, #980). Four shapes published an
  escape nobody wrote and disagreed with carve-js and carve-php.
- **`carve fmt` writes a code fence with no space before its info string**
  (#987). It emitted the Djot spelling, rewriting ` ```rust ` to ` ``` rust `.
- **The writer carries a trailing comment instead of re-grafting it from the
  source** (#1083), which wrote it twice and put the second copy on an earlier
  line.
- **Verbatim sigils stay text on migrate** (markup-carve/carve#1130, #1016). A
  paragraph ending in `$`, `$$` or `!` in front of a code span migrated to
  inline math, display math or an inline literal the source never spelled.
- **A code block resolves the no-break-space sentinel instead of emitting it**
  (PART 12 §3, markup-carve/carve#1262, #1017). HTML wrote U+E000 into
  `<pre><code>` and Markdown leaked it from a code span, an inline literal and a
  fenced block; both now match carve-js and carve-php byte for byte.
- **Presentation targets no longer discard authored text** (PART 11 §10e,
  markup-carve/carve#1179, #959, #967). A table caption vanished on Markdown and
  a fence title and label on plain text and the terminal. The caption is body
  text under the table separated by a blank line, without which a GFM reader
  reads it as another row.
- **A nested list is indented once on the Markdown target, not twice** (#874).
  Three levels came out at ten spaces, which every reader but Carve reads as an
  indented verbatim block.
- **The Markdown writer leaves no trailing space on an emptied marker** (#999) -
  `"-"` and `":"`, not `"- "` and `": "`.
- **A container-closed fence keeps an authored trailing blank line** - also when
  the fence ends with a list item, in document-level source rebuilds, and in the
  mapped collector (#909, #910, #911, #915).
- **Adjacent sibling lists stay separate through fmt** (#900), **lazy lines fold
  into deep item paragraphs** (#896), and **a structural `<ol>` attribute leads
  the author's own** (#895, markup-carve/carve#1090, PART 11 §5.1).
- **A value-less attribute is written as a boolean and LANG is no longer
  folded** (#938), and **an attribute needs a separator before it** (#942).

AST and interchange:

- **A task item's checkbox is not decided by its first block**
  (markup-carve/carve#1381, #1104). The serializer built the `<li>` opener in
  two branches and only the inline one consulted the checkbox, so every
  non-paragraph lead wrote it at column 0.
- **`attrs.keyValues` serializes in the author's source order** (#975). The
  `BTreeMap` behind it was iterated directly, so one `attrs` object stated two
  different orders and the renderer already agreed with the second.
- **A block-level HTML element imported from Markdown stays inside the container
  that holds it** (#970, alongside markup-carve/carve-js#1045). It was emitted
  past the enclosing quote, item or footnote definition and landed at the top of
  the document; the same catch-all no longer leaves an empty paragraph per HTML
  element either.
- **A tight list item imported from Markdown holds one paragraph, not one per
  inline node** (#976), and a construct written inside an image alt contributes
  its text to the alt rather than to a paragraph ahead of the image.
- **An unresolved caption-number placeholder renders as the literal `#` in
  HTML**, matching what the other targets already emitted.

## [0.1.2] - 2026-08-10

### Breaking

- **A frontmatter block whose opener named no format is written back as
  `---yaml`** (markup-carve/carve#1040). PART 11 §6b spells the format token on
  the opening delimiter for every format, the default one included, and says of
  the untyped opener that "A READER'S LENIENCY IS NOT A WRITER'S LICENSE". This
  writer reproduced the opener as authored, so `---toml` came back typed and a
  bare `---` came back bare - the special case for one value the clause removes.
  The closer stays bare, and a document that has no frontmatter grows none.

- **A blank line inside verbatim content carries no structural indent**
  (markup-carve/carve#1040). PART 11 §7 emits the indent of an EMPTY verbatim
  line as nothing: "that is layout, and it is omitted". Only the list item
  applied it; a fenced block under a footnote definition or a definition-list
  description came back with a whitespace-only line, which editors that strip on
  save and CI whitespace checks rewrite behind the formatter. A block quote keeps
  its `>` - an empty line there would close the quote and take the open fence
  with it.

- **An inline comment is written with the space that separates it from the
  construct before it** (markup-carve/carve#1028). `{,y,} %% c` was written back
  as `{,y,}%% c`, so the bytes changed for any document whose comment follows
  emphasis, a link, an image, a span or math. It is a repair, not a preference:
  `%%` opens a comment only at the start of a line or after whitespace, so the
  glued form re-parsed as literal text - `<p><sub>y</sub>%% c</p>` where the
  source rendered `<p><sub>y</sub></p>`, PART 11 §1 failing on the writer's own
  output.

  The writer already put one space back, but decided by asking the previous NODE
  for its last character, and those five kinds report none - indistinguishable
  from "nothing precedes me". It now decides on the bytes already emitted for the
  line, which is the test PART 11 §1a states.

- **The AST now publishes `thematic_break.marker` and the Carve writer
  reproduces it** (markup-carve/carve#976). Parsed `***` and `___` carry `*`
  and `_` respectively; the default `---` leaves the optional field absent.
  AST ingest accepts the field and defaults an absent one to `---`.

- **AST ingest refuses every property the schema does not name, including the
  ones it used to understand** (PART 12 §11, markup-carve/carve#743;
  carve-rs#820). Three spellings were accepted and now reject at decode:

  - `footnote.id`, read as an alias for `label`. carve-php always refused it, so
    a document decoded in two engines and failed in the third - the interchange
    break §3's "field names are spec surface" exists to prevent. **No engine
    publishes it today**: carve-js, carve-php, carve-rs and pandoc-carve all
    write `label`, and carve-php cannot even read `id` back. Only a tree stored
    before §7 settled the spelling carries it, and rewriting `id` to `label` on
    such a tree is the whole migration.
  - the LEGACY definition entry, `{terms, definitions}`. The record has no
    `type` - the schema gives it none - so it was invisible to a check keyed by
    `type`, and any field on it was accepted. The form itself still decodes; it
    is now closed to `terms`, `definitions`, `definitionLines` and
    `definitionSpans`, the set carve-js closed it to.
  - the CITATION record inside `citation_group.items`, which the schema names
    and closes but likewise gives no `type`. A citation carrying any extra
    property decoded and the property vanished in silence.

  The check now covers every untyped record the schema closes, generated from
  the schema rather than listed by hand, so a new one cannot arrive unguarded.
  Two decoder fallbacks that could never fire are gone with it (`code_block`
  reading `title` for `header`, `inline_extension` reading `children` for
  `content`): the unknown-field check refused those spellings before either
  could be consulted.

- **`BlockQuote` loses its `attribution` field** (carve-rs#832). Code that builds
  a `BlockQuote` with a struct literal drops `attribution: None`; code that read
  the field has nothing to read. It was never set to `Some` anywhere in the
  crate, no engine publishes an `attribution` property, and the AST schema does
  not name one - `block_quote` carries only `type`, `children`, `attrs` and
  `pos`, and ingest refuses the property like any other the schema does not
  name. So nothing an author writes, renders or ingests moves; the removal is
  visible only to a Rust consumer that names the field. A quote's attribution
  line is, and stays, an ordinary second paragraph.

- **`Span` and `RawInline` each gain an `injected` field.** Code that builds
  either with a struct literal, or matches one exhaustively, needs the extra
  field (`injected: false` for anything an author wrote). It records that a
  render-stage transform put the node there rather than the author, which is
  what lets a derived display text leave it out (PART 9R R4). Like
  `Link::from_crossref` it is a render-time fact and is never written to or read
  from the AST JSON, so the wire format is unchanged.

- **A heading ends at the newline** (markup-carve/carve#451,
  markup-carve/carve#434). Nothing folds into a heading, so `# Title` with prose
  beneath is a heading plus a paragraph, and its id comes from the heading line
  alone (`Title`, not `Title-Some-text`). Anything with a blank line after the
  heading is unaffected.

- **AST vocabulary: two node shapes change.** Rendered output does not move.

  - An escaped character is its own `InlineNode::EscapedText` instead of being
    folded into surrounding text. Consumers reading `Text` see the run split at
    each escape.
  - `::: |` is a `BlockNode::LineBlock` instead of a `Div` carrying a
    `.line-block` class. Consumers matching on the class must match on the
    variant. The HTML is still `<div class="line-block">`.

- **An inline attribute block's interior is space-only, and a quoted attribute
  value stops at the newline.** A tab at any of the five inline positions - after
  `{`, between two attributes, before `}`, after an unquoted value, and in the
  blessed empty block `{ }` - leaves the block unrecognized and its braces
  showing, and a no-break space no longer separates two attributes. A line break
  inside a quoted value ends the production, so `{k="a` / `b"}` above a paragraph
  is literal text where this engine used to attach `k="a b"`. The
  block-attribute LINE keeps `whitespace` at all three of its slots.

- **A reference definition is anchored at end of line.** `reference_definition`
  ends in `newline`, so anything left over makes the production fail and the line
  is an ordinary paragraph: `[a]: /u zzz`, `[a]: /u<TAB>"T"` and
  `[a]: /u<SP><SP>{.c}` are paragraphs now, as are the tab-first and mixed-run
  spellings at both slots. A trailing run of spaces or tabs is still the line
  ending. `[a]: /u{.c}` is still a definition whose destination reads the braces.

- **A reference definition carrying an unparseable attribute block stops
  defining, and the braces stay on the page.** `[a]: /u {#}` renders as a
  paragraph and an `[a][]` beside it no longer resolves; `{ }` and `{=}` answer
  alike. A VALID block still defines and still transfers its attributes.

- **A definition body ends below its own content column.** A line indented one or
  two columns under a `:  ` body used to fold in as lazy text, giving a sub-column
  indent the past-the-column band's meaning. Below the column the body now ENDS:
  `:: t` / `:  body` / ` > q` renders the definition list and then the paragraph
  `> q`. A flush-left line still folds, a line AT the column still opens a block
  inside the `<dd>`, and a line PAST the column is still lazy text. A tab spells
  the same columns a space does: §24 C1 gives a tab a column value, so a bare tab
  reaches column 4 and is PAST the body's column exactly as four spaces are. The
  form-A dedent used to consume the tab whole and leave the residue FLUSH LEFT,
  where a `>` is a block opener, so the two spellings of one column produced two
  documents; the tab's residual columns are written back as the spaces it bought
  past the margin, which also moves the dedented line's published position to the
  source column the tab really sits at.

- **Four padding slots take exactly one space** (PART 7). `link_title` (inline
  and at a reference definition), `image_title`, the code fence opener's slot
  before its info string, the frontmatter opener's slot before its format token,
  and the reference definition's slot before a trailing attribute block. So
  `[t](/u<SP><SP>"T")` is literal text, a two-space code fence opener falls back
  to an inline verbatim span, and a two-space frontmatter opener is paragraph
  text the metadata lines fold into. Slots spelled `space+` still take a run.

- **An ingested tree is bounded by what its payload cost, not by what the payload
  claims it cost** (carve-rs#811). Two caps were sized from `srcByteLength`, a
  number that arrives inside the payload. The expansion budgets (abbreviations,
  table of contents, index) are `max(1 MB, 8 x source length)`, and rewriting that
  one number to `1000000000` took a 214 KB payload from 1.04 MB of HTML to
  101 MB, 472x, for nine extra bytes; the profile's `max_length` check had the
  same shape, so `Profile::minimal()`, whose whole job is to cap input at 10,000
  bytes, accepted an 80 KB payload that claimed to have come from nothing.
  `from_json` records what the payload actually cost: the budgets take the
  smaller of the claim and the measurement, and the profile check takes the
  measurement. `srcByteLength` is still read exactly as written and re-encoded
  unchanged, because PART 12 §7 makes it a field of the payload; only the caps
  stop trusting it. The `max_length` check also moves ahead of the
  `before_render` hooks, so a cap on untrusted input is answered before the
  table-of-contents and index hooks traverse and allocate from the tree.

  **Behavior change:** under a profile with a `max_length`, an ingested payload
  larger than the cap is refused through `prepare_document_for_render` where it
  used to render, and an ingested tree gets a budget sized from its payload.
  Nothing on the parse path changes, and the ceiling binds on none of the 830
  corpus documents.

  **API:** `ast::Document` gains a public `ingest_payload_len` field and the
  `expansion_budget_len()` / `untrusted_input_len()` accessors, so code
  constructing a `Document` with a struct literal needs the new field, and code
  sizing a cap should read an accessor rather than `source_len`.

- **`fmt` writes `---` for a thematic break that opens the document** (PART 11
  §6a). The `***` fallback existed to keep a leading `---` from being read back
  as a frontmatter opener, and now fires only when the bytes about to be emitted
  really would open one - a frontmatter opener needs a CLOSER. `***` alone
  formats to `---`; `***` followed by a later `---` line still formats to `***`.

### Added

- **`Options::with_sections(false)`** renders headings without the `<section>`
  wrapper (markup-carve/carve#427, PART 9 §13). The id goes back on the `<h*>`
  alongside its other attributes. Default unchanged. The endnotes
  `<section role="doc-endnotes">` is still emitted.

### Changed

- **A profile's `admonition` deny list matches only the eight Tier-1 callout
  kinds** (`note`, `tip`, `warning`, `danger`, `info`, `success`, `example`,
  `quote`), not every named fence. `::: sidebar` classifies as `div`. The
  published AST is unaffected. In the same pass, `autolink` and `admonition`
  became deniable BY NAME at all - both folded into `link` / `div` before the
  allow/deny check, so a host could deny autolinks, get no error and no
  violation, and still emit them.

  **Migration:** a host that wants the old blanket behavior denies both
  `["admonition", "div"]`.

- **Braces alone on a list-item marker line are a block-attribute line** (PART 9
  §15 A8, markup-carve/carve#457). `- {a=b .c}` followed by an indented block
  attributes that block, instead of rendering the braces as the item's lead
  paragraph and dropping the attributes. The discriminator is whether content
  follows the braces: `- {.c} text` is still literal, `-{.c} text` still
  attributes the item. carve-rs was the only engine reading the brace-only form
  as text. A tight item's paragraph is wrapped in `<p>` when it carries authored
  attributes, which otherwise had nowhere to go.

- **An abbreviation definition written inside a container is a child of the
  document** (PART 12 §7), as a footnote definition already was, and it now
  EXPANDS. Two defects met: definitions were collected from the document's
  children alone, so one written inside a div, list item or block quote was never
  collected; and the expansion pass had no arm for a `:::` div, a block extension
  or a definition list, so an abbreviation never expanded inside one even when the
  definition sat at top level. Both rendered as plain text where carve-js renders
  `<abbr>`. `fmt` writes the definition after the container; `pos` still records
  where the author wrote it.

### Fixed

- **A short ANSI table row is padded out to the box** (markup-carve/carve#1044).
  The ANSI box draws its rules at the TABLE width, so a ragged table left the
  short row stopping mid-box with no right border:

  ```
  | h |
  |---|
  | |x |
  ```

  used to render

  ```
  ┌───┬───┐
  │ h │
  ├───┼───┤
  │   │ x │
  └───┴───┘
  ```

  and now renders

  ```
  ┌───┬───┐
  │ h │   │
  ├───┼───┤
  │   │ x │
  └───┴───┘
  ```

  The trailing cells a row does not have are a DISPLAY pad: nothing re-parses
  ANSI output, and a box has to be a rectangle to read as one. It is also what
  the HTML target already shows, since the table is two columns wide there. PART
  11 §10b forbids this same padding on the Markdown delimiter row because a
  reader parses that row; that reason is absent here, which is why the two
  targets settle it differently. AST row cell counts are unchanged, and the
  Markdown, plain and Carve targets still write each row's own cells.

- **A tab after a line-initial caret is not a caption slot, so the writer leaves
  it bare** (markup-carve/carve#1042 follow-up). A caption marker is a caret
  followed by a SPACE; a tab after it leaves the line as prose, which corpus
  `231-a-tab-after-a-heading-quote-or-caption-marker-leaves-the-line-as-prose-2`
  pins. The caret re-parses as text either way, so PART 11 §4 asks for the
  minimal form, and `carve fmt` on

  ```
  ![Moon](m.jpg)
  ^	Figure 1
  ```

  wrote a backslash before the caret where carve-js writes it bare. Nothing on
  the page changed; the divergence was in the canonical source, which PART 11 §2a
  requires the three engines to agree on byte for byte.

- **The Markdown separator row is sized from the header row, not the table**
  (markup-carve/carve#1042). PART 11 §10b says the delimiter "carries exactly one
  cell for each cell in the HEADER ROW, not one for each column reached by a
  wider body row", and the Markdown target sized it from the table width instead.
  A ragged table therefore emitted a separator wider than the row it promotes:

  ```
  | h |
  |---|
  | |x |
  ```

  used to write

  ```
  | h |
  | --- | --- |
  |  | x |
  ```

  which neither python-markdown nor marked reads as a table - the whole document
  published as a paragraph of pipes. It now writes

  ```
  | h |
  | --- |
  |  | x |
  ```

  and both readers render a table again. A header that is itself the widest row
  is unchanged, and the header's column alignment still reaches the separator.

- **An escaped space at the end of a line inside a list item writes back as the
  bare backslash** (carve-rs#855). PART 11 §2a wants canonical source that does
  not depend on an editor preserving a trailing space, so the escape expands to a
  lone backslash with nothing after it. That expansion ran at document level
  only, and the list writer indented first, so the space survived as
  mid-paragraph content:

  ```
  - item
  \
  x
  ```

  where the second line is a backslash followed by a space. That was written
  back with two spaces, a backslash and a trailing space, where carve-js and
  carve-php write two spaces and the bare backslash. The parse now discards trailing ASCII whitespace per physical line
  before escapes resolve, so the hard break the escape carries is the same
  before and after formatting.

- **A footnote definition with an EMPTY body now carries a position**
  (markup-carve/carve#1023, PART 12 §4). A definition's extent was derived from
  its body, and `[^f]: {empty}` parses to no blocks - so nothing was there to
  measure and the node went out with no `pos` at all. §4 permits omitting a
  position only for a node that cannot be placed; this one is written on a line
  of its own, so its extent is that line, which is what the reference publishes.
  A definition that HAS content keeps the extent its body gives it.

  The same gap moved a definition, not just its span: §7 orders collected
  definitions by source position, and one with no position sorted last - so
  `[^a]: {empty}` written above `[^b]: x` was published below it.

  AST ingest reads the definition's span back off the wire, so a decoded
  document publishes the position it arrived with rather than re-deriving one
  from an empty body.

  `Document` carries a new public field, `footnote_def_pos`, holding that span
  for the definitions whose body cannot supply one. Code that builds a
  `Document` with a struct literal has to name it; `Default::default()` is the
  value for a document with no such definition.

- Keep adjacent mergeable block openers separate when formatting a tight
  `+`-attached run, instead of collapsing two quotes or tables into one block.

- Preserve each row's cell count when formatting a ragged table instead of
  manufacturing empty cells to make the table rectangular.

- **`fmt` no longer manufactures a frontmatter block out of a promoted
  paragraph** (carve-rs#819). PART 11 §7 writes a hoisted link or footnote
  definition after the body, promoting whatever stood second to byte 0. When
  that block is a PARAGRAPH whose first line is `---yaml`-shaped, the emitted
  document opens a frontmatter block the input did not have and the next parse
  swallows everything down to the first bare `---`: `[a]: /u` / blank /
  `---yaml` / `k: v` / `---` rendered a paragraph and a rule, and after `fmt`
  rendered nothing at all. The previous guard could not see it - it asked
  whether the first rendered block was the string `---`, and a paragraph is not,
  and the line that has to move is the CLOSER four lines further down. The
  finished bytes are now handed to the parser's own opener test. **Behavior
  change:** when the emitted bytes would be read as opening a frontmatter block
  the document does not have, `fmt` writes every HYPHEN-spelled thematic break
  as `***`, not only the one at the head. Only the hyphen spelling can be read
  as a fence, so a break the author wrote `***` or `___` keeps its own marker
  even in that document, which is the smallest departure that restores the
  invariant. A document that is not misread is untouched, and one still misread
  with `***` keeps the authored spelling rather than paying a respelling that
  buys nothing. This is a deviation from §6, taken because PART 11 §1a makes §1
  the stronger clause.

- **`fmt` keeps the continuation marker on every block in a `+`-attached run**
  (carve-rs#819). The marker column is the item's own column, to the left of the
  item's content column, so once one child was written there every later child
  written at the content column was indented relative to it and read as its lazy
  continuation. `- x` / `+` / image / `+` / image came back as one item holding a
  single image paragraph with the second image's source as literal text, and with
  a caption on each, the second figure's whole source landed inside the first
  one's `<figcaption>`. The condition is the previous child's COLUMN rather than
  its kind: a block opener needs no marker of its own but still cannot sit two
  columns to the right of the block above it.

- **`fmt` writes a footnote definition with no blocks as `[^f]: {empty}`**
  (carve-rs#819, PART 11 §7b). The body empties whenever the definition line's
  whole body is a block-attribute run, which the line collects as attributes and
  discards. The writer emitted `[^f]:` with nothing after the colon, and that
  line is not a definition at all, so formatting the document lost BOTH halves:
  the definition came back as a paragraph and the reference to it came back as
  literal text. The sentinel is a valid attribute block, collected and discarded
  on the same line, so the note still renders empty and the reference still
  resolves. `{ }` and `{}` would not serve - a block-attribute line requires at
  least one attribute, so both stay literal text inside the note - and the
  spelling is pinned by the spec rather than chosen here, so all three engines
  write the same bytes. A body holding one block that RENDERS nothing, such as a
  comment, is not an empty body and is unaffected.

- **A profile's link policy reads the scheme through the characters a URL
  consumer discards** (carve-rs#835). `LinkPolicy::is_url_allowed` read the text
  before the first colon with no character filter, and `trim` only reaches the
  ends, so any control or whitespace character INSIDE the scheme defeated the
  denied-scheme lookup: `java<U+0001>script:alert(1)`, `java<DEL>script:` and
  `java<U+009B>script:` were all answered `allowed`, while the plain
  `javascript:alert(1)` was answered `denied`. The scheme is now read through
  `is_url_probe_skippable`, the same probe class the renderer settled on in
  carve-rs#833: every control character plus every whitespace character.

  Two further answers were wrong for the same reason and are now right: a split
  scheme was neither `http` nor `https`, so it also skipped the **denied-domain**
  check and the **`allow_external`** check, and `htt<DEL>ps://evil.com` was
  answered `allowed` under a policy denying `evil.com`. The link, inline-image
  and block-image gates share the one rule, so all three narrow together.

  This is a **narrowing only**. Filtering removes characters, so it can only make
  the deny lists recognize MORE destinations; **no destination a policy refuses
  today becomes allowed**. No legitimate scheme contains a filtered character -
  a URL scheme is a letter followed by letters, digits, `+`, `-` and `.` - so
  nothing legitimate starts being refused. The ALLOWLIST form deliberately still
  reads the raw text: it asks whether a scheme is exactly one it permits, a split
  scheme is not, and it was never defeated. It is unaffected here in both
  directions.

  A document rendered with default options was never at risk: PART 9 §25 blanks
  these destinations in the renderer no matter what a profile answered. What was
  affected is a caller using a profile to VALIDATE or FILTER, where the
  permissive answer is the whole output.

- **A denied URL scheme split by DEL or a C1 control is blanked** (PART 9 §25,
  carve-rs#833). `[x](java<DEL>script:alert(1))` reached the rendered `href`
  with the raw U+007F byte intact, and the image spelling reached `src` the same
  way, while the plain `javascript:alert(1)` was blanked correctly. The denylist
  was never wrong; the class of characters dropped before the scheme was read
  was. `is_url_probe_skippable` tested `(c as u32) <= 0x20`, which stops short of
  DEL and reaches only U+0085 of the C1 block, so a scheme split by any of the
  rest was invisible to it. The predicate is now `char::is_control`, which is
  the Cc category exactly, plus whitespace and the BOM - the same predicate the
  ANSI and Markdown targets were already applying to a destination before they
  probed it, which is why only HTML leaked. The SVG sanitizer's second copy of
  the same rule had the same gap, where it also defeated the
  reject-every-absolute-scheme check on paint attributes outright.

  This is a **defense-in-depth fix, not a demonstrated execution**: whether such
  a URL resolves depends on whether the consumer's URL parser discards the
  character before it reads the scheme, and consumers differ. The probe class is
  deliberately wider than the §29 emit class, because the two answer different
  questions - what a target may write, versus what the probe must see through.
  Filtering only removes characters, so the wider class can refuse more and can
  never permit more; a destination that is allowed is still emitted with its
  original bytes.

- **Two blank lines detach a caption** (PART 9 §4, markup-carve/carve#991;
  carve-rs#830). `caption_slot = [blank_line], caption` carries at most ONE
  optional blank line, and the scan read it as any number, so a `^ ` line two or
  more blank lines below its host attached anyway. A caption now attaches
  adjacent to its host or across exactly one blank line, and beyond that it
  detaches and stays an ordinary paragraph. One shared site served all five
  captionable hosts, so the table, the fenced code block, the blockquote, the
  image paragraph and standalone display math change together. Matches carve-js.

- **A footnote with an empty body carries a backlink** (PART 9 §16,
  markup-carve/carve#688; carve-rs#826). A body with no blocks at all now gets
  the synthesized wrapping paragraph the rule already gives a body whose last
  block is not a paragraph, so `[^f]: {x}` renders
  `<li id="fn1"> <p><a href="#fnref1" role="doc-backlink">↩</a></p> </li>`
  instead of an empty `<li id="fn1"> </li>`. The reference above it always
  rendered and always pointed at the note, so a reader who followed it had no
  way back. Three routes reach a zero-block body and all three are fixed: a body
  consumed as attributes, an ingested `"type":"footnote"` with empty `children`,
  and a profile whose disallowed action is Strip removing the body's every
  block. Matches carve-js and carve-php.

- **A definition body that holds no open paragraph does not take the lazy fold**
  (PART 0 §4, markup-carve/carve#956; carve-rs#790). The clause is stated on the
  PARAGRAPH, and only the FENCED body had been read that way. Seven more bodies
  fold no longer, each matching the LIST spelling of the same document, which
  already answered this way: an empty block quote, a closed div, a closed
  admonition, a table, a thematic break, a line block, and a body a
  block-attribute line left with no block at all. So `:: t` / `:  >` / `lazy`
  renders the empty quote inside the `<dd>` and `lazy` as a top-level paragraph,
  where it used to be a paragraph inside the `<dd>`.

  A HEADING still takes the fold, and so does a bare or captioned image: a
  heading is the exception `heading_folds_lazy.rs` pins, and an image line is a
  block only while nothing folds into it, which the following line decides.

- **`fmt` keeps a `+`-attached block that opens no block of its own** (PART 11
  §1, PART 9 §17 L3; carve-rs#819). The continuation marker was written back
  only for a PARAGRAPH after a paragraph, on the premise that nothing else can
  fold into one. An image line opens no block at the item's content column and
  folds like any other text, so `- x` / `+` / `![a](i.png)` / `^ cap` came back
  as `- x` / `  ![a](i.png)` / `  ^ cap`: the `<figure>` gone and the caption
  literal text. The bare image without a caption lost its block the same way.
  The writer now asks the parser's own opener test about the bytes it is about
  to emit, so a quote, a fence, a heading, a break and a div still get no marker.

- **`fmt` parts a header cell's marker from content that starts with an
  alignment sigil** (PART 11 §1; carve-rs#819). The header `=` is read glued to
  the pipe and the alignment sigil glued after it, so `| ~x~ |` - a header cell
  holding a strikethrough - was written as `|=~x~|` and re-read as a CENTERED
  column holding `x~`, centering every cell in the column by a marker nobody
  wrote. One space now parts them (`|= ~x~|`); only the marker's position
  relative to the pipe is significant. A cell that already carries alignment,
  and one with no prefix at all, are unchanged.

- **The Markdown target's escaping narrows on the line** (PART 11 §8a,
  markup-carve/carve#970; carve-rs#824). `_`, `#` and `[` are escaped if and only
  if the character is ADJACENT on the emitted line to an unescaped delimiter of
  the same character. So `company_id`, `C#` and `issue #123` are written as the
  author typed them, where before they were `company\_id`, `C\#` and
  `issue \#123` - a backslash inside an identifier breaks exact-match search in
  the published document and protects nothing a CommonMark reader would read
  differently. `a__b` and `[[x]]` keep both escapes, because unescaping would
  merge the two into one delimiter run.

  The ASTERISK is exempt and keeps its unconditional escape: this writer spells
  emphasis with `*`, so a literal asterisk can merge with a delimiter the writer
  itself just wrote. Nothing else narrows, and an author-escaped character is
  still emitted as an escape - including `\_`, which used to lose its backslash
  to the old intraword rule. Markdown output only; every other target is
  unchanged.

- **An escaped character reaches a heading's derived text** (carve-rs#800). It
  renders as visible prose, so it feeds the heading's title, its generated id and
  PART 9R R1's implicit `[label][]` index - and it fed none of them: the three
  projections behind those had no arm for it and dropped it silently.
  `# a\.b` published `id="ab"` and now publishes `id="a-b"`, and `[a.b][]`
  resolves to it. carve-js and carve-php already included it, so this was a
  one-engine divergence.

  **Behavior change:** a heading whose escaped character is a word separator the
  slug keeps a boundary for gets a different id. Where the escaped character is
  one the slug strips anyway - `# \*bold\*` - the id does not move. No corpus
  document changes.

- **Every derived display text clones the heading's nodes, not just the
  cross-reference label** (PART 9R R4 DERIVED DISPLAY TEXT CLONES THE SAME
  NODES, markup-carve/carve#957; carve-rs#782). A node carries the author's
  source run and a string does not, so flattening at the derivation site
  destroyed the emphasis, the code span and the escape before any renderer was
  invoked. For `# A *bold* h`:

  - a table-of-contents entry, both the injected `<nav>` and the `::: toc`
    placement directive, published `A bold h` and now carries the markup;
  - an index term's display published its term flattened and now carries the
    nodes the author wrote.

  A derived label is the heading's AUTHORED content, so nothing a later stage
  added appears in one: not a `section-number` span, not a permalink anchor, not
  a footnote reference, not a citation, not an abbreviation's expansion, and not
  an invisible `:index[term]` marker. An author's own `[v1]{.section-number}`
  span is authored content and stays. This also fixes a resolved `</#id>`
  publishing the target's permalink anchor inside its own anchor.

  A TOC entry is rendered by the render in progress rather than at derivation
  time, so it now obeys the caller's raw-HTML policy and symbols map as well as
  the typography mode it already followed. It is also escaped once, by that
  renderer: a `"` in a heading reaches the entry bare instead of as `&quot;`,
  which is what the heading itself already emitted and what carve-js emits.

- **A mention and a tag open no anchor inside a link** (PART 12 section 3a LINKS
  NEVER NEST). With `mention_url` / `tag_url` configured, `[see @bob](/u)`
  emitted an `<a>` inside the link's own `<a>`; a mention or tag inside any
  anchor now renders its template-less `<span>` form, which is what links,
  autolinks and cross-references already did there.
- **The Markdown and plain-text targets emit a non-whitespace C0 control instead
  of deleting it** (spec PART 9 §29 C0 CONTROLS ON THE RENDER TARGETS,
  markup-carve/carve#979; carve-rs#812). After markup-carve/carve#963 the
  whitespace of the language is exactly U+0020, U+0009, U+000A and U+000D, and
  every other C0 control - U+0000..U+0008, U+000B, U+000C, U+000E..U+001F - is
  ordinary content. Both targets stripped the whole class, so `a<VT>b<FF>c`
  rendered `abc` where the HTML target already rendered it whole; four Markdown
  readers were measured and all four keep these characters, so the strip made
  Carve the lossy party.

  **The ANSI target is unchanged and keeps its strip broad.** It is the one
  target whose consumer acts on the character, and §25 still requires DEL and the
  C1 controls to go with the C0 ones there. DEL and the C1 controls also still go
  on Markdown and plain, which §29 T5 leaves outside its scope.

- **A container a lazy line folded into is still open.** PART 1 S4's lazy branch
  folds a flush-left line into the innermost open paragraph and closes nothing,
  and that binds the lines after it too. This engine folded the line and then
  ended the container anyway, so a line that came back to the container's
  content column landed outside it: `- x` / `  :::` / `  a` / `d` / `  b` /
  `  :::` put `b` beside the div and printed a stray empty `<div>` for the
  closer, where the whole run is one paragraph in one div. The same shape with a
  block quote produced two quotes instead of one, a marker-line quote and a
  marker-line heading did the same through their own collectors, and one level
  of item nesting lost the construct to the top level. The governing parameter
  is an open paragraph anywhere in the stack, never the fence kind: an empty or
  a closed container still ends at the flush-left line, a closed container
  nested inside an unterminated one reads as closed, a colon-shaped line inside
  a code fence or a line block opens nothing, and a code fence body, which can
  hold no paragraph at all, is unchanged. A container nested inside a block
  quote is reached through the quote's marker, which it was not before, so
  `- x` / `  > :::` / `  > a` / `d` / `  > b` no longer spills the quoted tail
  out of the list as literal text.

- **A boundary line inside an open fence no longer ends the container**
  (markup-carve/carve#983 corpus category 279, markup-carve/carve#985,
  carve-rs#802). A `+` continuation marker attaches ONE block, and a fenced
  block ends at its closer - so a blank line, a sibling marker, a `>` quote line
  or the next definition written between an opener and its closer is fence
  content and ends nothing. The list item already answered this; the footnote
  body, the block quote and the definition body's two forms consulted no fence
  at all, so a code, `:::` or `%%%` fence with a blank in its body was cut in
  two in each of them - the opener left an empty block, the tail escaping to
  document level, and a code fence's closer coming back as an empty inline code
  span. All five now share one fence-aware scan. A list item's INDENTED body
  gained the colon container beside the code fence it already knew, so a list
  marker at the body's own column no longer splits a `:::` div around a nested
  list, and the looseness scan reads all three fence kinds, so a blank inside an
  item's own `%%%` or `:::` body no longer loosens the item that holds it.

- **Whitespace is a space or a tab, in every construct** (PART 7). Carve has
  exactly four whitespace characters and every other character is content, but
  this engine reached for Rust's wider classes at fifteen line-classification
  sites - so a line ending in a vertical tab or form feed still formed its
  construct and the character was eaten. A table row, a delimiter row, a
  colon-fence opener, a line-block opener, a hard-breaks opener, a block image
  line, a standalone block-attribute line, a list-item attribute line and a
  frontmatter opener all end at a space or a tab and at nothing else now, and an
  abbreviation expansion keeps a trailing no-break space, vertical tab or form
  feed. **Behavior change:** such a line is ordinary text where it previously
  formed the construct. The frontmatter case was the severe one - a yaml opener
  followed by a vertical tab opened a block that swallowed the document down to
  the next three-dash line.

  Two more producers had the same root. A no-break space survives the plain-text,
  Markdown and ANSI writers, which used `str::trim` and so deleted a character
  the author typed from a footnote body, a table cell and an ANSI figure caption
  while HTML and Carve kept it. And `fmt` writes back invisible characters
  instead of dropping them: a line whose only character was an OGHAM SPACE MARK,
  EN QUAD, THIN SPACE, HAIR SPACE, NARROW or MEDIUM MATHEMATICAL SPACE or
  IDEOGRAPHIC SPACE was written back EMPTY and re-read as a blank line, splitting
  its paragraph; every C0 control but tab/newline/return, DEL and the whole C1
  block were dropped from text. Only U+0000 is dropped now, and a leading byte
  order mark is written one column in.

- **Trailing whitespace is dropped on every content line, not only a block's
  last** (PART 2). The run before a SOFT BREAK survived until now, so `abc<SP>` /
  `def` and `abc` / `def` are the same document and render the same. Applies to a
  heading, list item, block quote line, definition term and description, footnote
  body line and table caption; a line block drops a one-column trailing gap. Only
  U+0020 and U+0009 drop - a no-break space, zero-width space, byte order mark,
  en quad, ideographic space, form feed and vertical tab are content. Verbatim
  payloads and the run before a hard-break backslash are unaffected.

- **A marker separator is a run of ASCII spaces, and the next character is
  content** (PART 5, PART 9). At both definition markers the first character that
  is not an ASCII space ends the separator and BEGINS the content, so
  `*[HTML]: <NBSP>Hyper Text` expands to a title starting with the no-break
  space and `*[HTML]: <TAB>Hyper Text` keeps the tab, where the expansion was
  previously trimmed and both were eaten. A tab AS the separator, and a marker
  with only spaces after it, are still paragraphs.

- **Every remaining opener slot takes a space, decided by position** (PART 7). A
  tab is syntax only inside a line's leading indentation run, so: the frontmatter
  opener's format slot (`---<TAB>yaml` is no longer a typed opener, and
  `---<NBSP>yaml` no longer opens frontmatter at all); the link-title slot in all
  four forms that share it, including the reference definition's trailing
  attribute block; every slot on a colon-fence opener line, where the separator
  was checked only at its first character so a lone tab was rejected while a
  space-then-tab was not; and all five table-cell productions
  (markup-carve/carve#910), where a tab stops being padding and becomes ordinary
  cell CONTENT - and at `delimiter_cell` the line stops being a delimiter row, so
  no header is promoted and the `---` run renders as an em dash. Cardinality is
  unchanged everywhere: a run of spaces still pads.

- **An autolink body admits non-ASCII** (PART 3). An internationalized domain
  (`<https://例.jp/>`), an accented host, a non-ASCII path and a non-ASCII
  non-letter now open an autolink instead of staying literal text - the same
  destination written `[t](https://例.jp/)` already linked. A General_Category Cf
  character is excluded (invisible, so a host carrying one links somewhere else),
  and so is a control character - without that term the C1 block would be
  admitted while every C0 one stayed out.

- **Unicode whitespace ends a link destination, in both forms.** The byte scans
  tested ASCII whitespace only, so a narrow no-break space passed for an ordinary
  destination character. The balanced-parens scan is a separate path and was
  missed first time round, which made the rule depend on whether the URL
  contained a parenthesis - and is why this engine looked as though it treated
  dangerous schemes specially, since `javascript:alert(1)` is parenthesised.
  Zero-width characters (U+200B, U+FEFF) are not whitespace and stay.

- **A construct opens only AT its container's content column** (PART 0 S4, PART 9
  §24 C3). One rule, four shapes: a line below every content column is text
  rather than the block it looks like, keeping its own indentation instead of
  being dedented to column 0 (` # H` under `- - a` used to publish an `<h1>`); a
  below-column line folds at every depth, carrying exactly one column, so
  `-   x` / `    - a` / `  - b` no longer nests `b` under `a`; a post-blank line
  below the content column ends the list, which was skipped entirely for an item
  whose content is all on the marker line; and a blank line inside a marker-line
  item no longer ends its sub-list, which `carve fmt` reached by emitting a blank
  the source did not have - breaking both PART 11 §1 invariants at once.

- **A fence's body inside a list item is verbatim** (PART 9 §24 S1/S2). §24
  places a line by the COLUMN it reaches and does not read its first character,
  so a list marker at an item's content column, inside a fence that item opened,
  is the same continuation a plain line is. The item collector's marker test had
  no fence guard, so the marker SEVERED the verbatim body: the fence closed
  empty, the marker opened a sub-list, and the fence's own closer came back as an
  empty code span. **Behavior change:** a bullet, ordered or task marker on a line
  inside an item's open fenced code block is part of the code now, where it
  previously ended the block and started a list. A marker after a CLOSED fence
  still opens a sub-list.

- **No open paragraph, no lazy line** (PART 0 S4). A fenced body is not a
  paragraph, so a fence opened on a list item's MARKER line or on a `:  `
  definition-body marker line no longer swallows a below-column body and its
  closer: the container closes, the item or `<dd>` holds an EMPTY code block, and
  the residue re-parses at document level - the answer the block-quote spelling
  already gave. The guard is on the OPEN fence, so it reaches a fence opened on a
  CONTINUATION line and clears at the closer. Conversely, an unterminated `::: `
  div holding a paragraph HAS one, so a flush-left line after it folds in; a
  closed or empty div still ends the item.

- **A colon fence on a marker line opens** (markup-carve/carve#514). `- :::`
  published `<li>:::</li>` unless item-owned content followed it. An opener
  opens, closer or no closer, and an empty body is a container with nothing in
  it. What can stop it is the strict content-column rule, so `- :::` / `x` is
  still the literal `::: x`.

- **A glued colon fence holds back only a bare fence, not every opener.** A
  `:::note` or `:::]` is paragraph text, and it used to disable colon-fence
  interruption for the rest of that paragraph - so a real opener on the next line
  was swallowed as text, at top level, in a block quote, in a container and in a
  list item's lead.

- **A malformed colon fence inside an open container no longer absorbs that
  container's closer** (PART 9 §12). §12 lets a paragraph absorb a fence-shaped
  line once a failing opener has made it prose, but that rule is about a would-be
  OPENER; a CLOSER belongs to the block that opened it. `::: note` / `:::oops` /
  `:::` / `tail` put `tail` inside the admonition, and a longer document put
  everything there. Over 10,000 generated colon-fence documents this moves 216,
  every one onto the spec oracle's answer.

- **A fence-shaped line inside an opaque span no longer closes the container the
  span sits in** (markup-carve/carve#450). A `:::` written inside a code fence or
  comment block inside a `::: note` ended the note there, and the real closer,
  the rest of the body and the following blocks reparsed outside it. The body
  collector tested the span's own opener against the closer pattern, so a bare
  ` ``` `, `~~~` or `%%%` ended the span on its own line. This shows up exactly
  when documenting Carve in Carve.

- **Definitions are collected where the author wrote them, and only there.** Four
  shapes that used to register a definition the reader could still see, or lose
  one the reader could not:

  - A content column is live only inside the container it was measured in, so
    `- a` / blank / `>   [r]: /u` no longer registers a definition at the column
    of a list that had already closed.
  - A definition after an INDENTED `%%%` closer registers. The block parser
    closed the comment and the two line-based prepasses did not, so every
    definition after the closer went unregistered and came back as VISIBLE text.
  - A comment body seeds no list content column, so a `- hidden` inside a comment
    no longer leaves a phantom column that swallows and registers the line below
    the fence.
  - An unterminated comment fence opens no span (§28), so the collector no longer
    dedents the next line by a span that does not exist - `- a` / `  %%% x` /
    ` # h` published an `<h1>` where every other engine keeps `# h` as text.

- **An invisible construct in a list item does not decide its looseness** (PART 9
  §17 L1/L1b/L2). A comment cannot stand between a blank line and the paragraph
  after it, so `- a` / blank / `  %% n` / `  text` is LOOSE - the first-block
  check found the comment and called the item tight. A blank before a SIBLING
  marker still loosens the list whatever sits in the gap. And a sub-list lead no
  longer exempts its item from the rule: `- - a` / blank / `  b` went tight where
  `- x` / blank / `  b` went loose, on the same blank line.

- **A comment below a list item's content column keeps the item open** (PART 9
  §24 C3). Below that column every other construct folds as the text it looks
  like; a comment does not, and being invisible it closes nothing - so `- a` /
  ` %% c` / `b` is one item holding `a` and `b`, where the comment was ending the
  item and `b` came back as a top-level paragraph. A comment on a marker line is
  a block too: `- %% c` used to leave the item holding an empty paragraph, which
  published a whitespace-only line inside the `<li>` and made the canonical
  writer emit `- +`.

- **A block that renders to nothing leaves no blank line inside its container.** A
  comment, comment block, abbreviation definition or non-HTML raw block renders as
  the empty string and the container pushed the separating newline before it knew
  that. The div, admonition, line block, block quote, definition body and
  extension div each had their own copy of the loop; they share one helper now.
  Output matches carve-php everywhere.

- **A floating attribute skips an abbreviation definition** (§15 A2a). It
  attaches to the next VISIBLE block, and the other invisible kinds were already
  skipped - a definition produced a node, so the pending attributes were taken and
  then dropped. `{#i}` / `*[A]: b` / blank / `e` now publishes `<p id="i">e</p>`.

- **Past the nesting cap, an opener degrades instead of vanishing, and a blank
  line still ends the run** (PART 9 §25). An over-cap opener with a CLOSER
  anywhere after it was consumed and never emitted, so 203 openers plus three
  closers published 200 titles and no trace of the other three. The flattened run
  was one paragraph holding the whole tail, blank lines included, which swallowed
  the block after the blank. The flattened text is inline-parsed with the depth
  budget handed back, so a canonical `\:\: x` no longer keeps its backslashes.

- **An implicit `[Heading][]` reference no longer resolves into a blockquote**
  (PART 11 R1). Quoted text names the quoted document's headings, not this one's,
  in either nesting order. One index served two lookups: a `</#id>` crossref DOES
  resolve into quoted material, and the implicit path inherited its inclusion.
  Crossrefs are unchanged.

- **A collapsed reference publishes the label it resolves by** (PART 12 §3a).
  `ref` is the DERIVED label - the label the reference resolves by - with the
  authored spelling in `rawRef`; this engine published the authored spelling in
  both. PART 9R R1 offers the heading index two keys in order, and `ref` follows
  the one that answered. A label carrying no markup derives to itself, and a
  reference resolving against an authored `[label]: url` definition keeps the
  label as written. Offering the second key also resolves a reference this engine
  answered only through its slug fallback, so `[*bold* heading][]` under a
  `{#custom}` heading now links to `#custom` where it did not resolve at all.
  Rendered HTML and canonical source are unchanged.

- **A cross-reference label is a budgeted expansion.** `</#slug>` republishes the
  target heading's whole display text while the reference itself costs only the
  slug, so a short slug on a long heading amplified output by (heading length) x
  (reference count): 20 KB of input produced 16.7 MB of HTML, 40 KB produced
  66.7 MB, and the ratio kept climbing with the input. The label charges the same
  per-render expansion budget an abbreviation charges, on the HTML, Markdown,
  plain-text and ANSI targets alike. **Behavior change:** once that budget is
  spent a cross-reference renders labelled with its authored target
  (`<a href="#A">A</a>`) rather than the target's full display text, the way an
  over-budget abbreviation renders as its plain key. Ordinary documents sit
  orders of magnitude below the budget. The Carve target reproduces the authored
  `</#slug>` and never expanded, so it is unchanged.

- **A numbered cross-reference label carries the heading's markup.** With
  `headingNumbers` active, a reference to ``# A *bold* `c` h`` rendered
  `Section 1 - A bold c h`: the label was flattened to a string before any
  renderer ran, undoing the rule that a resolved cross-reference renders the
  heading's cloned NODES. The label is the heading's nodes now, on every target,
  and smart typography's source mode reaches it for the same reason. The label is
  still taken from the PRISTINE heading, so the injected `section-number` span
  never appears in it.

- **An auto heading slug no longer collides with an explicit `{#id}`.**
  `{#API-2}` on one heading plus a later `# API` emitted `id="API-2"` twice -
  invalid HTML, where every `#API-2` anchor resolves to the first match and the
  second heading is unreachable. The cross-reference index carried a third copy
  of the numbering rule with no skip at all, so `</#api-2>` resolved carrying the
  WRONG heading's title.

- **An unwrapped heading no longer puts its id before the author's attributes**
  (PART 10 §1). `{a=b .c}` on a heading inside a blockquote rendered
  `<h1 id="Auto" a="b" class="c">` where carve-js and carve-php render
  `<h1 a="b" class="c" id="Auto">`. Authored attributes keep their source order
  and a generated id joins at the end; `data-source-line` stays last.

- **The published AST keeps what the author wrote.** Rendered output does not
  move on any target:

  - An unresolved reference stays a `link` (an `image` for `![alt][nope]`)
    carrying `ref` and `rawRef`, where it reverted to a `Text` node on the HTML
    path but not the formatter path - one document with two shapes depending on
    the entry point, and `fmt` rewrote `[a][]` as `\[a\]\[\]`.
  - A nested link or autolink inside a link's label reaches the AST as the node
    the author wrote. `[[x](y)](z)` published a link to `z` whose only child was
    `x`, so `y` was gone from the tree entirely and `fmt` gave two spellings of
    one document depending on whether it went through `--json`.
  - A trailing line comment is published. `text %% note` produced a paragraph
    holding one `text` node, so a consumer reading the tree lost the comment.
  - `fromCrossref` is no longer written. The schema does not name it, and PART 12
    §11 ingest refuses any property the schema does not name, so `--json` output
    produced with heading numbers active was rejected by `--from-json` on the
    same binary.
  - A link writes its `title` before its attributes, matching the other engines.

- **`smart_typography` reaches the plain-text and terminal renderers** (PART 12,
  markup-carve/carve#560). `render_ansi_with_options` took its options as
  `_options` and `render_plain_text_with_options` read only the heading-id flag,
  so `--smart-typography source`, `to_plain_text_with_options` and
  `to_ansi_with_options` were accepted on those two targets and silently did
  nothing - output that looks configured and is not. Both targets are now
  byte-identical to carve-js and carve-php in both modes.

- **The Markdown target stops breaking cross-references and identifiers.** It
  re-derived heading ids by re-slugging, so it never knew about the `-N` suffix
  on a duplicate heading and a reference to `Setup-2` cost both halves at once -
  the heading lost its `{#id}` and the reference degraded to bare text. A
  cross-reference no longer feeds the heading slug it sits in (`# A </#a>`
  slugged as `A-A`). Escapes the author wrote are reproduced (`A \" B \-\- C`
  stays escaped), and an intraword underscore is no longer escaped -
  `company_id` came out `company\_id` where CommonMark renders it literally
  either way. `*` stays escaped everywhere.

- **The Markdown target neutralizes what it actually emits.** The writer's stated
  invariant is that `<`, `>` and `&` in author content are escaped, so Markdown
  re-rendered to HTML cannot execute. Two ways out of it:

  - The writer probed the AUTHORED destination while normalizing the one it
    emits, so it manufactured live URLs the denylist had already dismissed.
    `[t](java<U+007F>script:alert1)` came out `[t](javascript:alert1)`, the whole
    C1 range with it, and `&#106;avascript:`, `&#x6A;`, `javascript&colon;` and
    `javascript&#58;` came out verbatim and decoded to a live scheme one hop
    downstream. **Behavior change:** a destination whose scheme is denied once
    control characters are stripped is blanked (this engine's ANSI target and
    carve-php already did that), and an ampersand that OPENS a character
    reference is emitted as `&amp;` so a consumer decodes it back to the authored
    bytes rather than into a scheme. An ampersand that opens nothing, such as one
    in a query string, is untouched.
  - Five author-content slots skipped the escape entirely: math content, the
    abbreviation definition line, the footnote label resolved and unresolved, and
    an unresolved cross-reference's target. A math span holding a `script` tag
    came out live, and an `<abbr title="...">` built from an escaped expansion
    sat in the same output as the unescaped `*[AB]:` line it came from.
    **Behavior change:** those slots escape like every other. A footnote label
    escapes in both the reference and the definition so the pair still matches;
    escaping math is transparent, since a consumer decodes the entity before its
    math renderer sees it, exactly as the HTML target has always relied on; and
    an unresolved cross-reference keeps its readable `</#target>` marker with
    only the target inside it escaped, because `</#a<script>` is a complete
    opening tag once the Markdown is rendered.

- **The Markdown renderer no longer de-escapes underscores inside verbatim
  content.** `` `a\_b` `` came back as `` `a_b` ``, and the same happened in
  fenced code blocks, link destinations, image sources and escaped raw HTML.

- **A link label's closing `]` is found past an editorial comment.** `[{#a]b#}](u)`
  formed no link and no spelling worked. Applied to both the scanner and the
  precomputed bracket table, which have to agree.

- **A `%%%` comment opener with trailing text no longer leaks the comment body
  and drops the next block** (PART 9 §28). Only the leading run of `%` is
  structural, so `%%% TODO` opens and `%%% end` closes; `%%% html` is a comment
  and its body stays hidden. The closer matches on exact delimiter length, so
  `%%%%` no longer closes a `%%%` block. An opener with no closer ahead degrades
  to a line comment.

- **`carve fmt` reproduces the document it was given.** Each of these produced a
  different document on the next parse:

  - Smart typography is no longer rewritten: formatting normalized `...`, `--`
    and `"` in the author's own source. The renderer splits text into literal and
    smart runs, so a smart run is emitted exactly as typed.
  - A nested list is written with the indentation it read. Each level was
    indented twice, so output grew as O(depth^3) where the source is O(depth^2)
    and `05-lists-5` came back with four spaces where it was written with two.
  - A table's alignment is no longer changed: the parser re-indexed a header
    cell at `[1]`, where the `=` was already stripped, so `|=<\< Note |` came out
    centred. Latent until the writer started emitting that shape, at which point
    the formatter corrupted the document it formatted.
  - Tables are written in the native header form (`=` cells plus per-cell
    alignment markers) instead of a GFM delimiter row.
  - A line block is written as `::: |` with plain-space indentation, not as a
    `.line-block` div with a literal no-break space, and a leading `{#verse}`
    attribute line reaches it instead of being silently ignored.
  - A lone table span marker stays padded (`| < |`), so a formatted table cannot
    be read elsewhere as a left-alignment marker.
  - A continuation line that reads as a list marker is aligned to the marker
    width and escaped, rather than given a fixed two-space indent that only
    worked while the marker was wider than two columns.
  - A break inside an ingested heading collapses to a space instead of splitting
    the heading on re-parse.

- **Source positions cover more of the tree.** A content line ending in
  whitespace still places what is on it - the trim at the end left every inline
  on that line unplaced. A tab-indented footnote continuation carries the
  positions the space spelling carries. A block's span covers a leading no-break
  space instead of starting one column past its own first child. The paragraph an
  over-cap opener degrades to publishes its position, as do its text runs and
  soft breaks. A `figure` rebuilt over a REFERENCE image keeps the span its
  paragraph carried. And a hard break in a line block is placed even when the
  stanza's text is not, since tab expansion does not move a line ending. Only the
  AST output moves; all five rendering targets are byte-identical.

- **A nested blockquote no longer re-walks its own markers once per level.** On a
  depth-200 ladder that cost 1,556,994 quote-marker strips where carve-js spends
  20,100; it is now 183,494, and the per-marker cost no longer climbs with depth.
  The predicate, its inputs and every parse result are unchanged - the whole
  corpus renders byte-identically on all five targets.

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

## [0.1.0] - 2026-07-14

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

[Unreleased]: https://github.com/markup-carve/carve-rs/compare/0.1.3...HEAD
[0.1.3]: https://github.com/markup-carve/carve-rs/compare/0.1.2...0.1.3
[0.1.2]: https://github.com/markup-carve/carve-rs/compare/0.1.1...0.1.2
[0.1.1]: https://github.com/markup-carve/carve-rs/compare/0.1.0...0.1.1
[0.1.0]: https://github.com/markup-carve/carve-rs/releases/tag/0.1.0
