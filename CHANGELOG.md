# Changelog

All notable changes to carve-rs are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- **A numbered cross-reference label carries the heading's markup.** With the
  `headingNumbers` extension active, a reference to `# A *bold* `c` h` rendered
  `Section 1 - A bold c h`: the label was flattened to a string where it was
  derived, so the emphasis, the code span, the escape and the author's source
  run were all destroyed before any renderer was invoked, and the core rule that
  a resolved cross-reference renders the heading's cloned NODES was undone by a
  render-stage transform. The label is the heading's nodes now, on every target -
  HTML gets `Section 1 - A <strong>bold</strong> <code>c</code> h` and Markdown
  gets the same markup in its own spelling. Smart typography's source mode
  reaches the label for the same reason, since the spelling is no longer decided
  before the renderer sees it. The label is still taken from the PRISTINE
  heading, so the `section-number` span this extension injects never appears in
  it, and the label word, the number and the separator are unchanged.

- **A link or an autolink inside a link's label reaches the AST as the node the
  author wrote.** The encoder flattened both to text, so `[[x](y)](z)` published
  a link to `z` whose only child was `x` and the inner destination `y` was gone
  from the tree entirely: `fmt` on the source wrote `[[x](y)](z)` back while
  `fmt` on the same document taken through `--json` wrote `[x](z)`, two
  spellings of one document. An autolink came back as a bare URL, which is a
  different document again. "Links never nest" is a RENDERING rule, so it now
  binds the renderer: the tree keeps the node and every target unwraps it at the
  render seam. **No rendered output moves** - HTML, Markdown, plain text and
  ANSI are byte-identical, including the `mailto:` scheme a nested autolink
  drops from its visible text. What moves is what a consumer of the tree
  receives. The node carries no non-anchor flag; a consumer infers it from
  context. An unresolved reference, an image and a code span in a label are
  unchanged.

- **A reference definition carrying an unparseable attribute block stops
  defining, and the braces stay on the page.** The trailing `{...}` was peeled
  off a definition line by a balance scan before anything validated it, so a
  block the `attributes` production rejects had already been consumed and
  discarded and the line went on to define with the author's braces gone from
  the output. `[a]: /u {#}` now renders as the paragraph `[a]: /u {#}` and a
  `[a][]` beside it no longer resolves, the same reading `x {#}` in a paragraph
  already had. `{ }` and `{=}` answer alike. A VALID block still defines and
  still transfers its attributes to every link and image that resolves the
  label, a `}` inside a quoted value still does not close the block, and braces
  glued to the destination are still part of it. **Behavior change:** a document
  relying on a definition line with an unparseable attribute block resolves one
  fewer reference.

- **A no-break space survives the plain-text, Markdown and ANSI writers.** Those
  three reached for `str::trim` to drop the layout around a rendered fragment,
  and Rust's `char::is_whitespace` includes U+00A0 - so a character the author
  typed was deleted from a footnote definition's body and a table cell on all
  three targets, and from a figure caption on ANSI. This engine's HTML and
  canonical Carve output kept it throughout, so one document rendered two ways
  depending on the target asked for. PART 11 section 7 states the rule: a
  no-break space is content, not layout. The ASCII-only trim the canonical
  writer already carried now lives in the module the presentation renderers
  share, and all four use it.
- **A fence opened on a definition marker line no longer swallows a body written
  below the column.** `:  ` + a fence opener, with the body flush left, kept the
  below-column line and then the closing delimiter as code text, so the fence
  never closed and the whole block stayed inside the `<dd>`. A fenced body is not
  a paragraph, so nothing below the body's content column folds into it while one
  is open (PART 0 S4): the containers close, the `<dd>` holds an empty code
  block, and the body re-parses at document level. That is the answer the list
  and block-quote spellings of the identical shape already gave here. The same
  holds once the fence has CLOSED with nothing after it - a finished code block
  is not an open paragraph either - so `lazy` after a closed fence is a
  top-level paragraph rather than a second block in the `<dd>`. Bodies written
  AT the content column, content collected after a closed fence, and the
  first-block `:  +` form are all unchanged.

- **A document with heading numbers round-trips through this engine's own AST
  JSON.** The encoder stamped a `fromCrossref` flag on every `link` the
  heading-numbers pass derived from a `</#id>` cross-reference. The published
  schema does not name that property, and PART 12 section 11 ingest refuses any
  property the schema does not name, so `--json` output produced with heading
  numbers active was rejected by `--from-json` on the very same binary. The flag
  is a render-time fact about how the link was produced, not a fact about the
  source, so it is no longer written: `fromCrossref` no longer appears in AST
  JSON output, and the decoder no longer looks for it. The in-memory flag is
  unchanged, so HTML, Markdown, ANSI and canonical Carve output are all
  byte-identical.

