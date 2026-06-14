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
        carve::to_html("---\r\ntitle: x\r\n---\r\n\r\nBody\r\n"),
        "<p>Body</p>"
    );
    assert_eq!(carve::to_html("---\n---\n"), "");
    assert_eq!(
        carve::to_html("---\ntitle: x\n---\n\nBody\n"),
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
        carve::to_html("[x]{#i id=j}"),
        "<p><span id=\"j\">x</span></p>"
    );
    // explicit empty id is real and wins over an earlier #old.
    assert_eq!(
        carve::to_html("[x]{#old id=\"\"}"),
        "<p><span id=\"\">x</span></p>"
    );
    // a bare boolean `id` also feeds the id slot (single, last-wins) -- no dup.
    assert_eq!(carve::to_html("[x]{id}"), "<p><span id=\"\">x</span></p>");
    assert_eq!(
        carve::to_html("[x]{id id=j}"),
        "<p><span id=\"j\">x</span></p>"
    );
    assert_eq!(
        carve::to_html("[x]{id=j id}"),
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
    assert_eq!(carve::to_html("[x]{}{}"), "<p><span>x</span>{}</p>");
    assert_eq!(carve::to_html("*x* {.b}"), "<p><strong>x</strong> {.b}</p>");
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
fn inline_attribute_block_is_single_line() {
    // A newline before the closing brace means it is not an inline attr block.
    assert_eq!(carve::to_html("[x]{.a\n.b}"), "<p>[x]{.a\n.b}</p>");
    // The empty-attribute path is single-line too: `[x]{\n}` stays literal,
    // while a single-line `[x]{ }` / `[x]{}` is an (empty) span.
    assert_eq!(carve::to_html("[x]{\n}"), "<p>[x]{\n}</p>");
    assert_eq!(carve::to_html("[x]{ }"), "<p><span>x</span></p>");
    assert_eq!(carve::to_html("[x]{}"), "<p><span>x</span></p>");
}

#[test]
fn unterminated_colon_fence_stays_literal() {
    assert_eq!(
        carve::to_html(":::note\nbody no closer"),
        "<p>:::note\nbody no closer</p>"
    );
    // a closed fence still parses.
    assert!(carve::to_html(":::note\nbody\n:::").contains("<aside class=\"admonition note\">"));
}

#[test]
fn block_attribute_line_attaches_to_thematic_break() {
    assert_eq!(carve::to_html("{.x}\n---"), "<hr class=\"x\">");
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
fn lone_plus_outside_a_list_stays_quote_prose() {
    // A lone `+` is only structural as a LIST continuation marker; inside an
    // open top-level block quote it folds as ordinary prose.
    assert_eq!(
        carve::to_html("> a\n+"),
        "<blockquote><p>a\n+</p></blockquote>"
    );
}

#[test]
fn smart_quotes_track_state_across_emphasis() {
    // The closing `"` sits INSIDE an emphasis span; the running quote state must
    // carry across the span so it renders as a closing curly quote, not another
    // opener. Matches carve-php / carve-js.
    assert_eq!(carve::to_html("\"a /b\" c/ d"), "<p>“a <em>b” c</em> d</p>");
    assert_eq!(
        carve::to_html("He said \"it's /great/\" today"),
        "<p>He said “it’s <em>great</em>” today</p>"
    );
    // State resets per block: the second paragraph's `"` is a fresh OPENING
    // quote (`“b`), not a closing one carried over from the first paragraph.
    assert_eq!(carve::to_html("\"a\n\n\"b"), "<p>“a</p>\n<p>“b</p>");
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
    let html = carve::to_html("| a | b |\n| c | d |\n|{rowspan=9} e | f |\n| ^ | h |");
    assert!(html.contains("rowspan=\"2\"") && !html.contains("rowspan=\"9\""));
}
