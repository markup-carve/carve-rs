//! Raw blocks use djot's `=FORMAT` syntax (```=html), matching carve-js /
//! carve-php. The former `raw FORMAT` keyword form was removed. The code-fence
//! language token accepts `/` so MIME-like tags stay a single token.

#[test]
fn raw_block_passes_through_matching_format() {
    assert_eq!(
        carve::to_html("```=html\n<custom-el>V</custom-el>\n```"),
        "<custom-el>V</custom-el>"
    );
}

#[test]
fn all_blank_raw_payload_lines_are_preserved() {
    let one = carve::parse("```=html\n\n```\n");
    let two = carve::parse("```=html\n\n\n```\n");
    let carve::BlockNode::RawBlock(one) = &one.children[0] else {
        panic!("expected raw block");
    };
    let carve::BlockNode::RawBlock(two) = &two.children[0] else {
        panic!("expected raw block");
    };
    assert_eq!(one.content, "\n");
    assert_eq!(two.content, "\n\n");
}

#[test]
fn raw_block_drops_non_matching_format() {
    assert_eq!(carve::to_html("```=latex\n\\emph{x}\n```"), "");
}

#[test]
fn raw_block_accepts_leading_whitespace_before_eq() {
    assert_eq!(carve::to_html("``` =html\n<b>x</b>\n```"), "<b>x</b>");
}

#[test]
fn eq_with_space_before_format_is_not_raw() {
    // ```= html is not a raw block; the line opens an inline code span.
    assert_eq!(
        carve::to_html("```= html\n<b>x</b>\n```"),
        "<p><code>= html\n&lt;b&gt;x&lt;/b&gt;\n</code></p>"
    );
}

#[test]
fn removed_raw_keyword_form_is_not_raw() {
    assert_eq!(
        carve::to_html("```raw html\n<b>x</b>\n```"),
        "<p><code>raw html\n&lt;b&gt;x&lt;/b&gt;\n</code></p>"
    );
}

#[test]
fn language_token_accepts_slash() {
    assert_eq!(
        carve::to_html("```text/html\nx\n```"),
        "<pre><code class=\"language-text/html\">x\n</code></pre>"
    );
}

#[test]
fn language_token_accepts_leading_slash() {
    assert_eq!(
        carve::to_html("```/html\nx\n```"),
        "<pre><code class=\"language-/html\">x\n</code></pre>"
    );
}

/// CARVE-P2-022 ties the payload's newline to the format matching the output
/// format, so a raw block the HTML target drops contributes no line at all --
/// not even the separator the two root loops push before an ordinary block.
#[test]
fn a_dropped_raw_block_owes_no_line_at_the_document_root() {
    assert_eq!(
        carve::to_html("a\n\n```=latex\n```\n\nb\n"),
        "<p>a</p>\n<p>b</p>"
    );
}

/// One blank payload line is a second source shape, and a dropped target
/// answers it the same way: the payload never reaches the output.
#[test]
fn a_dropped_raw_block_with_one_blank_payload_line_owes_no_line() {
    assert_eq!(
        carve::to_html("a\n\n```=latex\n\n```\n\nb\n"),
        "<p>a</p>\n<p>b</p>"
    );
}

/// A heading `<section>` is a rendering artifact: its children are still the
/// document's own blocks (CARVE-P9-073), and the section loop owes the dropped
/// block no line either.
#[test]
fn a_dropped_raw_block_owes_no_line_inside_a_heading_section() {
    assert_eq!(
        carve::to_html("a\n\n## h\n\nc\n\n```=latex\n```\n\nb\n"),
        "<p>a</p>\n<section id=\"h\">\n  <h2>h</h2>\n  <p>c</p>\n  <p>b</p>\n</section>"
    );
}

/// The suppression is keyed on the format the target drops, not on a raw block
/// rendering to something short. An `=html` payload still owes its line, so a
/// change that skipped every raw block would be caught here.
#[test]
fn a_matching_raw_block_still_owes_its_line() {
    assert_eq!(
        carve::to_html("a\n\n```=html\n<hr id=\"x\">\n```\n\nb\n"),
        "<p>a</p>\n<hr id=\"x\">\n<p>b</p>"
    );
}

/// Owing no line does not mean going unreported: CARVE-P2-024 requires the
/// drop stay observable, so the block still reaches the loss record.
#[test]
fn a_dropped_raw_block_that_owes_no_line_is_still_reported() {
    let result = carve::to_html_with_report(
        "a\n\n```=latex\n```\n\nb\n",
        carve::CheckedRenderOptions::default(),
    )
    .expect("the render stays inside the ceiling");
    assert_eq!(result.value, "<p>a</p>\n<p>b</p>");
    assert_eq!(result.total_losses, 1);
    assert_eq!(result.losses[0].code, "raw-format-dropped");
    assert_eq!(result.losses[0].format.as_deref(), Some("latex"));
}