- **A content line ending in whitespace still places what is on it.** Trailing
  ASCII whitespace is dropped from a content line, so the line the inline parser
  sees is the source line with characters taken off BOTH ends, and the column
  map only knew how to describe a removal at the FRONT. Every inline anchored on
  such a line therefore published no `pos`: `abc<SP>` gave a paragraph with an
  unplaced text node while the same document without the space placed it. A trim
  at the end moves nothing in front of it, so the span exists and is now
  published - across paragraphs, list items, block quotes and line blocks. A
  verse line ending in a single space is placed for the same reason; a verse
  line containing a TAB still publishes no position, because its value is not a
  slice of the source at any offset.

- **A tab-indented footnote continuation carries the positions the space
  spelling carries.** A note body whose continuation line was indented with a
  tab published no `pos` on the footnote, on its paragraph, on the soft break
  or on the second text node; the identical document written with two spaces
  published all five, and carve-js and carve-php publish all five for both. The
  cause was the column map's type, not a missing assignment: when a tab
  straddles the column a container strips to, the dedent re-inserts the
  overshoot as spaces, so the line the parser sees is two characters longer at
  the front than the source line was, and the constant mapping a column in it
  back to a column in the document is negative. The map is signed now and every
  `pos` column is built in one place. The dedent itself is unchanged, so a
  tab-indented fence and a tab-indented quote inside a note body stay literal.

- **A fenced body is not a paragraph, so a line below a list item's content
  column closes the item.** A fence opened on an item's MARKER line with its
  body below that column folded the below-column line into the code text, and
  the closer with it, so the fence never closed: `- ` + ` ``` ` / ` x` / ` ``` `
  rendered one code block holding `x` and a stray delimiter. PART 9 §24 has the
  item close there instead, holding an EMPTY code block, with the residue
  re-parsed in the surviving context - the answer the block-quote spelling of
  the same shape already gave. The guard is on the OPEN fence, so it also
  reaches a fence opened on a CONTINUATION line, and it clears at the closer: a
  below-column line after a CLOSED fence still folds into the item as before.
  A below-column list MARKER closes the item the same way, and the list it
  starts is a list of its own rather than a sub-list of the item. Two
  consequences follow: a fence's closer must be inside the same container, so a
  closer below the content column no longer makes the fence interrupt the item's
  paragraph; and at the content column a marker is still code text.
- **A reference definition is anchored at end of line.** `reference_definition`
  ends in `newline` and always has, so what follows the destination and the
  optional title makes the production FAIL and the line is an ordinary
  paragraph. This engine read the tail as junk and ignored it, so
  `[a]: /u zzz` defined a link, `[a]: /u<TAB>"T"` defined one without its title,
  and `[a]: /u<SP><SP>{.c}` defined one without its attributes. All of those are
  paragraphs now, as are the tab-first and both mixed-run spellings at the title
  slot and at the trailing-attributes slot. The line ending is `whitespace` - a
  space or a tab - so `[a]: /u<SP>`, `[a]: /u<TAB>` and `[a]: /u<SP><TAB><SP>`
  are still definitions, while a no-break space, an en quad or a form feed after
  the destination is content and makes the line a paragraph. `[a]: /u{.c}` is
  still a definition whose destination reads the braces.
- **An inline attribute block's interior is space-only.** A tab at any of the
  five inline positions - after `{`, between two attributes, before `}`, after
  an unquoted value, and in the blessed empty block `{ }` - makes the block
  unrecognized and its braces show. A no-break space no longer separates two
  attributes either. Inside a QUOTED value the character is content and does not
  move, and the block-attribute LINE keeps `whitespace` at all three of its
  slots, so `{<TAB>.a<TAB>.b<TAB>}` and a continuation line indented with a tab
  are both still one block.
- **A flush-left line after an unterminated div in a container folds into it.**
  PART 1 S4 folds such a line into the innermost OPEN paragraph. An unterminated
  `::: ` div holding a paragraph has one, so `- item` / `  ::: note` / `  body` /
  `tail` puts `tail` in the div's paragraph rather than at the top level. A div
  CLOSED by its fence has no open paragraph and still ends the item, and an
  EMPTY unterminated div has none either.
- **`fmt` writes back invisible characters instead of dropping them.** Three
  producers lost content the parser and every renderer keep, so
  `to_html(fmt(x)) == to_html(x)` failed on any document holding one: a line
  whose only character was an OGHAM SPACE MARK, EN QUAD, THIN SPACE, HAIR SPACE,
  NARROW or MEDIUM MATHEMATICAL SPACE or IDEOGRAPHIC SPACE was written back
  EMPTY and re-read as a blank line, splitting its paragraph; every C0 control
  but tab/newline/return, DEL and the whole C1 block were dropped from text
  outright; and a document whose first character is a byte order mark was
  written flush left, where a re-parse strips it as a document BOM. The
  whitespace terminal is now the two characters PART 1 names, only U+0000 is
  dropped (the parser drops it too), and a leading byte order mark is written
  one column in, where indentation carries it safely.

