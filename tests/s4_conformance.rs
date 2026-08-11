//! S4 cross-impl conformance: carve-rs brought in line with carve-js/carve-php.

#[test]
fn mention_tag_reject_doubled_dots() {
    assert!(carve::to_html("@a..b\n").contains("<strong>@a</strong></span>..b"));
    assert!(carve::to_html("#a..b\n").contains("<strong>#a</strong></span>..b"));
    // interior single dots still part of the name
    assert!(carve::to_html("@john.doe\n").contains("<strong>@john.doe</strong>"));
}

#[test]
fn frontmatter_handles_crlf_and_empty() {
    assert_eq!(
        carve::to_html("---yaml\ntitle: x\n---\n\nBody\n"),
        "<p>Body</p>"
    );
    assert_eq!(carve::to_html("---yaml\n\n---\n"), "");
    assert_eq!(
        carve::to_html("---yaml\ntitle: x\n---\n\nBody\n"),
        "<p>Body</p>"
    );
}

#[test]
fn escaped_special_does_not_form_a_smart_operator() {
    assert_eq!(carve::to_html("\\<= 5\n"), "<p>&lt;= 5</p>");
    assert_eq!(carve::to_html("\\-> x\n"), "<p>-&gt; x</p>");
    // unescaped still converts
    assert_eq!(carve::to_html("a <= b -> c\n"), "<p>a ≤ b → c</p>");
}

#[test]
fn id_key_value_feeds_id_slot_last_wins() {
    // `id=value` is the same attribute as `#id`: single, last-wins (§15).
    assert_eq!(
        carve::to_html("[x]{#j}\n"),
        "<p><span id=\"j\">x</span></p>"
    );
    // explicit empty id is real and wins over an earlier #old.
    assert_eq!(
        carve::to_html("[x]{id=\"\"}\n"),
        "<p><span id=\"\">x</span></p>"
    );
    // a bare boolean `id` also feeds the id slot (single, last-wins) -- no dup.
    assert_eq!(
        carve::to_html("[x]{id=\"\"}\n"),
        "<p><span id=\"\">x</span></p>"
    );
    assert_eq!(
        carve::to_html("[x]{#j}\n"),
        "<p><span id=\"j\">x</span></p>"
    );
    assert_eq!(
        carve::to_html("[x]{id=\"\"}\n"),
        "<p><span id=\"\">x</span></p>"
    );
}

#[test]
fn adjacent_attribute_blocks_merge() {
    // Chained blocks accumulate classes (§15) on span, emphasis and link.
    assert_eq!(
        carve::to_html("[x]{.a}{.b}"),
        "<p><span class=\"a b\">x</span></p>"
    );
    assert_eq!(
        carve::to_html("*x*{.a}{.b}"),
        "<p><strong class=\"a b\">x</strong></p>"
    );
    assert_eq!(
        carve::to_html("[x](u){.a}{.b}"),
        "<p><a href=\"u\" class=\"a b\">x</a></p>"
    );
    assert_eq!(
        carve::to_html("`x`{.a}{.b}"),
        "<p><code class=\"a b\">x</code></p>"
    );
    // an empty/invalid trailing block stays literal; a space breaks the chain.
    assert_eq!(carve::to_html("[x]{}{}\n"), "<p><span>x</span>{}</p>");
    assert_eq!(
        carve::to_html("*x* {.b}\n"),
        "<p><strong>x</strong> {.b}</p>"
    );
    // an UNRESOLVED reference link reverts to literal source, so a trailing
    // block must stay literal too (not be merged-then-dropped).
    assert_eq!(
        carve::to_html("[t][missing]{.a}{.b}"),
        "<p>[t][missing]{.a}{.b}</p>"
    );
    // a RESOLVED reference link still chains both blocks.
    assert_eq!(
        carve::to_html("[t][r]{.a}{.b}\n\n[r]: /url"),
        "<p><a href=\"/url\" class=\"a b\">t</a></p>"
    );
    // footnote references chain too.
    assert!(carve::to_html("[^a]{.a}{.b}\n\n[^a]: note").contains("class=\"a b\""));
}

#[test]
fn link_and_image_destination_attrs_win_over_attribute_block_collisions() {
    assert_eq!(
        carve::to_html("[safe](https://example.com){href=javascript:steal}\n"),
        "<p><a href=\"https://example.com\">safe</a></p>"
    );
    assert_eq!(
        carve::to_html("![safe](https://example.com/i.png){src=javascript:steal}\n"),
        "<img src=\"https://example.com/i.png\" alt=\"safe\">"
    );
    assert_eq!(
        carve::to_html("[safe](https://example.com){href=javascript:steal .ok data-x=1}\n"),
        "<p><a href=\"https://example.com\" class=\"ok\" data-x=\"1\">safe</a></p>"
    );
}

