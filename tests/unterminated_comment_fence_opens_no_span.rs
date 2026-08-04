//! An unterminated comment fence opens no span, and its neighbours stay put.
//!
//! §28: a fence with no closer degrades to an ordinary `%%` line comment. The
//! item collector opened a span for one anyway, and the span's dedent then
//! lifted a BELOW-column line to the body's column 0, where it parsed as a
//! block (carve-rs#586). The quoted-definition cases below came with the same
//! sweep and are pinned here beside it - they landed in #589.

use carve::to_html;

// --- An unterminated comment fence opens no span (carve-rs#586) ---

#[test]
fn an_unterminated_fence_does_not_lift_a_below_column_line() {
    // §28: a fence with no closer degrades to a `%%` line comment, so the lines
    // after it are just lines. The item collector opened the span anyway and
    // dedented the next line by the span's strip, which lifted a BELOW-column
    // line to the body's column 0 and parsed it as a block.
    assert_eq!(
        to_html("- a\n  %%% x\n # h"),
        "<ul>\n  <li>a\n    # h\n  </li>\n</ul>"
    );
}

#[test]
fn the_same_holds_one_level_deeper() {
    assert_eq!(
        to_html("- - a\n    %%% x\n   # h"),
        "<ul>\n  <li>\n    <ul>\n      <li>a\n        # h\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn a_terminated_fence_still_travels_as_one_span() {
    // The control: with a closer it IS a fence, its body is hidden, and the
    // span keeps its own columns.
    assert_eq!(
        to_html("- - a\n %%% c\n x\n %%%\n b"),
        "<ul>\n  <li>\n    <ul>\n      <li>a\n        b\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
}

// --- A definition inside a quote inside an item (carve-rs#588) ---

#[test]
fn a_quoted_definition_in_a_list_item_registers() {
    // `strip_blockquote_prefix` reads a flush-left `>` only, so a quote written
    // INSIDE an item arrived after the content column and the definition never
    // registered - while the same line one level up is collected and empties
    // the quote.
    assert_eq!(
        to_html("- a\n  > [r]: /u\n\nsee [t][r]"),
        "<ul>\n  <li>a\n    <blockquote>\n\n    </blockquote>\n  </li>\n</ul>\n<p>see <a href=\"/u\">t</a></p>"
    );
}

#[test]
fn the_footnote_form_registers_too() {
    let html = to_html("- a\n  > [^f]: x\n\nsee[^f]");

    assert!(html.contains("doc-noteref"), "{html}");
    assert!(
        html.contains("<blockquote>"),
        "the emptied quote stays in the item: {html}"
    );
}

#[test]
fn the_top_level_shape_is_unchanged() {
    assert_eq!(
        to_html("> [r]: /u\n\nsee [t][r]"),
        "<blockquote>\n\n</blockquote>\n<p>see <a href=\"/u\">t</a></p>"
    );
}