- **A block's span covers a leading no-break space instead of starting past
  it.** PART 12 §4 requires a parent's span to contain every child's, and a
  block whose line opened with a non-ASCII whitespace character began one column
  PAST its own first child - the indentation the span skipped was measured with
  the Unicode whitespace property, while PART 1's `indent` terminal is a space
  or a tab and nothing else (carve#890). So a no-break space, an en quad or an
  ideographic space at the head of a paragraph, a quoted line or a footnote body
  was treated as layout when it is content. Two producers carried the same
  measurement, `span_of` and `flattened_span`, and both move. Only positions
  change; all five rendering targets are byte-identical.

- **A quoted attribute value stops at the newline.** `quoted_value` excludes a
  newline in both of its alternatives, so a line break inside the quotes ends
  the production and the whole attribute block is unrecognized. On a
  block-attribute LINE this engine accepted the block and collapsed the break to
  a SPACE - a reading no production in either normative file describes - so
  `{k="a` + newline + `b"}` above a paragraph attached `k="a b"` to it instead of
  staying literal text. A block attribute may still span lines: `continuation`
  admits a newline BETWEEN two tokens, never inside one, so `{.a` + newline +
  `.b}` is still one block, as is a break after a value whose closing quote is on
  its own line. The inline form already answered correctly and does not move.

- **Trailing whitespace on a content line is dropped, on every line and not just
  a block's last.** PART 2 carries a NO TRAILING WHITESPACE clause: a whitespace
  run at the end of a content line does not reach the output and is not content.
  This engine stripped only a paragraph's final line, so a run before a SOFT
  BREAK survived - `abc<SP>` + newline + `def` and `abc` + newline + `def` are
  the same document and now render the same. The rule reaches every content
  line, so a heading, a list item, a block quote line, a definition term and
  description, a footnote body line and a table caption all drop it too, and a
  line block drops a ONE-column trailing gap (a run of two or more columns
  became NBSP content under PART 9 §23 before this rule could reach it, so
  `abc<SP><SP>` still ends in two non-breaking spaces). The dropped run is
  U+0020 and U+0009 only: a no-break space, a zero-width space, a byte order
  mark, an en quad, an ideographic space, a form feed and a vertical tab are
  content and survive. Verbatim payloads (a fenced code block's body, a raw
  block's body), whitespace interior to a construct (a code span, a literal
  inline, a table cell) and the run in front of a hard-break backslash are
  unaffected.

- **Four padding slots take exactly one space.** `link_title` (read inline AND
  at a reference definition), `image_title`, the code fence opener's slot before
  its info string, the frontmatter opener's slot before its format token, and
  the reference definition's slot before a trailing attribute block are each
  spelled as exactly ONE `space`; this engine accepted a run at all five. The
  failure mode is the one PART 7 already names: the slot does not match, the
  construct does not form, and every character survives as text. So
  `[t](/u<SP><SP>"T")` and `![a](/p.png<SP><SP>"T")` are literal text rather
  than a link and an image with titles, a two-space code fence opener falls back
  to an inline verbatim span in a paragraph, a two-space frontmatter opener is
  ordinary paragraph text the metadata lines fold into, and a two-space run at
  either reference-definition slot means no title and no attributes. The
  one-space form is unaffected at all five sites. Cardinality is per-production:
  the two metadata slots inside `code_fence_info`, the colon fence's separator
  and the definition markers' separator are spelled `space+` and still take a
  run.

- **A definition marker's separator is a run of ASCII spaces, and the next
  character is content.** PART 5 and PART 9 spell the marker-to-content
  separator `space+` at both definition markers. Two halves: the separator is a
  literal space, as it always was (a tab after the marker is still not a
  separator, so `*[HTML]:<TAB>x` and `[^f]:<TAB>x` stay paragraphs), and it is a
  RUN - all four readers already consumed one, so the grammar forbade a shape
  nothing rejected. The half that moves here is what follows the run: the first
  character that is not an ASCII space ENDS the separator and BEGINS the
  content. `*[HTML]: <NBSP>Hyper Text` now expands to a title that starts with
  the no-break space, and `*[HTML]: <TAB>Hyper Text` keeps the tab, where the
  expansion was previously `trim()`ed and both were eaten. The footnote marker
  already kept them and does not move. Widening the run is not widening the
  terminal: a tab as the separator, and a marker with only spaces after it, are
  still paragraphs.

- **An autolink body admits non-ASCII.** PART 3's `url_char` gains
  `unicode_url_char - format_char - control_char`, so an internationalized
  domain (`<https://例.jp/>`), an accented host, a non-ASCII path and a
  non-ASCII character that is not a letter (a currency sign, a CJK comma, an
  emoji, a combining mark) open an autolink instead of staying literal text.
  The deciding argument is the asymmetry with the inline form: the same
  destination written `[t](https://例.jp/)` already linked, because
  `link_destination` admits `unicode_url_char`, and one destination cannot
  answer differently on the character set depending on its spelling. A FORMAT
  character (General_Category Cf - the soft hyphen, the zero-width space, the
  byte order mark, the bidi marks) is excluded: it is invisible, so a host
  carrying one renders as the host without it and links somewhere else. So is a
  CONTROL character, which is not redundant with the ASCII enumeration - the C1
  block U+0080-U+009F is non-ASCII and non-whitespace, so without that term
  fourteen control characters would be admitted while every C0 one stayed out.
  `link_destination` is a different production and is unchanged; the ASCII
  exclusions (`"`, `\`, `` ` ``, `{`, `}`, `|`, `^`, `<`, `>`) and the ASCII-only
  `scheme` do not move.

- **A paragraph produced by the over-cap degrade publishes its position**, and
  so do the text runs and soft breaks in it. PART 9 §25 turns an opener past the
  nesting cap into literal paragraph text, and the flattened run that results is
  contiguous verbatim source - so PART 12 §4's exemption for a REASSEMBLED node
  does not reach it, and every node in it has an honest span. Two producers were
  publishing none: the colon-container degrade in `parse_capped_colon_body`,
  which discarded the line and column maps it already held, and the
  `DepthGuard::enter()` fallback in `parse_blocks`, which a deep quote or list
  ladder reaches. On the spec corpus document
  `182-openers-past-the-nesting-cap-are-one-paragraph` the paragraph now reports
  the same span as carve-js, and all eight of that document's missing positions
  are filled. Only the AST output moves; all five rendering targets are
  byte-identical.

- **A `figure` built over a REFERENCE image publishes its position.** A direct
  `![a](/p.png)` + `^ cap` becomes a figure at parse time and was placed there;
  a reference `![a][ok]` + `^ cap` cannot be, because whether the label resolves
  is unknown until the definitions are collected, so it arrives at the
  promotion pass as a paragraph and is rebuilt into a figure afterwards. That
  rebuild published no span, discarding the one the paragraph had carried all
  along. It now keeps it: the paragraph opened at the image and ran to the end
  of the caption, which is the figure's own extent. PART 12 §4's exemption for
  a REASSEMBLED node does not reach this - the lines are contiguous and the
  inline form of the same construct was already placed. The span is
  markup-inclusive and contains both children, per markup-carve/carve#913. Only
  the AST output moves; all five rendering targets are byte-identical.

- **A malformed colon fence inside an open container no longer absorbs that
  container's closer.** PART 9 §12 lets a paragraph absorb a fence-shaped line
  once a failing opener has made it prose, but the rule is about a would-be
  OPENER; a CLOSER belongs to the block that opened it, and that block was
  opened before the malformed line was read. carve-rs absorbed the closer
  anyway, so nothing closed the block afterwards and the rest of the document
  went inside it - `::: note` / `:::oops` / `:::` / `tail` put `tail` in the
  admonition, and a longer document put everything there. The closer of any
  container on the open stack is now reachable, inner ones included. Absorption
  is unchanged everywhere it applies: at top level with no container open, and
  inside a container for a fence-shaped line that is not its closer. Over
  10000 generated colon-fence documents this moves 216, every one of them onto
  the spec oracle's answer and none away from it; the spec corpus renders
  byte-identically across all six targets.

- **A table cell's padding slots take U+0020 only** (markup-carve/carve#910).
  PART 7 makes a tab syntax ONLY in a line's leading indentation run, and every
  table-cell padding slot sits after the row's opening `|`, so all five cell
  productions - `delimiter_cell`, `header_cell`, `data_cell`, `rowspan_marker`
  and `colspan_marker` - take a space. A tab in one of those slots is not a
  rejection: it stops being padding and becomes ordinary cell CONTENT, staying
  exactly where it was written, so `|<TAB>a |` now renders `<td><TAB>a</td>`
  rather than `<td>a</td>`. At `delimiter_cell` the effect is structural instead
  of textual - the cell is no longer a delimiter cell, so the line is not a
  delimiter row, no header is promoted, no alignment is assigned, and the `---`
  run becomes content that smart typography renders as an em dash. The two span
  markers follow: a tab beside `^` or `<` makes the cell ordinary content and
  the span does not happen. Cardinality is unchanged - `{space}` is a run, so
  `|=  i |` is still padded. Both ends of every production moved, and so did the
  continuation row, whose cells are `data_cell`s reached through a second code
  path. Pinned by the 21 shapes of the spec corpus category
  `256-table-cell-padding-must-be-a-space`.

- **`smart_typography` now reaches the plain-text and terminal renderers**
  (markup-carve/carve#560). PART 12 names four presentation renderers - HTML,
  Markdown, plain text and ANSI - and says source mode makes all of them emit
  each node's source run. Two of them ignored it: `render_ansi_with_options`
  took its options as `_options`, `render_plain_text_with_options` read only the
  heading-id flag, and both called the glyph form unconditionally. So
  `--smart-typography source`, `Options { smart_typography:
  SmartTypographyMode::Source, .. }`, `to_plain_text_with_options` and
  `to_ansi_with_options` were all accepted on those two targets and silently did
  nothing - output that looks configured and is not, which is the failure the
  switch exists to avoid. Both now carry the mode the way the Markdown renderer
  does, each in its own thread-local so no render can leave a mode behind in
  another. On the spec's `29-smart-typography-off` source, both targets are now
  byte-identical to carve-js and carve-php in both modes. The glyph default and
  every styling run are unchanged.

- **A nested blockquote no longer re-walks its own markers once per level**
  (#731). Parsing a quote decided, on every quoted line, whether the body left a
  paragraph open - and deciding that walked the line down to its innermost
  quoted content, a walk each enclosing level repeated over the same markers. On
  a depth-200 ladder that cost 1,556,994 quote-marker strips where carve-js
  spends 20,100; it is now 183,494, and the per-marker cost no longer climbs
  with depth. The answer is only consulted when a fenced-code opener or an
  unprefixed line arrives, so it is now computed there instead of in advance.
  The predicate, its inputs and every parse result are unchanged - the whole
  corpus renders byte-identically on all five targets. A quoted line carrying a
  `:::` run still resolves eagerly, because deciding whether an open paragraph
  absorbs it (#727) needs exactly that walk; ordinary prose holding a colon does
  not.

- **The frontmatter opener's format slot is a space** (#725). PART 7 decides the
  terminal by position, not by the slot's role: the slot sits after the `---`,
  and a tab is syntax only inside a line's leading indentation run. So

  ```
  ---	yaml
  a: 1
  ---
  x
  ```

  is no longer a typed opener - the metadata line is prose and the closing `---`
  is a thematic break, which is what the production says happens instead. The
  slot was a full Unicode trim rather than a check on any one character, so it
  admitted a tab in either direction and every Unicode space besides
  (`---<NBSP>yaml` opened frontmatter). `---yaml`, `--- yaml`, `---  yaml` and a
  bare `---` are unchanged, and whitespace after the token is still tolerated.
  The `fmt` path carried its own copy of the test and normalized the tab away
  while rewriting the block, so it is fixed with the parser.

- **The link-title slot is a space, in every form that shares it** (#726). PART 7
  decides the terminal by position: a tab is syntax only inside a line's leading
  indentation run, and an inline destination is nowhere near one. Four slots
  changed. `[t](/u<TAB>"T")` and `![t](/u<TAB>"T")` no longer take a title - the
  construct falls back to literal text, as it already did for a no-break space in
  the same slot - and `[r]: /u<TAB>"T"` is still a definition but no longer has
  one. The fourth is the reference definition's trailing attribute block:
  `[r]: /u<TAB>{.c}` no longer attaches it. Both directions of a mixed run are
  covered, because both guards were checks on a single character of a run: the
  definition's title slot was a full Unicode trim (so `[r]: /u<NBSP>"T"` took a
  title too), and the attribute block was matched on the character adjacent to
  the `{`, so a run holding a tab passed whenever its last character was a space.
  A run of spaces still pads every one of the four, so `[t](/u  "T")` is
  unchanged.

- **Every slot on a colon-fence opener line is a space** (#722). PART 7 decides
  these terminals by position: a tab is syntax only inside a line's leading
  indentation run, so from the fence onward neither a tab nor a Unicode space
  belongs. Three shapes changed. The admonition title slot admitted a tab, and
  the slot between the title and the label admitted a tab and every Unicode
  whitespace character besides - `::: note "T"` followed by U+00A0 and a
  `[label]` opened an admonition. The separator itself was checked only at its
  first character, so a lone tab was rejected while a space followed by a tab
  was not:

  ```
  ::: 	note
  x
  :::
  ```

  opened an admonition and now stays a paragraph, as does the same shape with a
  bare label. A run of spaces still separates and still pads, so `:::  note` is
  unchanged, and a bare label glued to the fence (`:::[lbl]`) still opens a div.
  The fenced-code, frontmatter and raw-block openers are untouched.

- **`fmt` keeps a lone table span marker padded.** Glued to the opening pipe,
  `<` is also the LEFT-ALIGNMENT sigil, and the two readings differ: the
  executable spec reads `|<|` as alignment on an empty cell where all three
  engines read a colspan (markup-carve/carve#710). The writer was turning the
  unambiguous `| < |` the author wrote into the ambiguous form, so a table
  formatted here and read anywhere else could change meaning. `^` takes the same
  shape; a cell attribute stays glued to the pipe, where the grammar puts it.

- **A list item's content column is live only inside the container it was
  measured in** (#593). Measuring content columns inside a block quote (#587)
  left the tracker unable to tell one container from another: `- a` / blank /
  `>   [r]: /u` registered a definition from a line inside a quote, at the
  column of a list that had already closed. Columns are scoped per container
  now - one frame per open quote level - so whether a column is live is answered
  by structure rather than by comparing indents measured in whichever coordinate
  the caller stripped to. Quote depth counts FLUSH-LEFT markers only: an
  indented `>` is a quote opening inside the current container, not one the line
  sits in, which is what keeps an item's column across a quote written at that
  item's own column.

- **`fmt` writes a nested list with the indentation it read** (#594). Each level
  was indented twice - once by an absolute `"  " * (list_depth - 1)` and again by
  the parent item's continuation prefix - with a two-space strip of the child's
  output as partial compensation. The parent's prefix IS the child's
  indentation, so the absolute term was redundant: output grew as O(depth^3)
  where the source is O(depth^2), and `05-lists-5` came back with four spaces
  where it was written with two. A nested list now round-trips byte for byte at
  every depth. Idempotence and the parse round trip held throughout, which is
  why nothing caught it.

- **An unterminated comment fence opens no span** (§28, #586). A fence with no
  closer degrades to a `%%` line comment, so the lines after it are ordinary
  lines - but the item collector opened a span anyway and dedented the next one
  by the span's strip, which lifted a BELOW-column line to the body's column 0
  and parsed it as a block. `- a` / `  %%% x` / ` # h` published an `<h1>` where
  every other engine keeps `# h` as text.

- **A post-blank line below the content column ends the list** (PART 9 §24 C3,
  #578, corpus `190-a-blank-after-a-comment-still-ends-the-item`). The rule was
  applied against the first COLLECTED block's indent and skipped entirely when
  nothing had been collected yet - which is exactly an item whose content is all
  on the marker line, so `- - a` / blank / ` b` kept `b` in the outer item where
  every other engine ends the list.

  A comment made it worse rather than causing it. Being invisible it may sit
  below the content column, and taking ITS indent as the block indent lowered
  the threshold under the content column again once the bare form was fixed. The
  fence form needs its BODY excluded too, not only its delimiters: the body is as
  invisible as they are.

  The single-level form was always right, and a block indented PAST the content
  column still belongs to the item (#301) - both are pinned.

- **A definition after an indented comment closer registers** (#574 follow-up).
  #574 taught the block parser that a `%%%` closer sits at any column and left
  the two line-based definition prepasses on the strict test, so they disagreed
  with the pass that decides: the block parser closed the comment, the prepasses
  did not, and every definition after the closer went unregistered - then came
  back as VISIBLE text, which is the one outcome a definition may never have.

  ```
  %%%
  hidden
    %%%
  [r]: /u
  [r][]
  ```

  published `<p>[r]: /u` and an unresolved `[r][]`; it now resolves, matching
  carve-js. Nothing in the corpus pairs an indented closer with a definition
  after it, which is why the suite stayed green.

- **A comment body seeds no list content column** (found while fixing the
  above). `extract_link_defs` tracked columns through a comment's body where
  `extract_footnote_defs` already treated it as opaque, so a `- hidden` inside a
  comment left a content column that outlived the fence and the indented line
  after it was stripped to that phantom column and registered - while the block
  parser reads the same line as top-level text. Registered AND visible is the
  worst of the two answers. Reachable on main through a flush-left closer; the
  indented-closer fix above only removed the accident that was hiding the rest.

- **An invisible line does not cancel a blank-line separation** (PART 9 §17 L1b,
  markup-carve/carve#630, corpus
  `185-an-invisible-line-does-not-cancel-a-blank-line-separation`). carve#621
  settled that an invisible construct is not the second PARAGRAPH that loosens
  an item, because it renders nothing. It cannot stand BETWEEN the blank and the
  paragraph after it either, because it is not a separator - so

  ```
  - a

    %% n
    text
  ```

  is loose. Reading only the FIRST collected block found the comment and called
  the item tight; the check now looks past what renders nothing for the first
  VISIBLE block. Both controls still hold: an invisible line with no paragraph
  behind it keeps the item tight (§17 L1), and a sub-list behind one keeps it
  tight too (§17 L2).

- **A comment below a list item's content column keeps the item open**
  (PART 9 §24 C3, markup-carve/carve#624, corpus
  `182-a-comment-is-recognized-at-any-column`). Below that column every other
  construct folds as the text it looks like, and a comment does not: it is the
  one construct each engine finds after trimming the line, wherever it sits.
  Being invisible it also closes nothing, so

  ```
  - a
   %% c
  b
  ```

  is one item holding `a` and `b`. The comment was ending the item here and `b`
  came back as a top-level paragraph.

  The exemption is comments only, unlike the two other `renders_nothing` checks
  in the parser, which also count an abbreviation definition. Inside an item
  there is no such node to count: a definition written there is not a definition
  at all (markup-carve/carve#611), it is the literal text the author typed, so
  it is visible and closes nothing to look past. Both engines agree byte for
  byte on that shape.

- **A blank line before a sibling item still loosens the list when an invisible
  construct sits in the gap** (§17, #557). `- a` / blank / `  %% note` / `- b`
  came out tight: the comment is not the item's second block, correctly, but
  clearing the pending blank with it hid the blank from the sibling that
  follows - and a blank before a sibling loosens the list whatever sits between
  them. Corpus `87-compact-list-blocks-6` pins it.

- **A colon fence on a marker line opens** (#511 item 4). `- :::` published
  `<li>:::</li>` unless item-owned content followed it. An opener opens, closer
  or no closer (carve#514), and an empty body is a container with nothing in it
  (carve#570) - what can stop it is the strict content-column rule, so `- :::`
  / `x` is still the literal `::: x` because `x` is lazy item text that folds
  the fence in with it.

- **A floating attribute skips an abbreviation definition** (§15 A2a, #511
  item 2). It attaches to the next VISIBLE block, and the other invisible kinds
  were already skipped - a definition produced a node, so the pending
  attributes were taken and then dropped. `{#i}` / `*[A]: b` / blank / `e` now
  publishes `<p id="i">e</p>`.

- **A trailing line comment is published** (PART 12, #513). `text %% note`
  produced a paragraph holding one `text` node, where carve-js and carve-php
  publish `text` then `comment`. Every rendered target was already right - a
  comment renders to nothing, and the canonical writer reproduced it - but the
  tree dropped what the author wrote, so a consumer reading it (carve-lsp, the
  pandoc bridge, anything formatting over the wire) lost the comment. It is an
  `InlineNode::Comment` now, encoded and decoded as `comment` with
  `block: false`.

- **Past the nesting cap, an opener degrades instead of vanishing - and a blank
  line still ends the run** (PART 9 §25, #530). An over-cap opener with a
  CLOSER anywhere after it was consumed and never emitted, so 203 openers plus
  three closers published 200 titles and no trace of the other three, while the
  same input without the closers kept them. And the flattened run was one
  paragraph holding the whole tail, blank lines included - a paragraph nothing
  else in the language can produce - which swallowed the block after the blank.
  The run now ends at the first blank like any other paragraph. And the
  flattened text is inline-parsed with the depth budget handed back: the block
  and inline passes share one counter, so at the cap the inline pass refused
  too and published the run verbatim - a canonical `\:\: x` kept its
  backslashes, and `fmt` stopped round-tripping the corpus document that
  reaches the cap.

- **A link writes its title before its attributes** (#543). `[E](/u "T"){.x}`
  published `class` before `title`, the opposite order from carve-js, carve-php
  and the executable spec. No corpus document pairs a title with an attribute
  block, so nothing compared them. Images were already right.

- **A comment on a marker line is a block, and an invisible block leaves no
  line in the item** (#511 item 7, #532). `- %% c` routed to the lead-paragraph
  path, where the inline scanner consumed the comment and left the item holding
  an EMPTY paragraph: the AST had no `comment` node where carve-js publishes
  one, the canonical writer saw an item with no content and wrote the
  CONTINUATION MARKER instead (`- +`, a construct that takes a body), and the
  empty paragraph published a whitespace-only line inside the `<li>`. A block
  that renders to nothing now contributes no line inside an item either, so
  `- a` / `  %% c` is `<li>a</li>` as it is in the other two engines.

- **A below-column line folds at every depth, not only one column in**
  (PART 9 §24 C3, carve#603). The previous fix kept such a line's own
  indentation, which two columns in REACHED the sub-list's content column
  inside the re-parsed stream and opened a list there - `-   x` / `    - a` /
  `  - b` nested `b` under `a`, as it did in all three engines. A folded line
  now carries exactly one column, which reaches no content column at all. At
  the content column a marker still opens a sublist, and at the base column it
  is still a sibling.

- **A line below every content column is text, not the block it looks like**
  (PART 0 S4, #512). Collecting an item's body dedented a line that never
  reached the content column by its own indent, landing it flush at column 0 -
  so `- - a` / ` # H` published an `<h1>` on the outer item and ` - b` became a
  second sub-item, where carve-js, carve-php and the spec fold both into the
  sub-item's paragraph (as this engine already did for the same line at the top
  level). Such a line keeps its own indentation now. A definition marker still
  attaches from any column, and plain text still dedents fully.

- **A glued colon fence holds back only a bare fence, not every opener** (#496).
  A colon fence with something glued to it (`:::note`, `:::]`) is paragraph
  text, and it used to disable colon-fence interruption for the rest of that
  paragraph - so a real opener on the next line was swallowed as text, at the
  top level, in a block quote, in a container and in a list item's lead. Only
  the BARE closer shape is held back now, which is the case the rule exists for
  and the one all three engines agree on.

- **A sub-list lead does not exempt its item from the looseness rule** (PART 9
  §11, #490). `- - a` / blank / `  b` stayed tight where `- x` / blank / `  b`
  went loose, on the same blank line; the outer item holds two blocks either
  way. Looseness still does not propagate outwards from the sub-list. The same
  shape with the body flush left (`- - a` / blank / `b`) folded across the
  blank into the inner item instead of ending the list.

- **A hard break in a line block is placed even when the stanza's text is not**
  (#480). A tab expands to placeholders and shifts every column after it within
  the line, so the anchor machinery refuses that line and its inlines come out
  unplaced - right for text, whose value stops being a slice of the source. A
  break is not content on the line, though: it is the newline ENDING it, and tab
  expansion does not move a line ending. Both breaks in a tab-bearing stanza had
  no position where all 8 elsewhere in the corpus did.

### Fixed

- **An unresolved reference is a link node, not reverted text** (PART 12 §3a,
  carve#486, carve-php#624). A reference nothing defines reverted to a `Text`
  node holding its source - but only on the HTML path; the formatter path kept
  the node. So one document had two shapes depending on which entry point
  produced it, the serialized tree published text where carve-js and carve-php
  publish a link, and `fmt` rewrote `[a][]` as `\[a\]\[\]`, escaping brackets
  the parser never interpreted. `[missing][nope]` now stays a `link` (an `image`
  for `![alt][nope]`) carrying `ref` and `rawRef`, and the HTML, Markdown,
  plain-text and ANSI writers reproduce that source the way the Carve writer
  already did. No rendered output changes for any corpus document.

  Two passes had to learn that such a node is not a link on the surface: a lone
  unresolved reference image is NOT promoted to a block image, so it keeps its
  `<p>`; and the links-never-nest pass leaves it alone, where unwrapping it to
  its label discarded the source and made `[[x][missing]](/z)` link the word
  `x`. The resolved-form half of §3a is open as carve#524 and is untouched here.

- **An abbreviation defined inside a container now expands.** Two defects met
  here. `apply_abbreviations` collects definitions from the document's children
  alone, so one written inside a div, list item or block quote was never
  collected; and `apply_abbreviations_block` had no arm for a `:::` div, a block
  extension or a definition list, so an abbreviation never expanded inside one
  even when the definition sat at the top level where collection was never in
  doubt. Both rendered as plain text where carve-js renders `<abbr>`.

### Changed

- **An abbreviation definition written inside a container is a child of the
  document** (carve-php#631, spec markup-carve/carve#518). PART 12 §7 puts a
  definition at document level even when it was authored inside a container,
  because its scope is the document wherever it sits - a footnote definition
  already worked that way here. `fmt` therefore writes it after the container,
  as it already does for a footnote definition. `pos` still records where the
  author wrote it.

- **Braces alone on a list-item marker line are a block-attribute line.**
  `- {a=b .c}` followed by an indented block attributes that block, instead of
  leaving the braces as literal item text and dropping the attributes (grammar
  PART 9 §15 A8, carve#454/#457). The discriminator is whether content follows
  the braces: `- {.c} text` is still literal, and `-{.c} text` still attributes
  the item. carve-rs was the only engine reading the brace-only form as text.

- `fmt` collapses a break inside a heading to a space instead of emitting it.
  No parse produces such a heading, but an ingested AST can (PART 12 permits any
  inline in a heading), and writing it out verbatim split the heading and moved
  text out of the title on re-parse. Only an odd run of backslashes before the
  newline is a hard break's marker, so a literal backslash ending a line
  survives.

### Fixed

- **A block that renders to nothing no longer leaves a blank line inside its
  container.** A comment, a comment block, an abbreviation definition or a
  non-HTML raw block renders as the empty string, and the container pushed the
  separating newline before it knew that, so `::: note` holding a `%%%` block
  came out as `<aside …>` then an empty line then `<p>body</p>`. The div,
  admonition, line block, block quote, definition body and the extension div
  each had their own copy of the loop; they now share one helper that drops
  empty renders. Output matches carve-php everywhere. A container whose whole
  body renders to nothing still renders exactly as a childless one does, so
  genuinely empty containers are unchanged.

- **A fence-shaped line inside an opaque span no longer closes the container
  the span sits in** (spec markup-carve/carve#450). A `:::` written inside a
  code fence or a comment block inside a `::: note` ended the note there, and
  the real closer, the rest of the body and the following blocks reparsed
  outside it. The body collector copies opaque spans through verbatim, but it
  tested the span's own opener against the closer pattern, so an opener with no
  info string - a bare ` ``` `, a bare `~~~`, a `%%%` - ended the span on its
  own line. The opener is now taken before the first closer test. This shows up
  exactly when documenting Carve in Carve.

- A tight list item's paragraph is wrapped in `<p>` when it carries authored
  attributes, which otherwise had nowhere to go and were dropped. Reachable via
  the attribute-line rule above.

### Changed

- **BREAKING: a heading ends at the newline** (spec markup-carve/carve#451,
  markup-carve/carve#434). Nothing folds into a heading any more - neither a
  plain line nor a same-count `#` line - so `# Title` with prose beneath is a
  heading plus a paragraph, and its id comes from the heading line alone
  (`Title`, not `Title-Some-text`). Documents that relied on the fold change
  meaning; anything with a blank line after the heading is unaffected.

  The fold was a silent corruption for anyone arriving from Markdown: the title
  text and the derived id were both wrong, `</#id>` cross-references and TOC
  anchors broke, and the intended body paragraph disappeared into the title with
  nothing to report. Lazy continuation now means one thing across the language -
  it continues an open paragraph - and a heading is not a paragraph.

  A flush-left line after a heading nested in a list item still belongs to that
  item; it is now the item's own content beside the heading instead of title
  text (corpus 73-list-nesting-and-looseness-4).

### Fixed

- **Braces alone on a list-item marker line are a block-attribute line**
  (spec markup-carve/carve#457, corpus 170). `- {a=b .c}` followed by an
  indented `# H` attributes the heading, exactly as those two lines do at
  document level; it used to render the braces as the item's lead paragraph.
  Braces followed by text (`- {.c} literal text`) stay literal.

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
