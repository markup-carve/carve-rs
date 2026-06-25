//! Focused cross-implementation conformance regressions.

fn html(src: &str) -> String {
    carve::to_html(src).trim().to_string()
}

#[test]
fn unresolved_collapsed_reference_resolves_to_matching_heading_slug() {
    let src = "See [name][]\n\n# Name";
    assert_eq!(
        html(src),
        concat!(
            "<p>See <a href=\"#Name\">name</a></p>\n",
            "<section id=\"Name\">\n",
            "  <h1>Name</h1>\n",
            "</section>"
        )
    );
    assert_eq!(
        html("See [NAME][]\n\n# name"),
        concat!(
            "<p>See <a href=\"#name\">NAME</a></p>\n",
            "<section id=\"name\">\n",
            "  <h1>name</h1>\n",
            "</section>"
        )
    );
    assert!(carve::to_markdown(src).contains("See [name](#Name)"));
    assert!(!carve::to_markdown(src).contains("[name][]"));
    assert!(!carve::to_plain_text(src).contains("[name][]"));
    assert!(!carve::to_ansi(src).contains("[name][]"));
}

#[test]
fn explicit_missing_reference_does_not_use_heading_fallback() {
    assert_eq!(
        html("See [name][label]\n\n# label"),
        concat!(
            "<p>See [name][label]</p>\n",
            "<section id=\"label\">\n",
            "  <h1>label</h1>\n",
            "</section>"
        )
    );
}

#[test]
fn non_html_subscript_is_not_strikethrough() {
    assert_eq!(carve::to_markdown(",sub,"), "<sub>sub</sub>\n");
    assert_eq!(carve::to_plain_text(",sub,"), "sub\n");
    assert_eq!(carve::to_ansi(",sub,"), "sub\n");
}

#[test]
fn empty_unquoted_attribute_value_rejects_whole_block() {
    assert_eq!(html("[a]{k=}"), "<p>[a]{k=}</p>");
}

#[test]
fn empty_link_destination_stays_literal() {
    assert_eq!(html("[]( )"), "<p>[]( )</p>");
}

#[test]
fn blank_separated_indented_footnote_continuation_stays_in_footnote() {
    assert_eq!(
        html("x[^1]\n\n[^1]: a\n\n  b"),
        concat!(
            "<p>x<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n",
            "<section role=\"doc-endnotes\">\n",
            "  <hr>\n",
            "  <ol>\n",
            "    <li id=\"fn1\">\n",
            "      <p>a</p>\n",
            "      <p>b<a href=\"#fnref1\" role=\"doc-backlink\">↩</a></p>\n",
            "    </li>\n",
            "  </ol>\n",
            "</section>"
        )
    );
}

#[test]
fn all_space_code_span_does_not_strip_or_panic() {
    assert_eq!(html("` `"), "<p><code> </code></p>");
    assert_eq!(html("`  a  `"), "<p><code> a </code></p>");
}

#[test]
fn adjacent_span_ids_and_classes_are_all_parsed() {
    assert_eq!(html("[a]{#i#j}"), "<p><span id=\"j\">a</span></p>");
    assert_eq!(html("[a]{.a.b}"), "<p><span class=\"a b\">a</span></p>");
}

#[test]
fn unordered_marker_tail_strips_all_leading_whitespace() {
    assert_eq!(html("-   x"), "<ul>\n  <li>x</li>\n</ul>");
}

#[test]
fn literal_nbsp_is_content_not_structural_whitespace() {
    assert_eq!(html("\u{00a0}x"), "<p>&nbsp;x</p>");
    assert_eq!(html("\u{00a0}\u{00a0}x"), "<p>&nbsp;&nbsp;x</p>");
    assert_eq!(html("a\n\n\u{00a0}b"), "<p>a</p>\n<p>&nbsp;b</p>");
    assert_eq!(
        html("> \u{00a0}x"),
        "<blockquote><p>&nbsp;x</p></blockquote>"
    );
    assert_eq!(html("- \u{00a0}x"), "<ul>\n  <li>&nbsp;x</li>\n</ul>");
    assert_eq!(html("\u{00a0}"), "<p>&nbsp;</p>");

    assert_eq!(carve::to_markdown("\u{00a0}x").as_bytes(), b"\xc2\xa0x\n");
    assert_eq!(carve::to_plain_text("\u{00a0}").as_bytes(), b"\xc2\xa0\n");
}