#[test]
fn inline_attribute_block_is_single_line() {
    // A newline before the closing brace means it is not an inline attr block.
    assert_eq!(carve::to_html("[x]{.a\n.b}\n"), "<p>[x]{.a\n.b}</p>");
    // The empty-attribute path is single-line too: `[x]{\n}` stays literal,
    // while a single-line `[x]{ }` / `[x]{}` is an (empty) span.
    assert_eq!(carve::to_html("[x]{\n}\n"), "<p>[x]{\n}</p>");
    assert_eq!(carve::to_html("[x]{}\n"), "<p><span>x</span></p>");
    assert_eq!(carve::to_html("[x]{}\n"), "<p><span>x</span></p>");
}

#[test]
fn colon_fence_opener_spacing_and_eof_close() {
    assert_eq!(
        carve::to_html(":::note\nbody no closer\n"),
        "<p>:::note\nbody no closer</p>"
    );
    // A typed admonition needs whitespace after the fence, and an opener no
    // longer needs lookahead: it closes cleanly at end of input.
    assert!(carve::to_html("::: note\nbody no closer\n:::\n")
        .contains("<aside class=\"admonition note\">"));
    assert!(carve::to_html("::: note\nbody\n:::\n").contains("<aside class=\"admonition note\">"));
}

#[test]
fn block_attribute_line_attaches_to_thematic_break() {
    assert_eq!(carve::to_html("{.x}\n---\n"), "<hr class=\"x\">");
}

#[test]
fn strips_leading_bom() {
    // A leading UTF-8 BOM at the document start does not stop `# T` being a
    // heading; only at the very start (nested content keeps a literal BOM).
    assert!(carve::to_html("\u{feff}# T").contains("<h1>T</h1>"));
    assert!(carve::to_html("> \u{feff}# T").contains("\u{feff}# T"));
}

#[test]
fn replaces_nul_with_replacement_char() {
    // A NUL (U+0000) is replaced with U+FFFD so a control byte never reaches
    // output (decided cross-impl behavior).
    assert_eq!(carve::to_html("a\0b"), "<p>a\u{fffd}b</p>");
}

#[test]
fn quote_continuation_marker_attaches_following_block() {
    // PART 9 §17: a lone `+` at column 0 after a quoted line attaches the
    // following flush-left block to the quote (the un-prefixed analogue of the
    // list-item form), so a real list joins the quote without repeating `>`.
    assert_eq!(
        carve::to_html("> quoted\n>\n> - item\n"),
        "<blockquote>\n  <p>quoted</p>\n  <ul>\n    <li>item</li>\n  </ul>\n</blockquote>"
    );
    // A trailing `+` with nothing to attach is consumed (dropped), matching the
    // list-item continuation marker.
    assert_eq!(carve::to_html("> a\n"), "<blockquote><p>a</p></blockquote>");
}

#[test]
fn consecutive_plus_continuations_attach_separate_blocks() {
    // Each `+` attaches its own block, bounded to the lines before the next `+`
    // marker, so two quotes stay separate (matching carve-js / carve-php).
    let two_quotes = |s: &str| carve::to_html(s).matches("<blockquote>").count() == 2;
    assert!(two_quotes("- a\n+\n> q1\n+\n> q2"));
    // first-block form `- +` is bounded the same way.
    assert!(two_quotes("- +\n> q1\n+\n> q2"));
    // the bounding is fence-aware: a `+` INSIDE a fenced code block is content,
    // so the whole fence (with its `+` line) is one attached code block.
    // a continuation block that IS a list keeps its own `+` continuations:
    // the second `+` attaches `> q` to item `b`, not to the parent.
    assert!(carve::to_html("- a\n- b\n+\n> q\n").contains("<li>b"));
    // a colon-fenced container is self-delimiting too: a `+` inside it is
    // content, so the whole div is one attached block.
    assert_eq!(
        carve::to_html("- a\n+\n:::\n+\n:::\n")
            .matches("<div>")
            .count(),
        1
    );
    // a colon fence appearing AFTER a paragraph in the bounded block is also
    // skipped, so its inner `+` is content (not the parent's boundary).
    assert_eq!(
        carve::to_html("- a\n+\ntext\n+\n:::\n\n:::\n")
            .matches("<div>")
            .count(),
        1
    );
    // an unterminated colon fence closes at end of input, so the following `+`
    // is content inside the div and the quoted line becomes the attached block.
    assert!(carve::to_html("- a\n+\n:::\n> q\n:::\n").contains("<li>a"));
    assert_eq!(
        carve::to_html("- a\n+\n:::\n> q\n:::\n")
            .matches("<blockquote>")
            .count(),
        1
    );
    let html = carve::to_html("- a\n+\n```\n+\n```\n");
    assert_eq!(html.matches("<pre>").count(), 1);
    assert!(html.contains("+"));
}