/// CARVE-P2-022 carries a MUST NOT: "Zero payload lines contribute nothing; one
/// blank payload line contributes one newline. An implementation MUST NOT encode
/// those two source shapes identically." Corpus category 521 pins the pair, with
/// a paragraph above the block so the difference is interior and a trimming
/// comparison cannot hide it.
#[test]
fn a_zero_line_and_a_one_blank_payload_do_not_encode_alike() {
    let zero = carve::to_html("a\n\n```=html\n```\n\nb\n");
    let one = carve::to_html("a\n\n```=html\n\n```\n\nb\n");
    assert_ne!(zero, one);
    assert_eq!(zero, "<p>a</p>\n\n<p>b</p>");
    assert_eq!(one, "<p>a</p>\n\n\n<p>b</p>");
}

/// Each further blank payload line contributes one more newline, so the count is
/// the payload's own and not a property of the block.
#[test]
fn each_blank_payload_line_contributes_its_own_newline() {
    assert_eq!(
        carve::to_html("a\n\n```=html\n\n\n```\n\nb\n"),
        "<p>a</p>\n\n\n\n<p>b</p>"
    );
}

/// A whitespace-only payload line is verbatim CONTENT, not a blank, so it reaches
/// the output as the bytes the author wrote.
#[test]
fn a_whitespace_only_payload_line_is_content() {
    assert_eq!(
        carve::to_html("a\n\n```=html\n \n```\n\nb\n"),
        "<p>a</p>\n \n<p>b</p>"
    );
}

/// Corpus 521-1 and 521-2: a list item publishes the payload slot for both
/// spellings. It is the last host that did not, and the reason was a `trim` on
/// the part filter, which could not tell a payload of whitespace from a comment
/// that renders to the empty string.
#[test]
fn a_list_item_publishes_the_payload_slot() {
    assert_eq!(
        carve::to_html("- a\n\n  ```=html\n  ```\n\n- s\n"),
        "<ul>\n  <li><p>a</p>\n    \n  </li>\n  <li><p>s</p></li>\n</ul>"
    );
    assert_eq!(
        carve::to_html("- a\n\n  ```=html\n\n  ```\n\n- s\n"),
        "<ul>\n  <li><p>a</p>\n    \n\n  </li>\n  <li><p>s</p></li>\n</ul>"
    );
}

/// The part filter still drops what renders to NOTHING, which is what keeps a
/// comment and an abbreviation definition from leaving the child indentation
/// behind (carve-rs#532). Both of those render the empty string, so the filter
/// separates them from a whitespace payload without trimming.
#[test]
fn an_item_still_drops_a_child_that_renders_nothing() {
    assert_eq!(
        carve::to_html("- a\n\n  %% c\n"),
        "<ul>\n  <li>a</li>\n</ul>"
    );
    assert_eq!(
        carve::to_html("- a\n\n  ```=latex\n  x\n  ```\n"),
        "<ul>\n  <li>a</li>\n</ul>"
    );
}

/// The container hosts already answered both spellings and must keep doing so:
/// the quote and the admonition, plus an unterminated fence, which runs to the
/// end of what encloses it and still ends its payload on the line before.
#[test]
fn the_container_hosts_keep_both_spellings_apart() {
    for (source, expected) in [
        (
            "> a\n>\n> ```=html\n> ```\n\nb\n",
            "<blockquote>\n  <p>a</p>\n  \n</blockquote>\n<p>b</p>",
        ),
        (
            "> a\n>\n> ```=html\n>\n> ```\n\nb\n",
            "<blockquote>\n  <p>a</p>\n  \n\n</blockquote>\n<p>b</p>",
        ),
        (
            "::: note\na\n\n```=html\n```\n:::\n",
            "<aside class=\"admonition note\" aria-label=\"Note\">\n  <p>a</p>\n  \n</aside>",
        ),
        (
            "::: note\na\n\n```=html\n\n```\n:::\n",
            "<aside class=\"admonition note\" aria-label=\"Note\">\n  <p>a</p>\n  \n\n</aside>",
        ),
        (
            "> a\n>\n> ```=html\n\nb\n",
            "<blockquote>\n  <p>a</p>\n  \n</blockquote>\n<p>b</p>",
        ),
        (
            "> a\n>\n> ```=html\n>\n\nb\n",
            "<blockquote>\n  <p>a</p>\n  \n\n</blockquote>\n<p>b</p>",
        ),
    ] {
        assert_eq!(carve::to_html(source), expected, "source: {source:?}");
    }
}