#[test]
fn changing_unordered_marker_starts_new_list() {
    assert_eq!(
        html("* a\n- b"),
        "<ul>\n  <li>a</li>\n</ul>\n<ul>\n  <li>b</li>\n</ul>"
    );
}

#[test]
fn marker_line_blockquote_nests_inside_list_item() {
    assert_eq!(
        html("- > q"),
        "<ul>\n  <li>\n    <blockquote><p>q</p></blockquote>\n  </li>\n</ul>"
    );
}

#[test]
fn marker_line_colon_blocks_nest_inside_list_item() {
    assert_eq!(
        html("- ::: note\n  body\n  :::"),
        concat!(
            "<ul>\n",
            "  <li>\n",
            "    <aside class=\"admonition note\">\n",
            "      <p>body</p>\n",
            "    </aside>\n",
            "  </li>\n",
            "</ul>"
        )
    );
    assert_eq!(
        html("- :::\n  body\n  :::"),
        concat!(
            "<ul>\n",
            "  <li>\n",
            "    <div>\n",
            "      <p>body</p>\n",
            "    </div>\n",
            "  </li>\n",
            "</ul>"
        )
    );
    assert_eq!(
        html("- ::: |\n  a\n  b\n  :::"),
        concat!(
            "<ul>\n",
            "  <li>\n",
            "    <div class=\"line-block\">\n",
            "      <p>a<br>\n",
            "b</p>\n",
            "    </div>\n",
            "  </li>\n",
            "</ul>"
        )
    );
}

#[test]
fn unindented_admonition_body_lazy_continues_list_item_literal_opener() {
    assert_eq!(
        html("- ::: note\nbody\n:::"),
        "<ul>\n  <li>::: note\nbody</li>\n</ul>\n<p>:::</p>"
    );
    assert_eq!(
        html("- :::\n:::"),
        "<ul>\n  <li>:::</li>\n</ul>\n<p>:::</p>"
    );
    assert_eq!(html("- a\nb"), "<ul>\n  <li>a\nb</li>\n</ul>");
}

#[test]
fn flush_left_colon_fence_shape_ends_lazy_continuation() {
    assert_eq!(html("- a\n:::"), "<ul>\n  <li>a</li>\n</ul>\n<p>:::</p>");
    assert_eq!(
        html("- a\n::: note\nno"),
        "<ul>\n  <li>a</li>\n</ul>\n<p>::: note\nno</p>"
    );
    assert_eq!(
        html("- a\n:::\nb"),
        "<ul>\n  <li>a</li>\n</ul>\n<p>:::\nb</p>"
    );
    assert_eq!(html("1. a\n:::"), "<ol>\n  <li>a</li>\n</ol>\n<p>:::</p>");
    assert_eq!(
        html("> a\n:::"),
        "<blockquote><p>a</p></blockquote>\n<p>:::</p>"
    );

    assert_eq!(
        html("- ::: note\nbody\n:::"),
        "<ul>\n  <li>::: note\nbody</li>\n</ul>\n<p>:::</p>"
    );
    assert_eq!(
        html("- ::: note\n  body\n  :::"),
        concat!(
            "<ul>\n",
            "  <li>\n",
            "    <aside class=\"admonition note\">\n",
            "      <p>body</p>\n",
            "    </aside>\n",
            "  </li>\n",
            "</ul>"
        )
    );
    assert_eq!(html("> a\nb"), "<blockquote><p>a\nb</p></blockquote>");
    assert_eq!(html("- a\nb"), "<ul>\n  <li>a\nb</li>\n</ul>");
    assert_eq!(
        html("> ::: x\n> y\n> :::"),
        "<blockquote>\n  <div class=\"x\">\n    <p>y</p>\n  </div>\n</blockquote>"
    );
}

#[test]
fn list_marker_requires_space_not_tab() {
    assert_eq!(html("-\tx"), "<p>-\tx</p>");
    assert_eq!(html("- x"), "<ul>\n  <li>x</li>\n</ul>");
    assert_eq!(html("1.\tx"), "<p>1.\tx</p>");
    assert_eq!(html("1. x"), "<ol>\n  <li>x</li>\n</ol>");
}

#[test]
fn empty_backtick_pair_after_dollar_is_code_not_math() {
    assert_eq!(html("$``"), "<p>$<code></code></p>");
    assert_eq!(html("$$``"), "<p>$$<code></code></p>");
}