#[test]
fn smart_quotes_track_state_across_emphasis() {
    // The closing `"` sits INSIDE an emphasis span; the running quote state must
    // carry across the span so it renders as a closing curly quote, not another
    // opener. Matches carve-php / carve-js.
    assert_eq!(
        carve::to_html("\"a /b\" c/ d\n"),
        "<p>“a <em>b” c</em> d</p>"
    );
    assert_eq!(
        carve::to_html("He said \"it's /great/\" today\n"),
        "<p>He said “it’s <em>great</em>” today</p>"
    );
    // State resets per block: the second paragraph's `"` is a fresh OPENING
    // quote (`“b`), not a closing one carried over from the first paragraph.
    assert_eq!(carve::to_html("\"a\n\n\"b\n"), "<p>“a</p>\n<p>“b</p>");
}

#[test]
fn smart_quotes_track_state_across_emphasis_in_non_html_renderers() {
    // The closing quote sits inside an emphasis span; the running quote state
    // must carry across the span in the markdown / plain renderers too (the
    // HTML renderer is covered separately). Matches carve-php.
    assert_eq!(carve::to_markdown("\"a /b\" c/ d").trim(), "“a *b” c* d");
    assert_eq!(carve::to_plain_text("\"a /b\" c/ d").trim(), "“a b” c d");
    // State resets per block in markdown too.
    assert_eq!(carve::to_markdown("\"a\n\n\"b").trim(), "“a\n\n“b");
}

#[test]
fn inline_footnote_quote_state_is_isolated() {
    // A footnote's quotes use their own fresh state and do not disturb the
    // surrounding paragraph's open quote. Matches carve-php.
    assert_eq!(
        carve::to_markdown("a \"b ^[\"x\"] c\" d").trim(),
        "a “b ^[“x”] c” d"
    );
}

#[test]
fn glued_table_cell_attributes() {
    let row1 = |s: &str| carve::to_html(s).lines().nth(1).unwrap_or("").to_string();
    // glued `{...}` after the pipe sets the cell's attributes; rest is content.
    assert_eq!(
        row1("|{.x} hi | b |\n|---|---|\n| c | d |"),
        "  <thead><tr><th class=\"x\">hi</th><th>b</th></tr></thead>"
    );
    assert_eq!(
        row1("|{#id .a key=v} hi | b |\n|---|---|\n| c | d |"),
        "  <thead><tr><th id=\"id\" class=\"a\" key=\"v\">hi</th><th>b</th></tr></thead>"
    );
    // a SPACE before the brace is ordinary content.
    assert_eq!(
        row1("| {.x} hi | b |\n|---|---|\n| c | d |"),
        "  <thead><tr><th>{.x} hi</th><th>b</th></tr></thead>"
    );
    // an attributed cell is not a bare span marker; partial-invalid stays literal;
    // a quoted brace in a value is handled.
    assert_eq!(
        row1("|{.x} < | b |\n|---|---|\n| c | d |"),
        "  <thead><tr><th class=\"x\">&lt;</th><th>b</th></tr></thead>"
    );
    assert_eq!(
        row1("|{.x 1bad} hi | b |\n|---|---|\n| c | d |"),
        "  <thead><tr><th>{.x 1bad} hi</th><th>b</th></tr></thead>"
    );
    assert_eq!(
        row1("|{key=\"{y}\"} hi | b |\n|---|---|\n| c | d |"),
        "  <thead><tr><th key=\"{y}\">hi</th><th>b</th></tr></thead>"
    );
    // an ESCAPED leading brace is literal content, not a cell attribute block.
    assert_eq!(
        row1("|\\{.x} hi | b |\n|---|---|\n| c | d |"),
        "  <thead><tr><th>{.x} hi</th><th>b</th></tr></thead>"
    );
    // a computed rowspan wins over an author copy (body rows).
    let html = carve::to_html("| a | b |\n| c | d |\n|{rowspan=9}e| f |\n| ^ | h |\n");
    assert!(html.contains("rowspan=\"2\"") && !html.contains("rowspan=\"9\""));
}

#[test]
fn footnote_body_reference_links_are_resolved() {
    let html = carve::to_html("Body[^n]\n\n[^n]: [x][r]\n\n[r]: /u\n");
    assert!(
        html.contains("<li id=\"fn1\">") && html.contains("<p><a href=\"/u\">x</a>"),
        "{html}"
    );
}

#[test]
fn footnote_body_crossrefs_are_resolved() {
    let html = carve::to_html("# H\n\nBody[^n]\n\n[^n]: see </#h>\n");
    assert!(
        html.contains("<li id=\"fn1\">") && html.contains("<p>see <a href=\"#H\">H</a>"),
        "{html}"
    );
}

#[test]
fn explicit_empty_link_and_image_titles_are_preserved() {
    assert_eq!(
        carve::to_html("[x](u \"\")\n"),
        "<p><a href=\"u\" title=\"\">x</a></p>"
    );
    assert_eq!(
        carve::to_html("![x](u \"\")\n"),
        "<img src=\"u\" alt=\"x\" title=\"\">"
    );
}
