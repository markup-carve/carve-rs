# Changelog

All notable changes to carve-rs are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Entries for 0.1.5 and earlier are archived in
[CHANGELOG-0.1.md](https://github.com/markup-carve/carve-rs/blob/main/CHANGELOG-0.1.md),
which the published crate does not carry.

## [0.1.8] - 2026-10-01

### Breaking

- A render that blanks a denied destination scheme reports one `destination-denied` loss, so a checked render of `[x](javascript:alert(1))` refuses what it used to pass, and `--allow-loss` does not accept the code. The emitted value does not move: `href=""` is what it was (#2243, markup-carve/carve#2679, markup-carve/carve#2681).
- `RenderLossError` reads "render would lose N nodes" rather than "render would drop N raw nodes", which was already wrong for `ruby-flattened` (#2243).

### Fixes

- A quoted attribute value, a quoted class value and a quoted link, image or reference-definition title double a backslash only where the re-parse needs it, so `t\zu` is written back as authored instead of carrying an escape the reader discards (#2224).
- A list table keeps its grouping label and its pipe table on the Markdown, plain and ANSI targets, since the HTML carrier rewrite reaches the HTML target alone (#2225).
- HTML import reads a literal reference-shaped tail before pairing brackets, so a superscript or subscript opener stops pairing with the tail's closing bracket and the span re-reads as itself (#2225).
- `carve lint` reports `list-item-block-overindented` once at each block's opener rather than per line, keeps a continuing table or quote on one finding, and leaves an aligned opener unreported when only a later continuation is overindented (#2226, markup-carve/carve#2643).
- An overindented quote marker after a fence opened on a list item's marker line stays literal item text, and the item's indentation base returns once that fence closes, at every nested quote level (#2228, markup-carve/carve#2627, markup-carve/carve#2658).
- Emphasis inside a link label keeps its own scope where the surrounding span uses the same kind, so the label is neither flattened nor reported as a loss; same-kind nesting within one scope still reports (#2229, markup-carve/carve#2522).
- A lazy line inside a closed quoted comment keeps its indentation, so a comment's content and the canonical Carve output agree with the other two engines (#2231, markup-carve/carve#2663).
- A canonical empty footnote or definition body draws no unattached-attribute warning, and list padding before a `>` adds no fence-indentation warning (#2234, markup-carve/carve#2663).
- Djot import pairs emphasis by Djot delimiter ownership and reads orphan attributes, empty definition fences, image alt text and reference links in their own contexts, while code, destinations and fenced metadata stay opaque and a paragraph boundary stops delimiter matching (#2235, markup-carve/carve#2522).
- A Djot code fence stays opaque where ordinary prose precedes an indented fence, Djot's numeric, single-letter and Roman list markers are read as markers, and a quote inside a list keeps the item's ownership while a shallower blank line discards exited quote context (#2236, markup-carve/carve#2522).
- A reference definition publishes its positions in the original input, so a CRLF line ending or a leading BOM no longer shifts its span; footnote definition columns follow the same rule (#2239).
- A denied destination's loss message names the sink: "Blanked a denied destination scheme" for a link or an autolink, "Blanked a denied image source" for an image, with no target suffix, since the target is already its own field (#2246, markup-carve/carve#2686).
- Canonical Carve output keeps the parentheses and backslashes of a denied URL scheme, so formatting no longer changes the destination it parsed; the presentation targets keep their destination filtering and loss reports (#2248, markup-carve/carve#2685).
- Link, image and collapsed heading references resolve inside plain and quoted line blocks (#2249).
- Renderers refuse a tree past the depth ceiling in a citation definition, an extension summary, a short caption and a caption on a figure table (#2242).

### Improvements

- `render_html_owned` and `render_html_owned_with_options` render a caller-owned document without cloning its tree, and the profile and loss-report facades take ownership of the temporary documents they already held (#2242).
- A run of adjacent top-level reference definitions renders from source to HTML through the layout scanner instead of the full AST pipeline, cutting the allocation that run requested by 84%. Hosted definitions and runs the shared grammar refuses keep the existing fallback (#2240).
- Definition collectors and the layout scanner reuse the destinations, titles and attributes they already parsed, and reference lookup borrows a key already in normalized form. Parsing 4,096 definitions and references makes 135,907 allocation requests rather than 160,473 (#2241).

## [0.1.7] - 2026-09-29

### Breaking

- A `footnote_ref` spells its target as `label` on the wire, and a reference carrying no target is refused (#1849, #1853).
- A citation item spells its own mode on the wire: `Citation` grows `mode`, and `CitationGroup::integral` becomes a method (#1860).
- A display equation carries its label and its number (#1862).
- `RenderLoss.format` is optional, and a checked render result carries totals by loss code (#1859, #1868).
- `Document` implements `Drop`, so field moves, by-value destructuring and struct update syntax no longer compile: borrow the fields, or `std::mem::take` a mutable document (#1887, #1916).
- The raw-keep report names a non-handler injection sink `injection-sink`, the spelling the spec and the other two engines use, not `active-content` (#1881, #1883).
- `HtmlImportOptions.max_depth` cannot raise HTML import past the 128-level `MAX_HTML_IMPORT_DEPTH` ceiling; a lower value still narrows it (#1907).
- Markdown conversion gains fallible `try_markdown_to_ast`, `try_markdown_to_carve` and `try_migrate_markdown`; the older entry points panic where excessive nesting or the canonical writer prevents conversion (#1875, #1877, #1884).
- The CLI rejects a non-UTF-8 argument as a usage error naming the argument, exit 2, instead of panicking with exit 101 (#1834, #1835).
- The HTML importer reads an empty mark carrying attributes as an empty span holding them, reversing the choice #1734 made (#1807).
- A link tail holds no whitespace before its closing parenthesis, so `[t](a )` reads as prose (#1809).
- A generated-content `:::` kind parses as a directive rather than an admonition (#1871, markup-carve/carve#2225).
- Every empty block container renders one blank HTML body line (#1841, markup-carve/carve#2184).
- `BlockNode::Section` and `TableCell.blocks` extend the public AST, so a downstream exhaustive match and a `TableCell` struct literal need the new variant and the new field (#1976).
- Escaped spaces and preserved line-block columns are `non_breaking_space` nodes, and U+E000 is literal content in every field (#1981).
- A tree stored under the old U+E000 marker emits that character raw into HTML, with no error and no version signal, because the AST contract stays `1.0`; reparsing the source is the only remedy (#1981).
- `TableRowGroups` adds `head_attrs` and `foot_attrs`, so a downstream struct literal initializes both to `None` when unused (#1989).
- `Table.row_groups` becomes `Option<Box<TableRowGroups>>`, keeping section metadata off every block node (#1989).
- The render-loss `code` enum is closed at the two codes that name a whole dropped node, so a consumer matching `table-section-attributes-dropped` no longer sees it and `--allow-loss` takes two names rather than three (#1994).
- A discarded table section attribute reports as `field-unspellable` on the conversion-diagnostics channel instead (#1994).
- HTML import writes a link's or a span's edge whitespace outside the construct, so `<a href="/x"> x</a>` imports as a space ahead of `[x](/x)`; whitespace-only content and a no-break space stay inside (#1999, markup-carve/carve#2361).
- The canonical writer always escapes an unpaired `[` or `]` among the text brackets a construct writes between its own brackets, and a `(` directly after a bare `]`. Rendered bytes change; the tree that output rereads to does not (#1995, markup-carve/carve#2357).
- Code block content is the literal payload text, so `"a"`, `"a\n"` and `"a\n\n"` stay distinct through AST JSON, and an empty fence holds no newline whichever thing ended it (#2191, #2195).
- Canonical writing reports `field-unspellable` where it has to add a payload break an unterminated fence did not have (markup-carve/carve#2603).

### Fixes

- A node pulled in by a sliced include reports positions in its own file's coordinates (#1783).
- List items, table rows and cells, definition terms and citation items carry `pos.file` (#1786).
- A fenced code block inside a description body carries a position, so the description and the list around it reach the fence's closer (#1837).
- A reference definition reads its destination the way an inline link tail does, with the trailing attribute block read after the destination and the title (#1792).
- A destination that reaches whitespace with an unclosed parenthesis is refused instead of publishing an unbalanced `href` (#1794).
- Emphasis pairs where the source pairs it: a form feed is content in the bold-italic guard, a bare delimiter is refused against a tab as against a space for `/`, `*`, `_`, `~` and `=`, and a marker pairs inside one bracket run rather than across a link's brackets (#1810, #1811, #1814, #2161, #2169, #2173, #2179).
- A combined emphasis token's trailing attribute block is written on the outer `<strong>`, which the HTML renderer used to drop (#2174).
- A `::` line with an empty term and trailing whitespace folds into an open description body, and a comment or a definition past a term's column folds into the term (#1815, #2058).
- A fence closer counts only at the fence's authored base or its container's column, and a nested fence ends an outer one and a quote's lazy continuation (#1816, #1821, #2067, #2068, #2078).
- A fence below an item's content column keeps its payload rather than handing it to the container above, and fences inside quoted lists and footnotes no longer absorb the unmarked lines below them (#2079, #2083, #2084, #2113, #2116, #2117).
- A fence in a list item or a description body interrupts the open paragraph only when a closer is written in that body at the content column; indented fence payloads and quoted container fences keep their shape through the same reading (#2118, #2121, #2123, #2124, #2148, #2170, #2180, markup-carve/carve#2550).
- A colon closer is measured from its authored base, a description body ends at a fence run below its column, and a definition placeholder stays out of verbatim content (#2089, #2092, #2095, #2099, #2104, #2119).
- A comment stops reviving a list item with no open paragraph, and keeps its payload indentation past the host column through both parsing and formatting (#1951, #1955, #2109).
- A comment places the list content below it from its opener column, even when opened on a marker line or below the item column, pairs its delimiters in three more hosts, takes the same trailing-comment reading in a container label as everywhere else, and is bounded at the bracket run holding it (#2111, #2113, #2122, #2144).
- A comment in a marker-line nested list resumes collection at the enclosing body's content column rather than the child's, which used to flatten child blocks and leave some parent blocks too deep; descendant columns survive a line comment (#2147, #2150, #2155).
- A `%%` line drops trailing ASCII space and tab from its content and consumes one leading space or tab as the separator, in the block, inline and verse readers alike, while a `%%%` block body keeps its bytes (#2158, #2160, #2164, #2178, #2197, markup-carve/carve#2530, markup-carve/carve#2535).
- The canonical writer emits nothing after the marker for a comment that is empty once trimmed, and a no-break space or a vertical tab survives either way (#2160, #2178).
- A `+` at a column no container's marker column names is ordinary text: a column-zero marker before indented prose attaches nothing, and a marker below a nested item's content column or one column left of an in-item quote's marker stays literal (#1980, #1982, #1987, markup-carve/carve-php#2470).
- An indented block opener under a term, a nested quote or a list item folds into the text it sits in instead of opening, at every depth, and a retained marker below the content column stays text (#1847, #1930, #1940, #2041, #2101, #2120, #2127, #2128).
- A block below a nested list stays out of the item above it instead of arriving as lazy continuation, a blank in a sibling sub-list no longer loosens the outer item, lazy code payload after a below-column comment survives inside nested list items, and a marker-line block that leaves nothing open ends its item (#2130, #2141, #2146, #2152, #2183, #2185, #2192, #2193).
- A raw or code payload keeps its own shape: a zero-line and a one-blank payload stay apart in every host, an empty raw payload is written back empty, and blank lines inside a payload survive (#2153, #2157, #2159, #2163, #2166, #2167).
- ANSI output preserves every code payload line, including trailing blank lines, and a dropped-target raw block takes no line (#2168, #2172, #2177, #2186, markup-carve/carve-js#2357).
- A class key-value folds into the class slot and is written back spellably, and an empty or refused class value is dropped (#1977, #1983, #2054).
- A structural base class merges by whole author entries and is written ahead of an engine-minted attribute and behind the author's own slots, and child attributes survive inside emphasis spans (#2056, #2057, #2060).
- Unquoted attribute values reject pipes, backslashes and quotes while preserving Unicode whitespace as content, and formatting quotes a value containing a backslash (#2064, #2065).
- A container an extension declines keeps its id, its key-values and the author's own classes, since the decline is asked before the carrier rewrite rather than after it (#2066, #2125).
- A container's `[label]` publishes its inline content rather than authored text, and the HTML importer lifts a label carrying markup back again (#1823, #2038, #2053).
- A span's text reaches a heading's slug, a folded term's part-boundary soft break is placed, and a no-break space stays visible in an unresolved reference's literal source and in a heading reference key (#2074, #2080, #2184, #2188, markup-carve/carve#2604).
- A quoted fence or container title is read with no escape mechanism, so an authored backslash survives and an escaped quote cannot extend the slot (#1946, #1949).
- A caret beside a brace is escaped only where dropping the escape would change the reread tree, so `{^`, `^}` and `x{^y` are written as authored (#1950, #1955).
- The canonical writer leaves a run holding an empty code span to the escape search, scans lone brackets through the nodes carve-js scans through, and writes adjacent text nodes as one run (#2008, #2022, #2032).
- A reference label holds an opening `[` as content, a `%%` comment inside a bare emphasis run consumes the rest of the line, and a caption's number placeholder is the first `#` that does not begin a tag (#1827).
- A nested item's bare `:::` run opens its div in that item, a code span on an indented continuation line begins at its backtick run, and an unclosed inline HTML opener is bounded at the end of its block, so the later blocks survive (#1827).
- A list keeps its tightness on the Markdown target, and the tight-item separator rule reaches an ATX heading, a fenced code block and a GFM table as well as the two spellings already named (#1900, #1911, #1912, #1914).
- A tight item's sibling quotes and tables are kept apart, since a quote absorbs a following quote or table rather than being interrupted by it (#1924, #1925).
- A task item's continuation lines are padded to the item's content column rather than past its checkbox, so a heading, fence, table or nested list below the first paragraph reaches the output (#1929, #1934, #1938).
- An emphasis that follows an escaped marker keeps its `**` or `*` spelling on the Markdown target, and frontmatter is kept (#1831, #1836).
- A link whose fragment names no heading survives with the ordinary destination encoding, so `[CHAPTER III.](#chap03)` no longer collapses to its label (#2001, #2014).
- A hard break inside a table cell is written as `<br>` on the Markdown target, in an inline cell and in a block-bearing one, rather than flattened to a space; the plain, ANSI and canonical Carve targets still flatten it (#2043, #2052, #2114, markup-carve/carve#2362).
- A tight item's separator is dropped before a div opening with a list (#2133).
- The Markdown importer keeps an ordered task item's marker as the text it was written as, and reads a box for a bullet task item whose label is also defined (#1886, #1888, #1890, #1891, #1899).
- The Markdown importer reads a task box only where the cmark-gfm tasklist extension reaches it, and reports an ordered task item's lost checkbox as `structure-unspellable` in one wording at both the Markdown and the HTML entry point (#1902, #1904, #1920, #1928, #1948).
- Markdown import keeps decoded line endings inside inline text, embedded backticks in raw inline HTML, empty headings as raw HTML blocks, and URL encoding and email destinations (#2098, #2110, #2112, #2126, #2134).
- Markdown import keeps div-wrapped paragraphs after nested lists, and unwraps same-kind nesting only where it should be unwrapped (#2135, #2136, #2137).
- A code block whose language hint is unsupported is kept with the hint omitted whole, rather than shortened to a different language or written as an invalid fence (#2139, #2140, #2145, #2151, markup-carve/carve#2522).
- Djot import preserves block, span and word attributes and folds heading continuation lines (#2131, #2132, #2149).
- Multiline elements and code inside an imported table cell stay on one Carve row, with the loss reported where content must be flattened (#1910, #1931).
- An unsupported block element is replaced by its children in place, so the headings, lists and code blocks inside a custom or unknown tag survive with only the wrapper reporting `element-unwrapped` (#1990).
- A list with no `<li>` is dropped with one `element-dropped` row at `warning` covering its attributes, while stray children move ahead of where the list stood (#1993, markup-carve/carve#2367).
- A container title the quoted slot cannot spell becomes the body's first paragraph reported as `structure-unspellable`, so an import that used to fail the whole document with `SourceUnspellable` completes (#2004).
- HTML import reads a `<math>` with no TeX as its text where its tokens read in order, rather than dropping the formula, and keeps one space where an `mspace` separates two letters or digits (#1999, #2000).
- A formula whose MathML the page hides imports once, through its fallback image's `alt`; a fraction or a script still drops (markup-carve/carve#2361).
- HTML import keeps a table cell's alignment in every mode, drops a multi-line comment in a cell with its row, and lets a raw block in a cell contribute nothing (#2007, #2011, #2012, #2013, #2015, #2017, #2019).
- HTML import trims edge whitespace on cells and terms, moves nested formatting whitespace outside links and spans, and collapses plain-text layout whitespace at flattened block boundaries to one space, nested lists in table cells included (#2020, #2021, #2027, #2028, #2030, #2033, #2035).
- Imported key-values keep document order, a URL-list attribute is carried instead of refused, an attribute-less definition list merges into the one before it, and a figure and its target are written in one attribute line (#2036, #2039, #2040, #2042, #2045, #2046, #2087).
- `LinkPolicy` reads the host a browser reads, and a denied-scheme destination is imported as its content (#1870, #1872, markup-carve/carve#2255).
- A raw-kept element reports its refused attributes, and a `style` in kept bytes is read through the refusal policy, so the row carries the code, severity and owner every other refused attribute gets (#1873, #1882, #1889).
- A `style` declaration's `url(...)` arguments are read off the same comment-stripped, escape-decoded text the renderer acts on, so the report cannot describe a danger the renderer does not leave behind (#1892, #1913).
- An unspellable attribute name is reported in the ruled words (#1921, #1943).
- A directive's quoted title survives parse, wire, writer and render, is walked where an admonition's is walked, and is visited at the eleven further traversals that skipped it (#1874, #1879, #1880, #1885, #1895, #1908).
- The directive-as-admonition substitution is reported as a degradation, and an active index directive writes its title and its label before its authored body and generated list (#1909, #1919, #1941, #1985).
- A placed element writes its opener and its closer at one column, so a glossary, an index, a `nav` and a block authored inside a toc marker stop paying the carrier's first-line pad twice (#1894, #1897, #1906).
- A placed reference list indents its items and closing tag to their container's depth (#1915).
- A placement a container cannot carry out is refused, rather than splicing an endnotes section into a block quote (#1922, #1927).
- A bare placement carrier survives a profile that denies nothing it uses, and a title or a `[label]` keeps a body-less container through the profile filter, as a caption keeps a figure group (#1936, #1942, #1974).
- `carve lint` gains reference and footnote diagnostics and completes its default triggers (#1935, #1939, #2072).
- `carve lint` reports `references-placement-in-container` for a nested references marker when citations are enabled, and lines its habit checks up with the parser's diagnostic boundaries (#2077, #2138).
- The ProseMirror bridge carries a container title's inline words through its flatten, and writes a degraded composite figure's group caption as a trailing paragraph inside the `carveDiv` that `from_prosemirror` accepts (#1785, #1944, #1947).
- The ProseMirror bridge reads `tag` through its own schema map entry rather than as a position inside `mention`, and keeps the link or span wrapping an image carrying marks instead of returning a bare block image (#1986, #2175, markup-carve/carve#2586).
- The BBCode importer spells the four formatting tags the way the Carve writer would, and escapes the inline constructs a post's own text forms beside the tags it converted (#1813, #1825, #1842).
- The BBCode importer keeps an empty quote as a `>` block instead of dropping it, with no trailing space on a blank line inside a quote (#1843, #1952).
- The autolink extension decodes backslash escapes in a bare URL (#1954).
- Verse and line-block text keeps its columns: indentation inside list items, a whitespace-only verbatim line stripped at its fence opener's column, and a generated verse column left unplaced (#1992, #2059, #2061).
- Hyphen-only verse text survives the formatter, and text beside an expanded tab carries positions mapped back to the source, with a leading tab staying inside the stanza paragraph's extent (#2062, #2103, #2107).
- Footnote continuations are collected inside quote prefixes, an item keeps its text when it opens with a note reference, and a figure target's positions are rebased inside an include (#2094, #2097, #2105, #2108).
- The plain-text escaper freezes a hash after an ampersand, so numeric-reference text cannot become a Carve tag (#1841).
- Imported source is a `fmt` fixed point for four more shapes, and code-span values and table-cell spacing are preserved (#2024, #2025, #2070).
- Retained marker and semantic attribute reports carry the severity and subject every other report gets, and migration reports verify the literal text they name (#2082, #2091).
- An extension container places its own output where the element can hold it: a list-table's grouping label precedes the table rather than sitting inside it, a code group's inner lines are written at column 0, and a nested code-group or tabs wrapper opens and closes at one column (#2200, #2202, #2207, markup-carve/carve#2632).
- A fenced quote keeps its closer when it is a figure target (#2201).
- The HTML importer escapes the opening bracket of a pair that crosses a formatting boundary, and decides that escape on the completed run, so its output re-reads as the document it was given and formatting it twice is stable (#2206, #2211, #2216, markup-carve/carve#2577).
- A critic mark does not pair across a bracket-run boundary (#2218).

### Improvements

- HTML list rendering appends nested blocks directly to the output instead of copying each child buffer through its ancestors. Nested list parsing also reuses the innermost marker content across its ownership checks.

- `to_ast_envelope_json` and `from_ast_envelope_json` read and write the versioned AST interchange envelope (#1963).
- `AstEnvelopeError` separates a newer contract, an unimplemented required extension and a foreign vocabulary from a tree that would not decode; a major too large for a machine integer is a version refusal too (#1966).
- A spanning table cell publishes its resolved extent, and a rowspan crossing a row-group boundary renders in one body group (#1850, #1854, markup-carve/carve#2224).
- JSON AST interchange carries ruby base and annotation pairs through rendering, profiles and canonical source output, with the flattened form reported as `ruby-flattened` and accepted by `--allow-loss ruby-flattened` (#1859, #1864).
- Line blocks keep their optional line-end ranges through JSON AST interchange, and a small-caps span is carried through every target (#1868).
- `block_extension` takes its specified wire name, and `block_extension`, `directive` and `ruby` join the canonical type vocabulary, so `is_type_allowed` stops answering "allowed" for a name it does not know (#1856, #1867).
- Conversion diagnostics name the AST structures and fields Carve source cannot preserve, and sections and block content reach a table cell through AST JSON, rendering, extensions and the ProseMirror bridge (#1893).
- Validated node identity, annotation range and provenance sidecars keep the ids of unchanged nodes across incremental snapshots, and the ProseMirror bridge builds the four types carve-grammars named for it (#1976).
- `ast::dispose_blocks` and `ast::dispose_inlines` drop a detached block or inline tree with the same iterative teardown `Document` uses, and `Document` gains depth-checked cloning and equality (#1917, #1918, #1926).
- Ordinary `drop` on detached nodes, and the derived `Clone`, `Debug` and equality traits, remain recursive (#2102).
- A spoiler wrapper follows the pinned extension attribute-order rule with and without an authored class slot, and annotation offsets use a fixed codepoint projection independent of JSON key order, including image alt text, math, breaks and generated spaces (#1981).
- Table heads and feet keep their attributes through AST exchange and HTML import: HTML applies them to `thead`, `tbody` and `tfoot`, and the source and text targets report the ones they discard (#1989, markup-carve/carve#2339).
- The canonical writer's escape search answers a probe whose bytes it has already judged by comparing them rather than reparsing, leaving the decisions, the budget spent and the output unchanged; about 40% of the probes on an imported web page take that path (#1991, #1995).
- The escape search finishes wherever the probes are cheap and bounds the extended search, so canonical output reaches the minimal form on more documents without the search running away, and a bare empty paragraph no longer reaches the tree (#2050, #2051).
- `carve migrate --from html` converts a page dense with brackets faster, since the two newly named occurrences are escaped without a search (markup-carve/carve#2357).
- `carve migrate --from html` streams accepted HTML instead of buffering the whole output, and incremental reparsing reuses plain paragraphs that did not move (#2090, #2093).
- List-table import is available as an option, and a list table is written as a pipe table on the Markdown target (#2031).
- An inline comment keeps its fenced spelling on the AST JSON wire (#2063).
- `Document::summary()` provides fixed-size metadata for logging without traversing or exposing AST content (#2048).
- AST JSON errors expose their kind, unknown-field path, and syntax location while preserving their display messages and serde error causes (#2044).
- HTML import recognizes explicit code-language hints on code blocks and Sphinx, GitHub and MediaWiki wrappers, with validated tokens and deterministic fallback (#2023, markup-carve/carve#2387).
- Nested lists scan less and HTML import copies fewer buffers (#2219).

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

[0.1.8]: https://github.com/markup-carve/carve-rs/compare/0.1.7...0.1.8
[0.1.7]: https://github.com/markup-carve/carve-rs/compare/0.1.6...0.1.7
[0.1.6]: https://github.com/markup-carve/carve-rs/compare/0.1.5...0.1.6