#[test]
fn reference_and_footnote_definitions_require_space_after_colon() {
    assert_eq!(html("[a]:u"), "<p>[a]:u</p>");
    assert_eq!(html("[^1]\n\n[^1]:x"), "<p>[^1]</p>\n<p>[^1]:x</p>");
}

#[test]
fn reference_definitions_inside_blockquote_and_list_items_resolve() {
    assert_eq!(
        html("> [ref]: /url\n\nSee [it][ref]."),
        "<blockquote>\n\n</blockquote>\n<p>See <a href=\"/url\">it</a>.</p>"
    );
    assert_eq!(
        html("- [ref]: /url\n\nSee [it][ref]."),
        "<ul>\n  <li></li>\n</ul>\n<p>See <a href=\"/url\">it</a>.</p>"
    );
}

#[test]
fn reference_definitions_inside_fenced_code_are_literal() {
    assert_eq!(
        html("```\n[ref]: /url\n```\n\nSee [it][ref]."),
        "<pre><code>[ref]: /url\n</code></pre>\n<p>See [it][ref].</p>"
    );
}

#[test]
fn nested_list_fence_closes_so_later_definitions_collect() {
    // The fence opens on a nested-list marker and closes on an indented line
    // (`    ~~~`, no list marker). The def-collection prepass must recognize
    // that close despite the residual indentation, else `in_fence` stays set
    // and the later definition is wrongly skipped.
    assert!(html("- - ~~~\n  code\n    ~~~\n\n[r]: /u\n\n[x][r]").contains("href=\"/u\""));
}

#[test]
fn only_bullet_decimal_and_task_list_markers_collect_definitions() {
    // Bullet / decimal-ordered / task: collected (resolve).
    assert!(html("1. [r]: /u\n\n[x][r]").contains("href=\"/u\""));
    assert!(html("- [ ] [r]: /u\n\n[x][r]").contains("href=\"/u\""));
    // Alpha / roman ordered: NOT collected (matches carve-js), so unresolved.
    assert!(!html("a. [r]: /u\n\n[x][r]").contains("href=\"/u\""));
    assert!(!html("i. [r]: /u\n\n[x][r]").contains("href=\"/u\""));
}

#[test]
fn abbreviation_definition_requires_space_after_colon() {
    assert_eq!(html("*[A]:x\n\nA"), "<p>*[A]:x</p>\n<p>A</p>");
}

#[test]
fn tag_after_crossref_opener_with_space_is_a_tag() {
    assert_eq!(
        html("</#a b>"),
        "<p>&lt;/<span class=\"tag\"><strong>#a</strong></span> b&gt;</p>"
    );
}

#[test]
fn smart_typography_tokenizes_overlapping_arrows_and_dashes_left_to_right() {
    assert_eq!(html("->-->"), "<p>→–&gt;</p>");
    assert_eq!(html("--->"), "<p>—&gt;</p>");
}

#[test]
fn bare_two_pipe_empty_content_is_paragraph_not_table() {
    assert_eq!(html("||"), "<p>||</p>");
}

#[test]
fn definition_blank_between_term_and_definition_ends_list() {
    assert_eq!(
        html(":: t\n\n:  d"),
        "<dl>\n  <dt>t</dt>\n</dl>\n<p>:  d</p>"
    );
}

#[test]
fn definition_pairs_separated_by_blank_share_one_list() {
    assert_eq!(
        html(":: a\n:  b\n\n:: c\n:  d"),
        "<dl>\n  <dt>a</dt>\n  <dd>b</dd>\n  <dt>c</dt>\n  <dd>d</dd>\n</dl>"
    );
}

#[test]
fn list_nested_under_definition_is_inside_description() {
    assert_eq!(
        html(":: t\n:  - a\n   - b"),
        concat!(
            "<dl>\n",
            "  <dt>t</dt>\n",
            "  <dd>\n",
            "    <ul>\n",
            "      <li>a</li>\n",
            "      <li>b</li>\n",
            "    </ul>\n",
            "  </dd>\n",
            "</dl>"
        )
    );
}

#[test]
fn empty_term_is_not_definition_list() {
    assert_eq!(html(":: \n:  d"), "<p>:: \n:  d</p>");
}

#[test]
fn empty_definition_body_is_literal_paragraph() {
    assert_eq!(html(":: t\n:  "), "<dl>\n  <dt>t</dt>\n</dl>\n<p>:</p>");
}

