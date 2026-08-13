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
    // Subscript is the braced `{,x,}` only; a bare `,sub,` is literal text.
    assert_eq!(carve::to_markdown("{,sub,}"), "<sub>sub</sub>\n");
    assert_eq!(carve::to_plain_text("{,sub,}"), "sub\n");
    assert_eq!(carve::to_ansi("{,sub,}"), "sub\n");
    assert_eq!(carve::to_plain_text(",sub,"), ",sub,\n");
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

/// Two attributes need a separator between them: `attribute_list` is
/// `attribute, {space+, attribute}` (PART 7), so an adjacent pair is one
/// malformed item and the whole block stays literal (PART 9 §14).
///
/// This test asserted the opposite until the rule was ruled on. It was added in
/// #136 as a conformance regression against carve-js, which accepted the pair -
/// so it pinned an engine agreement rather than the language, and the
/// executable spec refused these shapes the whole time.
#[test]
fn adjacent_span_ids_and_classes_stay_literal() {
    assert_eq!(html("[a]{.a.b}"), "<p>[a]{.a.b}</p>");
    assert_eq!(html("[a]{.a#i}"), "<p>[a]{.a#i}</p>");
    // `{#i#j}` is deliberately NOT asserted here. It is literal under this rule
    // too, but the literal text then goes through the `#tag` inline syntax, and
    // all three engines make only the FIRST `#i` a tag where the executable
    // spec makes both. That divergence is about tag adjacency rather than
    // attribute adjacency; it was unreachable while the block parsed as
    // attributes, and asserting either answer here would pin it by accident.
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
fn marker_line_colon_fence_below_content_column_stays_literal() {
    assert_eq!(
        html("- ::: note\nbody\n:::"),
        "<ul>\n  <li>::: note\nbody</li>\n</ul>\n<div>\n</div>"
    );
    // With NOTHING below the content column to fold it into, the opener opens:
    // the flush-left `:::` is a sibling of the list, not the item's body, so the
    // marker-line fence stands on its own with an empty body (carve#514 /
    // carve#570). carve-js, carve-php and the executable spec all publish this,
    // and this engine kept the opener literal until carve-rs#511 item 4.
    assert_eq!(
        html("- :::\n:::"),
        "<ul>\n  <li>\n    <div>\n    </div>\n  </li>\n</ul>\n<div>\n</div>"
    );
    assert_eq!(html("- a\nb"), "<ul>\n  <li>a\nb</li>\n</ul>");
}

#[test]
fn flush_left_colon_fence_shape_ends_lazy_continuation() {
    assert_eq!(html("- a\n:::"), "<ul>\n  <li>a</li>\n</ul>\n<div>\n</div>");
    assert_eq!(
        html("- a\n::: note\nno"),
        "<ul>\n  <li>a</li>\n</ul>\n<aside class=\"admonition note\">\n  <p>no</p>\n</aside>"
    );
    assert_eq!(
        html("- a\n:::\nb"),
        "<ul>\n  <li>a</li>\n</ul>\n<div>\n  <p>b</p>\n</div>"
    );
    assert_eq!(
        html("1. a\n:::"),
        "<ol>\n  <li>a</li>\n</ol>\n<div>\n</div>"
    );
    assert_eq!(
        html("> a\n:::"),
        "<blockquote><p>a</p></blockquote>\n<div>\n</div>"
    );

    assert_eq!(
        html("- ::: note\nbody\n:::"),
        "<ul>\n  <li>::: note\nbody</li>\n</ul>\n<div>\n</div>"
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
fn reference_prepass_does_not_open_residual_indented_fence() {
    // A document-level indented run is not a strict fence; the following
    // definition is still collected.
    assert!(html("  ```\n[r]: /u\n  ```\n\n[x][r]").contains("href=\"/u\""));
}

#[test]
fn reference_prepass_skips_defs_inside_list_nested_fences() {
    let cases = [
        "- ```\n  [r]: /nope\n  ```\n\n[x][r]",
        "- one\n  ```\n  [r]: /u\n  ```\n\n[r][]",
        "1. one\n   ```\n   [r]: /u\n   ```\n\n[r][]",
        "a. one\n   ```\n   [r]: /u\n   ```\n\n[r][]",
        "i. one\n   ```\n   [r]: /u\n   ```\n\n[r][]",
        "-{.c} one\n      ```\n      [r]: /u\n      ```\n\n[r][]",
        "- one\n  - two\n    ```\n    [r]: /u\n    ```\n\n[r][]",
    ];
    for src in cases {
        assert!(
            !html(src).contains("href=\"/u\""),
            "definition inside list-nested fence resolved for:\n{src}"
        );
    }
}

#[test]
fn reference_prepass_fence_closer_never_strips_list_markers() {
    let cases = [
        "```\n- ```\n[r]: /u\n```\n\n[r][]",
        "```\n1. ```\n[r]: /u\n```\n\n[r][]",
    ];
    for src in cases {
        assert!(
            !html(src).contains("href=\"/u\""),
            "definition inside document fence resolved for:\n{src}"
        );
    }
}

#[test]
fn reference_prepass_quoted_fence_closer_is_quote_only() {
    assert!(
        !html("```\n> ```\n[r]: /u\n```\n\n[r][]").contains("href=\"/u\""),
        "literal quoted marker line closed a document fence"
    );
    assert!(
        !html("> ```\n> [r]: /u\n> ```\n\n[r][]").contains("href=\"/u\""),
        "definition inside quoted fence resolved"
    );
    assert!(
        html("- > ```\n  > code\n  > ```\n\n[r]: /u\n\n[r][]").contains("href=\"/u\""),
        "definition after quoted fence in bullet item did not resolve"
    );
    assert!(
        html("- [ ] > ```\n  > code\n  > ```\n\n[r]: /u\n\n[r][]").contains("href=\"/u\""),
        "definition after quoted fence in task item did not resolve"
    );
}

#[test]
fn reference_prepass_keeps_forward_reference_resolution() {
    assert!(html("[x][r]\n\n[r]: /u").contains("href=\"/u\""));
}

#[test]
fn every_list_marker_dialect_collects_definitions() {
    assert!(html("1. [r]: /u\n\n[x][r]").contains("href=\"/u\""));
    assert!(html("- [ ] [r]: /u\n\n[x][r]").contains("href=\"/u\""));
    assert!(html("a. [r]: /u\n\n[x][r]").contains("href=\"/u\""));
    assert!(html("i. [r]: /u\n\n[x][r]").contains("href=\"/u\""));
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
fn definition_blank_between_term_and_definition_keeps_definition() {
    // A blank line between a term and its definition is a separator (djot
    // parity): the `:  d` still attaches to the term. (Previously the blank
    // stranded the definition in a paragraph -- a footgun, now fixed.)
    assert_eq!(
        html(":: t\n\n:  d"),
        "<dl>\n  <dt>t</dt>\n  <dd>d</dd>\n</dl>"
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
    // The trailing space after the marker is DROPPED on the way into the
    // paragraph (PART 2 NO TRAILING WHITESPACE, carve#926) - it is a content
    // line like any other, and the executable spec renders it this way. What
    // this case pins is that the line is a PARAGRAPH rather than a definition
    // entry, which is unchanged.
    assert_eq!(html(":: \n:  d"), "<p>::\n:  d</p>");
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

// A colon-fence-family opener on a quoted line must end the blockquote's lazy
// continuation, so an unquoted line after it is NOT absorbed into the quote.
// The opener is a real block even without a later closer; remaining containers
// close at end of input.
#[test]
fn colon_fence_openers_end_blockquote_lazy_continuation() {
    assert_eq!(
        html("> ::: |\noutside\n> :::"),
        concat!(
            "<blockquote>\n",
            "  <div class=\"line-block\">\n",
            "  </div>\n",
            "</blockquote>\n",
            "<p>outside</p>\n",
            "<blockquote>\n",
            "  <div>\n",
            "  </div>\n",
            "</blockquote>"
        )
    );
    assert_eq!(
        html("> ::: \\\noutside\n> :::"),
        concat!(
            "<blockquote>\n",
            "  <div class=\"hardbreaks\">\n",
            "  </div>\n",
            "</blockquote>\n",
            "<p>outside</p>\n",
            "<blockquote>\n",
            "  <div>\n",
            "  </div>\n",
            "</blockquote>"
        )
    );
    assert_eq!(
        html("> ::: note\noutside\n> :::"),
        concat!(
            "<blockquote>\n",
            "  <aside class=\"admonition note\">\n",
            "\n",
            "  </aside>\n",
            "</blockquote>\n",
            "<p>outside</p>\n",
            "<blockquote>\n",
            "  <div>\n",
            "  </div>\n",
            "</blockquote>"
        )
    );
    assert_eq!(
        html("> ::: |\noutside"),
        "<blockquote>\n  <div class=\"line-block\">\n  </div>\n</blockquote>\n<p>outside</p>"
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
    // Empty `""` at true start: the first opens, and the second follows an
    // opening curly quote (an opening context), so it opens too -> `““`,
    // matching carve-js / carve-php. After a soft break both close.
    assert_eq!(html("\"\""), "<p>““</p>");
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

/// A quote immediately after another OPENING quote is itself in an opening
/// context (the just-emitted `“`/`‘` is a nested-open char), so `"'x'"` nests
/// as `“‘x’”` -- matching carve-js / carve-php. Regression for reading the raw
/// source char (straight `"`) instead of the emitted curly quote.
#[test]
fn smart_quote_nested_open_after_open() {
    assert_eq!(html("\"'x'\""), "<p>“‘x’”</p>");
    assert_eq!(html("a \"'x'\" b"), "<p>a “‘x’” b</p>");
    // A quote after a CLOSING quote stays closing: `x"'y` -> `x”’y`.
    assert_eq!(html("x\"'y"), "<p>x”’y</p>");
    // Same on the non-HTML paths.
    assert_eq!(carve::to_plain_text("\"'x'\"").trim(), "“‘x’”");
    assert_eq!(carve::to_markdown("\"'x'\"").trim(), "“‘x’”");
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

/// A `%%%` comment-block fence is the leading run of 3+ `%`; any trailing text
/// on the opener/closer line is insignificant for matching and never renders.
#[test]
fn comment_fence_trailing_text_is_insignificant() {
    for src in [
        "before\n\n%%%\nsecret\n%%%\n\nafter\n",
        "before\n\n%%% html\nsecret\n%%%\n\nafter\n",
        "before\n\n%%% notes\nsecret\n%%%\n\nafter\n",
        "before\n\n%%%html\nsecret\n%%%\n\nafter\n",
        "before\n\n%%%\nsecret\n%%% end\n\nafter\n",
        "before\n\n%%%% html\nhidden %%% inner\n%%%%\n\nafter\n",
    ] {
        assert_eq!(html(src), "<p>before</p>\n<p>after</p>", "{src:?}");
    }
}

#[test]
fn unterminated_comment_fence_degrades_to_line_comment() {
    for src in [
        "before\n\n%%% TODO\nsecret\n\nafter\n",
        "before\n\n%%%\nsecret\n\nafter\n",
        "before\n\n%%%%\nsecret\n%%%\n\nafter\n",
        "before\n\n%%%\nsecret\n%%%%\n\nafter\n",
    ] {
        assert_eq!(
            html(src),
            "<p>before</p>\n<p>secret</p>\n<p>after</p>",
            "{src:?}"
        );
    }
}

#[test]
fn comment_fence_rules_apply_inside_containers() {
    let quoted = html("> before\n>\n> %%% note\n> secret\n> %%% done\n>\n> after\n");
    assert!(quoted.contains("<blockquote>"), "{quoted}");
    assert!(quoted.contains("<p>before</p>"), "{quoted}");
    assert!(quoted.contains("<p>after</p>"), "{quoted}");
    assert!(!quoted.contains("secret"), "{quoted}");

    let listed = html("- before\n\n  %%% note\n  secret\n  %%% done\n\n  after\n");
    assert!(listed.contains("<ul>"), "{listed}");
    assert!(listed.contains("<p>before</p>"), "{listed}");
    assert!(listed.contains("<p>after</p>"), "{listed}");
    assert!(!listed.contains("secret"), "{listed}");
}

#[test]
fn comment_fence_terminates_heading_and_caption() {
    let heading = html("# Head\n%%% note\nsecret\n%%% done\n");
    assert!(heading.contains("<h1>Head</h1>"), "{heading}");
    assert!(!heading.contains("secret"), "{heading}");
    assert_eq!(
        html("![x](x.png)\n^ cap\n%%% note\nsecret\n%%% done\n"),
        "<figure>\n  <img src=\"x.png\" alt=\"x\">\n  <figcaption>cap</figcaption>\n</figure>"
    );
}

/// Bold-italic `/*…*/` requires content that starts AND ends with a non-space
/// char (grammar `boldItalic = "/*" ~spaceOrEnd biInner+ "*/"`; carve-php also
/// rejects a whitespace-final closer). Empty (`/**/`) or space-bounded
/// (`/* */`, `/*x */`, `/* x*/`) content is NOT bold-italic and falls through
/// to ordinary `/` emphasis. Regression against carve-rs accepting empty /
/// space-initial spans that carve-php and the spec oracle reject.
#[test]
fn bold_italic_rejects_empty_and_space_bounded_content() {
    // Empty / space-bounded -> plain `/emphasis/` over the literal `*`s.
    assert_eq!(html("/**/"), "<p><em>**</em></p>");
    assert_eq!(html("/* */"), "<p><em>* *</em></p>");
    assert_eq!(html("/*x */"), "<p><em>*x *</em></p>");
    assert_eq!(html("/* x*/"), "<p><em>* x*</em></p>");
    // Genuine bold-italic still produces Strong>Emphasis.
    assert_eq!(html("/*x*/"), "<p><strong><em>x</em></strong></p>");
    assert_eq!(html("x/*y*/z"), "<p>x<strong><em>y</em></strong>z</p>");
}
