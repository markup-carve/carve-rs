# Changelog

All notable changes to carve-rs are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Breaking

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