#[test]
fn fenced_code_block_inside_list_item() {
    assert_eq!(
        html("- ```\n  x\n  ```"),
        "<ul>\n  <li>\n    <pre><code>x\n</code></pre>\n  </li>\n</ul>"
    );
}

#[test]
fn table_inside_list_item() {
    assert_eq!(
        html("- |a|b|\n  |-|-|"),
        concat!(
            "<ul>\n",
            "  <li>\n",
            "    <table>\n",
            "      <thead><tr><th>a</th><th>b</th></tr></thead>\n",
            "    </table>\n",
            "  </li>\n",
            "</ul>"
        )
    );
}

#[test]
fn empty_raw_format_tag_after_code_stays_literal() {
    assert_eq!(html("`a`{=}"), "<p><code>a</code>{=}</p>");
}

#[test]
fn escaped_dollar_stays_literal_before_code() {
    assert_eq!(html("\\$`a`"), "<p>$<code>a</code></p>");
}

#[test]
fn empty_list_item_line_keeps_no_trailing_space() {
    assert_eq!(
        html("- a\n- \n- b"),
        "<ul>\n  <li>a\n-</li>\n  <li>b</li>\n</ul>"
    );
}

#[test]
fn quote_after_non_breaking_space_opens() {
    // A non-breaking space is whitespace for smart-quote flanking, so a quote
    // after one opens -- both the escaped `\\ ` form and a literal U+00A0.
    assert_eq!(
        html("say\\ 'twas a fine\\ \"day\""),
        "<p>say&nbsp;\u{2018}twas a fine&nbsp;\u{201C}day\u{201D}</p>"
    );
    assert_eq!(html("a \'tis"), "<p>a&nbsp;\u{2018}tis</p>");
    // Non-HTML renderers agree on the opening quote too.
    assert_eq!(carve::to_plain_text("a\\ 'tis").trim_end(), "a \u{2018}tis");
}

// carve-rs issue 148: a colon-fence-family opener on a quoted line must end the
// blockquote's lazy continuation, so an unquoted line after it is NOT absorbed
// into the quote. This already held for the plain `:::` div; it now holds for
// the `::: |` line block and the `::: \` hard-break block too. (carve-js lags on
// the hard-break block, so the spec corpus is the reference, not carve-js.)
#[test]
fn colon_fence_openers_end_blockquote_lazy_continuation() {
    let expect = concat!(
        "<blockquote><p>{OPENER}</p></blockquote>\n",
        "<p>outside</p>\n",
        "<blockquote><p>:::</p></blockquote>"
    );
    assert_eq!(
        html("> ::: |\noutside\n> :::"),
        expect.replace("{OPENER}", "::: |")
    );
    assert_eq!(
        html("> ::: \\\noutside\n> :::"),
        expect.replace("{OPENER}", "::: \\")
    );
    // Plain div (the case that already worked -- regression guard).
    assert_eq!(
        html("> ::: note\noutside\n> :::"),
        expect.replace("{OPENER}", "::: note")
    );
    // No closer in the rest: the opener still ends the quote.
    assert_eq!(
        html("> ::: |\noutside"),
        "<blockquote><p>::: |</p></blockquote>\n<p>outside</p>"
    );
}

/// §8 smart-quote flanking battery (HTML path): a quote OPENS (left `“`/`‘`)
/// after start-of-content, whitespace/NBSP, or one of `( [ { = : - /`, an en/em
/// dash, or a nested opening curly quote; otherwise it CLOSES (right `”`/`’`).
/// The single quote additionally closes when the previous char is alphanumeric
/// or the next char is a digit. A node boundary (any prior sibling) is treated
/// as word-adjacent (closing) -- only the true start of content opens.
#[test]
fn smart_quote_flanking_html_double() {
    // Opening contexts -> left double quote, matching `"q"` closing on `q`.
    // (A leading space is trimmed from a paragraph, so the whitespace context
    // is exercised separately via `x "q"`.)
    for prefix in ["", "(", "[", "{", "=", ":", "-", "/"] {
        assert_eq!(
            html(&format!("{prefix}\"q\"")),
            format!("<p>{prefix}“q”</p>"),
            "prefix {prefix:?} should open a double quote",
        );
    }
    assert_eq!(html("x \"q\""), "<p>x “q”</p>");
    // Closing contexts -> right double quote on BOTH marks (word/punct before).
    assert_eq!(html("}\"q\""), "<p>}”q”</p>");
    assert_eq!(html(")\"q\""), "<p>)”q”</p>");
    assert_eq!(html("]\"q\""), "<p>]”q”</p>");
    assert_eq!(html(".\"q\""), "<p>.”q”</p>");
    assert_eq!(html(",\"q\""), "<p>,”q”</p>");
    assert_eq!(html("a\"b"), "<p>a”b</p>");
    // Empty `""` at true start opens then closes; after a soft break both close.
    assert_eq!(html("\"\""), "<p>“”</p>");
    assert_eq!(html("a\"b\n\"\""), "<p>a”b\n””</p>");
}

