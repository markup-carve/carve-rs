# Changelog

All notable changes to carve-rs are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Entries for 0.1.5 and earlier are archived in
[CHANGELOG-0.1.md](https://github.com/markup-carve/carve-rs/blob/main/CHANGELOG-0.1.md),
which the published crate does not carry.

## [Unreleased]

## [0.1.11] - 2026-10-21

### Breaking

- `carve --safe` leaves includes literal. `carve flatten --safe` and `carve flatten --no-includes` refuse to run with exit 2. Pipelines that flattened untrusted input must choose whether to allow file reads (#2911, #2912).
- Public struct literals need the new `Options.align_tables`, `Options.lint_includes`, `IncludeWarning.included_by`, `IncludeResult.included`, and `LintWarning.file` and `.included_by` fields. The new code-include and lint modes use these fields (#2888, #2890, #2900).
- `IncludeDenial` gains a `NonText` variant for invalid UTF-8 includes. Exhaustive matches need another arm (#2885).

### Includes and safety

- Convert a {table} csv/tsv code include as an authored fence. (#2893)
- Keep a region marker on its line through fmt. (#2899)
- Fix the include-root help line and document it for lint. (#2906)
- Refuse a repeated include option and see through a leading BOM. (#2910)
- Report a lint include cycle closed by a code include, with no chain at top level. (#2913)
- Keep definitions inside a region and report the authored fragment. (#2914)
- Lint a CR/CRLF CSV include over the content the renderer converts. (#2915)
- Keep a @lines or @region Carve include linear in its line count. (#2917)
- Keep a code include's blank last line and end its span after the last line. (#2919)
- Find region markers in every container when keeping definitions in place. (#2922)

### Lint

- Lint a bare key one edit from a consumed key. (#2905)
- Lint an unclosed code fence and a refused continuation marker. (#2941)
- Lint the two ordered-list column traps of explicit numbers. (#2962)
- Lint: report a list marker below the content column after a blank line. (#2969)
- Lint a description separator below its definition list column. (#3056)
- Lint: report the first-block `+` of a footnote or description refused above a column-0 list as `continuation-marker-refused`. (#3057; markup-carve/carve#3482)
- Lint: do not report a block at the content column of a description inside a list item as overindented. (#3062)
- Lint: say what a refused first-block `+` leaves behind and report a refused first-block `> +`. (#3061; markup-carve/carve#3524)

### Formatting and migration

- Report dropped BBCode quote user IDs. (#2886)
- Resolve Markdown heading fragments during migration. (#2891)
- Report dropped quote attribution fields as bbcode-quote-field-dropped. (#2892)
- Use ruled BBCode quote field-loss code. (#2894)
- Report case-only fragment rewrites as link-fragment-case-rewritten fixups. (#2898)
- Keep fmt idempotent on a block comment under a term-led list item. (#2916)
- Read a BBCode quote attribution the way the spec rules it. (#2920)
- Fix: keep Markdown destination brackets and unwrap a lone image in the assessor. (#2924)
- Keep a Markdown destination as written, and a bracketed autolink in angle form. (#2931)
- Keep a Markdown autolink's brackets in its destination. (#2935)
- Keep a Markdown destination's characters as Carve reads them. (#2944)
- Assess a literal pipe in a Markdown link destination. (#2950)
- Assess a table pipe escape at the row level, as GFM unescapes it. (#2953)
- Fix: assess table pipe shapes completely and report dropped extra cells. (#2957)
- Read a backslash run before a table pipe like cmark-gfm. (#2964)
- Fix: make two Markdown imports a fmt fixed point. (#2976)
- Perf: scan destination attributes in one pass during Markdown assessment. (#2980)
- Fix(markdown): keep an item tight above a captioned quote or fence. (#2981)
- Fix(markdown): keep the blank below a nested list before an image. (#2982)
- Fix: keep HTML import's recursive frames small enough for a 2 MiB stack. (#2985)
- Fix empty list term carrier in Markdown roundtrip. (#3028)

### Parsing

- A quote-prefixed `+` needs one space after each `>`: `>+`, `>>+` and `>> +` read as text, and a `> +` attaches a following quote to the item, description, note or inner quote at its column. Lint names the right host for a quote-prefixed `+` inside a list item (markup-carve/carve#3421).
- Fold a `::` below a nested definition list's column into the open term, and end a quoted item at a `:` on its marker column. (#3060)
- Fix: honor nested fence host boundary. (#2879)
- Keep list items tight across attached captions. (#2881)
- Attach captionable hosts in list items. (#2889)
- Keep a list tight across a caption's blank line after a nested list. (#2923)
- Read container documents by their ownership clauses. (#2926)
- Keep the outer item tight when a fence run dedents out of a child. (#2927)
- Keep the blank after an unterminated fence in a nested list item. (#2929)
- Loosen the outer item when a fence run folds after a continuing child paragraph. (#2932)
- Attach the block after a + below a nested item's open fence. (#2938)
- Skip a refused + under an open fence, end an open div at a +, and keep a folded child open. (#2939)
- Read a + under an outer quote prefix as the nested quote's marker. (#2943)
- Read a continuation marker only at column 0 or right after a block opener. (#2951)
- Fix(parser): start a tab-placed sublist on a real column. (#2979)
- Fix(parser): count a stripped authored base in codepoints. (#2984)
- Fix: keep a description separator out of an item's definition list below its column. (#2995)
- Fix: read an empty link definition past an item's content column. (#2996)
- Fix: open a table after an invisible definition in a list item. (#2997)
- Fix: continue the deepest open paragraph after a nested list below a description. (#2998)
- Read the continuation marker at relative column 0. (#3003)
- Keep matched fences outside footnotes after comments. (#3004)
- Keep below-column fence closers in definition descriptions. (#3008)
- Fix ruled continuation ownership in nested containers. (#3014)
- Keep description fences within their closing container. (#3016)
- Fix nested description and list ownership by column. (#3017)
- Keep quoted definitions within their list owner. (#3018)
- Open sibling list below a hosted description column. (#3021)
- Keep literal plus at safe column after nested task fences. (#3022)
- Place marker-line definition terms at the item content column. (#3024)
- Let folded fence text loosen later list paragraphs. (#3032)
- Keep deep nested description fence with its owner. (#3035)
- Release deeper list markers to the surviving description. (#3038)
- Collect overindented quoted note bodies at their authored base. (#3039)
- Keep outer-column fences outside nested list items. (#3042)
- Bound description fence search at footnote definitions. (#3044)
- Keep column-zero lists outside first-block footnotes and descriptions. (#3053, markup-carve/carve#3482)
- Keep an empty unterminated nested fence empty before a column-0 `+`. (#3071)
- Attach a quote after `> +` to a quoted description or footnote, as in a quoted list item. (#3068, markup-carve/carve#3525)
- Attach the block at its column to a `> +` that opens a quote on its own line in a list item or description. (markup-carve/carve#3421)

### Source positions

- Fix authored spans for nested fences and quotes. (#3030)

### Performance and internals

- Check grapheme extenders without the regex engine. (#2930)
- Allow Unicode text on borrowed HTML layout route. (#2933)
- Pin the layout guard's alphanumeric check to a generated table. (#2945)
- Speed up short plain text in Rust AST parsing. (#2954)
- Speed up plain ASCII table row splitting. (#2961)
- Perf: skip occupied sentinel scan when defaults are free. (#2974)
- Speed up plain AST list marker detection. (#2994)
- Split Rust list parsing into focused modules. (#3006)
- Split remaining Rust parser regions into modules. (#3007)
- Perf(parser): shortcut short Unicode plain text. (#3012)
- Move Rust block parsing helpers into modules. (#3013)
- Extract Rust parser cursor and source positions. (#3015)
- Separate Rust parser column tracking and unit tests. (#3019)
- Split parser inline and prepass modules by responsibility. (#3047)
- Extract list finalization from the parser loop. (#3050)
- Derive list item looseness from parsed child ownership instead of rescanning source lines. (#3046)
- Share indentation-body admission across list, description, and footnote collectors. (#3049)
- Pin the spec corpus to carve `54528118` and cover four new fence and container cases. (#3048)

## [0.1.10] - 2026-10-16

### Breaking

- Empty link destinations are real links: `[text]()` and `![alt]()` parse as a link and an image with an empty URL, and a bare `[r]:` line is an empty reference definition. Escape the opening bracket to keep such text literal. AST JSON carries contract version `1.2`, which adds `resolved: true` on a link or image reference use that resolved to an empty definition, an optional root `astVersion` that `--json` writes, and `comment.form` on comments; older readers refuse trees written by this release. `Link` and `Image` gain a `resolved_empty_reference` field that struct literals have to set (#2789, #2851, markup-carve/carve#2878).

- New render loss codes. Markdown, plain text and ANSI output report `table-span-flattened` for each merged table cell; Markdown reports `table-header-row-demoted`, `table-header-row-hoisted` and `table-header-cell-demoted` when it rewrites a header row or cell. `--allow-loss` accepts all four, and a gate that fails on unknown codes has to list them (#2800, #2819, markup-carve/carve#3202).

- `IncludeWarning` carries 1-based `line` and `column` fields pointing at the directive, heading or footnote that caused it, so code that builds one with a struct literal has to set them (#2808).

- The CLI replaces ill-formed UTF-8 in a document read from a file or stdin with U+FFFD instead of refusing it, for render, lint, `fmt`, flatten, merge and migrate. `fmt --write` and `fmt --check` still refuse to rewrite such a file, and an included file holding ill-formed bytes is refused with a warning. Check the encoding yourself if a pipeline relied on the refusal (#2810, #2820, #2832, markup-carve/carve#3212).

### Fixed
- CSV and TSV fenced tables now parse and render, keep each written record on one row, and carry source positions for rows, cells and text. Empty records have no invented spans (#2872, #2875, #2876, #2877, #2878, markup-carve/carve#3239).
- Plain text output writes list continuation blocks at the item content column (#2874).

- Autolinks, cross-references and comments are atomic to the constructs around them: a `]` or `[` inside one no longer closes or opens a link, image, span or inline-note label, a cross-reference no longer pairs its `/` with an earlier one, a `~>` that ends one is no substitution arrow, and a braced closer inside one no longer closes the span opened before it (#2779, #2783, #2809, #2831, markup-carve/carve#3204, markup-carve/carve#3244).

- An emphasis, editorial or braced delimiter followed by a combining mark, a spacing mark, a variation selector or a keycap stays text, including the combined `/*...*/` token, and the writer forces the spelling where a bare one would not read back (#2810, #2812, #2816, #2817, markup-carve/carve#3209, markup-carve/carve#3217).

- Inside an unclosed code fence in a `:::` container, only the innermost container's own closer ends the fence, also after a quote or list item that holds only a comment, an attribute line, or a table row; a block at a list item's content column also lets the fence end at that closer. Comment-retained list items, descriptions and footnote bodies keep a following unclosed fence as code. A colon line of any other width is code; it used to open a nested container, so the rest of the document became code (#2797, #2827, #2839, #2848, #2846, #2854, #2853, #2865, #2870, #2871, #2873, markup-carve/carve#3201).

- A list whose items hold block quotes or containers no longer parses in exponential time: 40 levels took 44 s (#2796).

- An escaped include directive (`\{{ path }}`) stays literal in `render` and `flatten`, and `fmt` no longer adds a brace to `See \{{ a {{ path }}`. A directive that opens on the second of three braces (`{{{ path }}`) now expands, as in carve-js and carve-php (#2780).

- `FileSystemResolver` reads only regular files: a FIFO, device or directory inside the root is denied instead of opened, so an include naming a FIFO no longer blocks the caller (#2786).

- A nested link's destination is content of the label around it, so a `]` inside it closes nothing: `![a [x](/u]) b](/i)` is one image, and `[a [x](/u]) b]{.c}` a span holding the link (#2837, markup-carve/carve-js#3180).

- A `=` after an autolink or cross-reference opens a highlight: the closing `>` is part of that construct and forms no `>=`, so `carve fmt` no longer turns a highlight between two autolinks into text (#2837).

- A line holding many `[x](/u ` attempts no longer parses in quadratic time: a destination scan stops at the first space that no title follows (#2837).

- Markdown and HTML import write an unresolved cross-reference back verbatim instead of escaping its `#`, so it stays a cross-reference, while an escaped `\</#id>` stays literal. Imported `</#id>` text stays literal even when its ID resolves, including Markdown entities that decode to that spelling (#2838, #2840, #2843, #2841, markup-carve/carve-php#3492).

- A quote with one paragraph stays compact when its other children render nothing, such as comments or raw blocks for another target (#2847, #2845).

- Markdown import keeps a strikethrough containing a lone `~`, such as `~~a x~y b~~` (#2849, #2842).

- Inside a `:::` container, a list-item line after a plain paragraph stays paragraph text even when the item carries a comment or attribute line (#2850, #2844).

- Formatting leaves `:::}`, `:::~`, and `:::t` unchanged on code-span continuation lines instead of adding spaces (#2855).

- A definition or attribute line inside a quote nested in a list item lets a following line at the document column start outside the item. A quote on a container's marker line registers definitions and footnotes at its own column, without swallowing a following document-column line (#2857, #2856, #2864, #2858, #2865, #2859, #2861, #2862).

- Lazy lines inside descriptions, terms, notes and marker-line quotes stay with their open paragraph or caption. Outside an item, a below-column heading, rule, table row or quote marker in a description paragraph stays text instead of ending the body. An over-indented quote line under a term stays text, while a quote past an item's content column can open a quote (#2865, #2859, #2861, #2862, #2868, #2870, #2873, markup-carve/carve#3295).

- Carve output writes a lazy thematic break in a description paragraph without indentation, so it reads back as paragraph text rather than a rule (#2868).

- Comments in list, description, quote and footnote bodies stay with their owning container through a matched comment fence; a comment at a container's opener column can end it. A definition-shaped line in a marked footnote quote stays quoted text (#2870, #2871, #2873).

- Source positions on a lazy line inside a list item keep their original columns instead of shifting by the trimmed indentation (#2867).

- The `Wikilinks` extension no longer closes `[[` at a `]]` whose first bracket is escaped, so `a [[x\]](u)` renders the core link (#2775).

- `render_carve` refuses a list nested past the nesting cap with `RenderCarveError::SourceUnspellable` instead of searching for minutes and writing source that reads back as a different document (#2794).

- Markdown output writes a strong around a single italic as one `***` run before a non-ASCII letter (`x ***a***é y`) instead of falling back to inline HTML, and keeps a joined `***` run when an italic wraps a strong inside a word (#2817, #2828).

- ANSI output prints no ` ()` suffix after a link whose destination is empty (#2811).

- `carve merge` treats a rewrapped paragraph as unchanged, and a soft break right after inline markup counts as a space, so removing it is still a change (#2805).

- Djot import keeps definition lists intact: empty root definitions, definitions in quotes and list items, block interruptions in terms, attributes before a following table or fence, literal list markers in terms, container ownership and tight descriptions (#2782, #2785, #2788, #2790, #2801).

- Djot import keeps image alternatives and references: collapsed references across multiline labels, deletion text in alternatives, and reference URL data such as `![a![b](x)](y)`, which it used to rewrite as nested images. A blank line or trailing space dropped from an alternative is reported at its source line (#2776, #2784, #2791, #2806).

- Djot import keeps empty nested task items such as `- - [ ]`, generic DIV classes such as `-x` or `toc`, and leaves an unresolved reference containing NUL literal instead of pointing it at a sanitized heading (#2764, #2769, #2772).

- Markdown import reads an angle autolink as a Carve autolink, keeps the encoded URL of an autolink whose address holds `[` or `]`, and imports a resolved shortcut reference `[foo]` as the collapsed reference `[foo][]` with its definition, as carve-js and carve-php do (#2778, #2803, #2825, markup-carve/carve#3241).

- `carve migrate --from markdown` no longer exits 2 on a reference definition whose title wraps; the uses of such a definition become inline links (#2823).

- BBCode import reads forum quote attributions: quoted names, phpBB `post_id` and Unix `time`, and vBulletin `name;id` (#2802).

### Improvements

- Includes can select a named region between paired source markers, including a region nested inside another. The marker lines are omitted and included content keeps its original source positions. Invalid or missing regions and conflicting selectors produce a warning and leave the directive literal (#2866).

- `carve migrate --from markdown --directory SOURCE --output TARGET` converts a Markdown tree, copies other regular files, skips symlinks and writes `migration-report.json`. Local links to converted files are rewritten to `.crv` where the target resolves; unresolved ones are listed in the report and put the file under review. It refuses an existing target, two inputs that map to one output and an entry that changed after the walk, and removes its staging directory when a conversion fails (#2824, #2832, #2834, #2836).

- Markdown output keeps the angle form `<https://x>` for a URL or email autolink where the same Markdown autolink reads back (#2778, markup-carve/carve#3196).

- Djot image chains with plain, quoted or Unicode labels convert without reparsing each nested label (#2793, #2804).

- Smart quote direction no longer builds a regex, which shrinks a render-only build without default features by about 40% (#2774).

## [0.1.9] - 2026-10-14

### Breaking

- Markdown import reads a `*[TERM]: expansion` line as an abbreviation definition, always on, instead of keeping it as text (#2655, #2662).

- New loss and migration codes. Render losses `carrier-attribute-dropped`, `definition-list-flattened`, `list-style-dropped` and `list-delimiter-changed` are reported and accepted by `--allow-loss` (#2523, #2562, #2624, #2635, #2647, #2648). Markdown and Djot import report `frontmatter-synthesized`, `raw-span-whitespace-trimmed`, `raw-code-fallback`, `carrier-markers-damaged` and `djot-footnote-definition-attributes-dropped` (#2390, #2403, #2408, #2416, #2441, #2662). A gate that fails on unknown codes has to list them.

- AST JSON records `delim` only for `)` and `bulletChar` only for `*`; the default `.` and `-` markers carry no field (#2412).

- The AST contract advances to 1.1: a table records its authored separator row as `separatorWidths`, and readers refuse a contract version higher than their own (#2482).

- `raw_block` carries the attribute line written above it as `attrs`, and `raw_block.content` keeps the payload's authored line endings (#2392, #2501).

- HTML output gains structural hooks: an ordered list with a `)` delimiter renders `data-delim=")"`, and a task list renders `class="task-list"` with `data-task-state` on checked items. HTML import reads both back (#2395, #2463).

- `SmartTypographyMode` has a new `QuotesSource` variant, so an exhaustive match on it no longer compiles (#2459).

- Image alt text resolves ASCII punctuation escapes, including `\|` in tables, so a literal backslash before punctuation must be doubled. Native writers keep the decoded value when formatting (#2365, markup-carve/carve#2782).

- A quoted include path resolves ASCII punctuation escapes, as a quoted attribute value does: `"a\.crv"` names `a.crv`. A backslash before anything else stays path text (#2370, markup-carve/carve#2778).

- The include rename warning's rule id is now `include-id-rename`, renamed from `include-heading-id-rename`. Hosts publish `warning.rule` verbatim, so anything matching the old id has to follow (markup-carve/carve#2772).

### Fixed

- Djot import keeps a source NUL apart from U+FFFD: nested image labels no longer leak shield text, collapsed image labels find their definitions, a reference with a NUL no longer selects a definition holding the replacement character, and implicit heading references with a NUL resolve to the imported headings' IDs (#2721, #2723, #2727, #2753).

- Djot import derives collapsed image reference keys from the image's child text, as Djot does, and reads simple chains of nested image labels once instead of overflowing the stack (#2697, #2742).

- Djot import adds the space Carve needs after a compact or tab-separated DIV opener such as `:::x`, keeps a quote after a multiline attribute line, and keeps the checkbox on an empty task item (#2724, #2729, #2738).

- Djot import keeps a DIV that opens a definition item in the definition body instead of reading it as the term, including when the definition holds a nested list (#2759, #2761).

- Djot import keeps Unicode spacing (NBSP, NEL, em space, U+3000) in a quote as paragraph payload instead of ending the quote or moving its attributes (#2744).

- Djot import keeps literal punctuation in nested image labels (#2748).

- Djot import writes tab-separated task markers with a separator Carve accepts and keeps their DIV boundaries. Literal checkbox markers Carve would read as tasks are escaped (#2755).

- Djot import reports a raw format containing U+0085 and a multiline code span holding a container opener as losses at the source line. The output does not change (#2741, #2751).

- Markdown import keeps the author's `*` or `-` bullet and writes a `+` bullet as `-`. A bullet list directly after another with the same marker takes the other marker, so the two stay separate lists (#2712, #2728, #2756).

- Markdown import follows cmark-gfm when underscore delimiters touch tildes, keeping a rejected emphasis pair literal, and reads destinations with unbalanced parentheses or escaped newlines, including links inside image labels (#2600, #2742).

- Markdown, plain text and ANSI output keep an unused link definition at its source position wherever includes leave that position clear, and Markdown and ANSI blank a denied destination. In tight Markdown list items a paragraph after a closed code block stays adjacent, and a thematic break that would read as a heading underline is written `***` (#2739, #2743).

- A `+` continuation on a blocked table span marker is that cell's content, and a later `<` that widens the blocked cell joins it. Formatting keeps the continuation with its marker, and HTML renders no leading space for it (#2700, #2732, #2734, markup-carve/carve#3151).

- A fence closer written with tabs at a raised fence's column in a list item closes the fence instead of landing below its opener (#2700).

- The parser matches the Carve layout checker at paragraph, definition, table, quote, list and comment boundaries. Note definitions inside nested code stay literal, comments no longer change which block owns the following ones, and code span formatting survives repeated writes (#2722).

- Lint no longer reports list overindent for a fence at a marker-line note's body column after a comment (#2638).

- Markdown export drops the blank line before a code fence that opens an admonition in a tight list item, matching carve-js (#2750).

- Writing Carve source refuses a thematic break in a table cell, at any depth, instead of dropping it and writing an empty cell (#2693).

- A fence closer below its authored base is measured after the line's available prefix: a matching run closes the fence and a shorter run stays payload (#2684).

- Plain text blanks a denied destination in an unused link definition and reports one `destination-denied` loss (#2692).

- Markdown import preserves HTML code payloads, nested markup and native code where source readback succeeds. Raw HTML fallbacks report their effect on other targets and profiles (#2441).

- Keep adjacent verbatim spans separate when a payload ends in a backslash, including empty siblings and transparent wrappers. Normalize CR and CRLF before spelling blank verbatim lines in line blocks (#2444).

- Markdown fragment links to headings with hard breaks use GitHub’s text anchor (#2438).

- Markdown export preserves terminal hard breaks and hard breaks in headings with inline HTML. A break-only paragraph stays a paragraph in GFM readers (#2425).

- Markdown code spans preserve significant edge spaces. Empty spans and payloads containing tabs or newlines use an inline HTML fallback that preserves the code text (#2434).

- Document `source_len` and JSON `srcByteLength` count the original UTF-8 input before BOM, NUL and line-ending normalization. Source-length limits and authored provenance use that same input length (#2432).

- List-item code fences keep blank and indented payload after a continuation marker that attaches nothing (#2431).

- HTML code-block import keeps sibling text, omits active payloads and reports discarded child markup, comments and attributes (#2423).

- Markdown export uses a safe tilde fence for headers and labels containing backticks (#2419).

- Formatting pairs label brackets with verbatim reference source, keeping enclosing links and spans intact (#2417).

- HTML import reports discarded code-span markup, attributes and comments, and removes active subtree text. Pipe-cell line breaks are folded only when writing source (#2415).

- Formatting preserves literal backslashes in cross-reference targets. Targets with no source spelling are refused. The reader keeps form feed in a target, matching the grammar and the other engines (#2413).

- Ordered-list formatting and HTML import keep adjacent alphabetic and Roman lists separate. A hard list boundary also ends the dialect lookahead (#2399).

- A profile that denies raw blocks but allows code blocks keeps the escaped payload in a code block, including its format, attributes and line endings (markup-carve/carve#2801).

- With raw HTML off, an escaped raw block renders as a code block whose language is the raw format, and keeps its attribute line (#2387, #2392).

- A soft or hard line break counts as whitespace for smart quote direction, and a quote after a dash or before closing punctuation follows the ruled direction. Formatting keeps the punctuation that decides a quote's direction unescaped (#2398, #2448, #2460).

- Braced spans no longer close at an escaped delimiter, inside a link destination or inside a bracket label (#2397).

- List items, descriptions and notes end where the spec says: a below-column or band line that no open paragraph at the deepest frame takes closes the list, and empty nested items keep their own level (#2429, #2464, #2468, #2479, #2489, #2555, #2570, #2652, #2671).

- Code fences inside list items, definition terms and notes keep their authored column, payload and closer, including tab-authored fences, unclosed fences and fences at the content floor (#2518, #2527, #2531, #2534, #2537, #2540, #2541, #2543, #2547, #2553, #2563, #2575, #2640, #2656, #2658, #2670, #2674, #2681).

- Quoted items and colon containers keep ownership across closed colon blocks, comments, literal payloads and root-column definitions. `> +` and `[^a]: +` read as first-block forms (#2506, #2512, #2515, #2519, #2535, #2542, #2548, #2654).

- Parser waves bring block ownership, comments, fences, note openers and inline spacing level with the spec corpus at each pin (#2576, #2588, #2590, #2596, #2599, #2613, #2618, #2629, #2648).

- An empty quote takes no caption, and the writer leaves the following literal `^` unescaped. Display math on a list marker line with a caption becomes an equation figure (#2480, #2488, #2564).

- Tables: a span never covers another cell, `|< |` is an empty left-aligned cell, an explicitly delimited row of blank cells is a row, and continuation text joins the cell the span walk resolves (#2626, #2651, #2660, #2664).

- Formatting keeps quoted attributes inside inline scopes, multiline inline code, verbatim continuations, footnote paragraph breaks and emphasis braces before a trailing escaped space (#2400, #2481, #2483, #2484).

- HTML, Markdown and source output keep authored private-use characters when every internal marker slot is occupied (#2495, #2496, #2509).

- Markdown export keeps link and image titles, including attribute-only ones, escapes brackets in link labels, carries attributes on inline HTML fallbacks and keeps literal image alt text (#2382, #2383, #2424, #2494).

- Markdown export tells a deletion from a strike with a class, keeps nested emphasis intact and indents a footnote's later blocks so they stay in the footnote (#2427, #2493, #2521).

- Markdown import decides front matter by a typed opener or a mapping-shaped body, and accepts a closer with trailing spaces or tabs (#2390, #2411).

- Markdown import keeps a line-ending form feed or vertical tab, an ordered list's `)` delimiter, code newlines after an unwound frame, GFM autolinks and a percent run with one escape. A raw span ends at the tag the importer recognized (#2396, #2402, #2442, #2445, #2458, #2524).

- Markdown import reports dropped destinations, language tokens and leading whitespace from a decoded reference as `structure-unspellable`, and leaves FTP and uppercase protocol links unassessed (#2436, #2450, #2469).

- Markdown import drops a definition an imported link consumed. Roundtrip Markdown export writes resolved links as native references (#2631, #2662).

- Djot import keeps delimiter and brace boundaries, escaped closers and percent runs literal where Djot does (#2381, #2391, #2404, #2420, #2426, #2452, #2454).

- Djot import keeps attribute values, invalid and empty attribute groups, escaped attribute words and multiline comment runs as Djot reads them (#2406, #2410, #2414, #2437, #2477).

- Djot import keeps block, quote, list, footnote and table ownership, table captions, empty footnote bodies, image alt text and attributes across blank and quote boundaries. Footnote definition attributes no longer style later content (#2416, #2433, #2461, #2476, #2538, #2544, #2558, #2599, #2683).

- HTML import keeps an authored heading ID carried through export, restores abbreviation and reference definitions from carriers, and recovers contradictory footnote carriers as numbered notes (#2485, #2500, #2523).

- HTML import restores raw HTML carriers only in roundtrip mode, keeps a carrier's text payload, and roundtrip export escapes the payload. `raw-format-dropped` is reported with roundtrip on or off (#2492, #2517, #2520, #2556).

- The conversion report covers hard breaks flattened in table cells and discarded short-caption fields (#2428, #2430).

- The ProseMirror bridge reports a table continuation row as the declared `cell_wrap` loss (#2678).

- Two include options with no space between them, as in `{{ path @shift:1@lines:1-8 }}`, both apply. The second was read as part of the first one's value, which warned `include-unknown-option` and left the directive literal (#2363, #2364).

- `{{ path#section }}` and `{{ "path"#section }}` resolve: a section name needs no whitespace in front of it. A name that selects nothing warns under `include-section` (#2356, #2360, markup-carve/carve#2773).

- A directive naming two sections warns under `include-selection-conflict` and stays literal instead of dropping the second name (#2356, #2360).

- Formatting a table image keeps the escape on a pipe in its title, so the cell no longer splits and the decoded title is preserved (#2369).

- An imported table footnote keeps its identity under a collision-free label, while an unresolved reference, code, HTML and an image description keep their contents (#2371).

- An escaped pipe keeps its table cell boundary across Djot import and Markdown export. Markdown import drops the table-only escape from autolink and raw HTML payloads and recovers an escaped email autolink (#2374).

- Djot import keeps opaque content opaque: autolink labels survive footnote aliasing and delimiter conversion, a fence keeps its code and owner, a reference-definition value stays literal, and URLs keep IPv6 brackets, raw backslashes and Djot's email priority (#2376).

- Markdown wraps an editorial comment in the span the HTML target uses instead of dropping it. Plain and ANSI keep their output and report one `editorial-comment-flattened` loss per comment, which `--allow-loss` accepts (#2378, markup-carve/carve#2791).

### Improvements

- Opt-in roundtrip export (`--roundtrip`, `Options::with_roundtrip`): HTML writes the named carriers and Markdown brackets element-less containers, attribute lines, definition lists, captions and list styles in comments. The importers read them back (#2439, #2465, #2471, #2473, #2478, #2516, #2526, #2562, #2635, #2647, #2650, #2676).

- `quotes-source` smart typography keeps source quotes while dashes, ellipses and symbols stay typographic (#2459).

- Nested braced emphasis parses, formats and imports from HTML, Markdown and Djot (#2462).

- Markdown migration reports construct fidelity with `line:N` paths, so fully assessed input passes the loss gate (#2386).

- Table continuation lines are kept as `cell_wrap` markers and `cellWraps` offsets, so a continued table formats to itself (#2675, #2679).

- Djot import scans structural prefixes once per line, and short inline words skip the full inline parser setup (#2401, #2508).

- Lint reports a `^` or `<` table span marker that resolves to no span under `table-span-marker-blocked`, at the marker's own line and column. A marker under a shorter row reports that no cell is above it, and a pipe in a code span or a lone CR line break no longer hides the marker of a continued row (#2760, #2765).

- An authored `{#id}` on a footnote reference replaces the generated `fnref` ID, and the backlink follows it. HTML import matches a backlink by its target (#2700).

- AST `cell_wrap` nodes and `cellWraps` entries carry `col`, the column a part was written in, and a source position at its `+`. The Carve writer puts each part back in its column and refuses two parts naming one continuation cell (#2701, #2745).

- A fence after a blank definition term, or on a list or definition marker line, no longer parses the whole document above it to decide whether it folds into a paragraph (#2699, #2754).

- Deeply nested Djot image labels with escaped brackets stay on the bounded import path (#2746).

- Swatch rendering bounds its class checks to the extension prefix, Carve writing and heading import use hash membership for large attribute lists, and HTML import indexes backlink names and sibling positions (#2357).

- Footnote lookup uses lazy target counts, subtree intervals and membership sets. Cleanup, empty-code settlement, nested-span repair and blank-row removal run in linear passes (#2362).

- Djot import precomputes closing brackets for footnote masking and indexes delimiter ranges by family. Repeated thematic breaks, parenthesized list markers and unfinished image or quoted-link destinations no longer rescan earlier lines (#2374, #2376).

## [0.1.8] - 2026-10-06

### Compatibility and migration

With the new engine, lint existing documents before deploying their output.
`fmt --migrate` repairs unambiguous case-only reference misses. Review changes
to collapsed link text and image alt text; include selectors, glossary
references and external fragment links need manual review. See the
[migration guide](https://github.com/markup-carve/carve-rs/blob/main/docs/cli.md#exact-case-reference-migration).

### Breaking

- `try_to_carve_with_options` returns a `CarveWriteError`, which carries the profile violation it already reported and the writer's own refusal beside it. A caller that names the old error type has to widen it; `to_carve` stays infallible (#2333, markup-carve/carve-rs#2326).
- A glossary reference matches its term exactly and links to the matched entry. Glossary ids preserve case, so two terms differing only in case take two ids (#2325, #2327, markup-carve/carve#2739).
- A render that blanks a denied destination scheme reports one `destination-denied` loss, so a checked render of `[x](javascript:alert(1))` refuses what it used to pass, and `--allow-loss` does not accept the code. The emitted value does not move: `href=""` is what it was. `RenderLossError` now reads "render would lose N nodes" rather than "render would drop N raw nodes", which was already wrong for `ruby-flattened` (#2243, markup-carve/carve#2679, markup-carve/carve#2681).
- Heading cross-references, numbered caption and equation references, and collapsed
  references that fall back to heading text now compare case exactly.
  Link-definition labels, footnote labels and include fragment selectors already
  did in the previous published engine.
  Case-only mismatches no longer resolve (#2320, #2321, markup-carve/carve#2732).

- Includes rename colliding explicit ids on any element, and each later
  occurrence takes its own least free suffix. References written in the same
  inclusion follow the rename (#2311, markup-carve/carve#2729,
  markup-carve/carve#2732).

### Fixes

- The upgrade guide distinguishes newly exact lookups from labels that already
  matched case exactly, and explains migration limits. Tests keep case-distinct
  numbered captions and equations separate (#2346).

- AST patch creation interns subtree comparisons once, and patch replay updates a private tree in place. Provenance ancestry and envelope extension checks avoid repeated scans (#2345).

- A single pipe carrying row attributes stays paragraph text instead of
  panicking during the table check (#2341).
- `carve --carve` and `carve fmt` refuse instead of aborting when the writer cannot spell a tree. A list one level past the nesting cap whose deepest item holds a code fence exited 101 on valid input; both parse-path targets now print a diagnostic and exit non-zero, the way `--from-json` already did (#2333, markup-carve/carve-rs#2326).
- A flush-left line below a nested item whose lead ends in a closed code fence or raw block folds into the outer item, and one below a lead ending in a heading, a one-row table or a comment leaves the list. A folded fence line takes the lazy sentinel, so the info string on it opens no block (#2332, markup-carve/carve#2734, markup-carve/carve#2741).
- `carve lint` reports a reference image with no matching definition under `unresolved-reference-link`, the rule that already covered reference links, under no new rule id (#2336, markup-carve/carve#2740).
- `carve fmt --migrate` rewrites a reference image's case-only label the way it rewrites a link's, so `![a][Label]` migrates with `[a][Label]` (#2330, markup-carve/carve#2322).
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
- Tables keep multiple bodies, intermediate body headers, empty bodies and per-body row-header counts through parsing, canonical source export and HTML rendering, using positional source attributes. Authored conflicts are kept and partition loss is reported, an explicitly empty body stays distinct from an absent one, decimal column widths keep their precision, and a simple partition omits `rowHeadColumns` when no count is authored, matching the JavaScript and PHP ASTs (#2273, #2274).
- HTML import keeps every empty `<tbody>` as a body boundary, canonical export keeps an imported table's ID and classes when it adds column or row-group metadata, and table-attribute loss is still reported when every row is blank and dropped (#2275, #2278, #2279).
- References resolve in authored AST children, including ruby content, short captions, block-cell tables, figure images and extension fallbacks (#2253).
- Literal definitions in quoted and list-contained verse are preserved, including lazy continuations and attached opaque spans, and a colon closer inside a closed code or comment span stays inside verse, matching the executable spec (#2253).
- An indented matching comment closer ends the comment inside a colon fence, so definitions that follow stay outside the container (#2256).
- A named colon fence with invalid opener metadata stays a container with its children parsed and reports `fence-title-syntax`, instead of reaching the page as text. A bare tab title is dropped so the tab falls back to its name, Djot migration keeps the rejected opener text, and `to_carve_patch` offers a format that would drop the metadata for review rather than as an edit (#2260, markup-carve/carve#2693).
- Citations skip an invalid `@` in prefix text so a later valid key matches, honor backslash escapes before `@` and the suppress-author `-`, and require the locator comma to touch the key, so `[@a , p. 4]` stays literal as in the other engines (#2267).
- HTML import keeps separate paragraphs when an unwrapped div exposes them inside a list item. Text separated by a code block remains tight (#2302).
- Roundtrip import collapses layout whitespace exposed by removed empty inline elements and preserves an empty definition term as raw HTML (#2302).
- Safe and semantic import drop and report an empty term while keeping its definitions in document order. Canonical source export refuses an empty definition term instead of writing invalid Carve (#2302).
- An included automatic heading id carries the assembled document's number rather than the included file's, so the same heading included twice no longer collides (#2315, markup-carve/carve#2729).
- `carve lint` stops checking a fragment link that never renders, so a link inside a construct the target drops reports nothing (#2314).
- A flush-left line folds into the body its nested list holds instead of ending the item (#2318).

### Improvements

- List-table spans and imported table groups are indexed rather than rescanned (#2331).
- Borrowed HTML table cells allocate less (#2338).
- `render_html_owned` and `render_html_owned_with_options` render a caller-owned document without cloning its tree, and the profile and loss-report facades take ownership of the temporary documents they already held (#2242).
- A run of adjacent top-level reference definitions renders from source to HTML through the layout scanner instead of the full AST pipeline, cutting the allocation that run requested by 84%. Hosted definitions and runs the shared grammar refuses keep the existing fallback (#2240).
- Definition collectors and the layout scanner reuse the destinations, titles and attributes they already parsed, and reference lookup borrows a key already in normalized form. Parsing 4,096 definitions and references makes 135,907 allocation requests rather than 160,473 (#2241).
- `carve --version` and `carve -V` print `carve-rs <version>` and exit 0 (#2270, #2271).
- Parsing stops re-reading what an enclosing level already scanned: verse ownership and position geometry, quoted fence closers, nested colon bodies, list marker tails, fence lookahead and wrapped attribute lookahead are each computed once. One parse of nested closed colon bodies allocates 4.8 MB rather than 11.5 MB, and 1,024 quoted opener lines take 18,488 prefix checks rather than 1,067,064 (#2253, #2256, #2257, #2283, #2285, #2286, #2291).
- Citation parsing caches bracket matches, rejects impossible groups before scanning their contents, and builds no bracket map for a short citation, so unmatched brackets and nested rejected groups no longer rescan the same text (#2263, #2266, #2268).
- HTML rendering writes container, quote and list children into the caller's buffer, borrows table data, plans table spans once per table and skips cloning a tree that preparation cannot change. Rendering 192 nested quotes requests 352 KB rather than 29.7 MB (#2281, #2282, #2283, #2285).
- ASCII heading IDs skip Unicode transforms and an unused heading ID reserves its base name once, cutting allocation requests for 1,024 unique headings from 6,316 to 2,220. Text escaping and borrowed inline rendering stop rescanning plain prefixes, and plain Unicode paragraphs and lines with trailing spaces take the borrowed source-to-HTML path (#2272, #2277, #2280).
- A failed inline scan stops re-reading the text it already rejected: a malformed autolink ends at its first invalid byte, an invalid cross-reference target is refused before its attributes are read and its range remembered, and an eligible combined-emphasis closer is indexed by bracket run. Target syntax, delimiter fallback and source positions are unchanged (#2297).
- `carve lint` reports `broken-fragment-link` for a `[text](#id)` link, inline or through a reference definition, whose fragment matches no id in the rendered document, with a case-only near miss naming the real id. Bare `#`, `#top`, cross-file links and URLs are skipped, as are `#ref-` and `#cite-` fragments when citations are enabled, and the rule stays silent under any other extension. Render output is unchanged (#2300, markup-carve/carve-js#2497).
- A cross-reference whose id exists on an element it cannot reach names that element kind and suggests the `[text](#id)` form (#2300).
- Repeated attribute folds, class and key emission, and abbreviation lookup are indexed rather than rescanned, so 4,096 consecutive classes, inline key blocks or block attribute lines no longer cost quadratic time. First-slot order, last values, class order, abbreviation boundaries and positions are unchanged; ordinary prose throughput is not improved (#2299).
- An include selects a block by its explicit id (#2311, markup-carve/carve#2727).
- Repeated braced closer and substitution scans are bounded rather than repeated, so a run of failed braced closers no longer costs quadratic time. Output is unchanged (#2317).
- A nested list line's indentation is measured once per enclosing level instead of about eleven times, so the indent scanning a line at depth 200 costs falls by 2.71x, counted rather than timed. Output is unchanged, and the cost is still quadratic in depth (#2324, #2301).
- Repeated scans are indexed rather than repeated across AST merging, batched
  reparsing, linting and code spans: AST merging indexes identity hints and
  concurrent additions instead of rescanning sibling lists and interns
  structural fingerprints once per tree, batched reparsing assembles the
  source once and indexes edit boundaries when retaining node identities,
  lint warning positions use indexes or advancing cursors instead of counting
  source prefixes, and empty code spans choose their backtick runs in one
  suffix pass and assemble the output once (#2341).
- HTML conversion and destination filtering allocate fewer temporary URLs (#2351).

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

[0.1.10]: https://github.com/markup-carve/carve-rs/compare/0.1.9...0.1.10
[0.1.9]: https://github.com/markup-carve/carve-rs/compare/0.1.8...0.1.9
[0.1.8]: https://github.com/markup-carve/carve-rs/compare/0.1.7...0.1.8
[0.1.7]: https://github.com/markup-carve/carve-rs/compare/0.1.6...0.1.7
[0.1.6]: https://github.com/markup-carve/carve-rs/compare/0.1.5...0.1.6