#[test]
fn smart_quote_flanking_html_single() {
    // Opening contexts (non-digit next) -> left single quote.
    for prefix in ["", "(", "[", "{", "=", ":", "-", "/"] {
        assert_eq!(
            html(&format!("{prefix}'q'")),
            format!("<p>{prefix}‘q’</p>"),
            "prefix {prefix:?} should open a single quote",
        );
    }
    assert_eq!(html("x 'q'"), "<p>x ‘q’</p>");
    // Apostrophe / closing: alnum before, or a digit next (decade elision).
    assert_eq!(html("it's"), "<p>it’s</p>");
    assert_eq!(html("the '70s"), "<p>the ’70s</p>");
    assert_eq!(html("'24'"), "<p>’24’</p>");
    assert_eq!(html("'word'"), "<p>‘word’</p>");
    // A quote at a node boundary (after emphasis) is word-adjacent -> closing.
    assert_eq!(html("*x*'s"), "<p><strong>x</strong>’s</p>");
    assert_eq!(html("*x*\"q"), "<p><strong>x</strong>”q</p>");
}

/// The same flanking rule drives the non-HTML renderers (plain text path).
#[test]
fn smart_quote_flanking_plain() {
    assert_eq!(carve::to_plain_text(":\"q\"").trim(), ":“q”");
    assert_eq!(carve::to_plain_text("}\"q\"").trim(), "}”q”");
    assert_eq!(carve::to_plain_text("the '70s").trim(), "the ’70s");
    assert_eq!(carve::to_plain_text("'word'").trim(), "‘word’");
    // Plain text joins a soft break as a space; both marks still close.
    assert_eq!(carve::to_plain_text("a\"b\n\"\"").trim(), "a”b ””");
    // Node boundary in plain text closes too.
    assert_eq!(carve::to_plain_text("*x*'s").trim(), "x’s");
}

/// Inline extension `:name[content]` (§16, carve-js regex
/// `^:([a-zA-Z_][\w-]*)\[([^\]]*)\]`): the name is an identifier (letter/`_`
/// first), the content runs to the FIRST `]` without balancing nested
/// brackets, and a trailing attribute block merges its classes into the SAME
/// `ext-NAME` class attribute (never two `class` attrs).
#[test]
fn inline_extension_name_content_and_class_merge() {
    // Digit-first name is invalid -> the whole construct stays literal.
    assert_eq!(html(":1[x]"), "<p>:1[x]</p>");
    // A name may contain digits after the first identifier char.
    assert_eq!(html(":a1[x]"), "<p><span class=\"ext-a1\">x</span></p>");
    // Content stops at the first `]`; the rest is literal text.
    assert_eq!(
        html(":foo[a [b] c]"),
        "<p><span class=\"ext-foo\">a [b</span> c]</p>"
    );
    // Authored classes merge into one `class` attribute, structural first.
    assert_eq!(
        html(":foo[a]{.cls}"),
        "<p><span class=\"ext-foo cls\">a</span></p>"
    );
    // Id / key-values from the attribute block still render (after the class).
    assert_eq!(
        html(":foo[a]{#id .cls}"),
        "<p><span class=\"ext-foo cls\" id=\"id\">a</span></p>"
    );
}

/// Reference definitions: an empty destination is not a definition (the line
/// stays literal, corpus 34-reference-link-9), and a backslash-escaped quote
/// inside the title is unescaped (corpus 34-reference-link-7).
#[test]
fn reference_definition_empty_destination_and_escaped_title() {
    // `[r]:` with only trailing whitespace is not a definition.
    assert_eq!(html("[r]:"), "<p>[r]:</p>");
    assert_eq!(html("[r]:   "), "<p>[r]:</p>");
    // A real definition with an escaped quote in the title.
    assert_eq!(
        html("[x][y]\n\n[y]: /u \"a\\\"b\\\"c\""),
        "<p><a href=\"/u\" title=\"a&quot;b&quot;c\">x</a></p>"
    );
}
